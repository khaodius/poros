//! The transfer kind of scheduled task: a saved list of files and folders copied to or from a
//! saved connection of any protocol, or from one saved connection to another. The files go
//! through the transfer queue, so its workers, limits, integrity checks and retries apply as
//! they do to any transfer, and the run ends when its own jobs have.

use std::path::PathBuf;

use chrono::Local;

use super::runner::{self, plural, TaskContext};
use super::schedule::expand_folder_pattern;
use super::task::{Outcome, ScheduledTask, TaskAction, TaskItem};
use crate::error::{AppError, AppResult};
use crate::events::LogLevel;
use crate::format::format_size;
use crate::protocol::{self, RemoteFileSystem};
use crate::remote_path;
use crate::session::SessionInfo;
use crate::transfer::{BatchTally, Direction, EnqueueRequest, JobBatch, TransferItem};

/// The result names the first failure and the first missing item, cut to this length.
const MAX_REASON_CHARS: usize = 160;

pub(super) async fn run(task: &ScheduledTask, context: &TaskContext<'_>) -> AppResult<Outcome> {
    let TaskAction::Transfer {
        connection_id,
        source_connection_id,
        direction,
        subfolder,
        ..
    } = &task.action
    else {
        return Err(AppError::invalid("Not a transfer"));
    };
    // The app checks a task as it is saved, but not one edited by hand in schedules.json: a
    // relative download folder would land wherever this process was started.
    task.action.validate(&Local::now())?;
    // Checked before anything is connected: a paused queue would keep the run, and the next
    // ones behind it, waiting for as long as it stays paused.
    if context.transfers.is_paused() {
        return Ok(runner::skipped_while_paused());
    }
    let subfolder = expand_folder_pattern(subfolder, &Local::now())?;
    let source_id = match (direction, source_connection_id) {
        (Direction::Relay, Some(source_id)) => Some(source_id),
        (Direction::Relay, None) => {
            return Err(AppError::invalid(
                "Choose the saved connection to copy from",
            ))
        }
        _ => None,
    };

    let target = runner::connect(context, connection_id, None).await?;
    let source = match source_id {
        Some(source_id) => match runner::connect(context, source_id, None).await {
            Ok(source) => Some(source),
            Err(error) => {
                let _ = context.sessions.disconnect(&target.id).await;
                return Err(error);
            }
        },
        None => None,
    };
    let result = copy(task, &target, source.as_ref(), &subfolder, context).await;
    if let Some(source) = &source {
        let _ = context.sessions.disconnect(&source.id).await;
    }
    let _ = context.sessions.disconnect(&target.id).await;
    result
}

/// A path on the server as the task has it: full, from the home folder with `~`, or from the
/// folder the connection opens in.
fn resolve_remote(session: &SessionInfo, path: &str) -> String {
    let path = path.trim();
    let resolved = if path == "~" || path.starts_with("~/") {
        protocol::resolve_against(&session.home, path)
    } else {
        protocol::resolve_against(&session.initial_path, path)
    };
    protocol::normalize(&resolved)
}

fn folder_parts(subfolder: &str) -> impl Iterator<Item = &str> {
    subfolder.split('/').filter(|part| !part.is_empty())
}

/// Creates `path` on the server with the folders above it that are missing.
async fn make_remote_folders(files: &dyn RemoteFileSystem, path: &str) -> AppResult<()> {
    match files.stat(path).await? {
        Some(stat) if stat.is_dir => return Ok(()),
        Some(_) => return Err(protocol::not_a_folder(path)),
        None => {}
    }
    let mut current = String::from("/");
    for part in folder_parts(path) {
        current = remote_path::join(&current, part);
        match files.stat(&current).await? {
            Some(stat) if stat.is_dir => {}
            Some(_) => return Err(protocol::not_a_folder(&current)),
            None => files.ensure_dir(&current).await?,
        }
    }
    Ok(())
}

/// The folder the items go into, made if it is not there: the destination, and inside it the
/// folder named from the time of the run.
async fn prepare_folder(
    direction: Direction,
    destination: &str,
    subfolder: &str,
    target: &SessionInfo,
    files: &dyn RemoteFileSystem,
) -> AppResult<String> {
    if direction == Direction::Download {
        let mut folder = PathBuf::from(destination.trim());
        folder.extend(folder_parts(subfolder));
        tokio::fs::create_dir_all(&folder)
            .await
            .map_err(|error| AppError::from(error).with_path(folder.display().to_string()))?;
        return Ok(folder.to_string_lossy().into_owned());
    }
    let mut folder = resolve_remote(target, destination);
    for part in folder_parts(subfolder) {
        folder = remote_path::join(&folder, part);
    }
    make_remote_folders(files, &folder).await?;
    Ok(folder)
}

/// An item as it is now: a file or a folder, and its size. `None` when it is not there.
async fn look_at(
    item: &TaskItem,
    direction: Direction,
    remote: &dyn RemoteFileSystem,
) -> AppResult<Option<(bool, u64)>> {
    if direction == Direction::Upload {
        return match tokio::fs::metadata(&item.path).await {
            Ok(metadata) => Ok(Some((metadata.is_dir(), metadata.len()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(AppError::from(error).with_path(item.path.clone())),
        };
    }
    let found = remote.stat(&protocol::normalize(&item.path)).await?;
    Ok(found.map(|stat| (stat.is_dir, stat.size)))
}

async fn copy(
    task: &ScheduledTask,
    target: &SessionInfo,
    source: Option<&SessionInfo>,
    subfolder: &str,
    context: &TaskContext<'_>,
) -> AppResult<Outcome> {
    let TaskAction::Transfer {
        direction,
        items,
        destination,
        exists,
        ..
    } = &task.action
    else {
        return Err(AppError::invalid("Not a transfer"));
    };
    let direction = *direction;
    let target_files = context.sessions.get(&target.id).await?.files();
    let folder = prepare_folder(direction, destination, subfolder, target, &*target_files).await?;
    // Where the items are read from, when that is a server.
    let read_from = match source {
        Some(source) => context.sessions.get(&source.id).await?.files(),
        None => target_files,
    };

    let mut found = Vec::with_capacity(items.len());
    let mut not_found = Vec::new();
    for item in items {
        match look_at(item, direction, &*read_from).await? {
            Some((is_dir, size)) => found.push(TransferItem {
                path: match direction {
                    Direction::Upload => item.path.clone(),
                    Direction::Download | Direction::Relay => protocol::normalize(&item.path),
                },
                name: item.name(direction),
                is_dir,
                size,
            }),
            None => {
                context.events.log(
                    LogLevel::Warn,
                    None,
                    format!(
                        "Scheduled task \"{}\": {} is not there any more, so it is left out",
                        task.name, item.path
                    ),
                );
                not_found.push(item.path.clone());
            }
        }
    }
    if found.is_empty() {
        return Ok(Outcome {
            succeeded: false,
            message: describe(direction, &folder, &BatchTally::default(), &not_found),
        });
    }

    let batch = JobBatch::new(task.name.clone(), Some(*exists));
    let queued = context
        .transfers
        .enqueue_batch(
            EnqueueRequest {
                session_id: target.id.clone(),
                source_session_id: source.map(|source| source.id.clone()),
                direction,
                target_directory: folder.clone(),
                items: found,
            },
            batch.clone(),
        )
        .await?;
    if !queued {
        return Ok(runner::skipped_while_paused());
    }
    let tally = context.transfers.watch().wait_for_batch(&batch).await;
    context.done_with(&batch);
    for failure in &tally.failures {
        context.events.log(
            LogLevel::Error,
            None,
            format!("Scheduled task \"{}\": {failure}", task.name),
        );
    }
    Ok(Outcome {
        succeeded: tally.is_clean() && not_found.is_empty(),
        message: describe(direction, &folder, &tally, &not_found),
    })
}

fn shortened(text: &str) -> String {
    let mut shown: String = text.chars().take(MAX_REASON_CHARS).collect();
    if shown.len() < text.len() {
        shown.push_str("...");
    }
    shown
}

/// The result of a run in one line: what moved, then everything that did not.
fn describe(
    direction: Direction,
    folder: &str,
    tally: &BatchTally,
    not_found: &[String],
) -> String {
    let verb = match direction {
        Direction::Upload => "uploaded",
        Direction::Download => "downloaded",
        Direction::Relay => "copied",
    };
    let mut message = format!(
        "{} {verb} ({}) to {folder}",
        plural(tally.done, "file"),
        format_size(tally.bytes)
    );
    if tally.skipped > 0 {
        message.push_str(&format!(", {} skipped", tally.skipped));
    }
    if tally.failed > 0 {
        message.push_str(&format!(", {} failed", tally.failed));
        if let Some(first) = tally.failures.first() {
            message.push_str(&format!(" ({})", shortened(first)));
        }
    }
    if tally.removed > 0 {
        message.push_str(&format!(", {} taken off the queue", tally.removed));
    }
    if tally.unfinished > 0 {
        message.push_str(&format!(", {} not finished", tally.unfinished));
    }
    if let Some(first) = not_found.first() {
        message.push_str(&format!(
            ", {} not found ({})",
            plural(not_found.len() as u64, "item"),
            shortened(first)
        ));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Protocol;

    fn session(home: &str, initial_path: &str) -> SessionInfo {
        SessionInfo {
            id: "session".into(),
            label: "server".into(),
            host: "example.com".into(),
            port: 22,
            username: "me".into(),
            home: home.into(),
            initial_path: initial_path.into(),
            saved_connection_id: None,
            protocol: Protocol::Sftp,
            vault: None,
            coarse_times: false,
        }
    }

    #[test]
    fn server_folders_start_at_the_root_the_home_or_where_the_connection_opens() {
        let session = session("/home/me", "/srv/www");
        assert_eq!(resolve_remote(&session, "/var/backups/"), "/var/backups");
        assert_eq!(resolve_remote(&session, "~/backups"), "/home/me/backups");
        assert_eq!(resolve_remote(&session, "~"), "/home/me");
        assert_eq!(resolve_remote(&session, "site/../logs"), "/srv/www/logs");
        assert_eq!(resolve_remote(&session, " /a/./b "), "/a/b");
    }

    #[tokio::test]
    async fn a_task_edited_by_hand_is_checked_before_it_runs() {
        let folder = tempfile::tempdir().unwrap();
        let events = crate::events::Events::default();
        let sessions = std::sync::Arc::new(crate::session::SessionManager::new(
            folder.path().join("known_hosts"),
            events.clone(),
        ));
        let sync = crate::sync::SyncManager::new(sessions.clone(), events.clone());
        let transfers = crate::transfer::TransferManager::new(
            sessions.clone(),
            events.clone(),
            Default::default(),
        );
        let context = TaskContext {
            sessions: &sessions,
            connections: std::sync::Arc::new(crate::connections::ConnectionStore::new(
                folder.path().join("connections.json"),
                Box::new(crate::connections::tests::MemorySecrets::default()),
            )),
            connection_settings: Default::default(),
            cloud_settings: Default::default(),
            sync: &sync,
            transfers: &transfers,
            events: &events,
            sign_in: Default::default(),
            keep_ended_jobs: true,
        };
        let mut task = super::super::task::tests::task(super::super::schedule::Trigger::Every {
            minutes: 60,
            start: 0,
        });
        task.action = TaskAction::Transfer {
            connection_id: "server".into(),
            source_connection_id: None,
            direction: Direction::Download,
            items: vec![TaskItem {
                path: "/var/www".into(),
                is_dir: true,
            }],
            // Would land under the folder the process was started in.
            destination: "reldest".into(),
            exists: crate::transfer::ExistsAction::Overwrite,
            subfolder: String::new(),
        };
        let error = run(&task, &context).await.unwrap_err();
        assert!(
            error.message.contains("reldest is not a full path"),
            "{}",
            error.message
        );
        assert!(!std::path::Path::new("reldest").exists());
    }

    #[test]
    fn results_say_what_moved_and_what_did_not() {
        let clean = BatchTally {
            done: 12,
            bytes: 3_565_158,
            ..BatchTally::default()
        };
        assert_eq!(
            describe(Direction::Upload, "/var/backups/2026-11-23", &clean, &[]),
            "12 files uploaded (3.4 MB) to /var/backups/2026-11-23"
        );
        let mixed = BatchTally {
            done: 1,
            skipped: 4,
            failed: 2,
            removed: 1,
            unfinished: 3,
            bytes: 10,
            failures: vec!["/srv/a.bin: Permission denied".into(), "second".into()],
        };
        assert_eq!(
            describe(
                Direction::Relay,
                "/archive",
                &mixed,
                &["/srv/gone".into(), "/srv/also-gone".into()]
            ),
            "1 file copied (10 B) to /archive, 4 skipped, 2 failed (/srv/a.bin: Permission denied), \
             1 taken off the queue, 3 not finished, 2 items not found (/srv/gone)"
        );
        let long = BatchTally {
            failed: 1,
            failures: vec!["x".repeat(400)],
            ..BatchTally::default()
        };
        let message = describe(Direction::Download, "/home/me", &long, &[]);
        assert!(message.len() < 260, "{message}");
        assert!(message.ends_with("...)"));
    }
}
