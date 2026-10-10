//! Google Drive and OneDrive as remote file systems, over their REST APIs. Accounts sign in in
//! the browser (OAuth 2.0 with PKCE and a loopback redirect); the refresh token is the
//! connection's secret, kept in the system keychain like a password.

mod api;
pub mod google_drive;
pub mod oauth;
pub mod onedrive;
mod upload;

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
use crate::ssh::{AuthMethod, ConnectProfile};
pub use oauth::{missing_client, Authorization, OAuthClient, OAuthVault, ProviderStatus, SignedIn};

/// How long a folder's id is trusted without asking the service again. Short, because the
/// folder can be renamed or removed from another device.
const FOLDER_CACHE_TTL: Duration = Duration::from_secs(60);

/// Receives a refresh token that replaced the one the session started with.
pub type TokenRotation = Arc<dyn Fn(&str) + Send + Sync>;

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudProvider {
    Google,
    Microsoft,
}

impl CloudProvider {
    pub fn for_protocol(protocol: Protocol) -> Option<Self> {
        match protocol {
            Protocol::GoogleDrive => Some(Self::Google),
            Protocol::OneDrive => Some(Self::Microsoft),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Microsoft => "Microsoft",
        }
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

pub async fn connect(
    session_id: &str,
    profile: &ConnectProfile,
    rotation: Option<TokenRotation>,
    events: &Events,
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
    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("Connecting to {}", profile.protocol.display_name()),
    );
    let tokens = Arc::new(api::AccessTokens::new(
        api::http_client()?,
        client,
        refresh_token.clone(),
        rotation,
    ));
    let api = api::Api::new(tokens);
    let files: Arc<dyn RemoteFileSystem> = match provider {
        CloudProvider::Google => Arc::new(google_drive::DriveFs::connect(api).await?),
        CloudProvider::Microsoft => Arc::new(onedrive::OneDriveFs::connect(api).await?),
    };
    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("{} session ready", profile.protocol.display_name()),
    );
    Ok(files)
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
    remaining: u64,
}

impl HttpReader {
    fn new(response: Option<reqwest::Response>, length: u64) -> Self {
        Self {
            response,
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
