//! Every transfer that is not an SFTP upload or download: those with FTP, FTPS and cloud
//! servers, and copies from one server to another. Data streams through `RemoteFileSystem`.
//! Downloads from servers that can read from an offset still split large files between
//! workers; uploads and copies go in order. Two FTP servers that allow it send a file to each
//! other directly (FXP), and otherwise the copy streams through this computer.
//!
//! Downloads go through a temporary file like SFTP ones. Uploads and copies write the target
//! itself: cloud uploads appear only once complete anyway, and FTP servers differ on whether a
//! rename may replace a file.

use std::future::Future;
use std::io::SeekFrom;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use bytes::Bytes;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use super::conflict::{decide, Decision, ExistsAction, FileFacts};
use super::copy::read_up_to;
use super::limiter::RateLimiter;
use super::queue::{Direction, JobKind, JobRun, JobSpec, Plan, ResumePoint, Role, RunOutcome};
use super::worker::{self, Connections, Destination, JobRef};
use super::Shared;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::LogLevel;
use crate::format::format_size;
use crate::ftp::fxp;
use crate::protocol::{
    Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest, WriteStream,
};
use crate::remote_path;

/// Bytes read from a local file per write to the server.
const LOCAL_READ_CHUNK: usize = 256 * 1024;

pub(super) fn applies(shared: &Shared, spec: &JobSpec) -> bool {
    spec.direction == Direction::Relay
        || shared
            .target(&spec.session_id)
            .is_ok_and(|target| target.protocol() != Protocol::Sftp)
}

/// The connection a relay reads from: its source session's, or a second one when source and
/// target are the same server.
pub(super) fn source_key(spec: &JobSpec) -> Option<String> {
    let source = spec.source_session_id.as_deref()?;
    Some(if source == spec.session_id {
        format!("{source}#source")
    } else {
        source.to_string()
    })
}

struct Endpoints {
    /// The server of an upload or download, or the target of a relay.
    remote: Arc<dyn RemoteFileSystem>,
    /// The server a relay reads from.
    source: Option<Arc<dyn RemoteFileSystem>>,
    /// Both ends of a relay are FTP connections borrowed from browsing sessions.
    borrows_both: bool,
}

async fn endpoints(job: &JobRef<'_>, connections: &mut Connections) -> AppResult<Endpoints> {
    let spec = job.spec;
    let target = connections
        .get(job.shared, &spec.session_id, &spec.session_id)
        .await?;
    let remote = target.files.clone();
    let mut borrows_both = target.borrows_ftp_connection();
    let source = match (&spec.source_session_id, source_key(spec)) {
        (Some(source_id), Some(key)) => {
            let source = connections.get(job.shared, source_id, &key).await?;
            borrows_both &= source.borrows_ftp_connection();
            Some(source.files.clone())
        }
        _ => {
            borrows_both = false;
            None
        }
    };
    // One FTP connection cannot read and write at once.
    if let Some(source) = &source {
        if source.protocol().is_ftp()
            && std::ptr::addr_eq(Arc::as_ptr(source), Arc::as_ptr(&remote))
        {
            return Err(AppError::unsupported(
                if job.settings.separate_connections {
                    "This server accepts only one connection, so files cannot be copied within it"
                } else {
                    "Copying within an FTP server needs a second connection. Turn on \"Give each \
                 transfer its own connection\" in Settings, Transfers."
                },
            ));
        }
    }
    Ok(Endpoints {
        remote,
        source,
        borrows_both,
    })
}

pub(super) async fn run(
    job: &JobRef<'_>,
    connections: &mut Connections,
    role: Role,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
) -> AppResult<RunOutcome> {
    let endpoints = endpoints(job, connections).await?;
    let spec = job.spec;
    match (spec.kind, role, spec.direction) {
        (JobKind::Folder, _, _) => {
            worker::expand_folder(job, endpoints.remote.as_ref(), endpoints.source.as_deref()).await
        }
        (JobKind::File, Role::Primary, Direction::Download) => {
            download(job, endpoints.remote, resolution, resume).await
        }
        (JobKind::File, Role::Primary, Direction::Upload) => {
            upload(job, endpoints.remote, resolution, resume).await
        }
        (JobKind::File, Role::Primary, Direction::Relay) => {
            let _turn = match endpoints.borrows_both {
                true => Some(tokio::select! {
                    turn = job.shared.borrowed_relays.lock() => turn,
                    _ = job.run.cancel.cancelled() => return Err(AppError::cancelled()),
                }),
                false => None,
            };
            relay(job, endpoints, resolution, resume).await
        }
        (JobKind::File, Role::Helper, _) => help(job, endpoints.remote).await,
    }
}

async fn download(
    job: &JobRef<'_>,
    files: Arc<dyn RemoteFileSystem>,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
) -> AppResult<RunOutcome> {
    let spec = job.spec;
    let source = remote_file(files.as_ref(), &spec.source).await?;
    let source_facts = facts(&source);
    let can_resume = files.capabilities().ranged_reads;
    let fresh = |target: String| worker::fresh_download(job.settings, target);
    let existing = worker::local_facts(&spec.target).await?;
    let mut destination = match (resume, existing) {
        (Some(resume), _) if !can_resume || worker::source_changed(&resume, source_facts) => {
            fresh(spec.target.clone()).await
        }
        (Some(resume), existing) => {
            let written = match &resume.partial {
                Some(partial) => worker::local_facts(partial).await?,
                None => existing,
            };
            let on_disk = written.map_or(0, |(_, facts)| facts.size);
            Destination {
                target: spec.target.clone(),
                partial: resume.partial,
                start: resume.offset.min(on_disk).min(source.size),
            }
        }
        (None, None) => fresh(spec.target.clone()).await,
        (None, Some((true, _))) => return Err(worker::folder_in_the_way(spec)),
        (None, Some((false, target_facts))) => {
            match decide(job.exists_action(resolution), source_facts, target_facts) {
                Decision::Ask => return Ok(worker::conflict(source_facts, target_facts)),
                Decision::Skip => return Ok(RunOutcome::Skipped("Already exists".into())),
                Decision::Rename => {
                    let (target, name) = worker::free_local_name(&spec.target).await?;
                    job.retarget(&target, name);
                    fresh(target).await
                }
                Decision::Write { offset } if offset > 0 && can_resume => {
                    Destination::in_place(&spec.target, offset)
                }
                Decision::Write { .. } => fresh(spec.target.clone()).await,
            }
        }
    };
    if source.size == 0 {
        // Nothing to read, but the server still has a say: some items cannot be downloaded.
        files
            .clone()
            .open_read(&spec.source, 0..0)
            .await?
            .finish()
            .await?;
    }

    let write_path = destination.write_path().to_string();
    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .truncate(destination.start == 0)
        .open(&write_path)
        .await
        .map_err(|error| AppError::from(error).with_path(write_path.clone()))?;
    if destination.start > 0 {
        let window = worker::check_window(destination.start);
        let theirs = read_remote_range(job, &files, &spec.source, window.start, window.len).await?;
        let ours = worker::read_local_range(&mut file, window.start, window.len).await?;
        if theirs != ours {
            job.log_starting_over();
            file.set_len(0).await?;
            destination.start = 0;
        }
    }
    job.begin(
        Plan {
            target: write_path,
            rename_to: destination.partial.is_some().then_some(destination.target),
            source_size: source.size,
            source_modified: source.modified,
            source_permissions: source.permissions,
            replaced_permissions: None,
        },
        destination.start,
        source.size,
        can_resume,
    );
    receive(job, &files, &mut file).await?;
    Ok(RunOutcome::Completed)
}

async fn upload(
    job: &JobRef<'_>,
    files: Arc<dyn RemoteFileSystem>,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
) -> AppResult<RunOutcome> {
    let spec = job.spec;
    let metadata = tokio::fs::metadata(&spec.source)
        .await
        .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
    if metadata.is_dir() {
        return Err(AppError::invalid(format!("{} is a folder", spec.source)));
    }
    let source_facts = FileFacts {
        size: metadata.len(),
        modified: worker::modified_seconds(&metadata),
    };
    let capabilities = files.capabilities();
    // The bytes before a resume point are compared first, which takes reading them back.
    let can_resume = capabilities.resumable_writes && capabilities.ranged_reads;
    let (target, mut start) = match prepare_remote_target(
        job,
        files.as_ref(),
        source_facts,
        resolution,
        resume,
        can_resume,
    )
    .await?
    {
        Prepared::Write { target, start } => (target, start),
        Prepared::Finished(outcome) => return Ok(outcome),
    };

    let source_permissions = worker::local_permissions(&metadata);
    let mut file = File::open(&spec.source)
        .await
        .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
    if start > 0 {
        let window = worker::check_window(start);
        let theirs = read_remote_range(job, &files, &target, window.start, window.len).await?;
        let ours = worker::read_local_range(&mut file, window.start, window.len).await?;
        if theirs != ours {
            job.log_starting_over();
            start = 0;
        }
    }
    file.seek(SeekFrom::Start(start)).await?;
    let writer = files
        .clone()
        .open_write(WriteRequest {
            path: target.clone(),
            offset: start,
            size: source_facts.size,
            modified: preserved_time(job, source_facts.modified),
            permissions: preserved_permissions(job, source_permissions),
        })
        .await?;
    job.begin(
        Plan {
            target,
            rename_to: None,
            source_size: source_facts.size,
            source_modified: source_facts.modified,
            source_permissions,
            replaced_permissions: None,
        },
        start,
        source_facts.size,
        false,
    );
    send(job, &mut file, writer).await?;
    Ok(RunOutcome::Completed)
}

async fn relay(
    job: &JobRef<'_>,
    endpoints: Endpoints,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
) -> AppResult<RunOutcome> {
    let spec = job.spec;
    let source_files = endpoints.source.ok_or_else(AppError::session_not_found)?;
    let target_files = endpoints.remote;
    let source = remote_file(source_files.as_ref(), &spec.source).await?;
    let target_capabilities = target_files.capabilities();
    let can_resume = source_files.capabilities().ranged_reads
        && target_capabilities.resumable_writes
        && target_capabilities.ranged_reads;
    let (target, mut start) = match prepare_remote_target(
        job,
        target_files.as_ref(),
        facts(&source),
        resolution,
        resume,
        can_resume,
    )
    .await?
    {
        Prepared::Write { target, start } => (target, start),
        Prepared::Finished(outcome) => return Ok(outcome),
    };
    if start > 0 {
        let window = worker::check_window(start);
        let theirs =
            read_remote_range(job, &target_files, &target, window.start, window.len).await?;
        let ours =
            read_remote_range(job, &source_files, &spec.source, window.start, window.len).await?;
        if theirs != ours {
            job.log_starting_over();
            start = 0;
        }
    }
    job.begin(
        Plan {
            target: target.clone(),
            rename_to: None,
            source_size: source.size,
            source_modified: source.modified,
            source_permissions: source.permissions,
            replaced_permissions: None,
        },
        start,
        source.size,
        false,
    );
    if job.settings.fxp
        && try_fxp(
            job,
            source_files.as_ref(),
            target_files.as_ref(),
            &target,
            start,
        )
        .await?
    {
        return Ok(RunOutcome::Completed);
    }

    let reader = source_files
        .clone()
        .open_read(&spec.source, start..source.size)
        .await?;
    let writer = target_files
        .clone()
        .open_write(WriteRequest {
            path: target,
            offset: start,
            size: source.size,
            modified: preserved_time(job, source.modified),
            permissions: preserved_permissions(job, source.permissions),
        })
        .await?;
    pipe(job, SourceStream::new(reader, start), writer).await?;
    Ok(RunOutcome::Completed)
}

/// A worker joining a large download the first worker already opened.
async fn help(job: &JobRef<'_>, files: Arc<dyn RemoteFileSystem>) -> AppResult<RunOutcome> {
    let Some(plan) = job.run.plan.get() else {
        return Ok(RunOutcome::Completed);
    };
    let mut file = OpenOptions::new()
        .write(true)
        .open(&plan.target)
        .await
        .map_err(|error| AppError::from(error).with_path(plan.target.clone()))?;
    receive(job, &files, &mut file).await?;
    Ok(RunOutcome::Completed)
}

/// Checks the result and applies the source's time and permissions where the protocol did
/// not take them while writing.
pub(super) async fn finalize(job: &JobRef<'_>, connections: &mut Connections) -> AppResult<()> {
    let spec = job.spec;
    let Some(plan) = job.run.plan.get().cloned() else {
        return Ok(());
    };
    let files = connections
        .get(job.shared, &spec.session_id, &spec.session_id)
        .await?
        .files
        .clone();
    if spec.direction == Direction::Download {
        let source = files.stat(&spec.source).await?;
        let end = worker::downloaded_end(job, &plan, source)?;
        return worker::complete_download(job.settings, plan, end, true).await;
    }

    let expected = job.run.end().unwrap_or(0);
    // A server that cannot find what it just stored gives nothing to compare.
    if let Some(stored) = files.stat(&plan.target).await? {
        if stored.is_dir || stored.size != expected {
            return Err(job.restart(format!(
                "The server holds {} of the {} sent for {}",
                format_size(stored.size),
                format_size(expected),
                spec.name
            )));
        }
    }
    if source_changed(job, connections, &plan).await? {
        return Err(job.restart(format!(
            "{} changed while it was being {}",
            spec.name,
            if spec.direction == Direction::Upload {
                "uploaded"
            } else {
                "copied"
            }
        )));
    }
    if files.protocol().is_cloud() {
        return Ok(());
    }
    let modified = preserved_time(job, plan.source_modified);
    let permissions = preserved_permissions(job, plan.source_permissions);
    if modified.is_none() && permissions.is_none() {
        return Ok(());
    }
    // Some servers refuse attribute changes; the file itself is complete.
    if let Err(error) = files
        .set_attributes(&plan.target, modified, permissions)
        .await
    {
        job.shared.events.log(
            LogLevel::Warn,
            Some(&spec.session_id),
            format!(
                "Could not set the time or permissions of {}: {}",
                plan.target, error.message
            ),
        );
    }
    Ok(())
}

/// Whether the source of an upload or copy differs from when it started.
async fn source_changed(
    job: &JobRef<'_>,
    connections: &mut Connections,
    plan: &Plan,
) -> AppResult<bool> {
    let spec = job.spec;
    let now = match (&spec.source_session_id, source_key(spec)) {
        (Some(source_id), Some(key)) => {
            let source = connections.get(job.shared, source_id, &key).await?;
            source
                .files
                .stat(&spec.source)
                .await?
                .map(|stat| facts(&stat))
        }
        _ => worker::local_facts(&spec.source)
            .await?
            .map(|(_, facts)| facts),
    };
    Ok(now.is_none_or(|now| now.size != plan.source_size || now.modified != plan.source_modified))
}

enum Prepared {
    Write { target: String, start: u64 },
    Finished(RunOutcome),
}

/// Decides where on the server a file goes, and from which offset, given what is there.
async fn prepare_remote_target(
    job: &JobRef<'_>,
    files: &dyn RemoteFileSystem,
    source: FileFacts,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
    can_resume: bool,
) -> AppResult<Prepared> {
    let spec = job.spec;
    let existing = files.stat(&spec.target).await?;
    let (target, start) = match (resume, existing) {
        (Some(resume), existing) if can_resume && !worker::source_changed(&resume, source) => {
            let on_server = existing.map_or(0, |stat| stat.size);
            (
                spec.target.clone(),
                resume.offset.min(on_server).min(source.size),
            )
        }
        (Some(_), _) => (spec.target.clone(), 0),
        (None, None) => (spec.target.clone(), 0),
        (None, Some(stat)) if stat.is_dir => return Err(worker::folder_in_the_way(spec)),
        (None, Some(stat)) => {
            let target_facts = facts(&stat);
            match decide(job.exists_action(resolution), source, target_facts) {
                Decision::Ask => {
                    return Ok(Prepared::Finished(worker::conflict(source, target_facts)))
                }
                Decision::Skip => {
                    return Ok(Prepared::Finished(RunOutcome::Skipped(
                        "Already exists".into(),
                    )))
                }
                Decision::Rename => {
                    let (target, name) = worker::free_remote_name(files, &spec.target).await?;
                    job.retarget(&target, name);
                    (target, 0)
                }
                Decision::Write { offset } => {
                    (spec.target.clone(), if can_resume { offset } else { 0 })
                }
            }
        }
    };
    Ok(Prepared::Write { target, start })
}

/// Asks two FTP servers to send the file to each other. `false` when they cannot, so the
/// caller copies it through this computer instead.
async fn try_fxp(
    job: &JobRef<'_>,
    source: &dyn RemoteFileSystem,
    target: &dyn RemoteFileSystem,
    target_path: &str,
    start: u64,
) -> AppResult<bool> {
    let (Some(source_ftp), Some(target_ftp)) = (source.as_ftp(), target.as_ftp()) else {
        return Ok(false);
    };
    if !fxp::possible(source_ftp, target_ftp) {
        return Ok(false);
    }
    let spec = job.spec;
    let source_target = job
        .shared
        .target(spec.source_session_id.as_deref().unwrap_or_default())?;
    if source_target
        .fxp_refused
        .lock()
        .unwrap()
        .contains(&spec.session_id)
    {
        return Ok(false);
    }
    job.run.direct.store(true, Ordering::Relaxed);
    let copied = fxp::copy(
        source_ftp,
        &spec.source,
        target_ftp,
        target_path,
        start,
        &job.run.cancel,
    )
    .await;
    if copied.is_err() {
        job.run.direct.store(false, Ordering::Relaxed);
    }
    match copied {
        Ok(()) => {
            while let Some(piece) = job.run.claim_piece() {
                job.run.reach_piece(piece.start, piece.end);
                job.run.complete_piece(piece.start);
            }
            record(job, job.run.end().unwrap_or(start).saturating_sub(start));
            Ok(true)
        }
        Err(error) if error.kind == ErrorKind::Unsupported => {
            let first_refusal = source_target
                .fxp_refused
                .lock()
                .unwrap()
                .insert(spec.session_id.clone());
            if first_refusal {
                job.shared.events.log(
                    LogLevel::Info,
                    Some(&spec.session_id),
                    format!(
                        "{} and {} would not transfer directly ({}); copying through this computer instead",
                        source_ftp.label(),
                        target_ftp.label(),
                        error.message
                    ),
                );
            }
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Claims pieces in turn and writes them to `file` at their offsets. Consecutive pieces are
/// read in one request, so a single worker reads the file in one go.
async fn receive(
    job: &JobRef<'_>,
    files: &Arc<dyn RemoteFileSystem>,
    file: &mut File,
) -> AppResult<()> {
    let spec = job.spec;
    let run = job.run;
    let end_of_file = run.end().unwrap_or(0);
    let limiter = job.shared.limiter_for(spec.direction);
    let mut source: Option<SourceStream> = None;
    let mut file_position = None;
    while let Some(piece) = run.claim_piece() {
        let current = match source.take() {
            Some(open) if open.next == piece.start => open,
            previous => {
                if let Some(previous) = previous {
                    previous.stream.finish().await?;
                }
                let stream = files
                    .clone()
                    .open_read(&spec.source, piece.start..end_of_file)
                    .await?;
                SourceStream::new(stream, piece.start)
            }
        };
        let current = source.insert(current);
        let mut offset = piece.start;
        while offset < piece.end {
            let Some(data) = current.take(piece.end - offset, run).await? else {
                run.end_at(offset);
                break;
            };
            throttle(run, limiter, data.len() as u64).await?;
            if file_position != Some(offset) {
                file.seek(SeekFrom::Start(offset)).await?;
            }
            file.write_all(&data).await?;
            offset += data.len() as u64;
            file_position = Some(offset);
            record(job, data.len() as u64);
            run.reach_piece(piece.start, offset);
        }
        run.complete_piece(piece.start);
    }
    if let Some(open) = source {
        open.stream.finish().await?;
    }
    file.flush().await?;
    Ok(())
}

/// Sends a local file, already at the job's start offset, through `writer`.
async fn send(
    job: &JobRef<'_>,
    file: &mut File,
    mut writer: Box<dyn WriteStream>,
) -> AppResult<()> {
    let run = job.run;
    let limiter = job.shared.limiter_for(Direction::Upload);
    while let Some(piece) = run.claim_piece() {
        let mut offset = piece.start;
        while offset < piece.end {
            let wanted = (piece.end - offset).min(LOCAL_READ_CHUNK as u64) as usize;
            throttle(run, limiter, wanted as u64).await?;
            let data = read_up_to(file, wanted).await?;
            let read = data.len();
            if read > 0 {
                cancellable(run, writer.write(Bytes::from(data))).await?;
                offset += read as u64;
                record(job, read as u64);
                run.reach_piece(piece.start, offset);
            }
            if read < wanted {
                // The file is shorter than when it was queued.
                run.end_at(offset);
                break;
            }
        }
        run.complete_piece(piece.start);
    }
    cancellable(run, writer.finish()).await
}

/// Streams from one server to another.
async fn pipe(
    job: &JobRef<'_>,
    mut source: SourceStream,
    mut writer: Box<dyn WriteStream>,
) -> AppResult<()> {
    let run = job.run;
    let shared = job.shared;
    while let Some(piece) = run.claim_piece() {
        let mut offset = piece.start;
        while offset < piece.end {
            let Some(data) = source.take(piece.end - offset, run).await? else {
                run.end_at(offset);
                break;
            };
            let length = data.len() as u64;
            throttle(run, shared.limiter_for(Direction::Relay), length).await?;
            throttle(run, shared.limiter_for(Direction::Upload), length).await?;
            cancellable(run, writer.write(data)).await?;
            offset += length;
            record(job, length);
            run.reach_piece(piece.start, offset);
        }
        run.complete_piece(piece.start);
    }
    source.stream.finish().await?;
    cancellable(run, writer.finish()).await
}

/// Reads `len` bytes from `offset`, or fewer where the file ends.
async fn read_remote_range(
    job: &JobRef<'_>,
    files: &Arc<dyn RemoteFileSystem>,
    path: &str,
    offset: u64,
    len: u32,
) -> AppResult<Vec<u8>> {
    let wanted = len as usize;
    let mut stream = files
        .clone()
        .open_read(path, offset..offset + u64::from(len))
        .await?;
    let mut data = Vec::with_capacity(wanted);
    while data.len() < wanted {
        match cancellable(job.run, stream.next_chunk()).await? {
            Some(chunk) => data.extend_from_slice(&chunk),
            None => break,
        }
    }
    data.truncate(wanted);
    stream.finish().await?;
    Ok(data)
}

/// A read stream, and what it has delivered beyond the piece being written.
struct SourceStream {
    stream: Box<dyn ReadStream>,
    /// The offset of the next byte `take` returns.
    next: u64,
    leftover: Option<Bytes>,
}

impl SourceStream {
    fn new(stream: Box<dyn ReadStream>, start: u64) -> Self {
        Self {
            stream,
            next: start,
            leftover: None,
        }
    }

    /// Up to `limit` bytes; `None` once the source has ended.
    async fn take(&mut self, limit: u64, run: &JobRun) -> AppResult<Option<Bytes>> {
        let mut data = loop {
            let data = match self.leftover.take() {
                Some(data) => data,
                None => match cancellable(run, self.stream.next_chunk()).await? {
                    Some(data) => data,
                    None => return Ok(None),
                },
            };
            if !data.is_empty() {
                break data;
            }
        };
        if data.len() as u64 > limit {
            self.leftover = Some(data.split_off(limit as usize));
        }
        self.next += data.len() as u64;
        Ok(Some(data))
    }
}

async fn remote_file(files: &dyn RemoteFileSystem, path: &str) -> AppResult<RemoteStat> {
    let stat = files.stat(path).await?.ok_or_else(|| {
        AppError::new(
            ErrorKind::NotFound,
            format!("{} no longer exists", remote_path::file_name(path)),
        )
        .with_path(path)
    })?;
    if stat.is_dir {
        return Err(AppError::invalid(format!("{path} is a folder")));
    }
    Ok(stat)
}

fn facts(stat: &RemoteStat) -> FileFacts {
    FileFacts {
        size: stat.size,
        modified: stat.modified,
    }
}

fn preserved_time(job: &JobRef<'_>, modified: Option<i64>) -> Option<i64> {
    job.settings
        .preserve_timestamps
        .then_some(modified)
        .flatten()
}

fn preserved_permissions(job: &JobRef<'_>, permissions: Option<u32>) -> Option<u32> {
    job.settings
        .preserve_permissions
        .then_some(permissions)
        .flatten()
}

fn record(job: &JobRef<'_>, bytes: u64) {
    job.run.add_progress(bytes);
    job.shared
        .total_for(job.spec.direction)
        .fetch_add(bytes, Ordering::Relaxed);
}

async fn throttle(run: &JobRun, limiter: &RateLimiter, bytes: u64) -> AppResult<()> {
    cancellable(run, async {
        limiter.acquire(bytes).await;
        Ok(())
    })
    .await
}

async fn cancellable<T>(run: &JobRun, work: impl Future<Output = AppResult<T>>) -> AppResult<T> {
    tokio::select! {
        biased;
        _ = run.cancel.cancelled() => Err(AppError::cancelled()),
        result = work => result,
    }
}
