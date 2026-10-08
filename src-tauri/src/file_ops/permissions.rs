//! What the Properties dialog shows and changes on a server: permissions, owner and group, and
//! how much a folder holds.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{Progress, ServerFeatures};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::EntryKind;
use crate::remote_path;
use crate::rsync::{run_command, shell_quote};
use crate::session::Session;
use crate::sftp::{EntryStat, RemoteFs};

const PARALLEL_ENTRIES: usize = 16;
const COMMAND_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_COMMAND_BYTES: usize = 32 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// Seconds since the Unix epoch.
    pub accessed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
}

pub async fn details(fs: &RemoteFs, path: &str) -> AppResult<Details> {
    let path = fs.resolve(path);
    let stat = fs.lstat_entry(&path).await?.ok_or_else(|| {
        AppError::new(ErrorKind::NotFound, "The item no longer exists").with_path(path.clone())
    })?;
    let link_target = if stat.kind == EntryKind::Symlink {
        fs.read_link(&path).await.ok()
    } else {
        None
    };
    Ok(Details {
        uid: stat.uid,
        gid: stat.gid,
        accessed: stat.accessed,
        link_target,
    })
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
}

#[derive(Default)]
struct UsageCounter {
    files: AtomicU64,
    folders: AtomicU64,
    bytes: AtomicU64,
}

/// Counts what the items hold, following no links.
pub async fn measure(
    session: &Session,
    paths: &[String],
    cancel: &CancellationToken,
    progress: &Progress,
) -> AppResult<Usage> {
    let fs = &session.fs;
    let counter = UsageCounter::default();
    let walk = async {
        for path in paths {
            let path = fs.resolve(path);
            let stat = fs.lstat_entry(&path).await?.ok_or_else(|| {
                AppError::new(ErrorKind::NotFound, "The item no longer exists")
                    .with_path(path.clone())
            })?;
            count(fs, path, stat, &counter, progress).await?;
        }
        Ok::<(), AppError>(())
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(AppError::cancelled()),
        walked = walk => walked?,
    }
    Ok(Usage {
        files: counter.files.into_inner(),
        folders: counter.folders.into_inner(),
        bytes: counter.bytes.into_inner(),
    })
}

fn count<'a>(
    fs: &'a RemoteFs,
    path: String,
    stat: EntryStat,
    counter: &'a UsageCounter,
    progress: &'a Progress,
) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(async move {
        if stat.kind != EntryKind::Dir {
            counter.files.fetch_add(1, Ordering::Relaxed);
            counter.bytes.fetch_add(stat.size, Ordering::Relaxed);
            progress.file_done();
            progress.add_bytes(stat.size);
            return Ok(());
        }
        counter.folders.fetch_add(1, Ordering::Relaxed);
        let children = fs.entries(&path).await?;
        let results: Vec<AppResult<()>> = stream::iter(children)
            .map(|(name, child)| {
                count(
                    fs,
                    remote_path::join(&path, &name),
                    child,
                    counter,
                    progress,
                )
            })
            .buffer_unordered(PARALLEL_ENTRIES)
            .collect()
            .await;
        results.into_iter().collect()
    })
}

/// Bits to turn on and off; bits in neither keep each entry's own value, so a selection with
/// mixed permissions can change one bit and leave the rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeChange {
    pub set: u32,
    pub clear: u32,
}

impl ModeChange {
    pub fn apply(self, mode: u32) -> u32 {
        ((mode & !self.clear) | self.set) & 0o7777
    }

    fn is_empty(self) -> bool {
        (self.set | self.clear) & 0o7777 == 0
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    /// Names this change for progress events and cancelling.
    pub operation_id: String,
    pub session_id: String,
    pub paths: Vec<String>,
    pub files: ModeChange,
    pub folders: ModeChange,
    /// Also change everything inside the folders.
    #[serde(default)]
    pub recursive: bool,
    /// A user name or number; `None` keeps the owner.
    pub owner: Option<String>,
    /// A group name or number; `None` keeps the group.
    pub group: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSummary {
    pub changed: u64,
    /// Links have no permissions of their own and are left alone.
    pub skipped_links: u64,
    pub failures: Vec<super::Failure>,
}

pub async fn apply(
    session: &Session,
    features: ServerFeatures,
    request: &PermissionRequest,
    cancel: &CancellationToken,
    progress: &Progress,
) -> AppResult<PermissionSummary> {
    let owner = clean(&request.owner);
    let group = clean(&request.group);
    let skipped_links = AtomicU64::new(0);
    let work = async {
        if owner.is_some() || group.is_some() {
            change_owner(session, features, request, owner, group, progress).await?;
        }
        if !request.files.is_empty() || !request.folders.is_empty() {
            let walk = Walk {
                fs: &session.fs,
                request,
                progress,
                skipped_links: &skipped_links,
            };
            for path in &request.paths {
                let path = session.fs.resolve(path);
                match session.fs.lstat_entry(&path).await {
                    Ok(Some(stat)) => walk.change_mode(path, stat).await,
                    Ok(None) => progress.fail(
                        &path,
                        AppError::new(ErrorKind::NotFound, "The item no longer exists"),
                    ),
                    Err(error) => progress.fail(&path, error),
                }
            }
        }
        Ok::<(), AppError>(())
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(AppError::cancelled()),
        done = work => done?,
    }
    Ok(PermissionSummary {
        changed: progress.files_done(),
        skipped_links: skipped_links.into_inner(),
        failures: progress.take_failures(),
    })
}

fn clean(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

struct Walk<'a> {
    fs: &'a RemoteFs,
    request: &'a PermissionRequest,
    progress: &'a Progress,
    skipped_links: &'a AtomicU64,
}

impl Walk<'_> {
    fn change_mode(&self, path: String, stat: EntryStat) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let change = match stat.kind {
                EntryKind::Symlink => {
                    self.skipped_links.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                EntryKind::Dir => self.request.folders,
                _ => self.request.files,
            };
            if let Some(current) = stat.permissions {
                let wanted = change.apply(current);
                if wanted != current {
                    self.progress.working_on(&path);
                    match self.fs.set_permissions(&path, wanted).await {
                        Ok(()) => self.progress.file_done(),
                        Err(error) => self.progress.fail(&path, error),
                    }
                }
            }
            if stat.kind != EntryKind::Dir || !self.request.recursive {
                return;
            }
            let children = match self.fs.entries(&path).await {
                Ok(children) => children,
                Err(error) => return self.progress.fail(&path, error),
            };
            stream::iter(children)
                .for_each_concurrent(PARALLEL_ENTRIES, |(name, child)| {
                    self.change_mode(remote_path::join(&path, &name), child)
                })
                .await;
        })
    }
}

/// `chown` on the server accepts names and numbers. Without commands, SFTP can still set
/// numeric ids.
async fn change_owner(
    session: &Session,
    features: ServerFeatures,
    request: &PermissionRequest,
    owner: Option<&str>,
    group: Option<&str>,
    progress: &Progress,
) -> AppResult<()> {
    let (program, spec) = match (owner, group) {
        (Some(owner), Some(group)) => ("chown", format!("{owner}:{group}")),
        (Some(owner), None) => ("chown", owner.to_string()),
        (None, Some(group)) => ("chgrp", group.to_string()),
        (None, None) => return Ok(()),
    };
    if spec.starts_with('-') || spec.contains(char::is_whitespace) {
        return Err(AppError::invalid(format!(
            "\"{spec}\" is not a valid owner or group"
        )));
    }
    let paths: Vec<String> = request
        .paths
        .iter()
        .map(|path| session.fs.resolve(path))
        .collect();
    if !(features.commands && super::server::runs_commands(session).await) {
        return change_owner_by_number(session, &paths, owner, group, request, progress).await;
    }
    let options = if request.recursive { "-R -P" } else { "-h" };
    let prefix = format!("{program} {options} -- {}", shell_quote(&spec));
    for command in command_batches(&prefix, &paths) {
        let channel = session.open_command_channel().await?;
        let output = tokio::time::timeout(
            COMMAND_TIMEOUT,
            run_command(channel, &command, COMMAND_OUTPUT_LIMIT),
        )
        .await
        .map_err(|_| AppError::new(ErrorKind::Timeout, format!("{program} took too long")))??;
        match output.exit_status {
            Some(0) => {}
            _ => {
                let reported = output
                    .error_output
                    .trim()
                    .lines()
                    .last()
                    .unwrap_or_default();
                let message = if reported.is_empty() {
                    format!("{program} on the server failed")
                } else {
                    reported.trim().to_string()
                };
                let kind = if reported.contains("Operation not permitted")
                    || reported.contains("Permission denied")
                {
                    ErrorKind::PermissionDenied
                } else {
                    ErrorKind::Io
                };
                return Err(AppError::new(kind, message));
            }
        }
    }
    Ok(())
}

async fn change_owner_by_number(
    session: &Session,
    paths: &[String],
    owner: Option<&str>,
    group: Option<&str>,
    request: &PermissionRequest,
    progress: &Progress,
) -> AppResult<()> {
    let parse = |value: Option<&str>| -> AppResult<Option<u32>> {
        value
            .map(|text| {
                text.parse::<u32>().map_err(|_| {
                    AppError::invalid(
                        "This server does not run commands, so owners and groups can only be set by number",
                    )
                })
            })
            .transpose()
    };
    let (uid, gid) = (parse(owner)?, parse(group)?);
    for path in paths {
        set_ids(
            &session.fs,
            path.clone(),
            uid,
            gid,
            request.recursive,
            progress,
        )
        .await;
    }
    Ok(())
}

fn set_ids<'a>(
    fs: &'a RemoteFs,
    path: String,
    uid: Option<u32>,
    gid: Option<u32>,
    recursive: bool,
    progress: &'a Progress,
) -> BoxFuture<'a, ()> {
    Box::pin(async move {
        let stat = match fs.lstat_entry(&path).await {
            Ok(Some(stat)) => stat,
            Ok(None) => return,
            Err(error) => return progress.fail(&path, error),
        };
        if stat.kind == EntryKind::Symlink {
            return;
        }
        if let (Some(new_uid), Some(new_gid)) = (uid.or(stat.uid), gid.or(stat.gid)) {
            if let Err(error) = fs.set_owner(&path, new_uid, new_gid).await {
                return progress.fail(&path, error);
            }
        }
        if recursive && stat.kind == EntryKind::Dir {
            let children = match fs.entries(&path).await {
                Ok(children) => children,
                Err(error) => return progress.fail(&path, error),
            };
            stream::iter(children)
                .for_each_concurrent(PARALLEL_ENTRIES, |(name, _)| {
                    set_ids(
                        fs,
                        remote_path::join(&path, &name),
                        uid,
                        gid,
                        recursive,
                        progress,
                    )
                })
                .await;
        }
    })
}

/// `prefix` followed by as many quoted paths as fit a modest command line.
fn command_batches(prefix: &str, paths: &[String]) -> Vec<String> {
    let mut batches = Vec::new();
    let mut command = prefix.to_string();
    for path in paths {
        let quoted = shell_quote(path);
        if command.len() > prefix.len() && command.len() + quoted.len() + 1 > MAX_COMMAND_BYTES {
            batches.push(std::mem::replace(&mut command, prefix.to_string()));
        }
        command.push(' ');
        command.push_str(&quoted);
    }
    if command.len() > prefix.len() {
        batches.push(command);
    }
    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_changes_keep_bits_they_do_not_name() {
        let add_group_write = ModeChange {
            set: 0o020,
            clear: 0,
        };
        assert_eq!(add_group_write.apply(0o644), 0o664);
        assert_eq!(add_group_write.apply(0o755), 0o775);
        let exact = ModeChange {
            set: 0o644,
            clear: 0o7777 & !0o644,
        };
        assert_eq!(exact.apply(0o4777), 0o644);
        assert!(ModeChange::default().is_empty());
    }

    #[test]
    fn splits_long_path_lists() {
        let paths: Vec<String> = (0..3000)
            .map(|index| format!("/srv/data/file-{index}"))
            .collect();
        let batches = command_batches("chown -h -- 'www'", &paths);
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|batch| batch.len() <= MAX_COMMAND_BYTES));
        assert!(batches
            .iter()
            .all(|batch| batch.starts_with("chown -h -- 'www' '")));
        let total: usize = batches
            .iter()
            .map(|batch| batch.matches("'/srv").count())
            .sum();
        assert_eq!(total, 3000);
    }
}
