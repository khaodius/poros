//! A server held in memory, for testing the mounted drive without a network: it counts
//! requests, can fail them on demand, and can stand in for servers that lack ranged reads
//! or moves between folders.

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{DirListing, EntryKind, FileEntry, LinkTarget};
use crate::protocol::{
    self, Capabilities, LinkTargets, Protocol, ReadStream, RemoteFileSystem, RemoteStat,
    WriteRequest, WriteStream,
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
    /// A symbolic link, with its target as the server keeps it.
    Link {
        target: String,
    },
}

#[derive(Default)]
pub struct Counters {
    pub listings: AtomicU64,
    pub read_requests: AtomicU64,
    pub bytes_read: AtomicU64,
    pub uploads: AtomicU64,
    /// Links asked for one at a time, rather than read from a listing.
    pub link_reads: AtomicU64,
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
    /// A pause before every piece of an upload, in milliseconds.
    pub write_delay_millis: AtomicU64,
    /// No new file may be written while set, as in a folder that takes no new files; the
    /// files there may still be.
    pub refused_new_files: AtomicBool,
    /// Listings say where links lead while set, as FTP listings do.
    pub links_in_listings: AtomicBool,
    /// A pause before every delete, in milliseconds.
    pub delete_delay_millis: AtomicU64,
    /// Paths nothing may be moved or renamed onto.
    refused_move_targets: Mutex<HashSet<String>>,
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
            write_delay_millis: AtomicU64::new(0),
            refused_new_files: AtomicBool::new(false),
            links_in_listings: AtomicBool::new(false),
            delete_delay_millis: AtomicU64::new(0),
            refused_move_targets: Mutex::new(HashSet::new()),
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

    /// Refuses, from now on, every move or rename onto `path`.
    pub fn refuse_moves_onto(&self, path: &str) {
        self.refused_move_targets
            .lock()
            .unwrap()
            .insert(path.to_string());
    }

    pub fn allow_every_move(&self) {
        self.refused_move_targets.lock().unwrap().clear();
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

    /// Stores a symbolic link leading to `target`, a full path or one relative to its folder.
    pub fn put_link(&self, path: &str, target: &str) {
        if let Some(parent) = remote_path::parent(path) {
            self.put_folder(&parent);
        }
        self.items.lock().unwrap().insert(
            path.to_string(),
            Item::Link {
                target: target.to_string(),
            },
        );
    }

    /// What a link leads to, following further links a few times as a server would.
    fn followed(items: &BTreeMap<String, Item>, path: &str) -> Option<(String, Item)> {
        let mut current = path.to_string();
        for _ in 0..8 {
            match items.get(&current)? {
                Item::Link { target } => {
                    let folder = remote_path::parent(&current).unwrap_or_else(|| "/".into());
                    current = if target.starts_with('/') {
                        protocol::normalize(target)
                    } else {
                        protocol::normalize(&format!("{folder}/{target}"))
                    };
                }
                item => return Some((current, item.clone())),
            }
        }
        None
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
            Some(_) => return Err(AppError::invalid("Not a folder")),
            None => return Err(not_found(&path)),
        }
        let entries = Self::children(&items, &path)
            .into_iter()
            .map(|(child, item)| {
                let name = remote_path::file_name(&child).to_string();
                let mut link_target = None;
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
                    Item::Link { target } => {
                        link_target = Some(match Self::followed(&items, &child) {
                            Some((_, Item::Folder)) => LinkTarget::Dir,
                            Some((_, Item::File { .. })) => LinkTarget::File,
                            _ => LinkTarget::Broken,
                        });
                        (EntryKind::Symlink, target.len() as u64, None, Some(0o777))
                    }
                };
                FileEntry {
                    hidden: name.starts_with('.'),
                    name,
                    path: child,
                    kind,
                    link_target,
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

    async fn list_dir_with_links(&self, path: &str) -> AppResult<(DirListing, LinkTargets)> {
        let listing = self.list_dir(path).await?;
        let mut targets = LinkTargets::new();
        if self.links_in_listings.load(Ordering::Relaxed) {
            let items = self.items.lock().unwrap();
            for entry in &listing.entries {
                if let Some(Item::Link { target }) = items.get(&entry.path) {
                    targets.insert(entry.name.clone(), target.clone());
                }
            }
        }
        Ok((listing, targets))
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
        // Refused as every real server refuses it.
        let path = &protocol::normalize_exact(path)?;
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

    async fn read_link(&self, path: &str) -> AppResult<String> {
        self.check_online()?;
        self.counters.link_reads.fetch_add(1, Ordering::Relaxed);
        match self.items.lock().unwrap().get(path) {
            Some(Item::Link { target }) => Ok(target.clone()),
            Some(_) => Err(AppError::invalid("Not a link")),
            None => Err(not_found(path)),
        }
    }

    /// A link goes by itself; what it leads to stays.
    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let delay = self.delete_delay_millis.load(Ordering::Relaxed);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        self.check_online()?;
        for path in paths {
            protocol::normalize_exact(path)?;
        }
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

    /// Follows links, as a server's `stat` does.
    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        self.check_online()?;
        let items = self.items.lock().unwrap();
        Ok(match Self::followed(&items, path).map(|(_, item)| item) {
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
                modified: Some(modified),
                permissions,
            }),
            Some(Item::Link { .. }) | None => None,
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
            Some(Item::Folder | Item::Link { .. }) => Ok(()),
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

    /// The file is there, empty, from the moment it is opened, as on an SFTP server.
    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        self.check_online()?;
        let modified = self.tick();
        {
            let mut items = self.items.lock().unwrap();
            let kept_permissions = match items.get(&request.path) {
                Some(Item::File { permissions, .. }) => *permissions,
                None if self.refused_new_files.load(Ordering::Relaxed) => {
                    return Err(AppError::new(
                        ErrorKind::PermissionDenied,
                        format!("No new file may be written as {}", request.path),
                    ));
                }
                _ => None,
            };
            items.insert(
                request.path.clone(),
                Item::File {
                    data: Bytes::new(),
                    modified,
                    permissions: request.permissions.or(kept_permissions),
                },
            );
        }
        Ok(Box::new(MemoryWriter {
            fs: self,
            request,
            data: Vec::new(),
        }))
    }
}

impl MemoryFs {
    fn relocate(&self, path: &str, target: &str) -> AppResult<()> {
        if self.refused_move_targets.lock().unwrap().contains(target) {
            return Err(AppError::new(
                ErrorKind::PermissionDenied,
                format!("Nothing may be moved onto {target}"),
            ));
        }
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
        let delay = self.fs.write_delay_millis.load(Ordering::Relaxed);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
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
        // Takes one refusal off the count, if any are left. A plain compare and swap, since
        // the standard library renamed `fetch_update` and CI builds with the newest compiler.
        let refusals = &self.fs.failing_uploads;
        let mut left = refusals.load(Ordering::Relaxed);
        let failing = loop {
            if left == 0 {
                break false;
            }
            match refusals.compare_exchange_weak(
                left,
                left - 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break true,
                Err(now) => left = now,
            }
        };
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
