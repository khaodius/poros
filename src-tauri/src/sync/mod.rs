//! Folder synchronization in the manner of rsync: list both sides, decide from size and time
//! (or contents) what differs, show that as a plan, then carry out the parts the user keeps.
//! Files go through the transfer queue, so they use its workers and rsync delta transfers.

mod filter;
mod hash;
mod plan;
mod tree;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel};
use crate::format::format_size;
use crate::local;
use crate::remote_path;
use crate::session::SessionManager;
use crate::transfer::{Direction, FileTransfer, TransferManager};
use filter::Filter;
pub use plan::{CompareMode, Counts, SyncAction, SyncDirection, SyncItem, SyncReason};
use plan::{Options, Plan};
use tree::{Counter, Tree};

/// Plans kept for running; older ones are dropped.
const KEPT_PLANS: usize = 4;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
const DEFAULT_TIME_TOLERANCE_SECS: u32 = 2;
const MAX_TIME_TOLERANCE_SECS: u32 = 24 * 60 * 60;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    /// Names this comparison for progress events and cancelling.
    pub request_id: String,
    pub session_id: String,
    pub local_path: String,
    pub remote_path: String,
    pub direction: SyncDirection,
    pub compare: CompareMode,
    #[serde(default)]
    pub delete_extraneous: bool,
    #[serde(default)]
    pub skip_newer_on_target: bool,
    #[serde(default)]
    pub ignore_existing: bool,
    #[serde(default = "default_time_tolerance")]
    pub time_tolerance_secs: u32,
    #[serde(default)]
    pub excludes: Vec<String>,
}

fn default_time_tolerance() -> u32 {
    DEFAULT_TIME_TOLERANCE_SECS
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlanView {
    pub plan_id: String,
    pub local_root: String,
    pub remote_root: String,
    pub items: Vec<SyncItem>,
    pub counts: Counts,
    /// Paths that were passed over, a sample for the user to check.
    pub passed_over: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncStage {
    Listing,
    Comparing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub request_id: String,
    pub stage: SyncStage,
    pub local_entries: u64,
    pub remote_entries: u64,
    pub compared: u64,
    pub to_compare: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRunRequest {
    pub plan_id: String,
    pub choices: Vec<SyncChoice>,
}

/// An item to carry out; a conflict between two files may be settled either way.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncChoice {
    pub id: u32,
    pub action: SyncAction,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRunSummary {
    pub queued_files: u64,
    pub queued_bytes: u64,
    pub deleted: u64,
    pub created_folders: u64,
    pub failures: Vec<String>,
}

struct StoredPlan {
    id: String,
    session_id: String,
    local: Tree,
    remote: Tree,
    items: Vec<SyncItem>,
    counts: Counts,
}

pub struct SyncManager {
    sessions: Arc<SessionManager>,
    events: Events,
    comparisons: Mutex<HashMap<String, CancellationToken>>,
    plans: Mutex<VecDeque<Arc<StoredPlan>>>,
}

impl SyncManager {
    pub fn new(sessions: Arc<SessionManager>, events: Events) -> Self {
        Self {
            sessions,
            events,
            comparisons: Mutex::new(HashMap::new()),
            plans: Mutex::new(VecDeque::new()),
        }
    }

    pub async fn compare(&self, request: SyncRequest) -> AppResult<SyncPlanView> {
        let cancel = CancellationToken::new();
        if let Some(previous) = self
            .comparisons
            .lock()
            .unwrap()
            .insert(request.request_id.clone(), cancel.clone())
        {
            previous.cancel();
        }
        let progress = Progress::default();
        let reporter = tokio::spawn(report_progress(
            self.events.clone(),
            request.request_id.clone(),
            progress.clone(),
            cancel.child_token(),
        ));
        let result = self.build_plan(&request, &progress, &cancel).await;
        reporter.abort();
        self.comparisons.lock().unwrap().remove(&request.request_id);
        let stored = Arc::new(result?);
        let view = SyncPlanView {
            plan_id: stored.id.clone(),
            local_root: stored.local.root.clone(),
            remote_root: stored.remote.root.clone(),
            items: stored.items.clone(),
            counts: stored.counts.clone(),
            passed_over: stored
                .local
                .passed_over
                .iter()
                .chain(&stored.remote.passed_over)
                .take(50)
                .cloned()
                .collect(),
        };
        let mut plans = self.plans.lock().unwrap();
        plans.push_back(stored);
        while plans.len() > KEPT_PLANS {
            plans.pop_front();
        }
        Ok(view)
    }

    async fn build_plan(
        &self,
        request: &SyncRequest,
        progress: &Progress,
        cancel: &CancellationToken,
    ) -> AppResult<StoredPlan> {
        let filter = Filter::new(&request.excludes)?;
        let options = Options {
            direction: request.direction,
            compare: request.compare,
            delete_extraneous: request.delete_extraneous
                && request.direction != SyncDirection::Both,
            skip_newer_on_target: request.skip_newer_on_target,
            ignore_existing: request.ignore_existing,
            time_tolerance_secs: i64::from(
                request.time_tolerance_secs.min(MAX_TIME_TOLERANCE_SECS),
            ),
        };
        let session = self.sessions.get(&request.session_id).await?;
        // A channel of its own, so listing does not hold up the file panes.
        let fs = session.open_channel().await?;
        let compared = async {
            let local_walk = {
                let root = request.local_path.clone();
                let filter = filter.clone();
                let counter = progress.local_entries.clone();
                let cancel = cancel.clone();
                tokio::task::spawn_blocking(move || {
                    tree::walk_local(&root, &filter, &counter, &cancel)
                })
            };
            let remote_root = fs.resolve(&request.remote_path);
            let remote_walk =
                tree::walk_remote(&fs, &remote_root, &filter, &progress.remote_entries, cancel);
            let (local, remote) = tokio::join!(local_walk, remote_walk);
            let (local, remote) = (local??, remote?);

            let to_hash = plan::files_to_hash(&local, &remote, &options);
            let same_contents = if to_hash.is_empty() {
                HashMap::new()
            } else {
                progress
                    .to_compare
                    .store(to_hash.len() as u64, Ordering::Relaxed);
                hash::same_contents(
                    &session,
                    &fs,
                    &local.root,
                    &remote.root,
                    &to_hash,
                    &progress.compared,
                    cancel,
                )
                .await?
            };
            let Plan { items, counts } = plan::plan(&local, &remote, &options, &same_contents);
            AppResult::Ok((local, remote, items, counts))
        }
        .await;
        fs.close();
        let (local, remote, items, counts) = compared?;
        Ok(StoredPlan {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: request.session_id.clone(),
            local,
            remote,
            items,
            counts,
        })
    }

    pub fn cancel(&self, request_id: &str) {
        if let Some(cancel) = self.comparisons.lock().unwrap().remove(request_id) {
            cancel.cancel();
        }
    }

    /// The folder a plan's deletions mirror, when it listed empty. Nobody reviews a scheduled
    /// run, so it leaves those deletions out.
    pub fn empty_mirrored_folder(&self, plan_id: &str) -> Option<String> {
        let plans = self.plans.lock().unwrap();
        let stored = plans.iter().find(|stored| stored.id == plan_id)?;
        plan::deletions_mirror_empty_folder(&stored.local, &stored.remote, &stored.items)
            .map(str::to_string)
    }

    pub fn discard(&self, plan_id: &str) {
        self.plans
            .lock()
            .unwrap()
            .retain(|stored| stored.id != plan_id);
    }

    pub async fn run(
        &self,
        transfers: &TransferManager,
        request: SyncRunRequest,
    ) -> AppResult<SyncRunSummary> {
        let stored = {
            let mut plans = self.plans.lock().unwrap();
            let position = plans
                .iter()
                .position(|stored| stored.id == request.plan_id)
                .ok_or_else(|| {
                    AppError::invalid("This comparison is no longer available; compare again")
                })?;
            plans.remove(position).expect("position is in range")
        };
        let session = self.sessions.get(&stored.session_id).await?;
        let local_root = stored.local.root.as_str();
        let remote_root = stored.remote.root.as_str();
        let mut summary = SyncRunSummary::default();
        let mut work = Work::default();
        for choice in &request.choices {
            let Some(item) = stored.items.get(choice.id as usize) else {
                continue;
            };
            let settles_conflict = item.action == SyncAction::Conflict
                && item.reason == SyncReason::BothChanged
                && matches!(choice.action, SyncAction::Upload | SyncAction::Download);
            if choice.action != item.action && !settles_conflict {
                continue;
            }
            work.add(&stored, item, choice.action, local_root, remote_root);
        }

        // Deletions first, so a folder replaced by a file of the same name is gone in time.
        for path in &work.delete_remote {
            match session.files().delete(std::slice::from_ref(path)).await {
                Ok(()) => summary.deleted += 1,
                Err(error) => summary.failures.push(format!("{path}: {}", error.message)),
            }
        }
        let delete_local = std::mem::take(&mut work.delete_local);
        let local_failures = tokio::task::spawn_blocking(move || {
            delete_local
                .iter()
                .filter_map(|path| {
                    local::delete(std::slice::from_ref(path))
                        .err()
                        .map(|error| format!("{path}: {}", error.message))
                })
                .collect::<Vec<_>>()
        })
        .await?;
        summary.deleted += work.local_deletions - local_failures.len() as u64;
        summary.failures.extend(local_failures);

        for folder in &work.remote_folders {
            match session.files().ensure_dir(folder).await {
                Ok(()) => summary.created_folders += 1,
                Err(error) => summary
                    .failures
                    .push(format!("{folder}: {}", error.message)),
            }
        }
        let local_folders = std::mem::take(&mut work.local_folders);
        let folder_failures = tokio::task::spawn_blocking(move || {
            local_folders
                .iter()
                .filter_map(|folder| {
                    std::fs::create_dir_all(folder)
                        .err()
                        .map(|error| format!("{folder}: {error}"))
                })
                .collect::<Vec<_>>()
        })
        .await?;
        summary.created_folders += work.local_folder_count - folder_failures.len() as u64;
        summary.failures.extend(folder_failures);

        summary.queued_files = work.files.len() as u64;
        summary.queued_bytes = work.files.iter().map(|file| file.size).sum();
        if !work.files.is_empty() {
            transfers
                .enqueue_files(&stored.session_id, work.files)
                .await?;
        }

        let mut message = format!(
            "Synchronizing {local_root} with {remote_root}: {} queued ({})",
            plural(summary.queued_files, "file"),
            format_size(summary.queued_bytes)
        );
        if summary.deleted > 0 {
            message.push_str(&format!(", {} deleted", plural(summary.deleted, "item")));
        }
        if summary.created_folders > 0 {
            message.push_str(&format!(
                ", {} created",
                plural(summary.created_folders, "folder")
            ));
        }
        self.events
            .log(LogLevel::Info, Some(&stored.session_id), message);
        for failure in &summary.failures {
            self.events
                .log(LogLevel::Error, Some(&stored.session_id), failure.clone());
        }
        Ok(summary)
    }
}

fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// What carrying out the chosen items takes, gathered before anything changes.
#[derive(Default)]
struct Work {
    delete_remote: Vec<String>,
    delete_local: Vec<String>,
    local_deletions: u64,
    remote_folders: Vec<String>,
    local_folders: Vec<String>,
    local_folder_count: u64,
    files: Vec<FileTransfer>,
}

impl Work {
    fn add(
        &mut self,
        stored: &StoredPlan,
        item: &SyncItem,
        action: SyncAction,
        local_root: &str,
        remote_root: &str,
    ) {
        let local_path = |relative: &str| {
            hash::local_path(local_root, relative)
                .to_string_lossy()
                .into_owned()
        };
        let remote_path = |relative: &str| remote_path::join(remote_root, relative);
        match action {
            SyncAction::DeleteRemote => self.delete_remote.push(remote_path(&item.path)),
            SyncAction::DeleteLocal => {
                self.delete_local.push(local_path(&item.path));
                self.local_deletions += 1;
            }
            SyncAction::Upload | SyncAction::Download => {
                let upload = action == SyncAction::Upload;
                let source = if upload {
                    &stored.local
                } else {
                    &stored.remote
                };
                let Some(node) = source.nodes.get(&item.path) else {
                    return;
                };
                let mut entries = vec![(&item.path, node)];
                if node.is_dir {
                    entries.extend(source.inside(&item.path));
                }
                for (relative, node) in entries {
                    if node.is_dir {
                        if upload {
                            self.remote_folders.push(remote_path(relative));
                        } else {
                            self.local_folders.push(local_path(relative));
                            self.local_folder_count += 1;
                        }
                        continue;
                    }
                    let name = relative.rsplit('/').next().unwrap_or(relative).to_string();
                    let parent = relative.rsplit_once('/').map_or("", |(parent, _)| parent);
                    let (source_path, target, target_directory) = if upload {
                        (
                            local_path(relative),
                            remote_path(relative),
                            if parent.is_empty() {
                                remote_root.to_string()
                            } else {
                                remote_path(parent)
                            },
                        )
                    } else {
                        let local_parent = if parent.is_empty() {
                            local_root.to_string()
                        } else {
                            local_path(parent)
                        };
                        (remote_path(relative), local_path(relative), local_parent)
                    };
                    self.files.push(FileTransfer {
                        direction: if upload {
                            Direction::Upload
                        } else {
                            Direction::Download
                        },
                        name,
                        source: source_path,
                        target,
                        target_directory,
                        size: node.size,
                    });
                }
            }
            SyncAction::Conflict => {}
        }
    }
}

#[derive(Clone, Default)]
struct Progress {
    local_entries: Counter,
    remote_entries: Counter,
    compared: Counter,
    to_compare: Counter,
}

async fn report_progress(
    events: Events,
    request_id: String,
    progress: Progress,
    stop: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = stop.cancelled() => return,
            _ = tokio::time::sleep(PROGRESS_INTERVAL) => {}
        }
        let to_compare = progress.to_compare.load(Ordering::Relaxed);
        events.sync_progress(&SyncProgress {
            request_id: request_id.clone(),
            stage: if to_compare > 0 {
                SyncStage::Comparing
            } else {
                SyncStage::Listing
            },
            local_entries: progress.local_entries.load(Ordering::Relaxed),
            remote_entries: progress.remote_entries.load(Ordering::Relaxed),
            compared: progress.compared.load(Ordering::Relaxed),
            to_compare,
        });
    }
}
