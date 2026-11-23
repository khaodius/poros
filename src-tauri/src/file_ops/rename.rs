//! Renaming many items of one folder in one call, on this computer or on a server of any
//! kind. The page works out the order (see `planRenames` in `src/lib/bulkRename.ts`); this runs
//! it, reports progress, stops at the first rename that fails, and says what was renamed.
//!
//! Every rename goes through the same call a single rename makes, which checks the new name
//! and never takes a name that is in use. Names that trade places cannot all be free in turn,
//! so one of them waits under a temporary name while the others move. If such a group cannot
//! finish, what it did is undone, so nothing is left under a temporary name.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{Location, Progress};
use crate::error::{AppError, AppResult};
use crate::local;
use crate::protocol::{self, RemoteFileSystem};
use crate::remote_path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRequest {
    /// Names this operation for progress events and cancelling.
    pub operation_id: String,
    pub location: Location,
    /// The folder every item is in.
    pub directory: String,
    /// Run in this order.
    pub groups: Vec<RenameGroup>,
}

/// Renames that go through together or not at all, in an order in which each new name is free
/// by the time it is needed. With `temporary`, the first item waits under that name while the
/// others move and takes its new name last, which is how names trade places.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameGroup {
    #[serde(default)]
    pub temporary: Option<String>,
    pub renames: Vec<Rename>,
}

/// One item's name in the folder, and the name it is to have.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rename {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSummary {
    /// The items that have their new name, in the order they got it.
    pub renamed: Vec<Renamed>,
    /// The rename that stopped the rest, and why. The items after it keep their names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<RenameFailure>,
    /// An item left under its temporary name, because it could not be moved back either.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stranded: Option<Stranded>,
    /// Stopped by the user before every item was renamed.
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Renamed {
    pub from: String,
    pub to: String,
    /// Where the item is now.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFailure {
    pub from: String,
    pub to: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stranded {
    /// The name the item had.
    pub from: String,
    /// The name it has now.
    pub temporary: String,
    /// The name it was to get.
    pub to: String,
    /// Why it could not be moved back.
    pub message: String,
}

/// A folder whose items can be renamed, wherever it is.
#[async_trait]
pub(crate) trait Folder: Send + Sync {
    /// Gives the item named `from` the name `to`, which must be free, and answers with the
    /// item's new path.
    async fn rename(&self, from: &str, to: &str) -> AppResult<String>;
}

/// A folder on this computer.
pub(crate) struct LocalFolder {
    directory: String,
}

impl LocalFolder {
    pub(crate) fn new(directory: &str) -> Self {
        Self {
            directory: directory.to_string(),
        }
    }
}

#[async_trait]
impl Folder for LocalFolder {
    async fn rename(&self, from: &str, to: &str) -> AppResult<String> {
        let source = Path::new(&self.directory)
            .join(from)
            .to_string_lossy()
            .into_owned();
        let new_name = to.to_string();
        tokio::task::spawn_blocking(move || local::rename(&source, &new_name)).await?
    }
}

/// A folder on a server, or in an unlocked vault.
pub(crate) struct RemoteFolder {
    files: Arc<dyn RemoteFileSystem>,
    directory: String,
}

impl RemoteFolder {
    pub(crate) fn new(files: Arc<dyn RemoteFileSystem>, directory: &str) -> Self {
        let directory = files.resolve(directory);
        Self { files, directory }
    }
}

#[async_trait]
impl Folder for RemoteFolder {
    async fn rename(&self, from: &str, to: &str) -> AppResult<String> {
        self.files
            .rename(&remote_path::join(&self.directory, from), to)
            .await
    }
}

impl Location {
    /// Checks a name the way a single rename there does.
    fn check_name(&self, name: &str) -> AppResult<()> {
        match self {
            Location::Local => local::validate_name(name),
            Location::Remote { .. } => protocol::validate_name(name),
        }
    }
}

/// The names among `names` that are taken in `directory`, on a server of any kind.
pub(crate) async fn taken_names(
    files: &dyn RemoteFileSystem,
    directory: &str,
    names: &[String],
) -> AppResult<Vec<String>> {
    let directory = files.resolve(directory);
    let mut taken = Vec::new();
    for name in names {
        protocol::validate_name(name)?;
        let path = remote_path::join(&directory, name);
        if files.stat(&path).await?.is_some() {
            taken.push(name.clone());
        }
    }
    Ok(taken)
}

/// One step of a group: a single rename in the folder.
struct Step<'a> {
    from: &'a str,
    to: &'a str,
    /// The rename asked for that this step is part of.
    item: &'a Rename,
    /// Whether the item has its new name once the step is done.
    completes: bool,
}

impl<'a> Step<'a> {
    /// A rename done in one step.
    fn plain(rename: &'a Rename) -> Self {
        Self {
            from: &rename.from,
            to: &rename.to,
            item: rename,
            completes: true,
        }
    }
}

impl RenameGroup {
    /// The single renames the group amounts to, in the order they run.
    fn steps(&self) -> Vec<Step<'_>> {
        match (&self.temporary, self.renames.split_first()) {
            (Some(temporary), Some((first, rest))) => {
                let mut steps = vec![Step {
                    from: &first.from,
                    to: temporary,
                    item: first,
                    completes: false,
                }];
                steps.extend(rest.iter().map(Step::plain));
                steps.push(Step {
                    from: temporary,
                    to: &first.to,
                    item: first,
                    completes: true,
                });
                steps
            }
            _ => self.renames.iter().map(Step::plain).collect(),
        }
    }
}

/// Refuses a request before anything is renamed: a name that is not a plain name there, a
/// rename to the name the item has, or two renames of one item or to one name. The page has
/// checked all this for the user to see; nothing it sends is trusted for that.
pub(crate) fn check(request: &RenameRequest) -> AppResult<()> {
    if request.directory.trim().is_empty() {
        return Err(AppError::invalid("No folder was given to rename in"));
    }
    let mut sources = HashSet::new();
    let mut targets = HashSet::new();
    for group in &request.groups {
        if group.renames.is_empty() {
            return Err(AppError::invalid("A group of renames is empty"));
        }
        if let Some(temporary) = &group.temporary {
            request.location.check_name(temporary)?;
            if !targets.insert(temporary.as_str()) {
                return Err(two_would_be_named(temporary));
            }
        }
        for rename in &group.renames {
            request.location.check_name(&rename.from)?;
            request.location.check_name(&rename.to)?;
            if rename.from == rename.to {
                return Err(AppError::invalid(format!(
                    "{} already has that name",
                    rename.from
                )));
            }
            if !sources.insert(rename.from.as_str()) {
                return Err(AppError::invalid(format!(
                    "{} is renamed twice",
                    rename.from
                )));
            }
            if !targets.insert(rename.to.as_str()) {
                return Err(two_would_be_named(&rename.to));
            }
        }
    }
    Ok(())
}

fn two_would_be_named(name: &str) -> AppError {
    AppError::invalid(format!("Two items would be named {name}"))
}

/// Why a group did not finish.
enum Stop {
    Cancelled,
    Failed(RenameFailure),
}

/// Runs the groups in order, stopping at the first that cannot finish or when cancelled.
pub(crate) async fn run(
    folder: &dyn Folder,
    groups: &[RenameGroup],
    cancel: &CancellationToken,
    progress: &Progress,
) -> RenameSummary {
    progress.found(groups.iter().map(|group| group.renames.len() as u64).sum());
    let mut summary = RenameSummary::default();
    for group in groups {
        if cancel.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        if let Some(stop) = run_group(folder, group, cancel, progress, &mut summary).await {
            match stop {
                Stop::Cancelled => summary.cancelled = true,
                Stop::Failed(failure) => summary.failure = Some(failure),
            }
            break;
        }
    }
    summary
}

/// Runs one group. If it cannot finish, its renames are undone, latest first, each of which
/// frees the name the one before it needs back.
async fn run_group(
    folder: &dyn Folder,
    group: &RenameGroup,
    cancel: &CancellationToken,
    progress: &Progress,
    summary: &mut RenameSummary,
) -> Option<Stop> {
    let steps = group.steps();
    // The path each finished step gave its item.
    let mut done: Vec<String> = Vec::with_capacity(steps.len());
    let mut stop = None;
    for step in &steps {
        // The first step of a group starts only if the caller found nothing cancelled.
        if !done.is_empty() && cancel.is_cancelled() {
            stop = Some(Stop::Cancelled);
            break;
        }
        progress.working_on(step.from);
        match folder.rename(step.from, step.to).await {
            Ok(path) => {
                done.push(path);
                if step.completes {
                    progress.file_done();
                }
            }
            Err(error) => {
                // The item's own names, also when a step to or from the temporary name failed.
                stop = Some(Stop::Failed(RenameFailure {
                    from: step.item.from.clone(),
                    to: step.item.to.clone(),
                    message: error.message,
                }));
                break;
            }
        }
    }

    if stop.is_some() {
        while let Some(step) = done.len().checked_sub(1).map(|last| &steps[last]) {
            match folder.rename(step.to, step.from).await {
                Ok(_) => {
                    done.pop();
                }
                Err(error) => {
                    // Only a group with a temporary name is left with one in use: its first
                    // step is the one that could not be undone, or comes before it.
                    if let (Some(temporary), Some(first)) =
                        (&group.temporary, group.renames.first())
                    {
                        summary.stranded = Some(Stranded {
                            from: first.from.clone(),
                            temporary: temporary.clone(),
                            to: first.to.clone(),
                            message: error.message,
                        });
                    }
                    break;
                }
            }
        }
    }

    // What is still done has its new name: the whole group, or the part that could not be
    // undone.
    for (step, path) in steps.iter().zip(done) {
        if step.completes {
            summary.renamed.push(Renamed {
                from: step.item.from.clone(),
                to: step.item.to.clone(),
                path,
            });
        }
    }
    stop
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::*;
    use crate::error::ErrorKind;

    /// A folder in memory that renames as the real ones do: never onto a name in use.
    #[derive(Default)]
    struct MemoryFolder {
        /// Each name, and what the item is, which stays with it as it is renamed.
        items: Mutex<BTreeMap<String, String>>,
        /// Renames that fail, as `from` and `to`.
        refused: Mutex<Vec<(String, String)>>,
        /// Every rename asked for, in order.
        calls: Mutex<Vec<(String, String)>>,
        /// Cancels once this many renames have been asked for.
        cancel_after: Option<(usize, CancellationToken)>,
    }

    impl MemoryFolder {
        fn holding(names: &[&str]) -> Self {
            let folder = Self::default();
            for name in names {
                folder
                    .items
                    .lock()
                    .unwrap()
                    .insert(name.to_string(), format!("was {name}"));
            }
            folder
        }

        fn refusing(self, from: &str, to: &str) -> Self {
            self.refused
                .lock()
                .unwrap()
                .push((from.to_string(), to.to_string()));
            self
        }

        fn cancelling_after(mut self, calls: usize, cancel: &CancellationToken) -> Self {
            self.cancel_after = Some((calls, cancel.clone()));
            self
        }

        /// Each name with the name its item started with.
        fn names(&self) -> Vec<(String, String)> {
            self.items
                .lock()
                .unwrap()
                .iter()
                .map(|(name, item)| (name.clone(), item.replace("was ", "")))
                .collect()
        }

        fn calls(&self) -> Vec<(String, String)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Folder for MemoryFolder {
        async fn rename(&self, from: &str, to: &str) -> AppResult<String> {
            let call = (from.to_string(), to.to_string());
            let asked = {
                let mut calls = self.calls.lock().unwrap();
                calls.push(call.clone());
                calls.len()
            };
            if let Some((after, cancel)) = &self.cancel_after {
                if asked >= *after {
                    cancel.cancel();
                }
            }
            if self.refused.lock().unwrap().contains(&call) {
                return Err(AppError::new(
                    ErrorKind::PermissionDenied,
                    format!("{from} is locked"),
                ));
            }
            let mut items = self.items.lock().unwrap();
            if items.contains_key(to) {
                return Err(protocol::already_exists(to));
            }
            let item = items
                .remove(from)
                .ok_or_else(|| AppError::new(ErrorKind::NotFound, format!("{from} is gone")))?;
            items.insert(to.to_string(), item);
            Ok(format!("/folder/{to}"))
        }
    }

    fn rename(from: &str, to: &str) -> Rename {
        Rename {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    fn single(from: &str, to: &str) -> RenameGroup {
        RenameGroup {
            temporary: None,
            renames: vec![rename(from, to)],
        }
    }

    fn ring(temporary: &str, renames: &[(&str, &str)]) -> RenameGroup {
        RenameGroup {
            temporary: Some(temporary.to_string()),
            renames: renames.iter().map(|(from, to)| rename(from, to)).collect(),
        }
    }

    fn pairs(names: &[(&str, &str)]) -> Vec<(String, String)> {
        names
            .iter()
            .map(|(first, second)| (first.to_string(), second.to_string()))
            .collect()
    }

    fn renamed_names(summary: &RenameSummary) -> Vec<(String, String)> {
        summary
            .renamed
            .iter()
            .map(|renamed| (renamed.from.clone(), renamed.to.clone()))
            .collect()
    }

    async fn run_on(folder: &MemoryFolder, groups: &[RenameGroup]) -> RenameSummary {
        run(
            folder,
            groups,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .await
    }

    fn request(groups: Vec<RenameGroup>) -> RenameRequest {
        RenameRequest {
            operation_id: "test".to_string(),
            location: Location::Local,
            directory: "/folder".to_string(),
            groups,
        }
    }

    #[tokio::test]
    async fn renames_in_the_order_given_and_reports_each_item() {
        let folder = MemoryFolder::holding(&["a", "b", "c"]);
        // A chain: each name is free once the rename before it has run.
        let groups = [single("c", "d"), single("b", "c"), single("a", "b")];
        let progress = Progress::default();
        let summary = run(&folder, &groups, &CancellationToken::new(), &progress).await;

        assert_eq!(
            renamed_names(&summary),
            pairs(&[("c", "d"), ("b", "c"), ("a", "b")])
        );
        assert_eq!(summary.renamed[0].path, "/folder/d");
        assert_eq!(summary.failure, None);
        assert_eq!(summary.stranded, None);
        assert!(!summary.cancelled);
        assert_eq!(folder.names(), pairs(&[("b", "a"), ("c", "b"), ("d", "c")]));
        let snapshot = progress.snapshot("test");
        assert_eq!((snapshot.files, snapshot.total), (3, Some(3)));
    }

    #[tokio::test]
    async fn trades_names_through_the_temporary_name() {
        let folder = MemoryFolder::holding(&["a", "b", "c", "x", "y"]);
        let groups = [
            ring(".tmp-1", &[("a", "b"), ("c", "a"), ("b", "c")]),
            ring(".tmp-2", &[("x", "y"), ("y", "x")]),
        ];
        let progress = Progress::default();
        let summary = run(&folder, &groups, &CancellationToken::new(), &progress).await;

        assert_eq!(
            folder.calls(),
            pairs(&[
                ("a", ".tmp-1"),
                ("c", "a"),
                ("b", "c"),
                (".tmp-1", "b"),
                ("x", ".tmp-2"),
                ("y", "x"),
                (".tmp-2", "y"),
            ])
        );
        assert_eq!(
            folder.names(),
            pairs(&[("a", "c"), ("b", "a"), ("c", "b"), ("x", "y"), ("y", "x")])
        );
        // The items, not the steps: the wait under a temporary name is no rename of its own.
        assert_eq!(
            renamed_names(&summary),
            pairs(&[("c", "a"), ("b", "c"), ("a", "b"), ("y", "x"), ("x", "y")])
        );
        assert_eq!(
            summary.renamed[2],
            Renamed {
                from: "a".to_string(),
                to: "b".to_string(),
                path: "/folder/b".to_string()
            }
        );
        let snapshot = progress.snapshot("test");
        assert_eq!((snapshot.files, snapshot.total), (5, Some(5)));
    }

    #[tokio::test]
    async fn stops_at_the_first_failure_and_leaves_the_rest_alone() {
        let folder = MemoryFolder::holding(&["a", "b", "c"]).refusing("b", "y");
        let groups = [single("a", "x"), single("b", "y"), single("c", "z")];
        let summary = run_on(&folder, &groups).await;

        assert_eq!(renamed_names(&summary), pairs(&[("a", "x")]));
        assert_eq!(
            summary.failure,
            Some(RenameFailure {
                from: "b".to_string(),
                to: "y".to_string(),
                message: "b is locked".to_string(),
            })
        );
        assert_eq!(summary.stranded, None);
        assert_eq!(folder.calls().len(), 2);
        assert_eq!(folder.names(), pairs(&[("b", "b"), ("c", "c"), ("x", "a")]));
    }

    #[tokio::test]
    async fn never_takes_a_name_that_is_in_use() {
        let folder = MemoryFolder::holding(&["a", "b"]);
        let summary = run_on(&folder, &[single("a", "b")]).await;

        assert!(summary.renamed.is_empty());
        let failure = summary.failure.unwrap();
        assert_eq!(failure.message, "b already exists");
        assert_eq!(folder.names(), pairs(&[("a", "a"), ("b", "b")]));
    }

    #[tokio::test]
    async fn undoes_a_ring_that_cannot_finish() {
        // Every step of the ring but the first, failing in turn, with the item it is part of.
        for (failing, item, undone) in [
            (("c", "a"), ("c", "a"), vec![(".tmp", "a")]),
            (("b", "c"), ("b", "c"), vec![("a", "c"), (".tmp", "a")]),
            (
                (".tmp", "b"),
                ("a", "b"),
                vec![("c", "b"), ("a", "c"), (".tmp", "a")],
            ),
        ] {
            let folder =
                MemoryFolder::holding(&["a", "b", "c", "other"]).refusing(failing.0, failing.1);
            let groups = [
                single("other", "first"),
                ring(".tmp", &[("a", "b"), ("c", "a"), ("b", "c")]),
                single("never", "reached"),
            ];
            let summary = run_on(&folder, &groups).await;

            assert_eq!(
                folder.names(),
                pairs(&[("a", "a"), ("b", "b"), ("c", "c"), ("first", "other")]),
                "{failing:?}"
            );
            assert_eq!(renamed_names(&summary), pairs(&[("other", "first")]));
            let failure = summary.failure.unwrap();
            assert_eq!((failure.from.as_str(), failure.to.as_str()), item);
            assert_eq!(summary.stranded, None);
            let calls = folder.calls();
            assert_eq!(calls[calls.len() - undone.len()..], pairs(&undone));
        }
    }

    #[tokio::test]
    async fn a_ring_whose_first_step_fails_has_nothing_to_undo() {
        let folder = MemoryFolder::holding(&["a", "b"]).refusing("a", ".tmp");
        let summary = run_on(&folder, &[ring(".tmp", &[("a", "b"), ("b", "a")])]).await;

        assert!(summary.renamed.is_empty());
        let failure = summary.failure.unwrap();
        assert_eq!((failure.from.as_str(), failure.to.as_str()), ("a", "b"));
        assert_eq!(folder.calls(), pairs(&[("a", ".tmp")]));
        assert_eq!(folder.names(), pairs(&[("a", "a"), ("b", "b")]));
    }

    #[tokio::test]
    async fn says_what_is_left_under_a_temporary_name_when_undoing_fails_too() {
        // The swap cannot finish, and the item waiting cannot go back either.
        let folder = MemoryFolder::holding(&["a", "b"])
            .refusing(".tmp", "b")
            .refusing(".tmp", "a");
        let summary = run_on(&folder, &[ring(".tmp", &[("a", "b"), ("b", "a")])]).await;

        assert_eq!(
            folder.names(),
            pairs(&[(".tmp", "a"), ("b", "b")]),
            "b went back, a could not"
        );
        assert!(summary.renamed.is_empty());
        assert_eq!(summary.failure.unwrap().message, ".tmp is locked");
        assert_eq!(
            summary.stranded,
            Some(Stranded {
                from: "a".to_string(),
                temporary: ".tmp".to_string(),
                to: "b".to_string(),
                message: ".tmp is locked".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn counts_what_could_not_be_undone_as_renamed() {
        // b took the name a had, and cannot give it back.
        let folder = MemoryFolder::holding(&["a", "b"])
            .refusing(".tmp", "b")
            .refusing("a", "b");
        let summary = run_on(&folder, &[ring(".tmp", &[("a", "b"), ("b", "a")])]).await;

        assert_eq!(folder.names(), pairs(&[(".tmp", "a"), ("a", "b")]));
        assert_eq!(renamed_names(&summary), pairs(&[("b", "a")]));
        assert_eq!(summary.stranded.unwrap().temporary, ".tmp");
    }

    #[tokio::test]
    async fn cancelling_stops_between_groups() {
        let cancel = CancellationToken::new();
        let folder = MemoryFolder::holding(&["a", "b", "c"]).cancelling_after(2, &cancel);
        let groups = [single("a", "x"), single("b", "y"), single("c", "z")];
        let summary = run(&folder, &groups, &cancel, &Progress::default()).await;

        assert!(summary.cancelled);
        assert_eq!(summary.failure, None);
        assert_eq!(renamed_names(&summary), pairs(&[("a", "x"), ("b", "y")]));
        assert_eq!(folder.names(), pairs(&[("c", "c"), ("x", "a"), ("y", "b")]));
    }

    #[tokio::test]
    async fn cancelling_inside_a_ring_puts_its_names_back() {
        let cancel = CancellationToken::new();
        let folder = MemoryFolder::holding(&["a", "b", "c"]).cancelling_after(2, &cancel);
        let groups = [ring(".tmp", &[("a", "b"), ("c", "a"), ("b", "c")])];
        let summary = run(&folder, &groups, &cancel, &Progress::default()).await;

        assert!(summary.cancelled);
        assert!(summary.renamed.is_empty());
        assert_eq!(summary.stranded, None);
        assert_eq!(folder.names(), pairs(&[("a", "a"), ("b", "b"), ("c", "c")]));
        assert_eq!(
            folder.calls(),
            pairs(&[("a", ".tmp"), ("c", "a"), ("a", "c"), (".tmp", "a")])
        );
    }

    #[tokio::test]
    async fn cancelled_before_it_starts_renames_nothing() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let folder = MemoryFolder::holding(&["a"]);
        let summary = run(&folder, &[single("a", "x")], &cancel, &Progress::default()).await;

        assert!(summary.cancelled);
        assert!(folder.calls().is_empty());
    }

    #[test]
    fn refuses_names_that_are_paths_before_anything_runs() {
        for (from, to) in [
            ("a", "../escaped"),
            ("../outside", "b"),
            ("a", "sub/b"),
            ("a", ""),
            ("", "b"),
            ("a", ".."),
            ("a", "b\0"),
            ("/etc/passwd", "b"),
        ] {
            let error = check(&request(vec![single(from, to)])).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidInput, "{from:?} to {to:?}");
        }
        let traversal = request(vec![ring("../tmp", &[("a", "b"), ("b", "a")])]);
        assert_eq!(check(&traversal).unwrap_err().kind, ErrorKind::InvalidInput);
        let server = RenameRequest {
            location: Location::Remote {
                session_id: "session".to_string(),
            },
            ..request(vec![single("a", "x/../../y")])
        };
        assert_eq!(check(&server).unwrap_err().kind, ErrorKind::InvalidInput);
    }

    #[test]
    fn refuses_requests_that_make_no_sense() {
        let message = |groups: Vec<RenameGroup>| check(&request(groups)).unwrap_err().message;
        assert_eq!(
            message(vec![single("a", "x"), single("a", "y")]),
            "a is renamed twice"
        );
        assert_eq!(
            message(vec![single("a", "x"), single("b", "x")]),
            "Two items would be named x"
        );
        assert_eq!(
            message(vec![ring("x", &[("a", "b"), ("b", "a")]), single("c", "x")]),
            "Two items would be named x"
        );
        assert_eq!(message(vec![single("a", "a")]), "a already has that name");
        assert_eq!(
            message(vec![RenameGroup {
                temporary: None,
                renames: Vec::new()
            }]),
            "A group of renames is empty"
        );
        let nowhere = RenameRequest {
            directory: "  ".to_string(),
            ..request(vec![single("a", "b")])
        };
        assert!(check(&nowhere).is_err());
        assert!(check(&request(vec![
            single("c", "d"),
            single("b", "c"),
            ring(".tmp", &[("x", "y"), ("y", "x")]),
        ]))
        .is_ok());
    }

    #[test]
    fn reads_the_request_the_page_sends() {
        let request: RenameRequest = serde_json::from_value(serde_json::json!({
            "operationId": "operation",
            "location": { "kind": "remote", "sessionId": "session" },
            "directory": "/srv/photos",
            "groups": [
                { "renames": [{ "from": "a", "to": "b" }] },
                { "temporary": ".tmp", "renames": [
                    { "from": "x", "to": "y" },
                    { "from": "y", "to": "x" },
                ] },
            ],
        }))
        .unwrap();
        assert_eq!(request.groups[0].temporary, None);
        assert_eq!(request.groups[1].temporary.as_deref(), Some(".tmp"));
        assert_eq!(request.groups[1].renames[1], rename("y", "x"));
        check(&request).unwrap();

        let summary = RenameSummary {
            renamed: vec![Renamed {
                from: "a".to_string(),
                to: "b".to_string(),
                path: "/srv/photos/b".to_string(),
            }],
            ..RenameSummary::default()
        };
        assert_eq!(
            serde_json::to_value(&summary).unwrap(),
            serde_json::json!({
                "renamed": [{ "from": "a", "to": "b", "path": "/srv/photos/b" }],
                "cancelled": false,
            })
        );
    }

    fn local_names(directory: &Path) -> Vec<(String, String)> {
        let mut names: Vec<(String, String)> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let contents = if path.is_dir() {
                    "folder".to_string()
                } else {
                    std::fs::read_to_string(&path).unwrap()
                };
                (name, contents)
            })
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn renames_files_and_folders_on_this_computer() {
        let scratch = tempfile::tempdir().unwrap();
        for name in ["one.txt", "two.txt", "report.txt"] {
            std::fs::write(scratch.path().join(name), name).unwrap();
        }
        std::fs::create_dir(scratch.path().join("old folder")).unwrap();
        let folder = LocalFolder::new(scratch.path().to_str().unwrap());
        let groups = [
            ring(
                ".poros-rename-1",
                &[("one.txt", "two.txt"), ("two.txt", "one.txt")],
            ),
            // On Windows this changes case alone, which needs the temporary name too.
            ring(".poros-rename-2", &[("report.txt", "REPORT.txt")]),
            single("old folder", "new folder"),
        ];
        let summary = run_on_local(&folder, &groups).await;

        assert_eq!(summary.failure, None);
        assert_eq!(summary.renamed.len(), 4);
        assert_eq!(
            summary.renamed[3].path,
            scratch.path().join("new folder").to_string_lossy()
        );
        assert_eq!(
            local_names(scratch.path()),
            pairs(&[
                ("REPORT.txt", "report.txt"),
                ("new folder", "folder"),
                ("one.txt", "two.txt"),
                ("two.txt", "one.txt"),
            ])
        );
    }

    #[tokio::test]
    async fn a_local_rename_onto_an_existing_name_stops_without_overwriting() {
        let scratch = tempfile::tempdir().unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(scratch.path().join(name), name).unwrap();
        }
        let folder = LocalFolder::new(scratch.path().to_str().unwrap());
        let groups = [single("a.txt", "b.txt"), single("c.txt", "d.txt")];
        let summary = run_on_local(&folder, &groups).await;

        assert!(summary.renamed.is_empty());
        assert_eq!(summary.failure.unwrap().message, "b.txt already exists");
        assert_eq!(
            local_names(scratch.path()),
            pairs(&[("a.txt", "a.txt"), ("b.txt", "b.txt"), ("c.txt", "c.txt")])
        );
    }

    async fn run_on_local(folder: &LocalFolder, groups: &[RenameGroup]) -> RenameSummary {
        run(
            folder,
            groups,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .await
    }
}
