//! Decides what a synchronization does from the listings of both sides. Kept free of I/O so
//! every rule is tested without a server.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::tree::{Node, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncDirection {
    /// Makes the server folder match the local one.
    Upload,
    /// Makes the local folder match the server one.
    Download,
    /// Copies new files both ways; where both have a file, the newer one wins.
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareMode {
    /// Same size and modification time means unchanged, as rsync assumes by default.
    SizeAndTime,
    SizeOnly,
    /// Compares the contents, reading every file that has the same size on both sides.
    Checksum,
    /// Copies every file, changed or not.
    Always,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub direction: SyncDirection,
    pub compare: CompareMode,
    pub delete_extraneous: bool,
    pub skip_newer_on_target: bool,
    pub ignore_existing: bool,
    pub time_tolerance_secs: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncAction {
    Upload,
    Download,
    DeleteLocal,
    DeleteRemote,
    /// Needs a decision: the file changed on both sides, or its type differs.
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncReason {
    /// Only on the side it is copied from.
    New,
    /// The size or time differs.
    Changed,
    /// Same size, different contents.
    ContentDiffers,
    /// Copied without comparing.
    Always,
    /// Newer on the side it is copied from.
    Newer,
    /// Not on the side being mirrored.
    Extraneous,
    /// A file on one side and a folder on the other.
    TypeDiffers,
    /// Different, with the same modification time on both sides.
    BothChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
}

impl From<&Node> for Facts {
    fn from(node: &Node) -> Self {
        Self {
            is_dir: node.is_dir,
            size: node.size,
            modified: node.modified,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncItem {
    pub id: u32,
    /// Relative to the synchronized folders, with `/` between components.
    pub path: String,
    pub is_dir: bool,
    pub action: SyncAction,
    pub reason: SyncReason,
    /// 1 for a file; for a folder, the files inside it.
    pub files: u64,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<Facts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<Facts>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub unchanged: u64,
    /// Left alone because the target's copy is newer, or because existing files are kept.
    pub kept: u64,
    /// Only on the target, and kept because deleting is off.
    pub extra_on_target: u64,
    pub excluded: u64,
    /// Broken or looping links, special files and unreadable folders.
    pub passed_over: u64,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub items: Vec<SyncItem>,
    pub counts: Counts,
}

/// Files on both sides whose contents decide the comparison.
pub fn files_to_hash(local: &Tree, remote: &Tree, options: &Options) -> Vec<String> {
    if options.compare != CompareMode::Checksum || options.ignore_existing {
        return Vec::new();
    }
    local
        .nodes
        .iter()
        .filter(|(path, node)| {
            !node.is_dir
                && remote
                    .nodes
                    .get(*path)
                    .is_some_and(|other| !other.is_dir && other.size == node.size)
        })
        .map(|(path, _)| path.clone())
        .collect()
}

/// `same_contents` holds, for files that were hashed, whether both sides matched.
pub fn plan(
    local: &Tree,
    remote: &Tree,
    options: &Options,
    same_contents: &HashMap<String, bool>,
) -> Plan {
    let mut planner = Planner {
        local,
        remote,
        options,
        same_contents,
        plan: Plan {
            counts: Counts {
                excluded: local.excluded + remote.excluded,
                passed_over: (local.passed_over.len() + remote.passed_over.len()) as u64,
                ..Counts::default()
            },
            ..Plan::default()
        },
        whole_folders: HashSet::new(),
    };
    let paths: BTreeSet<&String> = local.nodes.keys().chain(remote.nodes.keys()).collect();
    for path in paths {
        if !planner.within_whole_folder(path) {
            planner.decide(path);
        }
    }
    planner.plan
}

struct Planner<'a> {
    local: &'a Tree,
    remote: &'a Tree,
    options: &'a Options,
    same_contents: &'a HashMap<String, bool>,
    plan: Plan,
    /// Folders handled as one item, so nothing inside them is looked at again.
    whole_folders: HashSet<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Local,
    Remote,
}

impl Planner<'_> {
    fn decide(&mut self, path: &str) {
        let local = self.local.nodes.get(path);
        let remote = self.remote.nodes.get(path);
        match (local, remote) {
            (Some(local), None) => self.only_on(Side::Local, path, local),
            (None, Some(remote)) => self.only_on(Side::Remote, path, remote),
            (Some(local), Some(remote)) if local.is_dir && remote.is_dir => {}
            (Some(local), Some(remote)) if local.is_dir != remote.is_dir => {
                self.whole_folders.insert(path.to_string());
                self.push(
                    path,
                    local.is_dir,
                    SyncAction::Conflict,
                    SyncReason::TypeDiffers,
                );
            }
            (Some(local), Some(remote)) => self.both_files(path, local, remote),
            (None, None) => {}
        }
    }

    fn only_on(&mut self, side: Side, path: &str, node: &Node) {
        let (copy, delete, copies_from_here) = match side {
            Side::Local => (
                SyncAction::Upload,
                SyncAction::DeleteLocal,
                self.options.direction != SyncDirection::Download,
            ),
            Side::Remote => (
                SyncAction::Download,
                SyncAction::DeleteRemote,
                self.options.direction != SyncDirection::Upload,
            ),
        };
        if copies_from_here {
            self.push(path, node.is_dir, copy, SyncReason::New);
        } else if !self.options.delete_extraneous {
            self.plan.counts.extra_on_target += 1;
        } else if node.is_dir && self.tree(side).holds_excluded.contains(path) {
            // Excluded files inside stay, so the folder does too; what else is in it goes.
            return;
        } else {
            self.push(path, node.is_dir, delete, SyncReason::Extraneous);
        }
        if node.is_dir {
            self.whole_folders.insert(path.to_string());
        }
    }

    fn both_files(&mut self, path: &str, local: &Node, remote: &Node) {
        let options = self.options;
        if options.ignore_existing {
            self.plan.counts.kept += 1;
            return;
        }
        let tolerance = options.time_tolerance_secs;
        let close_in_time = match (local.modified, remote.modified) {
            (Some(local_time), Some(remote_time)) => (local_time - remote_time).abs() <= tolerance,
            (None, None) => true,
            _ => false,
        };
        let compare = match (options.direction, options.compare) {
            (SyncDirection::Both, CompareMode::Always) => CompareMode::SizeAndTime,
            (_, compare) => compare,
        };
        let unchanged = match compare {
            CompareMode::SizeAndTime => local.size == remote.size && close_in_time,
            CompareMode::SizeOnly => local.size == remote.size,
            CompareMode::Checksum => {
                local.size == remote.size && self.same_contents.get(path) == Some(&true)
            }
            CompareMode::Always => false,
        };
        if unchanged {
            self.plan.counts.unchanged += 1;
            return;
        }
        let reason = match compare {
            CompareMode::Checksum if local.size == remote.size => SyncReason::ContentDiffers,
            CompareMode::Always => SyncReason::Always,
            _ => SyncReason::Changed,
        };
        let local_newer = newer(local, remote, tolerance);
        let remote_newer = newer(remote, local, tolerance);
        let (action, reason) = match options.direction {
            SyncDirection::Upload if options.skip_newer_on_target && remote_newer => {
                self.plan.counts.kept += 1;
                return;
            }
            SyncDirection::Download if options.skip_newer_on_target && local_newer => {
                self.plan.counts.kept += 1;
                return;
            }
            SyncDirection::Upload => (SyncAction::Upload, reason),
            SyncDirection::Download => (SyncAction::Download, reason),
            SyncDirection::Both if local_newer => (SyncAction::Upload, SyncReason::Newer),
            SyncDirection::Both if remote_newer => (SyncAction::Download, SyncReason::Newer),
            SyncDirection::Both => (SyncAction::Conflict, SyncReason::BothChanged),
        };
        self.push(path, false, action, reason);
    }

    fn push(&mut self, path: &str, is_dir: bool, action: SyncAction, reason: SyncReason) {
        let local = self.local.nodes.get(path);
        let remote = self.remote.nodes.get(path);
        let measured = match action {
            SyncAction::Upload | SyncAction::DeleteLocal => local.map(|node| (Side::Local, node)),
            SyncAction::Download | SyncAction::DeleteRemote => {
                remote.map(|node| (Side::Remote, node))
            }
            SyncAction::Conflict => None,
        };
        let (files, bytes) = match measured {
            Some((side, node)) if node.is_dir => self
                .tree(side)
                .inside(path)
                .filter(|(_, inner)| !inner.is_dir)
                .fold((0, 0), |(files, bytes), (_, inner)| {
                    (files + 1, bytes + inner.size)
                }),
            Some((_, node)) => (1, node.size),
            None => (1, 0),
        };
        let id = self.plan.items.len() as u32;
        self.plan.items.push(SyncItem {
            id,
            path: path.to_string(),
            is_dir,
            action,
            reason,
            files,
            bytes,
            local: local.map(Facts::from),
            remote: remote.map(Facts::from),
        });
    }

    fn tree(&self, side: Side) -> &Tree {
        match side {
            Side::Local => self.local,
            Side::Remote => self.remote,
        }
    }

    fn within_whole_folder(&self, path: &str) -> bool {
        let mut current = path;
        while let Some((parent, _)) = current.rsplit_once('/') {
            if self.whole_folders.contains(parent) {
                return true;
            }
            current = parent;
        }
        false
    }
}

fn newer(this: &Node, other: &Node, tolerance: i64) -> bool {
    match (this.modified, other.modified) {
        (Some(this_time), Some(other_time)) => this_time > other_time + tolerance,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(entries: &[(&str, Option<(u64, i64)>)]) -> Tree {
        let mut tree = Tree::default();
        for (path, file) in entries {
            tree.nodes.insert(
                path.to_string(),
                Node {
                    is_dir: file.is_none(),
                    size: file.map_or(0, |(size, _)| size),
                    modified: Some(file.map_or(0, |(_, modified)| modified)),
                },
            );
        }
        tree
    }

    fn options(direction: SyncDirection) -> Options {
        Options {
            direction,
            compare: CompareMode::SizeAndTime,
            delete_extraneous: false,
            skip_newer_on_target: false,
            ignore_existing: false,
            time_tolerance_secs: 2,
        }
    }

    fn summary(plan: &Plan) -> Vec<(String, SyncAction, SyncReason, u64, u64)> {
        plan.items
            .iter()
            .map(|item| {
                (
                    item.path.clone(),
                    item.action,
                    item.reason,
                    item.files,
                    item.bytes,
                )
            })
            .collect()
    }

    fn run(local: &Tree, remote: &Tree, options: &Options) -> Plan {
        plan(local, remote, options, &HashMap::new())
    }

    #[test]
    fn upload_copies_new_and_changed_files_and_whole_new_folders() {
        let local = tree(&[
            ("same.txt", Some((10, 100))),
            ("close.txt", Some((10, 101))),
            ("edited.txt", Some((12, 200))),
            ("new", None),
            ("new/a", Some((3, 1))),
            ("new/deep", None),
            ("new/deep/b", Some((4, 1))),
            ("shared", None),
            ("shared/c", Some((5, 1))),
        ]);
        let remote = tree(&[
            ("same.txt", Some((10, 100))),
            ("close.txt", Some((10, 99))),
            ("edited.txt", Some((10, 100))),
            ("shared", None),
            ("extra.txt", Some((1, 1))),
        ]);
        let plan = run(&local, &remote, &options(SyncDirection::Upload));
        assert_eq!(
            summary(&plan),
            [
                (
                    "edited.txt".into(),
                    SyncAction::Upload,
                    SyncReason::Changed,
                    1,
                    12
                ),
                ("new".into(), SyncAction::Upload, SyncReason::New, 2, 7),
                ("shared/c".into(), SyncAction::Upload, SyncReason::New, 1, 5),
            ]
        );
        assert_eq!(plan.counts.unchanged, 2);
        assert_eq!(plan.counts.extra_on_target, 1);
    }

    #[test]
    fn deleting_extraneous_keeps_folders_with_excluded_files() {
        let local = tree(&[("keep.txt", Some((1, 1)))]);
        let mut remote = tree(&[
            ("keep.txt", Some((1, 1))),
            ("old", None),
            ("old/x", Some((2, 1))),
            ("cache", None),
            ("cache/y", Some((3, 1))),
        ]);
        remote.holds_excluded.insert("cache".into());
        let plan = run(
            &local,
            &remote,
            &Options {
                delete_extraneous: true,
                ..options(SyncDirection::Upload)
            },
        );
        assert_eq!(
            summary(&plan),
            [
                (
                    "cache/y".into(),
                    SyncAction::DeleteRemote,
                    SyncReason::Extraneous,
                    1,
                    3
                ),
                (
                    "old".into(),
                    SyncAction::DeleteRemote,
                    SyncReason::Extraneous,
                    1,
                    2
                ),
            ]
        );
    }

    #[test]
    fn both_directions_take_the_newer_file_and_flag_ties() {
        let local = tree(&[
            ("mine.txt", Some((5, 500))),
            ("theirs.txt", Some((5, 100))),
            ("tie.txt", Some((5, 100))),
            ("only-local", Some((1, 1))),
        ]);
        let remote = tree(&[
            ("mine.txt", Some((6, 100))),
            ("theirs.txt", Some((6, 500))),
            ("tie.txt", Some((6, 101))),
            ("only-remote", Some((1, 1))),
        ]);
        let plan = run(&local, &remote, &options(SyncDirection::Both));
        assert_eq!(
            summary(&plan),
            [
                (
                    "mine.txt".into(),
                    SyncAction::Upload,
                    SyncReason::Newer,
                    1,
                    5
                ),
                (
                    "only-local".into(),
                    SyncAction::Upload,
                    SyncReason::New,
                    1,
                    1
                ),
                (
                    "only-remote".into(),
                    SyncAction::Download,
                    SyncReason::New,
                    1,
                    1
                ),
                (
                    "theirs.txt".into(),
                    SyncAction::Download,
                    SyncReason::Newer,
                    1,
                    6
                ),
                (
                    "tie.txt".into(),
                    SyncAction::Conflict,
                    SyncReason::BothChanged,
                    1,
                    0
                ),
            ]
        );
    }

    #[test]
    fn newer_targets_existing_files_and_type_changes() {
        let local = tree(&[
            ("a.txt", Some((1, 100))),
            ("b.txt", Some((1, 100))),
            ("thing", Some((1, 1))),
        ]);
        let remote = tree(&[
            ("a.txt", Some((2, 900))),
            ("b.txt", Some((2, 10))),
            ("thing", None),
            ("thing/inner", Some((1, 1))),
        ]);
        let plan = run(
            &local,
            &remote,
            &Options {
                skip_newer_on_target: true,
                delete_extraneous: true,
                ..options(SyncDirection::Upload)
            },
        );
        assert_eq!(
            summary(&plan),
            [
                (
                    "b.txt".into(),
                    SyncAction::Upload,
                    SyncReason::Changed,
                    1,
                    1
                ),
                (
                    "thing".into(),
                    SyncAction::Conflict,
                    SyncReason::TypeDiffers,
                    1,
                    0
                ),
            ]
        );
        assert_eq!(plan.counts.kept, 1);

        let plan = run(
            &local,
            &remote,
            &Options {
                ignore_existing: true,
                ..options(SyncDirection::Upload)
            },
        );
        assert_eq!(plan.counts.kept, 2);
    }

    #[test]
    fn checksums_and_size_only_decide_by_content() {
        let local = tree(&[("same", Some((4, 1))), ("differs", Some((4, 1)))]);
        let remote = tree(&[("same", Some((4, 9))), ("differs", Some((4, 9)))]);
        let checksum = Options {
            compare: CompareMode::Checksum,
            ..options(SyncDirection::Download)
        };
        let hashed = files_to_hash(&local, &remote, &checksum);
        assert_eq!(hashed, ["differs", "same"]);
        let contents = HashMap::from([("same".to_string(), true), ("differs".to_string(), false)]);
        let plan = plan(&local, &remote, &checksum, &contents);
        assert_eq!(
            summary(&plan),
            [(
                "differs".into(),
                SyncAction::Download,
                SyncReason::ContentDiffers,
                1,
                4
            )]
        );

        let size_only = Options {
            compare: CompareMode::SizeOnly,
            ..options(SyncDirection::Download)
        };
        assert!(run(&local, &remote, &size_only).items.is_empty());
        let always = Options {
            compare: CompareMode::Always,
            ..options(SyncDirection::Download)
        };
        assert_eq!(run(&local, &remote, &always).items.len(), 2);
    }
}
