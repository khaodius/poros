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

pub type JobId = u64;

/// Top-level ranks grow down from here for "move to top" and up for everything else.
const RANK_MIDDLE: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    pub target: String,
    pub source_modified: Option<i64>,
    pub source_permissions: Option<u32>,
    pub resumed: bool,
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
    max_workers: AtomicU32,
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
            max_workers: AtomicU32::new(1),
        }
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
    /// Bytes before this offset are already in the target from an earlier run.
    pub resume_offset: Option<u64>,
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
    pub resume_offset: Option<u64>,
}

pub enum Release {
    /// Other workers are still on the job.
    Others,
    /// This was the last worker and every byte is in place: finalize, then `settle`.
    Finalize,
    /// This was the last worker: tidy up, then `settle`.
    Settle,
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
                resume_offset: None,
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
        id
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
                    resume_offset: None,
                });
            }
        }

        let (rank, id) = self
            .pending
            .iter()
            .find(|(_, id)| {
                self.jobs[id]
                    .not_before
                    .is_none_or(|not_before| not_before <= now)
            })
            .map(|(rank, id)| (rank.clone(), *id))?;
        self.pending.remove(&rank);
        let job = self.jobs.get_mut(&id)?;
        let run = Arc::new(JobRun::new(job.resume_offset.unwrap_or(0)));
        job.state = JobState::Running;
        job.run = Some(run.clone());
        job.workers = 1;
        job.stop = None;
        job.not_before = None;
        job.conflict = None;
        job.speed = 0;
        job.delta_bytes = None;
        job.sample = Some((now, job.resume_offset.unwrap_or(0)));
        self.dirty.insert(id);
        Some(Claim {
            id,
            role: Role::Primary,
            run,
            spec: job.spec.clone(),
            resolution: job.resolution,
            resume_offset: job.resume_offset,
        })
    }

    /// The first worker prepared the target; up to `max_workers` may now share the file.
    pub fn open_segments(&mut self, id: JobId, max_workers: u32) {
        if let Some(run) = self.jobs.get(&id).and_then(|job| job.run.as_ref()) {
            run.max_workers.store(max_workers, Ordering::Relaxed);
            self.segmented.push(id);
        }
    }

    /// A rename resolved a conflict.
    pub fn retarget(&mut self, id: JobId, target: String, name: String) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.spec.target = target;
            job.spec.name = name;
            self.dirty.insert(id);
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

    /// Decides a job's state once no worker is on it.
    pub fn settle(
        &mut self,
        id: JobId,
        outcome: RunOutcome,
        settings: &TransferSettings,
        now: Instant,
    ) {
        self.segmented.retain(|segmented| *segmented != id);
        let Some(job) = self.jobs.get_mut(&id) else {
            return;
        };
        let run = job.run.take();
        let stop = job.stop.take();
        job.workers = 0;
        job.speed = 0;
        job.sample = None;
        if let Some(run) = &run {
            job.transferred = run.transferred.load(Ordering::Relaxed);
            if let Some(prefix) = run.complete_prefix() {
                job.resume_offset = Some(prefix);
            }
            job.delta_bytes = run
                .delta
                .load(Ordering::Relaxed)
                .then(|| run.delta_bytes.load(Ordering::Relaxed));
        }
        self.dirty.insert(id);

        let failure = match (stop, outcome) {
            (Some(StopReason::Remove), _) => {
                self.forget(id);
                return;
            }
            (_, RunOutcome::Completed) => {
                self.totals.done += 1;
                job.state = JobState::Done;
                job.error = None;
                job.resume_offset = None;
                if let Some(end) = run.as_ref().and_then(|run| run.end()) {
                    job.spec.size = end;
                    job.transferred = end;
                }
                let changed = changed_directory(&job.spec);
                self.changed_directories.push(changed);
                if !settings.keep_completed {
                    self.forget(id);
                }
                return;
            }
            (_, RunOutcome::Skipped(reason)) => {
                self.totals.skipped += 1;
                job.state = JobState::Skipped;
                job.error = Some(reason);
                if !settings.keep_completed {
                    self.forget(id);
                }
                return;
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
                return;
            }
            (Some(StopReason::Pause), _) => {
                job.state = JobState::Paused;
                return;
            }
            (Some(StopReason::Requeue), _) | (None, RunOutcome::Stopped) => {
                job.state = JobState::Queued;
                self.pending.insert(job.rank.clone(), id);
                return;
            }
            (None, RunOutcome::Conflict(info)) => {
                job.state = JobState::Conflict;
                job.conflict = Some(info);
                return;
            }
            (Some(StopReason::Failed(error)), _) | (None, RunOutcome::Failed(error)) => error,
        };

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
    }

    fn forget(&mut self, id: JobId) {
        if let Some(job) = self.jobs.remove(&id) {
            self.pending.remove(&job.rank);
            self.dirty.remove(&id);
            self.removed.push(id);
        }
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
                }
                JobState::Running => stop_run(job, StopReason::Pause),
                _ => continue,
            }
            self.dirty.insert(*id);
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
        self.pending
            .values()
            .any(|id| self.jobs[id].not_before.is_some())
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
        claim.run.start_pieces(Pieces::new(0, 100, 10));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.add_progress(10);
        claim.run.complete_piece(piece.start);

        queue.set_paused(true);
        assert!(claim.run.cancel.is_cancelled());
        finish(&mut queue, id, RunOutcome::Stopped);
        let job = queue.job(id).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.resume_offset, Some(10));
        assert!(queue.claim(Instant::now()).is_none());

        queue.set_paused(false);
        let claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(claim.resume_offset, Some(10));
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
        assert!(job.not_before.is_some());
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
}
