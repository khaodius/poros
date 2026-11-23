//! A server held in memory, for testing the mounted drive without a network: it counts
//! requests, can fail them on demand, and can stand in for servers that lack ranged reads
//! or moves between folders.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{DirListing, EntryKind, FileEntry};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;

const READ_CHUNK: usize = 64 * 1024;

#[derive(Clone)]
enum Item {
    Folder,
    File {
        data: Bytes,
        modified: i64,
        permissions: Option<u32>,
    },
}

#[derive(Default)]
pub struct Counters {
    pub listings: AtomicU64,
    pub read_requests: AtomicU64,
    pub bytes_read: AtomicU64,
    pub uploads: AtomicU64,
}

pub struct MemoryFs {
    items: Mutex<BTreeMap<String, Item>>,
    pub counters: Counters,
    /// The number of coming uploads that fail when they finish.
    pub failing_uploads: AtomicU32,
    /// Every request fails as on a lost connection while set.
    pub offline: AtomicBool,
    /// Uploads store one byte less than they were sent while set.
    pub short_uploads: AtomicBool,
    pub ranged_reads: AtomicBool,
    pub moves_between_folders: AtomicBool,
    /// A pause before every chunk of a read, in milliseconds.
    pub read_delay_millis: AtomicU64,
    /// A pause before an upload finishes, in milliseconds.
    pub upload_delay_millis: AtomicU64,
    clock: AtomicU64,
}

impl MemoryFs {
    pub fn new() -> Arc<Self> {
        let mut items = BTreeMap::new();
        items.insert("/".to_string(), Item::Folder);
        Arc::new(Self {
            items: Mutex::new(items),
            counters: Counters::default(),
            failing_uploads: AtomicU32::new(0),
            offline: AtomicBool::new(false),
            short_uploads: AtomicBool::new(false),
            ranged_reads: AtomicBool::new(true),
            moves_between_folders: AtomicBool::new(true),
            read_delay_millis: AtomicU64::new(0),
            upload_delay_millis: AtomicU64::new(0),
            clock: AtomicU64::new(1_700_000_000),
        })
    }

    fn tick(&self) -> i64 {
        self.clock.fetch_add(1, Ordering::Relaxed) as i64
    }

    fn check_online(&self) -> AppResult<()> {
        if self.offline.load(Ordering::Relaxed) {
            Err(AppError::new(
                ErrorKind::Disconnected,
                "The connection was closed",
            ))
        } else {
            Ok(())
        }
    }

    pub fn put_folder(&self, path: &str) {
        let mut items = self.items.lock().unwrap();
        let mut current = String::new();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            current = format!("{current}/{part}");
            items.entry(current.clone()).or_insert(Item::Folder);
        }
    }

    /// Stores a file as another client of the server would, behind the drive's back.
    pub fn put_file(&self, path: &str, data: impl Into<Bytes>) {
        if let Some(parent) = remote_path::parent(path) {
            self.put_folder(&parent);
        }
        let modified = self.tick();
        self.items.lock().unwrap().insert(
            path.to_string(),
            Item::File {
                data: data.into(),
                modified,
                permissions: Some(0o644),
            },
        );
    }

    pub fn file(&self, path: &str) -> Option<Bytes> {
        match self.items.lock().unwrap().get(path) {
            Some(Item::File { data, .. }) => Some(data.clone()),
            _ => None,
        }
    }

    pub fn modified(&self, path: &str) -> Option<i64> {
        match self.items.lock().unwrap().get(path) {
            Some(Item::File { modified, .. }) => Some(*modified),
            _ => None,
        }
    }

    pub fn permissions(&self, path: &str) -> Option<u32> {
        match self.items.lock().unwrap().get(path) {
            Some(Item::File { permissions, .. }) => *permissions,
            _ => None,
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        self.items.lock().unwrap().contains_key(path)
    }

    fn children(items: &BTreeMap<String, Item>, folder: &str) -> Vec<(String, Item)> {
        let prefix = if folder == "/" {
            "/".to_string()
        } else {
            format!("{folder}/")
        };
        items
            .iter()
            .filter(|(path, _)| {
                path.starts_with(&prefix)
                    && path.len() > prefix.len()
                    && !path[prefix.len()..].contains('/')
            })
            .map(|(path, item)| (path.clone(), item.clone()))
            .collect()
    }
}

fn not_found(path: &str) -> AppError {
    AppError::new(ErrorKind::NotFound, format!("{path} does not exist")).with_path(path)
}

#[async_trait]
impl RemoteFileSystem for MemoryFs {
    fn protocol(&self) -> Protocol {
        Protocol::Sftp
    }

    fn home(&self) -> &str {
        "/"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            ranged_reads: self.ranged_reads.load(Ordering::Relaxed),
            resumable_writes: false,
        }
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        Ok(protocol::normalize(path))
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        self.check_online()?;
        self.counters.listings.fetch_add(1, Ordering::Relaxed);
        let path = protocol::normalize(path);
        let items = self.items.lock().unwrap();
        match items.get(&path) {
            Some(Item::Folder) => {}
            Some(Item::File { .. }) => return Err(AppError::invalid("Not a folder")),
            None => return Err(not_found(&path)),
        }
        let entries = Self::children(&items, &path)
            .into_iter()
            .map(|(child, item)| {
                let name = remote_path::file_name(&child).to_string();
                let (kind, size, modified, permissions) = match item {
                    Item::Folder => (EntryKind::Dir, 0, None, Some(0o755)),
                    Item::File {
                        data,
                        modified,
                        permissions,
                    } => (
                        EntryKind::File,
                        data.len() as u64,
                        Some(modified),
                        permissions,
                    ),
                };
                FileEntry {
                    hidden: name.starts_with('.'),
                    name,
                    path: child,
                    kind,
                    link_target: None,
                    size,
                    modified,
                    permissions,
                    owner: None,
                    group: None,
                }
            })
            .collect();
        Ok(DirListing {
            parent: remote_path::parent(&path),
            path,
            entries,
        })
    }

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        self.check_online()?;
        protocol::validate_name(name)?;
        let path = remote_path::join(&protocol::normalize(parent), name);
        let mut items = self.items.lock().unwrap();
        if items.contains_key(&path) {
            return Err(protocol::already_exists(&path));
        }
        items.insert(path.clone(), Item::Folder);
        Ok(path)
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        self.check_online()?;
        protocol::validate_name(new_name)?;
        let parent = remote_path::parent(path).ok_or_else(|| AppError::invalid("No parent"))?;
        let target = remote_path::join(&parent, new_name);
        if self.exists(&target) {
            return Err(protocol::already_exists(&target));
        }
        self.relocate(path, &target)?;
        Ok(target)
    }

    async fn move_path(&self, path: &str, target: &str) -> AppResult<()> {
        self.check_online()?;
        if !self.moves_between_folders.load(Ordering::Relaxed) {
            return Err(AppError::unsupported("This server only renames in place"));
        }
        self.relocate(path, target)
    }

    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        self.check_online()?;
        let mut items = self.items.lock().unwrap();
        for path in paths {
            if !items.contains_key(path) {
                return Err(not_found(path));
            }
            let prefix = format!("{path}/");
            items.retain(|known, _| known != path && !known.starts_with(&prefix));
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        self.check_online()?;
        Ok(match self.items.lock().unwrap().get(path) {
            Some(Item::Folder) => Some(RemoteStat {
                is_dir: true,
                size: 0,
                modified: None,
                permissions: Some(0o755),
            }),
            Some(Item::File {
                data,
                modified,
                permissions,
            }) => Some(RemoteStat {
                is_dir: false,
                size: data.len() as u64,
                modified: Some(*modified),
                permissions: *permissions,
            }),
            None => None,
        })
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        self.check_online()?;
        self.put_folder(path);
        Ok(())
    }

    async fn set_attributes(
        &self,
        path: &str,
        new_modified: Option<i64>,
        new_permissions: Option<u32>,
    ) -> AppResult<()> {
        self.check_online()?;
        match self.items.lock().unwrap().get_mut(path) {
            Some(Item::File {
                modified,
                permissions,
                ..
            }) => {
                if let Some(new_modified) = new_modified {
                    *modified = new_modified;
                }
                if new_permissions.is_some() {
                    *permissions = new_permissions;
                }
                Ok(())
            }
            Some(Item::Folder) => Ok(()),
            None => Err(not_found(path)),
        }
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        self.check_online()?;
        self.counters.read_requests.fetch_add(1, Ordering::Relaxed);
        if range.start > 0 && !self.ranged_reads.load(Ordering::Relaxed) {
            return Err(AppError::unsupported(
                "This server reads from the start only",
            ));
        }
        let data = self.file(path).ok_or_else(|| not_found(path))?;
        let start = (range.start as usize).min(data.len());
        let end = (range.end as usize).min(data.len()).max(start);
        Ok(Box::new(MemoryReader {
            fs: self,
            data: data.slice(start..end),
        }))
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        self.check_online()?;
        Ok(Box::new(MemoryWriter {
            fs: self,
            request,
            data: Vec::new(),
        }))
    }
}

impl MemoryFs {
    fn relocate(&self, path: &str, target: &str) -> AppResult<()> {
        let mut items = self.items.lock().unwrap();
        if !items.contains_key(path) {
            return Err(not_found(path));
        }
        let prefix = format!("{path}/");
        let moving: Vec<String> = items
            .keys()
            .filter(|known| *known == path || known.starts_with(&prefix))
            .cloned()
            .collect();
        for known in moving {
            if let Some(item) = items.remove(&known) {
                items.insert(format!("{target}{}", &known[path.len()..]), item);
            }
        }
        Ok(())
    }
}

struct MemoryReader {
    fs: Arc<MemoryFs>,
    data: Bytes,
}

#[async_trait]
impl ReadStream for MemoryReader {
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
        self.fs.check_online()?;
        let delay = self.fs.read_delay_millis.load(Ordering::Relaxed);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        if self.data.is_empty() {
            return Ok(None);
        }
        let chunk = self.data.split_to(READ_CHUNK.min(self.data.len()));
        self.fs
            .counters
            .bytes_read
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        Ok(Some(chunk))
    }

    async fn finish(self: Box<Self>) -> AppResult<()> {
        Ok(())
    }
}

struct MemoryWriter {
    fs: Arc<MemoryFs>,
    request: WriteRequest,
    data: Vec<u8>,
}

#[async_trait]
impl WriteStream for MemoryWriter {
    async fn write(&mut self, data: Bytes) -> AppResult<()> {
        self.fs.check_online()?;
        self.data.extend_from_slice(&data);
        Ok(())
    }

    async fn finish(mut self: Box<Self>) -> AppResult<()> {
        let delay = self.fs.upload_delay_millis.load(Ordering::Relaxed);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        self.fs.check_online()?;
        let failing = self
            .fs
            .failing_uploads
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if failing {
            return Err(AppError::new(
                ErrorKind::Sftp,
                "The server refused the upload",
            ));
        }
        if self.fs.short_uploads.load(Ordering::Relaxed) {
            self.data.pop();
        }
        self.fs.counters.uploads.fetch_add(1, Ordering::Relaxed);
        let modified = self.request.modified.unwrap_or_else(|| self.fs.tick());
        let mut items = self.fs.items.lock().unwrap();
        let kept_permissions = match items.get(&self.request.path) {
            Some(Item::File { permissions, .. }) => *permissions,
            _ => None,
        };
        items.insert(
            self.request.path.clone(),
            Item::File {
                data: Bytes::from(std::mem::take(&mut self.data)),
                modified,
                permissions: self.request.permissions.or(kept_permissions),
            },
        );
        Ok(())
    }
}
