//! The transfer queue, in the style of SmartFTP: several workers, each on its own SSH
//! connection, take jobs in queue order. Many small files spread across the workers, and idle
//! workers join a large file to move separate parts of it at once.

mod conflict;
mod copy;
mod delta;
mod limiter;
mod pieces;
mod queue;
mod worker;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

pub use conflict::ExistsAction;
use queue::Totals;
pub use queue::{
    ChangedDirectory, Direction, JobId, JobKind, JobSnapshot, JobState, QueueCounts, Side,
};

use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel};
use crate::format::{format_duration, format_size};
use crate::session::{Session, SessionManager};
use crate::settings::TransferSettings;
use crate::ssh::ConnectProfile;
use crate::{local, remote_path};
use limiter::RateLimiter;
use queue::{JobSpec, Queue};

const TICK: Duration = Duration::from_millis(250);
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
    pub session_id: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferList {
    pub jobs: Vec<JobSnapshot>,
    pub stats: TransferStats,
}

/// What workers need to reach a session's server, kept after the browsing session closes so
/// queued transfers can still run.
pub(crate) struct SessionTarget {
    pub session_id: String,
    pub label: String,
    pub profile: ConnectProfile,
    pub host_key_fingerprint: String,
    /// Set once the server refuses extra connections; workers then open channels on the
    /// browsing connection instead.
    pub channels_only: AtomicBool,
    /// Whether the server ran the configured rsync command, once a worker has tried.
    pub rsync: tokio::sync::Mutex<Option<delta::RsyncCheck>>,
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

    fn total_for(&self, direction: Direction) -> &AtomicU64 {
        match direction {
            Direction::Upload => &self.uploaded,
            Direction::Download => &self.downloaded,
        }
    }

    fn limiter_for(&self, direction: Direction) -> &RateLimiter {
        match direction {
            Direction::Upload => &self.upload_limiter,
            Direction::Download => &self.download_limiter,
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
        let mut specs = Vec::with_capacity(request.items.len());
        for item in &request.items {
            local::validate_name(&item.name)?;
            if item.name.contains('/') {
                return Err(AppError::invalid(format!(
                    "\"{}\" is not a valid name",
                    item.name
                )));
            }
            let target = match request.direction {
                Direction::Upload => remote_path::join(&request.target_directory, &item.name),
                Direction::Download => Path::new(&request.target_directory)
                    .join(&item.name)
                    .to_string_lossy()
                    .into_owned(),
            };
            specs.push(JobSpec {
                session_id: request.session_id.clone(),
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
        self.add_jobs(&session, specs, None);
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
        self.add_jobs(&session, specs, Some(ExistsAction::Overwrite));
        Ok(count)
    }

    fn add_jobs(&self, session: &Session, specs: Vec<JobSpec>, resolution: Option<ExistsAction>) {
        self.shared
            .targets
            .lock()
            .unwrap()
            .entry(session.id.clone())
            .or_insert_with(|| {
                Arc::new(SessionTarget {
                    session_id: session.id.clone(),
                    label: session.profile.label(),
                    profile: session.profile.clone(),
                    host_key_fingerprint: session.host_key_fingerprint.clone(),
                    channels_only: AtomicBool::new(false),
                    rsync: tokio::sync::Mutex::new(None),
                })
            });

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
        change(&mut self.shared.queue.lock().unwrap());
        self.shared.work.notify_waiters();
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
                log_summary(&shared.events, started, totals, uploaded + downloaded, now);
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
        }
    }
}

fn log_summary(events: &Events, batch: &Batch, totals: Totals, bytes: u64, now: Instant) {
    let done = totals.done - batch.totals.done;
    let failed = totals.failed - batch.totals.failed;
    let skipped = totals.skipped - batch.totals.skipped;
    if done + failed + skipped == 0 {
        return;
    }
    let elapsed = now.saturating_duration_since(batch.started_at);
    let mut message = format!(
        "Transfers finished: {done} {} ({}) in {}",
        if done == 1 { "file" } else { "files" },
        format_size(bytes.saturating_sub(batch.bytes)),
        format_duration(elapsed)
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
