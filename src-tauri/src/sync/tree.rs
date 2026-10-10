//! Lists everything under a folder on either side, following links the way transfers do and
//! leaving out what the filter excludes.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio_util::sync::CancellationToken;

use super::filter::Filter;
use crate::error::{AppError, AppResult};
use crate::local;
use crate::model::{DirListing, EntryKind, FileEntry, LinkTarget};
use crate::remote_path;
use crate::sftp::RemoteFs;

const PARALLEL_LISTINGS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<i64>,
}

#[derive(Debug, Default)]
pub struct Tree {
    /// The listed folder, with links resolved.
    pub root: String,
    /// Keyed by path relative to the root, with `/` between components.
    pub nodes: BTreeMap<String, Node>,
    /// Folders with excluded or passed over entries somewhere inside, which are never deleted
    /// whole.
    pub holds_excluded: HashSet<String>,
    pub excluded: u64,
    /// Links that lead nowhere or in a circle, special files, and unreadable folders. What
    /// they are is unknown, so a plan leaves these paths alone on both sides.
    pub passed_over: Vec<String>,
}

impl Tree {
    /// Entries under `folder`, in path order.
    pub fn inside<'a>(&'a self, folder: &str) -> impl Iterator<Item = (&'a String, &'a Node)> {
        let start = format!("{folder}/");
        // `0` follows `/`, so the range ends after the last path that starts with `folder/`.
        let end = format!("{folder}0");
        self.nodes.range(start..end)
    }

    fn add(&mut self, parent: &str, entry: &FileEntry, filter: &Filter) -> Option<String> {
        let relative = if parent.is_empty() {
            entry.name.clone()
        } else {
            format!("{parent}/{}", entry.name)
        };
        let is_dir = entry.is_dir_like();
        let is_file = entry.kind == EntryKind::File || entry.link_target == Some(LinkTarget::File);
        if !is_dir && !is_file {
            self.pass_over(relative);
            return None;
        }
        if filter.excludes(&relative, is_dir) {
            self.excluded += 1;
            self.mark_excluded(parent);
            return None;
        }
        self.nodes.insert(
            relative.clone(),
            Node {
                is_dir,
                size: if is_dir { 0 } else { entry.size },
                modified: entry.modified,
            },
        );
        is_dir.then_some(relative)
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

/// Counts entries as they are listed, for progress.
pub type Counter = Arc<AtomicU64>;

pub fn walk_local(
    root: &str,
    filter: &Filter,
    counter: &Counter,
    cancel: &CancellationToken,
) -> AppResult<Tree> {
    let top = local::list_dir(root)?;
    let mut tree = Tree {
        root: top.path.clone(),
        ..Tree::default()
    };
    let mut pending = vec![(String::new(), Arc::<[String]>::from([]), top)];
    while let Some((relative, ancestors, listing)) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        let mut chain = ancestors.to_vec();
        chain.push(listing.path.clone());
        let chain: Arc<[String]> = chain.into();
        for entry in &listing.entries {
            if local::validate_name(&entry.name).is_err() {
                continue;
            }
            counter.fetch_add(1, Ordering::Relaxed);
            let Some(folder) = tree.add(&relative, entry, filter) else {
                continue;
            };
            match local::list_dir(&entry.path) {
                Ok(inner) if !chain.contains(&inner.path) => {
                    pending.push((folder, chain.clone(), inner));
                }
                _ => tree.pass_over(folder),
            }
        }
    }
    Ok(tree)
}

pub async fn walk_remote(
    fs: &RemoteFs,
    root: &str,
    filter: &Filter,
    counter: &Counter,
    cancel: &CancellationToken,
) -> AppResult<Tree> {
    let top = fs.list_dir(root).await?;
    let mut tree = Tree {
        root: top.path.clone(),
        ..Tree::default()
    };
    let mut ready = vec![(String::new(), Arc::<[String]>::from([]), top)];
    let mut listing = FuturesUnordered::new();
    loop {
        while let Some((relative, ancestors, folder)) = ready.pop() {
            let mut chain = ancestors.to_vec();
            chain.push(folder.path.clone());
            let chain: Arc<[String]> = chain.into();
            for entry in &folder.entries {
                // Names a local folder could not hold are left alone.
                if local::validate_name(&entry.name).is_err() {
                    continue;
                }
                counter.fetch_add(1, Ordering::Relaxed);
                if let Some(child) = tree.add(&relative, entry, filter) {
                    let path = remote_path::join(&folder.path, &entry.name);
                    listing.push(list(fs, child, path, chain.clone()));
                }
            }
            while listing.len() >= PARALLEL_LISTINGS {
                take_listing(&mut tree, &mut ready, next(&mut listing, cancel).await?);
            }
        }
        if listing.is_empty() {
            break;
        }
        take_listing(&mut tree, &mut ready, next(&mut listing, cancel).await?);
    }
    Ok(tree)
}

type Listed = (String, Arc<[String]>, AppResult<DirListing>);

async fn list(fs: &RemoteFs, relative: String, path: String, chain: Arc<[String]>) -> Listed {
    let listing = fs.list_dir(&path).await;
    (relative, chain, listing)
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
    tree: &mut Tree,
    ready: &mut Vec<(String, Arc<[String]>, DirListing)>,
    (relative, chain, listing): Listed,
) {
    match listing {
        Ok(listing) if !chain.contains(&listing.path) => ready.push((relative, chain, listing)),
        _ => tree.pass_over(relative),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let filter = Filter::new(&["target/".to_string(), "*.log".to_string()]).unwrap();
        let counter = Counter::default();
        let tree = walk_local(
            &root.to_string_lossy(),
            &filter,
            &counter,
            &CancellationToken::new(),
        )
        .unwrap();
        let paths: Vec<&str> = tree.nodes.keys().map(String::as_str).collect();
        assert_eq!(paths, ["src", "src/deep", "src/main.rs"]);
        assert_eq!(tree.nodes["src/main.rs"].size, 12);
        assert_eq!(tree.excluded, 2);
        assert!(tree.holds_excluded.contains("src/deep"));
        assert!(tree.holds_excluded.contains("src"));
        #[cfg(unix)]
        assert_eq!(tree.passed_over, ["src/loop"]);
        let inside: Vec<&String> = tree.inside("src").map(|(path, _)| path).collect();
        assert_eq!(inside, ["src/deep", "src/main.rs"]);
    }
}
