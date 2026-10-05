use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use tokio::fs::{File, OpenOptions};

use super::conflict::{decide, numbered_name, Decision, ExistsAction, FileFacts};
use super::copy::{self, CopyContext};
use super::pieces::Pieces;
use super::queue::{
    Claim, ConflictInfo, Direction, JobId, JobKind, JobRun, JobSpec, JobState, Plan, Release, Role,
    RunOutcome,
};
use super::{SessionTarget, Shared};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::format::format_size;
use crate::model::{EntryKind, LinkTarget};
use crate::settings::TransferSettings;
use crate::sftp::RemoteFs;
use crate::ssh::{self, HostKeyApproval, SshHandle};
use crate::{local, remote_path, session};

const IDLE_POLL: Duration = Duration::from_secs(1);
const IDLE_DISCONNECT: Duration = Duration::from_secs(30);
const MEBIBYTE: u64 = 1024 * 1024;
/// Pieces for a file one worker moves alone: small enough that a resume repeats little.
const SINGLE_WORKER_PIECE: u64 = 8 * MEBIBYTE;
const MIN_SEGMENT_PIECE: u64 = 4 * MEBIBYTE;
const MAX_SEGMENT_PIECE: u64 = 64 * MEBIBYTE;
const MAX_RENAME_ATTEMPTS: u32 = 9999;

struct WorkerConnection {
    fs: RemoteFs,
    /// `None` for a channel on the browsing connection, which is not this worker's to close.
    handle: Option<SshHandle>,
}

impl WorkerConnection {
    async fn close(self) {
        self.fs.close();
        if let Some(handle) = self.handle {
            ssh::disconnect(&handle).await;
        }
    }
}

struct Connections {
    worker_index: usize,
    open: HashMap<String, WorkerConnection>,
    last_used: Instant,
}

impl Connections {
    async fn get(&mut self, shared: &Shared, session_id: &str) -> AppResult<&RemoteFs> {
        self.last_used = Instant::now();
        let closed = self.open.get(session_id).is_some_and(|connection| {
            connection
                .handle
                .as_ref()
                .is_some_and(|handle| handle.is_closed())
        });
        if closed {
            self.drop_connection(session_id).await;
        }
        if !self.open.contains_key(session_id) {
            let target = shared.target(session_id)?;
            let connection = open_connection(shared, &target, self.worker_index).await?;
            self.open.insert(session_id.to_string(), connection);
        }
        Ok(&self.open[session_id].fs)
    }

    fn existing(&self, session_id: &str) -> Option<&RemoteFs> {
        self.open.get(session_id).map(|connection| &connection.fs)
    }

    async fn drop_connection(&mut self, session_id: &str) {
        if let Some(connection) = self.open.remove(session_id) {
            connection.close().await;
        }
    }

    async fn close_all(&mut self) {
        for (_, connection) in self.open.drain() {
            connection.close().await;
        }
    }
}

async fn open_connection(
    shared: &Shared,
    target: &SessionTarget,
    worker_index: usize,
) -> AppResult<WorkerConnection> {
    let number = worker_index + 1;
    if shared.settings().separate_connections && !target.channels_only.load(Ordering::Relaxed) {
        let approval = (!target.host_key_fingerprint.is_empty()).then(|| HostKeyApproval {
            fingerprint: target.host_key_fingerprint.clone(),
            remember: false,
        });
        let connection_id = format!("{}#{number}", target.session_id);
        // Workers log one line each, not every handshake step.
        let quiet = Events::default();
        let refusal = match ssh::connect(
            &connection_id,
            &target.profile,
            &shared.sessions.known_hosts,
            approval,
            &quiet,
        )
        .await
        {
            Ok(connection) => match session::open_sftp(&connection.handle).await {
                Ok(fs) => {
                    shared.events.log(
                        LogLevel::Info,
                        Some(&target.session_id),
                        format!("Transfer connection {number} to {} opened", target.label),
                    );
                    return Ok(WorkerConnection {
                        fs,
                        handle: Some(connection.handle),
                    });
                }
                Err(error) => {
                    ssh::disconnect(&connection.handle).await;
                    error
                }
            },
            Err(error) if error.kind == ErrorKind::HostKeyChanged => return Err(error),
            Err(error) => error,
        };
        if !target.channels_only.swap(true, Ordering::Relaxed) {
            shared.events.log(
                LogLevel::Warn,
                Some(&target.session_id),
                format!(
                    "The server did not accept another connection ({}); transfers share the browsing connection",
                    refusal.message
                ),
            );
        }
    }
    let session = shared.sessions.get(&target.session_id).await?;
    let fs = session.open_channel().await?;
    Ok(WorkerConnection { fs, handle: None })
}

pub(super) async fn run(shared: Arc<Shared>, index: usize) {
    let mut connections = Connections {
        worker_index: index,
        open: HashMap::new(),
        last_used: Instant::now(),
    };
    loop {
        {
            let mut live = shared.live_workers.lock().unwrap();
            let stopping = shared.stopping.load(Ordering::Relaxed);
            if stopping || index >= shared.settings.read().unwrap().workers as usize {
                live.remove(&index);
                break;
            }
        }
        let notified = shared.work.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let claim = shared.queue.lock().unwrap().claim(Instant::now());
        match claim {
            Some(claim) => execute(&shared, &mut connections, claim).await,
            None => {
                let _ = tokio::time::timeout(IDLE_POLL, notified).await;
                if !connections.open.is_empty() && connections.last_used.elapsed() > IDLE_DISCONNECT
                {
                    connections.close_all().await;
                }
            }
        }
    }
    connections.close_all().await;
}

struct JobContext<'a> {
    shared: &'a Shared,
    fs: &'a RemoteFs,
    id: JobId,
    run: &'a JobRun,
    spec: &'a JobSpec,
    settings: &'a TransferSettings,
}

impl JobContext<'_> {
    fn copy_context(&self) -> CopyContext<'_> {
        let direction = self.spec.direction;
        CopyContext {
            fs: self.fs,
            run: self.run,
            limiter: self.shared.limiter_for(direction),
            total: self.shared.total_for(direction),
            request_size: self.settings.request_size(),
            requests_in_flight: self.settings.requests_in_flight as usize,
        }
    }

    fn exists_action(&self, resolution: Option<ExistsAction>) -> ExistsAction {
        resolution
            .or(self.shared.queue.lock().unwrap().conflict_override)
            .unwrap_or(self.settings.exists_action)
    }

    fn retarget(&self, target: &str, name: String) {
        self.shared
            .queue
            .lock()
            .unwrap()
            .retarget(self.id, target.to_string(), name);
    }

    /// Records where the bytes go and lets idle workers join if the file is large enough.
    fn begin(&self, plan: Plan, start: u64, end: u64) {
        let _ = self.run.plan.set(plan);
        let remaining = end.saturating_sub(start);
        let segmented = self.settings.segmented && remaining >= self.settings.segment_threshold();
        let piece_size = if segmented {
            (remaining / (u64::from(self.settings.max_segments) * 4))
                .clamp(MIN_SEGMENT_PIECE, MAX_SEGMENT_PIECE)
        } else {
            SINGLE_WORKER_PIECE
        };
        self.run.transferred.store(start, Ordering::Relaxed);
        self.run.start_pieces(Pieces::new(start, end, piece_size));
        if segmented {
            self.shared
                .queue
                .lock()
                .unwrap()
                .open_segments(self.id, self.settings.max_segments);
            self.shared.work.notify_waiters();
        }
    }
}

async fn execute(shared: &Shared, connections: &mut Connections, claim: Claim) {
    let settings = shared.settings();
    let Claim {
        id,
        role,
        run,
        spec,
        resolution,
        resume_offset,
    } = claim;

    let outcome = match connections.get(shared, &spec.session_id).await {
        Err(error) => RunOutcome::Failed(error),
        Ok(fs) => {
            let context = JobContext {
                shared,
                fs,
                id,
                run: &run,
                spec: &spec,
                settings: &settings,
            };
            let result = match (spec.kind, role) {
                (JobKind::Folder, _) => expand_folder(&context).await,
                (JobKind::File, Role::Primary) => match spec.direction {
                    Direction::Download => download(&context, resolution, resume_offset).await,
                    Direction::Upload => upload(&context, resolution, resume_offset).await,
                },
                (JobKind::File, Role::Helper) => help(&context).await,
            };
            result.unwrap_or_else(|error| {
                if error.kind == ErrorKind::Cancelled {
                    RunOutcome::Stopped
                } else {
                    RunOutcome::Failed(error)
                }
            })
        }
    };
    if let RunOutcome::Failed(error) = &outcome {
        if error.is_connection_lost() {
            connections.drop_connection(&spec.session_id).await;
        }
    }

    let release = shared.queue.lock().unwrap().release(id, &outcome);
    let outcome = match release {
        Release::Others => {
            shared.work.notify_waiters();
            return;
        }
        Release::Finalize => {
            let finalized = match spec.direction {
                Direction::Download => finalize_download(&run, &settings).await,
                Direction::Upload => match connections.get(shared, &spec.session_id).await {
                    Ok(fs) => finalize_upload(shared, fs, &run, &spec, &settings).await,
                    Err(error) => Err(error),
                },
            };
            match finalized {
                Ok(()) => RunOutcome::Completed,
                Err(error) => RunOutcome::Failed(error),
            }
        }
        Release::Settle => {
            trim_partial(connections.existing(&spec.session_id), &run, &spec).await;
            outcome
        }
    };

    let settled = {
        let mut queue = shared.queue.lock().unwrap();
        queue.settle(id, outcome, &settings, Instant::now());
        queue
            .job(id)
            .map(|job| (job.state, job.error.clone(), job.spec.clone()))
    };
    if let Some((state, error, spec)) = settled {
        log_result(shared, &settings, state, error, &spec);
    }
    shared.work.notify_waiters();
}

fn log_result(
    shared: &Shared,
    settings: &TransferSettings,
    state: JobState,
    error: Option<String>,
    spec: &JobSpec,
) {
    let verb = match spec.direction {
        Direction::Upload => "Upload",
        Direction::Download => "Download",
    };
    let session = Some(spec.session_id.as_str());
    match state {
        JobState::Failed => shared.events.log(
            LogLevel::Error,
            session,
            format!(
                "{verb} of {} failed: {}",
                spec.source,
                error.unwrap_or_default()
            ),
        ),
        JobState::Queued if error.is_some() => shared.events.log(
            LogLevel::Warn,
            session,
            format!(
                "{verb} of {} interrupted: {}",
                spec.source,
                error.unwrap_or_default()
            ),
        ),
        JobState::Done if settings.log_each_file && spec.kind == JobKind::File => {
            shared.events.log(
                LogLevel::Info,
                session,
                format!(
                    "{verb}ed {} to {} ({})",
                    spec.source,
                    spec.target,
                    format_size(spec.size)
                ),
            )
        }
        JobState::Skipped if settings.log_each_file => shared.events.log(
            LogLevel::Info,
            session,
            format!("Skipped {}: {}", spec.source, error.unwrap_or_default()),
        ),
        _ => {}
    }
}

async fn expand_folder(context: &JobContext<'_>) -> AppResult<RunOutcome> {
    let spec = context.spec;
    let listing = match spec.direction {
        Direction::Download => context.fs.list_dir(&spec.source).await?,
        Direction::Upload => {
            let source = spec.source.clone();
            tokio::task::spawn_blocking(move || local::list_dir(&source)).await??
        }
    };
    if spec.ancestors.contains(&listing.path) {
        return Ok(RunOutcome::Skipped(format!(
            "{} links back to a folder above it",
            spec.source
        )));
    }
    match spec.direction {
        Direction::Download => {
            let target = spec.target.clone();
            tokio::task::spawn_blocking(move || std::fs::create_dir_all(&target))
                .await?
                .map_err(|error| AppError::from(error).with_path(spec.target.clone()))?;
        }
        Direction::Upload => context.fs.ensure_dir(&spec.target).await?,
    }

    let mut ancestors = spec.ancestors.to_vec();
    ancestors.push(listing.path.clone());
    let ancestors: Arc<[String]> = ancestors.into();
    let mut entries = listing.entries;
    entries.sort_by(|left, right| {
        left.is_dir_like()
            .cmp(&right.is_dir_like())
            .then_with(|| left.name.cmp(&right.name))
    });

    let mut skipped = 0;
    let mut children = Vec::with_capacity(entries.len());
    for entry in entries {
        let is_dir = entry.is_dir_like();
        let is_file = entry.kind == EntryKind::File || entry.link_target == Some(LinkTarget::File);
        // A server could send names that climb out of the target folder.
        let valid_name = local::validate_name(&entry.name).is_ok() && !entry.name.contains('/');
        if !(is_dir || is_file) || !valid_name {
            skipped += 1;
            continue;
        }
        let target = match spec.direction {
            Direction::Upload => remote_path::join(&spec.target, &entry.name),
            Direction::Download => Path::new(&spec.target)
                .join(&entry.name)
                .to_string_lossy()
                .into_owned(),
        };
        children.push(JobSpec {
            session_id: spec.session_id.clone(),
            direction: spec.direction,
            kind: if is_dir {
                JobKind::Folder
            } else {
                JobKind::File
            },
            source: entry.path,
            target,
            target_directory: spec.target.clone(),
            size: if is_dir { 0 } else { entry.size },
            ancestors: ancestors.clone(),
            name: entry.name,
        });
    }
    if skipped > 0 {
        context.shared.events.log(
            LogLevel::Warn,
            Some(&spec.session_id),
            format!(
                "Skipped {skipped} {} in {} that {} not regular files or folders",
                if skipped == 1 { "item" } else { "items" },
                spec.source,
                if skipped == 1 { "is" } else { "are" },
            ),
        );
    }
    Ok(RunOutcome::Expanded(children))
}

async fn download(
    context: &JobContext<'_>,
    resolution: Option<ExistsAction>,
    resume_offset: Option<u64>,
) -> AppResult<RunOutcome> {
    let spec = context.spec;
    let source = context.fs.stat(&spec.source).await?.ok_or_else(|| {
        AppError::new(
            ErrorKind::NotFound,
            format!("{} no longer exists", spec.source),
        )
        .with_path(spec.source.clone())
    })?;
    if source.is_dir {
        return Err(AppError::invalid(format!("{} is a folder", spec.source)));
    }
    let source_facts = FileFacts {
        size: source.size,
        modified: source.modified,
    };

    let existing = local_facts(&spec.target).await?;
    let (target, start) = match (resume_offset, existing) {
        (Some(offset), existing) => {
            let on_disk = existing.map_or(0, |(_, facts)| facts.size);
            (spec.target.clone(), offset.min(on_disk).min(source.size))
        }
        (None, None) => (spec.target.clone(), 0),
        (None, Some((true, _))) => return Err(folder_in_the_way(spec)),
        (None, Some((false, target_facts))) => {
            match decide(
                context.exists_action(resolution),
                source_facts,
                target_facts,
            ) {
                Decision::Ask => return Ok(conflict(source_facts, target_facts)),
                Decision::Skip => return Ok(RunOutcome::Skipped("Already exists".into())),
                Decision::Rename => {
                    let (target, name) = free_local_name(&spec.target).await?;
                    context.retarget(&target, name);
                    (target, 0)
                }
                Decision::Write { offset } => (spec.target.clone(), offset),
            }
        }
    };

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(start == 0)
        .open(&target)
        .await
        .map_err(|error| AppError::from(error).with_path(target.clone()))?;
    context.begin(
        Plan {
            target,
            source_modified: source.modified,
            source_permissions: source.permissions,
            resumed: start > 0,
        },
        start,
        source.size,
    );
    let handle = context.fs.open_for_read(&spec.source).await?;
    let copied = copy::download(&context.copy_context(), &handle, &mut file).await;
    let _ = context.fs.close_handle(handle).await;
    copied?;
    Ok(RunOutcome::Completed)
}

async fn upload(
    context: &JobContext<'_>,
    resolution: Option<ExistsAction>,
    resume_offset: Option<u64>,
) -> AppResult<RunOutcome> {
    let spec = context.spec;
    let metadata = tokio::fs::metadata(&spec.source)
        .await
        .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
    if metadata.is_dir() {
        return Err(AppError::invalid(format!("{} is a folder", spec.source)));
    }
    let source_facts = FileFacts {
        size: metadata.len(),
        modified: modified_seconds(&metadata),
    };

    let existing = context.fs.stat(&spec.target).await?;
    let (target, start) = match (resume_offset, existing) {
        (Some(offset), existing) => {
            let on_server = existing.map_or(0, |stat| stat.size);
            (
                spec.target.clone(),
                offset.min(on_server).min(source_facts.size),
            )
        }
        (None, None) => (spec.target.clone(), 0),
        (None, Some(stat)) if stat.is_dir => return Err(folder_in_the_way(spec)),
        (None, Some(stat)) => {
            let target_facts = FileFacts {
                size: stat.size,
                modified: stat.modified,
            };
            match decide(
                context.exists_action(resolution),
                source_facts,
                target_facts,
            ) {
                Decision::Ask => return Ok(conflict(source_facts, target_facts)),
                Decision::Skip => return Ok(RunOutcome::Skipped("Already exists".into())),
                Decision::Rename => {
                    let (target, name) = free_remote_name(context.fs, &spec.target).await?;
                    context.retarget(&target, name);
                    (target, 0)
                }
                Decision::Write { offset } => (spec.target.clone(), offset),
            }
        }
    };

    let source_permissions = local_permissions(&metadata);
    let permissions = context
        .settings
        .preserve_permissions
        .then_some(source_permissions)
        .flatten();
    let mut file = File::open(&spec.source)
        .await
        .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
    let handle = context
        .fs
        .open_for_write(&target, start == 0, permissions)
        .await?;
    context.begin(
        Plan {
            target,
            source_modified: source_facts.modified,
            source_permissions,
            resumed: start > 0,
        },
        start,
        source_facts.size,
    );
    let copied = copy::upload(&context.copy_context(), &mut file, &handle).await;
    let closed = context.fs.close_handle(handle).await;
    copied?;
    closed?;
    Ok(RunOutcome::Completed)
}

/// A worker joining a large file the first worker already opened.
async fn help(context: &JobContext<'_>) -> AppResult<RunOutcome> {
    let Some(plan) = context.run.plan.get() else {
        return Ok(RunOutcome::Completed);
    };
    let spec = context.spec;
    match spec.direction {
        Direction::Download => {
            let mut file = OpenOptions::new()
                .write(true)
                .open(&plan.target)
                .await
                .map_err(|error| AppError::from(error).with_path(plan.target.clone()))?;
            let handle = context.fs.open_for_read(&spec.source).await?;
            let copied = copy::download(&context.copy_context(), &handle, &mut file).await;
            let _ = context.fs.close_handle(handle).await;
            copied?;
        }
        Direction::Upload => {
            let mut file = File::open(&spec.source)
                .await
                .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
            let handle = context.fs.open_for_write(&plan.target, false, None).await?;
            let copied = copy::upload(&context.copy_context(), &mut file, &handle).await;
            let closed = context.fs.close_handle(handle).await;
            copied?;
            closed?;
        }
    }
    Ok(RunOutcome::Completed)
}

async fn finalize_download(run: &JobRun, settings: &TransferSettings) -> AppResult<()> {
    let Some(plan) = run.plan.get().cloned() else {
        return Ok(());
    };
    let end = run.end().unwrap_or(0);
    let modified = settings
        .preserve_timestamps
        .then_some(plan.source_modified)
        .flatten();
    let permissions = settings
        .preserve_permissions
        .then_some(plan.source_permissions)
        .flatten();
    let target = plan.target.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new().write(true).open(&target)?;
        // Workers write pieces out of order, and a resumed file may be longer than its source.
        file.set_len(end)?;
        if let Some(seconds) = modified.and_then(|seconds| u64::try_from(seconds).ok()) {
            file.set_modified(UNIX_EPOCH + Duration::from_secs(seconds))?;
        }
        set_local_permissions(&file, permissions)
    })
    .await?
    .map_err(|error| AppError::from(error).with_path(plan.target))
}

async fn finalize_upload(
    shared: &Shared,
    fs: &RemoteFs,
    run: &JobRun,
    spec: &JobSpec,
    settings: &TransferSettings,
) -> AppResult<()> {
    let Some(plan) = run.plan.get() else {
        return Ok(());
    };
    if plan.resumed {
        let end = run.end().unwrap_or(0);
        if fs
            .stat(&plan.target)
            .await?
            .is_some_and(|stat| stat.size > end)
        {
            fs.truncate(&plan.target, end).await?;
        }
    }
    let modified = settings
        .preserve_timestamps
        .then_some(plan.source_modified)
        .flatten();
    let permissions = settings
        .preserve_permissions
        .then_some(plan.source_permissions)
        .flatten();
    // Some servers refuse attribute changes; the file itself is complete.
    if let Err(error) = fs.set_attributes(&plan.target, modified, permissions).await {
        shared.events.log(
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

/// Cuts an interrupted target back to its complete prefix, so a later resume by size is safe
/// even though workers wrote pieces out of order.
async fn trim_partial(fs: Option<&RemoteFs>, run: &JobRun, spec: &JobSpec) {
    let (Some(plan), Some(prefix)) = (run.plan.get(), run.complete_prefix()) else {
        return;
    };
    match spec.direction {
        Direction::Download => {
            let target = plan.target.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let file = std::fs::OpenOptions::new().write(true).open(&target)?;
                if file.metadata()?.len() > prefix {
                    file.set_len(prefix)?;
                }
                std::io::Result::Ok(())
            })
            .await;
        }
        Direction::Upload => {
            if let Some(fs) = fs {
                let _ = fs.truncate(&plan.target, prefix).await;
            }
        }
    }
}

fn conflict(source: FileFacts, target: FileFacts) -> RunOutcome {
    RunOutcome::Conflict(ConflictInfo {
        source_size: source.size,
        source_modified: source.modified,
        target_size: target.size,
        target_modified: target.modified,
    })
}

fn folder_in_the_way(spec: &JobSpec) -> AppError {
    AppError::new(
        ErrorKind::AlreadyExists,
        format!("A folder named {} is in the way", spec.name),
    )
    .with_path(spec.target.clone())
}

/// `None` when nothing is there; otherwise whether it is a folder, and its size and time.
async fn local_facts(path: &str) -> AppResult<Option<(bool, FileFacts)>> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(Some((
            metadata.is_dir(),
            FileFacts {
                size: metadata.len(),
                modified: modified_seconds(&metadata),
            },
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::from(error).with_path(path)),
    }
}

fn modified_seconds(metadata: &std::fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since_epoch| since_epoch.as_secs() as i64)
}

#[cfg(unix)]
fn local_permissions(metadata: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn local_permissions(_metadata: &std::fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_local_permissions(file: &std::fs::File, mode: Option<u32>) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match mode {
        Some(mode) => file.set_permissions(std::fs::Permissions::from_mode(mode)),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn set_local_permissions(_file: &std::fs::File, _mode: Option<u32>) -> std::io::Result<()> {
    Ok(())
}

async fn free_local_name(target: &str) -> AppResult<(String, String)> {
    let path = Path::new(target);
    let parent = path.parent().unwrap_or(Path::new(""));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    for number in 1..=MAX_RENAME_ATTEMPTS {
        let candidate_name = numbered_name(&name, number);
        let candidate = parent.join(&candidate_name);
        if !tokio::fs::try_exists(&candidate).await? {
            return Ok((candidate.to_string_lossy().into_owned(), candidate_name));
        }
    }
    Err(no_free_name(target))
}

async fn free_remote_name(fs: &RemoteFs, target: &str) -> AppResult<(String, String)> {
    let parent = remote_path::parent(target).unwrap_or_else(|| "/".into());
    let name = remote_path::file_name(target);
    for number in 1..=MAX_RENAME_ATTEMPTS {
        let candidate_name = numbered_name(name, number);
        let candidate = remote_path::join(&parent, &candidate_name);
        if fs.stat(&candidate).await?.is_none() {
            return Ok((candidate, candidate_name));
        }
    }
    Err(no_free_name(target))
}

fn no_free_name(target: &str) -> AppError {
    AppError::new(
        ErrorKind::AlreadyExists,
        format!("No free name next to {target}"),
    )
    .with_path(target)
}
