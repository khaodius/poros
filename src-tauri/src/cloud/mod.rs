//! Storage reached over HTTP: WebDAV servers, S3 and Backblaze B2 with access keys, and
//! Google Drive, OneDrive and Box over their REST APIs. Those three accounts sign in in the
//! browser (OAuth 2.0 with a loopback redirect); the refresh token is the connection's secret,
//! kept in the system keychain like a password.

mod api;
pub mod b2;
pub mod box_drive;
pub mod google_drive;
pub mod oauth;
pub mod onedrive;
pub mod s3;
mod upload;
pub mod webdav;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::model::{EntryKind, FileEntry};
use crate::protocol::{Protocol, ReadStream, RemoteFileSystem};
use crate::remote_path;
use crate::ssh::{AuthMethod, ConnectProfile, HostKeyApproval};
use crate::tls::{ServerTls, TrustedCertificates};
pub use oauth::{missing_client, Authorization, OAuthClient, OAuthVault, ProviderStatus, SignedIn};

/// How long a folder's id is trusted without asking the service again. Short, because the
/// folder can be renamed or removed from another device.
const FOLDER_CACHE_TTL: Duration = Duration::from_secs(60);

/// Everything but unreserved characters is escaped in a path segment of a URL.
const URL_SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// `path` as URL path segments, each escaped, with a leading slash and no trailing one.
fn encode_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            format!(
                "/{}",
                percent_encoding::utf8_percent_encode(segment, URL_SEGMENT)
            )
        })
        .collect()
}

/// Receives a refresh token that replaced the one the session started with.
pub type TokenRotation = Arc<dyn Fn(&str) + Send + Sync>;

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudProvider {
    Google,
    Microsoft,
    Box,
}

impl CloudProvider {
    pub const ALL: [CloudProvider; 3] = [Self::Google, Self::Microsoft, Self::Box];

    pub fn for_protocol(protocol: Protocol) -> Option<Self> {
        match protocol {
            Protocol::GoogleDrive => Some(Self::Google),
            Protocol::OneDrive => Some(Self::Microsoft),
            Protocol::Box => Some(Self::Box),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Microsoft => "Microsoft",
            Self::Box => "Box",
        }
    }
}

/// What a connection needs besides its profile: certificates the user approved for servers
/// the system does not trust, and where a replaced refresh token goes.
pub struct Login<'login> {
    pub certificates: &'login TrustedCertificates,
    pub approval: Option<HostKeyApproval>,
    pub rotation: Option<TokenRotation>,
}

impl Login<'_> {
    /// TLS that accepts what the system trusts and what the user approved for `host`.
    fn server_tls(
        &self,
        host: &str,
        port: u16,
        session_id: &str,
        events: &Events,
    ) -> AppResult<ServerTls> {
        ServerTls::new(
            host,
            port,
            self.certificates,
            self.approval.clone(),
            session_id,
            events,
        )
    }
}

/// A first request that failed because of the server's certificate fails with the prompt to
/// trust it instead.
fn certificate_rejection(error: AppError, tls: Option<&ServerTls>) -> AppError {
    tls.and_then(ServerTls::rejection).unwrap_or(error)
}

/// The password a key or account was given, for the services that sign in with one.
fn password_of(profile: &ConnectProfile) -> AppResult<String> {
    match &profile.auth {
        AuthMethod::Password { password } => Ok(password.clone()),
        _ => Err(AppError::invalid(format!(
            "{} connections sign in with a password or key",
            profile.protocol.display_name()
        ))),
    }
}

/// Signs in to an account in the browser.
pub async fn sign_in(client: &OAuthClient, cancel: &CancellationToken) -> AppResult<Authorization> {
    let http = api::http_client()?;
    oauth::sign_in(&http, client, open_in_browser, cancel).await
}

fn open_in_browser(url: &str) -> AppResult<()> {
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url).spawn().map_err(|error| {
        AppError::new(
            ErrorKind::Io,
            format!("Could not open the browser to sign in: {error}"),
        )
    })?;
    Ok(())
}

/// Opens a session's file system, returning it with the fingerprint of the certificate the
/// server proved it holds when the user names the server.
pub async fn connect(
    session_id: &str,
    profile: &ConnectProfile,
    login: Login<'_>,
    events: &Events,
) -> AppResult<(Arc<dyn RemoteFileSystem>, String)> {
    profile.validate()?;
    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("Connecting to {}", profile.label()),
    );
    let (files, fingerprint): (Arc<dyn RemoteFileSystem>, String) = match profile.protocol {
        Protocol::WebDav | Protocol::WebDavs => {
            let (files, fingerprint) =
                webdav::WebDavFs::connect(session_id, profile, &login, events).await?;
            (Arc::new(files), fingerprint)
        }
        Protocol::S3 => {
            let (files, fingerprint) =
                s3::S3Fs::connect(session_id, profile, &login, events).await?;
            (Arc::new(files), fingerprint)
        }
        Protocol::BackblazeB2 => (Arc::new(b2::B2Fs::connect(profile).await?), String::new()),
        _ => (connect_oauth(profile, login.rotation).await?, String::new()),
    };
    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("{} session ready", profile.protocol.display_name()),
    );
    Ok((files, fingerprint))
}

async fn connect_oauth(
    profile: &ConnectProfile,
    rotation: Option<TokenRotation>,
) -> AppResult<Arc<dyn RemoteFileSystem>> {
    let provider = CloudProvider::for_protocol(profile.protocol)
        .ok_or_else(|| AppError::invalid("Not a cloud storage connection"))?;
    let AuthMethod::OAuth {
        refresh_token,
        client,
        ..
    } = &profile.auth
    else {
        return Err(AppError::invalid(format!(
            "Sign in with {} to connect",
            provider.name()
        )));
    };
    if refresh_token.is_empty() {
        return Err(AppError::new(
            ErrorKind::AuthFailed,
            format!("Sign in with {} again to connect", provider.name()),
        ));
    }
    let client = client
        .clone()
        .ok_or_else(|| oauth::missing_client(provider))?;
    let tokens = Arc::new(api::AccessTokens::new(
        api::http_client()?,
        client,
        refresh_token.clone(),
        rotation,
    ));
    let api = api::Api::new(tokens);
    Ok(match provider {
        CloudProvider::Google => Arc::new(google_drive::DriveFs::connect(api).await?),
        CloudProvider::Microsoft => Arc::new(onedrive::OneDriveFs::connect(api).await?),
        CloudProvider::Box => Arc::new(box_drive::BoxFs::connect(api).await?),
    })
}

fn entry(path: &str, name: &str, kind: EntryKind, size: u64, modified: Option<i64>) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: path.to_string(),
        kind,
        link_target: None,
        size,
        modified,
        permissions: None,
        owner: None,
        group: None,
        hidden: name.starts_with('.'),
    }
}

fn not_found(path: &str) -> AppError {
    AppError::new(
        ErrorKind::NotFound,
        format!("{} does not exist", remote_path::file_name(path)),
    )
    .with_path(path)
}

fn is_a_folder(path: &str) -> AppError {
    AppError::invalid(format!("{} is a folder", remote_path::file_name(path))).with_path(path)
}

fn not_a_folder(path: &str) -> AppError {
    AppError::invalid(format!("{} is not a folder", remote_path::file_name(path))).with_path(path)
}

fn cannot_resume(protocol: Protocol) -> AppError {
    AppError::unsupported(format!(
        "{} uploads cannot continue a partial file",
        protocol.display_name()
    ))
}

/// Folder paths resolved recently, with the service's id for each.
#[derive(Default)]
struct FolderCache(Mutex<HashMap<String, (String, Instant)>>);

impl FolderCache {
    fn get(&self, path: &str) -> Option<String> {
        let mut folders = self.0.lock().unwrap();
        match folders.get(path) {
            Some((id, stored)) if stored.elapsed() < FOLDER_CACHE_TTL => Some(id.clone()),
            Some(_) => {
                folders.remove(path);
                None
            }
            None => None,
        }
    }

    fn insert(&self, path: &str, id: &str) {
        self.0
            .lock()
            .unwrap()
            .insert(path.to_string(), (id.to_string(), Instant::now()));
    }

    /// Forgets `path` and everything inside it, after it was renamed or deleted.
    fn forget(&self, path: &str) {
        let prefix = format!("{}/", path.trim_end_matches('/'));
        self.0
            .lock()
            .unwrap()
            .retain(|cached, _| cached != path && !cached.starts_with(&prefix));
    }
}

/// The first `start..end` bytes a file read can serve, given the file's size.
fn clamp_range(range: &std::ops::Range<u64>, size: u64) -> (u64, u64) {
    (range.start.min(size), range.end.min(size))
}

/// The `Range` header asking for `start..end` of a file `size` bytes long, or `None` for the
/// whole file.
fn range_header(start: u64, end: u64, size: u64) -> Option<String> {
    (start > 0 || end < size).then(|| format!("bytes={start}-{}", end - 1))
}

/// A download, read as the response body arrives.
struct HttpReader {
    response: Option<reqwest::Response>,
    /// Bytes at the start of the body that were not asked for, from a server that ignored the
    /// requested range.
    skip: u64,
    remaining: u64,
}

impl HttpReader {
    fn new(response: Option<reqwest::Response>, length: u64) -> Self {
        Self {
            response,
            skip: 0,
            remaining: length,
        }
    }

    /// Reads `length` bytes from `start` of a response that holds the whole file.
    fn skipping(response: reqwest::Response, start: u64, length: u64) -> Self {
        Self {
            response: Some(response),
            skip: start,
            remaining: length,
        }
    }
}

#[async_trait]
impl ReadStream for HttpReader {
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
        let Some(response) = self.response.as_mut() else {
            return Ok(None);
        };
        if self.remaining == 0 {
            self.response = None;
            return Ok(None);
        }
        while self.skip > 0 {
            let Some(chunk) = api::next_chunk(response).await? else {
                self.response = None;
                return Ok(None);
            };
            let skipped = (chunk.len() as u64).min(self.skip);
            self.skip -= skipped;
            if (skipped as usize) < chunk.len() {
                let mut rest = chunk.slice(skipped as usize..);
                rest.truncate(
                    rest.len()
                        .min(usize::try_from(self.remaining).unwrap_or(usize::MAX)),
                );
                self.remaining -= rest.len() as u64;
                return Ok(Some(rest));
            }
        }
        match api::next_chunk(response).await? {
            Some(mut chunk) => {
                if chunk.len() as u64 > self.remaining {
                    chunk.truncate(self.remaining as usize);
                }
                self.remaining -= chunk.len() as u64;
                Ok(Some(chunk))
            }
            None => {
                self.response = None;
                Ok(None)
            }
        }
    }

    async fn finish(self: Box<Self>) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_cache_forgets_whole_subtrees() {
        let cache = FolderCache::default();
        cache.insert("/a", "1");
        cache.insert("/a/b", "2");
        cache.insert("/ab", "3");
        cache.forget("/a");
        assert_eq!(cache.get("/a"), None);
        assert_eq!(cache.get("/a/b"), None);
        assert_eq!(cache.get("/ab").as_deref(), Some("3"));
    }

    #[test]
    fn asks_for_ranges_only_when_needed() {
        assert_eq!(range_header(0, 100, 100), None);
        assert_eq!(range_header(10, 100, 100).as_deref(), Some("bytes=10-99"));
        assert_eq!(range_header(0, 50, 100).as_deref(), Some("bytes=0-49"));
        assert_eq!(clamp_range(&(10..u64::MAX), 100), (10, 100));
    }
}
