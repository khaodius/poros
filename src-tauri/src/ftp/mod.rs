//! FTP and FTPS servers as a `RemoteFileSystem`. An `FtpFs` owns one control connection, so
//! its requests take turns; transfer workers each open their own. The connection reopens by
//! itself when the server drops it for being idle.

mod control;
pub mod fxp;
mod listing;

use std::ops::Range;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::future::BoxFuture;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::model::{DirListing, EntryKind, FileEntry, LinkTarget};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;
use crate::ssh::{ConnectProfile, HostKeyApproval, DEFAULT_KEEPALIVE_SECS};
use crate::tls::{ServerTls, TrustedCertificates};
use control::{Control, DataConnection, DataStart, Features};
use listing::RawEntry;

/// A connection idle this long is checked with `NOOP` before use, since servers drop idle
/// connections without saying so.
const STALE_AFTER: Duration = Duration::from_secs(15);
const READ_CHUNK: usize = 256 * 1024;
/// How long to wait, at the end of a ranged read, to see whether the file ended there too.
const END_PROBE: Duration = Duration::from_millis(500);

type ControlGuard = OwnedMutexGuard<Option<Control>>;

pub struct FtpFs {
    profile: ConnectProfile,
    home: String,
    session_id: String,
    trusted: TrustedCertificates,
    /// The certificate accepted when the session opened; reconnections insist on it.
    certificate: Option<String>,
    control: Arc<Mutex<Option<Control>>>,
    features: Features,
}

impl FtpFs {
    /// Connects and logs in. Also returns the fingerprint of the server's TLS certificate, or
    /// an empty string without TLS.
    pub async fn connect(
        session_id: &str,
        profile: &ConnectProfile,
        trusted: &TrustedCertificates,
        approval: Option<HostKeyApproval>,
        events: &Events,
    ) -> AppResult<(Arc<Self>, String)> {
        let tls = Self::tls(session_id, profile, trusted, approval, events)?;
        events.log(
            LogLevel::Info,
            Some(session_id),
            format!("Connecting to {}:{}", profile.host.trim(), profile.port),
        );
        let (control, home, certificate) = Self::open(session_id, profile, tls, events).await?;
        events.log(
            LogLevel::Info,
            Some(session_id),
            format!(
                "{} session ready, home folder {home}",
                profile.protocol.display_name()
            ),
        );
        let features = control.features;
        let file_system = Arc::new(Self {
            profile: profile.clone(),
            home,
            session_id: session_id.to_string(),
            trusted: trusted.clone(),
            certificate: certificate.clone(),
            control: Arc::new(Mutex::new(Some(control))),
            features,
        });
        let keepalive = profile.keepalive_secs.unwrap_or(DEFAULT_KEEPALIVE_SECS);
        if keepalive > 0 {
            tokio::spawn(keep_alive(
                Arc::downgrade(&file_system.control),
                Duration::from_secs(keepalive),
            ));
        }
        Ok((file_system, certificate.unwrap_or_default()))
    }

    fn tls(
        session_id: &str,
        profile: &ConnectProfile,
        trusted: &TrustedCertificates,
        approval: Option<HostKeyApproval>,
        events: &Events,
    ) -> AppResult<Option<ServerTls>> {
        match profile.protocol {
            Protocol::Ftps | Protocol::FtpsImplicit => Ok(Some(ServerTls::new(
                profile.host.trim(),
                profile.port,
                trusted,
                approval,
                session_id,
                events,
            )?)),
            _ => Ok(None),
        }
    }

    async fn open(
        session_id: &str,
        profile: &ConnectProfile,
        tls: Option<ServerTls>,
        events: &Events,
    ) -> AppResult<(Control, String, Option<String>)> {
        let (control, home) = Control::connect(profile, tls, session_id, events).await?;
        let certificate = control.certificate_fingerprint();
        Ok((control, home, certificate))
    }

    /// The control connection, reopened if the server dropped it.
    async fn connection(&self) -> AppResult<ControlGuard> {
        let mut guard = self.control.clone().lock_owned().await;
        if let Some(control) = guard.as_mut() {
            if control.idle_for() > STALE_AFTER && control.noop().await.is_err() {
                *guard = None;
            }
        }
        if guard.is_none() {
            let approval = self.certificate.clone().map(|fingerprint| HostKeyApproval {
                fingerprint,
                remember: false,
            });
            let quiet = Events::default();
            let tls = Self::tls(
                &self.session_id,
                &self.profile,
                &self.trusted,
                approval,
                &quiet,
            )?;
            let (control, _, _) = Self::open(&self.session_id, &self.profile, tls, &quiet).await?;
            *guard = Some(control);
        }
        Ok(guard)
    }

    /// Runs one request on the control connection, dropping the connection if it broke.
    async fn with_control<T>(
        &self,
        request: impl for<'control> FnOnce(&'control mut Control) -> BoxFuture<'control, AppResult<T>>,
    ) -> AppResult<T> {
        let mut guard = self.connection().await?;
        let control = guard.as_mut().expect("connection() always connects");
        let result = request(control).await;
        if result.as_ref().is_err_and(AppError::is_connection_lost) {
            *guard = None;
        }
        result
    }

    pub fn label(&self) -> String {
        self.profile.label()
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn entries_with_links(
        control: &mut Control,
        directory: &str,
    ) -> AppResult<Vec<FileEntry>> {
        let raw = control.list(directory).await?;
        let mut entries = Vec::with_capacity(raw.len());
        for entry in raw {
            let path = remote_path::join(directory, &entry.name);
            let mut file_entry = file_entry(&path, entry);
            if file_entry.kind == EntryKind::Symlink {
                let (target, size) = control.link_target(&path).await?;
                file_entry.link_target = Some(match target {
                    EntryKind::Dir => LinkTarget::Dir,
                    EntryKind::File => {
                        file_entry.size = size;
                        LinkTarget::File
                    }
                    _ => LinkTarget::Broken,
                });
            }
            entries.push(file_entry);
        }
        Ok(entries)
    }

    /// What is at `path` according to its folder's listing, which unlike `stat` tells a link
    /// from what it points to.
    async fn listed_kind(control: &mut Control, path: &str) -> AppResult<Option<EntryKind>> {
        let Some(parent) = remote_path::parent(path) else {
            return Ok(Some(EntryKind::Dir));
        };
        let name = remote_path::file_name(path);
        Ok(control
            .list(&parent)
            .await?
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.kind))
    }

    fn remove_tree<'control>(
        control: &'control mut Control,
        directory: String,
    ) -> BoxFuture<'control, AppResult<()>> {
        Box::pin(async move {
            let children = control.list(&directory).await?;
            for child in children {
                let path = remote_path::join(&directory, &child.name);
                if child.kind == EntryKind::Dir {
                    Self::remove_tree(control, path).await?;
                } else {
                    control.delete_file(&path).await?;
                }
            }
            control.remove_dir(&directory).await
        })
    }

    async fn ensure_absent(control: &mut Control, path: &str) -> AppResult<()> {
        match control.stat(path).await? {
            Some(_) => Err(protocol::already_exists(path)),
            None => Ok(()),
        }
    }
}

fn file_entry(path: &str, entry: RawEntry) -> FileEntry {
    FileEntry {
        path: path.to_string(),
        hidden: entry.name.starts_with('.'),
        kind: entry.kind,
        link_target: None,
        size: entry.size,
        modified: entry.modified,
        permissions: entry.permissions,
        owner: entry.owner,
        group: entry.group,
        name: entry.name,
    }
}

async fn keep_alive(control: Weak<Mutex<Option<Control>>>, interval: Duration) {
    loop {
        tokio::time::sleep(interval).await;
        let Some(control) = control.upgrade() else {
            return;
        };
        // A busy connection is in use, and so alive.
        let Ok(mut guard) = control.try_lock() else {
            continue;
        };
        if let Some(connection) = guard.as_mut() {
            if connection.idle_for() >= interval && connection.noop().await.is_err() {
                *guard = None;
            }
        }
    }
}

#[async_trait]
impl RemoteFileSystem for FtpFs {
    fn protocol(&self) -> Protocol {
        self.profile.protocol
    }

    fn home(&self) -> &str {
        &self.home
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            ranged_reads: self.features.rest_stream,
            resumable_writes: self.features.rest_stream,
        }
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        let path = self.resolve(path);
        self.with_control(move |control| {
            Box::pin(async move { control.change_directory(&path).await })
        })
        .await
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let requested = self.resolve(path);
        let mut guard = self.connection().await?;
        let control = guard.as_mut().expect("connection() always connects");
        let result = async {
            let directory = control.change_directory(&requested).await?;
            let entries = Self::entries_with_links(control, &directory).await?;
            Ok(DirListing {
                parent: remote_path::parent(&directory),
                path: directory,
                entries,
            })
        }
        .await;
        if result.as_ref().is_err_and(AppError::is_connection_lost) {
            *guard = None;
        }
        result
    }

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        protocol::validate_name(name)?;
        let path = remote_path::join(&self.resolve(parent), name);
        self.with_control(move |control| {
            Box::pin(async move {
                Self::ensure_absent(control, &path).await?;
                control.make_dir(&path).await?;
                Ok(path)
            })
        })
        .await
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let path = self.resolve(path);
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root folder"))?;
        let target = remote_path::join(&parent, new_name);
        self.with_control(move |control| {
            Box::pin(async move {
                Self::ensure_absent(control, &target).await?;
                control.rename(&path, &target).await?;
                Ok(target)
            })
        })
        .await
    }

    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        let targets = protocol::outermost(&resolved);
        self.with_control(move |control| {
            Box::pin(async move {
                for path in targets {
                    if path == "/" {
                        return Err(AppError::invalid("Refusing to delete /"));
                    }
                    match Self::listed_kind(control, &path).await? {
                        None => {
                            return Err(AppError::new(
                                ErrorKind::NotFound,
                                format!("{} no longer exists", remote_path::file_name(&path)),
                            )
                            .with_path(path))
                        }
                        Some(EntryKind::Dir) => Self::remove_tree(control, path).await?,
                        Some(_) => control.delete_file(&path).await?,
                    }
                }
                Ok(())
            })
        })
        .await
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        let path = self.resolve(path);
        self.with_control(move |control| Box::pin(async move { control.stat(&path).await }))
            .await
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let path = self.resolve(path);
        self.with_control(move |control| {
            Box::pin(async move {
                match control.stat(&path).await? {
                    Some(existing) if existing.is_dir => return Ok(()),
                    Some(_) => return Err(protocol::not_a_folder(&path)),
                    None => {}
                }
                match control.make_dir(&path).await {
                    Ok(()) => Ok(()),
                    Err(error) => match control.stat(&path).await {
                        Ok(Some(existing)) if existing.is_dir => Ok(()),
                        _ => Err(error),
                    },
                }
            })
        })
        .await
    }

    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        permissions: Option<u32>,
    ) -> AppResult<()> {
        let path = self.resolve(path);
        self.with_control(move |control| {
            Box::pin(async move {
                if let Some(modified) = modified {
                    control.set_modified(&path, modified).await?;
                }
                if let Some(permissions) = permissions {
                    control.set_permissions(&path, permissions).await?;
                }
                Ok(())
            })
        })
        .await
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let mut guard = self.connection().await?;
        let control = guard.as_mut().expect("connection() always connects");
        let started = control.retrieve(path, range.start).await;
        let data = match started {
            Ok(DataStart::Opened(data)) => Some(data),
            Ok(DataStart::Finished) => None,
            Err(error) => {
                if error.is_connection_lost() {
                    *guard = None;
                }
                return Err(error);
            }
        };
        Ok(Box::new(FtpReader {
            ended: data.is_none(),
            guard,
            data,
            remaining: range.end.saturating_sub(range.start),
            finished: false,
        }))
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        let mut guard = self.connection().await?;
        let control = guard.as_mut().expect("connection() always connects");
        let data = match control.store(&request.path, request.offset).await {
            Ok(DataStart::Opened(data)) => data,
            Ok(DataStart::Finished) => {
                return Err(AppError::new(
                    ErrorKind::Ftp,
                    "The server ended the upload before it started",
                )
                .with_path(request.path))
            }
            Err(error) => {
                if error.is_connection_lost() {
                    *guard = None;
                }
                return Err(error);
            }
        };
        Ok(Box::new(FtpWriter {
            guard,
            data: Some(data),
            finished: false,
        }))
    }

    async fn close(&self) {
        let mut guard = self.control.lock().await;
        if let Some(mut control) = guard.take() {
            control.quit().await;
        }
    }

    fn as_ftp(&self) -> Option<&FtpFs> {
        Some(self)
    }
}

/// Holds the control connection for the length of a download.
struct FtpReader {
    guard: ControlGuard,
    data: Option<DataConnection>,
    remaining: u64,
    /// The server closed the data connection: the file ended.
    ended: bool,
    finished: bool,
}

impl FtpReader {
    async fn complete(&mut self) -> AppResult<()> {
        let Some(mut data) = self.data.take() else {
            return Ok(());
        };
        let control = self.guard.as_mut().expect("held while reading");
        if !self.ended {
            let mut probe = [0u8; 1];
            if let Ok(Ok(0)) = tokio::time::timeout(END_PROBE, data.read_some(&mut probe)).await {
                self.ended = true;
            }
        }
        if self.ended {
            drop(data);
            control.finish_transfer().await
        } else {
            control.abort_transfer(data).await
        }
    }
}

#[async_trait]
impl ReadStream for FtpReader {
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
        let Some(data) = self.data.as_mut() else {
            return Ok(None);
        };
        if self.remaining == 0 || self.ended {
            return Ok(None);
        }
        let mut buffer = vec![0; READ_CHUNK.min(self.remaining as usize)];
        let read = data.read_some(&mut buffer).await?;
        if read == 0 {
            self.ended = true;
            return Ok(None);
        }
        self.remaining -= read as u64;
        buffer.truncate(read);
        Ok(Some(Bytes::from(buffer)))
    }

    async fn finish(mut self: Box<Self>) -> AppResult<()> {
        let result = self.complete().await;
        self.finished = true;
        if result.is_err() {
            *self.guard = None;
        }
        result
    }
}

impl Drop for FtpReader {
    /// A read abandoned midway leaves replies the connection can no longer pair with requests.
    fn drop(&mut self) {
        if !self.finished {
            *self.guard = None;
        }
    }
}

/// Holds the control connection for the length of an upload.
struct FtpWriter {
    guard: ControlGuard,
    data: Option<DataConnection>,
    finished: bool,
}

#[async_trait]
impl WriteStream for FtpWriter {
    async fn write(&mut self, bytes: Bytes) -> AppResult<()> {
        let data = self
            .data
            .as_mut()
            .ok_or_else(|| AppError::new(ErrorKind::Ftp, "The upload already ended"))?;
        if let Err(error) = data.write_all(&bytes).await {
            self.data = None;
            // The server usually says on the control connection why it stopped the upload.
            let control = self.guard.as_mut().expect("held while writing");
            let explained = tokio::time::timeout(Duration::from_secs(2), control.read_reply())
                .await
                .ok()
                .and_then(Result::ok)
                .filter(|reply| reply.class() >= 4)
                .map(|reply| control::reply_error(&reply));
            return Err(explained.unwrap_or(error));
        }
        Ok(())
    }

    async fn finish(mut self: Box<Self>) -> AppResult<()> {
        let result = async {
            let data = self
                .data
                .take()
                .ok_or_else(|| AppError::new(ErrorKind::Ftp, "The upload already ended"))?;
            let control = self.guard.as_mut().expect("held while writing");
            data.finish_sending(control.timeout()).await;
            control.finish_transfer().await
        }
        .await;
        self.finished = true;
        if result.is_err() {
            *self.guard = None;
        }
        result
    }
}

impl Drop for FtpWriter {
    fn drop(&mut self) {
        if !self.finished {
            *self.guard = None;
        }
    }
}
