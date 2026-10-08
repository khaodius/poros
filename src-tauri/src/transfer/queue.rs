//! Queue bookkeeping without I/O: job states, processing order, and which worker does what.
//! Everything here runs under the transfer manager's lock, so methods are synchronous.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::conflict::ExistsAction;
use super::pieces::Pieces;
use crate::error::AppError;
use crate::settings::TransferSettings;
use crate::sftp::RemoteStat;

pub type JobId = u64;

/// Top-level ranks grow down from here for "move to top" and up for everything else.
const RANK_MIDDLE: u32 = 0x8000_0000;
/// The longest wait between attempts to reach a server that dropped.
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Conflict,
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub source_size: u64,
    pub source_modified: Option<i64>,
    pub target_size: u64,
    pub target_modified: Option<i64>,
}

/// What a job moves. Fixed when queued, except `target` and `name`, which a rename changes.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub session_id: String,
    pub direction: Direction,
    pub kind: JobKind,
    pub name: String,
    pub source: String,
    pub target: String,
    pub target_directory: String,
    pub size: u64,
    /// Canonical source folders above this one, to stop at symbolic link loops.
    pub ancestors: Arc<[String]>,
}

#[derive(Debug, Clone)]
pub enum StopReason {
    Pause,
    Remove,
    Requeue,
    Failed(AppError),
}

/// How one worker's part of a job ended.
#[derive(Debug)]
pub enum RunOutcome {
    Completed,
    Skipped(String),
    Conflict(ConflictInfo),
    Expanded(Vec<JobSpec>),
    Failed(AppError),
    Stopped,
}

/// Prepared by the first worker on a file; helpers open the same target.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Where the bytes are written: the target, or a temporary file beside it.
    pub target: String,
    /// The real target when `target` is a temporary file, renamed over it once complete.
    pub rename_to: Option<String>,
    /// The source as it was when the transfer started.
    pub source_size: u64,
    pub source_modified: Option<i64>,
    pub source_permissions: Option<u32>,
    /// The permissions of the file a temporary file replaces, kept when the source's are not.
    pub replaced_permissions: Option<u32>,
}

/// Where an interrupted file continues from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePoint {
    /// Bytes before this offset are in place from an earlier run.
    pub offset: u64,
    /// The temporary file holding them; `None` when they are in the target itself.
    pub partial: Option<String>,
    /// The source as it was then. A source that changed since starts over.
    pub source_size: u64,
    pub source_modified: Option<i64>,
}

/// Shared by the workers on one running job.
pub struct JobRun {
    pub cancel: CancellationToken,
    pub transferred: AtomicU64,
    pub pieces: Mutex<Option<Pieces>>,
    pub plan: OnceLock<Plan>,
    /// Set while rsync moves the file, sending only what changed.
    pub delta: AtomicBool,
    /// File data a delta transfer sent over the network.
    pub delta_bytes: AtomicU64,
    /// What was written cannot be trusted, so the next attempt starts from the beginning.
    pub restart: AtomicBool,
    /// Read as the first worker closed its handle: the target of an upload, the source of a
    /// download. Only set when that worker moved the whole file alone.
    pub closing_stat: OnceLock<RemoteStat>,
    max_workers: AtomicU32,
    segmented: AtomicBool,
}

impl JobRun {
    fn new(start: u64) -> Self {
        Self {
            cancel: CancellationToken::new(),
            transferred: AtomicU64::new(start),
            pieces: Mutex::new(None),
            plan: OnceLock::new(),
            delta: AtomicBool::new(false),
            delta_bytes: AtomicU64::new(0),
            restart: AtomicBool::new(false),
            closing_stat: OnceLock::new(),
            max_workers: AtomicU32::new(1),
            segmented: AtomicBool::new(false),
        }
    }

    /// Other workers may have joined, so one worker's view of the file is not the whole.
    pub fn is_segmented(&self) -> bool {
        self.segmented.load(Ordering::Relaxed)
    }

    /// Where this run would continue from if it stopped now.
    fn resume_point(&self) -> Option<ResumePoint> {
        let plan = self.plan.get()?;
        let offset = self.complete_prefix()?;
        Some(ResumePoint {
            offset,
            partial: plan.rename_to.is_some().then(|| plan.target.clone()),
            source_size: plan.source_size,
            source_modified: plan.source_modified,
        })
    }

    pub fn start_pieces(&self, pieces: Pieces) {
        *self.pieces.lock().unwrap() = Some(pieces);
    }

    pub fn claim_piece(&self) -> Option<Range<u64>> {
        self.pieces.lock().unwrap().as_mut()?.claim()
    }

    pub fn complete_piece(&self, piece_start: u64) {
        if let Some(pieces) = self.pieces.lock().unwrap().as_mut() {
            pieces.complete(piece_start);
        }
    }

    pub fn reach_piece(&self, piece_start: u64, offset: u64) {
        if let Some(pieces) = self.pieces.lock().unwrap().as_mut() {
            pieces.reach(piece_start, offset);
        }
    }

    /// The source ended before its listed size.
    pub fn end_at(&self, offset: u64) {
        if let Some(pieces) = self.pieces.lock().unwrap().as_mut() {
            pieces.end_at(offset);
        }
    }

    pub fn end(&self) -> Option<u64> {
        self.pieces.lock().unwrap().as_ref().map(Pieces::end)
    }

    pub fn is_complete(&self) -> bool {
        self.pieces
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(Pieces::is_complete)
    }

    pub fn complete_prefix(&self) -> Option<u64> {
        self.pieces
            .lock()
            .unwrap()
            .as_ref()
            .map(Pieces::complete_prefix)
    }

    pub fn add_progress(&self, bytes: u64) {
        self.transferred.fetch_add(bytes, Ordering::Relaxed);
    }

    fn wants_helper(&self, workers: u32) -> bool {
        workers < self.max_workers.load(Ordering::Relaxed)
            && self
                .pieces
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(Pieces::has_unclaimed)
    }
}

pub struct Job {
    pub id: JobId,
    pub rank: String,
    pub spec: JobSpec,
    pub state: JobState,
    pub error: Option<String>,
    pub conflict: Option<ConflictInfo>,
    /// The user's answer to a conflict on this job.
    pub resolution: Option<ExistsAction>,
    pub attempts: u32,
    pub not_before: Option<Instant>,
    pub resume: Option<ResumePoint>,
    /// Waiting for its server to come back after the connection dropped.
    pub reconnecting: bool,
    pub transferred: u64,
    pub speed: u64,
    /// Bytes sent over the network when rsync moved only the changes.
    pub delta_bytes: Option<u64>,
    run: Option<Arc<JobRun>>,
    workers: u32,
    stop: Option<StopReason>,
    sample: Option<(Instant, u64)>,
}

impl Job {
    fn is_finished(&self) -> bool {
        matches!(
            self.state,
            JobState::Done | JobState::Skipped | JobState::Failed
        )
    }

    pub fn snapshot(&self) -> JobSnapshot {
        JobSnapshot {
            id: self.id,
            rank: self.rank.clone(),
            session_id: self.spec.session_id.clone(),
            direction: self.spec.direction,
            kind: self.spec.kind,
            name: self.spec.name.clone(),
            source: self.spec.source.clone(),
            target: self.spec.target.clone(),
            target_directory: self.spec.target_directory.clone(),
            size: self.spec.size,
            transferred: self.transferred,
            speed: if self.state == JobState::Running {
                self.speed
            } else {
                0
            },
            connections: self.workers,
            state: self.state,
            error: self.error.clone(),
            conflict: self.conflict.clone(),
            attempts: self.attempts,
            delta_bytes: self.delta_bytes,
            reconnecting: self.reconnecting,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: JobId,
    /// Sorts jobs in processing order.
    pub rank: String,
    pub session_id: String,
    pub direction: Direction,
    pub kind: JobKind,
    pub name: String,
    pub source: String,
    pub target: String,
    pub target_directory: String,
    pub size: u64,
    pub transferred: u64,
    pub speed: u64,
    pub connections: u32,
    pub state: JobState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<ConflictInfo>,
    pub attempts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_bytes: Option<u64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reconnecting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Local,
    Remote,
}

/// A folder whose contents a transfer changed, so panes showing it can refresh.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedDirectory {
    pub side: Side,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueCounts {
    pub queued: u32,
    pub running: u32,
    pub paused: u32,
    pub conflict: u32,
    pub done: u32,
    pub skipped: u32,
    pub failed: u32,
}

impl QueueCounts {
    pub fn is_idle(&self) -> bool {
        self.queued == 0 && self.running == 0 && self.conflict == 0
    }
}

/// Files settled since the app started, including ones no longer listed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Totals {
    pub done: u64,
    pub skipped: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Primary,
    Helper,
}

pub struct Claim {
    pub id: JobId,
    pub role: Role,
    pub run: Arc<JobRun>,
    pub spec: JobSpec,
    pub resolution: Option<ExistsAction>,
    pub resume: Option<ResumePoint>,
}

pub enum Release {
    /// Other workers are still on the job.
    Others,
    /// This was the last worker and every byte is in place: finalize, then `settle`.
    Finalize,
    /// This was the last worker: tidy up, then `settle`.
    Settle,
}

/// A change in whether a job's server can be reached, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerChange {
    /// A connection dropped; the server's transfers wait this long, then try again.
    Lost { retry_in: Duration },
    /// The server answered again.
    Back,
    /// The server stayed out of reach for the whole reconnect window, so its transfers failed.
    GaveUp { failed: usize },
}

/// A server whose connections dropped. Its jobs wait instead of using up their retries.
struct Outage {
    since: Instant,
    /// Its jobs are not started before this.
    until: Instant,
    failures: u32,
    /// Waiting ran out; its jobs fail like any other until something gets through.
    gave_up: bool,
}

/// A temporary file of a transfer that left the queue unfinished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abandoned {
    pub session_id: String,
    pub direction: Direction,
    pub path: String,
}

/// An unfinished job as it is saved between runs of the app.
#[derive(Debug, Clone)]
pub struct Unfinished {
    pub spec: JobSpec,
    pub state: JobState,
    pub error: Option<String>,
    pub resolution: Option<ExistsAction>,
    pub resume: Option<ResumePoint>,
}

#[derive(Default)]
pub struct Queue {
    jobs: HashMap<JobId, Job>,
    pending: BTreeMap<String, JobId>,
    segmented: Vec<JobId>,
    next_id: JobId,
    top_rank: u32,
    bottom_rank: u32,
    pub paused: bool,
    pub conflict_override: Option<ExistsAction>,
    pub totals: Totals,
    dirty: HashSet<JobId>,
    removed: Vec<JobId>,
    changed_directories: Vec<ChangedDirectory>,
    outages: HashMap<String, Outage>,
    abandoned: Vec<Abandoned>,
    /// Jobs were added, removed, reordered or changed state since the queue was last saved.
    pub unsaved: bool,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            top_rank: RANK_MIDDLE,
            bottom_rank: RANK_MIDDLE,
            ..Default::default()
        }
    }

    pub fn add(&mut self, spec: JobSpec) -> JobId {
        self.bottom_rank += 1;
        let rank = rank_segment(self.bottom_rank);
        self.insert(rank, spec)
    }

    /// Queues a job that already knows what to do if its target exists.
    pub fn add_resolved(&mut self, spec: JobSpec, resolution: ExistsAction) -> JobId {
        let id = self.add(spec);
        if let Some(job) = self.jobs.get_mut(&id) {
            job.resolution = Some(resolution);
        }
        id
    }

    fn insert(&mut self, rank: String, spec: JobSpec) -> JobId {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(rank.clone(), id);
        self.jobs.insert(
            id,
            Job {
                id,
                rank,
                spec,
                state: JobState::Queued,
                error: None,
                conflict: None,
                resolution: None,
                attempts: 0,
                not_before: None,
                resume: None,
                reconnecting: false,
                transferred: 0,
                speed: 0,
                delta_bytes: None,
                run: None,
                workers: 0,
                stop: None,
                sample: None,
            },
        );
        self.dirty.insert(id);
        self.unsaved = true;
        id
    }

    /// Brings back a job saved by an earlier run of the app. Jobs that were waiting or running
    /// come back paused, so nothing starts until the user says so.
    pub fn restore(&mut self, saved: Unfinished) -> JobId {
        let id = self.add(saved.spec);
        if let Some(job) = self.jobs.get_mut(&id) {
            self.pending.remove(&job.rank);
            job.state = match saved.state {
                JobState::Failed => JobState::Failed,
                _ => JobState::Paused,
            };
            job.error = saved.error;
            job.resolution = saved.resolution;
            job.transferred = saved.resume.as_ref().map_or(0, |resume| resume.offset);
            job.resume = saved.resume;
        }
        id
    }

    /// Every job not yet done, in queue order; running ones with where they would resume.
    pub fn unfinished(&self) -> Vec<Unfinished> {
        let mut jobs: Vec<&Job> = self
            .jobs
            .values()
            .filter(|job| !matches!(job.state, JobState::Done | JobState::Skipped))
            .collect();
        jobs.sort_by(|left, right| left.rank.cmp(&right.rank));
        jobs.into_iter()
            .map(|job| Unfinished {
                spec: job.spec.clone(),
                state: job.state,
                error: job.error.clone(),
                resolution: job.resolution,
                resume: job
                    .run
                    .as_ref()
                    .and_then(|run| run.resume_point())
                    .or_else(|| job.resume.clone()),
            })
            .collect()
    }

    pub fn job(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn has_session_jobs(&self, session_id: &str) -> bool {
        self.jobs
            .values()
            .any(|job| job.spec.session_id == session_id)
    }

    pub fn claim(&mut self, now: Instant) -> Option<Claim> {
        if self.paused {
            return None;
        }
        for &id in &self.segmented {
            let Some(job) = self.jobs.get_mut(&id) else {
                continue;
            };
            let Some(run) = job.run.clone() else {
                continue;
            };
            if job.stop.is_none() && run.wants_helper(job.workers) {
                job.workers += 1;
                self.dirty.insert(id);
                return Some(Claim {
                    id,
                    role: Role::Helper,
                    run,
                    spec: job.spec.clone(),
                    resolution: None,
                    resume: None,
                });
            }
        }

        let (rank, id) = self
            .pending
            .iter()
            .find(|(_, id)| {
                let job = &self.jobs[id];
                job.not_before.is_none_or(|not_before| not_before <= now)
                    && !self.server_unreachable(&job.spec.session_id, now)
            })
            .map(|(rank, id)| (rank.clone(), *id))?;
        self.pending.remove(&rank);
        let job = self.jobs.get_mut(&id)?;
        let start = job.resume.as_ref().map_or(0, |resume| resume.offset);
        let run = Arc::new(JobRun::new(start));
        self.unsaved = true;
        job.state = JobState::Running;
        job.run = Some(run.clone());
        job.workers = 1;
        job.stop = None;
        job.not_before = None;
        job.conflict = None;
        job.speed = 0;
        job.delta_bytes = None;
        job.reconnecting = false;
        job.sample = Some((now, start));
        self.dirty.insert(id);
        Some(Claim {
            id,
            role: Role::Primary,
            run,
            spec: job.spec.clone(),
            resolution: job.resolution,
            resume: job.resume.clone(),
        })
    }

    fn server_unreachable(&self, session_id: &str, now: Instant) -> bool {
        self.outages
            .get(session_id)
            .is_some_and(|outage| !outage.gave_up && now < outage.until)
    }

    /// The first worker prepared the target; up to `max_workers` may now share the file.
    pub fn open_segments(&mut self, id: JobId, max_workers: u32) {
        if let Some(run) = self.jobs.get(&id).and_then(|job| job.run.as_ref()) {
            run.max_workers.store(max_workers, Ordering::Relaxed);
            run.segmented.store(true, Ordering::Relaxed);
            self.segmented.push(id);
        }
    }

    /// A rename resolved a conflict.
    pub fn retarget(&mut self, id: JobId, target: String, name: String) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.spec.target = target;
            job.spec.name = name;
            self.dirty.insert(id);
            self.unsaved = true;
        }
    }

    /// Called by each worker leaving a job. Exactly one caller gets `Finalize` or `Settle`.
    pub fn release(&mut self, id: JobId, outcome: &RunOutcome) -> Release {
        let Some(job) = self.jobs.get_mut(&id) else {
            return Release::Others;
        };
        if let RunOutcome::Failed(error) = outcome {
            if job.stop.is_none() {
                job.stop = Some(StopReason::Failed(error.clone()));
            }
            if let Some(run) = &job.run {
                run.cancel.cancel();
            }
        }
        job.workers = job.workers.saturating_sub(1);
        self.dirty.insert(id);
        if job.workers > 0 {
            return Release::Others;
        }
        let finished_cleanly = job.stop.is_none()
            && matches!(outcome, RunOutcome::Completed)
            && job.run.as_ref().is_some_and(|run| run.is_complete());
        if finished_cleanly {
            Release::Finalize
        } else {
            Release::Settle
        }
    }

    /// Decides a job's state once no worker is on it, and reports whether that changed what
    /// is known about its server.
    pub fn settle(
        &mut self,
        id: JobId,
        outcome: RunOutcome,
        settings: &TransferSettings,
        now: Instant,
    ) -> Option<ServerChange> {
        self.segmented.retain(|segmented| *segmented != id);
        self.unsaved = true;
        let job = self.jobs.get_mut(&id)?;
        let run = job.run.take();
        let stop = job.stop.take();
        job.workers = 0;
        job.speed = 0;
        job.sample = None;
        if let Some(run) = &run {
            job.transferred = run.transferred.load(Ordering::Relaxed);
            if run.restart.load(Ordering::Relaxed) {
                job.resume = None;
                job.transferred = 0;
            } else if let Some(resume) = run.resume_point() {
                job.resume = Some(resume);
            }
            job.delta_bytes = run
                .delta
                .load(Ordering::Relaxed)
                .then(|| run.delta_bytes.load(Ordering::Relaxed));
        }
        self.dirty.insert(id);
        let session_id = job.spec.session_id.clone();

        let failure = match (stop, outcome) {
            (Some(StopReason::Remove), _) => {
                self.forget(id);
                return None;
            }
            (_, RunOutcome::Completed) => {
                self.totals.done += 1;
                job.state = JobState::Done;
                job.error = None;
                job.resume = None;
                if let Some(end) = run.as_ref().and_then(|run| run.end()) {
                    job.spec.size = end;
                    job.transferred = end;
                }
                let changed = changed_directory(&job.spec);
                self.changed_directories.push(changed);
                if !settings.keep_completed {
                    self.forget(id);
                }
                return self.server_answered(&session_id);
            }
            (_, RunOutcome::Skipped(reason)) => {
                self.totals.skipped += 1;
                job.state = JobState::Skipped;
                job.error = Some(reason);
                if !settings.keep_completed {
                    self.forget(id);
                }
                return self.server_answered(&session_id);
            }
            (_, RunOutcome::Expanded(children)) => {
                let rank = job.rank.clone();
                let changed = changed_directory(&job.spec);
                self.changed_directories.push(changed);
                self.forget(id);
                for (index, child) in children.into_iter().enumerate() {
                    let child_rank = format!("{rank}{}", rank_segment(index as u32));
                    self.insert(child_rank, child);
                }
                return self.server_answered(&session_id);
            }
            (Some(StopReason::Pause), _) => {
                job.state = JobState::Paused;
                return None;
            }
            (Some(StopReason::Requeue), _) | (None, RunOutcome::Stopped) => {
                job.state = JobState::Queued;
                self.pending.insert(job.rank.clone(), id);
                return None;
            }
            (None, RunOutcome::Conflict(info)) => {
                job.state = JobState::Conflict;
                job.conflict = Some(info);
                return self.server_answered(&session_id);
            }
            (Some(StopReason::Failed(error)), _) | (None, RunOutcome::Failed(error)) => error,
        };

        if failure.is_connection_lost() {
            if let Some(change) = self.wait_for_server(id, &failure, settings, now) {
                return change;
            }
        }
        let change = if failure.is_connection_lost() {
            None
        } else {
            self.server_answered(&session_id)
        };
        let job = self.jobs.get_mut(&id)?;
        job.attempts += 1;
        if failure.is_retryable() && job.attempts <= settings.retry_attempts {
            job.state = JobState::Queued;
            job.error = Some(format!("{} (retrying)", failure.message));
            job.not_before = Some(now + Duration::from_secs(u64::from(settings.retry_delay_secs)));
            self.pending.insert(job.rank.clone(), id);
        } else {
            self.totals.failed += 1;
            job.state = JobState::Failed;
            job.error = Some(failure.message);
        }
        change
    }

    /// Puts a job whose connection dropped back in line to wait for its server, without using
    /// up its retries. The drop that starts an outage does count, so a file that itself breaks
    /// the connection still fails in the end. `None` when the job should fail or retry as usual;
    /// otherwise what changed about the server, if anything.
    fn wait_for_server(
        &mut self,
        id: JobId,
        failure: &AppError,
        settings: &TransferSettings,
        now: Instant,
    ) -> Option<Option<ServerChange>> {
        if settings.retry_attempts == 0 || settings.reconnect_minutes == 0 {
            return None;
        }
        let session_id = self.jobs.get(&id)?.spec.session_id.clone();
        let delay = |failures: u32| {
            let first = Duration::from_secs(u64::from(settings.retry_delay_secs.max(1)));
            first
                .saturating_mul(1 << failures.saturating_sub(1).min(16))
                .min(MAX_RECONNECT_DELAY.max(first))
        };
        let change = match self.outages.get_mut(&session_id) {
            None => {
                let job = self.jobs.get_mut(&id)?;
                if job.attempts >= settings.retry_attempts {
                    return None;
                }
                job.attempts += 1;
                let retry_in = delay(1);
                self.outages.insert(
                    session_id.clone(),
                    Outage {
                        since: now,
                        until: now + retry_in,
                        failures: 1,
                        gave_up: false,
                    },
                );
                Some(ServerChange::Lost { retry_in })
            }
            Some(outage) if outage.gave_up => return None,
            // This attempt began before the outage did.
            Some(outage) if now < outage.until => None,
            Some(outage)
                if now.saturating_duration_since(outage.since) < settings.reconnect_window() =>
            {
                outage.failures += 1;
                let retry_in = delay(outage.failures);
                outage.until = now + retry_in;
                Some(ServerChange::Lost { retry_in })
            }
            Some(outage) => {
                outage.gave_up = true;
                let message = format!(
                    "{} (gave up after trying to reconnect for {} min)",
                    failure.message, settings.reconnect_minutes
                );
                let failed = self.fail_waiting(&session_id, &message) + 1;
                let job = self.jobs.get_mut(&id)?;
                self.totals.failed += 1;
                job.state = JobState::Failed;
                job.reconnecting = false;
                job.error = Some(message);
                return Some(Some(ServerChange::GaveUp { failed }));
            }
        };
        let job = self.jobs.get_mut(&id)?;
        job.state = JobState::Queued;
        job.reconnecting = true;
        job.not_before = None;
        job.error = Some(format!("{} (reconnecting)", failure.message));
        self.pending.insert(job.rank.clone(), id);
        Some(change)
    }

    /// Fails every queued job of a session; returns how many there were.
    fn fail_waiting(&mut self, session_id: &str, message: &str) -> usize {
        let waiting: Vec<(String, JobId)> = self
            .pending
            .iter()
            .filter(|(_, id)| self.jobs[*id].spec.session_id == session_id)
            .map(|(rank, id)| (rank.clone(), *id))
            .collect();
        for (rank, id) in &waiting {
            self.pending.remove(rank);
            if let Some(job) = self.jobs.get_mut(id) {
                job.state = JobState::Failed;
                job.reconnecting = false;
                job.error = Some(message.to_string());
                self.dirty.insert(*id);
            }
        }
        self.totals.failed += waiting.len() as u64;
        waiting.len()
    }

    /// Servers whose transfers wait after a dropped connection, with when that began.
    pub fn waiting_servers(&self) -> Vec<(String, Instant)> {
        self.outages
            .iter()
            .filter(|(_, outage)| !outage.gave_up)
            .map(|(session_id, outage)| (session_id.clone(), outage.since))
            .collect()
    }

    /// A new session reached the server, so its transfers need not wait out their delay.
    pub fn end_outage(&mut self, session_id: &str) -> bool {
        let ended = self
            .outages
            .get(session_id)
            .is_some_and(|outage| !outage.gave_up);
        if ended {
            self.outages.remove(session_id);
        }
        ended
    }

    /// Something got through to the server, so it is no longer out of reach.
    fn server_answered(&mut self, session_id: &str) -> Option<ServerChange> {
        self.outages.remove(session_id).map(|_| ServerChange::Back)
    }

    fn forget(&mut self, id: JobId) {
        if let Some(job) = self.jobs.remove(&id) {
            self.pending.remove(&job.rank);
            self.dirty.remove(&id);
            self.removed.push(id);
            self.unsaved = true;
            if let Some(partial) = job.resume.and_then(|resume| resume.partial) {
                self.abandoned.push(Abandoned {
                    session_id: job.spec.session_id,
                    direction: job.spec.direction,
                    path: partial,
                });
            }
        }
    }

    /// Temporary files of jobs that left the queue, for the caller to delete.
    pub fn take_abandoned(&mut self) -> Vec<Abandoned> {
        std::mem::take(&mut self.abandoned)
    }

    pub fn pause(&mut self, ids: &[JobId]) {
        for id in ids {
            let Some(job) = self.jobs.get_mut(id) else {
                continue;
            };
            match job.state {
                JobState::Queued | JobState::Conflict => {
                    self.pending.remove(&job.rank);
                    job.state = JobState::Paused;
                    job.not_before = None;
                    job.reconnecting = false;
                }
                JobState::Running => stop_run(job, StopReason::Pause),
                _ => continue,
            }
            self.dirty.insert(*id);
            self.unsaved = true;
        }
    }

    /// Paused jobs continue; failed and skipped ones start over with a fresh retry budget.
    pub fn resume(&mut self, ids: &[JobId]) {
        for id in ids {
            let Some(job) = self.jobs.get_mut(id) else {
                continue;
            };
            match job.state {
                JobState::Paused => {}
                JobState::Failed | JobState::Skipped => {
                    job.attempts = 0;
                    job.error = None;
                }
                _ => continue,
            }
            job.state = JobState::Queued;
            job.not_before = None;
            self.pending.insert(job.rank.clone(), *id);
            self.dirty.insert(*id);
            self.unsaved = true;
            // Asking again is a fresh start for a server Poros had given up on.
            let session_id = &job.spec.session_id;
            if self
                .outages
                .get(session_id)
                .is_some_and(|outage| outage.gave_up)
            {
                self.outages.remove(session_id);
            }
        }
    }

    pub fn remove(&mut self, ids: &[JobId]) {
        for id in ids {
            let Some(job) = self.jobs.get_mut(id) else {
                continue;
            };
            if job.state == JobState::Running {
                stop_run(job, StopReason::Remove);
                self.dirty.insert(*id);
            } else {
                self.forget(*id);
            }
        }
    }

    pub fn clear(&mut self, states: &[JobState]) {
        let ids: Vec<JobId> = self
            .jobs
            .values()
            .filter(|job| job.state != JobState::Running && states.contains(&job.state))
            .map(|job| job.id)
            .collect();
        for id in ids {
            self.forget(id);
        }
    }

    pub fn session_job_ids(&self, session_id: &str) -> Vec<JobId> {
        self.jobs
            .values()
            .filter(|job| job.spec.session_id == session_id && !job.is_finished())
            .map(|job| job.id)
            .collect()
    }

    /// Keeps the given order among the moved jobs.
    pub fn move_to(&mut self, ids: &[JobId], to_top: bool) {
        let mut ordered: Vec<&Job> = ids.iter().filter_map(|id| self.jobs.get(id)).collect();
        ordered.sort_by(|left, right| left.rank.cmp(&right.rank));
        let ordered: Vec<JobId> = ordered.into_iter().map(|job| job.id).collect();
        let ranks: Vec<u32> = if to_top {
            let first = self.top_rank - ordered.len() as u32;
            self.top_rank = first;
            (first..first + ordered.len() as u32).collect()
        } else {
            let first = self.bottom_rank + 1;
            self.bottom_rank += ordered.len() as u32;
            (first..=self.bottom_rank).collect()
        };
        for (id, rank) in ordered.into_iter().zip(ranks) {
            let Some(job) = self.jobs.get_mut(&id) else {
                continue;
            };
            let was_pending = self.pending.remove(&job.rank).is_some();
            job.rank = rank_segment(rank);
            if was_pending {
                self.pending.insert(job.rank.clone(), id);
            }
            self.dirty.insert(id);
            self.unsaved = true;
        }
    }

    pub fn resolve(&mut self, id: JobId, action: ExistsAction, apply_to_all: bool) {
        let ids: Vec<JobId> = if apply_to_all {
            self.conflict_override = Some(action);
            self.jobs
                .values()
                .filter(|job| job.state == JobState::Conflict)
                .map(|job| job.id)
                .collect()
        } else {
            vec![id]
        };
        for id in ids {
            let Some(job) = self.jobs.get_mut(&id) else {
                continue;
            };
            if job.state != JobState::Conflict {
                continue;
            }
            job.resolution = Some(action);
            job.conflict = None;
            job.state = JobState::Queued;
            self.pending.insert(job.rank.clone(), id);
            self.dirty.insert(id);
            self.unsaved = true;
        }
    }

    /// Pausing the queue interrupts running transfers; they continue from where they stopped.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            for job in self.jobs.values_mut() {
                if job.state == JobState::Running && job.stop.is_none() {
                    stop_run(job, StopReason::Requeue);
                }
            }
        }
    }

    pub fn counts(&self) -> QueueCounts {
        let mut counts = QueueCounts::default();
        for job in self.jobs.values() {
            let counter = match job.state {
                JobState::Queued => &mut counts.queued,
                JobState::Running => &mut counts.running,
                JobState::Paused => &mut counts.paused,
                JobState::Conflict => &mut counts.conflict,
                JobState::Done => &mut counts.done,
                JobState::Skipped => &mut counts.skipped,
                JobState::Failed => &mut counts.failed,
            };
            *counter += 1;
        }
        counts
    }

    /// Bytes still to move for queued and running files.
    pub fn remaining_bytes(&self) -> u64 {
        self.jobs
            .values()
            .filter(|job| matches!(job.state, JobState::Queued | JobState::Running))
            .map(|job| job.spec.size.saturating_sub(job.transferred))
            .sum()
    }

    pub fn sample_progress(&mut self, now: Instant) {
        for job in self.jobs.values_mut() {
            let (Some(run), Some((sampled_at, sampled_bytes))) = (&job.run, job.sample) else {
                continue;
            };
            let transferred = run.transferred.load(Ordering::Relaxed);
            let elapsed = now.saturating_duration_since(sampled_at).as_secs_f64();
            if elapsed <= 0.0 {
                continue;
            }
            let instant_speed = (transferred.saturating_sub(sampled_bytes) as f64 / elapsed) as u64;
            let speed = if job.speed == 0 {
                instant_speed
            } else {
                (job.speed * 2 + instant_speed) / 3
            };
            let delta_bytes = run
                .delta
                .load(Ordering::Relaxed)
                .then(|| run.delta_bytes.load(Ordering::Relaxed));
            if transferred != job.transferred
                || speed != job.speed
                || delta_bytes != job.delta_bytes
            {
                job.transferred = transferred;
                job.speed = speed;
                job.delta_bytes = delta_bytes;
                self.dirty.insert(job.id);
            }
            job.sample = Some((now, transferred));
        }
    }

    pub fn snapshots(&self) -> Vec<JobSnapshot> {
        let mut snapshots: Vec<JobSnapshot> = self.jobs.values().map(Job::snapshot).collect();
        snapshots.sort_by(|left, right| left.rank.cmp(&right.rank));
        snapshots
    }

    pub fn take_changes(&mut self) -> (Vec<JobSnapshot>, Vec<JobId>, Vec<ChangedDirectory>) {
        let jobs = self
            .dirty
            .drain()
            .filter_map(|id| self.jobs.get(&id).map(Job::snapshot))
            .collect();
        let mut changed = std::mem::take(&mut self.changed_directories);
        changed.sort_by(|left, right| left.path.cmp(&right.path));
        changed.dedup();
        (jobs, std::mem::take(&mut self.removed), changed)
    }

    pub fn has_delayed_jobs(&self) -> bool {
        self.pending.values().any(|id| {
            let job = &self.jobs[id];
            job.not_before.is_some() || self.outages.contains_key(&job.spec.session_id)
        })
    }

    /// With nothing left to run, no job waits for a server any more.
    pub fn forget_outages(&mut self) {
        self.outages.clear();
    }
}

fn stop_run(job: &mut Job, reason: StopReason) {
    let replace = match (&job.stop, &reason) {
        (None, _) => true,
        (Some(StopReason::Remove), _) => false,
        (Some(_), StopReason::Remove) => true,
        (Some(StopReason::Requeue), StopReason::Pause) => true,
        _ => false,
    };
    if replace {
        job.stop = Some(reason);
    }
    if let Some(run) = &job.run {
        run.cancel.cancel();
    }
}

fn changed_directory(spec: &JobSpec) -> ChangedDirectory {
    match spec.direction {
        Direction::Download => ChangedDirectory {
            side: Side::Local,
            session_id: None,
            path: spec.target_directory.clone(),
        },
        Direction::Upload => ChangedDirectory {
            side: Side::Remote,
            session_id: Some(spec.session_id.clone()),
            path: spec.target_directory.clone(),
        },
    }
}

/// Fixed-width hex, so ranks compare as strings and a child's rank extends its parent's.
fn rank_segment(value: u32) -> String {
    format!("{value:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn file(name: &str, size: u64) -> JobSpec {
        JobSpec {
            session_id: "session".into(),
            direction: Direction::Upload,
            kind: JobKind::File,
            name: name.into(),
            source: format!("/local/{name}"),
            target: format!("/remote/{name}"),
            target_directory: "/remote".into(),
            size,
            ancestors: Arc::from([]),
        }
    }

    fn folder(name: &str) -> JobSpec {
        JobSpec {
            kind: JobKind::Folder,
            ..file(name, 0)
        }
    }

    fn settings() -> TransferSettings {
        TransferSettings::default()
    }

    fn plan(target: &str, size: u64) -> Plan {
        Plan {
            target: target.into(),
            rename_to: None,
            source_size: size,
            source_modified: Some(1),
            source_permissions: None,
            replaced_permissions: None,
        }
    }

    fn claim_name(queue: &mut Queue) -> Option<String> {
        queue.claim(Instant::now()).map(|claim| claim.spec.name)
    }

    fn finish(queue: &mut Queue, id: JobId, outcome: RunOutcome) {
        assert!(matches!(queue.release(id, &outcome), Release::Settle));
        queue.settle(id, outcome, &settings(), Instant::now());
    }

    #[test]
    fn processes_in_queue_order_and_moves_to_top() {
        let mut queue = Queue::new();
        let first = queue.add(file("a", 1));
        queue.add(file("b", 1));
        let third = queue.add(file("c", 1));
        queue.move_to(&[third, first], true);
        assert_eq!(claim_name(&mut queue).as_deref(), Some("a"));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("c"));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("b"));
        assert_eq!(claim_name(&mut queue), None);
    }

    #[test]
    fn folder_children_take_the_folder_position() {
        let mut queue = Queue::new();
        let folder_id = queue.add(folder("dir"));
        queue.add(file("after", 1));
        let claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(claim.id, folder_id);
        finish(
            &mut queue,
            folder_id,
            RunOutcome::Expanded(vec![file("one", 1), file("two", 1)]),
        );
        assert!(queue.job(folder_id).is_none());
        assert_eq!(claim_name(&mut queue).as_deref(), Some("one"));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("two"));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("after"));
        let (_, removed, changed) = queue.take_changes();
        assert_eq!(removed, vec![folder_id]);
        assert_eq!(changed[0].path, "/remote");
    }

    #[test]
    fn pausing_the_queue_requeues_running_jobs_with_their_progress() {
        let mut queue = Queue::new();
        let id = queue.add(file("big", 100));
        let claim = queue.claim(Instant::now()).unwrap();
        claim.run.plan.set(plan("/remote/big", 100)).unwrap();
        claim.run.start_pieces(Pieces::new(0, 100, 10));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.add_progress(10);
        claim.run.complete_piece(piece.start);

        queue.set_paused(true);
        assert!(claim.run.cancel.is_cancelled());
        finish(&mut queue, id, RunOutcome::Stopped);
        let job = queue.job(id).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.resume.as_ref().map(|resume| resume.offset), Some(10));
        assert!(queue.claim(Instant::now()).is_none());

        queue.set_paused(false);
        let claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(claim.resume.map(|resume| resume.offset), Some(10));
    }

    #[test]
    fn helpers_join_segmented_files_until_pieces_run_out() {
        let mut queue = Queue::new();
        let id = queue.add(file("large", 30));
        queue.add(file("small", 1));
        let primary = queue.claim(Instant::now()).unwrap();
        primary.run.start_pieces(Pieces::new(0, 30, 10));
        queue.open_segments(id, 2);
        primary.run.claim_piece();

        let helper = queue.claim(Instant::now()).unwrap();
        assert_eq!(helper.role, Role::Helper);
        assert_eq!(helper.id, id);
        // The file allows two workers, so the next worker takes the next job.
        assert_eq!(claim_name(&mut queue).as_deref(), Some("small"));

        for piece in [0, 10, 20] {
            if piece > 0 {
                helper.run.claim_piece();
            }
            helper.run.complete_piece(piece);
        }
        assert!(matches!(
            queue.release(id, &RunOutcome::Completed),
            Release::Others
        ));
        assert!(matches!(
            queue.release(id, &RunOutcome::Completed),
            Release::Finalize
        ));
        queue.settle(id, RunOutcome::Completed, &settings(), Instant::now());
        assert_eq!(queue.job(id).unwrap().state, JobState::Done);
    }

    #[test]
    fn retryable_failures_wait_then_requeue_and_others_fail() {
        let mut queue = Queue::new();
        let id = queue.add(file("flaky", 1));
        queue.claim(Instant::now()).unwrap();
        let lost = AppError::new(ErrorKind::Disconnected, "gone");
        finish(&mut queue, id, RunOutcome::Failed(lost));
        let job = queue.job(id).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert!(job.reconnecting);
        assert!(queue.has_delayed_jobs());
        assert!(queue.claim(Instant::now()).is_none());
        assert!(queue
            .claim(Instant::now() + Duration::from_secs(60))
            .is_some());

        let denied = AppError::new(ErrorKind::PermissionDenied, "denied");
        finish(&mut queue, id, RunOutcome::Failed(denied));
        assert_eq!(queue.job(id).unwrap().state, JobState::Failed);
        queue.resume(&[id]);
        assert_eq!(queue.job(id).unwrap().state, JobState::Queued);
        assert_eq!(queue.job(id).unwrap().attempts, 0);
    }

    #[test]
    fn conflicts_resolved_for_all_requeue_every_waiting_job() {
        let mut queue = Queue::new();
        let info = || ConflictInfo {
            source_size: 1,
            source_modified: None,
            target_size: 2,
            target_modified: None,
        };
        let first = queue.add(file("a", 1));
        let second = queue.add(file("b", 1));
        for id in [first, second] {
            queue.claim(Instant::now()).unwrap();
            finish(&mut queue, id, RunOutcome::Conflict(info()));
        }
        assert_eq!(queue.counts().conflict, 2);
        queue.resolve(first, ExistsAction::Overwrite, true);
        assert_eq!(queue.conflict_override, Some(ExistsAction::Overwrite));
        let claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(claim.resolution, Some(ExistsAction::Overwrite));
        assert_eq!(queue.counts().queued, 1);
    }

    #[test]
    fn removing_a_running_job_drops_it_when_its_workers_stop() {
        let mut queue = Queue::new();
        let id = queue.add(file("a", 1));
        let claim = queue.claim(Instant::now()).unwrap();
        queue.remove(&[id]);
        assert!(claim.run.cancel.is_cancelled());
        assert!(queue.job(id).is_some());
        finish(&mut queue, id, RunOutcome::Stopped);
        assert!(queue.job(id).is_none());
    }

    fn drop_connection(
        queue: &mut Queue,
        id: JobId,
        settings: &TransferSettings,
        now: Instant,
    ) -> Option<ServerChange> {
        let lost = RunOutcome::Failed(AppError::new(ErrorKind::Disconnected, "gone"));
        assert!(matches!(queue.release(id, &lost), Release::Settle));
        queue.settle(id, lost, settings, now)
    }

    #[test]
    fn dropped_connections_wait_for_the_server_with_growing_delays() {
        let settings = TransferSettings {
            retry_attempts: 2,
            retry_delay_secs: 5,
            ..settings()
        };
        let mut queue = Queue::new();
        let first = queue.add(file("a", 1));
        let second = queue.add(file("b", 1));
        let start = Instant::now();
        queue.claim(start).unwrap();
        queue.claim(start).unwrap();

        assert_eq!(
            drop_connection(&mut queue, first, &settings, start),
            Some(ServerChange::Lost {
                retry_in: Duration::from_secs(5)
            })
        );
        // The second job lost the same connection, so the outage is already known.
        assert_eq!(drop_connection(&mut queue, second, &settings, start), None);
        assert_eq!(queue.job(first).unwrap().attempts, 1);
        assert_eq!(queue.job(second).unwrap().attempts, 0);
        assert!(queue.claim(start + Duration::from_secs(4)).is_none());

        let later = start + Duration::from_secs(5);
        let claim = queue.claim(later).unwrap();
        assert_eq!(
            drop_connection(&mut queue, claim.id, &settings, later),
            Some(ServerChange::Lost {
                retry_in: Duration::from_secs(10)
            })
        );
        // Waiting on the server does not use up retries.
        assert_eq!(queue.job(first).unwrap().attempts, 1);
        assert!(queue.claim(later + Duration::from_secs(9)).is_none());

        let back = later + Duration::from_secs(10);
        let claim = queue.claim(back).unwrap();
        assert!(matches!(
            queue.release(claim.id, &RunOutcome::Completed),
            Release::Settle
        ));
        assert_eq!(
            queue.settle(claim.id, RunOutcome::Completed, &settings, back),
            Some(ServerChange::Back)
        );
        assert!(queue.claim(back).is_some());
    }

    #[test]
    fn a_new_session_to_the_server_ends_the_wait() {
        let mut queue = Queue::new();
        let id = queue.add(file("a", 1));
        let start = Instant::now();
        queue.claim(start).unwrap();
        drop_connection(&mut queue, id, &settings(), start);
        assert_eq!(
            queue.waiting_servers(),
            vec![("session".to_string(), start)]
        );
        assert!(queue.claim(start).is_none());
        assert!(queue.end_outage("session"));
        assert!(queue.waiting_servers().is_empty());
        assert!(queue.claim(start).is_some());
    }

    #[test]
    fn a_server_out_of_reach_too_long_fails_its_waiting_jobs() {
        let settings = TransferSettings {
            reconnect_minutes: 1,
            ..settings()
        };
        let mut queue = Queue::new();
        let first = queue.add(file("a", 1));
        let second = queue.add(file("b", 1));
        let start = Instant::now();
        queue.claim(start).unwrap();
        drop_connection(&mut queue, first, &settings, start);

        let late = start + Duration::from_secs(61);
        let claim = queue.claim(late).unwrap();
        assert_eq!(claim.id, first);
        assert_eq!(
            drop_connection(&mut queue, first, &settings, late),
            Some(ServerChange::GaveUp { failed: 2 })
        );
        assert_eq!(queue.job(first).unwrap().state, JobState::Failed);
        assert_eq!(queue.job(second).unwrap().state, JobState::Failed);
        assert!(!queue.has_delayed_jobs());

        queue.resume(&[first, second]);
        assert!(queue.claim(late).is_some());
    }

    #[test]
    fn without_reconnecting_a_dropped_connection_is_an_ordinary_retry() {
        let settings = TransferSettings {
            reconnect_minutes: 0,
            ..settings()
        };
        let mut queue = Queue::new();
        let id = queue.add(file("a", 1));
        let start = Instant::now();
        queue.claim(start).unwrap();
        assert_eq!(drop_connection(&mut queue, id, &settings, start), None);
        let job = queue.job(id).unwrap();
        assert!(!job.reconnecting);
        assert!(job.not_before.is_some());
        assert_eq!(job.attempts, 1);
    }

    #[test]
    fn removed_jobs_leave_their_temporary_files_for_deletion() {
        let mut queue = Queue::new();
        let id = queue.add(file("big", 100));
        let claim = queue.claim(Instant::now()).unwrap();
        claim
            .run
            .plan
            .set(Plan {
                rename_to: Some("/remote/big".into()),
                ..plan("/remote/.big.poros-part", 100)
            })
            .unwrap();
        claim.run.start_pieces(Pieces::new(0, 100, 10));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.complete_piece(piece.start);
        queue.pause(&[id]);
        finish(&mut queue, id, RunOutcome::Stopped);
        assert!(queue.take_abandoned().is_empty());

        queue.remove(&[id]);
        assert_eq!(
            queue.take_abandoned(),
            vec![Abandoned {
                session_id: "session".into(),
                direction: Direction::Upload,
                path: "/remote/.big.poros-part".into(),
            }]
        );
    }

    #[test]
    fn unfinished_jobs_come_back_paused_where_they_stopped() {
        let mut queue = Queue::new();
        let done = queue.add(file("done", 1));
        let running = queue.add(file("running", 100));
        let failed = queue.add(file("failed", 1));
        queue.add(file("waiting", 1));
        queue.claim(Instant::now()).unwrap();
        finish(&mut queue, done, RunOutcome::Completed);
        let claim = queue.claim(Instant::now()).unwrap();
        claim.run.plan.set(plan("/remote/running", 100)).unwrap();
        claim.run.start_pieces(Pieces::new(0, 100, 10));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.complete_piece(piece.start);
        queue.claim(Instant::now()).unwrap();
        let denied = AppError::new(ErrorKind::PermissionDenied, "denied");
        finish(&mut queue, failed, RunOutcome::Failed(denied));
        assert_eq!(queue.job(running).unwrap().state, JobState::Running);

        let unfinished = queue.unfinished();
        let names: Vec<&str> = unfinished
            .iter()
            .map(|job| job.spec.name.as_str())
            .collect();
        assert_eq!(names, ["running", "failed", "waiting"]);
        assert_eq!(
            unfinished[0].resume.as_ref().map(|resume| resume.offset),
            Some(10)
        );

        let mut restored = Queue::new();
        let ids: Vec<JobId> = unfinished
            .into_iter()
            .map(|job| restored.restore(job))
            .collect();
        let states: Vec<JobState> = ids
            .iter()
            .map(|id| restored.job(*id).unwrap().state)
            .collect();
        assert_eq!(
            states,
            [JobState::Paused, JobState::Failed, JobState::Paused]
        );
        assert_eq!(restored.job(ids[0]).unwrap().transferred, 10);
        assert!(restored.claim(Instant::now()).is_none());
        restored.resume(&ids);
        let claim = restored.claim(Instant::now()).unwrap();
        assert_eq!(claim.spec.name, "running");
        assert_eq!(claim.resume.map(|resume| resume.offset), Some(10));
    }
}
