//! The transfer queue, in the style of SmartFTP: several workers, each on its own connection,
//! take jobs in queue order. Many small files spread across the workers, and idle workers join
//! a large file to move separate parts of it at once. SFTP has a pipelined path of its own;
//! other protocols, and copies between two servers, stream through `RemoteFileSystem`.

mod conflict;
mod copy;
mod delta;
mod hash_shell;
mod limit_schedule;
mod limiter;
mod options;
mod persist;
mod pieces;
mod published;
mod queue;
mod scheduled_limits;
mod single;
mod streamed;
mod verify;
mod worker;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

pub use conflict::ExistsAction;
pub use limit_schedule::{
    clock_time, LimitMode, LimitRule, LimitSchedule, Limits, PauseEnds, RateLimit,
};
pub use options::{Backup, JobOptions};
use queue::Totals;
pub use queue::{
    ChangedDirectory, Direction, JobId, JobKind, JobSnapshot, JobState, QueueCounts, Side,
};
pub use scheduled_limits::{ActiveRule, LimitClock, LimitStatus};
pub use single::{FileQueue, JobProgress, TransferRoute};
#[cfg(feature = "gui")]
pub(crate) use worker::is_partial_name;
pub(crate) use worker::{ensure_remote_folders, keep_local_backup, keep_remote_backup};

use crate::cryptomator::VaultFs;
use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel};
use crate::format::{format_duration, format_size};
use crate::protocol::{Protocol, RemoteFileSystem};
use crate::session::{Session, SessionManager};
use crate::settings::TransferSettings;
use crate::ssh::ConnectProfile;
use crate::{local, remote_path};
use limiter::RateLimiter;
use queue::{Abandoned, JobSpec, Queue};
use scheduled_limits::ScheduledLimits;

const TICK: Duration = Duration::from_millis(250);
/// A change to the queue is saved this long after it happens, gathering the ones that follow.
const SAVE_DELAY: Duration = Duration::from_secs(2);
const SAVE_PROGRESS_EVERY: Duration = Duration::from_secs(10);
/// The overall speed averages over this many ticks.
const SPEED_WINDOW_TICKS: usize = 8;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueRequest {
    /// The server written to, or read from for a download.
    pub session_id: String,
    /// The server a relay reads from.
    #[serde(default)]
    pub source_session_id: Option<String>,
    pub direction: Direction,
    pub target_directory: String,
    pub items: Vec<TransferItem>,
}

/// One file to copy, for callers that choose the files themselves.
#[derive(Debug, Clone)]
pub struct FileTransfer {
    pub direction: Direction,
    pub name: String,
    pub source: String,
    pub target: String,
    pub target_directory: String,
    pub size: u64,
    /// What happens to a file already at the target.
    pub resolution: ExistsAction,
    pub options: Option<Arc<JobOptions>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferStats {
    pub counts: QueueCounts,
    pub remaining_bytes: u64,
    /// Bytes per second over the last two seconds.
    pub upload_speed: u64,
    pub download_speed: u64,
    pub queue_paused: bool,
    /// The speed limits in force, and the rule of the schedule that set them.
    pub limits: LimitStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferUpdate {
    /// One more than the update before, so a window can tell when it missed one.
    pub sequence: u64,
    pub jobs: Vec<JobSnapshot>,
    pub removed: Vec<JobId>,
    pub changed_directories: Vec<ChangedDirectory>,
    pub stats: TransferStats,
}

/// Sent when the queue runs dry after moving at least one file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueFinished {
    pub done: u64,
    pub failed: u64,
    pub skipped: u64,
    pub bytes: u64,
    pub elapsed_millis: u64,
    /// Jobs still waiting because they are paused; the queue is not really finished.
    pub paused: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferList {
    /// The latest update this list already holds.
    pub sequence: u64,
    pub jobs: Vec<JobSnapshot>,
    pub stats: TransferStats,
}

/// Fills in a profile's route from the current connection settings and saved connections.
/// Blocks on the keychain.
pub type RouteResolver = Arc<dyn Fn(&mut ConnectProfile) -> AppResult<()> + Send + Sync>;

/// What workers need to reach a session's server, kept after the browsing session closes so
/// queued transfers can still run.
pub(crate) struct SessionTarget {
    pub session_id: String,
    pub label: String,
    login: Mutex<Login>,
    /// Set once the server refuses extra connections; workers then open channels on the
    /// browsing connection instead.
    pub channels_only: AtomicBool,
    /// Whether the server ran the configured rsync command, once a worker has tried.
    pub rsync: tokio::sync::Mutex<Option<delta::RsyncCheck>>,
    /// The server's checksum commands once a worker has looked.
    pub hashing: tokio::sync::Mutex<Option<verify::ServerHashing>>,
    /// Set once the server would not keep a shell open for checksum commands, which then
    /// each run on a channel of their own.
    pub shell_refused: AtomicBool,
    /// Checksum files and signatures found beside downloads, by folder.
    pub published: published::FolderCache,
    /// Saved by an earlier run of the app, without the password, passphrase or sign-in.
    restored: AtomicBool,
    /// Sessions this FTP server would not send files to directly, so relays to them skip
    /// trying FXP again.
    pub fxp_refused: Mutex<BTreeSet<String>>,
    /// A restored login's route is still to be worked out. Workers wait on the lock so none
    /// connects before it is, which could skip the proxy or a jump host.
    route_pending: tokio::sync::Mutex<bool>,
}

#[derive(Clone)]
pub(crate) struct Login {
    pub profile: ConnectProfile,
    pub host_key_fingerprint: String,
    /// The session whose connection carries channels when the server refuses more connections.
    pub browsing_session: String,
    /// That session's files for protocols other than SFTP. Cloud workers share them, and FTP
    /// workers fall back to them when the server refuses more connections.
    pub browsing_files: Option<Arc<dyn RemoteFileSystem>>,
    /// The unlocked vault the session shows, for a vault's session. Its profile is that of the
    /// server the vault is on, and every connection a worker opens with it is put inside the
    /// vault before anything is read or written.
    pub vault: Option<Arc<VaultFs>>,
}

impl Login {
    fn of(session: &Session) -> Self {
        Self {
            profile: session.profile.clone(),
            host_key_fingerprint: session.host_key_fingerprint.clone(),
            browsing_session: session.id.clone(),
            browsing_files: (session.protocol() != Protocol::Sftp).then(|| session.files()),
            vault: session.vault().cloned(),
        }
    }

    fn label(&self) -> String {
        match &self.vault {
            Some(vault) => format!("{} on {}", vault.name(), self.profile.label()),
            None => self.profile.label(),
        }
    }
}

impl SessionTarget {
    fn new(session_id: String, login: Login, restored: bool) -> Self {
        Self {
            session_id,
            label: login.label(),
            login: Mutex::new(login),
            channels_only: AtomicBool::new(false),
            rsync: tokio::sync::Mutex::new(None),
            hashing: tokio::sync::Mutex::new(None),
            shell_refused: AtomicBool::new(false),
            published: published::FolderCache::default(),
            restored: AtomicBool::new(restored),
            fxp_refused: Mutex::new(BTreeSet::new()),
            route_pending: tokio::sync::Mutex::new(restored),
        }
    }

    fn for_session(session: &Session) -> Self {
        Self::new(session.id.clone(), Login::of(session), false)
    }

    pub fn login(&self) -> Login {
        self.login.lock().unwrap().clone()
    }

    /// The protocol of the server, which for a vault is the server the vault is on.
    pub fn protocol(&self) -> Protocol {
        self.login.lock().unwrap().profile.protocol
    }

    /// Whether the session shows an unlocked vault instead of its server's own files.
    pub fn is_vault(&self) -> bool {
        self.login.lock().unwrap().vault.is_some()
    }

    pub fn browsing_files(&self) -> AppResult<Arc<dyn RemoteFileSystem>> {
        self.login
            .lock()
            .unwrap()
            .browsing_files
            .clone()
            .ok_or_else(AppError::session_not_found)
    }

    pub fn is_restored(&self) -> bool {
        self.restored.load(Ordering::Relaxed)
    }

    /// Works out a restored login's route the first time it connects on its own.
    pub async fn resolve_restored_route(&self, resolver: Option<&RouteResolver>) -> AppResult<()> {
        let mut pending = self.route_pending.lock().await;
        if !*pending || !self.is_restored() {
            return Ok(());
        }
        if let Some(resolve) = resolver.cloned() {
            let mut profile = self.login().profile;
            let profile =
                tokio::task::spawn_blocking(move || resolve(&mut profile).map(|()| profile))
                    .await??;
            let mut login = self.login.lock().unwrap();
            // A session the user opened meanwhile brought its own route.
            if self.is_restored() {
                login.profile = profile;
            }
        }
        *pending = false;
        Ok(())
    }

    /// Takes over a session the user has open to the same server when the browsing session
    /// is gone, for its connection, or this target was restored, for its password, sign-in and
    /// host key. A vault that was locked takes over the session it was unlocked in again.
    pub async fn adopt_live_session(&self, sessions: &SessionManager) {
        let login = self.login();
        // A vault is reached through its own session only, which outlives a reconnect of its
        // server. Another session of that server shows the encrypted files, not the vault's.
        if let Some(vault) = &login.vault {
            if vault.is_locked() {
                // Unlocked again since, the vault takes these transfers up in its new session.
                let unlocked = sessions.find_vault(&login.profile, vault.root()).await;
                if let Some(unlocked) = unlocked {
                    *self.login.lock().unwrap() = Login::of(&unlocked);
                }
            }
            return;
        }
        let restored = self.is_restored();
        if !restored && sessions.is_live(&login.browsing_session).await {
            return;
        }
        let Some(live) = sessions.find_live(&login.profile, None).await else {
            return;
        };
        let adopted = Login::of(&live);
        let mut current = self.login.lock().unwrap();
        current.browsing_session = adopted.browsing_session;
        current.browsing_files = adopted.browsing_files;
        if restored {
            current.profile = adopted.profile;
            current.host_key_fingerprint = adopted.host_key_fingerprint;
            self.restored.store(false, Ordering::Relaxed);
        }
    }
}

pub(crate) struct Shared {
    queue: Mutex<Queue>,
    work: Notify,
    settings: RwLock<TransferSettings>,
    targets: Mutex<HashMap<String, Arc<SessionTarget>>>,
    sessions: Arc<SessionManager>,
    events: Events,
    upload_limiter: RateLimiter,
    download_limiter: RateLimiter,
    /// What the speed limit schedule makes of the two limits now.
    scheduled_limits: ScheduledLimits,
    /// Limits shared by the jobs of one synchronization, by its limit group.
    job_limiters: Mutex<HashMap<String, Weak<RateLimiter>>>,
    uploaded: AtomicU64,
    downloaded: AtomicU64,
    live_workers: Mutex<BTreeSet<usize>>,
    stopping: AtomicBool,
    /// Taken by a relay that borrows the browsing connections of two FTP servers, so two
    /// relays in opposite directions cannot each hold one server while waiting for the other.
    borrowed_relays: tokio::sync::Mutex<()>,
    /// Where unfinished transfers are saved between runs; unset in tests.
    queue_file: OnceLock<PathBuf>,
    saving: AtomicBool,
    /// Held while the queue is saved, so saves are written one at a time.
    writing: Mutex<()>,
    /// Unset in tests, where restored servers connect directly.
    route_resolver: OnceLock<RouteResolver>,
    /// Jobs writing into these folders are not saved between runs.
    unsaved_folders: Mutex<Vec<PathBuf>>,
}

impl Shared {
    fn settings(&self) -> TransferSettings {
        self.settings.read().unwrap().clone()
    }

    /// The job writes into a folder that is not kept between runs, or belongs to a part of
    /// the app that queues it again itself.
    fn is_unsaved(&self, spec: &JobSpec) -> bool {
        spec.options.as_ref().is_some_and(|options| options.quiet)
            || self
                .unsaved_folders
                .lock()
                .unwrap()
                .iter()
                .any(|folder| downloads_into(spec, folder))
    }

    fn target(&self, session_id: &str) -> AppResult<Arc<SessionTarget>> {
        self.targets
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .ok_or_else(AppError::session_not_found)
    }

    /// A relay counts once, as data coming in, so batch totals do not count it twice.
    fn total_for(&self, direction: Direction) -> &AtomicU64 {
        match direction {
            Direction::Upload => &self.uploaded,
            Direction::Download | Direction::Relay => &self.downloaded,
        }
    }

    /// A relay is held to the download limit here and to the upload limit as it is sent on.
    fn limiter_for(&self, direction: Direction) -> &RateLimiter {
        match direction {
            Direction::Upload => &self.upload_limiter,
            Direction::Download | Direction::Relay => &self.download_limiter,
        }
    }

    /// The limit a job shares with the other jobs of its synchronization, if it has one.
    fn job_limiter(&self, spec: &JobSpec) -> Option<Arc<RateLimiter>> {
        let options = spec
            .options
            .as_deref()
            .filter(|options| options.rate_limit > 0)?;
        let mut limiters = self.job_limiters.lock().unwrap();
        if let Some(limiter) = limiters.get(&options.limit_group).and_then(Weak::upgrade) {
            return Some(limiter);
        }
        limiters.retain(|_, limiter| limiter.strong_count() > 0);
        let limiter = Arc::new(RateLimiter::new());
        limiter.set_rate(options.rate_limit);
        limiters.insert(options.limit_group.clone(), Arc::downgrade(&limiter));
        Some(limiter)
    }

    /// The settings a job runs with: the transfer settings, the rsync command of its server's
    /// site, and what a synchronization asked of it.
    fn job_settings(&self, spec: &JobSpec) -> TransferSettings {
        let mut settings = self.settings();
        let site_rsync = self
            .target(&spec.session_id)
            .ok()
            .and_then(|target| target.login().profile.rsync_path)
            .and_then(|path| one_line(&path));
        if let Some(path) = site_rsync {
            settings.rsync_path = path;
        }
        if let Some(options) = &spec.options {
            options.apply(&mut settings);
        }
        settings
    }
}

/// How the jobs a caller queued ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobsOutcome {
    pub done: u64,
    pub failed: u64,
    pub skipped: u64,
    /// Removed from the queue before they finished.
    pub removed: u64,
    pub bytes: u64,
}

/// Follows queued jobs. Unlike the manager, dropping it leaves the queue running.
#[derive(Clone)]
pub struct TransferWatch {
    shared: Arc<Shared>,
}

impl TransferWatch {
    /// Waits until none of `ids` is queued, running, paused or waiting on a conflict. The
    /// jobs must have been watched (`Queue::watch`) as they were queued.
    pub async fn wait(&self, ids: &[JobId]) -> JobsOutcome {
        loop {
            {
                let mut queue = self.shared.queue.lock().unwrap();
                let mut outcome = JobsOutcome::default();
                let mut pending = false;
                for id in ids {
                    let (state, size) = match queue.job(*id) {
                        Some(job) => (job.state, job.spec.size),
                        None => match queue.ended(*id) {
                            Some(ended) => (ended.state, ended.size),
                            None => {
                                outcome.removed += 1;
                                continue;
                            }
                        },
                    };
                    match state {
                        JobState::Done => {
                            outcome.done += 1;
                            outcome.bytes += size;
                        }
                        JobState::Failed => outcome.failed += 1,
                        JobState::Skipped => outcome.skipped += 1,
                        JobState::Queued
                        | JobState::Running
                        | JobState::Paused
                        | JobState::Conflict => {
                            if queue.job(*id).is_some() {
                                pending = true;
                            } else {
                                // Taken off the queue before it finished.
                                outcome.removed += 1;
                            }
                        }
                    }
                }
                if !pending || self.shared.stopping.load(Ordering::Relaxed) {
                    queue.unwatch(ids);
                    return outcome;
                }
            }
            tokio::time::sleep(TICK * 2).await;
        }
    }

    /// How the downloads into `folder` and below it stand.
    pub fn downloads_into(&self, folder: &Path) -> FolderProgress {
        let queue = self.shared.queue.lock().unwrap();
        let mut progress = FolderProgress {
            queue_paused: queue.paused,
            waiting_for_schedule: queue.held().download,
            ..FolderProgress::default()
        };
        for job in queue.jobs().filter(|job| downloads_into(&job.spec, folder)) {
            match job.state {
                JobState::Queued | JobState::Running | JobState::Paused | JobState::Conflict => {
                    progress.unfinished += 1
                }
                JobState::Failed => progress.failures.push(format!(
                    "{}: {}",
                    job.spec.name,
                    job.error.as_deref().unwrap_or("failed")
                )),
                JobState::Done | JobState::Skipped => {}
            }
        }
        progress
    }

    /// Waits until nothing is left to download into `folder`, or the app is closing.
    pub async fn wait_for_downloads_into(&self, folder: &Path) -> FolderProgress {
        loop {
            let progress = self.downloads_into(folder);
            if progress.unfinished == 0 || self.shared.stopping.load(Ordering::Relaxed) {
                return progress;
            }
            tokio::time::sleep(TICK).await;
        }
    }

    /// Takes every download into `folder` off the queue, stopping the running ones.
    pub fn discard_downloads_into(&self, folder: &Path) {
        update_queue(&self.shared, |queue| {
            let ids: Vec<JobId> = queue
                .jobs()
                .filter(|job| downloads_into(&job.spec, folder))
                .map(|job| job.id)
                .collect();
            queue.remove(&ids);
        });
    }
}

/// How the downloads into one folder stand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderProgress {
    /// Jobs still to finish, paused ones and ones waiting on a conflict included.
    pub unfinished: usize,
    /// What went wrong with the ones that failed.
    pub failures: Vec<String>,
    /// The whole queue is paused, so nothing unfinished moves until it runs again.
    pub queue_paused: bool,
    /// The speed limit schedule pauses downloads now, so they wait for it to let them go.
    pub waiting_for_schedule: bool,
}

pub struct TransferManager {
    shared: Arc<Shared>,
}

impl TransferManager {
    /// Spawns its tasks through `runtime`, so the app can create it outside an async context.
    pub fn new(sessions: Arc<SessionManager>, events: Events, settings: TransferSettings) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::new()),
            work: Notify::new(),
            settings: RwLock::new(settings.clone()),
            targets: Mutex::new(HashMap::new()),
            sessions,
            events,
            upload_limiter: RateLimiter::new(),
            download_limiter: RateLimiter::new(),
            scheduled_limits: ScheduledLimits::new(),
            job_limiters: Mutex::new(HashMap::new()),
            uploaded: AtomicU64::new(0),
            downloaded: AtomicU64::new(0),
            live_workers: Mutex::new(BTreeSet::new()),
            stopping: AtomicBool::new(false),
            borrowed_relays: tokio::sync::Mutex::new(()),
            queue_file: OnceLock::new(),
            saving: AtomicBool::new(false),
            writing: Mutex::new(()),
            route_resolver: OnceLock::new(),
            unsaved_folders: Mutex::new(Vec::new()),
        });
        let manager = Self { shared };
        manager.configure(settings);
        crate::runtime::spawn(report_progress(Arc::downgrade(&manager.shared)));
        manager
    }

    pub fn configure(&self, settings: TransferSettings) {
        let shared = &self.shared;
        let workers = settings.workers as usize;
        *shared.settings.write().unwrap() = settings;
        scheduled_limits::apply(shared);
        let mut live = shared.live_workers.lock().unwrap();
        for index in 0..workers {
            if live.insert(index) {
                crate::runtime::spawn(worker::run(shared.clone(), index));
            }
        }
        drop(live);
        // Workers above the new count notice and exit.
        shared.work.notify_waiters();
    }

    pub async fn enqueue(&self, request: EnqueueRequest) -> AppResult<usize> {
        let specs = self.specs_for(&request).await?;
        self.add_jobs(specs, Placement::Last);
        Ok(request.items.len())
    }

    /// Queues jobs ahead of everything else, starting them even when transfers wait for Start:
    /// a drag out of Poros waits on them.
    pub async fn enqueue_first(&self, request: EnqueueRequest) -> AppResult<()> {
        let specs = self.specs_for(&request).await?;
        self.add_jobs(specs, Placement::First);
        Ok(())
    }

    /// Leaves out of the saved queue the jobs writing into `folder`, which holds files only for
    /// as long as Poros runs.
    pub fn keep_out_of_saved_queue(&self, folder: PathBuf) {
        self.shared.unsaved_folders.lock().unwrap().push(folder);
    }

    async fn specs_for(&self, request: &EnqueueRequest) -> AppResult<Vec<JobSpec>> {
        let session = self.shared.sessions.get(&request.session_id).await?;
        let source_session = match (request.direction, &request.source_session_id) {
            (Direction::Relay, Some(source_id)) => Some(self.shared.sessions.get(source_id).await?),
            (Direction::Relay, None) => {
                return Err(AppError::invalid("A copy between servers needs a source"))
            }
            _ => None,
        };
        let mut specs = Vec::with_capacity(request.items.len());
        for item in &request.items {
            local::validate_name(&item.name)?;
            let target = match request.direction {
                Direction::Upload | Direction::Relay => {
                    remote_path::join(&request.target_directory, &item.name)
                }
                Direction::Download => Path::new(&request.target_directory)
                    .join(&item.name)
                    .to_string_lossy()
                    .into_owned(),
            };
            specs.push(JobSpec {
                session_id: request.session_id.clone(),
                source_session_id: source_session.as_ref().map(|source| source.id.clone()),
                direction: request.direction,
                kind: if item.is_dir {
                    JobKind::Folder
                } else {
                    JobKind::File
                },
                name: item.name.clone(),
                source: item.path.clone(),
                target,
                target_directory: request.target_directory.clone(),
                size: if item.is_dir { 0 } else { item.size },
                ancestors: Arc::from([]),
                options: None,
            });
        }
        if let Some(source_session) = &source_session {
            self.register(source_session);
        }
        self.register(&session);
        Ok(specs)
    }

    /// Queues single files, each knowing what to do with a file already at its target.
    pub async fn enqueue_files(
        &self,
        session_id: &str,
        files: Vec<FileTransfer>,
    ) -> AppResult<Vec<JobId>> {
        let session = self.shared.sessions.get(session_id).await?;
        self.register(&session);
        let auto_start = self.shared.settings().auto_start;
        let ids = {
            let mut queue = self.shared.queue.lock().unwrap();
            if !auto_start && queue.counts().is_idle() {
                queue.paused = true;
            }
            let ids: Vec<JobId> = files
                .into_iter()
                .map(|file| {
                    let spec = JobSpec {
                        session_id: session_id.to_string(),
                        source_session_id: None,
                        direction: file.direction,
                        kind: JobKind::File,
                        name: file.name,
                        source: file.source,
                        target: file.target,
                        target_directory: file.target_directory,
                        size: file.size,
                        ancestors: Arc::from([]),
                        options: file.options,
                    };
                    queue.add_resolved(spec, file.resolution)
                })
                .collect();
            queue.watch(&ids);
            ids
        };
        self.shared.work.notify_waiters();
        Ok(ids)
    }

    /// A handle that follows jobs without keeping the queue's workers.
    pub fn watch(&self) -> TransferWatch {
        TransferWatch {
            shared: self.shared.clone(),
        }
    }

    /// Keeps what workers need to reach the session's server.
    fn register(&self, session: &Session) {
        self.shared
            .targets
            .lock()
            .unwrap()
            .entry(session.id.clone())
            .or_insert_with(|| Arc::new(SessionTarget::for_session(session)));
    }

    fn add_jobs(&self, specs: Vec<JobSpec>, placement: Placement) {
        let auto_start = self.shared.settings().auto_start;
        {
            let mut queue = self.shared.queue.lock().unwrap();
            if placement == Placement::Last && !auto_start && queue.counts().is_idle() {
                queue.paused = true;
            }
            let ids: Vec<JobId> = specs.into_iter().map(|spec| queue.add(spec)).collect();
            if placement == Placement::First {
                queue.move_to(&ids, true);
            }
        }
        self.shared.work.notify_waiters();
    }

    pub fn list(&self) -> TransferList {
        let limits = self.shared.scheduled_limits.status();
        let queue = self.shared.queue.lock().unwrap();
        TransferList {
            sequence: queue.sequence,
            jobs: queue.snapshots(),
            stats: TransferStats {
                counts: queue.counts(),
                remaining_bytes: queue.remaining_bytes(),
                queue_paused: queue.paused,
                limits,
                ..Default::default()
            },
        }
    }

    /// Sets the speed limit schedule aside until it next changes what is in force, or takes
    /// it up again. Never saved: a restart follows the schedule.
    pub fn ignore_limit_schedule(&self, ignored: bool) {
        scheduled_limits::set_ignored(&self.shared, ignored);
    }

    /// Has the speed limit schedule read the time from `clock` instead of the system's.
    pub fn read_time_from(&self, clock: LimitClock) {
        scheduled_limits::set_clock(&self.shared, clock);
    }

    fn update_queue(&self, change: impl FnOnce(&mut Queue)) {
        update_queue(&self.shared, change);
    }

    /// How transfers restored from the last run reach their servers through the proxy and jump
    /// hosts before a session to them is open again.
    pub fn resolve_routes_with(&self, resolver: RouteResolver) {
        let _ = self.shared.route_resolver.set(resolver);
    }

    /// Restores the transfers saved in `file` by the last run, paused, and keeps saving there.
    pub fn keep_queue_in(&self, file: PathBuf) {
        if self.shared.queue_file.set(file.clone()).is_err() {
            return;
        }
        if !self.shared.settings().keep_queue {
            return;
        }
        let restored = persist::restore(&self.shared, &file);
        if restored > 0 {
            self.shared.events.log(
                LogLevel::Info,
                None,
                format!(
                    "Restored {restored} unfinished {} from last time, paused in the queue",
                    if restored == 1 {
                        "transfer"
                    } else {
                        "transfers"
                    },
                ),
            );
        }
    }

    /// Saves unfinished transfers now, as the app closes.
    pub fn save_queue(&self) {
        persist::save(&self.shared);
    }

    pub fn set_paused(&self, paused: bool) {
        self.update_queue(|queue| queue.set_paused(paused, Instant::now()));
    }

    pub fn pause(&self, ids: &[JobId]) {
        self.update_queue(|queue| queue.pause(ids));
    }

    pub fn resume(&self, ids: &[JobId]) {
        self.update_queue(|queue| queue.resume(ids));
    }

    pub fn remove(&self, ids: &[JobId]) {
        self.update_queue(|queue| queue.remove(ids));
    }

    pub fn clear(&self, states: &[JobState]) {
        self.update_queue(|queue| queue.clear(states));
    }

    pub fn move_to(&self, ids: &[JobId], to_top: bool) {
        self.update_queue(|queue| queue.move_to(ids, to_top));
    }

    pub fn resolve(&self, id: JobId, action: ExistsAction, apply_to_all: bool) {
        self.update_queue(|queue| queue.resolve(id, action, apply_to_all));
    }

    /// Pauses every unfinished job of a session; returns how many there were.
    pub fn pause_session(&self, session_id: &str) -> usize {
        let mut queue = self.shared.queue.lock().unwrap();
        let ids = queue.session_job_ids(session_id);
        queue.pause(&ids);
        ids.len()
    }

    pub fn unfinished_session_jobs(&self, session_id: &str) -> usize {
        self.shared
            .queue
            .lock()
            .unwrap()
            .session_job_ids(session_id)
            .len()
    }

    pub fn failed_session_jobs(&self, session_id: &str) -> usize {
        self.shared
            .queue
            .lock()
            .unwrap()
            .session_failures(session_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    First,
    Last,
}

/// Changes the queue, then deletes the temporary files of jobs that left it unfinished and
/// says which folders changed.
fn update_queue(shared: &Arc<Shared>, change: impl FnOnce(&mut Queue)) {
    let abandoned = {
        let mut queue = shared.queue.lock().unwrap();
        change(&mut queue);
        queue.take_abandoned()
    };
    shared.work.notify_waiters();
    if !abandoned.is_empty() {
        let shared = shared.clone();
        crate::runtime::spawn(async move {
            for partial in abandoned {
                match partial.direction {
                    Direction::Download => {
                        let _ = tokio::fs::remove_file(&partial.path).await;
                    }
                    Direction::Upload | Direction::Relay => {
                        discard_with_browsing_session(&shared, &partial).await
                    }
                }
                shared.queue.lock().unwrap().deleted(&partial);
            }
        });
    }
}

/// Whether a download job writes into `folder` or below it.
fn downloads_into(spec: &JobSpec, folder: &Path) -> bool {
    spec.direction == Direction::Download && Path::new(&spec.target).starts_with(folder)
}

impl Drop for TransferManager {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Relaxed);
        self.set_paused(true);
    }
}

/// Counters for the summary line logged when the queue runs dry.
struct Batch {
    started_at: Instant,
    totals: Totals,
    bytes: u64,
}

async fn report_progress(shared: Weak<Shared>) {
    let mut speed_samples: VecDeque<(Instant, u64, u64)> = VecDeque::new();
    let mut last_stats = TransferStats::default();
    let mut batch: Option<Batch> = None;
    let mut persist_due: Option<Instant> = None;
    let mut last_saved = Instant::now();
    loop {
        tokio::time::sleep(TICK).await;
        let Some(shared) = shared.upgrade() else {
            return;
        };
        let now = Instant::now();
        let uploaded = shared.uploaded.load(Ordering::Relaxed);
        let downloaded = shared.downloaded.load(Ordering::Relaxed);
        speed_samples.push_back((now, uploaded, downloaded));
        if speed_samples.len() > SPEED_WINDOW_TICKS {
            speed_samples.pop_front();
        }
        let (oldest_at, oldest_uploaded, oldest_downloaded) = speed_samples[0];
        let window = now.saturating_duration_since(oldest_at).as_secs_f64();
        let per_second = |bytes: u64| {
            if window > 0.0 {
                (bytes as f64 / window) as u64
            } else {
                0
            }
        };

        scheduled_limits::follow(&shared);
        let limits = shared.scheduled_limits.status();
        let (update, stats, totals, has_delayed, unreachable) = {
            let mut queue = shared.queue.lock().unwrap();
            queue.sample_progress(now);
            let (jobs, removed, changed_directories) = queue.take_changes();
            let counts = queue.counts();
            if counts.is_idle() {
                queue.conflict_override = None;
                queue.forget_outages();
            }
            let stats = TransferStats {
                counts,
                remaining_bytes: queue.remaining_bytes(),
                upload_speed: per_second(uploaded - oldest_uploaded),
                download_speed: per_second(downloaded - oldest_downloaded),
                queue_paused: queue.paused,
                limits,
            };
            if stats.counts.is_idle() {
                let mut targets = shared.targets.lock().unwrap();
                targets.retain(|session_id, _| queue.has_session_jobs(session_id));
            }
            if queue.unsaved {
                persist_due.get_or_insert(now);
            }
            let changed = !jobs.is_empty()
                || !removed.is_empty()
                || !changed_directories.is_empty()
                || stats != last_stats;
            // Numbered under the lock, so a list read meanwhile tells which updates it holds.
            let update = changed.then(|| {
                queue.sequence += 1;
                TransferUpdate {
                    sequence: queue.sequence,
                    jobs,
                    removed,
                    changed_directories,
                    stats: stats.clone(),
                }
            });
            (
                update,
                stats,
                queue.totals,
                queue.has_delayed_jobs(),
                queue.unreachable_servers(),
            )
        };

        let busy = stats.counts.running > 0 || stats.counts.queued > 0;
        match (&batch, busy) {
            (None, true) => {
                batch = Some(Batch {
                    started_at: now,
                    totals,
                    bytes: uploaded + downloaded,
                });
            }
            (Some(started), false) if stats.counts.conflict == 0 => {
                if let Some(finished) = summarize(
                    started,
                    totals,
                    uploaded + downloaded,
                    stats.counts.paused,
                    now,
                ) {
                    log_summary(&shared.events, &finished);
                    shared.events.queue_finished(&finished);
                }
                batch = None;
            }
            _ => {}
        }

        if let Some(update) = update {
            last_stats = stats;
            shared.events.transfers(&update);
        }
        if has_delayed {
            shared.work.notify_waiters();
            end_outages_reached_again(&shared).await;
        }
        if !unreachable.is_empty() {
            retry_reached_again(&shared, unreachable).await;
        }
        // Saved at most every few seconds, and while files move, often enough that a crash
        // loses little progress.
        let progress_due = busy && now.saturating_duration_since(last_saved) >= SAVE_PROGRESS_EVERY;
        let change_due =
            persist_due.is_some_and(|since| now.saturating_duration_since(since) >= SAVE_DELAY);
        if change_due || progress_due {
            persist_due = None;
            last_saved = now;
            persist::save_in_background(&shared);
        }
    }
}

/// Lets a server's transfers continue as soon as a new session reaches it, such as a tab
/// reconnecting, instead of at the end of their current delay.
async fn end_outages_reached_again(shared: &Shared) {
    let waiting = shared.queue.lock().unwrap().waiting_servers();
    for (session_id, since) in waiting {
        let Ok(target) = shared.target(&session_id) else {
            continue;
        };
        let profile = target.login().profile;
        let reached = shared
            .sessions
            .find_live(&profile, Some(since))
            .await
            .is_some();
        if reached && shared.queue.lock().unwrap().end_outage(&session_id) {
            shared.events.log(
                LogLevel::Info,
                Some(&session_id),
                format!("Reconnected to {}; its transfers continue", target.label),
            );
            shared.work.notify_waiters();
        }
    }
}

/// Puts transfers that failed because their server stayed out of reach back in line once a new
/// session reaches that server, such as a tab reconnecting.
async fn retry_reached_again(shared: &Shared, servers: Vec<(String, Instant)>) {
    for (session_id, since) in servers {
        let Ok(target) = shared.target(&session_id) else {
            continue;
        };
        let profile = target.login().profile;
        let reached = shared
            .sessions
            .find_live(&profile, Some(since))
            .await
            .is_some();
        if !reached {
            continue;
        }
        let retried = shared.queue.lock().unwrap().retry_unreachable(&session_id);
        if retried > 0 {
            let transfers = if retried == 1 {
                "transfer"
            } else {
                "transfers"
            };
            shared.events.log(
                LogLevel::Info,
                Some(&session_id),
                format!(
                    "Reconnected to {}; {retried} failed {transfers} try again",
                    target.label
                ),
            );
            shared.work.notify_waiters();
        }
    }
}

/// Deletes a server-side temporary file through the session the user browses with, if open.
pub(super) async fn discard_with_browsing_session(shared: &Shared, partial: &Abandoned) {
    let Ok(target) = shared.target(&partial.session_id) else {
        return;
    };
    target.adopt_live_session(&shared.sessions).await;
    let files = match target.browsing_files() {
        Ok(files) => files,
        Err(_) => match shared.sessions.get(&target.login().browsing_session).await {
            Ok(session) => session.files(),
            Err(_) => return,
        },
    };
    let _ = files.delete(std::slice::from_ref(&partial.path)).await;
}

/// The first line of an rsync command, trimmed; `None` when that is blank.
fn one_line(command: &str) -> Option<String> {
    let line = command.lines().next().unwrap_or("").trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// `None` when the batch settled no file.
fn summarize(
    batch: &Batch,
    totals: Totals,
    bytes: u64,
    paused: u32,
    now: Instant,
) -> Option<QueueFinished> {
    let done = totals.done - batch.totals.done;
    let failed = totals.failed - batch.totals.failed;
    let skipped = totals.skipped - batch.totals.skipped;
    if done + failed + skipped == 0 {
        return None;
    }
    Some(QueueFinished {
        done,
        failed,
        skipped,
        bytes: bytes.saturating_sub(batch.bytes),
        elapsed_millis: now.saturating_duration_since(batch.started_at).as_millis() as u64,
        paused,
    })
}

fn log_summary(events: &Events, finished: &QueueFinished) {
    let QueueFinished {
        done,
        failed,
        skipped,
        ..
    } = *finished;
    let mut message = format!(
        "Transfers finished: {done} {} ({}) in {}",
        if done == 1 { "file" } else { "files" },
        format_size(finished.bytes),
        format_duration(Duration::from_millis(finished.elapsed_millis))
    );
    if skipped > 0 {
        message.push_str(&format!(", {skipped} skipped"));
    }
    if failed > 0 {
        message.push_str(&format!(", {failed} failed"));
    }
    let level = if failed > 0 {
        LogLevel::Warn
    } else {
        LogLevel::Info
    };
    events.log(level, None, message);
}

#[cfg(test)]
mod tests {
    use super::queue::{Release, RunOutcome};
    use super::*;
    use crate::ssh::proxy::{Proxy, ProxyKind};
    use crate::ssh::{AuthMethod, Route};

    fn login() -> Login {
        Login {
            profile: ConnectProfile {
                protocol: Protocol::Sftp,
                host: "example.com".into(),
                port: 22,
                username: "me".into(),
                auth: AuthMethod::Agent,
                initial_path: None,
                timeout_secs: None,
                keepalive_secs: None,
                compression: false,
                receive_buffer_kib: None,
                send_buffer_kib: None,
                saved_connection_id: None,
                ftp_active: false,
                bypass_proxy: false,
                jump_connection_id: None,
                rsync_path: None,
                s3: Default::default(),
                route: Route::default(),
            },
            host_key_fingerprint: String::new(),
            browsing_session: "restored".into(),
            browsing_files: None,
            vault: None,
        }
    }

    fn counting_resolver(calls: Arc<AtomicU64>) -> RouteResolver {
        Arc::new(move |profile: &mut ConnectProfile| {
            calls.fetch_add(1, Ordering::Relaxed);
            profile.route.proxy = Some(Proxy {
                kind: ProxyKind::Socks5,
                host: "proxy.local".into(),
                port: 1080,
                username: String::new(),
                password: String::new(),
                remote_dns: true,
            });
            Ok(())
        })
    }

    #[tokio::test]
    async fn restored_targets_work_out_their_route_once() {
        let calls = Arc::new(AtomicU64::new(0));
        let resolver = counting_resolver(calls.clone());
        let target = Arc::new(SessionTarget::new("restored".into(), login(), true));
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let target = target.clone();
                let resolver = resolver.clone();
                tokio::spawn(async move { target.resolve_restored_route(Some(&resolver)).await })
            })
            .collect();
        for worker in workers {
            worker.await.unwrap().unwrap();
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(target.login().profile.route.proxy.is_some());
    }

    #[test]
    fn a_site_rsync_command_is_one_trimmed_line() {
        assert_eq!(
            one_line("  sudo rsync \nrm -rf ~"),
            Some("sudo rsync".into())
        );
        assert_eq!(one_line(" \n rsync"), None);
        assert_eq!(one_line(""), None);
    }

    #[test]
    fn an_unreadable_saved_queue_is_kept_and_reported() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("transfers.json");
        std::fs::write(&file, "{\"servers\": [], \"jobs\": [").unwrap();
        let events = Events::keeping_startup_log();
        let sessions = Arc::new(SessionManager::new(
            temp_dir.path().join("known_hosts"),
            events.clone(),
        ));

        let manager = TransferManager::new(sessions, events.clone(), TransferSettings::default());
        manager.keep_queue_in(file.clone());
        let notices = events.take_startup_log();
        assert_eq!(notices.len(), 1);
        assert!(matches!(notices[0].level, LogLevel::Warn));
        assert!(notices[0].message.contains("transfers.json is not valid"));
        let kept = crate::storage::tests::kept_copies(temp_dir.path(), "transfers.json.corrupt-");
        assert_eq!(kept.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&kept[0]).unwrap(),
            "{\"servers\": [], \"jobs\": ["
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_save_at_exit_keeps_the_latest_queue_while_a_background_save_is_written() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("transfers.json");
        let events = Events::default();
        let sessions = Arc::new(SessionManager::new(
            temp_dir.path().join("known_hosts"),
            events.clone(),
        ));
        let manager = TransferManager::new(sessions, events, TransferSettings::default());
        manager.keep_queue_in(file.clone());
        manager.shared.targets.lock().unwrap().insert(
            "session".into(),
            Arc::new(SessionTarget::new("session".into(), login(), false)),
        );
        let spec = |index: usize| JobSpec {
            session_id: "session".into(),
            source_session_id: None,
            direction: Direction::Upload,
            kind: JobKind::File,
            name: format!("{index}.bin"),
            source: format!("/local/{index}.bin"),
            target: format!("/remote/{index}.bin"),
            target_directory: "/remote".into(),
            size: 10,
            ancestors: Arc::from([]),
            options: None,
        };
        // Large enough that writing the queue takes a while.
        let mut queued = 20_000;
        {
            let mut queue = manager.shared.queue.lock().unwrap();
            queue.paused = true;
            for index in 0..queued {
                queue.add(spec(index));
            }
        }
        for _ in 0..10 {
            persist::save_in_background(&manager.shared);
            // Poros quits while that save is written, after one more change.
            manager.shared.queue.lock().unwrap().add(spec(queued));
            queued += 1;
            persist::save(&manager.shared);
            while manager.shared.saving.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let saved = crate::storage::read_json::<serde_json::Value>(&file);
            let jobs = saved.unwrap().unwrap()["jobs"].as_array().unwrap().len();
            assert_eq!(jobs, queued);
        }
    }

    #[tokio::test]
    async fn a_watch_tells_jobs_removed_before_they_ran_from_copied_ones() {
        let temp_dir = tempfile::tempdir().unwrap();
        let events = Events::default();
        let sessions = Arc::new(SessionManager::new(
            temp_dir.path().join("known_hosts"),
            events.clone(),
        ));
        let settings = TransferSettings {
            keep_completed: false,
            ..TransferSettings::default()
        };
        let manager = TransferManager::new(sessions, events, settings.clone());
        let spec = |name: &str| JobSpec {
            session_id: "session".into(),
            source_session_id: None,
            direction: Direction::Upload,
            kind: JobKind::File,
            name: name.into(),
            source: format!("/local/{name}"),
            target: format!("/remote/{name}"),
            target_directory: "/remote".into(),
            size: 10,
            ancestors: Arc::from([]),
            options: None,
        };
        // Under one lock, so no worker gets to the jobs.
        let ids: Vec<JobId> = {
            let mut queue = manager.shared.queue.lock().unwrap();
            let ids: Vec<JobId> = ["copied", "already there", "removed"]
                .into_iter()
                .map(|name| queue.add_resolved(spec(name), ExistsAction::Overwrite))
                .collect();
            queue.watch(&ids);
            queue.remove(&ids[2..]);
            for outcome in [
                RunOutcome::Completed,
                RunOutcome::Skipped("Already exists".into()),
            ] {
                let claim = queue.claim(Instant::now()).unwrap();
                assert!(matches!(queue.release(claim.id, &outcome), Release::Settle));
                queue.settle(claim.id, outcome, &settings, Instant::now());
            }
            assert_eq!(queue.jobs().count(), 0);
            ids
        };
        let outcome = manager.watch().wait(&ids).await;
        assert_eq!(
            outcome,
            JobsOutcome {
                done: 1,
                failed: 0,
                skipped: 1,
                removed: 1,
                bytes: 10,
            }
        );
    }

    #[tokio::test]
    async fn live_targets_keep_the_route_of_their_session() {
        let calls = Arc::new(AtomicU64::new(0));
        let target = SessionTarget::new("live".into(), login(), false);
        target
            .resolve_restored_route(Some(&counting_resolver(calls.clone())))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert!(target.login().profile.route.proxy.is_none());
    }
}
