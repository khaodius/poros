use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use russh::client::Msg;
use russh::Channel;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::conflict::{decide, numbered_name, Decision, ExistsAction, FileFacts};
use super::copy::{self, CopyContext};
use super::delta;
use super::pieces::Pieces;
use super::queue::{
    Abandoned, Claim, ConflictInfo, Direction, JobId, JobKind, JobRun, JobSpec, JobState, Plan,
    Release, ResumePoint, Role, RunOutcome, ServerChange,
};
use super::streamed;
use super::verify::{self, Checked};
use super::{Login, SessionTarget, Shared};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::format::format_size;
use crate::ftp::FtpFs;
use crate::model::{EntryKind, LinkTarget};
use crate::protocol::{Protocol, RemoteFileSystem};
use crate::settings::TransferSettings;
use crate::sftp::{RemoteFs, RemoteStat};
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
/// Added to a file's name, after a leading dot, while it is being written.
const PARTIAL_SUFFIX: &str = ".poros-part";
/// The longest file name most file systems take, in bytes.
const MAX_NAME_BYTES: usize = 255;
/// Bytes compared on both sides before an interrupted file continues.
const RESUME_CHECK_BYTES: u64 = 64 * 1024;

pub(super) struct WorkerConnection {
    pub files: Arc<dyn RemoteFileSystem>,
    /// The SFTP channel behind `files`, for the pipelined transfer path.
    pub sftp: Option<Arc<RemoteFs>>,
    /// This worker's own SSH connection; `None` for a channel on the browsing connection.
    pub handle: Option<SshHandle>,
    /// The worker opened `files` itself, as with an FTP connection, and so closes it.
    owns_files: bool,
}

impl WorkerConnection {
    fn sftp(fs: RemoteFs, handle: Option<SshHandle>) -> Self {
        let fs = Arc::new(fs);
        Self {
            files: fs.clone(),
            sftp: Some(fs),
            handle,
            owns_files: false,
        }
    }

    fn files(files: Arc<dyn RemoteFileSystem>, owns_files: bool) -> Self {
        Self {
            files,
            sftp: None,
            handle: None,
            owns_files,
        }
    }

    /// An FTP connection borrowed from the browsing session, which other workers may be
    /// waiting for.
    pub fn borrows_ftp_connection(&self) -> bool {
        !self.owns_files && self.files.protocol().is_ftp()
    }

    async fn close(self) {
        if let Some(fs) = &self.sftp {
            fs.close();
        }
        if let Some(handle) = self.handle {
            ssh::disconnect(&handle).await;
        }
        if self.owns_files {
            self.files.close().await;
        }
    }
}

pub(super) struct Connections {
    worker_index: usize,
    open: HashMap<String, WorkerConnection>,
    last_used: Instant,
}

impl Connections {
    /// This worker's connection to a session's server. `key` tells apart a second connection
    /// to the same server, for copies within it.
    pub async fn get(
        &mut self,
        shared: &Shared,
        session_id: &str,
        key: &str,
    ) -> AppResult<&WorkerConnection> {
        self.last_used = Instant::now();
        let closed = self.open.get(key).is_some_and(|connection| {
            connection
                .handle
                .as_ref()
                .is_some_and(|handle| handle.is_closed())
        });
        if closed {
            self.drop_connection(key).await;
        }
        if !self.open.contains_key(key) {
            let target = shared.target(session_id)?;
            target.adopt_live_session(&shared.sessions).await;
            let connection = open_connection(shared, &target, self.worker_index).await?;
            self.open.insert(key.to_string(), connection);
        }
        Ok(&self.open[key])
    }

    fn existing(&self, key: &str) -> Option<&RemoteFs> {
        self.open
            .get(key)
            .and_then(|connection| connection.sftp.as_deref())
    }

    pub async fn drop_connection(&mut self, key: &str) {
        if let Some(connection) = self.open.remove(key) {
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
    let login = target.login();
    let protocol = login.profile.protocol;
    if protocol.is_cloud() {
        // Cloud APIs take any number of requests over the browsing session's HTTP client.
        return match login.browsing_files {
            Some(files) => Ok(WorkerConnection::files(files, false)),
            None if target.is_restored() => Err(connect_first(target)),
            None => Err(AppError::session_not_found()),
        };
    }
    let browsing = shared.sessions.get(&login.browsing_session).await.ok();
    let separate =
        shared.settings().separate_connections && !target.channels_only.load(Ordering::Relaxed);
    // Without a browsing connection to share, a connection of its own is the only way.
    if separate || browsing.is_none() {
        let refusal = match connect_directly(shared, target, &login, worker_index + 1).await {
            Ok(connection) => return Ok(connection),
            Err(error) if error.kind == ErrorKind::HostKeyChanged => return Err(error),
            Err(error) if target.is_restored() && needs_login(&error) => {
                return Err(connect_first(target))
            }
            Err(error) => error,
        };
        // With nothing else connected, the server is out of reach rather than refusing more.
        if browsing.is_none() {
            return Err(refusal);
        }
        if separate && !target.channels_only.swap(true, Ordering::Relaxed) {
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
    let session = browsing.ok_or_else(AppError::session_not_found)?;
    match protocol {
        Protocol::Sftp => Ok(WorkerConnection::sftp(session.open_channel().await?, None)),
        _ => Ok(WorkerConnection::files(session.files(), false)),
    }
}

async fn connect_directly(
    shared: &Shared,
    target: &SessionTarget,
    login: &Login,
    number: usize,
) -> AppResult<WorkerConnection> {
    let approval = (!login.host_key_fingerprint.is_empty()).then(|| HostKeyApproval {
        fingerprint: login.host_key_fingerprint.clone(),
        remember: false,
    });
    let connection_id = format!("{}#{number}", target.session_id);
    // Workers log one line each, not every handshake step.
    let quiet = Events::default();
    let connection = if login.profile.protocol.is_ftp() {
        let (files, _) = FtpFs::connect(
            &connection_id,
            &login.profile,
            &shared.sessions.certificates,
            approval,
            &quiet,
        )
        .await?;
        WorkerConnection::files(files, true)
    } else {
        let connection = ssh::connect(
            &connection_id,
            &login.profile,
            &shared.sessions.known_hosts,
            approval,
            &quiet,
        )
        .await?;
        match session::open_sftp(&connection.handle).await {
            Ok(fs) => WorkerConnection::sftp(fs, Some(connection.handle)),
            Err(error) => {
                ssh::disconnect(&connection.handle).await;
                return Err(error);
            }
        }
    };
    shared.events.log(
        LogLevel::Info,
        Some(&target.session_id),
        format!("Transfer connection {number} to {} opened", target.label),
    );
    Ok(connection)
}

fn connect_first(target: &SessionTarget) -> AppError {
    AppError::new(
        ErrorKind::AuthFailed,
        format!(
            "Connect to {} to continue the transfers saved from last time",
            target.label
        ),
    )
}

/// Errors a saved login without its password or a confirmed host key runs into.
fn needs_login(error: &AppError) -> bool {
    matches!(
        error.kind,
        ErrorKind::AuthFailed | ErrorKind::PassphraseRequired | ErrorKind::HostKeyUnknown
    )
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

/// The parts of a running job every transfer path needs.
pub(super) struct JobRef<'a> {
    pub shared: &'a Shared,
    pub id: JobId,
    pub run: &'a JobRun,
    pub spec: &'a JobSpec,
    pub settings: &'a TransferSettings,
}

impl JobRef<'_> {
    pub fn exists_action(&self, resolution: Option<ExistsAction>) -> ExistsAction {
        resolution
            .or(self.shared.queue.lock().unwrap().conflict_override)
            .unwrap_or(self.settings.exists_action)
    }

    /// Marks what was written as untrustworthy, so the next attempt starts from the beginning.
    pub fn restart(&self, message: String) -> AppError {
        self.run.restart.store(true, Ordering::Relaxed);
        AppError::integrity(message)
    }

    pub fn retarget(&self, target: &str, name: String) {
        self.shared
            .queue
            .lock()
            .unwrap()
            .retarget(self.id, target.to_string(), name);
    }

    /// Records where the bytes go and, when the file can be split and is large enough, lets
    /// idle workers join.
    pub fn begin(&self, plan: Plan, start: u64, end: u64, splittable: bool) {
        let _ = self.run.plan.set(plan);
        let remaining = end.saturating_sub(start);
        let segmented =
            splittable && self.settings.segmented && remaining >= self.settings.segment_threshold();
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

    pub fn log_starting_over(&self) {
        self.shared.events.log(
            LogLevel::Warn,
            Some(&self.spec.session_id),
            format!(
                "The part of {} already transferred does not match the source; starting it over",
                self.spec.name
            ),
        );
    }
}

pub(super) struct JobContext<'a> {
    pub shared: &'a Shared,
    pub fs: &'a RemoteFs,
    /// This worker's own connection; `None` when it borrows a channel of the browsing one.
    pub handle: Option<&'a SshHandle>,
    id: JobId,
    pub run: &'a JobRun,
    pub spec: &'a JobSpec,
    pub settings: &'a TransferSettings,
}

impl JobContext<'_> {
    fn job(&self) -> JobRef<'_> {
        JobRef {
            shared: self.shared,
            id: self.id,
            run: self.run,
            spec: self.spec,
            settings: self.settings,
        }
    }

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
        self.job().exists_action(resolution)
    }

    /// A channel for running a command on the server: on this worker's connection, or on the
    /// browsing one when the worker borrows its channels.
    pub async fn command_channel(&self) -> AppResult<Channel<Msg>> {
        match self.handle {
            Some(handle) => Ok(handle.channel_open_session().await?),
            None => {
                let target = self.shared.target(&self.spec.session_id)?;
                let session = self
                    .shared
                    .sessions
                    .get(&target.login().browsing_session)
                    .await?;
                session.open_command_channel().await
            }
        }
    }

    fn restart(&self, message: String) -> AppError {
        self.job().restart(message)
    }

    fn retarget(&self, target: &str, name: String) {
        self.job().retarget(target, name);
    }

    /// What the first worker read as it closed, worth keeping only if no other worker joined.
    fn keep_closing_stat(&self, stat: Option<RemoteStat>) {
        if let Some(stat) = stat.filter(|_| !self.run.is_segmented()) {
            let _ = self.run.closing_stat.set(stat);
        }
    }

    fn begin(&self, plan: Plan, start: u64, end: u64) {
        self.job().begin(plan, start, end, true);
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
        resume,
    } = claim;
    let job = JobRef {
        shared,
        id,
        run: &run,
        spec: &spec,
        settings: &settings,
    };

    let streamed = streamed::applies(shared, &spec);
    let result = if streamed {
        streamed::run(&job, connections, role, resolution, resume).await
    } else {
        run_sftp(&job, connections, role, resolution, resume).await
    };
    let outcome = result.unwrap_or_else(|error| {
        if error.kind == ErrorKind::Cancelled {
            RunOutcome::Stopped
        } else {
            RunOutcome::Failed(error)
        }
    });
    if let RunOutcome::Failed(error) = &outcome {
        if error.is_connection_lost() {
            drop_job_connections(connections, &spec).await;
        }
    }

    let release = shared.queue.lock().unwrap().release(id, &outcome);
    let outcome = match release {
        Release::Others => {
            shared.work.notify_waiters();
            return;
        }
        Release::Finalize => {
            let finalized = if streamed {
                streamed::finalize(&job, connections).await
            } else {
                finalize_sftp(&job, connections).await
            };
            match finalized {
                Ok(()) => RunOutcome::Completed,
                Err(error) if error.kind == ErrorKind::Cancelled => RunOutcome::Stopped,
                Err(error) => {
                    if error.is_connection_lost() {
                        drop_job_connections(connections, &spec).await;
                    }
                    RunOutcome::Failed(error)
                }
            }
        }
        Release::Settle => {
            if !run.restart.load(Ordering::Relaxed) {
                trim_partial(connections.existing(&spec.session_id), &run, &spec).await;
            }
            outcome
        }
    };

    let (settled, change, abandoned) = {
        let mut queue = shared.queue.lock().unwrap();
        let change = queue.settle(id, outcome, &settings, Instant::now());
        let settled = queue.job(id).map(|job| {
            (
                job.state,
                job.error.clone(),
                job.spec.clone(),
                job.reconnecting,
            )
        });
        (settled, change, queue.take_abandoned())
    };
    let error = settled.as_ref().and_then(|(_, error, _, _)| error.clone());
    if let Some((state, error, spec, false)) = settled {
        log_result(shared, &settings, state, error, &spec);
    }
    if let Some(change) = change {
        log_server_change(shared, &spec.session_id, change, error.as_deref());
    }
    discard_partials(shared, connections, abandoned).await;
    shared.work.notify_waiters();
}

async fn drop_job_connections(connections: &mut Connections, spec: &JobSpec) {
    connections.drop_connection(&spec.session_id).await;
    if let Some(source_key) = streamed::source_key(spec) {
        connections.drop_connection(&source_key).await;
    }
}

/// The pipelined path for SFTP uploads and downloads.
async fn run_sftp(
    job: &JobRef<'_>,
    connections: &mut Connections,
    role: Role,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
) -> AppResult<RunOutcome> {
    let spec = job.spec;
    let connection = connections
        .get(job.shared, &spec.session_id, &spec.session_id)
        .await?;
    let context = sftp_context(job, connection)?;
    match (spec.kind, role, spec.direction) {
        (JobKind::Folder, _, _) => expand_folder(job, context.fs, None).await,
        (JobKind::File, Role::Primary, Direction::Download) => {
            download(&context, resolution, resume).await
        }
        (JobKind::File, Role::Primary, _) => upload(&context, resolution, resume).await,
        (JobKind::File, Role::Helper, _) => help(&context).await,
    }
}

async fn finalize_sftp(job: &JobRef<'_>, connections: &mut Connections) -> AppResult<()> {
    let spec = job.spec;
    let connection = connections
        .get(job.shared, &spec.session_id, &spec.session_id)
        .await?;
    let context = sftp_context(job, connection)?;
    match spec.direction {
        Direction::Download => finalize_download(&context).await,
        Direction::Upload | Direction::Relay => finalize_upload(&context).await,
    }
}

fn sftp_context<'a>(
    job: &JobRef<'a>,
    connection: &'a WorkerConnection,
) -> AppResult<JobContext<'a>> {
    let fs = connection
        .sftp
        .as_deref()
        .ok_or_else(|| AppError::unsupported("This transfer needs an SFTP connection"))?;
    Ok(JobContext {
        shared: job.shared,
        fs,
        handle: connection.handle.as_ref(),
        id: job.id,
        run: job.run,
        spec: job.spec,
        settings: job.settings,
    })
}

fn log_server_change(shared: &Shared, session_id: &str, change: ServerChange, error: Option<&str>) {
    let label = shared
        .target(session_id)
        .map(|target| target.label.clone())
        .unwrap_or_else(|_| "the server".into());
    let reason = error
        .and_then(|error| error.strip_suffix(" (reconnecting)"))
        .unwrap_or("connection lost");
    let (level, message) = match change {
        ServerChange::Lost { retry_in } => (
            LogLevel::Warn,
            format!(
                "Lost the connection to {label} ({reason}); its transfers continue when it is back, trying again in {} s",
                retry_in.as_secs()
            ),
        ),
        ServerChange::Back => (
            LogLevel::Info,
            format!("Reconnected to {label}; its transfers continue"),
        ),
        ServerChange::GaveUp { failed } => (
            LogLevel::Error,
            format!(
                "Could not reconnect to {label}; {failed} {} failed",
                if failed == 1 { "transfer" } else { "transfers" }
            ),
        ),
    };
    shared.events.log(level, Some(session_id), message);
}

/// Deletes the temporary files of transfers removed from the queue before they finished.
async fn discard_partials(shared: &Shared, connections: &Connections, abandoned: Vec<Abandoned>) {
    for partial in abandoned {
        match partial.direction {
            Direction::Download => {
                let _ = tokio::fs::remove_file(&partial.path).await;
            }
            Direction::Upload | Direction::Relay => {
                let paths = std::slice::from_ref(&partial.path);
                match connections.existing(&partial.session_id) {
                    Some(fs) => {
                        let _ = fs.delete(paths).await;
                    }
                    None => super::discard_with_browsing_session(shared, &partial).await,
                }
            }
        }
    }
}

fn log_result(
    shared: &Shared,
    settings: &TransferSettings,
    state: JobState,
    error: Option<String>,
    spec: &JobSpec,
) {
    let (verb, past) = match spec.direction {
        Direction::Upload => ("Upload", "Uploaded"),
        Direction::Download => ("Download", "Downloaded"),
        Direction::Relay => ("Copy", "Copied"),
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
                    "{past} {} to {} ({})",
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

/// Lists a folder and queues its contents. `remote` is the server of an upload or download,
/// or the target of a relay; `source` is the server a relay reads from.
pub(super) async fn expand_folder(
    job: &JobRef<'_>,
    remote: &dyn RemoteFileSystem,
    source: Option<&dyn RemoteFileSystem>,
) -> AppResult<RunOutcome> {
    let spec = job.spec;
    let listing = match spec.direction {
        Direction::Download => remote.list_dir(&spec.source).await?,
        Direction::Upload => {
            let source = spec.source.clone();
            tokio::task::spawn_blocking(move || local::list_dir(&source)).await??
        }
        Direction::Relay => {
            source
                .ok_or_else(AppError::session_not_found)?
                .list_dir(&spec.source)
                .await?
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
        Direction::Upload | Direction::Relay => remote.ensure_dir(&spec.target).await?,
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
            Direction::Upload | Direction::Relay => remote_path::join(&spec.target, &entry.name),
            Direction::Download => Path::new(&spec.target)
                .join(&entry.name)
                .to_string_lossy()
                .into_owned(),
        };
        children.push(JobSpec {
            session_id: spec.session_id.clone(),
            source_session_id: spec.source_session_id.clone(),
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
        job.shared.events.log(
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

/// Where a file's bytes go and from which offset.
pub(super) struct Destination {
    pub target: String,
    /// A temporary file renamed over `target` once complete; `None` writes the target itself.
    pub partial: Option<String>,
    pub start: u64,
}

impl Destination {
    pub fn in_place(target: &str, start: u64) -> Self {
        Self {
            target: target.to_string(),
            partial: None,
            start,
        }
    }

    pub fn write_path(&self) -> &str {
        self.partial.as_deref().unwrap_or(&self.target)
    }
}

/// `.name.poros-part`, unless that would be too long a name.
fn partial_name(name: &str) -> Option<String> {
    let partial = format!(".{name}{PARTIAL_SUFFIX}");
    (partial.len() <= MAX_NAME_BYTES).then_some(partial)
}

fn local_partial(target: &str) -> Option<String> {
    let path = Path::new(target);
    let name = partial_name(&path.file_name()?.to_string_lossy())?;
    Some(path.with_file_name(name).to_string_lossy().into_owned())
}

fn remote_partial(target: &str) -> Option<String> {
    let parent = remote_path::parent(target)?;
    Some(remote_path::join(
        &parent,
        &partial_name(remote_path::file_name(target))?,
    ))
}

/// A new local file: through a temporary file, unless that is turned off or the target is a
/// link, which must be written through rather than replaced.
pub(super) async fn fresh_download(settings: &TransferSettings, target: String) -> Destination {
    let wanted = settings.temporary_files && !delta::is_local_symlink(Path::new(&target)).await;
    Destination {
        partial: wanted.then(|| local_partial(&target)).flatten(),
        target,
        start: 0,
    }
}

async fn fresh_upload(
    context: &JobContext<'_>,
    target: String,
    replaces: bool,
) -> AppResult<Destination> {
    let wanted =
        context.settings.temporary_files && !(replaces && context.fs.is_symlink(&target).await?);
    Ok(Destination {
        partial: wanted.then(|| remote_partial(&target)).flatten(),
        target,
        start: 0,
    })
}

pub(super) fn source_changed(resume: &ResumePoint, source: FileFacts) -> bool {
    resume.source_size != source.size || resume.source_modified != source.modified
}

async fn download(
    context: &JobContext<'_>,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
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

    let fresh = |target: String| fresh_download(context.settings, target);
    let existing = local_facts(&spec.target).await?;
    let mut destination = match (resume, existing) {
        (Some(resume), _) if source_changed(&resume, source_facts) => {
            fresh(spec.target.clone()).await
        }
        (Some(resume), existing) => {
            let written = match &resume.partial {
                Some(partial) => local_facts(partial).await?,
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
                    fresh(target).await
                }
                Decision::Write { offset: 0 } => {
                    if let Some(outcome) =
                        delta::try_download(context, &source, target_facts).await?
                    {
                        return Ok(outcome);
                    }
                    fresh(spec.target.clone()).await
                }
                Decision::Write { offset } => Destination::in_place(&spec.target, offset),
            }
        }
    };

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
        let window = check_window(destination.start);
        let theirs = context
            .fs
            .read_range(&spec.source, window.start, window.len)
            .await?;
        let ours = read_local_range(&mut file, window.start, window.len).await?;
        if theirs != ours {
            context.job().log_starting_over();
            file.set_len(0).await?;
            destination.start = 0;
        }
    }
    context.begin(
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
    );
    let handle = context.fs.open_for_read(&spec.source).await?;
    let copied = copy::download(&context.copy_context(), &handle, &mut file).await;
    let (stat, _) = context.fs.close_and_stat(handle, false).await;
    copied?;
    context.keep_closing_stat(stat);
    Ok(RunOutcome::Completed)
}

async fn upload(
    context: &JobContext<'_>,
    resolution: Option<ExistsAction>,
    resume: Option<ResumePoint>,
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

    let fs = context.fs;
    let fresh = |target: String, replaces: bool| fresh_upload(context, target, replaces);
    let existing = fs.stat(&spec.target).await?;
    let mut destination = match (resume, existing) {
        (Some(resume), existing) if source_changed(&resume, source_facts) => {
            fresh(spec.target.clone(), existing.is_some()).await?
        }
        (Some(resume), existing) => {
            let written = match &resume.partial {
                Some(partial) => fs.stat(partial).await?,
                None => existing,
            };
            let on_server = written.map_or(0, |stat| stat.size);
            Destination {
                target: spec.target.clone(),
                partial: resume.partial,
                start: resume.offset.min(on_server).min(source_facts.size),
            }
        }
        (None, None) => fresh(spec.target.clone(), false).await?,
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
                    let (target, name) = free_remote_name(fs, &spec.target).await?;
                    context.retarget(&target, name);
                    fresh(target, false).await?
                }
                Decision::Write { offset: 0 } => {
                    if let Some(outcome) =
                        delta::try_upload(context, &metadata, source_facts, &stat).await?
                    {
                        return Ok(outcome);
                    }
                    fresh(spec.target.clone(), true).await?
                }
                Decision::Write { offset } => Destination::in_place(&spec.target, offset),
            }
        }
    };

    let source_permissions = local_permissions(&metadata);
    let replaced_permissions = existing.and_then(|stat| stat.permissions);
    let permissions = if context.settings.preserve_permissions {
        source_permissions
    } else {
        replaced_permissions
    };
    let mut file = File::open(&spec.source)
        .await
        .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
    let truncate = destination.start == 0;
    let handle = match fs
        .open_for_write(destination.write_path(), truncate, permissions)
        .await
    {
        // A folder may let files be replaced but not created.
        Err(error)
            if destination.partial.is_some() && error.kind == ErrorKind::PermissionDenied =>
        {
            destination = Destination::in_place(&spec.target, 0);
            fs.open_for_write(&spec.target, true, permissions).await?
        }
        opened => opened?,
    };
    if destination.start > 0 {
        let window = check_window(destination.start);
        let theirs = fs
            .read_range(destination.write_path(), window.start, window.len)
            .await?;
        let ours = read_local_range(&mut file, window.start, window.len).await?;
        if theirs != ours {
            context.job().log_starting_over();
            fs.truncate(destination.write_path(), 0).await?;
            destination.start = 0;
        }
    }
    let write_path = destination.write_path().to_string();
    context.begin(
        Plan {
            target: write_path,
            rename_to: destination.partial.is_some().then_some(destination.target),
            source_size: source_facts.size,
            source_modified: source_facts.modified,
            source_permissions,
            replaced_permissions,
        },
        destination.start,
        source_facts.size,
    );
    let copied = copy::upload(&context.copy_context(), &mut file, &handle).await;
    let (stat, closed) = fs
        .close_and_stat(handle, context.settings.flush_to_disk)
        .await;
    copied?;
    closed?;
    context.keep_closing_stat(stat);
    Ok(RunOutcome::Completed)
}

/// The bytes just before `start` that both sides must agree on before a file continues.
pub(super) struct CheckWindow {
    pub start: u64,
    pub len: u32,
}

pub(super) fn check_window(start: u64) -> CheckWindow {
    let len = start.min(RESUME_CHECK_BYTES);
    CheckWindow {
        start: start - len,
        len: len as u32,
    }
}

pub(super) async fn read_local_range(file: &mut File, offset: u64, len: u32) -> AppResult<Vec<u8>> {
    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut data = Vec::with_capacity(len as usize);
    file.take(u64::from(len)).read_to_end(&mut data).await?;
    Ok(data)
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
        Direction::Upload | Direction::Relay => {
            let mut file = File::open(&spec.source)
                .await
                .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
            let handle = context.fs.open_for_write(&plan.target, false, None).await?;
            let copied = copy::upload(&context.copy_context(), &mut file, &handle).await;
            let (_, closed) = context
                .fs
                .close_and_stat(handle, context.settings.flush_to_disk)
                .await;
            copied?;
            closed?;
        }
    }
    Ok(RunOutcome::Completed)
}

/// Checks the finished file, sets its time and permissions and moves it into place.
async fn finalize_download(context: &JobContext<'_>) -> AppResult<()> {
    let (run, spec, settings) = (context.run, context.spec, context.settings);
    let Some(plan) = run.plan.get().cloned() else {
        return Ok(());
    };
    let source = match run.closing_stat.get() {
        Some(stat) => Some(*stat),
        None => context.fs.stat(&spec.source).await?,
    };
    let end = downloaded_end(&context.job(), &plan, source)?;
    let target = plan.target.clone();
    let truncate = if settings.verify_checksums {
        let written = target.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::OpenOptions::new()
                .write(true)
                .open(&written)?
                .set_len(end)
        })
        .await?
        .map_err(|error| AppError::from(error).with_path(target.clone()))?;
        if let Checked::Different = verify::same_contents(context, &spec.source, &target).await? {
            return Err(context.restart(format!(
                "The downloaded copy of {} does not match the server's checksum",
                spec.name
            )));
        }
        false
    } else {
        true
    };
    complete_download(settings, plan, end, truncate).await
}

/// Where the downloaded file ends, once the source is known not to have changed meanwhile.
pub(super) fn downloaded_end(
    job: &JobRef<'_>,
    plan: &Plan,
    source: Option<RemoteStat>,
) -> AppResult<u64> {
    let end = job.run.end().unwrap_or(0);
    let unchanged = source
        .is_some_and(|stat| stat.size == plan.source_size && stat.modified == plan.source_modified)
        && end == plan.source_size;
    if !unchanged {
        return Err(job.restart(format!(
            "{} changed on the server while it was being downloaded",
            job.spec.name
        )));
    }
    Ok(end)
}

/// Sets the time and permissions of a downloaded file and moves it into place. `truncate`
/// cuts it to `end` first.
pub(super) async fn complete_download(
    settings: &TransferSettings,
    plan: Plan,
    end: u64,
    truncate: bool,
) -> AppResult<()> {
    let modified = settings
        .preserve_timestamps
        .then_some(plan.source_modified)
        .flatten();
    let preserve_permissions = settings.preserve_permissions;
    let flush = settings.flush_to_disk;
    let target = plan.target.clone();
    let complete = move || -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new().write(true).open(&plan.target)?;
        // Workers write pieces out of order, and a resumed file may be longer than its source.
        if truncate {
            file.set_len(end)?;
        }
        if let Some(seconds) = modified.and_then(|seconds| u64::try_from(seconds).ok()) {
            file.set_modified(UNIX_EPOCH + Duration::from_secs(seconds))?;
        }
        let permissions = if preserve_permissions {
            plan.source_permissions
        } else {
            // A file replaced through a temporary one keeps its permissions.
            plan.rename_to
                .as_ref()
                .and_then(|replaced| std::fs::metadata(replaced).ok())
                .and_then(|metadata| local_permissions(&metadata))
        };
        set_local_permissions(&file, permissions)?;
        if flush {
            file.sync_all()?;
        }
        drop(file);
        match &plan.rename_to {
            Some(replaced) => std::fs::rename(&plan.target, replaced),
            None => Ok(()),
        }
    };
    tokio::task::spawn_blocking(complete)
        .await?
        .map_err(|error| AppError::from(error).with_path(target))
}

async fn finalize_upload(context: &JobContext<'_>) -> AppResult<()> {
    let (fs, run, spec, settings) = (context.fs, context.run, context.spec, context.settings);
    let Some(plan) = run.plan.get() else {
        return Ok(());
    };
    let end = run.end().unwrap_or(0);
    let size = match run.closing_stat.get() {
        Some(stat) if stat.size == end => end,
        _ => fs.stat(&plan.target).await?.map_or(0, |stat| stat.size),
    };
    if size < end {
        return Err(context.restart(format!(
            "The server holds {} of the {} sent for {}",
            format_size(size),
            format_size(end),
            spec.name
        )));
    }
    if size > end {
        fs.truncate(&plan.target, end).await?;
    }
    let metadata = tokio::fs::metadata(&spec.source)
        .await
        .map_err(|error| AppError::from(error).with_path(spec.source.clone()))?;
    if metadata.len() != plan.source_size || modified_seconds(&metadata) != plan.source_modified {
        return Err(context.restart(format!("{} changed while it was being uploaded", spec.name)));
    }
    if settings.verify_checksums {
        if let Checked::Different =
            verify::same_contents(context, &plan.target, &spec.source).await?
        {
            return Err(context.restart(format!(
                "The copy of {} on the server does not match the local file's checksum",
                spec.name
            )));
        }
    }

    let modified = settings
        .preserve_timestamps
        .then_some(plan.source_modified)
        .flatten();
    let permissions = if settings.preserve_permissions {
        plan.source_permissions
    } else {
        plan.rename_to.as_ref().and(plan.replaced_permissions)
    };
    let attributes = fs.set_attributes(&plan.target, modified, permissions);
    // Sent together: servers apply them in order, so this costs one round trip.
    let (attributes, replaced) = match &plan.rename_to {
        Some(replaced) => tokio::join!(attributes, fs.replace(&plan.target, replaced)),
        None => (attributes.await, Ok(())),
    };
    // Some servers refuse attribute changes; the file itself is complete.
    if let Err(error) = attributes {
        context.shared.events.log(
            LogLevel::Warn,
            Some(&spec.session_id),
            format!(
                "Could not set the time or permissions of {}: {}",
                spec.target, error.message
            ),
        );
    }
    replaced
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
        Direction::Upload | Direction::Relay => {
            if let Some(fs) = fs {
                let _ = fs.truncate(&plan.target, prefix).await;
            }
        }
    }
}

pub(super) fn conflict(source: FileFacts, target: FileFacts) -> RunOutcome {
    RunOutcome::Conflict(ConflictInfo {
        source_size: source.size,
        source_modified: source.modified,
        target_size: target.size,
        target_modified: target.modified,
    })
}

pub(super) fn folder_in_the_way(spec: &JobSpec) -> AppError {
    AppError::new(
        ErrorKind::AlreadyExists,
        format!("A folder named {} is in the way", spec.name),
    )
    .with_path(spec.target.clone())
}

/// `None` when nothing is there; otherwise whether it is a folder, and its size and time.
pub(super) async fn local_facts(path: &str) -> AppResult<Option<(bool, FileFacts)>> {
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

pub(super) fn modified_seconds(metadata: &std::fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since_epoch| since_epoch.as_secs() as i64)
}

#[cfg(unix)]
pub(super) fn local_permissions(metadata: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
pub(super) fn local_permissions(_metadata: &std::fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
pub(super) fn set_local_permissions(
    file: &std::fs::File,
    mode: Option<u32>,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match mode {
        Some(mode) => file.set_permissions(std::fs::Permissions::from_mode(mode)),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
pub(super) fn set_local_permissions(
    _file: &std::fs::File,
    _mode: Option<u32>,
) -> std::io::Result<()> {
    Ok(())
}

pub(super) async fn free_local_name(target: &str) -> AppResult<(String, String)> {
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

pub(super) async fn free_remote_name(
    fs: &dyn RemoteFileSystem,
    target: &str,
) -> AppResult<(String, String)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_files_sit_hidden_beside_their_targets() {
        assert_eq!(
            remote_partial("/srv/data/report.pdf").as_deref(),
            Some("/srv/data/.report.pdf.poros-part")
        );
        assert_eq!(
            remote_partial("/report.pdf").as_deref(),
            Some("/.report.pdf.poros-part")
        );
        let local = local_partial("/home/me/report.pdf").unwrap();
        assert_eq!(
            Path::new(&local),
            Path::new("/home/me/.report.pdf.poros-part")
        );
        // Too long a name would be refused, so such files are written in place.
        assert!(partial_name(&"x".repeat(MAX_NAME_BYTES)).is_none());
    }

    #[test]
    fn resuming_compares_the_bytes_just_before_the_resume_point() {
        let near_start = check_window(1000);
        assert_eq!((near_start.start, near_start.len), (0, 1000));
        let further = check_window(RESUME_CHECK_BYTES * 3);
        assert_eq!(
            (further.start, u64::from(further.len)),
            (RESUME_CHECK_BYTES * 2, RESUME_CHECK_BYTES)
        );
    }
}
