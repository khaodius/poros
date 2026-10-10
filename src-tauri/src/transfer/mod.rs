//! The transfer queue, in the style of SmartFTP: several workers, each on its own connection,
//! take jobs in queue order. Many small files spread across the workers, and idle workers join
//! a large file to move separate parts of it at once. SFTP has a pipelined path of its own;
//! other protocols, and copies between two servers, stream through `RemoteFileSystem`.

mod conflict;
mod copy;
mod delta;
mod limiter;
mod persist;
mod pieces;
mod queue;
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
use queue::Totals;
pub use queue::{
    ChangedDirectory, Direction, JobId, JobKind, JobSnapshot, JobState, QueueCounts, Side,
};

use crate::checksum::Algorithm;
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferUpdate {
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
    /// The server's checksum command once a worker has looked; `Some(None)` when it has none.
    pub checksum: tokio::sync::Mutex<Option<Option<Algorithm>>>,
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
}

impl Login {
    fn of(session: &Session) -> Self {
        Self {
            profile: session.profile.clone(),
            host_key_fingerprint: session.host_key_fingerprint.clone(),
            browsing_session: session.id.clone(),
            browsing_files: (session.protocol() != Protocol::Sftp).then(|| session.files()),
        }
    }
}

impl SessionTarget {
    fn new(session_id: String, login: Login, restored: bool) -> Self {
        Self {
            session_id,
            label: login.profile.label(),
            login: Mutex::new(login),
            channels_only: AtomicBool::new(false),
            rsync: tokio::sync::Mutex::new(None),
            checksum: tokio::sync::Mutex::new(None),
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

    pub fn protocol(&self) -> Protocol {
        self.login.lock().unwrap().profile.protocol
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
    /// host key.
    pub async fn adopt_live_session(&self, sessions: &SessionManager) {
        let login = self.login();
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
    /// Unset in tests, where restored servers connect directly.
    route_resolver: OnceLock<RouteResolver>,
}

impl Shared {
    fn settings(&self) -> TransferSettings {
        self.settings.read().unwrap().clone()
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
}

pub struct TransferManager {
    shared: Arc<Shared>,
}

impl TransferManager {
    /// Spawns its tasks on Tauri's runtime, so it can be created outside an async context.
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
            uploaded: AtomicU64::new(0),
            downloaded: AtomicU64::new(0),
            live_workers: Mutex::new(BTreeSet::new()),
            stopping: AtomicBool::new(false),
            borrowed_relays: tokio::sync::Mutex::new(()),
            queue_file: OnceLock::new(),
            saving: AtomicBool::new(false),
            route_resolver: OnceLock::new(),
        });
        let manager = Self { shared };
        manager.configure(settings);
        tauri::async_runtime::spawn(report_progress(Arc::downgrade(&manager.shared)));
        manager
    }

    pub fn configure(&self, settings: TransferSettings) {
        let shared = &self.shared;
        shared
            .upload_limiter
            .set_rate(u64::from(settings.upload_limit_kib) * 1024);
        shared
            .download_limiter
            .set_rate(u64::from(settings.download_limit_kib) * 1024);
        let workers = settings.workers as usize;
        *shared.settings.write().unwrap() = settings;
        let mut live = shared.live_workers.lock().unwrap();
        for index in 0..workers {
            if live.insert(index) {
                tauri::async_runtime::spawn(worker::run(shared.clone(), index));
            }
        }
        drop(live);
        // Workers above the new count notice and exit.
        shared.work.notify_waiters();
    }

    pub async fn enqueue(&self, request: EnqueueRequest) -> AppResult<usize> {
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
            });
        }
        if let Some(source_session) = &source_session {
            self.register(source_session);
        }
        self.register(&session);
        self.add_jobs(specs, None);
        Ok(request.items.len())
    }

    /// Queues single files that replace whatever is at their targets.
    pub async fn enqueue_files(
        &self,
        session_id: &str,
        files: Vec<FileTransfer>,
    ) -> AppResult<usize> {
        let session = self.shared.sessions.get(session_id).await?;
        let count = files.len();
        let specs = files
            .into_iter()
            .map(|file| JobSpec {
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
            })
            .collect();
        self.register(&session);
        self.add_jobs(specs, Some(ExistsAction::Overwrite));
        Ok(count)
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

    fn add_jobs(&self, specs: Vec<JobSpec>, resolution: Option<ExistsAction>) {
        let auto_start = self.shared.settings().auto_start;
        {
            let mut queue = self.shared.queue.lock().unwrap();
            if !auto_start && queue.counts().is_idle() {
                queue.paused = true;
            }
            for spec in specs {
                match resolution {
                    Some(resolution) => queue.add_resolved(spec, resolution),
                    None => queue.add(spec),
                };
            }
        }
        self.shared.work.notify_waiters();
    }

    pub fn list(&self) -> TransferList {
        let queue = self.shared.queue.lock().unwrap();
        TransferList {
            jobs: queue.snapshots(),
            stats: TransferStats {
                counts: queue.counts(),
                remaining_bytes: queue.remaining_bytes(),
                queue_paused: queue.paused,
                ..Default::default()
            },
        }
    }

    fn update_queue(&self, change: impl FnOnce(&mut Queue)) {
        let abandoned = {
            let mut queue = self.shared.queue.lock().unwrap();
            change(&mut queue);
            queue.take_abandoned()
        };
        self.shared.work.notify_waiters();
        if !abandoned.is_empty() {
            let shared = self.shared.clone();
            tauri::async_runtime::spawn(async move {
                for partial in abandoned {
                    match partial.direction {
                        Direction::Download => {
                            let _ = tokio::fs::remove_file(&partial.path).await;
                        }
                        Direction::Upload | Direction::Relay => {
                            discard_with_browsing_session(&shared, &partial).await
                        }
                    }
                }
            });
        }
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
        self.update_queue(|queue| queue.set_paused(paused));
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

        let (jobs, removed, changed_directories, stats, totals, has_delayed) = {
            let mut queue = shared.queue.lock().unwrap();
            queue.sample_progress(now);
            let (jobs, removed, changed) = queue.take_changes();
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
            };
            if stats.counts.is_idle() {
                let mut targets = shared.targets.lock().unwrap();
                targets.retain(|session_id, _| queue.has_session_jobs(session_id));
            }
            if queue.unsaved {
                persist_due.get_or_insert(now);
            }
            (
                jobs,
                removed,
                changed,
                stats,
                queue.totals,
                queue.has_delayed_jobs(),
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

        let changed = !jobs.is_empty()
            || !removed.is_empty()
            || !changed_directories.is_empty()
            || stats != last_stats;
        if changed {
            last_stats = stats.clone();
            shared.events.transfers(&TransferUpdate {
                jobs,
                removed,
                changed_directories,
                stats,
            });
        }
        if has_delayed {
            shared.work.notify_waiters();
            end_outages_reached_again(&shared).await;
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
                route: Route::default(),
            },
            host_key_fingerprint: String::new(),
            browsing_session: "restored".into(),
            browsing_files: None,
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
