//! Decides what a synchronization does from the listings of both sides. Kept free of I/O so
//! every rule is tested without a server.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::chmod::Chmod;
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
    /// Updates only what the target already has (`--existing`).
    pub existing_only: bool,
    pub time_tolerance_secs: i64,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    /// Leaves out folders that would end up with no files (`-m`).
    pub prune_empty_dirs: bool,
    /// Deletes at most this many files and folders.
    pub max_delete: Option<u64>,
    /// Replaces a folder that is not empty with a file of the same name.
    pub force: bool,
    /// Deletes even when part of the source could not be read.
    pub ignore_errors: bool,
    /// Sends only what a shorter target lacks.
    pub append: bool,
    /// Sets the times of files whose contents already match.
    pub times: bool,
    /// Sets the permissions of files whose contents already match.
    pub perms: bool,
    /// `--chmod`, applied to the source's permissions before they are compared.
    pub chmod: Chmod,
}

impl Options {
    pub fn new(direction: SyncDirection, compare: CompareMode) -> Self {
        Self {
            direction,
            compare,
            delete_extraneous: false,
            skip_newer_on_target: false,
            ignore_existing: false,
            existing_only: false,
            time_tolerance_secs: 2,
            min_size: None,
            max_size: None,
            prune_empty_dirs: false,
            max_delete: None,
            force: false,
            ignore_errors: false,
            append: false,
            times: false,
            perms: false,
            chmod: Chmod::default(),
        }
    }

    /// Whether a target's permissions differ from what `--perms` gives it from the source.
    fn permissions_differ(&self, source: &Node, target: &Node) -> bool {
        match (source.permissions, target.permissions) {
            (Some(wanted), Some(current)) if self.perms => {
                self.chmod.apply(wanted, source.is_dir) != current & 0o7777
            }
            _ => false,
        }
    }

    /// Whether `--min-size` and `--max-size` let a file be copied. Folders and links always are.
    pub fn size_allowed(&self, node: &Node) -> bool {
        node.is_dir
            || node.link.is_some()
            || (self.min_size.is_none_or(|min| node.size >= min)
                && self.max_size.is_none_or(|max| node.size <= max))
    }
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

impl SyncAction {
    pub fn deletes(self) -> bool {
        matches!(self, Self::DeleteLocal | Self::DeleteRemote)
    }
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
    /// A file on one side and a folder on the other, or a link and something else. Copying
    /// replaces what the target has.
    TypeDiffers,
    /// Different, with the same modification time on both sides.
    BothChanged,
    /// The target is shorter; only the rest is sent.
    Append,
    /// The contents match; only the time or permissions are set.
    Attributes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<u32>,
    pub is_link: bool,
}

impl From<&Node> for Facts {
    fn from(node: &Node) -> Self {
        Self {
            is_dir: node.is_dir,
            size: node.size,
            modified: node.modified,
            permissions: node.permissions,
            is_link: node.link.is_some(),
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
    /// The change in rsync's `--itemize-changes` form, such as `>f.st......`.
    pub itemized: String,
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
    /// Only on the target, and kept by a protect rule.
    pub protected: u64,
    /// New on the source, and not created because only existing files are updated.
    pub not_created: u64,
    /// Outside the minimum or maximum size.
    pub size_limited: u64,
    /// Deletions past the `--max-delete` limit.
    pub held_back: u64,
    /// Folders left out because no file would end up in them.
    pub pruned: u64,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub items: Vec<SyncItem>,
    pub counts: Counts,
    /// What the user should know about the plan as a whole.
    pub notes: Vec<String>,
    /// Files already the same on both sides, which `--remove-source-files` removes too.
    pub unchanged_files: Vec<String>,
}

/// The root of the folder a plan's deletions mirror, when that folder listed empty. A folder
/// on a disk that is not mounted, or a share that is not there, lists as empty, which reads as
/// "delete everything on the other side".
pub fn deletions_mirror_empty_folder<'a>(
    local: &'a Tree,
    remote: &'a Tree,
    items: &[SyncItem],
) -> Option<&'a str> {
    let mirrored = items.iter().find_map(|item| match item.action {
        SyncAction::DeleteRemote => Some(local),
        SyncAction::DeleteLocal => Some(remote),
        _ => None,
    })?;
    mirrored.nodes.is_empty().then_some(mirrored.root.as_str())
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
                && node.link.is_none()
                && remote.nodes.get(*path).is_some_and(|other| {
                    !other.is_dir && other.link.is_none() && other.size == node.size
                })
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
    let sender_unreadable = match options.direction {
        SyncDirection::Upload => local.unreadable,
        SyncDirection::Download => remote.unreadable,
        SyncDirection::Both => 0,
    };
    let deleting = options.delete_extraneous && (sender_unreadable == 0 || options.ignore_errors);
    let mut planner = Planner {
        local,
        remote,
        options,
        same_contents,
        deleting,
        deleted: 0,
        plan: Plan {
            counts: Counts {
                excluded: local.excluded + remote.excluded,
                passed_over: (local.passed_over.len() + remote.passed_over.len()) as u64,
                ..Counts::default()
            },
            ..Plan::default()
        },
        whole_folders: HashSet::new(),
        passed_over: local
            .passed_over
            .iter()
            .chain(&remote.passed_over)
            .map(String::as_str)
            .collect(),
    };
    let paths: BTreeSet<&String> = local.nodes.keys().chain(remote.nodes.keys()).collect();
    for path in paths {
        if !planner.within_whole_folder(path) && !planner.within_passed_over(path) {
            planner.decide(path);
        }
    }
    let mut plan = planner.plan;
    if options.delete_extraneous && !deleting {
        plan.notes.push(format!(
            "Nothing is deleted because {} on the source could not be read; ignoring errors deletes anyway",
            plural(sender_unreadable, "folder")
        ));
    }
    if plan.counts.held_back > 0 {
        plan.notes.push(format!(
            "{} held back by the limit of {}",
            plural(plan.counts.held_back, "deletion"),
            options.max_delete.unwrap_or(0)
        ));
    }
    plan
}

fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

struct Planner<'a> {
    local: &'a Tree,
    remote: &'a Tree,
    options: &'a Options,
    same_contents: &'a HashMap<String, bool>,
    /// Deleting is on, and nothing on the source failed to list.
    deleting: bool,
    /// Files and folders deleted so far, for the limit.
    deleted: u64,
    plan: Plan,
    /// Folders handled as one item, so nothing inside them is looked at again.
    whole_folders: HashSet<String>,
    /// Paths either side could not read. A folder that failed to list is not absent, so
    /// nothing is copied over it or deleted to match it.
    passed_over: HashSet<&'a str>,
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
            (Some(local), Some(remote))
                if local.is_dir != remote.is_dir
                    || local.link.is_some() != remote.link.is_some() =>
            {
                self.type_differs(path, local, remote)
            }
            (Some(local), Some(remote)) if local.link.is_some() => {
                self.both_links(path, local, remote)
            }
            (Some(local), Some(remote)) => self.both_files(path, local, remote),
            (None, None) => {}
        }
    }

    /// The copy action toward the target in a one-way synchronization.
    fn one_way_copy(&self) -> Option<SyncAction> {
        match self.options.direction {
            SyncDirection::Upload => Some(SyncAction::Upload),
            SyncDirection::Download => Some(SyncAction::Download),
            SyncDirection::Both => None,
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
        if node.is_dir {
            self.whole_folders.insert(path.to_string());
        }
        if copies_from_here {
            if self.options.existing_only {
                self.plan.counts.not_created += 1;
            } else if !self.options.size_allowed(node) {
                self.plan.counts.size_limited += 1;
            } else if node.is_dir
                && self.options.prune_empty_dirs
                && !self.holds_copied_files(side, path)
            {
                self.plan.counts.pruned += 1;
            } else {
                self.push(path, node.is_dir, copy, SyncReason::New);
            }
        } else if !self.deleting {
            self.plan.counts.extra_on_target += 1;
        } else if node.protected {
            self.plan.counts.protected += 1;
        } else if node.is_dir && self.tree(side).holds_excluded.contains(path) {
            // Excluded files inside stay, so the folder does too; what else is in it goes.
            self.whole_folders.remove(path);
        } else {
            self.push(path, node.is_dir, delete, SyncReason::Extraneous);
        }
    }

    /// A file, folder or link on one side has something of another type on the other.
    fn type_differs(&mut self, path: &str, local: &Node, remote: &Node) {
        let conflict = |planner: &mut Self| {
            planner.whole_folders.insert(path.to_string());
            planner.push(
                path,
                local.is_dir,
                SyncAction::Conflict,
                SyncReason::TypeDiffers,
            );
        };
        let Some(copy) = self.one_way_copy() else {
            return conflict(self);
        };
        let (source, target, target_side) = if copy == SyncAction::Upload {
            (local, remote, Side::Remote)
        } else {
            (remote, local, Side::Local)
        };
        let target_tree = self.tree(target_side);
        let blocked = target.protected
            || (target.is_dir
                && (target_tree.holds_excluded.contains(path)
                    || (target_tree.inside(path).next().is_some()
                        && !(self.options.force || self.deleting))));
        if blocked {
            return conflict(self);
        }
        if !self.options.size_allowed(source) {
            self.plan.counts.size_limited += 1;
            return;
        }
        self.whole_folders.insert(path.to_string());
        self.push(path, source.is_dir, copy, SyncReason::TypeDiffers);
    }

    fn both_links(&mut self, path: &str, local: &Node, remote: &Node) {
        if local.link == remote.link {
            self.plan.counts.unchanged += 1;
            return;
        }
        if self.options.ignore_existing {
            self.plan.counts.kept += 1;
            return;
        }
        match self.one_way_copy() {
            Some(copy) => self.push(path, false, copy, SyncReason::Changed),
            None => self.push(path, false, SyncAction::Conflict, SyncReason::BothChanged),
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
            let copy = self.one_way_copy();
            let permissions_differ = match copy {
                Some(SyncAction::Upload) => options.permissions_differ(local, remote),
                Some(SyncAction::Download) => options.permissions_differ(remote, local),
                _ => false,
            };
            match copy {
                Some(copy) if (options.times && !close_in_time) || permissions_differ => {
                    self.push(path, false, copy, SyncReason::Attributes);
                }
                _ => {
                    self.plan.counts.unchanged += 1;
                    self.plan.unchanged_files.push(path.to_string());
                }
            }
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
        let source = match action {
            SyncAction::Upload => Some(local),
            SyncAction::Download => Some(remote),
            _ => None,
        };
        if source.is_some_and(|source| !options.size_allowed(source)) {
            self.plan.counts.size_limited += 1;
            return;
        }
        let reason = match (source, action) {
            (Some(source), SyncAction::Upload | SyncAction::Download)
                if options.append && options.direction != SyncDirection::Both =>
            {
                let target = if action == SyncAction::Upload {
                    remote
                } else {
                    local
                };
                if target.size >= source.size {
                    // rsync skips a file the target has at least as much of.
                    self.plan.counts.kept += 1;
                    return;
                }
                SyncReason::Append
            }
            _ => reason,
        };
        self.push(path, false, action, reason);
    }

    /// Whether copying the folder at `path` would put any file on the target.
    fn holds_copied_files(&self, side: Side, path: &str) -> bool {
        self.tree(side)
            .inside(path)
            .any(|(_, inner)| !inner.is_dir && self.options.size_allowed(inner))
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
        if action.deletes() {
            let weight = match measured {
                Some((side, node)) if node.is_dir => {
                    1 + self.tree(side).inside(path).count() as u64
                }
                _ => 1,
            };
            if self
                .options
                .max_delete
                .is_some_and(|limit| self.deleted + weight > limit)
            {
                self.plan.counts.held_back += weight;
                return;
            }
            self.deleted += weight;
        }
        let copies = !action.deletes();
        let (files, bytes) = match measured {
            Some((side, node)) if node.is_dir => self
                .tree(side)
                .inside(path)
                .filter(|(_, inner)| !inner.is_dir && (!copies || self.options.size_allowed(inner)))
                .fold((0, 0), |(files, bytes), (_, inner)| {
                    (files + 1, bytes + inner.size)
                }),
            Some(_) if reason == SyncReason::Attributes => (1, 0),
            Some((_, node)) => (1, node.size),
            None => (1, 0),
        };
        let itemized = itemize(action, reason, is_dir, local, remote, self.options);
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
            itemized,
        });
    }

    fn tree(&self, side: Side) -> &Tree {
        match side {
            Side::Local => self.local,
            Side::Remote => self.remote,
        }
    }

    fn within_passed_over(&self, path: &str) -> bool {
        let mut current = path;
        loop {
            if self.passed_over.contains(current) {
                return true;
            }
            match current.rsplit_once('/') {
                Some((parent, _)) => current = parent,
                None => return false,
            }
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

/// rsync's `--itemize-changes` code: what moves where, the kind of entry, and what differs.
fn itemize(
    action: SyncAction,
    reason: SyncReason,
    is_dir: bool,
    local: Option<&Node>,
    remote: Option<&Node>,
    options: &Options,
) -> String {
    let (source, target, arrow) = match action {
        SyncAction::Upload => (local, remote, '<'),
        SyncAction::Download => (remote, local, '>'),
        SyncAction::DeleteLocal | SyncAction::DeleteRemote => return "*deleting".into(),
        SyncAction::Conflict => return String::new(),
    };
    let Some(source) = source else {
        return String::new();
    };
    let kind = if is_dir {
        'd'
    } else if source.link.is_some() {
        'L'
    } else {
        'f'
    };
    let update = if reason == SyncReason::Attributes {
        '.'
    } else if kind == 'f' {
        arrow
    } else {
        'c'
    };
    let target = target.filter(|_| reason != SyncReason::TypeDiffers);
    let Some(target) = target else {
        return format!("{update}{kind}+++++++++");
    };
    let flag = |differs: bool, letter: char| if differs { letter } else { '.' };
    let times_differ = match (source.modified, target.modified) {
        (Some(source_time), Some(target_time)) => {
            (source_time - target_time).abs() > options.time_tolerance_secs
        }
        _ => false,
    };
    let checksum = match reason {
        SyncReason::ContentDiffers => true,
        _ => source.link.is_some() && source.link != target.link,
    };
    let permissions = options.permissions_differ(source, target);
    format!(
        "{update}{kind}{}{}{}{}.....",
        flag(checksum, 'c'),
        flag(source.size != target.size && source.link.is_none(), 's'),
        flag(times_differ && options.times, 't'),
        flag(permissions, 'p'),
    )
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
                    ..Node::default()
                },
            );
        }
        tree
    }

    fn options(direction: SyncDirection) -> Options {
        Options::new(direction, CompareMode::SizeAndTime)
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
    fn deletions_that_mirror_an_empty_folder_are_noticed() {
        let mut local = tree(&[]);
        local.root = "/mnt/backup".into();
        let remote = tree(&[("a.txt", Some((1, 1))), ("b", None)]);
        let mirror = Options {
            delete_extraneous: true,
            ..options(SyncDirection::Upload)
        };
        let plan = run(&local, &remote, &mirror);
        assert_eq!(
            deletions_mirror_empty_folder(&local, &remote, &plan.items),
            Some("/mnt/backup")
        );

        let local = tree(&[("a.txt", Some((1, 1)))]);
        let plan = run(&local, &remote, &mirror);
        assert_eq!(
            deletions_mirror_empty_folder(&local, &remote, &plan.items),
            None
        );
        let plan = run(&local, &tree(&[]), &options(SyncDirection::Download));
        assert_eq!(
            deletions_mirror_empty_folder(&local, &tree(&[]), &plan.items),
            None
        );
    }

    #[test]
    fn folders_one_side_could_not_read_are_left_alone() {
        let delete = |direction| Options {
            delete_extraneous: true,
            ..options(direction)
        };
        // The server folder timed out while listing, so it looks absent from the server.
        let local = tree(&[("photos", None), ("photos/a.jpg", Some((5, 1)))]);
        let mut remote = tree(&[]);
        remote.passed_over.push("photos".into());
        assert!(run(&local, &remote, &delete(SyncDirection::Upload))
            .items
            .is_empty());
        assert!(run(&local, &remote, &delete(SyncDirection::Download))
            .items
            .is_empty());

        // The local folder could not be read, so it looks absent from this computer.
        let mut local = tree(&[("keep.txt", Some((1, 1))), ("private", None)]);
        local.passed_over.push("private/secrets".into());
        local.holds_excluded.insert("private".into());
        let remote = tree(&[
            ("keep.txt", Some((1, 1))),
            ("private", None),
            ("private/secrets", None),
            ("private/secrets/key", Some((2, 1))),
            ("private/stale.txt", Some((3, 1))),
        ]);
        assert_eq!(
            summary(&run(&local, &remote, &delete(SyncDirection::Upload))),
            [(
                "private/stale.txt".into(),
                SyncAction::DeleteRemote,
                SyncReason::Extraneous,
                1,
                3
            )]
        );
        assert_eq!(
            summary(&run(&local, &remote, &delete(SyncDirection::Download))),
            [(
                "private/stale.txt".into(),
                SyncAction::Download,
                SyncReason::New,
                1,
                3
            )]
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
                    SyncAction::Upload,
                    SyncReason::TypeDiffers,
                    1,
                    1
                ),
            ]
        );
        assert_eq!(plan.counts.kept, 1);
        // Without deleting or --force, a folder with files in it is not replaced.
        let plan = run(
            &local,
            &remote,
            &Options {
                skip_newer_on_target: true,
                ..options(SyncDirection::Upload)
            },
        );
        assert_eq!(plan.items[1].action, SyncAction::Conflict);
        let plan = run(
            &local,
            &remote,
            &Options {
                force: true,
                ..options(SyncDirection::Upload)
            },
        );
        assert_eq!(plan.items[2].reason, SyncReason::TypeDiffers);
        assert_eq!(plan.items[2].action, SyncAction::Upload);

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

    #[test]
    fn existing_size_limits_and_empty_folders() {
        let local = tree(&[
            ("new.txt", Some((5, 1))),
            ("big.bin", Some((5000, 1))),
            ("tiny", Some((1, 1))),
            ("shared.txt", Some((50, 9))),
            ("empty", None),
            ("empty/deeper", None),
            ("mixed", None),
            ("mixed/huge.iso", Some((9000, 1))),
            ("mixed/ok.txt", Some((10, 1))),
        ]);
        let remote = tree(&[("shared.txt", Some((40, 1))), ("big.bin", Some((10, 1)))]);
        let limited = Options {
            min_size: Some(2),
            max_size: Some(1000),
            prune_empty_dirs: true,
            ..options(SyncDirection::Upload)
        };
        let plan = run(&local, &remote, &limited);
        assert_eq!(
            summary(&plan),
            [
                ("mixed".into(), SyncAction::Upload, SyncReason::New, 1, 10),
                ("new.txt".into(), SyncAction::Upload, SyncReason::New, 1, 5),
                (
                    "shared.txt".into(),
                    SyncAction::Upload,
                    SyncReason::Changed,
                    1,
                    50
                ),
            ]
        );
        assert_eq!(plan.counts.size_limited, 2);
        assert_eq!(plan.counts.pruned, 1);

        let existing = Options {
            existing_only: true,
            ..options(SyncDirection::Upload)
        };
        let plan = run(&local, &remote, &existing);
        assert_eq!(
            plan.items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            ["big.bin", "shared.txt"]
        );
        assert_eq!(plan.counts.not_created, 4);
    }

    #[test]
    fn deletion_limits_errors_and_protection() {
        let local = tree(&[("keep", Some((1, 1)))]);
        let mut remote = tree(&[
            ("keep", Some((1, 1))),
            ("a", Some((1, 1))),
            ("b", None),
            ("b/1", Some((1, 1))),
            ("b/2", Some((1, 1))),
            ("c", Some((1, 1))),
            ("guarded", Some((1, 1))),
        ]);
        remote.nodes.get_mut("guarded").unwrap().protected = true;
        let limited = Options {
            delete_extraneous: true,
            max_delete: Some(2),
            ..options(SyncDirection::Upload)
        };
        let plan = run(&local, &remote, &limited);
        assert_eq!(
            plan.items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert_eq!(plan.counts.held_back, 3);
        assert_eq!(plan.counts.protected, 1);
        assert_eq!(plan.notes, ["3 deletions held back by the limit of 2"]);

        let mut unreadable = local;
        unreadable.unreadable = 1;
        let deleting = Options {
            delete_extraneous: true,
            ..options(SyncDirection::Upload)
        };
        let plan = run(&unreadable, &remote, &deleting);
        assert!(plan.items.is_empty());
        assert_eq!(plan.notes.len(), 1);
        let plan = run(
            &unreadable,
            &remote,
            &Options {
                ignore_errors: true,
                ..deleting
            },
        );
        assert_eq!(plan.items.len(), 3);
    }

    #[test]
    fn appending_and_attribute_updates() {
        let local = tree(&[
            ("log", Some((100, 50))),
            ("same", Some((10, 50))),
            ("longer", Some((5, 50))),
        ]);
        let remote = tree(&[
            ("log", Some((60, 10))),
            ("same", Some((10, 10))),
            ("longer", Some((9, 10))),
        ]);
        let append = Options {
            append: true,
            ..options(SyncDirection::Upload)
        };
        let plan = run(&local, &remote, &append);
        // A target at least as long as the source is skipped.
        assert_eq!(
            summary(&plan),
            [("log".into(), SyncAction::Upload, SyncReason::Append, 1, 100)]
        );
        assert_eq!(plan.counts.kept, 2);

        let size_only = Options {
            compare: CompareMode::SizeOnly,
            times: true,
            ..options(SyncDirection::Upload)
        };
        let plan = run(&local, &remote, &size_only);
        let same = plan.items.iter().find(|item| item.path == "same").unwrap();
        assert_eq!(same.reason, SyncReason::Attributes);
        assert_eq!((same.files, same.bytes), (1, 0));
        assert_eq!(same.itemized, ".f..t......");
        let log = plan.items.iter().find(|item| item.path == "log").unwrap();
        assert_eq!(log.itemized, "<f.st......");
    }

    #[test]
    fn links_compare_by_where_they_point() {
        let link = |target: &str| Node {
            link: Some(target.to_string()),
            size: target.len() as u64,
            ..Node::default()
        };
        let mut local = tree(&[("file", Some((3, 1)))]);
        local.nodes.insert("same".into(), link("a"));
        local.nodes.insert("moved".into(), link("b"));
        local.nodes.insert("new-link".into(), link("c"));
        local.nodes.insert("was-file".into(), link("d"));
        let mut remote = tree(&[("file", Some((3, 1))), ("was-file", Some((1, 1)))]);
        remote.nodes.insert("same".into(), link("a"));
        remote.nodes.insert("moved".into(), link("x"));
        let plan = run(&local, &remote, &options(SyncDirection::Upload));
        let found: Vec<(&str, SyncReason, &str)> = plan
            .items
            .iter()
            .map(|item| (item.path.as_str(), item.reason, item.itemized.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("moved", SyncReason::Changed, "cLc........"),
                ("new-link", SyncReason::New, "cL+++++++++"),
                ("was-file", SyncReason::TypeDiffers, "cL+++++++++"),
            ]
        );
        let downloads = run(&local, &remote, &options(SyncDirection::Download));
        assert_eq!(downloads.items[0].itemized, "cLc........");
        let deleting = Options {
            delete_extraneous: true,
            ..options(SyncDirection::Download)
        };
        let plan = run(&local, &remote, &deleting);
        assert!(plan
            .items
            .iter()
            .any(|item| item.path == "new-link" && item.itemized == "*deleting"));
    }
}
