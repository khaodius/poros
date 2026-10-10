//! Built on `RawSftpSession` because listings need `longname` (owner names) and transfers
//! need offset-addressed, pipelined reads and writes. Requests multiplex over one channel.

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::future::BoxFuture;
use futures::stream::{self, FuturesOrdered, FuturesUnordered, StreamExt};
use russh::client::Msg;
use russh::Channel;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::rawsession::Limits;
use russh_sftp::client::{Config, RawSftpSession};
use russh_sftp::extensions;
use russh_sftp::protocol::{FileAttributes, OpenFlags, Packet, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{kind_from_mode, DirListing, EntryKind, FileEntry, LinkTarget};
pub use crate::protocol::RemoteStat;
use crate::protocol::{
    Capabilities, Protocol, ReadStream, RemoteFileSystem, WriteRequest, WriteStream,
};
use crate::remote_path;
use crate::stall::Activity;

mod ops;
mod stall;

pub use ops::EntryStat;
use stall::{StallGuardedSession, TrackedStream};

const PARALLEL_REQUESTS: usize = 16;
/// How long a connection may move no bytes while requests wait before they fail.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// The library's own deadline runs from when a request is queued, so on a slow link it passes
/// before requests deep in the pipeline are even sent. This many seconds overflows the
/// deadline, which turns it off, and `stall` times requests out instead.
const NO_LIBRARY_DEADLINE_SECS: u64 = u64::MAX;
/// Requests kept in flight by the streams other protocols' transfers read and write through.
const STREAM_REQUESTS: usize = 32;
const STREAM_CHUNK: u32 = 32 * 1024;
/// Renames over an existing file in one step, unlike a plain SFTP v3 rename.
const POSIX_RENAME: &str = "posix-rename@openssh.com";

pub struct RemoteFs {
    raw: StallGuardedSession,
    pub home: String,
    /// Largest read and write payloads the server accepts (`limits@openssh.com`).
    read_limit: Option<u32>,
    write_limit: Option<u32>,
    posix_rename: bool,
    fsync: bool,
    /// The server copies data between its own files (`copy-data`).
    copy_data: bool,
}

pub enum ReadChunk {
    Data(Vec<u8>),
    Eof,
}

impl From<&FileAttributes> for RemoteStat {
    fn from(attributes: &FileAttributes) -> Self {
        Self {
            is_dir: is_dir(attributes),
            size: attributes.size.unwrap_or(0),
            modified: attributes.mtime.map(i64::from),
            permissions: attributes.permissions.map(|mode| mode & 0o7777),
        }
    }
}

impl RemoteFs {
    pub async fn open(channel: Channel<Msg>) -> AppResult<Self> {
        channel.request_subsystem(true, "sftp").await?;
        Self::over(channel.into_stream()).await
    }

    /// Starts SFTP over any byte stream: a channel, or an in-memory pipe in tests.
    async fn over<S>(stream: S) -> AppResult<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let config = Config {
            request_timeout_secs: NO_LIBRARY_DEADLINE_SECS,
            ..Default::default()
        };
        let activity = Activity::new(STALL_TIMEOUT);
        let stream = TrackedStream::new(stream, activity.clone());
        let session = RawSftpSession::new_with_config(stream, config);
        let mut raw = StallGuardedSession::new(session, activity);
        let version = raw.init().await?;
        let mut limits = Limits::default();
        let offers = |name: &str| version.extensions.contains_key(name);
        let posix_rename = offers(POSIX_RENAME);
        let fsync = offers(extensions::FSYNC);
        let copy_data = offers(ops::COPY_DATA);
        if version
            .extensions
            .get(extensions::LIMITS)
            .map(String::as_str)
            == Some("1")
        {
            if let Ok(reported) = raw.limits().await {
                limits = Limits::from(reported);
                raw.set_limits(limits);
            }
        }
        let home = canonicalize(&raw, ".").await?;
        let clamp = |limit: Option<u64>| limit.map(|bytes| bytes.min(u64::from(u32::MAX)) as u32);
        Ok(Self {
            raw,
            home,
            read_limit: clamp(limits.read_len),
            write_limit: clamp(limits.write_len),
            posix_rename,
            fsync,
            copy_data,
        })
    }

    /// Whether the server can flush a file to its disk (`fsync@openssh.com`).
    pub fn can_sync(&self) -> bool {
        self.fsync
    }

    pub fn read_size(&self, requested: u32) -> u32 {
        self.read_limit
            .map_or(requested, |limit| requested.min(limit))
    }

    pub fn write_size(&self, requested: u32) -> u32 {
        self.write_limit
            .map_or(requested, |limit| requested.min(limit))
    }

    pub fn close(&self) {
        let _ = self.raw.close_session();
    }

    pub async fn canonicalize(&self, path: &str) -> AppResult<String> {
        canonicalize(&self.raw, path).await
    }

    pub fn resolve(&self, path: &str) -> String {
        let path = path.trim();
        if path.is_empty() || path == "~" {
            self.home.clone()
        } else if let Some(rest) = path.strip_prefix("~/") {
            remote_path::join(&self.home, rest)
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            remote_path::join(&self.home, path)
        }
    }

    pub async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let resolved = self.resolve(path);
        let directory = self
            .canonicalize(&resolved)
            .await
            .map_err(|error| error.with_path(resolved.clone()))?;
        let raw_entries = self
            .read_dir(&directory)
            .await
            .map_err(|error| error.with_path(directory.clone()))?;

        let mut entries: Vec<FileEntry> = raw_entries
            .into_iter()
            .map(|(name, longname, attributes)| {
                entry_from_attributes(&directory, name, &longname, &attributes)
            })
            .collect();

        // The UI needs to know which links lead to folders.
        let links: Vec<(usize, String)> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == EntryKind::Symlink)
            .map(|(index, entry)| (index, entry.path.clone()))
            .collect();
        let resolved_links: Vec<(usize, Option<FileAttributes>)> = stream::iter(links)
            .map(|(index, path)| async move {
                (
                    index,
                    self.raw.stat(path).await.ok().map(|reply| reply.attrs),
                )
            })
            .buffer_unordered(PARALLEL_REQUESTS)
            .collect()
            .await;
        for (index, target) in resolved_links {
            let entry = &mut entries[index];
            match target {
                Some(attributes) => {
                    let is_dir = attributes
                        .permissions
                        .is_some_and(|mode| kind_from_mode(mode) == EntryKind::Dir);
                    entry.link_target = Some(if is_dir {
                        LinkTarget::Dir
                    } else {
                        LinkTarget::File
                    });
                    if !is_dir {
                        entry.size = attributes.size.unwrap_or(0);
                        entry.modified = attributes.mtime.map(i64::from).or(entry.modified);
                    }
                }
                None => entry.link_target = Some(LinkTarget::Broken),
            }
        }

        Ok(DirListing {
            parent: remote_path::parent(&directory),
            path: directory,
            entries,
        })
    }

    pub async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        validate_name(name)?;
        let path = remote_path::join(&self.resolve(parent), name);
        // SFTP v3 reports an existing target as a generic failure; check first.
        self.ensure_absent(&path, name).await?;
        self.raw
            .mkdir(path.clone(), FileAttributes::empty())
            .await
            .map_err(|error| AppError::from(error).with_path(path.clone()))?;
        Ok(path)
    }

    pub async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        validate_name(new_name)?;
        let path = self.resolve(path);
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root directory"))?;
        let target = remote_path::join(&parent, new_name);
        // SFTP v3 rename semantics on existing targets vary by server; refuse up front.
        self.ensure_absent(&target, new_name).await?;
        self.raw
            .rename(path.clone(), target.clone())
            .await
            .map_err(|error| AppError::from(error).with_path(path))?;
        Ok(target)
    }

    /// Symlinks are removed, never followed.
    pub async fn delete(&self, paths: &[String]) -> AppResult<()> {
        for path in paths {
            let path = self.resolve(path);
            if path == "/" {
                return Err(AppError::invalid("Refusing to delete /"));
            }
            let attributes = self
                .raw
                .lstat(path.clone())
                .await
                .map_err(|error| AppError::from(error).with_path(path.clone()))?
                .attrs;
            if is_dir(&attributes) {
                self.remove_tree(path).await?;
            } else {
                self.raw
                    .remove(path.clone())
                    .await
                    .map_err(|error| AppError::from(error).with_path(path))?;
            }
        }
        Ok(())
    }

    fn remove_tree(&self, directory: String) -> BoxFuture<'_, AppResult<()>> {
        Box::pin(async move {
            let children = self
                .read_dir(&directory)
                .await
                .map_err(|error| error.with_path(directory.clone()))?;
            let mut subdirs = Vec::new();
            let mut files = Vec::new();
            for (name, _, attributes) in children {
                let path = remote_path::join(&directory, &name);
                if is_dir(&attributes) {
                    subdirs.push(path);
                } else {
                    files.push(path);
                }
            }
            let results: Vec<AppResult<()>> = stream::iter(files)
                .map(|path| async move {
                    self.raw
                        .remove(path.clone())
                        .await
                        .map(|_| ())
                        .map_err(|error| AppError::from(error).with_path(path))
                })
                .buffer_unordered(PARALLEL_REQUESTS)
                .collect()
                .await;
            results.into_iter().collect::<AppResult<Vec<_>>>()?;
            for sub in subdirs {
                self.remove_tree(sub).await?;
            }
            self.raw
                .rmdir(directory.clone())
                .await
                .map_err(|error| AppError::from(error).with_path(directory))?;
            Ok(())
        })
    }

    /// `None` when nothing exists at `path`. Follows symlinks.
    pub async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        match self.raw.stat(path).await {
            Ok(reply) => Ok(Some(RemoteStat::from(&reply.attrs))),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {
                Ok(None)
            }
            Err(error) => Err(AppError::from(error).with_path(path)),
        }
    }

    /// Whether `path` is a symbolic link; false when nothing is there.
    pub async fn is_symlink(&self, path: &str) -> AppResult<bool> {
        match self.raw.lstat(path).await {
            Ok(reply) => Ok(reply
                .attrs
                .permissions
                .is_some_and(|mode| kind_from_mode(mode) == EntryKind::Symlink)),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {
                Ok(false)
            }
            Err(error) => Err(AppError::from(error).with_path(path)),
        }
    }

    /// Creates `path` unless a folder is already there; another worker may race to create it.
    pub async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        if let Some(existing) = self.stat(path).await? {
            return if existing.is_dir {
                Ok(())
            } else {
                Err(AppError::new(
                    ErrorKind::AlreadyExists,
                    format!(
                        "{} exists and is not a folder",
                        remote_path::file_name(path)
                    ),
                )
                .with_path(path))
            };
        }
        match self.raw.mkdir(path, FileAttributes::empty()).await {
            Ok(_) => Ok(()),
            Err(error) => match self.stat(path).await {
                Ok(Some(existing)) if existing.is_dir => Ok(()),
                _ => Err(AppError::from(error).with_path(path)),
            },
        }
    }

    pub async fn open_for_read(&self, path: &str) -> AppResult<String> {
        self.raw
            .open(path, OpenFlags::READ, FileAttributes::empty())
            .await
            .map(|reply| reply.handle)
            .map_err(|error| AppError::from(error).with_path(path))
    }

    pub async fn open_for_write(
        &self,
        path: &str,
        truncate: bool,
        permissions: Option<u32>,
    ) -> AppResult<String> {
        let mut flags = OpenFlags::WRITE | OpenFlags::CREATE;
        if truncate {
            flags |= OpenFlags::TRUNCATE;
        }
        let attributes = FileAttributes {
            permissions,
            ..Default::default()
        };
        self.raw
            .open(path, flags, attributes)
            .await
            .map(|reply| reply.handle)
            .map_err(|error| AppError::from(error).with_path(path))
    }

    /// Servers may return fewer bytes than requested before the end of the file.
    pub async fn read_chunk(&self, handle: &str, offset: u64, len: u32) -> AppResult<ReadChunk> {
        match self.raw.read(handle, offset, len).await {
            Ok(reply) if reply.data.is_empty() => Ok(ReadChunk::Eof),
            Ok(reply) => Ok(ReadChunk::Data(reply.data)),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::Eof => {
                Ok(ReadChunk::Eof)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn write_chunk(&self, handle: &str, offset: u64, data: Vec<u8>) -> AppResult<()> {
        self.raw.write(handle, offset, data).await?;
        Ok(())
    }

    pub async fn close_handle(&self, handle: String) -> AppResult<()> {
        self.raw.close(handle).await?;
        Ok(())
    }

    /// Closes a handle, reading its attributes in the same round trip and, when asked and
    /// offered, flushing the file to the server's disk first. The attributes are `None` when
    /// the server would not report them.
    pub async fn close_and_stat(
        &self,
        handle: String,
        sync: bool,
    ) -> (Option<RemoteStat>, AppResult<()>) {
        let synced = async {
            if sync && self.fsync {
                self.raw.fsync(handle.as_str()).await.map(|_| ())
            } else {
                Ok(())
            }
        };
        // Sent back to back: servers handle one file's requests in order, so the attributes
        // follow every acknowledged write.
        let (synced, stat, closed) = tokio::join!(
            synced,
            self.raw.fstat(handle.as_str()),
            self.raw.close(handle.as_str())
        );
        let stat = stat.ok().map(|reply| RemoteStat::from(&reply.attrs));
        let result = synced.and(closed.map(|_| ())).map_err(AppError::from);
        (stat, result)
    }

    /// Reads `len` bytes at `offset`, or fewer where the file ends.
    pub async fn read_range(&self, path: &str, offset: u64, len: u32) -> AppResult<Vec<u8>> {
        let handle = self.open_for_read(path).await?;
        let mut data = Vec::with_capacity(len as usize);
        let result = loop {
            if data.len() >= len as usize {
                break Ok(());
            }
            let missing = self.read_size(len - data.len() as u32);
            match self
                .read_chunk(&handle, offset + data.len() as u64, missing)
                .await
            {
                Ok(ReadChunk::Data(more)) => data.extend_from_slice(&more),
                Ok(ReadChunk::Eof) => break Ok(()),
                Err(error) => break Err(error),
            }
        };
        let _ = self.close_handle(handle).await;
        result.map(|_| data)
    }

    /// Moves `from` over `to`, replacing a file there: in one step where the server offers
    /// `posix-rename@openssh.com` (OpenSSH), with a plain rename where that replaces files
    /// (SFTPGo), and otherwise by moving the old file aside first. It is deleted only once
    /// the new one is in place, so a connection lost in between leaves it on the server.
    pub async fn replace(&self, from: &str, to: &str) -> AppResult<()> {
        if self.posix_rename {
            return self.posix_rename(from, to).await;
        }
        let Err(refused) = self.raw.rename(from, to).await else {
            return Ok(());
        };
        if !matches!(self.stat(to).await, Ok(Some(existing)) if !existing.is_dir) {
            return Err(AppError::from(refused).with_path(from));
        }
        let aside = aside_path(to);
        self.raw
            .rename(to, &aside)
            .await
            .map_err(|error| AppError::from(error).with_path(to))?;
        if let Err(error) = self.raw.rename(from, to).await {
            let _ = self.raw.rename(&aside, to).await;
            return Err(AppError::from(error).with_path(from));
        }
        let _ = self.raw.remove(&aside).await;
        Ok(())
    }

    async fn posix_rename(&self, from: &str, to: &str) -> AppResult<()> {
        let mut data = Vec::with_capacity(8 + from.len() + to.len());
        for path in [from, to] {
            data.extend_from_slice(&(path.len() as u32).to_be_bytes());
            data.extend_from_slice(path.as_bytes());
        }
        match self.raw.extended(POSIX_RENAME, data).await {
            Ok(Packet::Status(status)) if status.status_code == StatusCode::Ok => Ok(()),
            Ok(Packet::Status(status)) => {
                Err(AppError::from(SftpError::from(status)).with_path(from))
            }
            Ok(_) => Err(AppError::new(
                ErrorKind::Sftp,
                "The server answered a rename unexpectedly",
            )
            .with_path(from)),
            Err(error) => Err(AppError::from(error).with_path(from)),
        }
    }

    pub async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        permissions: Option<u32>,
    ) -> AppResult<()> {
        // SFTP v3 sets access and modification times together.
        let time = modified.and_then(|seconds| u32::try_from(seconds).ok());
        let attributes = FileAttributes {
            atime: time,
            mtime: time,
            permissions,
            ..Default::default()
        };
        if time.is_none() && permissions.is_none() {
            return Ok(());
        }
        self.raw
            .setstat(path, attributes)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(path))
    }

    pub async fn truncate(&self, path: &str, len: u64) -> AppResult<()> {
        let attributes = FileAttributes {
            size: Some(len),
            ..Default::default()
        };
        self.raw
            .setstat(path, attributes)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(path))
    }

    async fn read_dir(&self, directory: &str) -> AppResult<Vec<(String, String, FileAttributes)>> {
        let handle = self.raw.opendir(directory).await?.handle;
        let mut out = Vec::new();
        let result = loop {
            match self.raw.readdir(handle.as_str()).await {
                Ok(name) => out.extend(
                    name.files
                        .into_iter()
                        .filter(|file| file.filename != "." && file.filename != "..")
                        .map(|file| (file.filename, file.longname, file.attrs)),
                ),
                Err(SftpError::Status(status)) if status.status_code == StatusCode::Eof => {
                    break Ok(())
                }
                Err(error) => break Err(AppError::from(error)),
            }
        };
        let _ = self.raw.close(handle).await;
        result.map(|_| out)
    }

    async fn ensure_absent(&self, path: &str, name: &str) -> AppResult<()> {
        if self.raw.lstat(path).await.is_ok() {
            Err(
                AppError::new(ErrorKind::AlreadyExists, format!("{name} already exists"))
                    .with_path(path),
            )
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl RemoteFileSystem for RemoteFs {
    fn protocol(&self) -> Protocol {
        Protocol::Sftp
    }

    fn home(&self) -> &str {
        &self.home
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            ranged_reads: true,
            resumable_writes: true,
        }
    }

    fn resolve(&self, path: &str) -> String {
        RemoteFs::resolve(self, path)
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        RemoteFs::canonicalize(self, path).await
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        RemoteFs::list_dir(self, path).await
    }

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        RemoteFs::make_dir(self, parent, name).await
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        RemoteFs::rename(self, path, new_name).await
    }

    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        RemoteFs::delete(self, paths).await
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        RemoteFs::stat(self, path).await
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        RemoteFs::ensure_dir(self, path).await
    }

    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        permissions: Option<u32>,
    ) -> AppResult<()> {
        RemoteFs::set_attributes(self, path, modified, permissions).await
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let handle = self.open_for_read(path).await?;
        let chunk = self.read_size(STREAM_CHUNK).max(1);
        Ok(Box::new(SftpReader {
            fs: self,
            handle: Some(handle),
            next_offset: range.start,
            end: range.end,
            chunk,
            in_flight: FuturesOrdered::new(),
            ended: false,
        }))
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        let handle = self
            .open_for_write(&request.path, request.offset == 0, request.permissions)
            .await?;
        let chunk = self.write_size(STREAM_CHUNK).max(1) as usize;
        Ok(Box::new(SftpWriter {
            fs: self,
            handle: Some(handle),
            offset: request.offset,
            chunk,
            in_flight: FuturesUnordered::new(),
        }))
    }

    async fn close(&self) {
        RemoteFs::close(self);
    }
}

type ReadRequest = BoxFuture<'static, (u64, u32, AppResult<ReadChunk>)>;

/// Sequential reads with many requests in flight, so latency does not bound throughput.
struct SftpReader {
    fs: Arc<RemoteFs>,
    handle: Option<String>,
    next_offset: u64,
    end: u64,
    chunk: u32,
    in_flight: FuturesOrdered<ReadRequest>,
    ended: bool,
}

#[async_trait]
impl ReadStream for SftpReader {
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
        let Some(handle) = self.handle.clone() else {
            return Ok(None);
        };
        while !self.ended && self.in_flight.len() < STREAM_REQUESTS && self.next_offset < self.end {
            let offset = self.next_offset;
            let len = u64::from(self.chunk).min(self.end - offset) as u32;
            self.next_offset += u64::from(len);
            let fs = self.fs.clone();
            let handle = handle.clone();
            self.in_flight.push_back(Box::pin(async move {
                (offset, len, fs.read_chunk(&handle, offset, len).await)
            }));
        }
        let Some((offset, len, result)) = self.in_flight.next().await else {
            return Ok(None);
        };
        match result? {
            ReadChunk::Eof => {
                self.ended = true;
                self.in_flight = FuturesOrdered::new();
                Ok(None)
            }
            ReadChunk::Data(data) => {
                if data.len() < len as usize {
                    // A short read: everything requested after it starts at the wrong offset.
                    self.in_flight = FuturesOrdered::new();
                    self.next_offset = offset + data.len() as u64;
                }
                Ok(Some(Bytes::from(data)))
            }
        }
    }

    async fn finish(mut self: Box<Self>) -> AppResult<()> {
        self.in_flight = FuturesOrdered::new();
        match self.handle.take() {
            Some(handle) => self.fs.close_handle(handle).await,
            None => Ok(()),
        }
    }
}

/// Writes with many requests in flight; acknowledgements may arrive in any order.
struct SftpWriter {
    fs: Arc<RemoteFs>,
    handle: Option<String>,
    offset: u64,
    chunk: usize,
    in_flight: FuturesUnordered<BoxFuture<'static, AppResult<()>>>,
}

#[async_trait]
impl WriteStream for SftpWriter {
    async fn write(&mut self, data: Bytes) -> AppResult<()> {
        let handle = self
            .handle
            .clone()
            .ok_or_else(|| AppError::invalid("The file is already closed"))?;
        let mut position = 0;
        while position < data.len() {
            while self.in_flight.len() >= STREAM_REQUESTS {
                if let Some(written) = self.in_flight.next().await {
                    written?;
                }
            }
            let end = (position + self.chunk).min(data.len());
            let piece = data.slice(position..end).to_vec();
            let offset = self.offset;
            self.offset += piece.len() as u64;
            position = end;
            let fs = self.fs.clone();
            let handle = handle.clone();
            self.in_flight.push(Box::pin(async move {
                fs.write_chunk(&handle, offset, piece).await
            }));
        }
        Ok(())
    }

    async fn finish(mut self: Box<Self>) -> AppResult<()> {
        let mut result = Ok(());
        while let Some(written) = self.in_flight.next().await {
            if result.is_ok() {
                result = written;
            }
        }
        if let Some(handle) = self.handle.take() {
            let closed = self.fs.close_handle(handle).await;
            result = result.and(closed);
        }
        result
    }
}

async fn canonicalize(raw: &StallGuardedSession, path: &str) -> AppResult<String> {
    let name = raw.realpath(path).await?;
    name.files
        .into_iter()
        .next()
        .map(|file| file.filename)
        .ok_or_else(|| AppError::new(ErrorKind::Sftp, "Server returned no path"))
}

fn is_dir(attributes: &FileAttributes) -> bool {
    attributes
        .permissions
        .is_some_and(|mode| kind_from_mode(mode) == EntryKind::Dir)
}

fn validate_name(name: &str) -> AppResult<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        Err(AppError::invalid(format!("\"{name}\" is not a valid name")))
    } else {
        Ok(())
    }
}

fn entry_from_attributes(
    directory: &str,
    name: String,
    longname: &str,
    attributes: &FileAttributes,
) -> FileEntry {
    let kind = attributes
        .permissions
        .map(kind_from_mode)
        .unwrap_or(EntryKind::File);
    let (owner, group) = owner_group(longname, attributes);
    FileEntry {
        path: remote_path::join(directory, &name),
        hidden: name.starts_with('.'),
        kind,
        link_target: None,
        size: if kind == EntryKind::Dir {
            0
        } else {
            attributes.size.unwrap_or(0)
        },
        modified: attributes.mtime.map(i64::from),
        permissions: attributes.permissions.map(|mode| mode & 0o7777),
        owner,
        group,
        name,
    }
}

/// SFTP v3 only carries numeric uid/gid; OpenSSH and most servers put the names in the
/// `ls -l` style longname (`drwxr-xr-x  2 alice staff 4096 Jan 1 00:00 name`).
fn owner_group(longname: &str, attributes: &FileAttributes) -> (Option<String>, Option<String>) {
    let mut fields = longname.split_whitespace();
    let looks_like_ls = fields.next().is_some_and(|mode| {
        mode.len() >= 10 && mode.starts_with(['-', 'd', 'l', 'c', 'b', 'p', 's'])
    });
    if looks_like_ls {
        let _links = fields.next();
        if let (Some(owner), Some(group)) = (fields.next(), fields.next()) {
            return (Some(owner.to_string()), Some(group.to_string()));
        }
    }
    (
        attributes.uid.map(|uid| uid.to_string()),
        attributes.gid.map(|gid| gid.to_string()),
    )
}

/// A hidden name beside `path` for the file a replace moves out of the way.
fn aside_path(path: &str) -> String {
    let name = format!(".poros-replaced-{}", uuid::Uuid::new_v4().simple());
    match remote_path::parent(path) {
        Some(parent) => remote_path::join(&parent, &name),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;
    use russh_sftp::protocol::{Data, File, Name, Version};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
    use tokio::time::Instant;

    /// Answers like a server at the far end of a slow link: one request at a time, each read
    /// after `delay`, and nothing more after `reads_answered` reads.
    async fn slow_server(mut stream: DuplexStream, delay: Duration, reads_answered: usize) {
        let mut answered = 0;
        loop {
            let Ok(length) = stream.read_u32().await else {
                return;
            };
            let mut body = vec![0; length as usize];
            if stream.read_exact(&mut body).await.is_err() {
                return;
            }
            let reply = match Packet::try_from(&mut Bytes::from(body)).expect("a valid request") {
                Packet::Init(_) => Packet::Version(Version::new()),
                Packet::RealPath(request) => Packet::Name(Name {
                    id: request.id,
                    files: vec![File::dummy("/home/poros")],
                }),
                Packet::Read(request) => {
                    if answered == reads_answered {
                        std::future::pending::<()>().await;
                    }
                    answered += 1;
                    tokio::time::sleep(delay).await;
                    Packet::Data(Data {
                        id: request.id,
                        data: vec![0; request.len as usize],
                    })
                }
                other => panic!("unexpected request {other:?}"),
            };
            let reply = Bytes::try_from(reply).expect("an encodable reply");
            if stream.write_all(&reply).await.is_err() {
                return;
            }
        }
    }

    async fn connect_to_slow_server(reads_answered: usize) -> RemoteFs {
        let (client, server) = tokio::io::duplex(1024 * 1024);
        tokio::spawn(slow_server(server, Duration::from_secs(5), reads_answered));
        RemoteFs::over(client).await.unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn reads_queued_behind_slow_replies_do_not_time_out() {
        let fs = connect_to_slow_server(usize::MAX).await;
        let started = Instant::now();
        // The last of these is answered 100 seconds after it was sent, while every reply before
        // it shows the connection is moving.
        let results =
            join_all((0..20).map(|index| fs.read_chunk("file", index * 1024, 1024))).await;
        assert_eq!(started.elapsed(), Duration::from_secs(100));
        for result in results {
            assert!(matches!(result, Ok(ReadChunk::Data(data)) if data.len() == 1024));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reads_time_out_once_the_connection_stops_moving() {
        let fs = connect_to_slow_server(3).await;
        let started = Instant::now();
        let results = join_all((0..6).map(|index| fs.read_chunk("file", index * 1024, 1024))).await;
        // The third reply arrives at 15 seconds, and nothing moves after it.
        assert_eq!(started.elapsed(), Duration::from_secs(15) + STALL_TIMEOUT);
        let (answered, stalled) = results.split_at(3);
        assert!(answered.iter().all(|result| result.is_ok()));
        for result in stalled {
            let error = result.as_ref().err().expect("a stalled read fails");
            assert_eq!(error.kind, ErrorKind::Timeout);
            assert!(error.is_connection_lost());
        }
    }

    #[test]
    fn owner_group_from_longname() {
        let attributes = FileAttributes {
            uid: Some(1000),
            gid: Some(1000),
            ..Default::default()
        };
        assert_eq!(
            owner_group(
                "-rw-r--r--    1 alice    staff        42 Jan  1 00:00 f.txt",
                &attributes
            ),
            (Some("alice".into()), Some("staff".into()))
        );
        assert_eq!(
            owner_group("", &attributes),
            (Some("1000".into()), Some("1000".into()))
        );
    }

    #[test]
    fn replaced_files_move_aside_into_their_own_folder() {
        let aside = aside_path("/srv/www/index.html");
        assert!(aside.starts_with("/srv/www/.poros-replaced-"));
        assert_ne!(aside, aside_path("/srv/www/index.html"));
        assert!(aside_path("/top").starts_with("/.poros-replaced-"));
        assert!(aside_path("relative").starts_with(".poros-replaced-"));
    }

    #[test]
    fn entry_kinds_and_sizes() {
        let folder_attributes = FileAttributes {
            permissions: Some(0o040755),
            size: Some(4096),
            mtime: Some(1_700_000_000),
            ..Default::default()
        };
        let entry = entry_from_attributes("/srv", "www".into(), "", &folder_attributes);
        assert_eq!(entry.kind, EntryKind::Dir);
        assert_eq!(entry.size, 0);
        assert_eq!(entry.path, "/srv/www");
        assert_eq!(entry.permissions, Some(0o755));
        assert_eq!(entry.modified, Some(1_700_000_000));

        let file = FileAttributes {
            permissions: Some(0o100600),
            size: Some(12),
            ..Default::default()
        };
        let entry = entry_from_attributes("/", ".env".into(), "", &file);
        assert_eq!(entry.kind, EntryKind::File);
        assert_eq!(entry.size, 12);
        assert!(entry.hidden);
        assert_eq!(entry.path, "/.env");
    }
}
