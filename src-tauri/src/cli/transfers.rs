//! Runs files through the app's transfer queue, with its workers, segmented downloads, delta
//! transfers, retries and checks, and waits until every one has finished.

use std::time::Duration;

use serde::Serialize;

use super::args::{Exists, TransferOptions};
use super::context::Context;
use super::exit::Exit;
use super::output::{plural, Reply};
use super::progress::{Reporter, Snapshot};
use crate::error::{AppError, AppResult};
use crate::format::{format_duration, format_size};
use crate::settings::TransferSettings;
use crate::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobKind, JobSnapshot, JobState, TransferItem,
    TransferManager,
};

const POLL: Duration = Duration::from_millis(100);
/// After Ctrl+C, the queue gets this long to stop its workers and remove partial files.
const WIND_DOWN: Duration = Duration::from_millis(1500);

/// The app's transfer settings with the command line's changes. The queue never waits for an
/// answer and keeps nothing for next time.
pub fn settings(base: &TransferSettings, options: &TransferOptions) -> TransferSettings {
    let mut settings = base.clone();
    settings.auto_start = true;
    settings.keep_queue = false;
    settings.keep_completed = true;
    settings.exists_action = match options.exists {
        Exists::Overwrite => ExistsAction::Overwrite,
        Exists::Skip => ExistsAction::Skip,
        Exists::Newer => ExistsAction::OverwriteIfNewer,
        Exists::Different => ExistsAction::OverwriteIfDifferent,
        Exists::Resume => ExistsAction::Resume,
        Exists::Rename => ExistsAction::Rename,
    };
    if let Some(workers) = options.workers {
        settings.workers = workers;
    }
    if options.verify {
        settings.verify_checksums = true;
    }
    if options.no_verify {
        settings.verify_checksums = false;
    }
    settings.flush_to_disk |= options.flush;
    settings.temporary_files &= !options.no_temp_files;
    settings.segmented &= !options.no_segments;
    settings.delta_transfers &= !options.no_delta;
    if let Some(limit) = options.limit_up {
        settings.upload_limit_kib = limit;
    }
    if let Some(limit) = options.limit_down {
        settings.download_limit_kib = limit;
    }
    if let Some(retries) = options.retries {
        settings.retry_attempts = retries;
    }
    settings.preserve_permissions |= options.preserve_permissions;
    settings.preserve_timestamps &= !options.no_preserve_times;
    settings.separate_connections &= !options.single_connection;
    settings
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileResult {
    pub source: String,
    pub target: String,
    pub size: u64,
    pub state: JobState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Bytes rsync actually sent, when only the changes went.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub done: u64,
    pub skipped: u64,
    pub failed: u64,
    pub bytes: u64,
    pub elapsed_millis: u64,
    pub bytes_per_second: u64,
    pub workers: u32,
    pub verified: bool,
    pub files: Vec<FileResult>,
    /// Folders that could not be listed or created, so nothing inside them moved.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folder_failures: Vec<FileResult>,
}

impl TransferSummary {
    pub fn failures(&self) -> u64 {
        self.failed + self.folder_failures.len() as u64
    }

    pub fn describe(&self, verb: &str) -> String {
        let mut text = format!(
            "{} {verb} ({}) in {}",
            plural(self.done, "file"),
            format_size(self.bytes),
            format_duration(Duration::from_millis(self.elapsed_millis))
        );
        if self.bytes_per_second > 0 {
            text.push_str(&format!(", {}/s", format_size(self.bytes_per_second)));
        }
        if self.skipped > 0 {
            text.push_str(&format!(", {} skipped", self.skipped));
        }
        if self.failures() > 0 {
            text.push_str(&format!(", {} failed", self.failures()));
        }
        for failure in self.folder_failures.iter().chain(
            self.files
                .iter()
                .filter(|file| file.state == JobState::Failed),
        ) {
            text.push_str(&format!(
                "\n  failed: {}: {}",
                failure.source,
                failure.error.as_deref().unwrap_or("failed")
            ));
        }
        text
    }

    pub fn into_reply(self, verb: &str) -> Reply {
        let text = self.describe(verb);
        let failed = self.failures() as usize;
        let total = (self.done + self.skipped) as usize + failed;
        Reply::new(&self, text).with_failures(failed, total, Exit::Partial)
    }
}

/// Queues the items and waits for the queue to run dry, reporting progress as it goes.
pub async fn run(
    context: &Context,
    session_id: &str,
    direction: Direction,
    items: Vec<TransferItem>,
    target_directory: String,
    options: &TransferOptions,
) -> AppResult<TransferSummary> {
    let settings = settings(&context.settings.transfers, options);
    let workers = settings.workers;
    let verified = settings.verify_checksums;
    let manager = TransferManager::new(context.sessions.clone(), context.events.clone(), settings);
    manager
        .enqueue(EnqueueRequest {
            session_id: session_id.to_string(),
            source_session_id: None,
            direction,
            target_directory,
            items,
        })
        .await?;
    wait(context, &manager, workers, verified).await
}

/// Waits for every job the manager holds, then sums them up.
pub async fn wait(
    context: &Context,
    manager: &TransferManager,
    workers: u32,
    verified: bool,
) -> AppResult<TransferSummary> {
    let mut reporter = Reporter::new(context.progress());
    loop {
        let list = manager.list();
        reporter.update(snapshot(&list.jobs));
        let counts = &list.stats.counts;
        if counts.queued == 0 && counts.running == 0 && counts.paused == 0 && counts.conflict == 0 {
            break;
        }
        tokio::select! {
            _ = context.interrupt.cancelled() => {
                reporter.finish();
                let ids: Vec<_> = list.jobs.iter().map(|job| job.id).collect();
                manager.remove(&ids);
                tokio::time::sleep(WIND_DOWN).await;
                return Err(AppError::cancelled());
            }
            _ = tokio::time::sleep(POLL) => {}
        }
    }
    reporter.finish();
    let elapsed = reporter.elapsed();
    Ok(summarize(&manager.list().jobs, elapsed, workers, verified))
}

fn snapshot(jobs: &[JobSnapshot]) -> Snapshot {
    let mut snapshot = Snapshot::default();
    for job in jobs.iter().filter(|job| job.kind == JobKind::File) {
        snapshot.files_total += 1;
        snapshot.bytes_total += job.size;
        match job.state {
            JobState::Done | JobState::Skipped => {
                snapshot.files_done += 1;
                snapshot.bytes_done += job.size;
            }
            JobState::Failed => snapshot.failed += 1,
            _ => snapshot.bytes_done += job.transferred.min(job.size),
        }
    }
    snapshot
}

fn summarize(
    jobs: &[JobSnapshot],
    elapsed: Duration,
    workers: u32,
    verified: bool,
) -> TransferSummary {
    let mut summary = TransferSummary {
        elapsed_millis: elapsed.as_millis() as u64,
        workers,
        verified,
        ..TransferSummary::default()
    };
    for job in jobs {
        let result = FileResult {
            source: job.source.clone(),
            target: job.target.clone(),
            size: job.size,
            state: job.state,
            error: job.error.clone(),
            delta_bytes: job.delta_bytes,
        };
        match (job.kind, job.state) {
            (JobKind::Folder, JobState::Failed) => summary.folder_failures.push(result),
            (JobKind::Folder, _) => {}
            (JobKind::File, state) => {
                match state {
                    JobState::Done => {
                        summary.done += 1;
                        summary.bytes += job.size;
                    }
                    JobState::Skipped => summary.skipped += 1,
                    _ => summary.failed += 1,
                }
                summary.files.push(result);
            }
        }
    }
    let seconds = elapsed.as_secs_f64();
    if seconds > 0.0 {
        summary.bytes_per_second = (summary.bytes as f64 / seconds) as u64;
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn options(arguments: &[&str]) -> TransferOptions {
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            options: TransferOptions,
        }
        let mut line = vec!["test"];
        line.extend_from_slice(arguments);
        Wrapper::try_parse_from(line).unwrap().options
    }

    #[test]
    fn the_command_line_changes_only_what_it_names() {
        let base = TransferSettings {
            verify_checksums: true,
            workers: 6,
            ..TransferSettings::default()
        };
        let kept = settings(&base, &options(&[]));
        assert_eq!(kept.workers, 6);
        assert!(kept.verify_checksums && kept.auto_start && !kept.keep_queue);
        assert_eq!(kept.exists_action, ExistsAction::Overwrite);

        let changed = settings(
            &base,
            &options(&[
                "-w",
                "12",
                "--no-verify",
                "--exists",
                "resume",
                "--no-temp-files",
            ]),
        );
        assert_eq!(changed.workers, 12);
        assert!(!changed.verify_checksums && !changed.temporary_files);
        assert_eq!(changed.exists_action, ExistsAction::Resume);

        let last_wins = settings(&base, &options(&["--no-verify", "--verify"]));
        assert!(last_wins.verify_checksums);
    }
}
