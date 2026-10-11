//! Folder synchronization in the manner of rsync: list both sides, decide from size and time
//! (or contents) what differs, show that as a plan, then carry out the parts the user keeps.
//! Files go through the transfer queue, so they use its workers and rsync delta transfers.
//! rsync's options decide what is listed, compared, copied, deleted and kept.

mod chmod;
mod filter;
mod hash;
mod options;
mod owners;
mod plan;
mod run;
mod tree;

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult};
use crate::events::Events;
use crate::remote_path;
use crate::session::SessionManager;
use crate::transfer::TransferManager;
pub use chmod::Chmod;
use filter::Filter;
pub use options::{
    check as check_options, AppendMode, DeleteTiming, FilterFiles, LinkMode, OptionCheck,
    SyncOptions,
};
use owners::{Accounts, Owners};
pub use plan::{CompareMode, Counts, SyncAction, SyncDirection, SyncItem, SyncReason};
use run::{Run, Work};
use tree::{Counter, Role, Tree, Walk};

/// Plans kept for running; older ones are dropped.
const KEPT_PLANS: usize = 4;
/// Finished runs kept for whoever waits on them.
const KEPT_REPORTS: usize = 16;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
/// Larger include, exclude and files-from lists are refused.
const MAX_LIST_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Which side of a synchronization something is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Place {
    Local,
    Remote,
}

impl Place {
    fn other(self) -> Self {
        match self {
            Self::Local => Self::Remote,
            Self::Remote => Self::Local,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    /// Names this comparison for progress events and cancelling.
    pub request_id: String,
    pub session_id: String,
    pub local_path: String,
    pub remote_path: String,
    pub direction: SyncDirection,
    #[serde(flatten)]
    pub options: SyncOptions,
    /// rsync arguments applied over `options`, typed as rsync takes them.
    #[serde(default)]
    pub arguments: String,
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
    /// What the user should know: notes on the options and on the plan.
    pub notes: Vec<String>,
    /// The options as the rsync command line they amount to.
    pub arguments: String,
    pub dry_run: bool,
    /// The folder being synchronized to does not exist yet, and is created.
    pub creates_target: bool,
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
    pub run_id: String,
    /// Nothing was changed; the counts are what the run would have done.
    pub dry_run: bool,
    pub queued_files: u64,
    pub queued_bytes: u64,
    pub deleted: u64,
    /// Deletions that wait until the queued files are copied.
    pub deleting_later: u64,
    pub created_folders: u64,
    pub created_links: u64,
    /// Files whose time, permissions or owner were set without copying them.
    pub updated: u64,
    /// Unchanged files removed from the source by `--remove-source-files`.
    pub removed_sources: u64,
    pub failures: Vec<String>,
}

/// How a run ended, once the queue is done with its files.
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub copied: u64,
    pub failed: u64,
    pub bytes: u64,
    pub deleted: u64,
    pub failures: Vec<String>,
}

struct StoredPlan {
    id: String,
    session_id: String,
    direction: SyncDirection,
    options: SyncOptions,
    plan_options: plan::Options,
    /// The options as rsync arguments, for the log.
    arguments: String,
    local: Tree,
    remote: Tree,
    items: Vec<SyncItem>,
    counts: Counts,
    notes: Vec<String>,
    unchanged_files: Vec<String>,
    owners: Owners,
}

type Report = watch::Receiver<Option<SyncReport>>;

pub struct SyncManager {
    sessions: Arc<SessionManager>,
    events: Events,
    comparisons: Mutex<HashMap<String, CancellationToken>>,
    plans: Mutex<VecDeque<Arc<StoredPlan>>>,
    reports: Mutex<VecDeque<(String, Report)>>,
}

impl SyncManager {
    pub fn new(sessions: Arc<SessionManager>, events: Events) -> Self {
        Self {
            sessions,
            events,
            comparisons: Mutex::new(HashMap::new()),
            plans: Mutex::new(VecDeque::new()),
            reports: Mutex::new(VecDeque::new()),
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
            notes: stored.notes.clone(),
            arguments: stored.arguments.clone(),
            dry_run: stored.options.dry_run,
            creates_target: stored.local.missing || stored.remote.missing,
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
        let direction = request.direction;
        let checked = check_options(request.options.clone(), &[&request.arguments], direction);
        if !checked.errors.is_empty() {
            return Err(AppError::invalid(checked.errors.join("; ")));
        }
        let options = checked.options;
        let filter = {
            let options = options.clone();
            let local_root = request.local_path.clone();
            tokio::task::spawn_blocking(move || build_filter(&options, &local_root)).await??
        };
        let plan_options = plan_options(&options, direction);
        let session = self.sessions.get(&request.session_id).await?;
        // A channel of its own, so listing does not hold up the file panes.
        let fs = session.open_channel().await?;
        let compared = async {
            let (local_role, remote_role) = match direction {
                SyncDirection::Upload => (Role::Sender, Role::Receiver),
                SyncDirection::Download => (Role::Receiver, Role::Sender),
                SyncDirection::Both => (Role::Both, Role::Both),
            };
            let local_walk = {
                let root = request.local_path.clone();
                let walk = walk_for(&options, filter.clone(), local_role);
                let counter = progress.local_entries.clone();
                let cancel = cancel.clone();
                tokio::task::spawn_blocking(move || {
                    tree::walk_local(&root, &walk, &counter, &cancel)
                })
            };
            let remote_root = fs.resolve(&request.remote_path);
            let remote_walk = walk_for(&options, filter.clone(), remote_role);
            let remote_listing = tree::walk_remote(
                &fs,
                &remote_root,
                &remote_walk,
                &progress.remote_entries,
                cancel,
            );
            let (local, remote) = tokio::join!(local_walk, remote_listing);
            let (local, remote) = (local??, remote?);
            if !options.mkpath {
                if local.missing {
                    let parent = Path::new(&local.root).parent();
                    if !parent.is_some_and(Path::is_dir) {
                        return Err(missing_parent(&local.root));
                    }
                }
                if remote.missing {
                    let parent = remote_path::parent(&remote.root).unwrap_or_else(|| "/".into());
                    if !fs.stat(&parent).await?.is_some_and(|stat| stat.is_dir) {
                        return Err(missing_parent(&remote.root));
                    }
                }
            }

            let to_hash = plan::files_to_hash(&local, &remote, &plan_options);
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
            let plan = plan::plan(&local, &remote, &plan_options, &same_contents);
            let owners = if Owners::wanted(&options) {
                let local_accounts = tokio::task::spawn_blocking(Accounts::local).await?;
                let remote_accounts = Accounts::remote(&fs).await;
                Owners::new(
                    &options,
                    local_accounts,
                    remote_accounts,
                    receivers(direction),
                )
                .map_err(AppError::invalid)?
            } else {
                Owners::default()
            };
            AppResult::Ok((local, remote, plan, owners))
        }
        .await;
        fs.close();
        let (local, remote, plan, owners) = compared?;
        let mut notes = checked.warnings;
        if local.missing || remote.missing {
            notes.push("The folder being synchronized to does not exist yet; it is created".into());
        }
        notes.extend(plan.notes);
        Ok(StoredPlan {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: request.session_id.clone(),
            direction,
            arguments: checked.arguments,
            options,
            plan_options,
            local,
            remote,
            items: plan.items,
            counts: plan.counts,
            notes,
            unchanged_files: plan.unchanged_files,
            owners,
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

    /// Carries out the chosen items up to queueing the files; what follows the transfers runs
    /// on its own, and [`Self::finished`] waits for it.
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
        let fs = session.sftp()?.clone();
        let run = Arc::new(Run::new(stored.clone(), fs, self.events.clone()));
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
            work.add(&run, item, choice.action);
        }

        let (sender, receiver) = watch::channel(None);
        {
            let mut reports = self.reports.lock().unwrap();
            reports.push_back((run.id.clone(), receiver));
            while reports.len() > KEPT_REPORTS {
                reports.pop_front();
            }
        }
        let session_id = stored.session_id.clone();
        let outcome = run::start(run, work, |files| {
            transfers.enqueue_files(&session_id, files)
        })
        .await?;
        match outcome.follow_up {
            Some(follow_up) => {
                tokio::spawn(follow_up.finish(transfers.watch(), sender));
            }
            None => {
                let _ = sender.send(Some(SyncReport::default()));
            }
        }
        Ok(outcome.summary)
    }

    /// Waits until a run is over, delayed deletions and folder attributes included. `None`
    /// once the run is no longer known or failed before queueing anything.
    pub async fn finished(&self, run_id: &str) -> Option<SyncReport> {
        let mut receiver = self
            .reports
            .lock()
            .unwrap()
            .iter()
            .find(|(id, _)| id == run_id)
            .map(|(_, receiver)| receiver.clone())?;
        let report = receiver.wait_for(Option::is_some).await.ok()?.clone();
        report
    }
}

fn missing_parent(root: &str) -> AppError {
    AppError::invalid(format!(
        "The folder above {root} does not exist; --mkpath creates the whole path"
    ))
}

/// The sides that receive files.
fn receivers(direction: SyncDirection) -> &'static [Place] {
    match direction {
        SyncDirection::Upload => &[Place::Remote],
        SyncDirection::Download => &[Place::Local],
        SyncDirection::Both => &[Place::Local, Place::Remote],
    }
}

/// The filter rules, with lists and merge files read relative to the local folder.
fn build_filter(options: &SyncOptions, local_root: &str) -> AppResult<Filter> {
    let read = |path: &str| read_list(local_root, path);
    let mut spec = options.filter_spec(&read)?;
    // rsync keeps --delete away from a backup folder inside the target.
    let backup_dir = options.backup_dir.trim();
    if options.backup
        && !backup_dir.is_empty()
        && !backup_dir.starts_with('/')
        && !Path::new(backup_dir).is_absolute()
    {
        spec.rules
            .push(format!("P /{}/", backup_dir.trim_matches('/')));
    }
    Filter::build(&spec, &read)
}

fn read_list(local_root: &str, path: &str) -> AppResult<String> {
    let given = Path::new(path.trim());
    let path = if given.is_absolute() {
        given.to_path_buf()
    } else {
        Path::new(local_root).join(given)
    };
    let shown = path.to_string_lossy().into_owned();
    let metadata =
        std::fs::metadata(&path).map_err(|error| AppError::from(error).with_path(shown.clone()))?;
    if metadata.len() > MAX_LIST_FILE_BYTES {
        return Err(AppError::invalid(format!(
            "{shown} is larger than {} MB",
            MAX_LIST_FILE_BYTES / (1024 * 1024)
        )));
    }
    std::fs::read_to_string(&path).map_err(|error| AppError::from(error).with_path(shown))
}

fn walk_for(options: &SyncOptions, filter: Filter, role: Role) -> Walk {
    Walk {
        recursive: options.recursive,
        dirs: options.dirs,
        links: options.links,
        safe_links: options.safe_links,
        copy_unsafe_links: options.copy_unsafe_links,
        copy_dirlinks: options.copy_dirlinks,
        keep_dirlinks: options.keep_dirlinks,
        owners: options.owner || options.group,
        // The receiving folder is created when it is missing.
        allow_missing: role == Role::Receiver,
        ..Walk::new(filter, role)
    }
}

fn plan_options(options: &SyncOptions, direction: SyncDirection) -> plan::Options {
    plan::Options {
        delete_extraneous: options.deletes(direction),
        skip_newer_on_target: options.skip_newer_on_target,
        ignore_existing: options.ignore_existing,
        existing_only: options.existing_only,
        time_tolerance_secs: options.time_tolerance(),
        min_size: options.min_size,
        max_size: options.max_size,
        prune_empty_dirs: options.prune_empty_dirs,
        max_delete: options.max_delete,
        force: options.force,
        ignore_errors: options.ignore_errors,
        append: options.append != AppendMode::Off,
        times: options.times,
        perms: options.perms,
        chmod: options.chmod_rules(),
        ..plan::Options::new(direction, options.compare)
    }
}

fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_read_old_and_new_fields() {
        let request: SyncRequest = serde_json::from_value(serde_json::json!({
            "requestId": "r",
            "sessionId": "s",
            "localPath": "/home/me/site",
            "remotePath": "/var/www",
            "direction": "upload",
            "compare": "checksum",
            "deleteExtraneous": true,
            "excludes": ["*.tmp"],
            "maxDelete": 10,
            "links": "preserve",
            "arguments": "--chmod=F644"
        }))
        .unwrap();
        assert_eq!(request.options.compare, CompareMode::Checksum);
        assert!(request.options.delete_extraneous);
        assert_eq!(request.options.excludes, ["*.tmp"]);
        assert_eq!(request.options.max_delete, Some(10));
        assert_eq!(request.options.links, LinkMode::Preserve);
        assert_eq!(request.options.time_tolerance_secs, 2);
        assert!(request.options.recursive && request.options.times);
        assert_eq!(request.arguments, "--chmod=F644");
    }

    #[test]
    fn lists_read_relative_to_the_local_folder() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("excludes.txt"), "*.log\n").unwrap();
        let root = folder.path().to_string_lossy().into_owned();
        assert_eq!(read_list(&root, "excludes.txt").unwrap(), "*.log\n");
        assert!(read_list(&root, "missing.txt").is_err());

        let options = SyncOptions {
            exclude_from: vec!["excludes.txt".into()],
            backup: true,
            backup_dir: "old".into(),
            ..SyncOptions::default()
        };
        let filter = build_filter(&options, &root).unwrap();
        let rules = filter::FolderRules::default();
        let verdict = |path, is_dir, side| filter.verdict(path, is_dir, side, &rules);
        assert!(!verdict("debug.log", false, filter::Side::Sender).is_included());
        // The backup folder is kept from deletion, not from copying.
        assert!(verdict("old", true, filter::Side::Sender).is_included());
        assert!(!verdict("old", true, filter::Side::Receiver).is_included());
    }
}
