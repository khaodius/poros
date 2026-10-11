//! WebDAV (RFC 4918) over HTTP or HTTPS: Nextcloud, ownCloud, Apache, nginx, IIS, NAS boxes
//! and the like. Folders are listed with `PROPFIND`, files move with `GET` and `PUT`, and the
//! server's own `MKCOL`, `MOVE` and `DELETE` do the rest. A server that asks for Basic or Digest
//! sign-in gets it, and an HTTPS server whose certificate the system does not trust can be
//! approved like an FTPS one.

mod auth;
mod multistatus;

use std::convert::Infallible;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use percent_encoding::percent_decode_str;
use reqwest::header::{HeaderName, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, RANGE};
use reqwest::{Body, Client, Method, Response, StatusCode, Url};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::api::{self, STALL_TIMEOUT};
use super::upload::{ChunkedUpload, UploadTarget};
use super::{FolderCache, HttpReader, Login};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::Events;
use crate::model::{DirListing, EntryKind};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;
use crate::ssh::ConnectProfile;
use crate::tls::ServerTls;
use auth::Authenticator;
use multistatus::DavItem;

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/><d:getlastmodified/></d:prop></d:propfind>"#;
/// Request bodies of streamed uploads go out in pieces this size, each taken once the
/// connection has room for it, which is how a slow upload shows that it is still moving.
const BODY_PIECE: usize = 256 * 1024;
/// Pieces waiting for the connection.
const BODY_QUEUE: usize = 4;
/// How long the server may take to answer once a streamed upload is in, which can include
/// assembling, scanning or encrypting a large file.
const FINISH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Nextcloud and ownCloud set a file's modification time from this upload header.
const MTIME_HEADER: &str = "x-oc-mtime";

fn method(name: &'static str) -> Method {
    Method::from_bytes(name.as_bytes()).expect("a WebDAV method name")
}

/// A request, kept so it can be rebuilt for a retry or a sign-in.
struct DavRequest {
    method: Method,
    url: Url,
    headers: Vec<(HeaderName, String)>,
    body: Option<Bytes>,
}

impl DavRequest {
    fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            headers: Vec::new(),
            body: None,
        }
    }

    fn header(mut self, name: HeaderName, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }

    fn xml(self, body: &str) -> Self {
        let mut request = self.header(CONTENT_TYPE, "application/xml; charset=utf-8");
        request.body = Some(Bytes::copy_from_slice(body.as_bytes()));
        request
    }
}

/// The outcome of creating one folder.
enum Created {
    Made,
    /// Something already has the name.
    Exists,
    ParentMissing,
}

/// The server's address, and the folder a URL typed as the host points into.
fn endpoint(profile: &ConnectProfile) -> AppResult<(Url, Option<String>)> {
    let typed = profile.host.trim();
    let without_scheme = typed.split_once("://").map_or(typed, |(_, rest)| rest);
    let (authority, path) = match without_scheme.find('/') {
        Some(slash) => without_scheme.split_at(slash),
        None => (without_scheme, ""),
    };
    let scheme = match profile.protocol {
        Protocol::WebDav => "http",
        _ => "https",
    };
    let mut url = Url::parse(&format!("{scheme}://{authority}/"))
        .ok()
        .filter(|url| url.host_str().is_some_and(|host| !host.is_empty()))
        .ok_or_else(|| AppError::invalid(format!("{typed} is not a valid server address")))?;
    if url.port().is_none() {
        let _ = url.set_port(Some(profile.port));
    }
    let folder = percent_decode_str(path).decode_utf8_lossy();
    let folder = protocol::normalize(&folder);
    Ok((url, (folder != "/").then_some(folder)))
}

pub struct WebDavFs {
    protocol: Protocol,
    http: Client,
    base: Url,
    home: String,
    auth: Authenticator,
    tls: Option<ServerTls>,
    folders: FolderCache,
    folder_creation: tokio::sync::Mutex<()>,
    /// The server refused to set a modification time once, so it is not asked again.
    times_refused: AtomicBool,
}

impl WebDavFs {
    pub async fn connect(
        session_id: &str,
        profile: &ConnectProfile,
        login: &Login<'_>,
        events: &Events,
    ) -> AppResult<(Self, String)> {
        let (base, folder_in_address) = endpoint(profile)?;
        let tls = match profile.protocol {
            Protocol::WebDavs => Some(login.server_tls(
                base.host_str().unwrap_or_default(),
                base.port_or_known_default().unwrap_or(443),
                session_id,
                events,
            )?),
            _ => None,
        };
        let http = match &tls {
            Some(tls) => api::approving_client(tls)?,
            None => api::http_client()?,
        };
        let password = super::password_of(profile)?;
        let start = profile
            .initial_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| protocol::normalize(&protocol::resolve_against("/", path)))
            .or(folder_in_address)
            .unwrap_or_else(|| "/".to_string());
        let mut files = Self {
            protocol: profile.protocol,
            http,
            base,
            home: "/".to_string(),
            auth: Authenticator::new(profile.username.trim(), &password),
            tls,
            folders: FolderCache::default(),
            folder_creation: tokio::sync::Mutex::new(()),
            times_refused: AtomicBool::new(false),
        };
        // The first request also learns how the server wants the sign-in, and checks it. A
        // remote folder that is gone leaves the session at the top, if that is a folder.
        let found = |item: Option<DavItem>| item.is_some_and(|item| item.is_dir);
        if found(files.item(&start).await?) {
            files.home = start;
        } else if start == "/" || !found(files.item("/").await?) {
            return Err(AppError::new(
                ErrorKind::NotFound,
                format!(
                    "The server has no WebDAV folder at {start}; check the address and the remote folder"
                ),
            ));
        }
        let fingerprint = files
            .tls
            .as_ref()
            .and_then(ServerTls::accepted_fingerprint)
            .unwrap_or_default();
        Ok((files, fingerprint))
    }

    fn url(&self, path: &str, is_dir: bool) -> Url {
        let mut encoded = super::encode_path(&protocol::normalize(path));
        if encoded.is_empty() || is_dir {
            encoded.push('/');
        }
        let mut url = self.base.clone();
        url.set_path(&encoded);
        url
    }

    /// Sends a request, signing in when the server asks and retrying what is safe to retry.
    async fn send(&self, request: &DavRequest) -> AppResult<Response> {
        let mut signed_in = false;
        loop {
            let build = || {
                let mut builder = self
                    .http
                    .request(request.method.clone(), request.url.clone());
                for (name, value) in &request.headers {
                    builder = builder.header(name, value);
                }
                if let Some(body) = &request.body {
                    builder = builder.body(body.clone());
                }
                if let Some(authorization) = self.auth.header(&request.method, &request.url) {
                    builder = builder.header(AUTHORIZATION, authorization);
                }
                builder.build().map_err(api::transport_error)
            };
            let response = api::send_retrying(&self.http, &build)
                .await
                .map_err(|error| super::certificate_rejection(error, self.tls.as_ref()))?;
            if response.status() == StatusCode::UNAUTHORIZED
                && !signed_in
                && self.auth.challenge(&response)?
            {
                signed_in = true;
                continue;
            }
            return Ok(response);
        }
    }

    /// `PROPFIND` at `depth`; `None` when nothing is at the path.
    async fn propfind(&self, path: &str, depth: u8) -> AppResult<Option<Vec<DavItem>>> {
        let request = DavRequest::new(method("PROPFIND"), self.url(path, false))
            .header(HeaderName::from_static("depth"), depth.to_string())
            .xml(PROPFIND_BODY);
        let response = self.send(&request).await?;
        match response.status() {
            StatusCode::MULTI_STATUS => {
                let body = api::body_bytes(response).await?;
                multistatus::parse_listing(&body).map(Some)
            }
            StatusCode::NOT_FOUND | StatusCode::GONE => Ok(None),
            status if status.is_success() => Err(AppError::invalid(
                "The server did not answer as a WebDAV server; check the address and the remote folder",
            )),
            _ => Err(failure(response, path).await),
        }
    }

    async fn item(&self, path: &str) -> AppResult<Option<DavItem>> {
        let path = protocol::normalize(path);
        let Some(items) = self.propfind(&path, 0).await? else {
            return Ok(None);
        };
        let item = items
            .into_iter()
            .next()
            .map(|item| DavItem { path, ..item });
        if let Some(item) = item.as_ref().filter(|item| item.is_dir) {
            self.folders.insert(&item.path, "");
        }
        Ok(item)
    }

    async fn existing(&self, path: &str) -> AppResult<DavItem> {
        self.item(path).await?.ok_or_else(|| super::not_found(path))
    }

    async fn create_folder(&self, path: &str) -> AppResult<Created> {
        let request = DavRequest::new(method("MKCOL"), self.url(path, true));
        let response = self.send(&request).await?;
        match response.status() {
            status if status.is_success() => {
                self.folders.insert(path, "");
                Ok(Created::Made)
            }
            StatusCode::METHOD_NOT_ALLOWED => Ok(Created::Exists),
            StatusCode::CONFLICT => Ok(Created::ParentMissing),
            _ => Err(failure(response, path).await),
        }
    }
}

/// An error for a failed answer, with the server's own explanation when it sent one.
async fn failure(response: Response, path: &str) -> AppError {
    let status = response.status();
    let explanation = match api::body_bytes(response).await {
        Ok(body) => multistatus::error_message(&body),
        Err(_) => None,
    };
    let name = remote_path::file_name(path);
    let (kind, message) = match status {
        StatusCode::UNAUTHORIZED => (
            ErrorKind::AuthFailed,
            "The server did not accept the username or password".to_string(),
        ),
        StatusCode::FORBIDDEN => (
            ErrorKind::PermissionDenied,
            format!("The server does not allow this for {name}"),
        ),
        StatusCode::NOT_FOUND | StatusCode::GONE => return super::not_found(path),
        StatusCode::PRECONDITION_FAILED => return protocol::already_exists(path),
        StatusCode::LOCKED => (
            ErrorKind::PermissionDenied,
            format!("{name} is locked by another user"),
        ),
        StatusCode::INSUFFICIENT_STORAGE => (
            ErrorKind::PermissionDenied,
            "The server is out of storage space".to_string(),
        ),
        StatusCode::PAYLOAD_TOO_LARGE => (
            ErrorKind::PermissionDenied,
            format!("The server does not take files as large as {name}"),
        ),
        status if status.is_server_error() => {
            (ErrorKind::Cloud, format!("The server answered {status}"))
        }
        status => (
            ErrorKind::InvalidInput,
            format!("The server answered {status}"),
        ),
    };
    let message = match explanation {
        Some(explanation) => format!("{message}: {explanation}"),
        None => message,
    };
    AppError::new(kind, message).with_path(path)
}

#[async_trait]
impl RemoteFileSystem for WebDavFs {
    fn protocol(&self) -> Protocol {
        self.protocol
    }

    fn home(&self) -> &str {
        &self.home
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            ranged_reads: true,
            resumable_writes: false,
        }
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        let path = protocol::normalize(&self.resolve(path));
        match self.item(&path).await? {
            Some(item) if item.is_dir => Ok(path),
            Some(_) => Err(super::not_a_folder(&path)),
            None => Err(super::not_found(&path)),
        }
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let directory = protocol::normalize(&self.resolve(path));
        let items = self
            .propfind(&directory, 1)
            .await?
            .ok_or_else(|| super::not_found(&directory))?;
        let mut entries = Vec::with_capacity(items.len());
        for item in items {
            // The folder itself, which the server may spell with other escapes or case.
            if item.path.eq_ignore_ascii_case(&directory) {
                if !item.is_dir {
                    return Err(super::not_a_folder(&directory));
                }
                continue;
            }
            let name = remote_path::file_name(&item.path);
            if name.is_empty() {
                continue;
            }
            let path = remote_path::join(&directory, name);
            let kind = if item.is_dir {
                self.folders.insert(&path, "");
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            entries.push(super::entry(&path, name, kind, item.size, item.modified));
        }
        Ok(DirListing {
            parent: remote_path::parent(&directory),
            path: directory,
            entries,
        })
    }

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        protocol::validate_name(name)?;
        let parent = protocol::normalize(&self.resolve(parent));
        let path = remote_path::join(&parent, name);
        match self.create_folder(&path).await? {
            Created::Made => Ok(path),
            Created::Exists => Err(protocol::already_exists(&path)),
            Created::ParentMissing => Err(super::not_found(&parent)),
        }
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let path = protocol::normalize(&self.resolve(path));
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root folder"))?;
        let target = remote_path::join(&parent, new_name);
        let item = self.existing(&path).await?;
        let request = DavRequest::new(method("MOVE"), self.url(&path, item.is_dir))
            .header(
                HeaderName::from_static("destination"),
                self.url(&target, item.is_dir).as_str(),
            )
            .header(HeaderName::from_static("overwrite"), "F");
        let response = self.send(&request).await?;
        match response.status() {
            status if status.is_success() => {
                self.folders.forget(&path);
                Ok(target)
            }
            StatusCode::PRECONDITION_FAILED => Err(protocol::already_exists(&target)),
            _ => Err(failure(response, &path).await),
        }
    }

    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        for path in protocol::outermost(&resolved) {
            if path == "/" || path == self.home {
                return Err(AppError::invalid(format!("Refusing to delete {path}")));
            }
            let item = self.existing(&path).await?;
            let request = DavRequest::new(Method::DELETE, self.url(&path, item.is_dir));
            let response = self.send(&request).await?;
            match response.status() {
                // Some of the folder's contents could not be deleted.
                StatusCode::MULTI_STATUS => {
                    return Err(AppError::new(
                        ErrorKind::PermissionDenied,
                        format!("The server kept part of {}", remote_path::file_name(&path)),
                    )
                    .with_path(path))
                }
                status if status.is_success() => self.folders.forget(&path),
                _ => return Err(failure(response, &path).await),
            }
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        Ok(self
            .item(&self.resolve(path))
            .await?
            .map(|item| RemoteStat {
                is_dir: item.is_dir,
                size: item.size,
                modified: item.modified,
                permissions: None,
            }))
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" || self.folders.get(&path).is_some() {
            return Ok(());
        }
        let _creating = self.folder_creation.lock().await;
        // From the folder itself up to the first one that exists, so nothing above the folders
        // the account can reach is ever touched.
        let mut missing = Vec::new();
        let mut current = path.clone();
        loop {
            match self.create_folder(&current).await? {
                Created::Made => break,
                Created::Exists => {
                    if !self.existing(&current).await?.is_dir {
                        return Err(protocol::not_a_folder(&current));
                    }
                    break;
                }
                Created::ParentMissing => {
                    let parent = remote_path::parent(&current)
                        .filter(|parent| parent != "/")
                        .ok_or_else(|| super::not_found(&current))?;
                    missing.push(std::mem::replace(&mut current, parent));
                }
            }
        }
        for folder in missing.iter().rev() {
            match self.create_folder(folder).await? {
                Created::Made | Created::Exists => {}
                Created::ParentMissing => return Err(super::not_found(folder)),
            }
        }
        Ok(())
    }

    /// Asks the server to keep the time as the file's `lastmodified` property, which Nextcloud
    /// and ownCloud apply to the file. A server that refuses is not asked again.
    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        _permissions: Option<u32>,
    ) -> AppResult<()> {
        let Some(modified) = modified else {
            return Ok(());
        };
        if self.times_refused.load(Ordering::Relaxed) {
            return Ok(());
        }
        let path = protocol::normalize(&self.resolve(path));
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<d:propertyupdate xmlns:d="DAV:"><d:set><d:prop><d:lastmodified>{modified}</d:lastmodified></d:prop></d:set></d:propertyupdate>"#
        );
        let request = DavRequest::new(method("PROPPATCH"), self.url(&path, false)).xml(&body);
        let response = self.send(&request).await?;
        let accepted = match response.status() {
            StatusCode::MULTI_STATUS => {
                multistatus::all_properties_set(&api::body_bytes(response).await?)
            }
            status => status.is_success(),
        };
        if accepted {
            return Ok(());
        }
        self.times_refused.store(true, Ordering::Relaxed);
        Err(AppError::unsupported(
            "The server does not let Poros set modification times",
        ))
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let path = protocol::normalize(&self.resolve(path));
        if range.start >= range.end {
            let item = self.existing(&path).await?;
            if item.is_dir {
                return Err(super::is_a_folder(&path));
            }
            return Ok(Box::new(HttpReader::new(None, 0)));
        }
        let mut request = DavRequest::new(Method::GET, self.url(&path, false));
        let length = range.end - range.start;
        if range.start > 0 || range.end != u64::MAX {
            let last = match range.end {
                u64::MAX => String::new(),
                end => (end - 1).to_string(),
            };
            request = request.header(RANGE, format!("bytes={}-{last}", range.start));
        }
        let response = self.send(&request).await?;
        match response.status() {
            StatusCode::PARTIAL_CONTENT => Ok(Box::new(HttpReader::new(Some(response), length))),
            // The server sent the whole file, ignoring the range.
            StatusCode::OK => Ok(Box::new(HttpReader::skipping(
                response,
                range.start,
                length,
            ))),
            StatusCode::RANGE_NOT_SATISFIABLE => Ok(Box::new(HttpReader::new(None, 0))),
            _ => Err(failure(response, &path).await),
        }
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        if request.offset > 0 {
            return Err(super::cannot_resume(self.protocol));
        }
        let path = protocol::normalize(&request.path);
        if path == "/" {
            return Err(AppError::invalid("Cannot write to the root folder"));
        }
        let target = WebDavUpload {
            files: self,
            path,
            size: request.size,
            modified: request.modified,
            stream: None,
        };
        Ok(Box::new(ChunkedUpload::new(target, request.size)))
    }
}

/// A `PUT` whose body is still being written.
struct StreamedPut {
    pieces: Option<mpsc::Sender<Bytes>>,
    answer: JoinHandle<AppResult<Response>>,
}

/// Replaces whatever file is at the path. A small file goes in one request that can be
/// retried; a larger one streams through a single request, since WebDAV has no standard way to
/// send a file in parts.
struct WebDavUpload {
    files: Arc<WebDavFs>,
    path: String,
    size: u64,
    modified: Option<i64>,
    stream: Option<StreamedPut>,
}

impl WebDavUpload {
    fn headers(&self, length: u64) -> Vec<(HeaderName, String)> {
        let mut headers = vec![
            (CONTENT_TYPE, "application/octet-stream".to_string()),
            (CONTENT_LENGTH, length.to_string()),
        ];
        if let Some(modified) = self.modified {
            headers.push((HeaderName::from_static(MTIME_HEADER), modified.to_string()));
        }
        headers
    }

    async fn check(&self, response: Response) -> AppResult<()> {
        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::CONFLICT => Err(super::not_found(
                &remote_path::parent(&self.path).unwrap_or_default(),
            )),
            StatusCode::METHOD_NOT_ALLOWED => Err(super::is_a_folder(&self.path)),
            _ => Err(failure(response, &self.path).await),
        }
    }

    async fn stream_failure(&mut self) -> AppError {
        let Some(stream) = self.stream.take() else {
            return AppError::new(ErrorKind::Cloud, "The upload already ended");
        };
        drop(stream.pieces);
        match stream.answer.await {
            Ok(Ok(response)) => match self.check(response).await {
                Ok(()) => AppError::new(ErrorKind::Cloud, "The server ended the upload early"),
                Err(error) => error,
            },
            Ok(Err(error)) => error,
            Err(_) => AppError::new(ErrorKind::Cloud, "The upload stopped"),
        }
    }
}

impl Drop for WebDavUpload {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.answer.abort();
        }
    }
}

#[async_trait]
impl UploadTarget for WebDavUpload {
    async fn upload_whole(&mut self, data: Bytes) -> AppResult<()> {
        let mut request = DavRequest::new(Method::PUT, self.files.url(&self.path, false));
        request.headers = self.headers(data.len() as u64);
        request.body = Some(data);
        let response = self.files.send(&request).await?;
        self.check(response).await
    }

    async fn start_session(&mut self) -> AppResult<()> {
        let (sender, receiver) = mpsc::channel::<Bytes>(BODY_QUEUE);
        let pieces = futures::stream::unfold(receiver, |mut receiver| async move {
            let piece = receiver.recv().await?;
            Some((Ok::<_, Infallible>(piece), receiver))
        });
        let url = self.files.url(&self.path, false);
        let mut builder = self.files.http.put(url.clone());
        for (name, value) in self.headers(self.size) {
            builder = builder.header(name, value);
        }
        if let Some(authorization) = self.files.auth.header(&Method::PUT, &url) {
            builder = builder.header(AUTHORIZATION, authorization);
        }
        let request = builder
            .body(Body::wrap_stream(pieces))
            .build()
            .map_err(api::transport_error)?;
        let http = self.files.http.clone();
        let answer =
            tokio::spawn(async move { http.execute(request).await.map_err(api::transport_error) });
        self.stream = Some(StreamedPut {
            pieces: Some(sender),
            answer,
        });
        Ok(())
    }

    async fn send_chunk(&mut self, _offset: u64, data: Bytes, last: bool) -> AppResult<()> {
        let Some(sender) = self
            .stream
            .as_ref()
            .and_then(|stream| stream.pieces.clone())
        else {
            return Err(AppError::new(ErrorKind::Cloud, "No upload is open"));
        };
        for start in (0..data.len()).step_by(BODY_PIECE) {
            let piece = data.slice(start..(start + BODY_PIECE).min(data.len()));
            match tokio::time::timeout(STALL_TIMEOUT, sender.send(piece)).await {
                Ok(Ok(())) => {}
                // The request ended while there was still data to send.
                Ok(Err(_)) => return Err(self.stream_failure().await),
                Err(_) => {
                    return Err(AppError::new(
                        ErrorKind::Timeout,
                        "The server stopped taking the upload",
                    ))
                }
            }
        }
        drop(sender);
        if !last {
            return Ok(());
        }
        let Some(mut stream) = self.stream.take() else {
            return Err(AppError::new(ErrorKind::Cloud, "No upload is open"));
        };
        stream.pieces = None;
        let response = match tokio::time::timeout(FINISH_TIMEOUT, &mut stream.answer).await {
            Ok(Ok(result)) => result?,
            Ok(Err(_)) => return Err(AppError::new(ErrorKind::Cloud, "The upload stopped")),
            Err(_) => {
                stream.answer.abort();
                return Err(AppError::new(
                    ErrorKind::Timeout,
                    "The server did not confirm the upload in time",
                ));
            }
        };
        if response.status() == StatusCode::UNAUTHORIZED {
            // A Digest nonce went stale. The body is spent, so the queue sends the file again
            // with the fresh one.
            self.files.auth.challenge(&response)?;
            return Err(AppError::new(
                ErrorKind::Cloud,
                "The server asked to sign in again during the upload",
            ));
        }
        self.check(response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::AuthMethod;

    fn profile(protocol: Protocol, host: &str, port: u16) -> ConnectProfile {
        serde_json::from_value(serde_json::json!({
            "protocol": if protocol == Protocol::WebDav { "webDav" } else { "webDavs" },
            "host": host,
            "port": port,
            "username": "ada",
            "auth": { "type": "password", "password": "secret" },
        }))
        .unwrap()
    }

    #[test]
    fn reads_the_address_from_the_host_field() {
        let (url, folder) =
            endpoint(&profile(Protocol::WebDavs, "cloud.example.com", 443)).unwrap();
        assert_eq!(url.as_str(), "https://cloud.example.com/");
        assert_eq!(folder, None);

        let pasted = "https://cloud.example.com:8443/remote.php/dav/files/ada%20l/";
        let (url, folder) = endpoint(&profile(Protocol::WebDavs, pasted, 443)).unwrap();
        assert_eq!(url.as_str(), "https://cloud.example.com:8443/");
        assert_eq!(folder.as_deref(), Some("/remote.php/dav/files/ada l"));

        let (url, _) = endpoint(&profile(Protocol::WebDav, "nas.local", 5005)).unwrap();
        assert_eq!(url.as_str(), "http://nas.local:5005/");
        assert!(endpoint(&profile(Protocol::WebDav, "", 80)).is_err());
        assert!(matches!(
            profile(Protocol::WebDav, "x", 80).auth,
            AuthMethod::Password { .. }
        ));
    }

    #[test]
    fn escapes_paths_in_urls() {
        let files = WebDavFs {
            protocol: Protocol::WebDavs,
            http: api::http_client().unwrap(),
            base: Url::parse("https://dav.example.com:8443/").unwrap(),
            home: "/".into(),
            auth: Authenticator::new("", ""),
            tls: None,
            folders: FolderCache::default(),
            folder_creation: tokio::sync::Mutex::new(()),
            times_refused: AtomicBool::new(false),
        };
        assert_eq!(
            files.url("/a b/#1?.txt", false).as_str(),
            "https://dav.example.com:8443/a%20b/%231%3F.txt"
        );
        assert_eq!(
            files.url("/Photos", true).as_str(),
            "https://dav.example.com:8443/Photos/"
        );
        assert_eq!(
            files.url("/", false).as_str(),
            "https://dav.example.com:8443/"
        );
        assert_eq!(
            files.url("/100%", false).as_str(),
            "https://dav.example.com:8443/100%25"
        );
    }
}
