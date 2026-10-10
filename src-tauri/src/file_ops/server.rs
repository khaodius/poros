//! Moving and copying within one server, without the data passing through this computer when
//! the server can avoid it. Moves are renames, falling back to `mv` across disks. Files are
//! copied with the `copy-data` extension; folders with `cp` when the server runs commands, since
//! one command beats a round trip per file. When neither works, data is read and written back
//! over SFTP in memory.

use std::future::Future;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{self, StreamExt, TryStreamExt};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::names;
use super::{
    summarize, Channel, Conflict, Method, Mode, MoveCopyRequest, OperationSummary, Progress,
    ServerFeatures,
};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::EntryKind;
use crate::remote_path;
use crate::rsync::{run_command, shell_quote};
use crate::session::Session;
use crate::sftp::{EntryStat, RemoteFs};

/// Top-level items worked on at once.
const PARALLEL_ITEMS: usize = 4;
/// Files copied at once inside a folder.
const PARALLEL_FILES: usize = 8;
const PARALLEL_LOOKUPS: usize = 16;
/// Each `copy-data` request copies at most this much, so a slow disk never runs into the
/// request timeout and progress and cancelling stay responsive.
const COPY_DATA_CHUNK: u64 = 16 * 1024 * 1024;
const STREAM_CHUNK: u32 = 256 * 1024;
const STREAM_REQUESTS: usize = 16;
const COMMAND_PROBE: &str = "echo poros-ready";
const COMMAND_PROBE_REPLY: &str = "poros-ready";
const COMMAND_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_FREE_NAME_ATTEMPTS: usize = 10_000;

/// The names among `names` already taken in `directory`.
pub async fn existing_names(
    fs: &RemoteFs,
    directory: &str,
    names: &[String],
) -> AppResult<Vec<String>> {
    let directory = fs.resolve(directory);
    let found: Vec<AppResult<Option<String>>> = stream::iter(names.iter().cloned())
        .map(|name| {
            let path = remote_path::join(&directory, &name);
            async move { Ok(fs.lstat_entry(&path).await?.map(|_| name)) }
        })
        .buffered(PARALLEL_LOOKUPS)
        .collect()
        .await;
    Ok(found
        .into_iter()
        .collect::<AppResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect())
}

/// Where an item goes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Destination {
    /// Nothing is there yet.
    Fresh(String),
    /// A file or link that is replaced.
    Replace(String),
    /// A folder the item's contents are merged into.
    Merge(String),
}

impl Destination {
    fn path(&self) -> &str {
        match self {
            Self::Fresh(path) | Self::Replace(path) | Self::Merge(path) => path,
        }
    }
}

pub struct Work<'a> {
    session: &'a Session,
    features: ServerFeatures,
    fs: Channel<'a>,
    cancel: &'a CancellationToken,
    progress: &'a Progress,
    /// Whether the server runs shell commands, once asked.
    runs_commands: OnceCell<bool>,
}

impl<'a> Work<'a> {
    pub async fn new(
        session: &'a Session,
        features: ServerFeatures,
        cancel: &'a CancellationToken,
        progress: &'a Progress,
    ) -> AppResult<Work<'a>> {
        Ok(Self {
            session,
            features,
            fs: Channel::open(session).await?,
            cancel,
            progress,
            runs_commands: OnceCell::new(),
        })
    }

    pub async fn move_or_copy(&self, request: &MoveCopyRequest) -> AppResult<OperationSummary> {
        let directory = trim_trailing_slash(&self.fs.resolve(&request.target_directory));
        match self.fs.stat(&directory).await? {
            Some(stat) if stat.is_dir => {}
            Some(_) => return Err(AppError::invalid(format!("{directory} is not a folder"))),
            None => {
                return Err(AppError::new(
                    ErrorKind::NotFound,
                    format!("{directory} does not exist"),
                )
                .with_path(directory))
            }
        }
        let results = stream::iter(request.sources.iter().cloned())
            .map(|source| {
                let source = trim_trailing_slash(&self.fs.resolve(&source));
                let directory = &directory;
                async move {
                    let result = self
                        .place(&source, directory, request.mode, request.conflict)
                        .await;
                    (source, result)
                }
            })
            .buffered(PARALLEL_ITEMS)
            .collect()
            .await;
        summarize(self.progress, results, self.cancel)
    }

    async fn cancellable<T>(&self, future: impl Future<Output = AppResult<T>>) -> AppResult<T> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(AppError::cancelled()),
            result = future => result,
        }
    }

    fn check_cancelled(&self) -> AppResult<()> {
        if self.cancel.is_cancelled() {
            Err(AppError::cancelled())
        } else {
            Ok(())
        }
    }

    /// Moves or copies one item into `directory`; `None` when it was skipped.
    async fn place(
        &self,
        source: &str,
        directory: &str,
        mode: Mode,
        conflict: Conflict,
    ) -> AppResult<Option<String>> {
        self.check_cancelled()?;
        let name = remote_path::file_name(source).to_string();
        let stat = self.fs.lstat_entry(source).await?.ok_or_else(|| {
            AppError::new(ErrorKind::NotFound, format!("{name} no longer exists")).with_path(source)
        })?;
        if stat.kind == EntryKind::Dir && is_within(directory, source) {
            let verb = if mode == Mode::Move { "move" } else { "copy" };
            return Err(AppError::invalid(format!(
                "Cannot {verb} the folder {name} into itself"
            )));
        }
        let in_place = remote_path::parent(source).as_deref() == Some(directory);
        let destination = match (mode, in_place) {
            (Mode::Move, true) => return Ok(None),
            (Mode::Copy, true) => Some(Destination::Fresh(
                self.free_name(directory, &name, stat.kind).await?,
            )),
            _ => {
                self.destination(directory, &name, stat.kind, conflict)
                    .await?
            }
        };
        let Some(destination) = destination else {
            return Ok(None);
        };
        self.progress.working_on(source);
        match mode {
            Mode::Move => self.move_to(source, stat, &destination).await?,
            Mode::Copy => self.copy_to(source, stat, &destination, true).await?,
        }
        Ok(Some(destination.path().to_string()))
    }

    /// Where an item named `name` goes in `directory`; `None` to skip it.
    async fn destination(
        &self,
        directory: &str,
        name: &str,
        kind: EntryKind,
        conflict: Conflict,
    ) -> AppResult<Option<Destination>> {
        let path = remote_path::join(directory, name);
        let Some(existing) = self.fs.lstat_entry(&path).await? else {
            return Ok(Some(Destination::Fresh(path)));
        };
        match conflict {
            Conflict::Skip => Ok(None),
            Conflict::KeepBoth => Ok(Some(Destination::Fresh(
                self.free_name(directory, name, kind).await?,
            ))),
            Conflict::Replace => match (kind == EntryKind::Dir, existing.kind == EntryKind::Dir) {
                (true, true) => Ok(Some(Destination::Merge(path))),
                (false, false) => Ok(Some(Destination::Replace(path))),
                (true, false) => Err(already_exists(&path, name, "a file", "folder")),
                (false, true) => Err(already_exists(&path, name, "a folder", "file")),
            },
        }
    }

    async fn free_name(&self, directory: &str, name: &str, kind: EntryKind) -> AppResult<String> {
        for candidate in names::numbered(name, kind == EntryKind::Dir).take(MAX_FREE_NAME_ATTEMPTS)
        {
            let path = remote_path::join(directory, &candidate);
            if self.fs.lstat_entry(&path).await?.is_none() {
                return Ok(path);
            }
        }
        Err(AppError::new(
            ErrorKind::AlreadyExists,
            format!("No free name for a copy of {name}"),
        ))
    }

    fn move_to<'b>(
        &'b self,
        source: &'b str,
        stat: EntryStat,
        destination: &'b Destination,
    ) -> BoxFuture<'b, AppResult<()>> {
        Box::pin(async move {
            self.check_cancelled()?;
            let renamed = match destination {
                Destination::Fresh(target) => self.fs.rename_path(source, target).await,
                Destination::Replace(target) => self.fs.replace(source, target).await,
                Destination::Merge(target) => return self.merge_into(source, target).await,
            };
            match renamed {
                Ok(()) => {
                    self.progress.used(Method::Rename);
                    self.progress.file_done();
                    Ok(())
                }
                // A generic failure is what a rename to another disk gives.
                Err(error) if error.kind == ErrorKind::Sftp => {
                    self.move_across_disks(source, stat, destination, error)
                        .await
                }
                Err(error) => Err(error),
            }
        })
    }

    /// Moves a folder's contents into an existing folder, replacing files, then removes it.
    async fn merge_into(&self, source: &str, target: &str) -> AppResult<()> {
        let children = self.fs.entries(source).await?;
        let mut all_moved = true;
        for (name, stat) in children {
            let child = remote_path::join(source, &name);
            let moved = async {
                let destination = self
                    .destination(target, &name, stat.kind, Conflict::Replace)
                    .await?
                    .expect("replacing never skips");
                self.move_to(&child, stat, &destination).await
            }
            .await;
            if let Err(error) = moved {
                self.check_cancelled()?;
                self.progress.fail(&child, error);
                all_moved = false;
            }
        }
        if all_moved {
            if let Err(error) = self.fs.remove_empty_dir(source).await {
                // Something deeper inside failed to move, and was reported there.
                let still_holds_entries = self
                    .fs
                    .entries(source)
                    .await
                    .is_ok_and(|left| !left.is_empty());
                if !still_holds_entries {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    async fn move_across_disks(
        &self,
        source: &str,
        stat: EntryStat,
        destination: &Destination,
        rename_error: AppError,
    ) -> AppResult<()> {
        if self.runs_commands().await {
            let target = destination.path();
            self.run(&format!(
                "mv -f -- {} {}",
                shell_quote(source),
                shell_quote(target)
            ))
            .await?;
            if self.fs.lstat_entry(target).await?.is_none() {
                return Err(rename_error);
            }
            self.progress.used(Method::Command);
            self.progress.file_done();
            return Ok(());
        }
        match (stat.kind, destination) {
            // Moved one entry at a time, so whatever fails to copy stays where it was.
            (EntryKind::Dir, Destination::Fresh(target)) => {
                // Writable by its owner while it fills; the real permissions are set last.
                let permissions = stat.permissions.map(|mode| mode | 0o700);
                self.fs.make_dir_at(target, permissions).await?;
                self.merge_into(source, target).await?;
                let _ = self
                    .fs
                    .set_attributes(target, stat.modified, stat.permissions)
                    .await;
                Ok(())
            }
            _ => {
                self.copy_to(source, stat, destination, false).await?;
                self.fs.delete(&[source.to_string()]).await
            }
        }
    }

    fn copy_to<'b>(
        &'b self,
        source: &'b str,
        stat: EntryStat,
        destination: &'b Destination,
        commands_allowed: bool,
    ) -> BoxFuture<'b, AppResult<()>> {
        Box::pin(async move {
            self.check_cancelled()?;
            let commands = commands_allowed && self.runs_commands().await;
            match stat.kind {
                EntryKind::Dir if commands => self.copy_with_command(source, destination).await,
                EntryKind::Dir => match destination {
                    Destination::Fresh(target) => self.copy_tree(source, stat, target, false).await,
                    Destination::Merge(target) => self.copy_tree(source, stat, target, true).await,
                    Destination::Replace(target) => Err(already_exists(
                        target,
                        remote_path::file_name(target),
                        "a file",
                        "folder",
                    )),
                },
                EntryKind::File => self.copy_file(source, stat, destination, commands).await,
                EntryKind::Symlink => self.copy_link(source, destination).await,
                EntryKind::Other => Err(AppError::invalid(format!(
                    "{} is not a file, folder or link",
                    remote_path::file_name(source)
                ))
                .with_path(source)),
            }
        })
    }

    /// Copies a folder entry by entry over SFTP, for servers that do not run commands.
    fn copy_tree<'b>(
        &'b self,
        source: &'b str,
        stat: EntryStat,
        target: &'b str,
        merge: bool,
    ) -> BoxFuture<'b, AppResult<()>> {
        Box::pin(async move {
            self.check_cancelled()?;
            if !merge {
                // Writable by its owner while it fills; the real permissions are set last.
                let permissions = stat.permissions.map(|mode| mode | 0o700);
                self.fs.make_dir_at(target, permissions).await?;
            }
            let children = self.fs.entries(source).await?;
            let (folders, others): (Vec<_>, Vec<_>) = children
                .into_iter()
                .partition(|(_, child)| child.kind == EntryKind::Dir);
            stream::iter(others)
                .for_each_concurrent(PARALLEL_FILES, |(name, child)| async move {
                    let child_source = remote_path::join(source, &name);
                    let copied = async {
                        let destination =
                            self.child_destination(target, &name, child, merge).await?;
                        self.copy_to(&child_source, child, &destination, false)
                            .await
                    }
                    .await;
                    if let Err(error) = copied {
                        self.progress.fail(&child_source, error);
                    }
                })
                .await;
            for (name, child) in folders {
                self.check_cancelled()?;
                let child_source = remote_path::join(source, &name);
                let copied = async {
                    let destination = self.child_destination(target, &name, child, merge).await?;
                    self.copy_to(&child_source, child, &destination, false)
                        .await
                }
                .await;
                if let Err(error) = copied {
                    self.progress.fail(&child_source, error);
                }
            }
            self.check_cancelled()?;
            if !merge {
                // Adding entries changes a folder's time, so it is set after them.
                let _ = self
                    .fs
                    .set_attributes(target, stat.modified, stat.permissions)
                    .await;
            }
            Ok(())
        })
    }

    async fn child_destination(
        &self,
        target: &str,
        name: &str,
        stat: EntryStat,
        merge: bool,
    ) -> AppResult<Destination> {
        if !merge {
            return Ok(Destination::Fresh(remote_path::join(target, name)));
        }
        Ok(self
            .destination(target, name, stat.kind, Conflict::Replace)
            .await?
            .expect("replacing never skips"))
    }

    /// A replaced file is written beside it and renamed over it, so it is never left half
    /// written.
    async fn copy_file(
        &self,
        source: &str,
        stat: EntryStat,
        destination: &Destination,
        commands: bool,
    ) -> AppResult<()> {
        let target = destination.path();
        let writing = match destination {
            Destination::Replace(_) => remote_path::join(
                &remote_path::parent(target).unwrap_or_default(),
                &names::temporary(remote_path::file_name(target)),
            ),
            _ => target.to_string(),
        };
        let copied = self
            .copy_file_contents(source, stat, &writing, commands)
            .await;
        let placed = match copied {
            Ok(()) if writing != target => self.fs.replace(&writing, target).await,
            other => other,
        };
        if placed.is_err() {
            let _ = self.fs.remove_file(&writing).await;
        }
        placed?;
        self.progress.file_done();
        Ok(())
    }

    async fn copy_file_contents(
        &self,
        source: &str,
        stat: EntryStat,
        target: &str,
        commands: bool,
    ) -> AppResult<()> {
        if self.features.copy_data && self.fs.supports_copy_data() {
            match self.copy_file_with_copy_data(source, stat, target).await {
                Ok(()) => {
                    self.progress.used(Method::CopyData);
                    return Ok(());
                }
                // Refused for this file; another way may still work.
                Err(error) if error.kind == ErrorKind::Sftp => {}
                Err(error) => return Err(error),
            }
        }
        if commands {
            self.run(&format!(
                "cp -Pp -- {} {}",
                shell_quote(source),
                shell_quote(target)
            ))
            .await?;
            self.progress.used(Method::Command);
            self.progress.add_bytes(stat.size);
            return Ok(());
        }
        self.stream_file(source, stat, target).await?;
        self.progress.used(Method::Stream);
        Ok(())
    }

    async fn copy_file_with_copy_data(
        &self,
        source: &str,
        stat: EntryStat,
        target: &str,
    ) -> AppResult<()> {
        self.with_open_files(source, stat, target, |from, to| async move {
            let mut offset = 0;
            while offset < stat.size {
                let length = (stat.size - offset).min(COPY_DATA_CHUNK);
                self.cancellable(self.fs.copy_data(&from, &to, offset, length))
                    .await
                    .map_err(|error| error.with_path(source))?;
                offset += length;
                self.progress.add_bytes(length);
            }
            Ok(())
        })
        .await
    }

    /// Reads the file and writes it back, many requests at a time, without touching the disk
    /// of this computer.
    async fn stream_file(&self, source: &str, stat: EntryStat, target: &str) -> AppResult<()> {
        let chunk = self
            .fs
            .read_size(STREAM_CHUNK)
            .min(self.fs.write_size(STREAM_CHUNK))
            .max(1);
        self.with_open_files(source, stat, target, |from, to| async move {
            let offsets = (0..stat.size).step_by(chunk as usize);
            let copy = stream::iter(offsets)
                .map(Ok::<u64, AppError>)
                .try_for_each_concurrent(STREAM_REQUESTS, |offset| {
                    let (from, to) = (&from, &to);
                    async move {
                        let length = (stat.size - offset).min(u64::from(chunk)) as u32;
                        let data = self.fs.read_handle_range(from, offset, length).await?;
                        let read = data.len() as u64;
                        if read > 0 {
                            self.fs.write_chunk(to, offset, data).await?;
                        }
                        self.progress.add_bytes(read);
                        Ok(())
                    }
                });
            self.cancellable(copy)
                .await
                .map_err(|error| error.with_path(source))
        })
        .await
    }

    /// Opens both files around `copy`, then gives the copy the source's time and permissions.
    async fn with_open_files<F, Fut>(
        &self,
        source: &str,
        stat: EntryStat,
        target: &str,
        copy: F,
    ) -> AppResult<()>
    where
        F: FnOnce(String, String) -> Fut,
        Fut: Future<Output = AppResult<()>>,
    {
        let from = self.fs.open_for_read(source).await?;
        let to = match self.fs.open_for_write(target, true, stat.permissions).await {
            Ok(handle) => handle,
            Err(error) => {
                let _ = self.fs.close_handle(from).await;
                return Err(error);
            }
        };
        let copied = copy(from.clone(), to.clone()).await;
        let _ = self.fs.close_handle(from).await;
        // Closing a written file can report a failed write.
        let closed = self.fs.close_handle(to).await;
        copied?;
        closed.map_err(|error| error.with_path(target))?;
        let _ = self
            .fs
            .set_attributes(target, stat.modified, stat.permissions)
            .await;
        Ok(())
    }

    async fn copy_link(&self, source: &str, destination: &Destination) -> AppResult<()> {
        let link_target = self.fs.read_link(source).await?;
        if let Destination::Replace(existing) = destination {
            self.fs.remove_file(existing).await?;
        }
        self.fs
            .make_symlink(destination.path(), &link_target)
            .await?;
        self.progress.file_done();
        Ok(())
    }

    /// Copies a folder or file with one `cp` on the server. `cp -PpR` is POSIX: links stay
    /// links, and times and permissions are kept.
    async fn copy_with_command(&self, source: &str, destination: &Destination) -> AppResult<()> {
        let command = match destination {
            Destination::Merge(target) => format!(
                "cp -PpR -- {} {}",
                shell_quote(&format!("{source}/.")),
                shell_quote(target)
            ),
            Destination::Fresh(target) | Destination::Replace(target) => {
                format!("cp -PpR -- {} {}", shell_quote(source), shell_quote(target))
            }
        };
        self.run(&command).await?;
        // A server that forces one program for every command may do nothing and still succeed.
        if self.fs.lstat_entry(destination.path()).await?.is_none() {
            return Err(AppError::new(
                ErrorKind::Io,
                format!("The server did not copy {}", remote_path::file_name(source)),
            )
            .with_path(source));
        }
        self.progress.used(Method::Command);
        self.progress.file_done();
        Ok(())
    }

    async fn runs_commands(&self) -> bool {
        *self
            .runs_commands
            .get_or_init(|| async { self.features.commands && runs_commands(self.session).await })
            .await
    }

    async fn run(&self, command: &str) -> AppResult<()> {
        let channel = self.session.open_command_channel().await?;
        let output = self
            .cancellable(run_command(channel, command, COMMAND_OUTPUT_LIMIT))
            .await?;
        if output.exit_status == Some(0) {
            return Ok(());
        }
        Err(command_error(
            command,
            output.exit_status,
            &output.error_output,
        ))
    }
}

/// Whether the server runs shell commands. Servers limited to SFTP may run their SFTP server
/// for every command and report success, so the probe checks what comes back.
pub async fn runs_commands(session: &Session) -> bool {
    let Ok(channel) = session.open_command_channel().await else {
        return false;
    };
    let probe = tokio::time::timeout(
        COMMAND_PROBE_TIMEOUT,
        run_command(channel, COMMAND_PROBE, COMMAND_OUTPUT_LIMIT),
    )
    .await;
    matches!(
        probe,
        Ok(Ok(output)) if output.exit_status == Some(0)
            && String::from_utf8_lossy(&output.output).trim() == COMMAND_PROBE_REPLY
    )
}

fn command_error(command: &str, exit_status: Option<u32>, error_output: &str) -> AppError {
    let program = command.split_whitespace().next().unwrap_or(command);
    let reported = error_output.trim().lines().last().map(str::trim);
    let kind = if error_output.contains("Permission denied") {
        ErrorKind::PermissionDenied
    } else {
        ErrorKind::Io
    };
    let message = match (reported, exit_status) {
        (Some(reported), _) if !reported.is_empty() => reported.to_string(),
        (_, Some(status)) => format!("{program} on the server exited with status {status}"),
        (_, None) => format!("{program} on the server did not finish"),
    };
    AppError::new(kind, message)
}

fn already_exists(path: &str, name: &str, existing: &str, incoming: &str) -> AppError {
    AppError::new(
        ErrorKind::AlreadyExists,
        format!("{name} is already {existing} there, so the {incoming} cannot replace it"),
    )
    .with_path(path)
}

/// Whether `path` is `folder` or inside it.
fn is_within(path: &str, folder: &str) -> bool {
    if folder == "/" {
        return true;
    }
    path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn trim_trailing_slash(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_contain_their_descendants_only() {
        assert!(is_within("/srv/www", "/srv/www"));
        assert!(is_within("/srv/www/site", "/srv/www"));
        assert!(!is_within("/srv/www-old", "/srv/www"));
        assert!(!is_within("/srv", "/srv/www"));
        assert!(is_within("/anything", "/"));
    }

    #[test]
    fn trims_trailing_slashes_but_keeps_root() {
        assert_eq!(trim_trailing_slash("/srv/www/"), "/srv/www");
        assert_eq!(trim_trailing_slash("/"), "/");
        assert_eq!(trim_trailing_slash("//"), "/");
    }

    #[test]
    fn command_errors_quote_the_server() {
        let error = command_error(
            "cp -PpR -- 'a' 'b'",
            Some(1),
            "cp: cannot create regular file 'b': Permission denied\n",
        );
        assert_eq!(error.kind, ErrorKind::PermissionDenied);
        assert_eq!(
            error.message,
            "cp: cannot create regular file 'b': Permission denied"
        );
        let silent = command_error("mv -f -- 'a' 'b'", Some(1), "");
        assert_eq!(silent.message, "mv on the server exited with status 1");
    }
}
