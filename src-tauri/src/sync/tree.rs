//! Lists everything under a folder on either side, applying the filter rules the way the
//! side's role in the synchronization calls for, and handling links as the options say.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio_util::sync::CancellationToken;

use super::filter::{Filter, FolderRules, Side, Verdict};
use super::options::LinkMode;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::local;
use crate::model::{DirListing, EntryKind, FileEntry, LinkTarget};
use crate::sftp::RemoteFs;

const PARALLEL_LISTINGS: usize = 8;
/// Larger rule files are refused rather than read into memory.
const MAX_RULE_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Node {
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// Where a link kept as a link points.
    pub link: Option<String>,
    /// The receiving side's rules keep it from being deleted.
    pub protected: bool,
}

#[derive(Debug, Default)]
pub struct Tree {
    /// The listed folder, with links resolved.
    pub root: String,
    /// Keyed by path relative to the root, with `/` between components.
    pub nodes: BTreeMap<String, Node>,
    /// Folders with excluded, protected or passed over entries somewhere inside, which are
    /// never deleted whole.
    pub holds_excluded: HashSet<String>,
    pub excluded: u64,
    /// Links that lead nowhere or in a circle, special files, and unreadable folders. What
    /// they are is unknown, so a plan leaves these paths alone on both sides.
    pub passed_over: Vec<String>,
    /// Folders that could not be read. rsync deletes nothing after such an error unless told
    /// to ignore errors.
    pub unreadable: u64,
    /// The folder does not exist yet; it is created when the synchronization runs.
    pub missing: bool,
}

impl Tree {
    /// Entries under `folder`, in path order.
    pub fn inside<'a>(&'a self, folder: &str) -> impl Iterator<Item = (&'a String, &'a Node)> {
        let start = format!("{folder}/");
        // `0` follows `/`, so the range ends after the last path that starts with `folder/`.
        let end = format!("{folder}0");
        self.nodes.range(start..end)
    }

    fn pass_over(&mut self, relative: String) {
        self.nodes.remove(&relative);
        if let Some((parent, _)) = relative.rsplit_once('/') {
            self.mark_excluded(parent);
        }
        self.passed_over.push(relative);
    }

    fn mark_excluded(&mut self, folder: &str) {
        let mut current = folder;
        while !current.is_empty() && self.holds_excluded.insert(current.to_string()) {
            current = current.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
    }
}

/// The part a side plays, which decides the rules that apply to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Sender,
    Receiver,
    /// Both at once, in a two-way synchronization.
    Both,
}

/// How to walk one side.
#[derive(Debug, Clone)]
pub struct Walk {
    pub filter: Filter,
    pub role: Role,
    pub recursive: bool,
    /// Without recursion, still lists the folders at the top.
    pub dirs: bool,
    pub links: LinkMode,
    pub safe_links: bool,
    pub copy_unsafe_links: bool,
    pub copy_dirlinks: bool,
    pub keep_dirlinks: bool,
    /// Reads each entry's owner and group.
    pub owners: bool,
    /// A missing folder lists as empty instead of failing.
    pub allow_missing: bool,
}

impl Walk {
    pub fn new(filter: Filter, role: Role) -> Self {
        Self {
            filter,
            role,
            recursive: true,
            dirs: false,
            links: LinkMode::Follow,
            safe_links: false,
            copy_unsafe_links: false,
            copy_dirlinks: false,
            keep_dirlinks: false,
            owners: false,
            allow_missing: false,
        }
    }

    fn sends(&self) -> bool {
        self.role != Role::Receiver
    }

    /// Whether a link's own text decides what happens to it.
    fn needs_link_text(&self, entry: &FileEntry) -> bool {
        if entry.kind != EntryKind::Symlink {
            return false;
        }
        let to_folder = entry.link_target == Some(LinkTarget::Dir);
        if self.sends() {
            self.links == LinkMode::Preserve && !(self.copy_dirlinks && to_folder)
        } else {
            self.links != LinkMode::Follow && !(self.keep_dirlinks && to_folder)
        }
    }

    fn shape(&self, entry: &FileEntry, link_text: Option<String>, relative: &str) -> Shape {
        let followed = || match entry.link_target {
            Some(LinkTarget::Dir) => Shape::Dir,
            Some(LinkTarget::File) => Shape::File,
            _ => Shape::PassOver,
        };
        match entry.kind {
            EntryKind::Dir => Shape::Dir,
            EntryKind::File => Shape::File,
            EntryKind::Other => Shape::PassOver,
            EntryKind::Symlink => match link_text {
                None if self.sends() && self.links == LinkMode::Skip => Shape::Leave,
                None => followed(),
                Some(text) if self.sends() && is_unsafe_link(&text, relative) => {
                    if self.copy_unsafe_links {
                        followed()
                    } else if self.safe_links {
                        Shape::Leave
                    } else {
                        Shape::Link(text)
                    }
                }
                Some(text) => Shape::Link(text),
            },
        }
    }

    fn judge(&self, path: &str, is_dir: bool, rules: &FolderRules) -> Judgement {
        let verdict = |side| self.filter.verdict(path, is_dir, side, rules);
        if self.sends() {
            return match verdict(Side::Sender) {
                Verdict::Included => Judgement::Keep {
                    protected: false,
                    holds: false,
                },
                Verdict::Excluded { perishable } => Judgement::Skip { holds: !perishable },
            };
        }
        match verdict(Side::Receiver) {
            Verdict::Included => Judgement::Keep {
                protected: false,
                holds: false,
            },
            Verdict::Excluded { perishable } => {
                if verdict(Side::Sender).is_included() {
                    Judgement::Keep {
                        protected: true,
                        holds: !perishable,
                    }
                } else {
                    Judgement::Skip { holds: !perishable }
                }
            }
        }
    }
}

/// What an entry is once the link options are applied.
enum Shape {
    File,
    Dir,
    Link(String),
    /// Left out: a link with neither `-l` nor `-L`, or an unsafe one with `--safe-links`.
    Leave,
    /// A broken link that would be followed, or a special file.
    PassOver,
}

enum Judgement {
    Keep { protected: bool, holds: bool },
    Skip { holds: bool },
}

/// A link that leads out of the synchronized folder: an absolute one, or one whose `..`
/// climbs above the folder it is relative to.
pub fn is_unsafe_link(target: &str, relative_path: &str) -> bool {
    let target = target.replace('\\', "/");
    if target.starts_with('/') || target.get(1..2) == Some(":") {
        return true;
    }
    let mut depth = relative_path.matches('/').count() as i64;
    for component in target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            _ => depth += 1,
        }
    }
    false
}

/// A folder being walked.
#[derive(Clone)]
struct Folder {
    relative: String,
    /// Canonical paths of the folders above, to stop at link loops.
    chain: Arc<[String]>,
    protected: bool,
    rules: FolderRules,
}

/// A folder found inside another, still to list.
struct Child {
    relative: String,
    path: String,
    protected: bool,
}

impl Folder {
    fn child(&self, relative: String, protected: bool) -> Folder {
        Folder {
            relative,
            chain: self.chain.clone(),
            protected,
            rules: self.rules.clone(),
        }
    }
}

struct Collector<'a> {
    walk: &'a Walk,
    tree: Tree,
}

impl Collector<'_> {
    /// Adds an entry of `folder`; returns the folder to list next when it is one.
    fn add(
        &mut self,
        folder: &Folder,
        entry: &FileEntry,
        link_text: Option<String>,
        owner: Option<(u32, u32)>,
    ) -> Option<Child> {
        let relative = if folder.relative.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", folder.relative, entry.name)
        };
        let (is_dir, link) = match self.walk.shape(entry, link_text, &relative) {
            Shape::PassOver => {
                self.tree.pass_over(relative);
                return None;
            }
            Shape::Leave => {
                self.tree.excluded += 1;
                return None;
            }
            Shape::File => (false, None),
            Shape::Dir => (true, None),
            Shape::Link(text) => (false, Some(text)),
        };
        // rsync leaves folders out without -r or -d.
        if is_dir && !self.walk.recursive && !self.walk.dirs && self.walk.sends() {
            return None;
        }
        let protected = match self.walk.judge(&relative, is_dir, &folder.rules) {
            Judgement::Skip { holds } => {
                self.tree.excluded += 1;
                if holds {
                    self.tree.mark_excluded(&folder.relative);
                }
                return None;
            }
            Judgement::Keep { protected, holds } => {
                if holds {
                    self.tree.mark_excluded(&folder.relative);
                }
                protected || folder.protected
            }
        };
        self.tree.nodes.insert(
            relative.clone(),
            Node {
                is_dir,
                size: match (&link, is_dir) {
                    (Some(text), _) => text.len() as u64,
                    (None, true) => 0,
                    (None, false) => entry.size,
                },
                modified: entry.modified,
                permissions: entry.permissions,
                uid: owner.map(|(uid, _)| uid),
                gid: owner.map(|(_, gid)| gid),
                link,
                protected,
            },
        );
        (is_dir && self.walk.recursive).then(|| Child {
            relative,
            path: entry.path.clone(),
            protected,
        })
    }

    /// Notes a folder that could not be listed.
    fn unlisted(&mut self, relative: String, error: Option<AppError>) {
        if error.is_some() {
            self.tree.unreadable += 1;
        }
        self.tree.pass_over(relative);
    }
}

/// Counts entries as they are listed, for progress.
pub type Counter = Arc<AtomicU64>;

fn is_missing(error: &AppError) -> bool {
    error.kind == ErrorKind::NotFound
}

pub fn walk_local(
    root: &str,
    walk: &Walk,
    counter: &Counter,
    cancel: &CancellationToken,
) -> AppResult<Tree> {
    let top = match local::list_dir(root) {
        Ok(top) => top,
        Err(error) if walk.allow_missing && is_missing(&error) => {
            return Ok(Tree {
                root: root.to_string(),
                missing: true,
                ..Tree::default()
            })
        }
        Err(error) => return Err(error),
    };
    let mut collector = Collector {
        walk,
        tree: Tree {
            root: top.path.clone(),
            ..Tree::default()
        },
    };
    let root_folder = Folder {
        relative: String::new(),
        chain: Arc::from([]),
        protected: false,
        rules: FolderRules::default(),
    };
    let mut pending = vec![(root_folder, top)];
    while let Some((mut folder, listing)) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        let mut chain = folder.chain.to_vec();
        chain.push(listing.path.clone());
        folder.chain = chain.into();
        let files = read_local_rule_files(walk, &listing)?;
        folder.rules = walk.filter.enter(&folder.rules, &folder.relative, &files)?;
        for entry in &listing.entries {
            if local::validate_name(&entry.name).is_err() {
                continue;
            }
            counter.fetch_add(1, Ordering::Relaxed);
            let link_text = walk
                .needs_link_text(entry)
                .then(|| std::fs::read_link(&entry.path).ok())
                .flatten()
                .map(|target| target.to_string_lossy().replace('\\', "/"));
            let owner = if walk.owners {
                local_owner(&entry.path)
            } else {
                None
            };
            let Some(child) = collector.add(&folder, entry, link_text, owner) else {
                continue;
            };
            match local::list_dir(&child.path) {
                Ok(inner) if !folder.chain.contains(&inner.path) => {
                    pending.push((folder.child(child.relative, child.protected), inner));
                }
                Ok(_) => collector.unlisted(child.relative, None),
                Err(error) => collector.unlisted(child.relative, Some(error)),
            }
        }
    }
    Ok(collector.tree)
}

fn read_local_rule_files(walk: &Walk, listing: &DirListing) -> AppResult<HashMap<String, String>> {
    let names = walk.filter.merge_file_names();
    let mut files = HashMap::new();
    if names.is_empty() {
        return Ok(files);
    }
    for entry in &listing.entries {
        if names.contains(&entry.name.as_str()) && !entry.is_dir_like() {
            if entry.size > MAX_RULE_FILE_BYTES {
                return Err(too_large(&entry.path));
            }
            let contents = std::fs::read(&entry.path)
                .map_err(|error| AppError::from(error).with_path(entry.path.clone()))?;
            files.insert(
                entry.name.clone(),
                String::from_utf8_lossy(&contents).into_owned(),
            );
        }
    }
    Ok(files)
}

fn too_large(path: &str) -> AppError {
    AppError::invalid(format!(
        "{path} is too large to be a rule file (over {} KiB)",
        MAX_RULE_FILE_BYTES / 1024
    ))
}

#[cfg(unix)]
fn local_owner(path: &str) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)
        .or_else(|_| std::fs::symlink_metadata(path))
        .ok()?;
    Some((metadata.uid(), metadata.gid()))
}

#[cfg(not(unix))]
fn local_owner(_path: &str) -> Option<(u32, u32)> {
    None
}

pub async fn walk_remote(
    fs: &RemoteFs,
    root: &str,
    walk: &Walk,
    counter: &Counter,
    cancel: &CancellationToken,
) -> AppResult<Tree> {
    let top = match fs.list_dir(root).await {
        Ok(top) => top,
        Err(error) if walk.allow_missing && is_missing(&error) => {
            return Ok(Tree {
                root: fs.resolve(root),
                missing: true,
                ..Tree::default()
            })
        }
        Err(error) => return Err(error),
    };
    let mut collector = Collector {
        walk,
        tree: Tree {
            root: top.path.clone(),
            ..Tree::default()
        },
    };
    let root_folder = Folder {
        relative: String::new(),
        chain: Arc::from([]),
        protected: false,
        rules: FolderRules::default(),
    };
    let mut ready = vec![(root_folder, top)];
    let mut listing = FuturesUnordered::new();
    loop {
        while let Some((mut folder, contents)) = ready.pop() {
            let mut chain = folder.chain.to_vec();
            chain.push(contents.path.clone());
            folder.chain = chain.into();
            let files = read_remote_rule_files(fs, walk, &contents).await?;
            folder.rules = walk.filter.enter(&folder.rules, &folder.relative, &files)?;
            let owners = if walk.owners {
                remote_owners(fs, &contents.path).await
            } else {
                HashMap::new()
            };
            for entry in &contents.entries {
                // Names a local folder could not hold are left alone.
                if local::validate_name(&entry.name).is_err() {
                    continue;
                }
                counter.fetch_add(1, Ordering::Relaxed);
                let link_text = if walk.needs_link_text(entry) {
                    fs.read_link(&entry.path).await.ok()
                } else {
                    None
                };
                let owner = owners.get(&entry.name).copied();
                if let Some(child) = collector.add(&folder, entry, link_text, owner) {
                    let inner = folder.child(child.relative, child.protected);
                    listing.push(list(fs, inner, child.path));
                }
            }
            while listing.len() >= PARALLEL_LISTINGS {
                take_listing(
                    &mut collector,
                    &mut ready,
                    next(&mut listing, cancel).await?,
                );
            }
        }
        if listing.is_empty() {
            break;
        }
        take_listing(
            &mut collector,
            &mut ready,
            next(&mut listing, cancel).await?,
        );
    }
    Ok(collector.tree)
}

async fn read_remote_rule_files(
    fs: &RemoteFs,
    walk: &Walk,
    listing: &DirListing,
) -> AppResult<HashMap<String, String>> {
    let names = walk.filter.merge_file_names();
    let mut files = HashMap::new();
    if names.is_empty() {
        return Ok(files);
    }
    for entry in &listing.entries {
        if names.contains(&entry.name.as_str()) && !entry.is_dir_like() {
            if entry.size > MAX_RULE_FILE_BYTES {
                return Err(too_large(&entry.path));
            }
            let contents = fs.read_to_end(&entry.path, MAX_RULE_FILE_BYTES).await?;
            files.insert(
                entry.name.clone(),
                String::from_utf8_lossy(&contents).into_owned(),
            );
        }
    }
    Ok(files)
}

/// The owner and group of each entry in a folder, which listings give by name only.
async fn remote_owners(fs: &RemoteFs, folder: &str) -> HashMap<String, (u32, u32)> {
    fs.entries(folder)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, stat)| Some((name, (stat.uid?, stat.gid?))))
        .collect()
}

type Listed = (Folder, AppResult<DirListing>);

async fn list(fs: &RemoteFs, folder: Folder, path: String) -> Listed {
    let listing = fs.list_dir(&path).await;
    (folder, listing)
}

async fn next<F: std::future::Future<Output = Listed>>(
    listing: &mut FuturesUnordered<F>,
    cancel: &CancellationToken,
) -> AppResult<Listed> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(AppError::cancelled()),
        listed = listing.next() => Ok(listed.expect("called with listings pending")),
    }
}

fn take_listing(
    collector: &mut Collector<'_>,
    ready: &mut Vec<(Folder, DirListing)>,
    (folder, listing): Listed,
) {
    match listing {
        Ok(listing) if !folder.chain.contains(&listing.path) => ready.push((folder, listing)),
        Ok(_) => collector.unlisted(folder.relative, None),
        Err(error) => collector.unlisted(folder.relative, Some(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::filter::FilterSpec;

    fn filter(spec: FilterSpec) -> Filter {
        Filter::build(&spec, &|name| Err(AppError::invalid(name.to_string()))).unwrap()
    }

    fn excluding(patterns: &[&str]) -> Filter {
        filter(FilterSpec {
            excludes: patterns.iter().map(|pattern| pattern.to_string()).collect(),
            ..FilterSpec::default()
        })
    }

    fn walk_dir(root: &std::path::Path, walk: &Walk) -> Tree {
        walk_local(
            &root.to_string_lossy(),
            walk,
            &Counter::default(),
            &CancellationToken::new(),
        )
        .unwrap()
    }

    fn paths(tree: &Tree) -> Vec<&str> {
        tree.nodes.keys().map(String::as_str).collect()
    }

    #[test]
    fn walks_a_local_tree_with_excludes_and_link_loops() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/main.rs"), b"fn main() {}").unwrap();
        std::fs::write(root.join("src/deep/notes.log"), b"log").unwrap();
        std::fs::write(root.join("target/out"), b"binary").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root, root.join("src/loop")).unwrap();

        let walk = Walk::new(excluding(&["target/", "*.log"]), Role::Sender);
        let tree = walk_dir(root, &walk);
        assert_eq!(paths(&tree), ["src", "src/deep", "src/main.rs"]);
        assert_eq!(tree.nodes["src/main.rs"].size, 12);
        assert_eq!(tree.excluded, 2);
        assert!(tree.holds_excluded.contains("src/deep"));
        assert!(tree.holds_excluded.contains("src"));
        assert_eq!(tree.unreadable, 0);
        #[cfg(unix)]
        assert_eq!(tree.passed_over, ["src/loop"]);
        let inside: Vec<&String> = tree.inside("src").map(|(path, _)| path).collect();
        assert_eq!(inside, ["src/deep", "src/main.rs"]);
    }

    #[test]
    fn the_receiver_keeps_protected_files_and_lists_hidden_ones() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("uploads")).unwrap();
        std::fs::write(root.join("uploads/a.jpg"), b"a").unwrap();
        std::fs::write(root.join("secret.txt"), b"s").unwrap();
        std::fs::write(root.join("debug.log"), b"l").unwrap();
        std::fs::write(root.join("x.o"), b"o").unwrap();
        let rules = filter(FilterSpec {
            rules: vec!["P /uploads/".into(), "H secret.txt".into(), "-p *.o".into()],
            excludes: vec!["*.log".into()],
            ..FilterSpec::default()
        });
        let tree = walk_dir(root, &Walk::new(rules.clone(), Role::Receiver));
        assert_eq!(paths(&tree), ["secret.txt", "uploads", "uploads/a.jpg"]);
        assert!(tree.nodes["uploads"].protected);
        assert!(tree.nodes["uploads/a.jpg"].protected);
        assert!(!tree.nodes["secret.txt"].protected);
        assert_eq!(tree.excluded, 2);
        assert!(tree.holds_excluded.is_empty());

        let sender = walk_dir(root, &Walk::new(rules, Role::Sender));
        assert_eq!(paths(&sender), ["uploads", "uploads/a.jpg"]);
    }

    #[test]
    fn per_folder_rule_files_and_no_recursion() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("app/cache")).unwrap();
        std::fs::write(root.join(".rsync-filter"), "- *.bak\n").unwrap();
        std::fs::write(root.join("app/.rsync-filter"), "- cache/\n").unwrap();
        std::fs::write(root.join("app/main.bak"), b"b").unwrap();
        std::fs::write(root.join("app/main.c"), b"c").unwrap();
        std::fs::write(root.join("app/cache/x"), b"x").unwrap();
        std::fs::write(root.join("top.txt"), b"t").unwrap();
        let rules = filter(FilterSpec {
            per_folder_filters: true,
            exclude_filter_files: true,
            ..FilterSpec::default()
        });
        let tree = walk_dir(root, &Walk::new(rules.clone(), Role::Sender));
        assert_eq!(paths(&tree), ["app", "app/main.c", "top.txt"]);

        let flat = Walk {
            recursive: false,
            ..Walk::new(rules.clone(), Role::Sender)
        };
        assert_eq!(paths(&walk_dir(root, &flat)), ["top.txt"]);
        let with_dirs = Walk {
            recursive: false,
            dirs: true,
            ..Walk::new(rules, Role::Sender)
        };
        assert_eq!(paths(&walk_dir(root, &with_dirs)), ["app", "top.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_kept_skipped_or_followed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::write(root.join("real/file"), b"data").unwrap();
        std::os::unix::fs::symlink("real/file", root.join("inside")).unwrap();
        std::os::unix::fs::symlink("/etc/hostname", root.join("outside")).unwrap();
        std::os::unix::fs::symlink("real", root.join("folder")).unwrap();
        let none = Filter::default();

        let keep = Walk {
            links: LinkMode::Preserve,
            ..Walk::new(none.clone(), Role::Sender)
        };
        let tree = walk_dir(root, &keep);
        assert_eq!(tree.nodes["inside"].link.as_deref(), Some("real/file"));
        assert_eq!(tree.nodes["outside"].link.as_deref(), Some("/etc/hostname"));
        assert_eq!(tree.nodes["folder"].link.as_deref(), Some("real"));
        assert!(!tree.nodes["folder"].is_dir);

        let safe = Walk {
            safe_links: true,
            copy_dirlinks: true,
            ..keep.clone()
        };
        let tree = walk_dir(root, &safe);
        assert!(!tree.nodes.contains_key("outside"));
        assert!(tree.nodes["folder"].is_dir);
        assert!(tree.nodes.contains_key("folder/file"));

        let skip = Walk {
            links: LinkMode::Skip,
            ..Walk::new(none, Role::Sender)
        };
        assert_eq!(paths(&walk_dir(root, &skip)), ["real", "real/file"]);
    }

    #[test]
    fn unsafe_links_leave_the_folder() {
        assert!(is_unsafe_link("/etc/passwd", "a"));
        assert!(is_unsafe_link("../x", "a"));
        assert!(!is_unsafe_link("../x", "sub/a"));
        assert!(is_unsafe_link("../../x", "sub/a"));
        assert!(!is_unsafe_link("./b/../c", "a"));
        assert!(is_unsafe_link("C:\\Windows", "a"));
    }

    #[test]
    fn a_missing_folder_lists_empty_when_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("not-yet");
        let walk = Walk {
            allow_missing: true,
            ..Walk::new(Filter::default(), Role::Receiver)
        };
        let tree = walk_dir(&missing, &walk);
        assert!(tree.missing && tree.nodes.is_empty());
        let strict = Walk::new(Filter::default(), Role::Receiver);
        assert!(walk_local(
            &missing.to_string_lossy(),
            &strict,
            &Counter::default(),
            &CancellationToken::new()
        )
        .is_err());
    }
}
