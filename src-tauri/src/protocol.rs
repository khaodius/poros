//! What every kind of server connection offers the file panes and the transfer queue. SFTP,
//! FTP and FTPS, WebDAV, S3, Backblaze B2, Google Drive, OneDrive and Box each implement
//! `RemoteFileSystem`; SFTP keeps its own pipelined transfer path and SSH extras (rsync,
//! commands) on top.

use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::DirListing;
use crate::remote_path;

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    #[default]
    Sftp,
    Ftp,
    /// FTP upgraded to TLS with `AUTH TLS` before logging in (explicit FTPS).
    Ftps,
    /// FTP inside TLS from the first byte, usually on port 990 (implicit FTPS).
    FtpsImplicit,
    /// WebDAV over plain HTTP.
    WebDav,
    /// WebDAV over HTTPS.
    WebDavs,
    /// Amazon S3 or any service with an S3-compatible API.
    S3,
    /// Backblaze B2 over its native API.
    BackblazeB2,
    GoogleDrive,
    OneDrive,
    Box,
}

impl Protocol {
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Sftp => 22,
            Protocol::Ftp | Protocol::Ftps => 21,
            Protocol::FtpsImplicit => 990,
            Protocol::WebDav => 80,
            Protocol::WebDavs
            | Protocol::S3
            | Protocol::BackblazeB2
            | Protocol::GoogleDrive
            | Protocol::OneDrive
            | Protocol::Box => 443,
        }
    }

    pub fn is_ftp(self) -> bool {
        matches!(
            self,
            Protocol::Ftp | Protocol::Ftps | Protocol::FtpsImplicit
        )
    }

    pub fn is_webdav(self) -> bool {
        matches!(self, Protocol::WebDav | Protocol::WebDavs)
    }

    /// Requests go over HTTP, any number at once on the session's own client, so transfers
    /// need no connections of their own.
    pub fn is_http(self) -> bool {
        !matches!(
            self,
            Protocol::Sftp | Protocol::Ftp | Protocol::Ftps | Protocol::FtpsImplicit
        )
    }

    /// The account signs in through the browser instead of with a password.
    pub fn signs_in_with_oauth(self) -> bool {
        matches!(
            self,
            Protocol::GoogleDrive | Protocol::OneDrive | Protocol::Box
        )
    }

    /// The host a service with one fixed address lists its connections under.
    pub fn service_host(self) -> Option<&'static str> {
        match self {
            Protocol::GoogleDrive => Some("drive.google.com"),
            Protocol::OneDrive => Some("onedrive.live.com"),
            Protocol::Box => Some("app.box.com"),
            Protocol::BackblazeB2 => Some("api.backblazeb2.com"),
            _ => None,
        }
    }

    /// An upload carries the file's modification time, so none is set after it.
    pub fn uploads_carry_time(self) -> bool {
        matches!(
            self,
            Protocol::S3
                | Protocol::BackblazeB2
                | Protocol::GoogleDrive
                | Protocol::OneDrive
                | Protocol::Box
        )
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Protocol::Sftp => "SFTP",
            Protocol::Ftp => "FTP",
            Protocol::Ftps | Protocol::FtpsImplicit => "FTPS",
            Protocol::WebDav | Protocol::WebDavs => "WebDAV",
            Protocol::S3 => "S3",
            Protocol::BackblazeB2 => "Backblaze B2",
            Protocol::GoogleDrive => "Google Drive",
            Protocol::OneDrive => "OneDrive",
            Protocol::Box => "Box",
        }
    }
}

/// The parts of a remote file's attributes a transfer needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteStat {
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Reads can start anywhere and stop early, so idle workers can download separate parts of
    /// one large file.
    pub ranged_reads: bool,
    /// A write can continue an existing file from an offset, so an interrupted upload resumes
    /// instead of starting over.
    pub resumable_writes: bool,
}

/// A file being written. Nothing is guaranteed to be on the server until `finish` returns.
#[derive(Debug, Clone)]
pub struct WriteRequest {
    pub path: String,
    /// Where writing starts; anything after it is replaced. Zero creates or truncates the file.
    pub offset: u64,
    /// The size the file will have, for protocols that announce it up front.
    pub size: u64,
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
}

#[async_trait]
pub trait ReadStream: Send {
    /// The next bytes of the requested range; `None` once it, or the file, has ended.
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>>;

    /// Ends the read, whether or not it reached the end, and leaves the connection ready for
    /// the next request.
    async fn finish(self: Box<Self>) -> AppResult<()>;
}

#[async_trait]
pub trait WriteStream: Send {
    async fn write(&mut self, data: Bytes) -> AppResult<()>;

    async fn finish(self: Box<Self>) -> AppResult<()>;
}

#[async_trait]
pub trait RemoteFileSystem: Send + Sync {
    fn protocol(&self) -> Protocol;

    fn home(&self) -> &str;

    fn capabilities(&self) -> Capabilities;

    /// An absolute path for `path`, which may be relative to the home folder or start with `~`.
    fn resolve(&self, path: &str) -> String {
        resolve_against(self.home(), path)
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String>;

    async fn list_dir(&self, path: &str) -> AppResult<DirListing>;

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String>;

    /// Creates an empty file, refusing a name already taken. Protocols that can create a file
    /// only if it is absent in one request override this to close the gap between the check and
    /// the write.
    async fn make_file(self: Arc<Self>, parent: &str, name: &str) -> AppResult<String> {
        validate_name(name)?;
        let path = remote_path::join(&self.resolve(parent), name);
        if self.stat(&path).await?.is_some() {
            return Err(already_exists(&path));
        }
        let request = WriteRequest {
            path: path.clone(),
            offset: 0,
            size: 0,
            modified: None,
            permissions: None,
        };
        self.open_write(request).await?.finish().await?;
        Ok(path)
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String>;

    /// Folders are removed with everything in them.
    async fn delete(&self, paths: &[String]) -> AppResult<()>;

    /// `None` when nothing exists at `path`.
    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>>;

    /// Creates `path` unless a folder is already there; another worker may race to create it.
    async fn ensure_dir(&self, path: &str) -> AppResult<()>;

    /// Best effort: servers may refuse either change.
    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        permissions: Option<u32>,
    ) -> AppResult<()>;

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>>;

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>>;

    async fn close(&self) {}

    /// The FTP connection behind this file system, for transfers straight between two FTP
    /// servers (FXP).
    fn as_ftp(&self) -> Option<&crate::ftp::FtpFs> {
        None
    }
}

pub fn resolve_against(home: &str, path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "~" {
        home.to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        remote_path::join(home, rest)
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        remote_path::join(home, path)
    }
}

/// `/a/./b/../c/` becomes `/a/c`; paths never climb above the root.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

pub fn validate_name(name: &str) -> AppResult<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        Err(AppError::invalid(format!("\"{name}\" is not a valid name")))
    } else {
        Ok(())
    }
}

pub fn already_exists(path: &str) -> AppError {
    AppError::new(
        ErrorKind::AlreadyExists,
        format!("{} already exists", remote_path::file_name(path)),
    )
    .with_path(path)
}

pub fn not_a_folder(path: &str) -> AppError {
    AppError::new(
        ErrorKind::AlreadyExists,
        format!(
            "{} exists and is not a folder",
            remote_path::file_name(path)
        ),
    )
    .with_path(path)
}

/// The paths in `paths` that are not inside another of them, so a folder and its contents are
/// not deleted twice.
pub fn outermost(paths: &[String]) -> Vec<String> {
    let mut sorted: Vec<String> = paths.iter().map(|path| normalize(path)).collect();
    sorted.sort();
    sorted.dedup();
    let mut kept: Vec<String> = Vec::new();
    for path in sorted {
        let inside_kept = kept.iter().any(|outer| {
            outer == "/"
                || path
                    .strip_prefix(outer.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        });
        if !inside_kept {
            kept.push(path);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize(""), "/");
        assert_eq!(normalize("/a/./b/../c/"), "/a/c");
        assert_eq!(normalize("/../../x"), "/x");
        assert_eq!(normalize("a//b"), "/a/b");
    }

    #[test]
    fn resolves_against_home() {
        assert_eq!(resolve_against("/home/u", ""), "/home/u");
        assert_eq!(resolve_against("/home/u", "~/docs"), "/home/u/docs");
        assert_eq!(resolve_against("/home/u", "docs"), "/home/u/docs");
        assert_eq!(resolve_against("/home/u", "/etc"), "/etc");
    }

    #[test]
    fn keeps_only_outermost_paths() {
        let paths = vec![
            "/a/b".to_string(),
            "/a".to_string(),
            "/ab".to_string(),
            "/a/b/c".to_string(),
        ];
        assert_eq!(outermost(&paths), vec!["/a".to_string(), "/ab".to_string()]);
    }

    #[test]
    fn default_ports() {
        assert_eq!(Protocol::Sftp.default_port(), 22);
        assert_eq!(Protocol::Ftps.default_port(), 21);
        assert_eq!(Protocol::FtpsImplicit.default_port(), 990);
        assert_eq!(Protocol::WebDav.default_port(), 80);
        assert_eq!(Protocol::WebDavs.default_port(), 443);
        assert!(Protocol::OneDrive.signs_in_with_oauth());
        assert!(Protocol::Box.signs_in_with_oauth());
        assert!(!Protocol::S3.signs_in_with_oauth());
        assert!(Protocol::FtpsImplicit.is_ftp());
        assert!(Protocol::WebDavs.is_http() && Protocol::BackblazeB2.is_http());
        assert!(!Protocol::Sftp.is_http() && !Protocol::Ftps.is_http());
    }
}
