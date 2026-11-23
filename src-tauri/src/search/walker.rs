//! Searching by listing folder after folder, for everything that has no `find` to ask: FTP,
//! WebDAV, the storage services, vaults, SFTP accounts without a shell, and this computer.
//!
//! Folders wait in a queue instead of on the call stack, so a deep tree costs memory for the
//! folders still to list and nothing else. As many are listed at once as the connection takes
//! without requests getting in each other's way.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio_util::sync::CancellationToken;

use super::criteria::Criteria;
use super::Output;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::file_ops::Failure;
use crate::local;
use crate::model::{DirListing, EntryKind, FileEntry, LinkTarget};
use crate::protocol::{DeepPage, Protocol, RemoteFileSystem};

/// SFTP answers requests in any order, so several listings share the channel.
const SFTP_LISTINGS: usize = 8;
/// The storage services take any number of requests; this many keeps clear of their limits.
const HTTP_LISTINGS: usize = 6;
const LOCAL_LISTINGS: usize = 4;
/// Links to folders followed on the way to one folder. Servers that tell where a link leads
/// never get this far: a folder already listed is not listed again.
const MAX_LINKS_FOLLOWED: u8 = 8;

/// Where the folders are.
#[derive(Clone)]
pub(super) enum Folders {
    Local,
    Remote(Arc<dyn RemoteFileSystem>),
}

impl Folders {
    pub async fn list(&self, path: &str) -> AppResult<DirListing> {
        match self {
            Self::Local => {
                let path = path.to_string();
                tokio::task::spawn_blocking(move || local::list_dir(&path))
                    .await
                    .map_err(|error| AppError::new(ErrorKind::Io, error.to_string()))?
            }
            Self::Remote(files) => files.list_dir(path).await,
        }
    }

    /// How many folders are listed at once. FTP has one control connection that requests
    /// take turns on, so more would only queue up behind each other.
    pub fn parallel_listings(&self) -> usize {
        match self {
            Self::Local => LOCAL_LISTINGS,
            Self::Remote(files) => match files.transport() {
                Protocol::Sftp => SFTP_LISTINGS,
                transport if transport.is_ftp() => 1,
                _ => HTTP_LISTINGS,
            },
        }
    }

    /// Whether requests take turns on one connection, as on FTP, where a request given up
    /// half way would leave the next one reading its answer.
    fn take_turns(&self) -> bool {
        matches!(self, Self::Remote(files) if files.transport().is_ftp())
    }

    /// Whether the service lists everything below a folder in one go, which S3 and B2 do
    /// faster than folder by folder: they store names, not folders.
    fn lists_deep(&self) -> bool {
        match self {
            Self::Local => false,
            Self::Remote(files) => {
                matches!(files.protocol(), Protocol::S3 | Protocol::BackblazeB2)
            }
        }
    }

    /// A page of everything below `path`; `None` where this folder has to be listed by
    /// itself, such as the list of buckets.
    async fn list_deep(&self, path: &str, cursor: Option<&str>) -> AppResult<Option<DeepPage>> {
        match self {
            Self::Local => Ok(None),
            Self::Remote(files) => files.list_deep(path, cursor).await,
        }
    }
}

/// A folder waiting to be listed.
struct Folder {
    path: String,
    /// How deep its entries are: 1 for the folder the search starts in.
    depth: u32,
    links_followed: u8,
}

enum Listed {
    Folder(AppResult<DirListing>),
    /// A page of everything below the folder, with what asks for the next page.
    Deep(AppResult<Option<DeepPage>>),
}

/// Searches below `root` and reports what is found to `output`. Returns early, without an
/// error, when the search is cancelled or has found as much as it may. A folder that cannot
/// be read is reported and skipped; only the start folder has to be readable.
pub(super) async fn walk(
    folders: &Folders,
    root: &str,
    criteria: &Criteria,
    cancel: &CancellationToken,
    output: &mut Output<'_>,
) -> AppResult<()> {
    let mut reading = FuturesUnordered::new();
    let walked = walk_until_done(folders, root, criteria, cancel, output, &mut reading).await;
    // Listings still under way when the search ends early are dropped, which SFTP and HTTP
    // take well. An FTP connection would be left in the middle of a command, so there the
    // listing under way is seen to its end, without the search waiting for it.
    if folders.take_turns() && !reading.is_empty() {
        tokio::spawn(async move { while reading.next().await.is_some() {} });
    }
    walked
}

type Reading = FuturesUnordered<BoxFuture<'static, (Folder, Listed)>>;

/// Lists `folder`, or asks for a page of everything below it when `deep`.
fn read(
    folders: &Folders,
    folder: Folder,
    deep: bool,
    cursor: Option<String>,
) -> BoxFuture<'static, (Folder, Listed)> {
    let folders = folders.clone();
    Box::pin(async move {
        let listed = if deep {
            Listed::Deep(folders.list_deep(&folder.path, cursor.as_deref()).await)
        } else {
            Listed::Folder(folders.list(&folder.path).await)
        };
        (folder, listed)
    })
}

async fn walk_until_done(
    folders: &Folders,
    root: &str,
    criteria: &Criteria,
    cancel: &CancellationToken,
    output: &mut Output<'_>,
    reading: &mut Reading,
) -> AppResult<()> {
    let parallel = folders.parallel_listings();
    // One request lists a whole tree only when all of it is wanted.
    let deep = criteria.max_depth.is_none() && folders.lists_deep();
    let mut waiting = VecDeque::from([Folder {
        path: root.to_string(),
        depth: 1,
        links_followed: 0,
    }]);
    // Every folder looked at, by the path the server gives it: a link back up leads to one
    // already here.
    let mut seen: HashSet<String> = HashSet::new();
    loop {
        while reading.len() < parallel {
            match waiting.pop_front() {
                Some(folder) => reading.push(read(folders, folder, deep, None)),
                None => break,
            }
        }
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            next = reading.next() => next,
        };
        let Some((folder, listed)) = next else {
            return Ok(());
        };
        match listed {
            Listed::Deep(Ok(Some(page))) => {
                if seen.insert(folder.path.clone()) {
                    output.visit(&folder.path);
                }
                for entry in page.entries {
                    offer_deep(entry, &folder, criteria, &mut seen, output);
                }
                if let Some(cursor) = page.next {
                    reading.push(read(folders, folder, true, Some(cursor)));
                }
            }
            Listed::Deep(Ok(None)) => reading.push(read(folders, folder, false, None)),
            Listed::Folder(Ok(listing)) => {
                if !seen.insert(listing.path.clone()) {
                    continue;
                }
                output.visit(&listing.path);
                for entry in listing.entries {
                    if let Some(inside) = folder_inside(&entry, &folder, criteria) {
                        if !seen.contains(&inside.path) {
                            waiting.push_back(inside);
                        }
                    }
                    if criteria.matches(&entry) {
                        output.offer(entry);
                    }
                }
            }
            Listed::Deep(Err(error)) | Listed::Folder(Err(error)) => {
                // Only the start folder has that depth.
                if folder.depth == 1 || error.is_connection_lost() {
                    return Err(error);
                }
                output.unreadable(Failure {
                    path: folder.path,
                    message: error.message,
                });
            }
        }
        if output.is_full() {
            return Ok(());
        }
        output.report_if_due();
    }
}

/// The folder `entry` is, when the search goes into it.
fn folder_inside(entry: &FileEntry, parent: &Folder, criteria: &Criteria) -> Option<Folder> {
    let is_link = entry.kind == EntryKind::Symlink;
    let is_folder = match entry.kind {
        EntryKind::Dir => true,
        EntryKind::Symlink => criteria.follow_links && entry.link_target == Some(LinkTarget::Dir),
        EntryKind::File | EntryKind::Other => false,
    };
    let links_followed = parent.links_followed + u8::from(is_link);
    let enter = is_folder
        && (criteria.enter_hidden || !entry.hidden)
        && parent.depth < criteria.depth_limit()
        && links_followed <= MAX_LINKS_FOLLOWED;
    enter.then(|| Folder {
        path: entry.path.clone(),
        depth: parent.depth + 1,
        links_followed,
    })
}

/// Takes one entry of a listing of everything below `start`. Such a listing names files (and
/// folders somebody made by hand) by their whole path, so the folders on the way are worked
/// out from the path and each is offered once.
fn offer_deep(
    entry: FileEntry,
    start: &Folder,
    criteria: &Criteria,
    seen: &mut HashSet<String>,
    output: &mut Output<'_>,
) {
    let below = match start.path.as_str() {
        "/" => entry.path.strip_prefix('/'),
        start => entry
            .path
            .strip_prefix(start)
            .and_then(|rest| rest.strip_prefix('/')),
    };
    let Some(below) = below else {
        return;
    };
    let names: Vec<&str> = below.split('/').collect();
    if names
        .iter()
        .any(|name| name.is_empty() || *name == "." || *name == "..")
    {
        return;
    }
    let mut path = start.path.trim_end_matches('/').to_string();
    for (level, name) in names[..names.len() - 1].iter().enumerate() {
        let depth = start.depth + level as u32;
        if depth > criteria.depth_limit() {
            return;
        }
        path.push('/');
        path.push_str(name);
        if seen.insert(path.clone()) {
            output.visit(&path);
            let folder = folder_entry(&path, name);
            if criteria.matches(&folder) {
                output.offer(folder);
            }
        }
        if name.starts_with('.') && !criteria.enter_hidden {
            return;
        }
    }
    if start.depth + (names.len() - 1) as u32 > criteria.depth_limit() {
        return;
    }
    if entry.kind == EntryKind::Dir {
        if !seen.insert(entry.path.clone()) {
            return;
        }
        output.visit(&entry.path);
    }
    if criteria.matches(&entry) {
        output.offer(entry);
    }
}

fn folder_entry(path: &str, name: &str) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: path.to_string(),
        kind: EntryKind::Dir,
        link_target: None,
        size: 0,
        modified: None,
        permissions: None,
        owner: None,
        group: None,
        hidden: name.starts_with('.'),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    use super::*;
    use crate::search::criteria::tests::{entry, request};
    use crate::search::{KindFilter, SearchReport, SearchRequest, SearchSink};

    struct Found {
        /// Paths relative to the start folder, sorted.
        paths: Vec<String>,
        folders: u64,
        unreadable: Vec<String>,
        full: bool,
    }

    async fn search_with(root: &Path, request: SearchRequest, cancel: &CancellationToken) -> Found {
        let reports: Arc<Mutex<Vec<SearchReport>>> = Arc::default();
        let sink_reports = reports.clone();
        let sink: SearchSink = Arc::new(move |report| sink_reports.lock().unwrap().push(report));
        let criteria = Criteria::from_request(&request).unwrap();
        let mut output = Output::new(&sink, criteria.limit);
        let start = root.to_string_lossy().into_owned();
        walk(&Folders::Local, &start, &criteria, cancel, &mut output)
            .await
            .unwrap();
        output.report();
        let canonical = fs::canonicalize(root).unwrap();
        let reports = reports.lock().unwrap();
        let mut paths: Vec<String> = reports
            .iter()
            .flat_map(|report| &report.entries)
            .map(|entry| {
                Path::new(&entry.path)
                    .strip_prefix(&canonical)
                    .unwrap_or(Path::new(&entry.path))
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        paths.sort();
        Found {
            paths,
            folders: output.folders,
            unreadable: reports
                .iter()
                .flat_map(|report| &report.unreadable)
                .map(|failure| failure.path.clone())
                .collect(),
            full: output.is_full(),
        }
    }

    async fn search(root: &Path, request: SearchRequest) -> Found {
        search_with(root, request, &CancellationToken::new()).await
    }

    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, size: usize| {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, vec![b'x'; size]).unwrap();
        };
        write("top.log", 10);
        write("notes.txt", 2_000);
        write("logs/app.log", 300);
        write("logs/old/app.log.1", 5_000);
        write("logs/old/deep/deeper/trace.LOG", 1);
        write(".cache/hidden.log", 7);
        write("logs/.hidden.log", 8);
        fs::create_dir_all(root.path().join("empty")).unwrap();
        root
    }

    #[tokio::test]
    async fn finds_names_in_every_folder_but_hidden_ones() {
        let root = tree();
        let found = search(root.path(), request("*.log")).await;
        assert_eq!(
            found.paths,
            [
                "logs/.hidden.log",
                "logs/app.log",
                "logs/old/deep/deeper/trace.LOG",
                "top.log"
            ]
        );
        // The start folder, logs, old, deep, deeper and empty; not .cache.
        assert_eq!(found.folders, 6);
        assert!(found.unreadable.is_empty());
    }

    #[tokio::test]
    async fn hidden_folders_are_searched_when_asked() {
        let root = tree();
        let found = search(
            root.path(),
            SearchRequest {
                hidden: true,
                case_sensitive: true,
                ..request("*.log")
            },
        )
        .await;
        assert_eq!(
            found.paths,
            [
                ".cache/hidden.log",
                "logs/.hidden.log",
                "logs/app.log",
                "top.log"
            ]
        );
    }

    #[tokio::test]
    async fn depth_limits_how_far_down_the_search_goes() {
        let root = tree();
        let at_depth = |max_depth| {
            let root = root.path().to_path_buf();
            async move {
                search(
                    &root,
                    SearchRequest {
                        max_depth: Some(max_depth),
                        ..request("log")
                    },
                )
                .await
                .paths
            }
        };
        assert_eq!(at_depth(1).await, ["logs", "top.log"]);
        assert_eq!(
            at_depth(2).await,
            ["logs", "logs/.hidden.log", "logs/app.log", "top.log"]
        );
        assert_eq!(at_depth(3).await.len(), 5);
        assert_eq!(at_depth(5).await.len(), 6);
    }

    #[tokio::test]
    async fn kind_and_size_narrow_the_finds() {
        let root = tree();
        let folders = search(
            root.path(),
            SearchRequest {
                kind: KindFilter::Folders,
                ..request("")
            },
        )
        .await;
        assert_eq!(
            folders.paths,
            [
                ".cache",
                "empty",
                "logs",
                "logs/old",
                "logs/old/deep",
                "logs/old/deep/deeper"
            ]
        );
        let large = search(
            root.path(),
            SearchRequest {
                min_size: Some(300),
                max_size: Some(2_000),
                ..request("")
            },
        )
        .await;
        assert_eq!(large.paths, ["logs/app.log", "notes.txt"]);
    }

    #[tokio::test]
    async fn the_search_stops_at_the_limit() {
        let root = tree();
        let found = search(
            root.path(),
            SearchRequest {
                limit: Some(2),
                hidden: true,
                ..request("")
            },
        )
        .await;
        assert_eq!(found.paths.len(), 2);
        assert!(found.full);
    }

    #[tokio::test]
    async fn a_cancelled_search_ends_without_an_error() {
        let root = tree();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let found = search_with(root.path(), request(""), &cancel).await;
        assert!(found.paths.is_empty());
        assert_eq!(found.folders, 0);
    }

    #[tokio::test]
    async fn a_deep_tree_needs_no_deep_stack() {
        let root = tempfile::tempdir().unwrap();
        let mut path = root.path().to_path_buf();
        for level in 0..150 {
            path.push(format!("d{level}"));
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("needle.txt"), b"x").unwrap();
        let found = search(root.path(), request("needle")).await;
        assert_eq!(found.paths.len(), 1);
        assert_eq!(found.folders, 151);
    }

    #[tokio::test]
    async fn a_start_folder_that_is_not_there_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        let (sink, criteria) = (
            Arc::new(|_| {}) as SearchSink,
            Criteria::from_request(&request("")).unwrap(),
        );
        let mut output = Output::new(&sink, criteria.limit);
        let result = walk(
            &Folders::Local,
            &missing.to_string_lossy(),
            &criteria,
            &CancellationToken::new(),
            &mut output,
        )
        .await;
        assert_eq!(result.unwrap_err().kind, ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn links_are_entered_only_when_asked_and_never_twice() {
        use std::os::unix::fs::symlink;
        let root = tree();
        symlink(root.path(), root.path().join("logs/old/back-to-start")).unwrap();
        symlink(root.path().join("logs"), root.path().join("empty/to-logs")).unwrap();
        symlink("nowhere", root.path().join("dangling")).unwrap();

        let plain = search(root.path(), request("*.txt")).await;
        assert_eq!(plain.paths, ["notes.txt"]);
        assert_eq!(plain.folders, 6);

        let following = search(
            root.path(),
            SearchRequest {
                follow_links: true,
                ..request("*.txt")
            },
        )
        .await;
        assert_eq!(following.paths, ["notes.txt"], "nothing is found twice");
        assert_eq!(following.folders, 6, "no folder is listed twice");

        let links = search(
            root.path(),
            SearchRequest {
                follow_links: true,
                kind: KindFilter::Folders,
                ..request("to")
            },
        )
        .await;
        assert_eq!(links.paths, ["empty/to-logs", "logs/old/back-to-start"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_folder_that_cannot_be_read_is_reported_and_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let root = tree();
        let locked = root.path().join("logs/old");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads everything; the check means something only for ordinary users.
        let readable_anyway = fs::read_dir(&locked).is_ok();
        let found = search(root.path(), request("*.log")).await;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if readable_anyway {
            return;
        }
        assert_eq!(found.paths, ["logs/.hidden.log", "logs/app.log", "top.log"]);
        assert_eq!(found.unreadable.len(), 1);
        assert!(found.unreadable[0].ends_with("old"));
    }

    fn deep_criteria(request: SearchRequest) -> Criteria {
        Criteria::from_request(&request).unwrap()
    }

    fn offered(start: &str, criteria: &Criteria, entries: Vec<FileEntry>) -> (Vec<String>, u64) {
        let reports: Arc<Mutex<Vec<SearchReport>>> = Arc::default();
        let sink_reports = reports.clone();
        let sink: SearchSink = Arc::new(move |report| sink_reports.lock().unwrap().push(report));
        let mut output = Output::new(&sink, criteria.limit);
        let mut seen = HashSet::new();
        let start = Folder {
            path: start.to_string(),
            depth: 1,
            links_followed: 0,
        };
        for entry in entries {
            offer_deep(entry, &start, criteria, &mut seen, &mut output);
        }
        output.report();
        let paths = reports
            .lock()
            .unwrap()
            .iter()
            .flat_map(|report| &report.entries)
            .map(|entry| entry.path.clone())
            .collect();
        (paths, output.folders)
    }

    fn object(path: &str, kind: EntryKind) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            ..entry(crate::remote_path::file_name(path), kind, 5, Some(1_000))
        }
    }

    #[test]
    fn a_deep_listing_yields_the_folders_on_the_way_once() {
        let criteria = deep_criteria(request(""));
        let (paths, folders) = offered(
            "/bucket",
            &criteria,
            vec![
                object("/bucket/a/b/one.txt", EntryKind::File),
                object("/bucket/a/b/two.txt", EntryKind::File),
                object("/bucket/a/b", EntryKind::Dir),
                object("/bucket/a/made-by-hand", EntryKind::Dir),
                object("/bucket/top.txt", EntryKind::File),
            ],
        );
        assert_eq!(
            paths,
            [
                "/bucket/a",
                "/bucket/a/b",
                "/bucket/a/b/one.txt",
                "/bucket/a/b/two.txt",
                "/bucket/a/made-by-hand",
                "/bucket/top.txt"
            ]
        );
        // a, a/b and the empty folder made by hand.
        assert_eq!(folders, 3);
    }

    #[test]
    fn a_deep_listing_keeps_out_of_hidden_folders_and_other_trees() {
        let entries = || {
            vec![
                object("/bucket/data/.git/config", EntryKind::File),
                object("/bucket/data/.env", EntryKind::File),
                object("/bucket/database/x", EntryKind::File),
                object("/bucket/data/../escape", EntryKind::File),
                object("/bucket/data//nameless", EntryKind::File),
                object("/elsewhere/x", EntryKind::File),
            ]
        };
        let (paths, _) = offered("/bucket/data", &deep_criteria(request("")), entries());
        assert_eq!(paths, ["/bucket/data/.git", "/bucket/data/.env"]);

        let (paths, _) = offered(
            "/bucket/data",
            &deep_criteria(SearchRequest {
                hidden: true,
                ..request("")
            }),
            entries(),
        );
        assert_eq!(
            paths,
            [
                "/bucket/data/.git",
                "/bucket/data/.git/config",
                "/bucket/data/.env"
            ]
        );
    }

    #[test]
    fn folders_are_entered_by_kind_depth_and_links() {
        let parent = Folder {
            path: "/srv".to_string(),
            depth: 2,
            links_followed: 0,
        };
        let enters = |entry: &FileEntry, request: SearchRequest| {
            folder_inside(entry, &parent, &deep_criteria(request)).map(|folder| folder.depth)
        };
        let folder = entry("folder", EntryKind::Dir, 0, None);
        let hidden = entry(".git", EntryKind::Dir, 0, None);
        let link = FileEntry {
            link_target: Some(LinkTarget::Dir),
            ..entry("link", EntryKind::Symlink, 0, None)
        };
        assert_eq!(enters(&folder, request("")), Some(3));
        assert_eq!(
            enters(&entry("file", EntryKind::File, 1, None), request("")),
            None
        );
        assert_eq!(enters(&hidden, request("")), None);
        assert_eq!(
            enters(
                &hidden,
                SearchRequest {
                    hidden: true,
                    ..request("")
                }
            ),
            Some(3)
        );
        assert_eq!(enters(&link, request("")), None);
        assert_eq!(
            enters(
                &link,
                SearchRequest {
                    follow_links: true,
                    ..request("")
                }
            ),
            Some(3)
        );
        assert_eq!(
            enters(
                &folder,
                SearchRequest {
                    max_depth: Some(2),
                    ..request("")
                }
            ),
            None
        );

        let far = Folder {
            path: "/srv".to_string(),
            depth: 2,
            links_followed: MAX_LINKS_FOLLOWED,
        };
        let following = deep_criteria(SearchRequest {
            follow_links: true,
            ..request("")
        });
        assert!(folder_inside(&link, &far, &following).is_none());
        assert!(folder_inside(&folder, &far, &following).is_some());
    }
}
