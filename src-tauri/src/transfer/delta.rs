//! Moves only the changed parts of a file the other side already has, with rsync on the
//! server. Anything that keeps rsync from finishing falls back to copying the whole file.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use russh::client::Msg;
use russh::Channel;

use super::conflict::FileFacts;
use super::limiter::RateLimiter;
use super::queue::{JobRun, RunOutcome};
use super::worker::{local_permissions, set_local_permissions, JobContext};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::LogLevel;
use crate::format::format_size;
use crate::rsync::{self, RemoteFile};
use crate::settings::TransferSettings;
use crate::sftp::RemoteStat;

/// rsync 2.6.4 and later speak it.
const MIN_PROTOCOL: i32 = 29;
/// Permissions sent for a file whose own are unknown, as on Windows.
const DEFAULT_PERMISSIONS: u32 = 0o644;

/// What a worker found when it tried the configured rsync command on a server.
pub struct RsyncCheck {
    path: String,
    available: bool,
}

/// `None` when rsync does not apply or did not work; the caller then copies the whole file.
pub(super) async fn try_upload(
    context: &JobContext<'_>,
    metadata: &std::fs::Metadata,
    source: FileFacts,
    existing: &RemoteStat,
) -> AppResult<Option<RunOutcome>> {
    let spec = context.spec;
    if !wanted(context.settings, source.size, existing.size)
        || context.fs.is_symlink(&spec.target).await?
    {
        return Ok(None);
    }
    let Some(rsync_path) = available_rsync(context).await else {
        return Ok(None);
    };
    let permissions = local_permissions(metadata);
    let request = rsync::Upload {
        rsync_path: &rsync_path,
        source: Path::new(&spec.source),
        target: &spec.target,
        size: source.size,
        modified: source.modified.unwrap_or(0),
        permissions: permissions.unwrap_or(DEFAULT_PERMISSIONS),
        preserve_timestamps: context.settings.preserve_timestamps && source.modified.is_some(),
        preserve_permissions: context.settings.preserve_permissions && permissions.is_some(),
    };
    let pace = RunPace::new(context);
    let sent = attempt(context, async {
        let channel = open_channel(context).await?;
        rsync::upload(channel, &request, &pace).await
    })
    .await?;
    Ok(sent.map(|report| finished(context, report)))
}

pub(super) async fn try_download(
    context: &JobContext<'_>,
    source: &RemoteStat,
    existing: FileFacts,
) -> AppResult<Option<RunOutcome>> {
    let spec = context.spec;
    let target = PathBuf::from(&spec.target);
    if !wanted(context.settings, source.size, existing.size) || is_local_symlink(&target).await {
        return Ok(None);
    }
    let Some(rsync_path) = available_rsync(context).await else {
        return Ok(None);
    };
    let partial = partial_path(&target);
    let request = rsync::Download {
        rsync_path: &rsync_path,
        source: &spec.source,
        basis: &target,
        output: &partial,
    };
    let pace = RunPace::new(context);
    let received = attempt(context, async {
        let channel = open_channel(context).await?;
        let (report, file) = rsync::download(channel, &request, &pace).await?;
        put_in_place(context.settings, &partial, &target, file).await?;
        Ok(report)
    })
    .await;
    if !matches!(received, Ok(Some(_))) {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    Ok(received?.map(|report| finished(context, report)))
}

fn wanted(settings: &TransferSettings, source_size: u64, target_size: u64) -> bool {
    settings.delta_transfers && target_size > 0 && source_size >= settings.delta_threshold()
}

fn finished(context: &JobContext<'_>, report: rsync::DeltaReport) -> RunOutcome {
    if context.settings.log_each_file {
        context.shared.events.log(
            LogLevel::Info,
            Some(&context.spec.session_id),
            format!(
                "rsync sent {} of {} for {}; the rest was already there",
                format_size(report.literal_bytes),
                format_size(report.literal_bytes + report.matched_bytes),
                context.spec.name
            ),
        );
    }
    RunOutcome::Completed
}

/// Runs a delta transfer; `None` when it failed in a way a whole-file copy may not.
async fn attempt<T>(
    context: &JobContext<'_>,
    work: impl Future<Output = AppResult<T>>,
) -> AppResult<Option<T>> {
    let run = context.run;
    run.delta.store(true, Ordering::Relaxed);
    let result = tokio::select! {
        biased;
        _ = run.cancel.cancelled() => Err(AppError::cancelled()),
        result = work => result,
    };
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if stops_the_job(&error) => Err(error),
        Err(error) => {
            run.delta.store(false, Ordering::Relaxed);
            run.delta_bytes.store(0, Ordering::Relaxed);
            run.transferred.store(0, Ordering::Relaxed);
            context.shared.events.log(
                LogLevel::Warn,
                Some(&context.spec.session_id),
                format!(
                    "rsync could not update {} ({}); copying the whole file instead",
                    context.spec.name, error.message
                ),
            );
            Ok(None)
        }
    }
}

/// Cancellation, and a lost connection, which a whole-file copy cannot get past either.
fn stops_the_job(error: &AppError) -> bool {
    matches!(
        error.kind,
        ErrorKind::Cancelled | ErrorKind::Disconnected | ErrorKind::Timeout | ErrorKind::Connection
    )
}

/// The configured rsync command, if the server runs it; checked once per server.
async fn available_rsync(context: &JobContext<'_>) -> Option<String> {
    let target = context.shared.target(&context.spec.session_id).ok()?;
    let path = context.settings.rsync_path.clone();
    let mut check = target.rsync.lock().await;
    if let Some(known) = check.as_ref().filter(|known| known.path == path) {
        return known.available.then_some(path);
    }
    let probed = tokio::select! {
        biased;
        _ = context.run.cancel.cancelled() => return None,
        probed = async {
            let channel = open_channel(context).await?;
            rsync::probe(channel, &path).await
        } => probed,
    };
    let protocol = probed.ok().flatten();
    let available = protocol.is_some_and(|version| version >= MIN_PROTOCOL);
    let message = match protocol {
        Some(version) if available => format!(
            "Changed files on {} are updated with rsync (protocol {version})",
            target.label
        ),
        Some(version) => format!(
            "rsync on {} is too old (protocol {version}); files are copied whole",
            target.label
        ),
        None => format!(
            "rsync is not available on {}; files are copied whole",
            target.label
        ),
    };
    context
        .shared
        .events
        .log(LogLevel::Info, Some(&context.spec.session_id), message);
    *check = Some(RsyncCheck {
        path: path.clone(),
        available,
    });
    available.then_some(path)
}

async fn open_channel(context: &JobContext<'_>) -> AppResult<Channel<Msg>> {
    match context.handle {
        Some(handle) => Ok(handle.channel_open_session().await?),
        None => {
            let session = context
                .shared
                .sessions
                .get(&context.spec.session_id)
                .await?;
            session.open_command_channel().await
        }
    }
}

/// The new version is built beside the target and replaces it once complete.
fn partial_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..8];
    target.with_file_name(format!(".{name}.poros-{suffix}"))
}

async fn is_local_symlink(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Gives the new version the server's time, and the permissions a whole-file copy would
/// leave, then moves it over the target.
async fn put_in_place(
    settings: &TransferSettings,
    partial: &Path,
    target: &Path,
    file: RemoteFile,
) -> AppResult<()> {
    let preserve_timestamps = settings.preserve_timestamps;
    let preserve_permissions = settings.preserve_permissions;
    let partial = partial.to_path_buf();
    let target = target.to_path_buf();
    let target_display = target.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let output = std::fs::OpenOptions::new().write(true).open(&partial)?;
        if preserve_timestamps {
            if let Ok(seconds) = u64::try_from(file.modified) {
                output.set_modified(UNIX_EPOCH + Duration::from_secs(seconds))?;
            }
        }
        let permissions = if preserve_permissions {
            Some(file.permissions)
        } else {
            local_permissions(&std::fs::metadata(&target)?)
        };
        set_local_permissions(&output, permissions)?;
        drop(output);
        std::fs::rename(&partial, &target)
    })
    .await?
    .map_err(|error| AppError::from(error).with_path(target_display))
}

/// Progress and speed limits for rsync, in the units the queue shows.
struct RunPace<'a> {
    run: &'a JobRun,
    limiter: &'a RateLimiter,
    total: &'a AtomicU64,
}

impl<'a> RunPace<'a> {
    fn new(context: &JobContext<'a>) -> Self {
        let direction = context.spec.direction;
        Self {
            run: context.run,
            limiter: context.shared.limiter_for(direction),
            total: context.shared.total_for(direction),
        }
    }
}

impl rsync::Pace for RunPace<'_> {
    fn advance(&self, file_bytes: u64, literal_bytes: u64) {
        self.run.add_progress(file_bytes);
        self.run
            .delta_bytes
            .fetch_add(literal_bytes, Ordering::Relaxed);
        self.total.fetch_add(literal_bytes, Ordering::Relaxed);
    }

    async fn throttle(&self, literal_bytes: u64) -> AppResult<()> {
        tokio::select! {
            biased;
            _ = self.run.cancel.cancelled() => Err(AppError::cancelled()),
            _ = self.limiter.acquire(literal_bytes) => Ok(()),
        }
    }
}
