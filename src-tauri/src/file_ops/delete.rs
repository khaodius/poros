//! Deleting on a server. When the server runs commands, one `rm -r` per folder removes the
//! whole selection there without a round trip per entry, and names each entry as it goes, so
//! progress stays exact. Everything else is removed over several SFTP channels at once, with many
//! requests in flight on each.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use futures::future::BoxFuture;
use futures::stream::{self, StreamExt};
use russh::{ChannelMsg, Sig};
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio_util::sync::CancellationToken;

use super::server::{runs_commands, trim_trailing_slash};
use super::{Channel, DeleteSummary, Method, Progress, ServerFeatures};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::EntryKind;
use crate::remote_path;
use crate::rsync::shell_quote;
use crate::session::Session;
use crate::sftp::RemoteFs;

/// SFTP channels opened to delete over. Most servers run an SFTP server process for each, so
/// they also work in parallel on the server.
const SFTP_CHANNELS: usize = 4;
const REQUESTS_PER_CHANNEL: usize = 64;
/// Entries of one folder started at once; the requests they make are limited separately.
const PARALLEL_ENTRIES: usize = 256;
/// The longest command sent at once. Linux takes at most 128 KiB in one argument, and the
/// shell gets the whole command as one.
const COMMAND_LIMIT: usize = 64 * 1024;
/// What the script around the names takes, besides the folder twice.
const SCRIPT_ROOM: usize = 512;
/// Longer lines of output are cut.
const LINE_LIMIT: usize = 16 * 1024;
/// How the script ends when the shell cannot enter the folder, and when the folder it enters is
/// not the one SFTP shows, as on servers that give SFTP a root of its own.
const FOLDER_UNREACHABLE: u32 = 97;
const OTHER_FOLDER: u32 = 98;
/// Starts the line where the script reports how many entries it is about to remove.
const TOTAL_LINE: &str = "total ";
/// An empty file made over SFTP in the folder, which the shell must see before it deletes.
const MARKER_PREFIX: &str = ".poros-delete-";

/// Deletes `paths` and everything inside folders among them. Links are removed, never followed.
pub async fn delete(
    session: &Session,
    features: ServerFeatures,
    paths: &[String],
    cancel: &CancellationToken,
    progress: &Progress,
) -> AppResult<DeleteSummary> {
    let fs = session.sftp()?;
    let paths = outermost(
        paths
            .iter()
            .map(|path| trim_trailing_slash(&fs.resolve(path))),
    );
    if paths.is_empty() {
        return Ok(DeleteSummary::default());
    }
    if paths.iter().any(|path| path == "/") {
        return Err(AppError::invalid("Refusing to delete /"));
    }
    progress.set_counting(true);
    let mut left = paths;
    if features.commands && runs_commands(session).await {
        let commands = Commands {
            session,
            fs,
            cancel,
            progress,
            uncounted: AtomicUsize::new(0),
        };
        left = commands.delete(left).await?;
    }
    if !left.is_empty() {
        SftpDeletion::open(session, cancel, progress)
            .await?
            .delete(left)
            .await;
    }
    progress.set_counting(false);
    if cancel.is_cancelled() {
        return Err(AppError::cancelled());
    }
    Ok(DeleteSummary {
        deleted: progress.files_done(),
        failed: progress.failed(),
        failures: progress.take_failures(),
        methods: progress.methods(),
    })
}

/// The paths without repeats and without those inside another one, which deleting that folder
/// takes care of.
fn outermost(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let unique: BTreeSet<String> = paths.into_iter().collect();
    unique
        .iter()
        .filter(|path| {
            let mut ancestor = remote_path::parent(path);
            while let Some(folder) = ancestor {
                if unique.contains(&folder) {
                    return false;
                }
                ancestor = remote_path::parent(&folder);
            }
            true
        })
        .cloned()
        .collect()
}

/// Deleting with `rm` run on the server.
struct Commands<'a> {
    session: &'a Session,
    fs: &'a RemoteFs,
    cancel: &'a CancellationToken,
    progress: &'a Progress,
    /// Commands yet to report how many entries they remove.
    uncounted: AtomicUsize,
}

impl Commands<'_> {
    /// Deletes `paths` folder by folder; returns those left to delete over SFTP.
    async fn delete(&self, paths: Vec<String>) -> AppResult<Vec<String>> {
        let mut folders: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in paths {
            if let Some(folder) = remote_path::parent(&path) {
                let name = remote_path::file_name(&path).to_string();
                folders.entry(folder).or_default().push(name);
            }
        }
        let plan: Vec<(String, Vec<Vec<String>>)> = folders
            .into_iter()
            .map(|(folder, names)| {
                let batches = batches(&folder, names);
                (folder, batches)
            })
            .collect();
        let commands = plan.iter().map(|(_, batches)| batches.len()).sum();
        self.uncounted.store(commands, Ordering::Relaxed);
        let mut left = Vec::new();
        for (folder, batches) in plan {
            left.extend(self.delete_in(&folder, batches).await?);
        }
        Ok(left)
    }

    /// Deletes names in one folder; returns the paths left to delete over SFTP.
    async fn delete_in(&self, folder: &str, batches: Vec<Vec<String>>) -> AppResult<Vec<String>> {
        let paths = |names: &[String]| -> Vec<String> {
            names
                .iter()
                .map(|name| remote_path::join(folder, name))
                .collect()
        };
        let marker_name = format!("{MARKER_PREFIX}{}", uuid::Uuid::new_v4().simple());
        // The folder may not take new files; SFTP still deletes what it can.
        let Ok(marker) = self.fs.make_file(folder, &marker_name).await else {
            self.counted(batches.len());
            return Ok(batches.iter().flat_map(|names| paths(names)).collect());
        };
        let deleted = async {
            let mut left = Vec::new();
            let mut usable = true;
            for names in &batches {
                if !usable {
                    self.counted(1);
                    left.extend(paths(names));
                    continue;
                }
                match self.run_batch(folder, &marker_name, names).await? {
                    Batch::Unusable => {
                        usable = false;
                        left.extend(paths(names));
                    }
                    Batch::Done => {}
                    Batch::Unfinished { reported } => {
                        left.extend(self.still_there(folder, names, &reported).await);
                    }
                }
            }
            Ok(left)
        }
        .await;
        let _ = self.fs.remove_file(&marker).await;
        deleted
    }

    fn counted(&self, commands: usize) {
        let before = self.uncounted.fetch_sub(commands, Ordering::Relaxed);
        if before <= commands {
            self.progress.set_counting(false);
        }
    }

    async fn run_batch(&self, folder: &str, marker: &str, names: &[String]) -> AppResult<Batch> {
        let arguments = names
            .iter()
            .map(|name| shell_quote(&format!("./{name}")))
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!(
            "cd -- {folder} || exit {FOLDER_UNREACHABLE}; \
             [ -e {marker} ] || exit {OTHER_FOLDER}; \
             printf '{TOTAL_LINE}%s\\n' \"$(find {arguments} 2>/dev/null | wc -l)\"; \
             LC_ALL=C rm -rfv -- {arguments}",
            folder = shell_quote(folder),
            marker = shell_quote(marker),
        );
        // Whatever the login shell, `sh` runs the script.
        let command = format!("sh -c {}", shell_quote(&script));
        let mut output = BatchOutput {
            commands: self,
            folder,
            names: names.len() as u64,
            counted: None,
            removed: 0,
            latest: Vec::new(),
            reported: HashSet::new(),
        };
        let exit_status = run_streaming(self.session, &command, self.cancel, &mut output).await;
        if output.counted.is_none() {
            self.counted(1);
        }
        let exit_status = exit_status?;
        if output.removed > 0 {
            self.progress.used(Method::Command);
        }
        let unusable = matches!(exit_status, Some(FOLDER_UNREACHABLE | OTHER_FOLDER))
            || (exit_status != Some(0) && output.removed == 0 && output.reported.is_empty());
        Ok(if unusable {
            // SFTP finds these entries again.
            self.progress.unfound(output.counted.unwrap_or(0));
            Batch::Unusable
        } else if exit_status == Some(0) {
            Batch::Done
        } else {
            Batch::Unfinished {
                reported: output.reported,
            }
        })
    }

    /// The paths of `names` still in the folder that `rm` gave no reason for, as when it was
    /// stopped partway.
    async fn still_there(
        &self,
        folder: &str,
        names: &[String],
        reported: &HashSet<String>,
    ) -> Vec<String> {
        let unexplained = names.iter().filter(|name| !reported.contains(*name));
        let Ok(entries) = self.fs.entries(folder).await else {
            return unexplained
                .map(|name| remote_path::join(folder, name))
                .collect();
        };
        let present: HashSet<String> = entries.into_iter().map(|(name, _)| name).collect();
        unexplained
            .filter(|name| present.contains(*name))
            .map(|name| remote_path::join(folder, name))
            .collect()
    }
}

/// How one command went.
enum Batch {
    Done,
    /// The shell could not work in the folder, or `rm` did nothing at all, as when it lacks
    /// `-v`: SFTP takes over.
    Unusable,
    /// `rm` ended with entries left. `reported` holds the names it gave a reason for, inside or
    /// of the entry itself.
    Unfinished {
        reported: HashSet<String>,
    },
}

/// Follows what one command prints.
struct BatchOutput<'a> {
    commands: &'a Commands<'a>,
    folder: &'a str,
    /// The names the command was given.
    names: u64,
    /// The entries the command found to remove, once it has said.
    counted: Option<u64>,
    removed: u64,
    latest: Vec<u8>,
    reported: HashSet<String>,
}

impl Lines for BatchOutput<'_> {
    fn output(&mut self, line: &[u8]) {
        if self.counted.is_none() {
            let text = String::from_utf8_lossy(line);
            let found = text
                .strip_prefix(TOTAL_LINE)
                .and_then(|count| count.trim().parse::<u64>().ok())
                .unwrap_or(0)
                .max(self.names);
            self.counted = Some(found);
            self.commands.progress.found(found);
            self.commands.counted(1);
            if text.starts_with(TOTAL_LINE) {
                return;
            }
        }
        self.removed += 1;
        self.commands.progress.file_done();
        self.latest.clear();
        self.latest.extend_from_slice(line);
    }

    fn error(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        let Some((path, reason)) = removal_error(&text) else {
            return;
        };
        let relative = path.trim_start_matches("./");
        if let Some(name) = relative.split('/').next() {
            self.reported.insert(name.to_string());
        }
        let kind = if reason.contains("Permission denied") || reason.contains("not permitted") {
            ErrorKind::PermissionDenied
        } else {
            ErrorKind::Io
        };
        self.commands.progress.fail(
            &remote_path::join(self.folder, relative),
            AppError::new(kind, reason),
        );
    }

    fn caught_up(&mut self) {
        if self.latest.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.latest);
        let relative = removed_path(&text).trim_start_matches("./");
        self.commands
            .progress
            .working_on(&remote_path::join(self.folder, relative));
        self.latest.clear();
    }
}

/// Splits names into groups small enough for one command each. Each name is in the command
/// twice, with `./` before it, quoted once for the script and again for `sh -c`.
fn batches(folder: &str, names: Vec<String>) -> Vec<Vec<String>> {
    let quoted_length = |text: &str| shell_quote(&shell_quote(text)).len();
    let room = COMMAND_LIMIT.saturating_sub(SCRIPT_ROOM + 2 * quoted_length(folder));
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut length = 0;
    for name in names {
        let cost = 2 * (quoted_length(&format!("./{name}")) + 1);
        if !batch.is_empty() && length + cost > room {
            batches.push(std::mem::take(&mut batch));
            length = 0;
        }
        length += cost;
        batch.push(name);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// The path in a line of `rm -v`: `removed 'x'` and `removed directory 'x'` from GNU,
/// `removed directory: 'x'` from BusyBox, the bare path from BSD.
fn removed_path(line: &str) -> &str {
    match line.strip_prefix("removed ") {
        Some(rest) => {
            let rest = rest
                .strip_prefix("directory")
                .map(|rest| rest.trim_start_matches(':').trim_start())
                .unwrap_or(rest);
            unquote(rest)
        }
        None => line,
    }
}

/// The path and reason in an error from `rm`: `rm: cannot remove 'x': reason` from GNU,
/// `rm: can't remove 'x': reason` from BusyBox, `rm: x: reason` from BSD.
fn removal_error(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("rm: ")?;
    let rest = rest
        .strip_prefix("cannot remove ")
        .or_else(|| rest.strip_prefix("can't remove "))
        .unwrap_or(rest);
    let (path, reason) = rest.rsplit_once(": ")?;
    Some((unquote(path), reason.trim()))
}

/// GNU quotes names, in double quotes when they hold a single quote.
fn unquote(text: &str) -> &str {
    ['\'', '"']
        .into_iter()
        .find_map(|quote| text.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(text)
}

/// Takes what a command prints, line by line, as it comes.
trait Lines {
    fn output(&mut self, line: &[u8]);
    fn error(&mut self, line: &[u8]);
    /// After each piece of output, so what is shown can catch up.
    fn caught_up(&mut self);
}

/// Runs `command` on the server, handing what it prints to `lines`; returns how it exited, if it
/// said. Cancelling signals the command and closes its output, which ends `rm -v` at the next
/// entry it names.
async fn run_streaming(
    session: &Session,
    command: &str,
    cancel: &CancellationToken,
    lines: &mut impl Lines,
) -> AppResult<Option<u32>> {
    let mut channel = session.open_command_channel().await?;
    channel.exec(true, command).await?;
    let _ = channel.eof().await;
    let mut output = LineSplitter::default();
    let mut errors = LineSplitter::default();
    let mut exit_status = None;
    loop {
        let message = tokio::select! {
            biased;
            _ = cancel.cancelled() => None,
            message = channel.wait() => Some(message),
        };
        let Some(message) = message else {
            let _ = channel.signal(Sig::TERM).await;
            let _ = channel.close().await;
            return Err(AppError::cancelled());
        };
        match message {
            Some(ChannelMsg::Data { data }) => {
                output.split(&data, |line| lines.output(line));
                lines.caught_up();
            }
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                errors.split(&data, |line| lines.error(line));
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => exit_status = Some(status),
            // The server refused to run it.
            Some(ChannelMsg::Failure) | Some(ChannelMsg::Close) | None => break,
            Some(_) => {}
        }
    }
    output.finish(|line| lines.output(line));
    errors.finish(|line| lines.error(line));
    lines.caught_up();
    Ok(exit_status)
}

#[derive(Default)]
struct LineSplitter {
    partial: Vec<u8>,
}

impl LineSplitter {
    fn split(&mut self, data: &[u8], mut line: impl FnMut(&[u8])) {
        let mut rest = data;
        while let Some(end) = rest.iter().position(|&byte| byte == b'\n') {
            if self.partial.is_empty() {
                line(&rest[..end]);
            } else {
                self.keep(&rest[..end]);
                line(&self.partial);
                self.partial.clear();
            }
            rest = &rest[end + 1..];
        }
        self.keep(rest);
    }

    fn keep(&mut self, bytes: &[u8]) {
        let room = LINE_LIMIT.saturating_sub(self.partial.len());
        self.partial
            .extend_from_slice(&bytes[..bytes.len().min(room)]);
    }

    fn finish(&mut self, mut line: impl FnMut(&[u8])) {
        if !self.partial.is_empty() {
            line(&self.partial);
            self.partial.clear();
        }
    }
}

/// Deleting over SFTP, spread over several channels.
struct SftpDeletion<'a> {
    channels: Vec<Channel<'a>>,
    next_channel: AtomicUsize,
    requests: Semaphore,
    /// Folders found and not yet listed. Until none are left, more entries may turn up.
    unlisted: AtomicU64,
    cancel: &'a CancellationToken,
    progress: &'a Progress,
}

impl<'a> SftpDeletion<'a> {
    /// Opens channels of its own, or shares the browsing one when the server allows no more.
    async fn open(
        session: &'a Session,
        cancel: &'a CancellationToken,
        progress: &'a Progress,
    ) -> AppResult<SftpDeletion<'a>> {
        let opened =
            futures::future::join_all((0..SFTP_CHANNELS).map(|_| session.open_channel())).await;
        let mut channels: Vec<Channel<'a>> = opened
            .into_iter()
            .filter_map(Result::ok)
            .map(Channel::Own)
            .collect();
        if channels.is_empty() {
            channels.push(Channel::Shared(session.sftp()?));
        }
        let requests = Semaphore::new(channels.len() * REQUESTS_PER_CHANNEL);
        Ok(Self {
            channels,
            next_channel: AtomicUsize::new(0),
            requests,
            unlisted: AtomicU64::new(0),
            cancel,
            progress,
        })
    }

    async fn delete(&self, paths: Vec<String>) {
        self.progress.set_counting(true);
        self.progress.found(paths.len() as u64);
        // Until it is known which of them are folders.
        self.unlisted.store(paths.len() as u64, Ordering::Relaxed);
        stream::iter(paths)
            .for_each_concurrent(PARALLEL_ENTRIES, |path| self.remove_top(path))
            .await;
    }

    fn channel(&self) -> &RemoteFs {
        let index = self.next_channel.fetch_add(1, Ordering::Relaxed) % self.channels.len();
        &self.channels[index]
    }

    async fn permit(&self) -> AppResult<SemaphorePermit<'_>> {
        let permit = self
            .requests
            .acquire()
            .await
            .map_err(|_| AppError::cancelled())?;
        if self.cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        Ok(permit)
    }

    fn listed(&self) {
        if self.unlisted.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.progress.set_counting(false);
        }
    }

    async fn remove_top(&self, path: String) {
        let stat = match self.permit().await {
            Ok(_permit) => self.channel().lstat_entry(&path).await,
            Err(error) => Err(error),
        };
        match stat {
            Ok(Some(stat)) if stat.kind == EntryKind::Dir => {
                self.remove_tree(path).await;
            }
            Ok(Some(_)) => {
                self.listed();
                self.remove_file(path).await;
            }
            // Gone already.
            Ok(None) => self.listed(),
            Err(error) => {
                self.listed();
                self.progress.fail(&path, error);
            }
        }
    }

    /// Removes a folder's entries, then the folder; whether it is gone.
    fn remove_tree(&self, folder: String) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            let listing = match self.permit().await {
                Ok(_permit) => self.channel().entries(&folder).await,
                Err(error) => Err(error),
            };
            let children = match listing {
                Ok(children) => children,
                Err(error) => {
                    self.listed();
                    self.progress.fail(&folder, error);
                    return false;
                }
            };
            let subfolders = children
                .iter()
                .filter(|(_, stat)| stat.kind == EntryKind::Dir)
                .count();
            self.progress.found(children.len() as u64);
            self.unlisted
                .fetch_add(subfolders as u64, Ordering::Relaxed);
            self.listed();
            let all_removed = stream::iter(children)
                .map(|(name, stat)| {
                    let path = remote_path::join(&folder, &name);
                    async move {
                        if stat.kind == EntryKind::Dir {
                            self.remove_tree(path).await
                        } else {
                            self.remove_file(path).await
                        }
                    }
                })
                .buffer_unordered(PARALLEL_ENTRIES)
                .fold(true, |all, removed| async move { all && removed })
                .await;
            if !all_removed {
                return false;
            }
            let removed = match self.permit().await {
                Ok(_permit) => self.channel().remove_empty_dir(&folder).await,
                Err(error) => Err(error),
            };
            self.removed(&folder, removed)
        })
    }

    async fn remove_file(&self, path: String) -> bool {
        let removed = match self.permit().await {
            Ok(_permit) => self.channel().remove_file(&path).await,
            Err(error) => Err(error),
        };
        self.removed(&path, removed)
    }

    fn removed(&self, path: &str, result: AppResult<()>) -> bool {
        match result {
            Ok(()) => {
                self.progress.file_done();
                self.progress.working_on(path);
                true
            }
            Err(error) => {
                self.progress.fail(path, error);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_outermost_paths() {
        let paths = [
            "/srv/a/b",
            "/srv/a",
            "/srv/a b",
            "/srv/c",
            "/srv/c",
            "/srv/a/b/c",
        ];
        assert_eq!(
            outermost(paths.map(String::from)),
            ["/srv/a", "/srv/a b", "/srv/c"]
        );
        assert_eq!(outermost(["/srv/x".into(), "/".into()]), ["/"]);
    }

    #[test]
    fn reads_the_paths_rm_names() {
        assert_eq!(removed_path("removed './a/b.txt'"), "./a/b.txt");
        assert_eq!(removed_path("removed directory './a'"), "./a");
        assert_eq!(removed_path("removed directory: './a'"), "./a");
        assert_eq!(removed_path("removed \"./it's\""), "./it's");
        assert_eq!(removed_path("./a/b.txt"), "./a/b.txt");
        assert_eq!(removed_path("removed './directory x'"), "./directory x");
    }

    #[test]
    fn reads_the_reasons_rm_gives() {
        assert_eq!(
            removal_error("rm: cannot remove './a/b': Permission denied"),
            Some(("./a/b", "Permission denied"))
        );
        assert_eq!(
            removal_error("rm: can't remove './a/b': Read-only file system"),
            Some(("./a/b", "Read-only file system"))
        );
        assert_eq!(
            removal_error("rm: ./a/b: Operation not permitted"),
            Some(("./a/b", "Operation not permitted"))
        );
        assert_eq!(removal_error("sh: 1: rm: not found"), None);
    }

    #[test]
    fn splits_lines_across_pieces_of_output() {
        let mut splitter = LineSplitter::default();
        let mut lines = Vec::new();
        splitter.split(b"removed 'a'\nremo", |line| lines.push(line.to_vec()));
        splitter.split(b"ved 'b'\n\nremoved 'c'", |line| lines.push(line.to_vec()));
        splitter.finish(|line| lines.push(line.to_vec()));
        assert_eq!(
            lines,
            [
                b"removed 'a'".to_vec(),
                b"removed 'b'".to_vec(),
                Vec::new(),
                b"removed 'c'".to_vec()
            ]
        );
    }

    #[test]
    fn batches_stay_under_the_command_limit() {
        let names: Vec<String> = (0..20_000)
            .map(|index| format!("file {index}.txt"))
            .collect();
        let split = batches("/srv/www", names.clone());
        assert!(split.len() > 1);
        assert_eq!(split.concat(), names);
        for batch in &split {
            let arguments = batch
                .iter()
                .map(|name| shell_quote(&format!("./{name}")))
                .collect::<Vec<_>>()
                .join(" ");
            let script = format!(
                "cd -- {} || exit 97; [ -e '.poros-delete-00000000000000000000000000000000' ] \
                 || exit 98; printf 'total %s\\n' \"$(find {arguments} 2>/dev/null | wc -l)\"; \
                 LC_ALL=C rm -rfv -- {arguments}",
                shell_quote("/srv/www")
            );
            assert!(format!("sh -c {}", shell_quote(&script)).len() <= COMMAND_LIMIT);
        }
        assert_eq!(batches("/srv", vec!["one".into()]), [["one"]]);
    }
}
