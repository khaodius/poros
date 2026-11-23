//! Jobs queued together for a caller that follows them to their end with no one watching, such
//! as one run of a scheduled task. A folder hands its batch on to what it holds, so the batch
//! covers the whole tree however it expands. Its jobs start even while transfers wait for Start,
//! since no one is there to press it, but never while the user has paused the queue.

use std::sync::{Arc, Mutex};

use super::conflict::ExistsAction;
use super::queue::{Job, JobKind, JobState};

/// How many failures a tally describes; the log has every one.
const MAX_FAILURES_KEPT: usize = 5;

#[derive(Debug)]
pub struct JobBatch {
    /// Shown beside its jobs in the transfer lists: the name of the task that queued them.
    pub label: String,
    /// What happens to a file already at the target, for jobs that do not say. No one is there
    /// to answer, so a batch queued with `Ask` overwrites.
    pub exists_action: Option<ExistsAction>,
    /// Someone follows the jobs, so they go while transfers wait for Start. Jobs brought back
    /// from the saved queue keep their label but are followed by no one.
    pub unattended: bool,
    /// The jobs that have left the queue. The ones still in it are counted by their state.
    departed: Mutex<BatchTally>,
}

/// How the jobs of a batch ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchTally {
    pub done: u64,
    pub skipped: u64,
    pub failed: u64,
    /// Taken off the queue before they finished.
    pub removed: u64,
    /// Still waiting when the queue shut down.
    pub unfinished: u64,
    /// Bytes of the files that are done.
    pub bytes: u64,
    /// What went wrong with the first few that failed.
    pub failures: Vec<String>,
}

impl BatchTally {
    /// Counts a job by the state it is in. Folders that listed well are no one's file, so only
    /// the ones that could not be listed count.
    pub(super) fn count(&mut self, job: &Job) {
        match job.state {
            JobState::Done if job.spec.kind == JobKind::File => {
                self.done += 1;
                self.bytes += job.spec.size;
            }
            JobState::Done => {}
            JobState::Skipped => self.skipped += 1,
            JobState::Failed => {
                self.failed += 1;
                if self.failures.len() < MAX_FAILURES_KEPT {
                    self.failures.push(format!(
                        "{}: {}",
                        job.spec.source,
                        job.error.as_deref().unwrap_or("failed")
                    ));
                }
            }
            JobState::Queued | JobState::Running | JobState::Paused | JobState::Conflict => {
                self.unfinished += 1;
            }
        }
    }

    /// Every file arrived or was rightly passed over.
    pub fn is_clean(&self) -> bool {
        self.failed == 0 && self.removed == 0 && self.unfinished == 0
    }
}

impl JobBatch {
    pub fn new(label: impl Into<String>, exists_action: Option<ExistsAction>) -> Arc<Self> {
        Arc::new(Self::with(label.into(), exists_action, true))
    }

    /// The batch of a job saved with the queue, brought back by the next start.
    pub(super) fn restored(label: String, exists_action: Option<ExistsAction>) -> Arc<Self> {
        Arc::new(Self::with(label, exists_action, false))
    }

    fn with(label: String, exists_action: Option<ExistsAction>, unattended: bool) -> Self {
        Self {
            label,
            exists_action: exists_action.map(|action| match action {
                ExistsAction::Ask => ExistsAction::Overwrite,
                action => action,
            }),
            unattended,
            departed: Mutex::new(BatchTally::default()),
        }
    }

    /// Notes a job leaving the queue: finished, or removed before it was.
    pub(super) fn departs(&self, job: &Job) {
        let mut departed = self.departed.lock().unwrap();
        let mut left = BatchTally::default();
        left.count(job);
        departed.done += left.done;
        departed.skipped += left.skipped;
        departed.failed += left.failed;
        departed.removed += left.unfinished;
        departed.bytes += left.bytes;
        let room = MAX_FAILURES_KEPT.saturating_sub(departed.failures.len());
        departed
            .failures
            .extend(left.failures.into_iter().take(room));
    }

    /// How the jobs that have left the queue ended.
    pub(super) fn departed(&self) -> BatchTally {
        self.departed.lock().unwrap().clone()
    }
}
