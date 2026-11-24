//! Queue bookkeeping without I/O: job states, processing order, and which worker does what.
//! Everything here runs under the transfer manager's lock, so methods are synchronous.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::batch::{BatchTally, JobBatch};
use super::conflict::ExistsAction;
use super::options::JobOptions;
use super::pieces::Pieces;
use super::TransferStats;
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
    /// From one server to another, straight between them where both allow it (FXP) and
    /// through this computer otherwise.
    Relay,
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
    /// The server written to, or read from for a download.
    pub session_id: String,
    /// The server read from in a relay.
    pub source_session_id: Option<String>,
    pub direction: Direction,
    pub kind: JobKind,
    pub name: String,
    pub source: String,
    pub target: String,
    pub target_directory: String,
    pub size: u64,
    /// Canonical source folders above this one, to stop at symbolic link loops.
    pub ancestors: Arc<[String]>,
    /// What a synchronization asks of the file beyond the transfer settings.
    pub options: Option<Arc<JobOptions>>,
    /// The jobs it was queued with for a caller that follows them all, such as a scheduled
    /// task.
    pub batch: Option<Arc<JobBatch>>,
}

impl JobSpec {
    pub fn involves(&self, session_id: &str) -> bool {
        self.session_id == session_id || self.source_session_id.as_deref() == Some(session_id)
    }

    /// Queued for a caller no one is there to press Start for, such as a scheduled task.
    pub fn runs_unattended(&self) -> bool {
        self.batch.as_ref().is_some_and(|batch| batch.unattended)
    }

    /// The file a file job writes, told apart from the files other jobs write.
    fn written_file(&self) -> Option<WrittenFile> {
        (self.kind == JobKind::File).then(|| self.file_at(&self.target))
    }

    /// `path` on the side this job writes to, told apart from the files other jobs write.
    fn file_at(&self, path: &str) -> WrittenFile {
        match self.direction {
            Direction::Download => WrittenFile {
                session_id: None,
                path: local_file(path),
            },
            Direction::Upload | Direction::Relay => WrittenFile {
                session_id: Some(self.session_id.clone()),
                path: path.to_string(),
            },
        }
    }
}

/// Local file systems may ignore case, so names that differ only in case are one file. So, on
/// Windows, are names that differ only in the dots and spaces that end them.
fn local_file(target: &str) -> String {
    if cfg!(windows) {
        as_windows_opens(target).to_lowercase()
    } else {
        target.to_lowercase()
    }
}

/// `path` as Windows opens it, each name without the dots and spaces that end it: `report.`
/// and `report ` are both `report`. Paths starting `\\?\` are opened as written.
fn as_windows_opens(path: &str) -> String {
    if path.starts_with(r"\\?\") {
        return path.to_string();
    }
    path.split(['\\', '/'])
        .map(|name| match name {
            "." | ".." => name,
            _ => name.trim_end_matches(['.', ' ']),
        })
        .collect::<Vec<_>>()
        .join("\\")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WrittenFile {
    /// The server written to; `None` for this computer.
    session_id: Option<String>,
    path: String,
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
    /// A local source's finer marks, to tell a change the size and time would miss.
    pub source_stamp: Option<SourceStamp>,
    /// The permissions of the file a temporary file replaces, kept when the source's are not.
    pub replaced_permissions: Option<u32>,
}

/// Finer than the size and whole-second time a local source is otherwise compared by: the time
/// to the nanosecond and, on Unix, the inode and status change time, which every write moves
/// even when the modification time is put back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStamp {
    pub modified_nanos: Option<i64>,
    pub changed_nanos: Option<i64>,
    pub inode: Option<u64>,
}

impl SourceStamp {
    pub fn of(metadata: &std::fs::Metadata) -> Self {
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|since| i64::try_from(since.as_nanos()).ok());
        #[cfg(unix)]
        let (changed_nanos, inode) = {
            use std::os::unix::fs::MetadataExt;
            let changed = metadata
                .ctime()
                .checked_mul(1_000_000_000)
                .and_then(|nanos| nanos.checked_add(metadata.ctime_nsec()));
            (changed, Some(metadata.ino()))
        };
        #[cfg(not(unix))]
        let (changed_nanos, inode) = (None, None);
        Self {
            modified_nanos,
            changed_nanos,
            inode,
        }
    }
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
    /// Absent from queues saved before it was kept, and for sources on a server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_stamp: Option<SourceStamp>,
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
    /// Set while two FTP servers send the file to each other (FXP).
    pub direct: AtomicBool,
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
            direct: AtomicBool::new(false),
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
            source_stamp: plan.source_stamp,
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

    /// Where this run began writing: 0 unless it continued an earlier one.
    pub fn start(&self) -> Option<u64> {
        self.pieces.lock().unwrap().as_ref().map(Pieces::start)
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

    /// What a run that ends without `Release::Finalize` comes to. Its bytes stay in the
    /// temporary file, or lack their time and permissions, until finalized, so a run stopped
    /// after the last byte arrived but before that has not completed.
    pub fn unfinalized(&self, outcome: RunOutcome) -> RunOutcome {
        match outcome {
            RunOutcome::Completed if self.plan.get().is_some() => RunOutcome::Stopped,
            outcome => outcome,
        }
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
    /// A temporary file an attempt left that failed a check, so the next attempt starts over.
    /// Deleted if the job leaves the queue before an attempt finishes it.
    pub stale_partial: Option<String>,
    /// Waiting for its server to come back after the connection dropped.
    pub reconnecting: bool,
    /// When its server went out of reach, for a job that failed because the server stayed away.
    /// The job goes back in line once the server can be reached again.
    pub unreachable_since: Option<Instant>,
    pub transferred: u64,
    pub speed: u64,
    /// Bytes sent over the network when rsync moved only the changes.
    pub delta_bytes: Option<u64>,
    /// The servers of a relay sent the file to each other.
    pub direct: bool,
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
            source_session_id: self.spec.source_session_id.clone(),
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
            direct: self.direct,
            reconnecting: self.reconnecting,
            label: self.spec.batch.as_ref().map(|batch| batch.label.clone()),
            unattended: self.spec.runs_unattended(),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_session_id: Option<String>,
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
    pub direct: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reconnecting: bool,
    /// What queued the job when it was not the user: a scheduled task's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A run follows it, so it goes while transfers wait for Start.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unattended: bool,
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
    /// How long since then the paused queue or the schedule kept all its waiting jobs from
    /// starting. Nothing tried the server meanwhile, so that time does not use up the
    /// reconnect window.
    untried: Duration,
    /// Its jobs are not started before this.
    until: Instant,
    failures: u32,
    /// Waiting ran out; its jobs fail like any other until something gets through.
    gave_up: bool,
}

/// The directions the speed limit schedule pauses. Their jobs stay in line, with what they
/// have moved so far, and start again when the schedule lets them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub upload: bool,
    pub download: bool,
}

impl Held {
    /// A relay reads from one server and writes to another, so it waits for either.
    pub fn holds(self, direction: Direction) -> bool {
        match direction {
            Direction::Upload => self.upload,
            Direction::Download => self.download,
            Direction::Relay => self.upload || self.download,
        }
    }
}

const DIRECTIONS: [Direction; 3] = [Direction::Upload, Direction::Download, Direction::Relay];

/// Since when the paused queue or the schedule keeps each direction from starting, while it
/// does.
#[derive(Debug, Clone, Copy, Default)]
struct BlockedSince {
    upload: Option<Instant>,
    download: Option<Instant>,
    relay: Option<Instant>,
}

impl BlockedSince {
    fn get(self, direction: Direction) -> Option<Instant> {
        match direction {
            Direction::Upload => self.upload,
            Direction::Download => self.download,
            Direction::Relay => self.relay,
        }
    }

    fn of(&mut self, direction: Direction) -> &mut Option<Instant> {
        match direction {
            Direction::Upload => &mut self.upload,
            Direction::Download => &mut self.download,
            Direction::Relay => &mut self.relay,
        }
    }
}

/// A temporary file of a transfer that left the queue unfinished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abandoned {
    pub session_id: String,
    pub direction: Direction,
    pub path: String,
    /// The folder it is in.
    pub directory: String,
}

/// A temporary file a job left, until the caller takes it to delete.
#[derive(Debug)]
struct Left {
    partial: Abandoned,
    /// The file as other jobs writing it would name it.
    file: WrittenFile,
    /// The target it was written beside.
    beside: Option<WrittenFile>,
}

impl Left {
    fn new(spec: &JobSpec, path: String) -> Self {
        Self {
            file: spec.file_at(&path),
            beside: spec.written_file(),
            partial: Abandoned {
                session_id: spec.session_id.clone(),
                direction: spec.direction,
                path,
                directory: spec.target_directory.clone(),
            },
        }
    }
}

/// How a watched job stood when it left the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ended {
    pub state: JobState,
    pub size: u64,
}

/// An unfinished job as it is saved between runs of the app.
#[derive(Debug, Clone)]
pub struct Unfinished {
    pub spec: JobSpec,
    pub state: JobState,
    pub error: Option<String>,
    pub resolution: Option<ExistsAction>,
    pub resume: Option<ResumePoint>,
    pub stale_partial: Option<String>,
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
    /// The queue is paused only because transfers wait for Start, not by the user, so jobs
    /// that run unattended still go.
    pub held_for_start: bool,
    held: Held,
    blocked_since: BlockedSince,
    pub conflict_override: Option<ExistsAction>,
    pub totals: Totals,
    dirty: HashSet<JobId>,
    removed: Vec<JobId>,
    changed_directories: Vec<ChangedDirectory>,
    outages: HashMap<String, Outage>,
    abandoned: Vec<Left>,
    /// Files that running jobs write. Two jobs writing one file would share its temporary
    /// file, so a job waits while another writes its target.
    writing: HashMap<WrittenFile, JobId>,
    /// Jobs a caller follows to the end, and how those that left the queue ended.
    watched: HashSet<JobId>,
    ended: HashMap<JobId, Ended>,
    /// Jobs were added, removed, reordered or changed state since the queue was last saved.
    pub unsaved: bool,
    /// The number of the latest progress update sent to the windows.
    pub sequence: u64,
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
                stale_partial: None,
                reconnecting: false,
                unreachable_since: None,
                transferred: 0,
                speed: 0,
                delta_bytes: None,
                direct: false,
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
            job.stale_partial = saved.stale_partial;
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
                stale_partial: job.stale_partial.clone(),
            })
            .collect()
    }

    pub fn job(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    pub fn has_session_jobs(&self, session_id: &str) -> bool {
        self.jobs.values().any(|job| job.spec.involves(session_id))
    }

    pub fn claim(&mut self, now: Instant) -> Option<Claim> {
        if self.paused && !self.held_for_start {
            return None;
        }
        let unattended_only = self.paused;
        for &id in &self.segmented {
            let Some(job) = self.jobs.get_mut(&id) else {
                continue;
            };
            let Some(run) = job.run.clone() else {
                continue;
            };
            if unattended_only && !job.spec.runs_unattended() {
                continue;
            }
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
                // Jobs that run unattended pass a queue waiting for Start, not the
                // directions the speed limit schedule holds.
                (!unattended_only || job.spec.runs_unattended())
                    && job.not_before.is_none_or(|not_before| not_before <= now)
                    && !self.held.holds(job.spec.direction)
                    && !self.server_unreachable(&job.spec.session_id, now)
                    && job
                        .spec
                        .source_session_id
                        .as_deref()
                        .is_none_or(|source| !self.server_unreachable(source, now))
                    && job
                        .spec
                        .written_file()
                        .is_none_or(|file| !self.writing.contains_key(&file))
            })
            .map(|(rank, id)| (rank.clone(), *id))?;
        self.pending.remove(&rank);
        if let Some(file) = self.jobs[&id].spec.written_file() {
            self.writing.insert(file, id);
        }
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
        job.direct = false;
        job.reconnecting = false;
        job.unreachable_since = None;
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
            let file = job.run.as_ref().and_then(|_| job.spec.written_file());
            if let Some(file) = file {
                self.writing.retain(|_, writer| *writer != id);
                self.writing.insert(file, id);
            }
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
        self.writing.retain(|_, writer| *writer != id);
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
                // What is at the target cannot be trusted, so the next attempt replaces it
                // rather than resuming onto it or skipping it as already there.
                job.resolution = Some(ExistsAction::Overwrite);
                if let Some(plan) = run.plan.get().filter(|plan| plan.rename_to.is_some()) {
                    job.stale_partial = Some(plan.target.clone());
                }
                // A download that failed its check is deleted, or rewritten, by now.
                if job.spec.direction == Direction::Download {
                    self.changed_directories.push(changed_directory(&job.spec));
                }
            } else if let Some(resume) = run.resume_point() {
                job.resume = Some(resume);
            }
            job.delta_bytes = run
                .delta
                .load(Ordering::Relaxed)
                .then(|| run.delta_bytes.load(Ordering::Relaxed));
            job.direct = run.direct.load(Ordering::Relaxed);
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
                // Moved into place, unless this attempt wrote through another temporary file.
                let stale = job.stale_partial.take().filter(|stale| {
                    !run.as_ref()
                        .and_then(|run| run.plan.get())
                        .is_some_and(|plan| plan.target == *stale)
                });
                let left = stale.map(|stale| Left::new(&job.spec, stale));
                if let Some(end) = run.as_ref().and_then(|run| run.end()) {
                    job.spec.size = end;
                    job.transferred = end;
                }
                let changed = changed_directory(&job.spec);
                self.changed_directories.push(changed);
                self.abandoned.extend(left);
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
            (stop, RunOutcome::Expanded(children)) => {
                let rank = job.rank.clone();
                let changed = changed_directory(&job.spec);
                self.changed_directories.push(changed);
                // Its children stand for it in its batch from here on.
                job.spec.batch = None;
                self.forget(id);
                // Paused while it was being listed: what it holds waits as the folder would.
                let paused = matches!(stop, Some(StopReason::Pause));
                for (index, child) in children.into_iter().enumerate() {
                    let child_rank = format!("{rank}{}", rank_segment(index as u32));
                    let child_id = self.insert(child_rank, child);
                    if paused {
                        self.pause(&[child_id]);
                    }
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
        let given_up_since = self
            .outages
            .get(&session_id)
            .filter(|outage| outage.gave_up && failure.is_connection_lost())
            .map(|outage| outage.since);
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
            job.unreachable_since = given_up_since;
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
                        untried: Duration::ZERO,
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
                if now
                    .saturating_duration_since(outage.since)
                    .saturating_sub(outage.untried)
                    < settings.reconnect_window() =>
            {
                outage.failures += 1;
                let retry_in = delay(outage.failures);
                outage.until = now + retry_in;
                Some(ServerChange::Lost { retry_in })
            }
            Some(outage) => {
                outage.gave_up = true;
                let since = outage.since;
                let message = format!(
                    "{} (gave up after trying to reconnect for {} min)",
                    failure.message, settings.reconnect_minutes
                );
                let failed = self.fail_waiting(&session_id, &message, since) + 1;
                let job = self.jobs.get_mut(&id)?;
                self.totals.failed += 1;
                job.state = JobState::Failed;
                job.reconnecting = false;
                job.unreachable_since = Some(since);
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

    /// Fails every queued job of a session, out of reach since `since`; returns how many there
    /// were.
    fn fail_waiting(&mut self, session_id: &str, message: &str, since: Instant) -> usize {
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
                job.unreachable_since = Some(since);
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

    /// Something got through to the server, so it is no longer out of reach, and transfers that
    /// failed while it was go back in line.
    fn server_answered(&mut self, session_id: &str) -> Option<ServerChange> {
        let outage = self.outages.remove(session_id)?;
        if outage.gave_up {
            self.retry_unreachable(session_id);
        }
        Some(ServerChange::Back)
    }

    /// Servers that transfers failed to reach, each with when the earliest of them lost it.
    pub fn unreachable_servers(&self) -> Vec<(String, Instant)> {
        let mut servers: HashMap<String, Instant> = HashMap::new();
        for job in self.jobs.values() {
            if let (JobState::Failed, Some(since)) = (job.state, job.unreachable_since) {
                servers
                    .entry(job.spec.session_id.clone())
                    .and_modify(|earliest| *earliest = (*earliest).min(since))
                    .or_insert(since);
            }
        }
        servers.into_iter().collect()
    }

    /// Puts the transfers that failed because a server was out of reach back in line, with a
    /// fresh retry budget; returns how many there were.
    pub fn retry_unreachable(&mut self, session_id: &str) -> usize {
        let ids: Vec<JobId> = self
            .jobs
            .values()
            .filter(|job| {
                job.state == JobState::Failed
                    && job.unreachable_since.is_some()
                    && job.spec.session_id == session_id
            })
            .map(|job| job.id)
            .collect();
        self.resume(&ids);
        ids.len()
    }

    /// Keeps how these jobs end once they leave the queue, for `ended`.
    pub fn watch(&mut self, ids: &[JobId]) {
        self.watched.extend(ids);
    }

    pub fn ended(&self, id: JobId) -> Option<Ended> {
        self.ended.get(&id).copied()
    }

    /// The caller no longer follows these jobs.
    pub fn unwatch(&mut self, ids: &[JobId]) {
        for id in ids {
            self.watched.remove(id);
            self.ended.remove(id);
        }
    }

    fn forget(&mut self, id: JobId) {
        if let Some(job) = self.jobs.remove(&id) {
            if let Some(batch) = &job.spec.batch {
                batch.departs(&job);
            }
            if self.watched.remove(&id) {
                self.ended.insert(
                    id,
                    Ended {
                        state: job.state,
                        size: job.spec.size,
                    },
                );
            }
            self.pending.remove(&job.rank);
            self.dirty.remove(&id);
            self.removed.push(id);
            self.unsaved = true;
            let partial = job.resume.and_then(|resume| resume.partial);
            let stale = job
                .stale_partial
                .filter(|stale| partial.as_ref() != Some(stale));
            for path in partial.into_iter().chain(stale) {
                self.abandoned.push(Left::new(&job.spec, path));
            }
        }
    }

    /// Temporary files of jobs that left the queue, for the caller to delete: each once, and
    /// none that a job still in the queue writes or keeps to continue from.
    pub fn take_abandoned(&mut self) -> Vec<Abandoned> {
        if self.abandoned.is_empty() {
            return Vec::new();
        }
        let mut held = HashSet::new();
        // Running jobs that have not yet chosen where to write: one writing the same target
        // would choose the same temporary file.
        let mut preparing = HashSet::new();
        for job in self.jobs.values() {
            let planned = job
                .run
                .as_ref()
                .map(|run| run.plan.get().map(|plan| plan.target.as_str()));
            if planned == Some(None) {
                preparing.extend(job.spec.written_file());
            }
            let partial = job
                .resume
                .as_ref()
                .and_then(|resume| resume.partial.as_deref());
            let stale = job.stale_partial.as_deref();
            for path in planned.flatten().into_iter().chain(partial).chain(stale) {
                held.insert(job.spec.file_at(path));
            }
        }
        std::mem::take(&mut self.abandoned)
            .into_iter()
            .filter(|left| {
                left.beside
                    .as_ref()
                    .is_none_or(|target| !preparing.contains(target))
                    && held.insert(left.file.clone())
            })
            .map(|left| left.partial)
            .collect()
    }

    /// The caller deleted a temporary file `take_abandoned` gave it, so panes showing its
    /// folder can refresh.
    pub fn deleted(&mut self, partial: &Abandoned) {
        let folder = folder_written(partial.direction, &partial.session_id, &partial.directory);
        self.changed_directories.push(folder);
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
            job.unreachable_since = None;
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

    /// Takes the jobs of `batch` that have ended off the queue. How the batch went stays as it
    /// was.
    pub fn clear_batch(&mut self, batch: &Arc<JobBatch>) {
        let ids: Vec<JobId> = self
            .jobs
            .values()
            .filter(|job| {
                job.is_finished()
                    && job
                        .spec
                        .batch
                        .as_ref()
                        .is_some_and(|other| Arc::ptr_eq(other, batch))
            })
            .map(|job| job.id)
            .collect();
        for id in ids {
            self.forget(id);
        }
    }

    pub fn session_job_ids(&self, session_id: &str) -> Vec<JobId> {
        self.jobs
            .values()
            .filter(|job| job.spec.involves(session_id) && !job.is_finished())
            .map(|job| job.id)
            .collect()
    }

    /// Jobs of a session that failed after their retries.
    pub fn session_failures(&self, session_id: &str) -> usize {
        self.jobs
            .values()
            .filter(|job| job.spec.session_id == session_id && job.state == JobState::Failed)
            .count()
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
    pub fn set_paused(&mut self, paused: bool, now: Instant) {
        self.paused = paused;
        self.held_for_start = false;
        if paused {
            for job in self.jobs.values_mut() {
                if job.state == JobState::Running && job.stop.is_none() {
                    stop_run(job, StopReason::Requeue);
                }
            }
        }
        self.note_blocked(now);
    }

    /// Holds back the directions the speed limit schedule pauses, interrupting their running
    /// transfers like a paused queue does, and lets the others go. Returns whether that changed.
    pub fn set_held(&mut self, held: Held, now: Instant) -> bool {
        if self.held == held {
            return false;
        }
        self.held = held;
        for job in self.jobs.values_mut() {
            if job.state == JobState::Running && held.holds(job.spec.direction) {
                stop_run(job, StopReason::Requeue);
                self.dirty.insert(job.id);
            }
        }
        self.note_blocked(now);
        true
    }

    pub fn held(&self) -> Held {
        self.held
    }

    /// Notes which directions the paused queue or the schedule now keeps from starting, and
    /// since when. A server whose waiting jobs were all kept back, and some of which may start
    /// again now, does not count the time nothing tried it towards its reconnect window, so the
    /// window still runs out however often the schedule lets them go.
    fn note_blocked(&mut self, now: Instant) {
        let before = self.blocked_since;
        let mut let_go = false;
        for direction in DIRECTIONS {
            // A queue waiting for Start still lets unattended jobs try their servers.
            let blocked = self.paused_by_user() || self.held.holds(direction);
            let since = self.blocked_since.of(direction);
            match (blocked, since.is_some()) {
                (true, false) => *since = Some(now),
                (false, true) => {
                    *since = None;
                    let_go = true;
                }
                _ => {}
            }
        }
        if !let_go {
            return;
        }
        // For each server with waiting jobs: when the last of their directions was kept back,
        // or nothing if one of them could start meanwhile; and whether one may start now.
        let mut untried_since: HashMap<&String, Option<Instant>> = HashMap::new();
        let mut released: HashSet<&String> = HashSet::new();
        for id in self.pending.values() {
            let job = &self.jobs[id];
            let direction = job.spec.direction;
            let kept_back = before.get(direction);
            untried_since
                .entry(&job.spec.session_id)
                .and_modify(|latest| {
                    *latest = latest
                        .zip(kept_back)
                        .map(|(first, second)| first.max(second));
                })
                .or_insert(kept_back);
            if kept_back.is_some() && self.blocked_since.get(direction).is_none() {
                released.insert(&job.spec.session_id);
            }
        }
        for (session_id, outage) in &mut self.outages {
            if outage.gave_up || !released.contains(session_id) {
                continue;
            }
            if let Some(Some(kept_back)) = untried_since.get(session_id) {
                outage.untried += now.saturating_duration_since(outage.since.max(*kept_back));
            }
        }
    }

    /// Pauses an idle queue until the user presses Start, as transfers do when they do not start
    /// on their own. Jobs that run unattended go on, and a queue the user paused stays so.
    pub fn hold_for_start(&mut self) {
        let waiting_or_running = self.jobs.values().any(|job| {
            !job.spec.runs_unattended()
                && matches!(
                    job.state,
                    JobState::Queued | JobState::Running | JobState::Conflict
                )
        });
        if !self.paused && !waiting_or_running {
            self.paused = true;
            self.held_for_start = true;
        }
    }

    /// The user paused the queue, so nothing starts, unattended or not.
    pub fn paused_by_user(&self) -> bool {
        self.paused && !self.held_for_start
    }

    /// How the queue stands, for the transfer lists. Speeds are left for the caller to add.
    pub fn stats(&self) -> TransferStats {
        TransferStats {
            counts: self.counts(),
            remaining_bytes: self.remaining_bytes(),
            queue_paused: self.paused,
            held_for_start: self.paused && self.held_for_start,
            // Ones the speed limit schedule holds back are not going anywhere, so Pause would
            // stop nothing.
            unattended_running: self.jobs.values().any(|job| {
                job.spec.runs_unattended()
                    && match job.state {
                        JobState::Running => true,
                        JobState::Queued => !self.held.holds(job.spec.direction),
                        _ => false,
                    }
            }),
            ..TransferStats::default()
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
            let direct = run.direct.load(Ordering::Relaxed);
            if transferred != job.transferred
                || speed != job.speed
                || delta_bytes != job.delta_bytes
                || direct != job.direct
            {
                job.transferred = transferred;
                job.speed = speed;
                job.delta_bytes = delta_bytes;
                job.direct = direct;
                self.dirty.insert(job.id);
            }
            job.sample = Some((now, transferred));
        }
    }

    /// How the jobs of `batch` stand: the ones that left the queue, and the ones still in it
    /// by the state they are in.
    pub fn batch_tally(&self, batch: &Arc<JobBatch>) -> BatchTally {
        let mut tally = batch.departed();
        let in_batch = |job: &&Job| {
            job.spec
                .batch
                .as_ref()
                .is_some_and(|other| Arc::ptr_eq(other, batch))
        };
        for job in self.jobs.values().filter(in_batch) {
            tally.count(job);
        }
        tally
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
            // A held job waits for the schedule, which wakes the workers itself when it ends.
            !self.held.holds(job.spec.direction)
                && (job.not_before.is_some() || self.outages.contains_key(&job.spec.session_id))
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
    folder_written(spec.direction, &spec.session_id, &spec.target_directory)
}

/// `directory` on the side a transfer of `direction` to or from `session_id` writes to.
fn folder_written(direction: Direction, session_id: &str, directory: &str) -> ChangedDirectory {
    match direction {
        Direction::Download => ChangedDirectory {
            side: Side::Local,
            session_id: None,
            path: directory.to_string(),
        },
        Direction::Upload | Direction::Relay => ChangedDirectory {
            side: Side::Remote,
            session_id: Some(session_id.to_string()),
            path: directory.to_string(),
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
            source_session_id: None,
            direction: Direction::Upload,
            kind: JobKind::File,
            name: name.into(),
            source: format!("/local/{name}"),
            target: format!("/remote/{name}"),
            target_directory: "/remote".into(),
            size,
            ancestors: Arc::from([]),
            options: None,
            batch: None,
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
            source_stamp: None,
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
    fn a_batch_follows_its_jobs_through_folders_and_out_of_the_queue() {
        let batch = JobBatch::new("Nightly", Some(ExistsAction::Ask));
        // No one is there to answer.
        assert_eq!(batch.exists_action, Some(ExistsAction::Overwrite));
        let in_batch = |spec: JobSpec| JobSpec {
            batch: Some(batch.clone()),
            ..spec
        };
        let mut queue = Queue::new();
        let folder_id = queue.add(in_batch(folder("dir")));
        let other = queue.add(file("someone else's", 5));
        assert_eq!(queue.batch_tally(&batch).unfinished, 1);
        let labels: Vec<Option<String>> =
            queue.snapshots().into_iter().map(|job| job.label).collect();
        assert_eq!(labels, [Some("Nightly".to_string()), None]);

        // The folder lists, and what it held stands for it from then on.
        queue.claim(Instant::now()).unwrap();
        let children = ["one", "two", "three", "four"].map(|name| in_batch(file(name, 10)));
        finish(
            &mut queue,
            folder_id,
            RunOutcome::Expanded(children.to_vec()),
        );
        let listed = queue.batch_tally(&batch);
        assert_eq!((listed.unfinished, listed.done, listed.removed), (4, 0, 0));

        let ids: Vec<JobId> = (0..4)
            .map(|_| queue.claim(Instant::now()).unwrap().id)
            .collect();
        finish(&mut queue, ids[0], RunOutcome::Completed);
        let denied = AppError::new(ErrorKind::PermissionDenied, "denied");
        finish(&mut queue, ids[1], RunOutcome::Failed(denied));
        finish(&mut queue, ids[2], RunOutcome::Skipped("is there".into()));
        // Taken off the queue while it runs.
        queue.remove(&[ids[3]]);
        finish(&mut queue, ids[3], RunOutcome::Stopped);
        let ended = BatchTally {
            done: 1,
            skipped: 1,
            failed: 1,
            removed: 1,
            unfinished: 0,
            bytes: 10,
            failures: vec!["/local/two: denied".into()],
        };
        assert_eq!(queue.batch_tally(&batch), ended);
        assert!(!ended.is_clean());

        // Clearing the list afterwards changes nothing about how the batch ended.
        queue.clear(&[JobState::Done, JobState::Skipped, JobState::Failed]);
        assert_eq!(queue.batch_tally(&batch), ended);
        // The other job was never part of it.
        assert_eq!(queue.job(other).unwrap().state, JobState::Queued);
    }

    #[test]
    fn a_batch_that_has_been_counted_leaves_the_queue_without_its_neighbours() {
        let batch = JobBatch::new("Nightly", None);
        let in_batch = |name: &str| JobSpec {
            batch: Some(batch.clone()),
            ..file(name, 10)
        };
        let mut queue = Queue::new();
        let failing = queue.add(in_batch("one"));
        let skipped = queue.add(in_batch("two"));
        let running = queue.add(in_batch("three"));
        let others = JobBatch::new("Weekly", None);
        let other = queue.add(JobSpec {
            batch: Some(others.clone()),
            ..file("weekly", 1)
        });
        let by_hand = queue.add(file("by hand", 1));
        for _ in 0..3 {
            queue.claim(Instant::now()).unwrap();
        }
        let denied = AppError::new(ErrorKind::PermissionDenied, "denied");
        finish(&mut queue, failing, RunOutcome::Failed(denied));
        finish(&mut queue, skipped, RunOutcome::Skipped("is there".into()));
        let other_claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(other_claim.id, other);
        let gone = AppError::new(ErrorKind::PermissionDenied, "gone");
        finish(&mut queue, other, RunOutcome::Failed(gone));
        let counted = queue.batch_tally(&batch);

        queue.clear_batch(&batch);
        assert!(queue.job(failing).is_none() && queue.job(skipped).is_none());
        assert_eq!(queue.batch_tally(&batch), counted);
        // A job that still runs stays until it ends; other batches and hand-queued files stay.
        assert_eq!(queue.job(running).unwrap().state, JobState::Running);
        assert_eq!(queue.job(other).unwrap().state, JobState::Failed);
        assert_eq!(queue.job(by_hand).unwrap().state, JobState::Queued);
    }

    #[test]
    fn unattended_jobs_go_while_the_users_wait_for_start_but_not_while_paused() {
        let task = JobBatch::new("Nightly", None);
        let scheduled = |name: &str| JobSpec {
            batch: Some(task.clone()),
            ..file(name, 1)
        };
        let mut queue = Queue::new();
        // Scheduled files are queued without holding an idle queue for Start.
        queue.add(scheduled("first run"));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("first run"));
        let stats = queue.stats();
        assert!(!stats.queue_paused && !stats.held_for_start && stats.unattended_running);

        // The user queues by hand while it runs: their file waits for Start.
        queue.hold_for_start();
        let by_hand = queue.add(file("by hand", 1));
        assert!(queue.paused && queue.held_for_start && !queue.paused_by_user());
        assert_eq!(claim_name(&mut queue), None);
        queue.add(scheduled("second run"));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("second run"));
        assert_eq!(queue.job(by_hand).unwrap().state, JobState::Queued);
        // The lists say the queue waits for Start while scheduled files go on, so it is shown
        // as going, with Pause.
        let stats = queue.stats();
        assert!(stats.queue_paused && stats.held_for_start && stats.unattended_running);
        assert_eq!((stats.counts.running, stats.counts.queued), (2, 1));
        // When they are through, only the user's file is left, waiting for Start.
        let running: Vec<JobId> = queue
            .jobs
            .values()
            .filter(|job| job.state == JobState::Running)
            .map(|job| job.id)
            .collect();
        for id in running {
            finish(&mut queue, id, RunOutcome::Completed);
        }
        let stats = queue.stats();
        assert!(stats.queue_paused && stats.held_for_start && !stats.unattended_running);
        // Jobs brought back from the saved queue are followed by no one, and wait too.
        queue.add(JobSpec {
            batch: Some(JobBatch::restored("Nightly".into(), None)),
            ..file("restored", 1)
        });
        assert_eq!(claim_name(&mut queue), None);

        // Pausing holds back everything, and Start lets everything go.
        queue.add(scheduled("third run"));
        queue.set_paused(true, Instant::now());
        assert!(queue.paused_by_user());
        assert_eq!(claim_name(&mut queue), None);
        let stats = queue.stats();
        assert!(stats.queue_paused && !stats.held_for_start && stats.unattended_running);
        // A queue the user paused stays paused when more is queued.
        queue.hold_for_start();
        assert!(queue.paused_by_user());
        queue.set_paused(false, Instant::now());
        let mut started: Vec<String> = std::iter::from_fn(|| claim_name(&mut queue)).collect();
        started.sort();
        assert_eq!(started, ["by hand", "restored", "third run"]);
    }

    #[test]
    fn scheduled_files_pass_a_queue_waiting_for_start_but_not_the_speed_schedule() {
        let task = JobBatch::new("Nightly", None);
        let scheduled = |spec: JobSpec| JobSpec {
            batch: Some(task.clone()),
            ..spec
        };
        let mut queue = Queue::new();
        queue.hold_for_start();
        let by_hand = queue.add(file("by hand", 1));
        queue.add(scheduled(file("nightly upload", 1)));
        queue.add(scheduled(download("nightly download", 1)));

        // A window that pauses uploads holds the scheduled upload too; the download goes.
        queue.set_held(UPLOADS_HELD, Instant::now());
        assert_eq!(claim_name(&mut queue).as_deref(), Some("nightly download"));
        assert_eq!(claim_name(&mut queue), None);
        let stats = queue.stats();
        assert!(stats.held_for_start && stats.unattended_running);
        let running: Vec<JobId> = queue
            .jobs
            .values()
            .filter(|job| job.state == JobState::Running)
            .map(|job| job.id)
            .collect();
        for id in running {
            finish(&mut queue, id, RunOutcome::Completed);
        }
        // Only a held upload is left of the run: nothing of it moves, so the queue reads as
        // waiting for Start, and Start would send only the user's file.
        let stats = queue.stats();
        assert!(stats.queue_paused && stats.held_for_start && !stats.unattended_running);

        // When the window ends, the scheduled upload goes, and the user's file still waits.
        queue.set_held(Held::default(), Instant::now());
        assert!(queue.stats().unattended_running);
        assert_eq!(claim_name(&mut queue).as_deref(), Some("nightly upload"));
        assert_eq!(claim_name(&mut queue), None);
        assert_eq!(queue.job(by_hand).unwrap().state, JobState::Queued);

        // A window that starts while it runs stops it, as it stops any upload.
        queue.set_held(UPLOADS_HELD, Instant::now());
        let upload = queue
            .jobs
            .values()
            .find(|job| job.spec.name == "nightly upload")
            .unwrap();
        assert!(upload.stop.is_some());
    }

    #[test]
    fn a_folder_paused_while_it_is_listed_queues_its_contents_paused() {
        let mut queue = Queue::new();
        let folder_id = queue.add(folder("dir"));
        queue.claim(Instant::now()).unwrap();
        queue.pause(&[folder_id]);
        finish(
            &mut queue,
            folder_id,
            RunOutcome::Expanded(vec![file("one", 1), folder("two")]),
        );
        assert!(queue.job(folder_id).is_none());
        assert!(queue.claim(Instant::now()).is_none());
        let children = queue.snapshots();
        let states: Vec<JobState> = children.iter().map(|job| job.state).collect();
        assert_eq!(states, [JobState::Paused, JobState::Paused]);

        let ids: Vec<JobId> = children.iter().map(|job| job.id).collect();
        queue.resume(&ids);
        assert_eq!(claim_name(&mut queue).as_deref(), Some("one"));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("two"));
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

        queue.set_paused(true, Instant::now());
        assert!(claim.run.cancel.is_cancelled());
        finish(&mut queue, id, RunOutcome::Stopped);
        let job = queue.job(id).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.resume.as_ref().map(|resume| resume.offset), Some(10));
        assert!(queue.claim(Instant::now()).is_none());

        queue.set_paused(false, Instant::now());
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
    fn a_failed_check_starts_the_next_attempt_over_whatever_is_at_the_target() {
        let mut queue = Queue::new();
        let id = queue.add_resolved(file("a", 100), ExistsAction::Resume);
        let claim = queue.claim(Instant::now()).unwrap();
        let _ = claim.run.plan.set(plan("/remote/a", 100));
        claim.run.start_pieces(Pieces::new(40, 100, 10));
        claim.run.restart.store(true, Ordering::Relaxed);
        let failed = RunOutcome::Failed(AppError::integrity("does not match"));
        assert!(matches!(queue.release(id, &failed), Release::Settle));
        let retrying = TransferSettings {
            retry_delay_secs: 0,
            ..settings()
        };
        queue.settle(id, failed, &retrying, Instant::now());
        let retry = queue.claim(Instant::now()).unwrap();
        assert_eq!(retry.resolution, Some(ExistsAction::Overwrite));
        assert!(retry.resume.is_none());
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
    fn transfers_given_up_on_try_again_once_the_server_answers() {
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
        queue.claim(late).unwrap();
        drop_connection(&mut queue, first, &settings, late);
        assert_eq!(
            queue.unreachable_servers(),
            vec![("session".to_string(), start)]
        );

        // A transfer added since gets through, so the server is back.
        let third = queue.add(file("c", 1));
        let claim = queue.claim(late).unwrap();
        assert_eq!(claim.id, third);
        assert!(matches!(
            queue.release(third, &RunOutcome::Completed),
            Release::Settle
        ));
        assert_eq!(
            queue.settle(third, RunOutcome::Completed, &settings, late),
            Some(ServerChange::Back)
        );
        for id in [first, second] {
            let job = queue.job(id).unwrap();
            assert_eq!(job.state, JobState::Queued);
            assert_eq!(job.attempts, 0);
            assert!(job.unreachable_since.is_none());
        }
        assert!(queue.unreachable_servers().is_empty());
    }

    #[test]
    fn transfers_given_up_on_wait_for_the_server_to_be_reached() {
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
        queue.claim(late).unwrap();
        drop_connection(&mut queue, first, &settings, late);

        // Failing for another reason says nothing about the server being out of reach.
        queue.resume(&[second]);
        let claim = queue.claim(late).unwrap();
        assert_eq!(claim.id, second);
        let denied = AppError::new(ErrorKind::PermissionDenied, "denied");
        finish(&mut queue, second, RunOutcome::Failed(denied));
        assert_eq!(queue.job(second).unwrap().state, JobState::Failed);
        assert!(queue.job(second).unwrap().unreachable_since.is_none());

        assert_eq!(queue.retry_unreachable("elsewhere"), 0);
        assert_eq!(queue.retry_unreachable("session"), 1);
        assert_eq!(queue.job(first).unwrap().state, JobState::Queued);
        assert_eq!(queue.job(second).unwrap().state, JobState::Failed);
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
    fn pausing_as_the_last_bytes_arrive_keeps_the_job_until_it_is_finalized() {
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
        claim.run.start_pieces(Pieces::new(0, 100, 100));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.complete_piece(piece.start);

        // The worker read every byte, then the pause arrived during its last round trip.
        queue.pause(&[id]);
        assert!(matches!(
            queue.release(id, &RunOutcome::Completed),
            Release::Settle
        ));
        let outcome = claim.run.unfinalized(RunOutcome::Completed);
        queue.settle(id, outcome, &settings(), Instant::now());
        let job = queue.job(id).unwrap();
        assert_eq!(job.state, JobState::Paused);
        assert_eq!(
            job.resume
                .as_ref()
                .map(|resume| (resume.offset, resume.partial.as_deref())),
            Some((100, Some("/remote/.big.poros-part")))
        );
        assert!(queue.take_abandoned().is_empty());

        queue.resume(&[id]);
        let claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(claim.resume.map(|resume| resume.offset), Some(100));
    }

    #[test]
    fn transfers_that_need_no_finalizing_complete_as_they_end() {
        let mut queue = Queue::new();
        let id = queue.add(file("delta", 100));
        let claim = queue.claim(Instant::now()).unwrap();
        let outcome = claim.run.unfinalized(RunOutcome::Completed);
        finish(&mut queue, id, outcome);
        assert_eq!(queue.job(id).unwrap().state, JobState::Done);
    }

    #[test]
    fn jobs_writing_the_same_file_take_turns() {
        let mut queue = Queue::new();
        let first = queue.add(file("same", 1));
        let second = queue.add(file("same", 1));
        let other = queue.add(file("other", 1));
        let mut download = file("same", 1);
        download.direction = Direction::Download;
        download.target = "/LOCAL/Same".into();
        let local_first = queue.add(download.clone());
        download.target = "/local/same".into();
        let local_second = queue.add(download);

        let claimed: Vec<JobId> = std::iter::from_fn(|| queue.claim(Instant::now()))
            .map(|claim| claim.id)
            .collect();
        assert_eq!(claimed, [first, other, local_first]);

        finish(&mut queue, first, RunOutcome::Completed);
        finish(&mut queue, local_first, RunOutcome::Completed);
        let claimed: Vec<JobId> = std::iter::from_fn(|| queue.claim(Instant::now()))
            .map(|claim| claim.id)
            .collect();
        assert_eq!(claimed, [second, local_second]);
    }

    #[test]
    fn windows_opens_names_without_the_dots_and_spaces_that_end_them() {
        for (path, opened) in [
            (r"C:\Downloads\report.", r"C:\Downloads\report"),
            (r"C:\Downloads\report . .", r"C:\Downloads\report"),
            (
                r"C:\Downloads\draft.\report.txt",
                r"C:\Downloads\draft\report.txt",
            ),
            (r"C:/Downloads/report ", r"C:\Downloads\report"),
            (r"\\server\share\report.", r"\\server\share\report"),
            (r"..\notes\.\report", r"..\notes\.\report"),
            (r"\\?\C:\Downloads\report.", r"\\?\C:\Downloads\report."),
        ] {
            assert_eq!(as_windows_opens(path), opened, "{path}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn downloads_onto_names_windows_opens_as_one_take_turns() {
        let mut queue = Queue::new();
        let mut download = file("report", 1);
        download.direction = Direction::Download;
        download.target = r"C:\Downloads\report".into();
        let first = queue.add(download.clone());
        download.target = r"C:\Downloads\report.".into();
        let second = queue.add(download.clone());
        download.target = r"C:\Downloads\REPORT ".into();
        let third = queue.add(download);

        assert_eq!(
            queue.claim(Instant::now()).map(|claim| claim.id),
            Some(first)
        );
        assert!(queue.claim(Instant::now()).is_none());
        finish(&mut queue, first, RunOutcome::Completed);
        assert_eq!(
            queue.claim(Instant::now()).map(|claim| claim.id),
            Some(second)
        );
        assert!(queue.claim(Instant::now()).is_none());
        finish(&mut queue, second, RunOutcome::Completed);
        assert_eq!(
            queue.claim(Instant::now()).map(|claim| claim.id),
            Some(third)
        );
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
                directory: "/remote".into(),
            }]
        );
    }

    /// Claims a job and lets its attempt fail a check after writing its temporary file.
    fn fail_check(queue: &mut Queue, id: JobId, settings: &TransferSettings) {
        let claim = queue.claim(Instant::now()).unwrap();
        claim
            .run
            .plan
            .set(Plan {
                rename_to: Some("/remote/live.log".into()),
                ..plan("/remote/.live.log.poros-part", 100)
            })
            .unwrap();
        claim.run.start_pieces(Pieces::new(0, 100, 100));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.complete_piece(piece.start);
        claim.run.restart.store(true, Ordering::Relaxed);
        let failed = RunOutcome::Failed(AppError::integrity("live.log changed meanwhile"));
        assert!(matches!(queue.release(id, &failed), Release::Settle));
        queue.settle(id, failed, settings, Instant::now());
    }

    #[test]
    fn a_temporary_file_that_failed_its_check_is_deleted_with_its_job() {
        let temporary_file = Abandoned {
            session_id: "session".into(),
            direction: Direction::Upload,
            path: "/remote/.live.log.poros-part".into(),
            directory: "/remote".into(),
        };
        let mut queue = Queue::new();
        let id = queue.add(file("live.log", 100));
        let no_retries = TransferSettings {
            retry_attempts: 0,
            ..settings()
        };
        fail_check(&mut queue, id, &no_retries);
        assert_eq!(queue.job(id).unwrap().state, JobState::Failed);
        assert!(queue.take_abandoned().is_empty());
        queue.clear(&[JobState::Failed]);
        assert_eq!(queue.take_abandoned(), vec![temporary_file.clone()]);

        // Removed while it waits to try again.
        let id = queue.add(file("live.log", 100));
        let retrying = TransferSettings {
            retry_delay_secs: 60,
            ..settings()
        };
        fail_check(&mut queue, id, &retrying);
        assert_eq!(queue.job(id).unwrap().state, JobState::Queued);
        queue.remove(&[id]);
        assert_eq!(queue.take_abandoned(), vec![temporary_file.clone()]);

        // Saved with the queue and restored, it is still known.
        let id = queue.add(file("live.log", 100));
        fail_check(&mut queue, id, &no_retries);
        let mut restored = Queue::new();
        for job in queue.unfinished() {
            restored.restore(job);
        }
        let ids: Vec<JobId> = restored.jobs().map(|job| job.id).collect();
        restored.remove(&ids);
        assert_eq!(restored.take_abandoned(), vec![temporary_file]);
    }

    #[test]
    fn a_temporary_file_a_later_attempt_finished_is_not_deleted() {
        let mut queue = Queue::new();
        let id = queue.add(file("live.log", 100));
        let retrying = TransferSettings {
            retry_delay_secs: 0,
            ..settings()
        };
        fail_check(&mut queue, id, &retrying);
        let claim = queue.claim(Instant::now()).unwrap();
        claim
            .run
            .plan
            .set(Plan {
                rename_to: Some("/remote/live.log".into()),
                ..plan("/remote/.live.log.poros-part", 100)
            })
            .unwrap();
        finish(&mut queue, id, RunOutcome::Completed);
        queue.clear(&[JobState::Done]);
        assert!(queue.take_abandoned().is_empty());
    }

    #[test]
    fn a_temporary_file_another_job_writes_is_not_deleted() {
        let no_retries = TransferSettings {
            retry_attempts: 0,
            ..settings()
        };
        let mut queue = Queue::new();
        let failed = queue.add(file("live.log", 100));
        fail_check(&mut queue, failed, &no_retries);
        assert!(queue.take_abandoned().is_empty());

        // The same file queued again and running.
        let again = queue.add(file("live.log", 100));
        let claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(claim.id, again);
        queue.clear(&[JobState::Failed]);
        assert!(queue.take_abandoned().is_empty());

        // Paused part-way, it holds the file to continue from.
        let failed = queue.add(file("live.log", 100));
        claim
            .run
            .plan
            .set(Plan {
                rename_to: Some("/remote/live.log".into()),
                ..plan("/remote/.live.log.poros-part", 100)
            })
            .unwrap();
        claim.run.start_pieces(Pieces::new(0, 100, 10));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.complete_piece(piece.start);
        queue.pause(&[again]);
        finish(&mut queue, again, RunOutcome::Stopped);
        fail_check(&mut queue, failed, &no_retries);
        queue.clear(&[JobState::Failed]);
        assert!(queue.take_abandoned().is_empty());

        // Once nothing holds it, it goes with the last job.
        queue.remove(&[again]);
        assert_eq!(
            queue.take_abandoned(),
            vec![Abandoned {
                session_id: "session".into(),
                direction: Direction::Upload,
                path: "/remote/.live.log.poros-part".into(),
                directory: "/remote".into(),
            }]
        );
    }

    #[test]
    fn deleting_a_file_reports_its_folder() {
        let mut queue = Queue::new();
        let id = queue.add(JobSpec {
            direction: Direction::Download,
            source: "/remote/live.log".into(),
            target: "/local/live.log".into(),
            target_directory: "/local".into(),
            ..file("live.log", 100)
        });
        let claim = queue.claim(Instant::now()).unwrap();
        claim.run.plan.set(plan("/local/live.log", 100)).unwrap();
        claim.run.restart.store(true, Ordering::Relaxed);
        queue.take_changes();
        // Failed its check, it was deleted where it lay.
        let failed = RunOutcome::Failed(AppError::integrity("live.log changed meanwhile"));
        finish(&mut queue, id, failed);
        let (_, _, changed) = queue.take_changes();
        assert_eq!(
            changed,
            vec![ChangedDirectory {
                side: Side::Local,
                session_id: None,
                path: "/local".into(),
            }]
        );
        queue.remove(&[id]);
        assert!(queue.take_abandoned().is_empty());

        // A removed job's temporary file, once it is deleted and not before.
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
        queue.remove(&[id]);
        finish(&mut queue, id, RunOutcome::Stopped);
        let abandoned = queue.take_abandoned();
        assert_eq!(abandoned.len(), 1);
        assert!(queue.take_changes().2.is_empty());
        queue.deleted(&abandoned[0]);
        assert_eq!(
            queue.take_changes().2,
            vec![ChangedDirectory {
                side: Side::Remote,
                session_id: Some("session".into()),
                path: "/remote".into(),
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

    fn download(name: &str, size: u64) -> JobSpec {
        JobSpec {
            direction: Direction::Download,
            source: format!("/remote/{name}"),
            target: format!("/local/{name}"),
            target_directory: "/local".into(),
            ..file(name, size)
        }
    }

    const UPLOADS_HELD: Held = Held {
        upload: true,
        download: false,
    };

    #[test]
    fn a_held_direction_waits_with_its_progress_while_the_other_runs() {
        let mut queue = Queue::new();
        let upload = queue.add(file("up", 100));
        queue.add(download("down", 1));
        queue.add(JobSpec {
            direction: Direction::Relay,
            source_session_id: Some("other".into()),
            ..file("across", 1)
        });
        let claim = queue.claim(Instant::now()).unwrap();
        claim.run.plan.set(plan("/remote/up", 100)).unwrap();
        claim.run.start_pieces(Pieces::new(0, 100, 10));
        let piece = claim.run.claim_piece().unwrap();
        claim.run.add_progress(10);
        claim.run.complete_piece(piece.start);

        assert!(queue.set_held(UPLOADS_HELD, Instant::now()));
        assert!(!queue.set_held(UPLOADS_HELD, Instant::now()));
        assert!(claim.run.cancel.is_cancelled());
        finish(&mut queue, upload, RunOutcome::Stopped);
        let job = queue.job(upload).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.attempts, 0);
        assert_eq!(job.resume.as_ref().map(|resume| resume.offset), Some(10));

        // Downloads go on. A relay writes to a server too, so it waits with the uploads.
        assert_eq!(claim_name(&mut queue).as_deref(), Some("down"));
        assert_eq!(claim_name(&mut queue), None);
        // Waiting for the schedule is not having finished.
        let counts = queue.counts();
        assert_eq!((counts.queued, counts.running), (2, 1));
        assert!(!counts.is_idle());
        assert_eq!(queue.remaining_bytes(), 92);

        assert!(queue.set_held(Held::default(), Instant::now()));
        let claim = queue.claim(Instant::now()).unwrap();
        assert_eq!(claim.spec.name, "up");
        assert_eq!(claim.resume.map(|resume| resume.offset), Some(10));
        assert_eq!(claim_name(&mut queue).as_deref(), Some("across"));
    }

    #[test]
    fn a_hold_leaves_pauses_and_retries_as_they_were() {
        let mut queue = Queue::new();
        let paused_by_hand = queue.add(file("by hand", 1));
        let retried = queue.add(file("retried", 1));
        queue.add(file("waiting", 1));

        // Paused by the user as the hold begins: it stays paused when the hold ends.
        let claim = queue.claim(Instant::now()).unwrap();
        queue.pause(&[paused_by_hand]);
        queue.set_held(UPLOADS_HELD, Instant::now());
        assert!(claim.run.cancel.is_cancelled());
        finish(&mut queue, paused_by_hand, RunOutcome::Stopped);
        assert_eq!(queue.job(paused_by_hand).unwrap().state, JobState::Paused);

        // A retry that falls due during the hold waits for it, and costs no further attempt.
        queue.set_held(Held::default(), Instant::now());
        queue.claim(Instant::now()).unwrap();
        let refused = AppError::new(ErrorKind::Sftp, "failure");
        finish(&mut queue, retried, RunOutcome::Failed(refused));
        assert_eq!(queue.job(retried).unwrap().attempts, 1);
        queue.set_held(UPLOADS_HELD, Instant::now());
        let after_the_delay = Instant::now() + Duration::from_secs(3600);
        assert!(queue.claim(after_the_delay).is_none());
        assert_eq!(queue.job(retried).unwrap().state, JobState::Queued);
        assert_eq!(queue.job(retried).unwrap().attempts, 1);

        // A paused queue stays paused when the hold ends, and a held one held when started.
        queue.set_paused(true, Instant::now());
        queue.set_held(Held::default(), Instant::now());
        assert!(queue.claim(after_the_delay).is_none());
        queue.set_held(UPLOADS_HELD, Instant::now());
        queue.set_paused(false, Instant::now());
        assert!(queue.claim(after_the_delay).is_none());
        queue.set_held(Held::default(), Instant::now());
        let claim = queue.claim(after_the_delay).unwrap();
        assert_eq!(claim.spec.name, "retried");
        assert_eq!(claim_name(&mut queue).as_deref(), Some("waiting"));
        assert_eq!(claim_name(&mut queue), None);

        // Resumed by hand during a hold, a job joins the ones that wait for it.
        queue.set_held(UPLOADS_HELD, Instant::now());
        queue.resume(&[paused_by_hand]);
        assert_eq!(queue.job(paused_by_hand).unwrap().state, JobState::Queued);
        assert!(queue.claim(after_the_delay).is_none());
    }

    #[test]
    fn a_hold_or_a_paused_queue_does_not_use_up_the_reconnect_window() {
        let settings = TransferSettings {
            reconnect_minutes: 1,
            retry_delay_secs: 5,
            ..settings()
        };
        let seconds = Duration::from_secs;
        let lost_again = |retry_in| Some(ServerChange::Lost { retry_in });
        let gave_up = Some(ServerChange::GaveUp { failed: 2 });
        let mut queue = Queue::new();
        let first = queue.add(file("a", 1));
        let second = queue.add(file("b", 1));
        let start = Instant::now();
        queue.claim(start).unwrap();
        drop_connection(&mut queue, first, &settings, start);

        // The schedule holds the uploads for far longer than the reconnect window. Nothing
        // tried the server meanwhile, so one more dropped connection waits for it again, with
        // the next delay rather than the first.
        queue.set_held(UPLOADS_HELD, start);
        let released = start + seconds(3600);
        queue.set_held(Held::default(), released);
        assert_eq!(queue.claim(released).unwrap().id, first);
        assert_eq!(
            drop_connection(&mut queue, first, &settings, released),
            lost_again(seconds(10))
        );
        assert_eq!(queue.job(first).unwrap().state, JobState::Queued);
        assert_eq!(queue.job(second).unwrap().state, JobState::Queued);
        // A new session to the server since the connection dropped still ends the wait.
        assert_eq!(
            queue.waiting_servers(),
            vec![("session".to_string(), start)]
        );

        // The same for a queue paused that long.
        queue.set_paused(true, released);
        let started = released + seconds(3600);
        queue.set_paused(false, started);
        assert_eq!(queue.claim(started).unwrap().id, first);
        assert_eq!(
            drop_connection(&mut queue, first, &settings, started),
            lost_again(seconds(20))
        );
        assert_eq!(queue.job(second).unwrap().state, JobState::Queued);

        // Within the window, a server that stays out of reach is still given up on.
        let late = started + seconds(61);
        assert_eq!(queue.claim(late).unwrap().id, first);
        assert_eq!(drop_connection(&mut queue, first, &settings, late), gave_up);

        // Holds within the window only add the time they took, so the window still runs out
        // however often the schedule lets the jobs go: here 20 seconds of holds make it end
        // 80 seconds after the connection dropped.
        let mut queue = Queue::new();
        let first = queue.add(file("a", 1));
        queue.add(file("b", 1));
        queue.claim(start).unwrap();
        drop_connection(&mut queue, first, &settings, start);
        for held_at in [10, 30] {
            queue.set_held(UPLOADS_HELD, start + seconds(held_at));
            queue.set_held(Held::default(), start + seconds(held_at + 10));
        }
        let within = start + seconds(75);
        assert_eq!(queue.claim(within).unwrap().id, first);
        assert_eq!(
            drop_connection(&mut queue, first, &settings, within),
            lost_again(seconds(10))
        );
        let past = start + seconds(85);
        assert_eq!(queue.claim(past).unwrap().id, first);
        assert_eq!(drop_connection(&mut queue, first, &settings, past), gave_up);

        // Downloads from the server could go on while its uploads were held, so the hold
        // takes nothing off the window.
        let mut queue = Queue::new();
        let first = queue.add(file("a", 1));
        queue.add(download("down", 1));
        queue.claim(start).unwrap();
        drop_connection(&mut queue, first, &settings, start);
        queue.set_held(UPLOADS_HELD, start);
        queue.set_held(Held::default(), start + seconds(50));
        let past = start + seconds(60);
        assert_eq!(queue.claim(past).unwrap().id, first);
        assert_eq!(drop_connection(&mut queue, first, &settings, past), gave_up);
    }

    #[test]
    fn a_held_job_waiting_to_retry_does_not_wake_the_workers() {
        let mut queue = Queue::new();
        let retried = queue.add(file("retried", 1));
        queue.claim(Instant::now()).unwrap();
        let refused = AppError::new(ErrorKind::Sftp, "failure");
        finish(&mut queue, retried, RunOutcome::Failed(refused));
        assert!(queue.has_delayed_jobs());

        queue.set_held(UPLOADS_HELD, Instant::now());
        assert!(!queue.has_delayed_jobs());
        queue.set_held(Held::default(), Instant::now());
        assert!(queue.has_delayed_jobs());
    }
}
