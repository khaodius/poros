//! Compares two folders, each on this computer or on a server of any kind, for the visual
//! comparison of two file panes: which of the folders both sides have hold the same files,
//! and how the others differ. Both sides are listed the way a two-way synchronization lists
//! them and the differences are the ones its plan finds, so a folder shown as the same is one
//! that synchronizing would leave alone. The panes compare the two folders on show themselves,
//! by the same rule (`src/lib/folderCompare.ts`); this fills in what is below them.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::filter::Filter;
use super::options::MAX_TIME_TOLERANCE_SECS;
use super::plan::{self, CompareMode, Plan, SyncAction, SyncDirection, SyncItem, SyncReason};
use super::tree::{self, Counter, Role, Tree, Walk};
use super::{report_progress, Progress, SyncManager};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::file_ops::Location;
use crate::protocol::RemoteFileSystem;
use crate::session::SessionManager;
use crate::sftp::RemoteFs;

/// Entries listed on both sides together before a comparison gives up, which bounds the
/// memory it takes.
const MAX_ENTRIES: u64 = 500_000;
/// Folders reported; past this the deepest are left out, and the panes show them as not
/// compared.
const MAX_FOLDERS: usize = 20_000;
const LIMIT_CHECK_INTERVAL: Duration = Duration::from_millis(100);
const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
/// The furthest apart two time zones are: UTC+14 and UTC-12.
const MAX_ZONE_OFFSET_HOURS: i64 = 26;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRef {
    pub location: Location,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCompareRequest {
    /// Names this comparison for progress events and cancelling, as `SyncRequest` does.
    pub request_id: String,
    pub left: FolderRef,
    pub right: FolderRef,
    /// Files of one size are the same whatever their times.
    #[serde(default)]
    pub size_only: bool,
    /// Times this close are the same (`--modify-window`).
    pub time_tolerance_secs: u32,
    /// Times a whole number of hours apart are the same, as when one side shows its local
    /// time for UTC.
    #[serde(default)]
    pub ignore_hour_offsets: bool,
    #[serde(default)]
    pub include_hidden: bool,
    /// Names that differ only in case are one name, as on Windows.
    #[serde(default)]
    pub ignore_case: bool,
}

/// How the files in one folder that both sides have compare, at any depth below it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    /// Relative to the compared folders, with `/` between components; in lower case when
    /// case is ignored.
    pub path: String,
    pub identical: u64,
    pub newer_left: u64,
    pub newer_right: u64,
    /// A folder on one side only counts the files in it, or itself when it has none.
    pub only_left: u64,
    pub only_right: u64,
    /// The same time and another size, or a file on one side and a folder on the other.
    pub different: u64,
    /// Folders that could not be read, links that lead nowhere and special files.
    pub not_compared: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderComparison {
    /// Shallowest first.
    pub folders: Vec<FolderSummary>,
    /// More folders than are reported were compared; the deepest are left out.
    pub truncated: bool,
    pub left_entries: u64,
    pub right_entries: u64,
}

/// When two files' times count as the same moment.
#[derive(Debug, Clone, Copy)]
struct TimeRules {
    tolerance: i64,
    ignore_hour_offsets: bool,
    left_coarse: bool,
    right_coarse: bool,
}

impl TimeRules {
    fn same_moment(&self, left: i64, right: i64) -> bool {
        let tolerance = self
            .tolerance
            .max(slack(left, self.left_coarse))
            .max(slack(right, self.right_coarse));
        let difference = left - right;
        if difference.abs() <= tolerance {
            return true;
        }
        if !self.ignore_hour_offsets {
            return false;
        }
        let hours = (difference + HOUR / 2).div_euclid(HOUR);
        hours != 0
            && hours.abs() <= MAX_ZONE_OFFSET_HOURS
            && (difference - hours * HOUR).abs() <= tolerance
    }
}

/// How far the real time may be from one a coarse listing gives: a day for a time at
/// midnight, which is how a listing with only a date reads, and a minute otherwise.
fn slack(time: i64, coarse: bool) -> i64 {
    match (coarse, time.rem_euclid(DAY) == 0) {
        (false, _) => 0,
        (true, true) => DAY,
        (true, false) => MINUTE,
    }
}

/// Gives each file on the right the time of the file it is compared with where the two count
/// as the same moment, so the plan, which knows one tolerance for every file, sees them as
/// equal.
fn align_times(left: &Tree, right: &mut Tree, rules: &TimeRules) {
    for (path, node) in &mut right.nodes {
        let Some(other) = left.nodes.get(path) else {
            continue;
        };
        if let (Some(left_time), Some(right_time)) = (other.modified, node.modified) {
            if !node.is_dir && !other.is_dir && rules.same_moment(left_time, right_time) {
                node.modified = Some(left_time);
            }
        }
    }
}

/// Spells every path in lower case, so names that differ only in case are one name. Where one
/// side has two names that differ only in case, which of them the other side's file stands for
/// is unknown: the first is kept and the name is passed over, so it is not compared at all, and
/// the other's contents are left out with it.
fn fold_case(tree: &mut Tree) {
    let mut folded = BTreeMap::new();
    let mut dropped = HashSet::new();
    for (path, node) in std::mem::take(&mut tree.nodes) {
        if within(&dropped, &path) {
            continue;
        }
        match folded.entry(path.to_lowercase()) {
            Entry::Occupied(kept) => {
                tree.passed_over.push(kept.key().clone());
                dropped.insert(path);
            }
            Entry::Vacant(free) => {
                free.insert(node);
            }
        }
    }
    tree.nodes = folded;
    for path in &mut tree.passed_over {
        *path = path.to_lowercase();
    }
}

/// Whether `path` is inside one of `folders`.
fn within(folders: &HashSet<String>, path: &str) -> bool {
    let mut current = path;
    while let Some((parent, _)) = current.rsplit_once('/') {
        if folders.contains(parent) {
            return true;
        }
        current = parent;
    }
    false
}

/// How something that is not the same on both sides differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Difference {
    OnlyLeft,
    OnlyRight,
    NewerLeft,
    NewerRight,
    Different,
}

/// A two-way plan's item read as a difference, with the left side in the local folder's
/// place: what would be uploaded is new or newer on the left.
fn difference(item: &SyncItem) -> Difference {
    match (item.action, item.reason) {
        (SyncAction::Upload, SyncReason::New) => Difference::OnlyLeft,
        (SyncAction::Download, SyncReason::New) => Difference::OnlyRight,
        (SyncAction::Upload, _) => Difference::NewerLeft,
        (SyncAction::Download, _) => Difference::NewerRight,
        // A two-way plan deletes nothing, so what is left are its conflicts.
        (SyncAction::Conflict | SyncAction::DeleteLocal | SyncAction::DeleteRemote, _) => {
            Difference::Different
        }
    }
}

/// The plan a two-way synchronization of the two trees would make, by the rules given.
fn two_way_plan(left: &Tree, right: &mut Tree, size_only: bool, rules: &TimeRules) -> Plan {
    align_times(left, right, rules);
    let compare = if size_only {
        CompareMode::SizeOnly
    } else {
        CompareMode::SizeAndTime
    };
    let options = plan::Options {
        time_tolerance_secs: rules.tolerance,
        ..plan::Options::new(SyncDirection::Both, compare)
    };
    plan::plan(left, right, &options, &HashMap::new())
}

/// Counts, for each folder both trees have, what the plan found in it at any depth.
fn summarize(left: &Tree, right: &Tree, plan: &Plan) -> (Vec<FolderSummary>, bool) {
    // A folder passed over, or inside one, was not compared, so nothing is said about it.
    let passed_over: HashSet<String> = left
        .passed_over
        .iter()
        .chain(&right.passed_over)
        .cloned()
        .collect();
    let mut folders: BTreeMap<&str, FolderSummary> = left
        .nodes
        .iter()
        .filter(|(path, node)| {
            node.is_dir
                && right.nodes.get(*path).is_some_and(|other| other.is_dir)
                && !passed_over.contains(*path)
                && !within(&passed_over, path)
        })
        .map(|(path, _)| (path.as_str(), FolderSummary::default()))
        .collect();
    let mut count = |path: &str, field: fn(&mut FolderSummary) -> &mut u64, amount: u64| {
        let mut current = path;
        while let Some((parent, _)) = current.rsplit_once('/') {
            if let Some(summary) = folders.get_mut(parent) {
                *field(summary) += amount;
            }
            current = parent;
        }
    };
    for item in &plan.items {
        let amount = if item.is_dir { item.files.max(1) } else { 1 };
        let field: fn(&mut FolderSummary) -> &mut u64 = match difference(item) {
            Difference::OnlyLeft => |summary| &mut summary.only_left,
            Difference::OnlyRight => |summary| &mut summary.only_right,
            Difference::NewerLeft => |summary| &mut summary.newer_left,
            Difference::NewerRight => |summary| &mut summary.newer_right,
            Difference::Different => |summary| &mut summary.different,
        };
        count(&item.path, field, amount);
    }
    for path in &plan.unchanged_files {
        count(path, |summary| &mut summary.identical, 1);
    }
    for path in left.passed_over.iter().chain(&right.passed_over) {
        count(path, |summary| &mut summary.not_compared, 1);
    }

    let mut summaries: Vec<FolderSummary> = folders
        .into_iter()
        .map(|(path, summary)| FolderSummary {
            path: path.to_string(),
            ..summary
        })
        .collect();
    let depth = |summary: &FolderSummary| summary.path.matches('/').count();
    summaries.sort_by(|first, second| {
        depth(first)
            .cmp(&depth(second))
            .then_with(|| first.path.cmp(&second.path))
    });
    let truncated = summaries.len() > MAX_FOLDERS;
    summaries.truncate(MAX_FOLDERS);
    (summaries, truncated)
}

/// One of the two folders, ready to be listed.
enum Side {
    Local {
        root: String,
    },
    /// On an SFTP channel of its own, as a synchronization lists a server, so the file panes
    /// are not held up.
    Sftp {
        fs: RemoteFs,
        root: String,
    },
    Server {
        files: Arc<dyn RemoteFileSystem>,
        root: String,
    },
}

impl Side {
    /// Whether the side's listings give times only to the minute or the day. The backend knows,
    /// from what the server offered, so the page is not asked.
    fn coarse_times(&self) -> bool {
        match self {
            Self::Local { .. } | Self::Sftp { .. } => false,
            Self::Server { files, .. } => files.coarse_times(),
        }
    }

    async fn open(sessions: &SessionManager, folder: &FolderRef) -> AppResult<Self> {
        let path = folder.path.trim();
        if path.is_empty() {
            return Err(AppError::invalid("There is no folder to compare"));
        }
        let Location::Remote { session_id } = &folder.location else {
            return Ok(Self::Local {
                root: path.to_string(),
            });
        };
        let session = sessions.get(session_id).await?;
        if session.sftp().is_ok() {
            let fs = session.open_channel().await?;
            let root = fs.resolve(path);
            return Ok(Self::Sftp { fs, root });
        }
        let files = session.files();
        let root = files.resolve(path);
        Ok(Self::Server { files, root })
    }

    async fn walk(
        &self,
        walk: &Walk,
        counter: &Counter,
        cancel: &CancellationToken,
    ) -> AppResult<Tree> {
        match self {
            Self::Local { root } => {
                let (root, walk) = (root.clone(), walk.clone());
                let (counter, cancel) = (counter.clone(), cancel.clone());
                tokio::task::spawn_blocking(move || {
                    tree::walk_local(&root, &walk, &counter, &cancel)
                })
                .await?
            }
            Self::Sftp { fs, root } => tree::walk_remote(fs, root, walk, counter, cancel).await,
            Self::Server { files, root } => {
                tree::walk_files(files.as_ref(), root, walk, counter, cancel).await
            }
        }
    }

    fn close(&self) {
        if let Self::Sftp { fs, .. } = self {
            fs.close();
        }
    }
}

/// Stops the listing once it has taken in more than a comparison keeps in memory.
async fn stop_past_limit(progress: Progress, stop: CancellationToken) {
    loop {
        tokio::time::sleep(LIMIT_CHECK_INTERVAL).await;
        if listed(&progress) > MAX_ENTRIES {
            stop.cancel();
            return;
        }
    }
}

fn listed(progress: &Progress) -> u64 {
    progress.local_entries.load(Ordering::Relaxed) + progress.remote_entries.load(Ordering::Relaxed)
}

impl SyncManager {
    /// Lists both folders and everything below them and compares what they hold. Progress goes
    /// out as a synchronization's does, with the left side counted as the local one, and
    /// [`Self::cancel`] stops it.
    pub async fn compare_folders(
        &self,
        request: FolderCompareRequest,
    ) -> AppResult<FolderComparison> {
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
        let result = self.compare_listed(&request, &progress, &cancel).await;
        reporter.abort();
        self.comparisons.lock().unwrap().remove(&request.request_id);
        result
    }

    async fn compare_listed(
        &self,
        request: &FolderCompareRequest,
        progress: &Progress,
        cancel: &CancellationToken,
    ) -> AppResult<FolderComparison> {
        let left_side = Side::open(&self.sessions, &request.left).await?;
        let right_side = match Side::open(&self.sessions, &request.right).await {
            Ok(side) => side,
            Err(error) => {
                left_side.close();
                return Err(error);
            }
        };
        let walk = Walk {
            skip_hidden: !request.include_hidden,
            ..Walk::new(Filter::default(), Role::Both)
        };
        let within_limit = cancel.child_token();
        let limit = tokio::spawn(stop_past_limit(progress.clone(), within_limit.clone()));
        let (left, right) = tokio::join!(
            left_side.walk(&walk, &progress.local_entries, &within_limit),
            right_side.walk(&walk, &progress.remote_entries, &within_limit),
        );
        limit.abort();
        left_side.close();
        right_side.close();
        let too_many = within_limit.is_cancelled() && !cancel.is_cancelled();
        let listed_both = match (left, right) {
            (Ok(left), Ok(right)) => Ok((left, right)),
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
        let (mut left, mut right) = listed_both.map_err(|error| {
            if too_many && error.kind == ErrorKind::Cancelled {
                AppError::invalid(format!(
                    "These folders hold more than {MAX_ENTRIES} files and folders, too many to compare at once; compare folders further down"
                ))
            } else {
                error
            }
        })?;

        if request.ignore_case {
            fold_case(&mut left);
            fold_case(&mut right);
        }
        let rules = TimeRules {
            tolerance: i64::from(request.time_tolerance_secs.min(MAX_TIME_TOLERANCE_SECS)),
            ignore_hour_offsets: request.ignore_hour_offsets,
            // Read after listing, which is when an FTP server may turn out to lack `MLSD`.
            left_coarse: left_side.coarse_times(),
            right_coarse: right_side.coarse_times(),
        };
        let plan = two_way_plan(&left, &mut right, request.size_only, &rules);
        let (folders, truncated) = summarize(&left, &right, &plan);
        Ok(FolderComparison {
            folders,
            truncated,
            left_entries: progress.local_entries.load(Ordering::Relaxed),
            right_entries: progress.remote_entries.load(Ordering::Relaxed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::tree::Node;

    fn tree(entries: &[(&str, Option<(u64, i64)>)]) -> Tree {
        let mut tree = Tree::default();
        for (path, file) in entries {
            tree.nodes.insert(
                path.to_string(),
                Node {
                    is_dir: file.is_none(),
                    size: file.map_or(0, |(size, _)| size),
                    modified: file.map(|(_, modified)| modified),
                    ..Node::default()
                },
            );
        }
        tree
    }

    const PLAIN: TimeRules = TimeRules {
        tolerance: 2,
        ignore_hour_offsets: false,
        left_coarse: false,
        right_coarse: false,
    };

    fn summaries(left: &Tree, right: &mut Tree) -> Vec<FolderSummary> {
        let plan = two_way_plan(left, right, false, &PLAIN);
        summarize(left, right, &plan).0
    }

    #[test]
    fn folders_count_what_differs_at_any_depth() {
        let left = tree(&[
            ("same", None),
            ("same/a.txt", Some((3, 100))),
            ("same/inner", None),
            ("same/inner/b.txt", Some((4, 100))),
            ("mixed", None),
            ("mixed/newer.txt", Some((5, 900))),
            ("mixed/older.txt", Some((5, 100))),
            ("mixed/tie.txt", Some((5, 100))),
            ("mixed/kept.txt", Some((7, 100))),
            ("mixed/only-left", None),
            ("mixed/only-left/one", Some((1, 1))),
            ("mixed/only-left/two", Some((1, 1))),
            ("mixed/empty-left", None),
            ("mixed/deep", None),
            ("mixed/deep/thing", Some((1, 1))),
            ("left-only", None),
            ("top.txt", Some((1, 1))),
        ]);
        let mut right = tree(&[
            ("same", None),
            ("same/a.txt", Some((3, 101))),
            ("same/inner", None),
            ("same/inner/b.txt", Some((4, 100))),
            ("mixed", None),
            ("mixed/newer.txt", Some((5, 100))),
            ("mixed/older.txt", Some((5, 900))),
            ("mixed/tie.txt", Some((6, 101))),
            ("mixed/kept.txt", Some((7, 100))),
            ("mixed/only-right.txt", Some((1, 1))),
            ("mixed/deep", None),
            ("mixed/deep/thing", None),
            ("mixed/deep/thing/inside", Some((1, 1))),
            ("top.txt", Some((2, 1))),
        ]);
        let found = summaries(&left, &mut right);
        let paths: Vec<&str> = found.iter().map(|summary| summary.path.as_str()).collect();
        // Shallowest first, and only folders both sides have.
        assert_eq!(paths, ["mixed", "same", "mixed/deep", "same/inner"]);
        assert_eq!(
            found[0],
            FolderSummary {
                path: "mixed".into(),
                identical: 1,
                newer_left: 1,
                newer_right: 1,
                // The two files in the folder only on the left, and the empty folder itself.
                only_left: 3,
                only_right: 1,
                // The tie, and the file that is a folder on the other side.
                different: 2,
                not_compared: 0,
            }
        );
        assert_eq!(
            found[1],
            FolderSummary {
                path: "same".into(),
                identical: 2,
                ..FolderSummary::default()
            }
        );
        assert_eq!(found[2].different, 1);
        assert_eq!(found[3].identical, 1);
    }

    #[test]
    fn what_could_not_be_read_is_not_compared() {
        let left = tree(&[("photos", None), ("photos/a.jpg", Some((5, 1)))]);
        let mut right = tree(&[("photos", None)]);
        right.passed_over.push("photos/albums".into());
        let found = summaries(&left, &mut right);
        assert_eq!(found[0].only_left, 1);
        assert_eq!(found[0].not_compared, 1);
    }

    #[test]
    fn case_is_folded_on_both_sides() {
        let mut left = tree(&[("Docs", None), ("Docs/Read Me.TXT", Some((3, 100)))]);
        let mut right = tree(&[("docs", None), ("docs/read me.txt", Some((3, 100)))]);
        fold_case(&mut left);
        fold_case(&mut right);
        let found = summaries(&left, &mut right);
        assert_eq!(
            found,
            [FolderSummary {
                path: "docs".into(),
                identical: 1,
                ..FolderSummary::default()
            }]
        );
    }

    #[test]
    fn names_one_side_has_in_two_cases_are_not_compared() {
        // A case-sensitive server against a Windows folder: which of README and readme the
        // Windows file stands for is unknown, so neither is compared, nor the second Assets.
        let mut left = tree(&[
            ("docs", None),
            ("docs/README", Some((9, 500))),
            ("docs/readme", Some((3, 100))),
            ("docs/notes.txt", Some((4, 100))),
            ("docs/Assets", None),
            ("docs/Assets/logo.svg", Some((5, 100))),
            ("docs/assets", None),
            ("docs/assets/icon.svg", Some((6, 100))),
        ]);
        let mut right = tree(&[
            ("docs", None),
            ("docs/readme", Some((3, 100))),
            ("docs/notes.txt", Some((4, 100))),
            ("docs/assets", None),
            ("docs/assets/logo.svg", Some((5, 100))),
        ]);
        fold_case(&mut left);
        fold_case(&mut right);
        assert_eq!(left.passed_over, ["docs/assets", "docs/readme"]);
        assert!(!left.nodes.contains_key("docs/assets/icon.svg"));
        let found = summaries(&left, &mut right);
        // Nothing is said about the folder that was passed over.
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0],
            FolderSummary {
                path: "docs".into(),
                identical: 1,
                not_compared: 2,
                ..FolderSummary::default()
            }
        );
    }

    #[test]
    fn the_deepest_folders_are_left_out_past_the_limit() {
        let names: Vec<String> = (0..=MAX_FOLDERS)
            .map(|index| format!("top/folder-{index:05}"))
            .collect();
        let mut entries: Vec<(&str, Option<(u64, i64)>)> = vec![("top", None)];
        entries.extend(names.iter().map(|name| (name.as_str(), None)));
        let left = tree(&entries);
        let mut right = tree(&entries);
        let plan = two_way_plan(&left, &mut right, false, &PLAIN);
        let (found, truncated) = summarize(&left, &right, &plan);
        assert!(truncated);
        assert_eq!(found.len(), MAX_FOLDERS);
        assert_eq!(found[0].path, "top");
    }

    #[tokio::test]
    async fn listing_stops_past_the_most_a_comparison_holds() {
        let progress = Progress::default();
        progress.local_entries.store(MAX_ENTRIES, Ordering::Relaxed);
        let stop = CancellationToken::new();
        let watching = tokio::spawn(stop_past_limit(progress.clone(), stop.clone()));
        tokio::time::sleep(LIMIT_CHECK_INTERVAL * 3).await;
        assert!(!stop.is_cancelled());
        progress.remote_entries.store(1, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(5), stop.cancelled())
            .await
            .unwrap();
        watching.await.unwrap();
    }

    /// One row of the table the page's port of the rule is tested against too
    /// (`src/lib/folderCompare.test.ts`), so the two cannot drift apart unnoticed.
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        #[serde(default)]
        size_only: bool,
        #[serde(default = "default_tolerance")]
        tolerance: i64,
        #[serde(default)]
        ignore_hour_offsets: bool,
        #[serde(default)]
        left_coarse: bool,
        #[serde(default)]
        right_coarse: bool,
        left: Option<Thing>,
        right: Option<Thing>,
        expect: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Thing {
        #[serde(default)]
        folder: bool,
        #[serde(default)]
        size: u64,
        modified: Option<i64>,
    }

    fn default_tolerance() -> i64 {
        2
    }

    fn single(thing: &Option<Thing>) -> Tree {
        let mut tree = Tree::default();
        if let Some(thing) = thing {
            tree.nodes.insert(
                "item".into(),
                Node {
                    is_dir: thing.folder,
                    size: thing.size,
                    modified: thing.modified,
                    ..Node::default()
                },
            );
        }
        tree
    }

    #[test]
    fn the_shared_cases_come_out_as_the_page_shows_them() {
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../tests/fixtures/folder_compare_cases.json"
        ))
        .unwrap();
        assert!(cases.len() > 30);
        for case in cases {
            let left = single(&case.left);
            let mut right = single(&case.right);
            let rules = TimeRules {
                tolerance: case.tolerance,
                ignore_hour_offsets: case.ignore_hour_offsets,
                left_coarse: case.left_coarse,
                right_coarse: case.right_coarse,
            };
            let plan = two_way_plan(&left, &mut right, case.size_only, &rules);
            let found = match plan.items.as_slice() {
                [] if plan.unchanged_files == ["item"] => "identical",
                [] => "bothFolders",
                [item] => match difference(item) {
                    Difference::OnlyLeft => "onlyLeft",
                    Difference::OnlyRight => "onlyRight",
                    Difference::NewerLeft => "newerLeft",
                    Difference::NewerRight => "newerRight",
                    Difference::Different => "different",
                },
                items => panic!("{}: {} items", case.name, items.len()),
            };
            assert_eq!(found, case.expect, "{}", case.name);
        }
    }
}
