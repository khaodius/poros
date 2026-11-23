//! The file system a mounted drive shows, over any `RemoteFileSystem`. The platform adapters
//! (FUSE, WinFsp) translate kernel requests into the calls here and nothing else.
//!
//! - Folder listings are cached for a short while and answer every lookup in their folder.
//!   A change made through the drive updates them at once; a change made elsewhere shows
//!   when the listing is read again.
//! - Reads come from blocks fetched with ranged reads and kept in a bounded cache. A file
//!   read in order is streamed ahead of the reader over one request.
//! - Most servers cannot change part of a file, so a file opened for writing is held whole
//!   in a local spool file and uploaded when the program flushes or closes it. A file opened
//!   to be changed is fetched first, at its first write.
//! - A failed upload is reported to the program where the system allows it (`close` and
//!   `fsync` wait for the upload). The spooled data is kept either way: it stays readable
//!   through the drive, is tried again, and is kept on disk with a note if the drive is
//!   unmounted before it got through.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::{Bytes, BytesMut};
use futures::future::{BoxFuture, FutureExt, Shared};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::backend::{Backend, Purpose};
use super::blocks::{BlockCache, BlockKey, BLOCK_SIZE};
use super::names::{self, FolderNames, NameStyle};
use super::spool::{Spool, UnsentNote};
use super::upload::{self, ChooseCheck, UploadCheck, UploadRequest};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{DirListing, EntryKind, FileEntry, LinkTarget};
use crate::protocol::{self, ReadStream, RemoteFileSystem};
use crate::remote_path;

pub type NodeId = u64;
pub type HandleId = u64;
pub const ROOT_NODE: NodeId = 1;

/// How long one read from the server may stall before it fails.
const READ_STEP_LIMIT: Duration = Duration::from_secs(45);
/// A stream nobody has read from for this long is closed, to free its connection.
const STREAM_IDLE: Duration = Duration::from_secs(4);
/// How often an upload is tried again by itself when the file changed while it was sent.
const SUPERSEDED_RETRIES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    AlreadyExists,
    NotAFolder,
    IsAFolder,
    NotEmpty,
    ReadOnly,
    PermissionDenied,
    InvalidName,
    /// A move between folders the server cannot do; programs then copy and delete.
    CrossFolder,
    Unsupported,
    Interrupted,
    TimedOut,
    BadHandle,
    NoSpace,
    Io(String),
}

pub type VfsResult<T> = Result<T, VfsError>;

/// Takes the path of a file whose upload failed, and the reason.
pub type FailureReport = Arc<dyn Fn(&str, &str) + Send + Sync>;

impl From<AppError> for VfsError {
    fn from(error: AppError) -> Self {
        match error.kind {
            ErrorKind::NotFound => VfsError::NotFound,
            ErrorKind::AlreadyExists => VfsError::AlreadyExists,
            ErrorKind::PermissionDenied => VfsError::PermissionDenied,
            ErrorKind::Unsupported => VfsError::Unsupported,
            ErrorKind::Cancelled => VfsError::Interrupted,
            ErrorKind::Timeout => VfsError::TimedOut,
            _ => VfsError::Io(error.message),
        }
    }
}

impl From<std::io::Error> for VfsError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::StorageFull => VfsError::NoSpace,
            _ => VfsError::Io(format!("The local copy could not be written: {error}")),
        }
    }
}

impl std::fmt::Display for VfsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VfsError::Io(message) => formatter.write_str(message),
            other => write!(formatter, "{other:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Folder,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub node: NodeId,
    pub kind: NodeKind,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenMode {
    pub read: bool,
    pub write: bool,
    pub truncate: bool,
    pub append: bool,
}

impl OpenMode {
    pub const READ: OpenMode = OpenMode {
        read: true,
        write: false,
        truncate: false,
        append: false,
    };
    pub const READ_WRITE: OpenMode = OpenMode {
        read: true,
        write: true,
        truncate: false,
        append: false,
    };
    pub const WRITE_TRUNCATE: OpenMode = OpenMode {
        read: false,
        write: true,
        truncate: true,
        append: false,
    };
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttributeChanges {
    pub size: Option<u64>,
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
}

#[derive(Clone)]
pub struct FsOptions {
    pub read_only: bool,
    pub style: NameStyle,
    /// How long a folder listing answers lookups before it is read again.
    pub metadata_lifetime: Duration,
    /// The most memory the blocks read from the server may take.
    pub memory_budget: usize,
    /// How many blocks a file read in order is streamed ahead of its reader.
    pub read_ahead_blocks: u64,
    /// The connection's name, for the notes kept with files that were not uploaded.
    pub connection: String,
    /// Where files open for writing are held.
    pub spool_directory: PathBuf,
    /// How an upload to a given server is checked.
    pub upload_check: Arc<ChooseCheck>,
}

impl FsOptions {
    pub fn new(connection: impl Into<String>, spool_directory: PathBuf) -> Self {
        Self {
            read_only: false,
            style: NameStyle::native(),
            metadata_lifetime: Duration::from_secs(10),
            memory_budget: 64 * 1024 * 1024,
            read_ahead_blocks: 4,
            connection: connection.into(),
            spool_directory,
            upload_check: Arc::new(|_| UploadCheck::Size),
        }
    }
}

/// A file whose contents are not, or not yet, on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingUpload {
    pub path: String,
    pub size: u64,
    pub sent: u64,
    pub uploading: bool,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

/// Running totals of what went over the connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Activity {
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub open_files: u32,
}

#[derive(Clone)]
struct Entry {
    kind: NodeKind,
    size: u64,
    modified: Option<i64>,
    permissions: Option<u32>,
}

struct Listing {
    fetched: Instant,
    /// By remote name.
    entries: BTreeMap<String, Entry>,
    names: FolderNames,
}

struct LocalState {
    size: u64,
    /// Counts every change. The file is on the server once `uploaded` has caught up.
    generation: u64,
    uploaded: u64,
    uploading: Option<u64>,
    /// The last generation an upload failed for, and why.
    failed: Option<(u64, String)>,
    /// The time the file is to carry on the server, once a program set one.
    modified: Option<i64>,
    /// When the file was last written here.
    touched: i64,
    permissions: Option<u32>,
}

struct LocalFile {
    spool: Spool,
    state: Mutex<LocalState>,
    /// Counts finished uploads, successful or not, to wake those waiting on one.
    settled: watch::Sender<u64>,
    sent: AtomicU64,
}

impl LocalFile {
    fn is_dirty(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.generation != state.uploaded
    }
}

struct Node {
    path: String,
    kind: NodeKind,
    /// References the kernel holds.
    lookups: u64,
    handles: u32,
    /// Removed or replaced on the server while still referenced.
    removed: bool,
    /// Bumped whenever the contents on the server may have changed.
    version: u64,
    /// The size and time the cached blocks belong to.
    seen: Option<(u64, Option<i64>)>,
    local: Option<Arc<LocalFile>>,
    /// Held while the file is fetched into its spool, so it is fetched once.
    filling: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct Nodes {
    by_id: HashMap<NodeId, Node>,
    by_path: HashMap<String, NodeId>,
}

impl Nodes {
    /// The node of a path, made if there is none. Ids follow from the path, so a folder
    /// listing can name them without creating them.
    fn intern(&mut self, path: &str, kind: NodeKind) -> NodeId {
        if let Some(id) = self.by_path.get(path) {
            return *id;
        }
        let mut id = path_id(path);
        while self.by_id.contains_key(&id) {
            id = id.wrapping_add(1).max(ROOT_NODE + 1);
        }
        self.by_id.insert(
            id,
            Node {
                path: path.to_string(),
                kind,
                lookups: 0,
                handles: 0,
                removed: false,
                version: 0,
                seen: None,
                local: None,
                filling: Arc::new(tokio::sync::Mutex::new(())),
            },
        );
        self.by_path.insert(path.to_string(), id);
        id
    }

    fn id_of(&self, path: &str) -> NodeId {
        self.by_path
            .get(path)
            .copied()
            .unwrap_or_else(|| path_id(path))
    }

    /// Drops a node nothing refers to any more.
    fn release_if_unused(&mut self, id: NodeId) {
        let unused = self.by_id.get(&id).is_some_and(|node| {
            id != ROOT_NODE && node.lookups == 0 && node.handles == 0 && node.local.is_none()
        });
        if unused {
            if let Some(node) = self.by_id.remove(&id) {
                if self.by_path.get(&node.path) == Some(&id) {
                    self.by_path.remove(&node.path);
                }
            }
        }
    }

    /// Marks the node at `path`, and for a folder those below it, as gone from the server.
    fn mark_removed(&mut self, path: &str) -> Vec<NodeId> {
        let prefix = format!("{path}/");
        let gone: Vec<String> = self
            .by_path
            .keys()
            .filter(|known| *known == path || known.starts_with(&prefix))
            .cloned()
            .collect();
        let mut ids = Vec::new();
        for known in gone {
            if let Some(id) = self.by_path.remove(&known) {
                if let Some(node) = self.by_id.get_mut(&id) {
                    node.removed = true;
                }
                ids.push(id);
            }
        }
        for id in &ids {
            self.release_if_unused(*id);
        }
        ids
    }

    /// Gives the node at `from`, and those below it, their paths under `to`.
    fn move_paths(&mut self, from: &str, to: &str) {
        let prefix = format!("{from}/");
        let moved: Vec<String> = self
            .by_path
            .keys()
            .filter(|known| *known == from || known.starts_with(&prefix))
            .cloned()
            .collect();
        for known in moved {
            let Some(id) = self.by_path.remove(&known) else {
                continue;
            };
            let target = format!("{to}{}", &known[from.len()..]);
            if let Some(node) = self.by_id.get_mut(&id) {
                node.path = target.clone();
            }
            self.by_path.insert(target, id);
        }
    }
}

/// A 64-bit FNV-1a hash of a path, never the root's id or zero.
fn path_id(path: &str) -> NodeId {
    let hash = path.bytes().fold(0xCBF2_9CE4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
    });
    hash.max(ROOT_NODE + 1)
}

struct Prefetch {
    version: u64,
    start: u64,
    /// The first block not yet stored, and whether the stream has ended.
    progress: watch::Sender<(u64, bool)>,
    /// Blocks below this may be fetched.
    wanted: watch::Sender<u64>,
    stop: CancellationToken,
}

impl Drop for Prefetch {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[derive(Default)]
struct Reader {
    /// The highest block read so far.
    last_block: Option<u64>,
    prefetch: Option<Arc<Prefetch>>,
}

struct OpenFile {
    node: NodeId,
    mode: OpenMode,
    reader: Mutex<Reader>,
}

type SharedFetch = Shared<BoxFuture<'static, VfsResult<Bytes>>>;

pub struct MountFs {
    backend: Arc<dyn Backend>,
    root: String,
    options: FsOptions,
    nodes: Mutex<Nodes>,
    handles: Mutex<HashMap<HandleId, Arc<OpenFile>>>,
    next_handle: AtomicU64,
    listings: Mutex<HashMap<String, Listing>>,
    /// One lock per folder being listed, so a folder is listed once however many ask.
    listing_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    blocks: BlockCache,
    fetches: Mutex<HashMap<BlockKey, SharedFetch>>,
    bytes_read: AtomicU64,
    bytes_written: AtomicU64,
    /// Told about uploads that failed with nobody waiting to hear of it.
    on_upload_failure: Mutex<Option<FailureReport>>,
}

impl Drop for MountFs {
    fn drop(&mut self) {
        // Spool files that were not kept go with their files, and the folder with them
        // when nothing was kept in it.
        if let Ok(handles) = self.handles.get_mut() {
            handles.clear();
        }
        if let Ok(nodes) = self.nodes.get_mut() {
            nodes.by_id.clear();
        }
        let _ = std::fs::remove_dir(&self.options.spool_directory);
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn entry_of(file: &FileEntry) -> Option<Entry> {
    let kind = match (file.kind, file.link_target) {
        (EntryKind::Dir, _) | (EntryKind::Symlink, Some(LinkTarget::Dir)) => NodeKind::Folder,
        (EntryKind::File, _) | (EntryKind::Symlink, Some(LinkTarget::File)) => NodeKind::File,
        // Links that lead nowhere, devices and sockets have nothing to read or write.
        _ => return None,
    };
    Some(Entry {
        kind,
        size: if kind == NodeKind::File { file.size } else { 0 },
        modified: file.modified,
        permissions: file.permissions.map(|mode| mode & 0o7777),
    })
}

impl MountFs {
    /// A drive showing `root` of the servers `backend` reaches.
    pub fn new(backend: Arc<dyn Backend>, root: &str, options: FsOptions) -> Arc<Self> {
        let root = protocol::normalize(root);
        let mut nodes = Nodes::default();
        nodes.by_id.insert(
            ROOT_NODE,
            Node {
                path: root.clone(),
                kind: NodeKind::Folder,
                lookups: 1,
                handles: 0,
                removed: false,
                version: 0,
                seen: None,
                local: None,
                filling: Arc::new(tokio::sync::Mutex::new(())),
            },
        );
        nodes.by_path.insert(root.clone(), ROOT_NODE);
        Arc::new(Self {
            backend,
            root,
            blocks: BlockCache::new(options.memory_budget),
            options,
            nodes: Mutex::new(nodes),
            handles: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
            listings: Mutex::new(HashMap::new()),
            listing_locks: Mutex::new(HashMap::new()),
            fetches: Mutex::new(HashMap::new()),
            bytes_read: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            on_upload_failure: Mutex::new(None),
        })
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn is_read_only(&self) -> bool {
        self.options.read_only
    }

    /// Calls `report` with a file's path and the reason when an upload fails after the
    /// program that wrote the file has gone.
    pub fn report_upload_failures(&self, report: FailureReport) {
        *self.on_upload_failure.lock().unwrap() = Some(report);
    }

    // ---- requests to the server ----

    /// Runs a request that is safe to repeat, once more if its connection was lost.
    async fn request<T, Work, Running>(&self, work: Work) -> AppResult<T>
    where
        Work: Fn(Arc<dyn RemoteFileSystem>) -> Running,
        Running: Future<Output = AppResult<T>>,
    {
        let mut retried = false;
        loop {
            let lease = self.backend.lease(Purpose::Request).await?;
            match work(lease.files.clone()).await {
                Err(error) if error.is_connection_lost() && !retried => {
                    self.backend.connection_lost();
                    retried = true;
                }
                result => return result,
            }
        }
    }

    /// Runs a request that changes the server, once.
    async fn change<T, Work, Running>(&self, work: Work) -> AppResult<T>
    where
        Work: FnOnce(Arc<dyn RemoteFileSystem>) -> Running,
        Running: Future<Output = AppResult<T>>,
    {
        let lease = self.backend.lease(Purpose::Request).await?;
        let result = work(lease.files.clone()).await;
        if result.as_ref().is_err_and(AppError::is_connection_lost) {
            self.backend.connection_lost();
        }
        result
    }

    fn writable(&self) -> VfsResult<()> {
        if self.options.read_only {
            Err(VfsError::ReadOnly)
        } else {
            Ok(())
        }
    }

    // ---- nodes and listings ----

    fn node_path(&self, id: NodeId) -> VfsResult<(String, NodeKind)> {
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.by_id.get(&id).ok_or(VfsError::NotFound)?;
        if node.removed {
            return Err(VfsError::NotFound);
        }
        Ok((node.path.clone(), node.kind))
    }

    fn folder_path(&self, id: NodeId) -> VfsResult<String> {
        match self.node_path(id)? {
            (path, NodeKind::Folder) => Ok(path),
            (_, NodeKind::File) => Err(VfsError::NotAFolder),
        }
    }

    fn local_of(&self, id: NodeId) -> Option<Arc<LocalFile>> {
        self.nodes
            .lock()
            .unwrap()
            .by_id
            .get(&id)
            .and_then(|node| node.local.clone())
    }

    fn listing_is_fresh(&self, folder: &str) -> bool {
        self.listings
            .lock()
            .unwrap()
            .get(folder)
            .is_some_and(|listing| listing.fetched.elapsed() < self.options.metadata_lifetime)
    }

    /// Makes sure the folder's listing is cached and no older than its lifetime.
    async fn ensure_listing(&self, folder: &str) -> VfsResult<()> {
        if self.listing_is_fresh(folder) {
            return Ok(());
        }
        let lock = self
            .listing_locks
            .lock()
            .unwrap()
            .entry(folder.to_string())
            .or_default()
            .clone();
        let _listing_once = lock.lock().await;
        if self.listing_is_fresh(folder) {
            return Ok(());
        }
        let listed = self
            .request(|files| async move { files.list_dir(folder).await })
            .await;
        self.listing_locks.lock().unwrap().remove(folder);
        self.store_listing(folder, listed?);
        Ok(())
    }

    fn store_listing(&self, folder: &str, listed: DirListing) {
        let mut entries = BTreeMap::new();
        for file in &listed.entries {
            if let Some(entry) = entry_of(file) {
                entries.insert(file.name.clone(), entry);
            }
        }
        let names = FolderNames::of(entries.keys().map(String::as_str), self.options.style);
        entries.retain(|remote, _| names.local(remote).is_some());
        self.listings.lock().unwrap().insert(
            folder.to_string(),
            Listing {
                fetched: Instant::now(),
                entries,
                names,
            },
        );
    }

    /// The remote name and entry behind a local name in a folder.
    async fn find(&self, folder: &str, local_name: &str) -> VfsResult<Option<(String, Entry)>> {
        self.ensure_listing(folder).await?;
        let listings = self.listings.lock().unwrap();
        let Some(listing) = listings.get(folder) else {
            return Ok(None);
        };
        let Some(remote) = listing.names.remote(local_name, self.options.style) else {
            return Ok(None);
        };
        Ok(listing
            .entries
            .get(remote)
            .map(|entry| (remote.to_string(), entry.clone())))
    }

    /// The cached entry of a remote path, reading its folder's listing if need be.
    async fn entry_at(&self, path: &str) -> VfsResult<Option<Entry>> {
        let Some(folder) = remote_path::parent(path) else {
            return Ok(None);
        };
        self.ensure_listing(&folder).await?;
        let listings = self.listings.lock().unwrap();
        Ok(listings
            .get(&folder)
            .and_then(|listing| listing.entries.get(remote_path::file_name(path)))
            .cloned())
    }

    /// Changes a folder's cached listing in place, if it is cached.
    fn update_listing(&self, folder: &str, change: impl FnOnce(&mut Listing)) {
        if let Some(listing) = self.listings.lock().unwrap().get_mut(folder) {
            change(listing);
        }
    }

    fn put_entry(&self, path: &str, local_name: Option<&str>, entry: Entry) {
        let Some(folder) = remote_path::parent(path) else {
            return;
        };
        let remote = remote_path::file_name(path).to_string();
        let style = self.options.style;
        self.update_listing(&folder, |listing| {
            if listing.names.local(&remote).is_none() {
                let local = local_name
                    .map(str::to_string)
                    .or_else(|| names::local_name(&remote, style));
                if let Some(local) = local {
                    listing.names.add(&remote, &local, style);
                }
            }
            listing.entries.insert(remote, entry);
        });
    }

    fn drop_entry(&self, path: &str) {
        let Some(folder) = remote_path::parent(path) else {
            return;
        };
        let remote = remote_path::file_name(path);
        let style = self.options.style;
        self.update_listing(&folder, |listing| {
            listing.entries.remove(remote);
            listing.names.remove(remote, style);
        });
    }

    /// Forgets the listings of a folder and of every folder below it.
    fn drop_listings_under(&self, folder: &str) {
        let prefix = format!("{folder}/");
        self.listings
            .lock()
            .unwrap()
            .retain(|known, _| known != folder && !known.starts_with(&prefix));
    }

    fn attributes_of(&self, id: NodeId, entry: &Entry) -> Attributes {
        let mut attributes = Attributes {
            node: id,
            kind: entry.kind,
            size: entry.size,
            modified: entry.modified,
            permissions: entry.permissions,
        };
        if let Some(local) = self.local_of(id) {
            let state = local.state.lock().unwrap();
            attributes.size = state.size;
            attributes.modified = Some(state.modified.unwrap_or(state.touched));
            if state.permissions.is_some() {
                attributes.permissions = state.permissions;
            }
        }
        attributes
    }

    fn root_attributes(&self) -> Attributes {
        Attributes {
            node: ROOT_NODE,
            kind: NodeKind::Folder,
            size: 0,
            modified: None,
            permissions: None,
        }
    }

    /// Finds `name` in a folder. The kernel holds a reference to the node until `forget`.
    pub async fn lookup(&self, parent: NodeId, name: &str) -> VfsResult<Attributes> {
        let folder = self.folder_path(parent)?;
        let (remote, entry) = self.find(&folder, name).await?.ok_or(VfsError::NotFound)?;
        let path = remote_path::join(&folder, &remote);
        let id = {
            let mut nodes = self.nodes.lock().unwrap();
            let id = nodes.intern(&path, entry.kind);
            if let Some(node) = nodes.by_id.get_mut(&id) {
                node.lookups += 1;
                node.kind = entry.kind;
            }
            id
        };
        Ok(self.attributes_of(id, &entry))
    }

    /// Gives back `count` references taken by `lookup`, `create` or `make_folder`.
    pub fn forget(&self, id: NodeId, count: u64) {
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.by_id.get_mut(&id) {
            node.lookups = node.lookups.saturating_sub(count);
        }
        nodes.release_if_unused(id);
    }

    pub async fn attributes(&self, id: NodeId) -> VfsResult<Attributes> {
        if id == ROOT_NODE {
            return Ok(self.root_attributes());
        }
        let (path, kind, removed, has_local) = {
            let nodes = self.nodes.lock().unwrap();
            let node = nodes.by_id.get(&id).ok_or(VfsError::NotFound)?;
            (
                node.path.clone(),
                node.kind,
                node.removed,
                node.local.is_some(),
            )
        };
        let entry = if removed {
            None
        } else {
            self.entry_at(&path).await?
        };
        match entry {
            Some(entry) => Ok(self.attributes_of(id, &entry)),
            // A file removed while a program still has it open keeps its local contents.
            None if has_local => Ok(self.attributes_of(
                id,
                &Entry {
                    kind,
                    size: 0,
                    modified: None,
                    permissions: None,
                },
            )),
            None => Err(VfsError::NotFound),
        }
    }

    /// The folder's entries under their local names, in name order.
    pub async fn read_folder(&self, id: NodeId) -> VfsResult<Vec<(String, Attributes)>> {
        let folder = self.folder_path(id)?;
        self.ensure_listing(&folder).await?;
        let listed: Vec<(String, String, Entry)> = {
            let listings = self.listings.lock().unwrap();
            let Some(listing) = listings.get(&folder) else {
                return Ok(Vec::new());
            };
            listing
                .entries
                .iter()
                .filter_map(|(remote, entry)| {
                    let local = listing.names.local(remote)?;
                    Some((local.to_string(), remote.clone(), entry.clone()))
                })
                .collect()
        };
        let mut entries: Vec<(String, Attributes)> = listed
            .into_iter()
            .map(|(local, remote, entry)| {
                let path = remote_path::join(&folder, &remote);
                let id = self.nodes.lock().unwrap().id_of(&path);
                (local, self.attributes_of(id, &entry))
            })
            .collect();
        entries.sort_by(|first, second| first.0.cmp(&second.0));
        Ok(entries)
    }

    // ---- opening and closing ----

    fn register(&self, node: NodeId, mode: OpenMode) -> HandleId {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handles.lock().unwrap().insert(
            handle,
            Arc::new(OpenFile {
                node,
                mode,
                reader: Mutex::new(Reader::default()),
            }),
        );
        if let Some(node) = self.nodes.lock().unwrap().by_id.get_mut(&node) {
            node.handles += 1;
        }
        handle
    }

    fn open_file(&self, handle: HandleId) -> VfsResult<Arc<OpenFile>> {
        self.handles
            .lock()
            .unwrap()
            .get(&handle)
            .cloned()
            .ok_or(VfsError::BadHandle)
    }

    pub async fn open(self: &Arc<Self>, id: NodeId, mode: OpenMode) -> VfsResult<HandleId> {
        let (path, kind) = self.node_path(id)?;
        if kind == NodeKind::Folder {
            return Err(VfsError::IsAFolder);
        }
        if mode.write || mode.truncate {
            self.writable()?;
        }
        let has_local = self.local_of(id).is_some();
        if !has_local {
            let entry = self.entry_at(&path).await?.ok_or(VfsError::NotFound)?;
            self.note_remote_state(id, entry.size, entry.modified);
        }
        if mode.truncate {
            self.truncate_local(id, 0).await?;
        } else if mode.read && !has_local {
            let lease = self.backend.lease(Purpose::Request).await?;
            let ranged = lease.files.capabilities().ranged_reads;
            drop(lease);
            if !ranged {
                // Without ranged reads a program could only ever read from the start.
                self.local_file(id, Fill::FromServer).await?;
            }
        }
        Ok(self.register(id, mode))
    }

    /// Remembers what the server holds for a file, forgetting its cached blocks if that
    /// differs from what they were read from.
    fn note_remote_state(&self, id: NodeId, size: u64, modified: Option<i64>) {
        let mut nodes = self.nodes.lock().unwrap();
        let Some(node) = nodes.by_id.get_mut(&id) else {
            return;
        };
        let now = Some((size, modified));
        if node.seen.is_some() && node.seen != now {
            node.version += 1;
            self.blocks.forget_file(id);
        }
        node.seen = now;
    }

    /// Creates a file and opens it for writing. With `exclusive`, an existing file is an
    /// error; otherwise it is opened, truncated if `mode` says so.
    pub async fn create(
        self: &Arc<Self>,
        parent: NodeId,
        name: &str,
        mode: OpenMode,
        exclusive: bool,
        permissions: Option<u32>,
    ) -> VfsResult<(Attributes, HandleId)> {
        self.writable()?;
        let folder = self.folder_path(parent)?;
        if self.find(&folder, name).await?.is_some() {
            if exclusive {
                return Err(VfsError::AlreadyExists);
            }
            let attributes = self.lookup(parent, name).await?;
            let handle = match self.open(attributes.node, mode).await {
                Ok(handle) => handle,
                Err(error) => {
                    self.forget(attributes.node, 1);
                    return Err(error);
                }
            };
            return Ok((self.attributes(attributes.node).await?, handle));
        }
        let remote = names::remote_name_for_new(name, self.options.style);
        protocol::validate_name(&remote).map_err(|_| VfsError::InvalidName)?;
        let path = self
            .change(|files| {
                let (folder, remote) = (folder.clone(), remote.clone());
                async move { files.make_file(&folder, &remote).await }
            })
            .await?;
        let entry = Entry {
            kind: NodeKind::File,
            size: 0,
            modified: Some(now_seconds()),
            permissions: permissions.map(|bits| bits & 0o7777),
        };
        self.put_entry(&path, Some(name), entry.clone());
        let id = {
            let mut nodes = self.nodes.lock().unwrap();
            let id = nodes.intern(&path, NodeKind::File);
            if let Some(node) = nodes.by_id.get_mut(&id) {
                node.lookups += 1;
                node.seen = Some((0, entry.modified));
            }
            id
        };
        let local = self.local_file(id, Fill::Empty).await?;
        local.state.lock().unwrap().permissions = entry.permissions;
        let handle = self.register(id, mode);
        Ok((self.attributes_of(id, &entry), handle))
    }

    /// Called for every `close` of a descriptor: sends what was written and waits for it,
    /// so the program learns of a failed upload.
    pub async fn flush(self: &Arc<Self>, handle: HandleId) -> VfsResult<()> {
        let open = self.open_file(handle)?;
        if !open.mode.write {
            return Ok(());
        }
        self.send(open.node).await
    }

    /// Called once the last descriptor of an open is closed. Whatever is still unsent is
    /// sent in the background.
    pub fn release(self: &Arc<Self>, handle: HandleId) {
        let Some(open) = self.handles.lock().unwrap().remove(&handle) else {
            return;
        };
        open.reader.lock().unwrap().prefetch = None;
        let id = open.node;
        let dirty = {
            let mut nodes = self.nodes.lock().unwrap();
            let Some(node) = nodes.by_id.get_mut(&id) else {
                return;
            };
            node.handles = node.handles.saturating_sub(1);
            node.local.as_ref().is_some_and(|local| local.is_dirty())
        };
        if dirty {
            let fs = self.clone();
            tokio::spawn(async move {
                if let Err(error) = fs.send(id).await {
                    fs.report_failure(id, &error.to_string());
                }
                fs.settle(id);
            });
        } else {
            self.settle(id);
        }
    }

    fn report_failure(&self, id: NodeId, reason: &str) {
        let report = self.on_upload_failure.lock().unwrap().clone();
        let path = self
            .nodes
            .lock()
            .unwrap()
            .by_id
            .get(&id)
            .map(|node| node.path.clone());
        if let (Some(report), Some(path)) = (report, path) {
            report(&path, reason);
        }
    }

    /// Lets go of a file's spool once nothing has it open and the server holds its contents.
    fn settle(&self, id: NodeId) {
        let mut nodes = self.nodes.lock().unwrap();
        let Some(node) = nodes.by_id.get_mut(&id) else {
            return;
        };
        let done = node.handles == 0
            && node.local.as_ref().is_some_and(|local| {
                let state = local.state.lock().unwrap();
                node.removed || (state.generation == state.uploaded && state.uploading.is_none())
            });
        if done {
            if let Some(local) = node.local.take() {
                // Removed through the drive: what it held was meant to go.
                local.spool.release();
            }
        }
        nodes.release_if_unused(id);
    }

    // ---- local contents ----

    /// The file's spool, made if it has none: empty, or filled with what the server holds.
    async fn local_file(self: &Arc<Self>, id: NodeId, fill: Fill) -> VfsResult<Arc<LocalFile>> {
        let filling = {
            let nodes = self.nodes.lock().unwrap();
            let node = nodes.by_id.get(&id).ok_or(VfsError::NotFound)?;
            if let Some(local) = &node.local {
                return Ok(local.clone());
            }
            node.filling.clone()
        };
        let _filling_once = filling.lock().await;
        if let Some(local) = self.local_of(id) {
            return Ok(local);
        }
        let (path, _) = self.node_path(id)?;
        let spool = Spool::create(&self.options.spool_directory)?;
        let mut size = 0;
        if fill == Fill::FromServer {
            let entry = self.entry_at(&path).await?.ok_or(VfsError::NotFound)?;
            if entry.size > 0 {
                size = self.download_into(&path, entry.size, &spool).await?;
            }
        }
        let (settled, _) = watch::channel(0);
        let local = Arc::new(LocalFile {
            spool,
            state: Mutex::new(LocalState {
                size,
                generation: 1,
                uploaded: 1,
                uploading: None,
                failed: None,
                modified: None,
                touched: now_seconds(),
                permissions: None,
            }),
            settled,
            sent: AtomicU64::new(0),
        });
        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.by_id.get_mut(&id).ok_or(VfsError::NotFound)?;
        node.local = Some(local.clone());
        Ok(local)
    }

    /// Fetches a whole file into a spool. Returns how many bytes arrived.
    async fn download_into(&self, path: &str, size: u64, spool: &Spool) -> VfsResult<u64> {
        let lease = self.backend.lease(Purpose::Stream).await?;
        let mut stream = read_step(lease.files.clone().open_read(path, 0..size)).await?;
        let mut offset = 0;
        let outcome: VfsResult<()> = async {
            while let Some(chunk) = read_step(stream.next_chunk()).await? {
                spool.write_at(offset, &chunk)?;
                offset += chunk.len() as u64;
                self.bytes_read
                    .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            }
            Ok(())
        }
        .await;
        match outcome {
            Ok(()) => {
                stream.finish().await?;
                Ok(offset)
            }
            Err(error) => {
                self.abandon(stream, &error).await;
                Err(error)
            }
        }
    }

    /// Ends a read that failed. A stream whose connection is in doubt is dropped, which
    /// makes its connection reopen; any other is closed in order, so the server frees it.
    async fn abandon(&self, stream: Box<dyn ReadStream>, error: &VfsError) {
        if matches!(error, VfsError::TimedOut | VfsError::Io(_)) {
            self.backend.connection_lost();
        } else {
            let _ = tokio::time::timeout(Duration::from_secs(5), stream.finish()).await;
        }
    }

    /// Sets a file's local size, fetching what the server holds first unless it all goes.
    async fn truncate_local(self: &Arc<Self>, id: NodeId, size: u64) -> VfsResult<()> {
        let fill = if size == 0 {
            Fill::Empty
        } else {
            Fill::FromServer
        };
        let had_local = self.local_of(id).is_some();
        let remote_size = match (had_local, self.node_path(id)) {
            (false, Ok((path, _))) => self.entry_at(&path).await?.map_or(0, |entry| entry.size),
            _ => 0,
        };
        let local = self.local_file(id, fill).await?;
        let first_change = {
            let mut state = local.state.lock().unwrap();
            if had_local || remote_size != size {
                local.spool.set_len(size)?;
                let first_change = state.generation == state.uploaded;
                state.size = size;
                state.generation += 1;
                state.touched = now_seconds();
                first_change
            } else {
                false
            }
        };
        if first_change {
            self.note_unsent(id, &local);
        }
        Ok(())
    }

    pub async fn write(
        self: &Arc<Self>,
        handle: HandleId,
        offset: u64,
        data: &[u8],
    ) -> VfsResult<u32> {
        self.writable()?;
        let open = self.open_file(handle)?;
        if !open.mode.write {
            return Err(VfsError::BadHandle);
        }
        let local = self.local_file(open.node, Fill::FromServer).await?;
        let first_change = {
            let mut state = local.state.lock().unwrap();
            let offset = if open.mode.append { state.size } else { offset };
            local.spool.write_at(offset, data)?;
            let first_change = state.generation == state.uploaded;
            state.size = state.size.max(offset + data.len() as u64);
            state.generation += 1;
            state.touched = now_seconds();
            first_change
        };
        if first_change {
            self.note_unsent(open.node, &local);
        }
        Ok(data.len() as u32)
    }

    /// Writes the note that says where a spool's contents belong, from their first change
    /// on, so they are found again even after a crash.
    fn note_unsent(&self, id: NodeId, local: &LocalFile) {
        let Some(remote_path) = self
            .nodes
            .lock()
            .unwrap()
            .by_id
            .get(&id)
            .map(|node| node.path.clone())
        else {
            return;
        };
        let note = {
            let state = local.state.lock().unwrap();
            UnsentNote {
                connection: self.options.connection.clone(),
                remote_path,
                size: state.size,
                error: state.failed.as_ref().map(|(_, reason)| reason.clone()),
                kept_at: now_seconds(),
            }
        };
        if let Err(error) = local.spool.note(&note) {
            log::warn!("Could not note where a file written to a drive belongs: {error}");
        }
    }

    /// Uploads the file if it has unsent changes and waits until the server holds them.
    async fn send(self: &Arc<Self>, id: NodeId) -> VfsResult<()> {
        let Some(local) = self.local_of(id) else {
            return Ok(());
        };
        let target = local.state.lock().unwrap().generation;
        // Whether an attempt this call waited for has ended. A failure from before the call
        // is no answer: the server may be back, so the upload is tried again.
        let mut waited = false;
        loop {
            let mut settled = local.settled.subscribe();
            {
                let mut state = local.state.lock().unwrap();
                if state.uploaded >= target {
                    return Ok(());
                }
                if state.uploading.is_none() {
                    match &state.failed {
                        Some((generation, reason)) if waited && *generation >= target => {
                            return Err(VfsError::Io(reason.clone()));
                        }
                        _ => {}
                    }
                    state.uploading = Some(state.generation);
                    let fs = self.clone();
                    let local = local.clone();
                    tokio::spawn(async move { fs.run_upload(id, local).await });
                }
            }
            if settled.changed().await.is_err() {
                return Err(VfsError::Io("The upload was abandoned".into()));
            }
            waited = true;
        }
    }

    /// Uploads the spool until the server holds its current contents or an attempt fails.
    async fn run_upload(self: Arc<Self>, id: NodeId, local: Arc<LocalFile>) {
        let mut attempts = 0;
        loop {
            let (generation, request) = {
                let nodes = self.nodes.lock().unwrap();
                let mut state = local.state.lock().unwrap();
                let generation = state.generation;
                state.uploading = Some(generation);
                let target = nodes
                    .by_id
                    .get(&id)
                    .filter(|node| !node.removed)
                    .map(|node| node.path.clone());
                let Some(remote_path) = target else {
                    // Removed while open: there is nowhere to send it.
                    state.uploaded = generation;
                    state.uploading = None;
                    state.failed = None;
                    drop(state);
                    local.settled.send_modify(|count| *count += 1);
                    return;
                };
                let request = UploadRequest {
                    local: local.spool.path().to_path_buf(),
                    remote_path,
                    size: state.size,
                    modified: state.modified,
                    permissions: state.permissions,
                };
                (generation, request)
            };
            local.sent.store(0, Ordering::Relaxed);
            let result = upload::upload(
                self.backend.as_ref(),
                &request,
                &local.sent,
                self.options.upload_check.as_ref(),
            )
            .await;
            let failure = match result {
                Ok(()) => None,
                Err(error) => {
                    if error.is_connection_lost() {
                        self.backend.connection_lost();
                    }
                    Some(error.message)
                }
            };
            let mut state = local.state.lock().unwrap();
            match failure {
                None => {
                    state.uploaded = generation;
                    state.uploading = None;
                    state.failed = None;
                    // Under the lock: a write after this notes its change again, and one
                    // made while the upload ran keeps its note.
                    if state.generation == generation {
                        local.spool.release();
                    }
                    let modified = state.modified.unwrap_or(state.touched);
                    let permissions = state.permissions;
                    drop(state);
                    self.bytes_written
                        .fetch_add(request.size, Ordering::Relaxed);
                    self.uploaded(
                        id,
                        &request.remote_path,
                        request.size,
                        modified,
                        permissions,
                    );
                    local.settled.send_modify(|count| *count += 1);
                    return;
                }
                Some(_) if state.generation != generation && attempts < SUPERSEDED_RETRIES => {
                    // The file changed while it was sent; what matters is its newer contents.
                    attempts += 1;
                }
                Some(reason) => {
                    state.failed = Some((generation, reason.clone()));
                    let note = UnsentNote {
                        connection: self.options.connection.clone(),
                        remote_path: request.remote_path.clone(),
                        size: state.size,
                        error: Some(reason),
                        kept_at: now_seconds(),
                    };
                    drop(state);
                    // Still marked as uploading meanwhile, so no attempt that succeeds can
                    // remove the note before it is written.
                    if let Err(error) = local.spool.keep(&note) {
                        log::warn!("Could not note an unsent file: {error}");
                    }
                    local.state.lock().unwrap().uploading = None;
                    local.settled.send_modify(|count| *count += 1);
                    return;
                }
            }
        }
    }

    /// Brings the caches in line with a file the server now holds.
    fn uploaded(&self, id: NodeId, path: &str, size: u64, modified: i64, permissions: Option<u32>) {
        let known = self
            .listings
            .lock()
            .unwrap()
            .get(&remote_path::parent(path).unwrap_or_default())
            .and_then(|listing| listing.entries.get(remote_path::file_name(path)).cloned());
        let entry = Entry {
            kind: NodeKind::File,
            size,
            modified: Some(modified),
            permissions: permissions.or(known.and_then(|entry| entry.permissions)),
        };
        self.put_entry(path, None, entry);
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.by_id.get_mut(&id) {
            node.version += 1;
            node.seen = Some((size, Some(modified)));
        }
        self.blocks.forget_file(id);
    }

    /// Waits for an upload of the file that is under way, without starting one.
    async fn wait_for_upload(&self, id: NodeId) {
        let Some(local) = self.local_of(id) else {
            return;
        };
        loop {
            let mut settled = local.settled.subscribe();
            if local.state.lock().unwrap().uploading.is_none() {
                return;
            }
            if settled.changed().await.is_err() {
                return;
            }
        }
    }

    /// Tries again every upload that failed or never ran, for files no program has open.
    pub fn retry_unsent(self: &Arc<Self>) {
        let waiting: Vec<NodeId> = {
            let nodes = self.nodes.lock().unwrap();
            nodes
                .by_id
                .iter()
                .filter(|(_, node)| {
                    node.handles == 0
                        && !node.removed
                        && node.local.as_ref().is_some_and(|local| {
                            let state = local.state.lock().unwrap();
                            state.generation != state.uploaded && state.uploading.is_none()
                        })
                })
                .map(|(id, _)| *id)
                .collect()
        };
        for id in waiting {
            let fs = self.clone();
            tokio::spawn(async move {
                if let Some(local) = fs.local_of(id) {
                    local.state.lock().unwrap().failed = None;
                }
                let _ = fs.send(id).await;
                fs.settle(id);
            });
        }
    }

    // ---- reading ----

    pub async fn read(
        self: &Arc<Self>,
        handle: HandleId,
        offset: u64,
        size: u32,
    ) -> VfsResult<Bytes> {
        let open = self.open_file(handle)?;
        if let Some(local) = self.local_of(open.node) {
            let available = local.state.lock().unwrap().size;
            if offset >= available {
                return Ok(Bytes::new());
            }
            let wanted = u64::from(size).min(available - offset) as usize;
            return Ok(Bytes::from(local.spool.read_at(offset, wanted)?));
        }
        let (path, version, file_size) = {
            let nodes = self.nodes.lock().unwrap();
            let node = nodes.by_id.get(&open.node).ok_or(VfsError::NotFound)?;
            if node.removed {
                return Err(VfsError::NotFound);
            }
            let (size, _) = node.seen.unwrap_or((0, None));
            (node.path.clone(), node.version, size)
        };
        if offset >= file_size || size == 0 {
            return Ok(Bytes::new());
        }
        let end = (offset + u64::from(size)).min(file_size);
        let mut gathered = BytesMut::with_capacity((end - offset) as usize);
        let mut position = offset;
        while position < end {
            let index = position / BLOCK_SIZE;
            let key = BlockKey {
                file: open.node,
                version,
                index,
            };
            let block = self.block(&open, &path, key, file_size).await?;
            let within = (position - index * BLOCK_SIZE) as usize;
            if within >= block.len() {
                // The file is shorter on the server than its listing said.
                break;
            }
            let take = ((end - position) as usize).min(block.len() - within);
            gathered.extend_from_slice(&block[within..within + take]);
            position += take as u64;
        }
        Ok(gathered.freeze())
    }

    /// One block of a file: from the cache, from the stream reading ahead for this handle,
    /// or fetched by itself.
    async fn block(
        self: &Arc<Self>,
        open: &OpenFile,
        path: &str,
        key: BlockKey,
        file_size: u64,
    ) -> VfsResult<Bytes> {
        if let Some(block) = self.blocks.get(key) {
            self.note_read(open, key.index);
            return Ok(block);
        }
        let ahead = self.options.read_ahead_blocks;
        let prefetch = {
            let mut reader = open.reader.lock().unwrap();
            let covering = reader.prefetch.clone().filter(|prefetch| {
                let (next, ended) = *prefetch.progress.borrow();
                prefetch.version == key.version
                    && !ended
                    && key.index >= prefetch.start
                    && key.index >= next
                    && key.index <= next + ahead
            });
            let in_order = key.index == 0
                || reader
                    .last_block
                    .is_some_and(|last| key.index == last || key.index == last + 1);
            match covering {
                Some(prefetch) => Some(prefetch),
                None if in_order && ahead > 0 => {
                    let prefetch = self.start_prefetch(path, key, file_size);
                    reader.prefetch = Some(prefetch.clone());
                    Some(prefetch)
                }
                None => None,
            }
        };
        self.note_read(open, key.index);
        if let Some(prefetch) = prefetch {
            prefetch.wanted.send_if_modified(|wanted| {
                let target = key.index + 1 + ahead;
                let raised = target > *wanted;
                if raised {
                    *wanted = target;
                }
                raised
            });
            let mut progress = prefetch.progress.subscribe();
            loop {
                if let Some(block) = self.blocks.get(key) {
                    return Ok(block);
                }
                let (next, ended) = *progress.borrow_and_update();
                if next > key.index || ended {
                    break;
                }
                if progress.changed().await.is_err() {
                    break;
                }
            }
            if let Some(block) = self.blocks.get(key) {
                return Ok(block);
            }
        }
        self.fetch_block(path, key, file_size).await
    }

    fn note_read(&self, open: &OpenFile, index: u64) {
        let mut reader = open.reader.lock().unwrap();
        reader.last_block = Some(reader.last_block.map_or(index, |last| last.max(index)));
    }

    /// Starts a stream that reads the file from `key`'s block on, storing blocks as they
    /// fill and staying a few blocks ahead of the reader.
    fn start_prefetch(
        self: &Arc<Self>,
        path: &str,
        key: BlockKey,
        file_size: u64,
    ) -> Arc<Prefetch> {
        let (progress, _) = watch::channel((key.index, false));
        let (wanted, wanted_by_reader) = watch::channel(key.index + 1);
        let prefetch = Arc::new(Prefetch {
            version: key.version,
            start: key.index,
            progress,
            wanted,
            stop: CancellationToken::new(),
        });
        let fs = self.clone();
        let path = path.to_string();
        let stop = prefetch.stop.clone();
        let progress = prefetch.progress.clone();
        tokio::spawn(async move {
            let streamed = fs
                .stream_blocks(&path, key, file_size, &progress, wanted_by_reader, &stop)
                .await;
            if let Err(error) = streamed {
                log::debug!("Reading ahead in {path} stopped: {error}");
            }
            progress.send_modify(|(_, ended)| *ended = true);
        });
        prefetch
    }

    async fn stream_blocks(
        &self,
        path: &str,
        first: BlockKey,
        file_size: u64,
        progress: &watch::Sender<(u64, bool)>,
        mut wanted: watch::Receiver<u64>,
        stop: &CancellationToken,
    ) -> VfsResult<()> {
        let Some(lease) = self.backend.try_lease(Purpose::Stream).await else {
            // Every stream is taken; the reader fetches its blocks one by one meanwhile.
            return Ok(());
        };
        let lease = lease?;
        let start = first.index * BLOCK_SIZE;
        let mut stream = read_step(lease.files.clone().open_read(path, start..file_size)).await?;
        let mut index = first.index;
        let mut filling = BytesMut::with_capacity(BLOCK_SIZE as usize);
        let outcome: VfsResult<bool> = async {
            loop {
                while index >= *wanted.borrow_and_update() {
                    let asked = tokio::select! {
                        _ = stop.cancelled() => return Ok(false),
                        asked = tokio::time::timeout(STREAM_IDLE, wanted.changed()) => asked,
                    };
                    // Otherwise the reader stopped or went elsewhere.
                    if !matches!(asked, Ok(Ok(()))) {
                        return Ok(false);
                    }
                }
                // The stream is only ever left between chunks, so it can be closed in order.
                let chunk = tokio::select! {
                    _ = stop.cancelled() => return Ok(false),
                    chunk = read_step(stream.next_chunk()) => chunk?,
                };
                let Some(chunk) = chunk else {
                    return Ok(true);
                };
                self.bytes_read
                    .fetch_add(chunk.len() as u64, Ordering::Relaxed);
                filling.extend_from_slice(&chunk);
                while filling.len() as u64 >= BLOCK_SIZE {
                    let block = filling.split_to(BLOCK_SIZE as usize).freeze();
                    self.blocks.insert(BlockKey { index, ..first }, block);
                    index += 1;
                    progress.send_modify(|(next, _)| *next = index);
                }
            }
        }
        .await;
        match outcome {
            Ok(reached_end) => {
                if reached_end && !filling.is_empty() {
                    self.blocks
                        .insert(BlockKey { index, ..first }, filling.freeze());
                    progress.send_modify(|(next, _)| *next = index + 1);
                }
                stream.finish().await?;
                Ok(())
            }
            Err(error) => {
                self.abandon(stream, &error).await;
                Err(error)
            }
        }
    }

    /// Fetches one block with a ranged read. Readers asking for the same block share one
    /// request, which carries on even if the first of them gives up.
    async fn fetch_block(
        self: &Arc<Self>,
        path: &str,
        key: BlockKey,
        file_size: u64,
    ) -> VfsResult<Bytes> {
        let fetch = {
            let mut fetches = self.fetches.lock().unwrap();
            match fetches.get(&key) {
                Some(fetch) => fetch.clone(),
                None => {
                    let fs = self.clone();
                    let path = path.to_string();
                    let task = tokio::spawn(async move {
                        let fetched = fs.read_range(&path, key, file_size).await;
                        if let Ok(block) = &fetched {
                            fs.blocks.insert(key, block.clone());
                        }
                        fs.fetches.lock().unwrap().remove(&key);
                        fetched
                    });
                    let fetch: SharedFetch = async move {
                        task.await.unwrap_or_else(|_| {
                            Err(VfsError::Io("Reading from the server was abandoned".into()))
                        })
                    }
                    .boxed()
                    .shared();
                    fetches.insert(key, fetch.clone());
                    fetch
                }
            }
        };
        fetch.await
    }

    async fn read_range(&self, path: &str, key: BlockKey, file_size: u64) -> VfsResult<Bytes> {
        let start = key.index * BLOCK_SIZE;
        let end = (start + BLOCK_SIZE).min(file_size);
        let block = self
            .request(|files| async move {
                let mut stream = step_or_timeout(files.open_read(path, start..end)).await?;
                let mut data = BytesMut::with_capacity((end - start) as usize);
                let read = async {
                    while (data.len() as u64) < end - start {
                        match step_or_timeout(stream.next_chunk()).await? {
                            Some(chunk) => data.extend_from_slice(&chunk),
                            None => break,
                        }
                    }
                    Ok(())
                }
                .await;
                match read {
                    Ok(()) => {
                        stream.finish().await?;
                        data.truncate((end - start) as usize);
                        Ok(data.freeze())
                    }
                    Err(error) => Err(error),
                }
            })
            .await?;
        self.bytes_read
            .fetch_add(block.len() as u64, Ordering::Relaxed);
        Ok(block)
    }

    // ---- changes ----

    pub async fn make_folder(&self, parent: NodeId, name: &str) -> VfsResult<Attributes> {
        self.writable()?;
        let folder = self.folder_path(parent)?;
        if self.find(&folder, name).await?.is_some() {
            return Err(VfsError::AlreadyExists);
        }
        let remote = names::remote_name_for_new(name, self.options.style);
        protocol::validate_name(&remote).map_err(|_| VfsError::InvalidName)?;
        let path = self
            .change(|files| {
                let (folder, remote) = (folder.clone(), remote.clone());
                async move { files.make_dir(&folder, &remote).await }
            })
            .await?;
        let entry = Entry {
            kind: NodeKind::Folder,
            size: 0,
            modified: Some(now_seconds()),
            permissions: None,
        };
        self.put_entry(&path, Some(name), entry.clone());
        self.drop_listings_under(&path);
        let mut nodes = self.nodes.lock().unwrap();
        let id = nodes.intern(&path, NodeKind::Folder);
        if let Some(node) = nodes.by_id.get_mut(&id) {
            node.lookups += 1;
        }
        drop(nodes);
        Ok(self.attributes_of(id, &entry))
    }

    pub async fn remove_file(&self, parent: NodeId, name: &str) -> VfsResult<()> {
        self.writable()?;
        let folder = self.folder_path(parent)?;
        let (remote, entry) = self.find(&folder, name).await?.ok_or(VfsError::NotFound)?;
        if entry.kind == NodeKind::Folder {
            return Err(VfsError::IsAFolder);
        }
        let path = remote_path::join(&folder, &remote);
        let known = self.nodes.lock().unwrap().by_path.get(&path).copied();
        if let Some(id) = known {
            // An upload that finished after the removal would bring the file back.
            self.wait_for_upload(id).await;
        }
        self.delete(&path).await
    }

    pub async fn remove_folder(&self, parent: NodeId, name: &str) -> VfsResult<()> {
        self.writable()?;
        let folder = self.folder_path(parent)?;
        let (remote, entry) = self.find(&folder, name).await?.ok_or(VfsError::NotFound)?;
        if entry.kind == NodeKind::File {
            return Err(VfsError::NotAFolder);
        }
        let path = remote_path::join(&folder, &remote);
        // The server removes a folder with everything in it, so whether it is empty is
        // asked first, of the server and not of the cache.
        let listed = self
            .request(|files| {
                let path = path.clone();
                async move { files.list_dir(&path).await }
            })
            .await?;
        if !listed.entries.is_empty() {
            self.store_listing(&path, listed);
            return Err(VfsError::NotEmpty);
        }
        self.delete(&path).await
    }

    async fn delete(&self, path: &str) -> VfsResult<()> {
        let target = vec![path.to_string()];
        self.change(|files| async move { files.delete(&target).await })
            .await?;
        self.forget_path(path);
        Ok(())
    }

    /// Drops everything cached about a path that is gone from the server.
    fn forget_path(&self, path: &str) {
        self.drop_entry(path);
        self.drop_listings_under(path);
        let gone = self.nodes.lock().unwrap().mark_removed(path);
        for id in gone {
            self.blocks.forget_file(id);
            self.settle(id);
        }
    }

    /// Moves or renames. With `replace`, a file or empty folder already at the target gives
    /// way; without, it is an error.
    pub async fn rename(
        self: &Arc<Self>,
        parent: NodeId,
        name: &str,
        new_parent: NodeId,
        new_name: &str,
        replace: bool,
    ) -> VfsResult<()> {
        self.writable()?;
        let source_folder = self.folder_path(parent)?;
        let target_folder = self.folder_path(new_parent)?;
        let (source_remote, source) = self
            .find(&source_folder, name)
            .await?
            .ok_or(VfsError::NotFound)?;
        let existing = self.find(&target_folder, new_name).await?;
        let source_path = remote_path::join(&source_folder, &source_remote);
        let target_remote = match &existing {
            Some((remote, _)) => remote.clone(),
            None => names::remote_name_for_new(new_name, self.options.style),
        };
        protocol::validate_name(&target_remote).map_err(|_| VfsError::InvalidName)?;
        let target_path = remote_path::join(&target_folder, &target_remote);
        if source_path == target_path {
            return Ok(());
        }
        if source.kind == NodeKind::Folder && target_path.starts_with(&format!("{source_path}/")) {
            return Err(VfsError::InvalidName);
        }
        if let Some((_, target)) = &existing {
            if !replace {
                return Err(VfsError::AlreadyExists);
            }
            match (source.kind, target.kind) {
                (NodeKind::File, NodeKind::Folder) => return Err(VfsError::IsAFolder),
                (NodeKind::Folder, NodeKind::File) => return Err(VfsError::NotAFolder),
                (NodeKind::Folder, NodeKind::Folder) => {
                    let listed = self
                        .request(|files| {
                            let path = target_path.clone();
                            async move { files.list_dir(&path).await }
                        })
                        .await?;
                    if !listed.entries.is_empty() {
                        return Err(VfsError::NotEmpty);
                    }
                }
                (NodeKind::File, NodeKind::File) => {}
            }
        }
        // Uploads under way finish first, so none lands on a path that has moved on.
        for path in [&source_path, &target_path] {
            let known = self.nodes.lock().unwrap().by_path.get(path).copied();
            if let Some(id) = known {
                self.wait_for_upload(id).await;
            }
        }

        let same_folder = source_folder == target_folder;
        let moved = self
            .change(|files| {
                let (from, to) = (source_path.clone(), target_path.clone());
                async move { files.move_path(&from, &to).await }
            })
            .await;
        match moved {
            Ok(()) => {}
            Err(error) if error.kind == ErrorKind::Unsupported && same_folder => {
                if existing.is_some() {
                    let target = vec![target_path.clone()];
                    self.change(|files| async move { files.delete(&target).await })
                        .await?;
                }
                self.change(|files| {
                    let (from, name) = (source_path.clone(), target_remote.clone());
                    async move { files.rename(&from, &name).await }
                })
                .await?;
            }
            Err(error) if error.kind == ErrorKind::Unsupported => {
                return Err(VfsError::CrossFolder)
            }
            Err(error) => return Err(error.into()),
        }

        if existing.is_some() {
            self.forget_path(&target_path);
        }
        self.drop_entry(&source_path);
        self.drop_listings_under(&source_path);
        self.nodes
            .lock()
            .unwrap()
            .move_paths(&source_path, &target_path);
        self.put_entry(&target_path, Some(new_name), source);
        for (id, local) in self.unsent_under(&target_path) {
            self.note_unsent(id, &local);
        }
        Ok(())
    }

    /// The files at or below `path` whose changes are not on the server yet.
    fn unsent_under(&self, path: &str) -> Vec<(NodeId, Arc<LocalFile>)> {
        let prefix = format!("{path}/");
        self.nodes
            .lock()
            .unwrap()
            .by_id
            .iter()
            .filter(|(_, node)| node.path == path || node.path.starts_with(&prefix))
            .filter_map(|(id, node)| {
                let local = node.local.as_ref().filter(|local| local.is_dirty())?;
                Some((*id, local.clone()))
            })
            .collect()
    }

    /// Truncates or extends a file, sets its time, or its permissions where the server has
    /// them. Changes a server refuses for want of support are let pass.
    pub async fn set_attributes(
        self: &Arc<Self>,
        id: NodeId,
        changes: AttributeChanges,
    ) -> VfsResult<Attributes> {
        if changes == AttributeChanges::default() {
            return self.attributes(id).await;
        }
        self.writable()?;
        let (path, kind) = self.node_path(id)?;
        if let Some(size) = changes.size {
            if kind == NodeKind::Folder {
                return Err(VfsError::IsAFolder);
            }
            let open = self
                .nodes
                .lock()
                .unwrap()
                .by_id
                .get(&id)
                .is_some_and(|node| node.handles > 0);
            self.truncate_local(id, size).await?;
            if !open {
                // No close will follow to send it.
                let sent = self.send(id).await;
                self.settle(id);
                sent?;
            }
        }
        let local = self.local_of(id);
        if let Some(local) = &local {
            let mut state = local.state.lock().unwrap();
            if let Some(modified) = changes.modified {
                state.modified = Some(modified);
            }
            if let Some(permissions) = changes.permissions {
                state.permissions = Some(permissions & 0o7777);
            }
        }
        let unsent = local.as_ref().is_some_and(|local| local.is_dirty());
        let modified = changes.modified.filter(|_| !unsent);
        let permissions = changes.permissions.map(|bits| bits & 0o7777);
        if modified.is_some() || permissions.is_some() {
            let set = self
                .change(|files| {
                    let path = path.clone();
                    async move { files.set_attributes(&path, modified, permissions).await }
                })
                .await;
            match set {
                Ok(()) => {}
                Err(error) if error.kind == ErrorKind::PermissionDenied => {
                    return Err(VfsError::PermissionDenied)
                }
                Err(error) if error.is_connection_lost() => return Err(error.into()),
                // Many servers keep no permissions and take no times.
                Err(_) => {}
            }
        }
        if id != ROOT_NODE {
            if let Some(folder) = remote_path::parent(&path) {
                let remote = remote_path::file_name(&path).to_string();
                self.update_listing(&folder, |listing| {
                    if let Some(entry) = listing.entries.get_mut(&remote) {
                        if let Some(modified) = changes.modified {
                            entry.modified = Some(modified);
                        }
                        if let Some(permissions) = permissions {
                            entry.permissions = Some(permissions);
                        }
                    }
                });
            }
            // The blocks read so far stay valid: only the time changed.
            if let Some(modified) = changes.modified {
                let mut nodes = self.nodes.lock().unwrap();
                if let Some(node) = nodes.by_id.get_mut(&id) {
                    if let Some((size, _)) = node.seen {
                        node.seen = Some((size, Some(modified)));
                    }
                }
            }
        }
        self.attributes(id).await
    }

    // ---- state for the app ----

    pub fn activity(&self) -> Activity {
        let open_files = self
            .nodes
            .lock()
            .unwrap()
            .by_id
            .values()
            .filter(|node| node.handles > 0)
            .count() as u32;
        Activity {
            bytes_read: self.bytes_read.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            open_files,
        }
    }

    /// The remote paths of the files programs have open.
    pub fn open_paths(&self) -> Vec<String> {
        let nodes = self.nodes.lock().unwrap();
        let mut paths: Vec<String> = nodes
            .by_id
            .values()
            .filter(|node| node.handles > 0)
            .map(|node| node.path.clone())
            .collect();
        paths.sort();
        paths
    }

    /// The files whose contents the server does not hold yet.
    pub fn pending_uploads(&self) -> Vec<PendingUpload> {
        let nodes = self.nodes.lock().unwrap();
        let mut pending: Vec<PendingUpload> = nodes
            .by_id
            .values()
            .filter(|node| !node.removed)
            .filter_map(|node| {
                let local = node.local.as_ref()?;
                let state = local.state.lock().unwrap();
                if state.generation == state.uploaded && state.uploading.is_none() {
                    return None;
                }
                Some(PendingUpload {
                    path: node.path.clone(),
                    size: state.size,
                    sent: local.sent.load(Ordering::Relaxed).min(state.size),
                    uploading: state.uploading.is_some(),
                    error: state.failed.as_ref().map(|(_, reason)| reason.clone()),
                })
            })
            .collect();
        pending.sort_by(|first, second| first.path.cmp(&second.path));
        pending
    }

    /// Keeps every unsent file on disk with a note of where it belongs, for a drive that is
    /// unmounted regardless. Returns the remote paths of the files kept.
    pub fn keep_unsent(&self) -> Vec<String> {
        let nodes = self.nodes.lock().unwrap();
        let mut kept = Vec::new();
        for node in nodes.by_id.values().filter(|node| !node.removed) {
            let Some(local) = &node.local else {
                continue;
            };
            let state = local.state.lock().unwrap();
            if state.generation == state.uploaded {
                continue;
            }
            let note = UnsentNote {
                connection: self.options.connection.clone(),
                remote_path: node.path.clone(),
                size: state.size,
                error: state.failed.as_ref().map(|(_, reason)| reason.clone()),
                kept_at: now_seconds(),
            };
            match local.spool.keep(&note) {
                Ok(()) => kept.push(node.path.clone()),
                Err(error) => log::error!(
                    "Could not keep the unsent contents of {}: {error}",
                    node.path
                ),
            }
        }
        kept.sort();
        kept
    }

    /// Forgets every cached listing and block, so the next request asks the server.
    pub fn forget_cached(&self) {
        self.listings.lock().unwrap().clear();
        let ids: Vec<NodeId> = self.nodes.lock().unwrap().by_id.keys().copied().collect();
        for id in ids {
            self.blocks.forget_file(id);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    Empty,
    FromServer,
}

async fn step_or_timeout<T>(work: impl Future<Output = AppResult<T>>) -> AppResult<T> {
    match tokio::time::timeout(READ_STEP_LIMIT, work).await {
        Ok(result) => result,
        Err(_) => Err(AppError::new(
            ErrorKind::Timeout,
            "The server stopped answering",
        )),
    }
}

async fn read_step<T>(work: impl Future<Output = AppResult<T>>) -> VfsResult<T> {
    step_or_timeout(work).await.map_err(VfsError::from)
}
