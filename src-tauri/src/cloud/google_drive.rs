//! Google Drive over the Drive v3 API. Drive addresses files by id and lets a folder hold
//! several files of the same name, so paths are resolved one folder at a time (with recent
//! folders cached) and the most recently changed file wins a name clash, folders first.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::{BufMut, Bytes, BytesMut};
use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE, LOCATION, RANGE};
use reqwest::{Method, Response, StatusCode};
use serde::Deserialize;
use serde_json::json;

use super::api::Api;
use super::upload::{content_range, ChunkedUpload, UploadTarget};
use super::{FolderCache, HttpReader};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{DirListing, EntryKind, FileEntry};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;
use crate::timestamp;

const FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const FOLDER_TYPE: &str = "application/vnd.google-apps.folder";
/// Google Docs, Sheets, shortcuts and the like: Drive items without file content.
const NATIVE_TYPE_PREFIX: &str = "application/vnd.google-apps.";
const FILE_FIELDS: &str = "id,name,mimeType,size,modifiedTime";
const LIST_FIELDS: &str = "nextPageToken,files(id,name,mimeType,size,modifiedTime)";
/// Folders before files, then the most recently changed, so a name clash always resolves to
/// the same item.
const ORDER: &str = "folder,modifiedTime desc";
const PAGE_SIZE: &str = "1000";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    name: String,
    mime_type: String,
    /// Drive sends 64-bit numbers as strings.
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    modified_time: Option<String>,
}

impl DriveFile {
    fn is_folder(&self) -> bool {
        self.mime_type == FOLDER_TYPE
    }

    fn is_native(&self) -> bool {
        !self.is_folder() && self.mime_type.starts_with(NATIVE_TYPE_PREFIX)
    }

    fn size(&self) -> u64 {
        self.size
            .as_deref()
            .and_then(|size| size.parse().ok())
            .unwrap_or(0)
    }

    fn modified(&self) -> Option<i64> {
        self.modified_time
            .as_deref()
            .and_then(timestamp::parse_rfc3339)
    }

    fn kind(&self) -> EntryKind {
        if self.is_folder() {
            EntryKind::Dir
        } else if self.is_native() {
            EntryKind::Other
        } else {
            EntryKind::File
        }
    }

    fn stat(&self) -> RemoteStat {
        RemoteStat {
            is_dir: self.is_folder(),
            size: self.size(),
            modified: self.modified(),
            permissions: None,
        }
    }

    fn entry(&self, path: &str) -> FileEntry {
        super::entry(path, &self.name, self.kind(), self.size(), self.modified())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    #[serde(default)]
    next_page_token: Option<String>,
}

/// Quotes a value for a Drive search query.
fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

pub struct DriveFs {
    api: Api,
    root: DriveFile,
    folders: FolderCache,
    /// Parallel uploads would otherwise each create a missing folder, and Drive would keep
    /// every copy.
    folder_creation: tokio::sync::Mutex<()>,
}

impl DriveFs {
    pub async fn connect(api: Api) -> AppResult<Self> {
        let root: DriveFile = api
            .json(&|http| {
                http.get(format!("{FILES_URL}/root"))
                    .query(&[("fields", FILE_FIELDS)])
            })
            .await?;
        Ok(Self {
            api,
            root,
            folders: FolderCache::default(),
            folder_creation: tokio::sync::Mutex::new(()),
        })
    }

    async fn child(&self, parent_id: &str, name: &str) -> AppResult<Option<DriveFile>> {
        let query = format!(
            "{} in parents and name = {} and trashed = false",
            quoted(parent_id),
            quoted(name)
        );
        let list: FileList = self
            .api
            .json(&|http| {
                http.get(FILES_URL).query(&[
                    ("q", query.as_str()),
                    ("fields", LIST_FIELDS),
                    ("orderBy", ORDER),
                    ("pageSize", "10"),
                ])
            })
            .await?;
        Ok(list.files.into_iter().next())
    }

    /// The id of the folder at `path`, or `None` when no folder is there.
    async fn folder_id(&self, path: &str) -> AppResult<Option<String>> {
        let path = protocol::normalize(path);
        if path == "/" {
            return Ok(Some(self.root.id.clone()));
        }
        if let Some(id) = self.folders.get(&path) {
            return Ok(Some(id));
        }
        let names: Vec<&str> = path.split('/').filter(|name| !name.is_empty()).collect();
        let (mut depth, mut id) = (0, self.root.id.clone());
        for known in (1..names.len()).rev() {
            if let Some(cached) = self.folders.get(&format!("/{}", names[..known].join("/"))) {
                (depth, id) = (known, cached);
                break;
            }
        }
        for walked in depth..names.len() {
            match self.child(&id, names[walked]).await? {
                Some(folder) if folder.is_folder() => {
                    self.folders
                        .insert(&format!("/{}", names[..=walked].join("/")), &folder.id);
                    id = folder.id;
                }
                _ => return Ok(None),
            }
        }
        Ok(Some(id))
    }

    async fn lookup(&self, path: &str) -> AppResult<Option<DriveFile>> {
        let path = protocol::normalize(path);
        let Some(parent) = remote_path::parent(&path) else {
            return Ok(Some(self.root.clone()));
        };
        let Some(parent_id) = self.folder_id(&parent).await? else {
            return Ok(None);
        };
        let found = self
            .child(&parent_id, remote_path::file_name(&path))
            .await?;
        if let Some(folder) = found.as_ref().filter(|file| file.is_folder()) {
            self.folders.insert(&path, &folder.id);
        }
        Ok(found)
    }

    async fn existing(&self, path: &str) -> AppResult<DriveFile> {
        self.lookup(path)
            .await?
            .ok_or_else(|| super::not_found(path))
    }

    async fn create_folder(&self, parent_id: &str, name: &str) -> AppResult<DriveFile> {
        let metadata = json!({ "name": name, "mimeType": FOLDER_TYPE, "parents": [parent_id] });
        self.api
            .json(&|http| {
                http.post(FILES_URL)
                    .query(&[("fields", FILE_FIELDS)])
                    .json(&metadata)
            })
            .await
    }

    async fn update(&self, id: &str, metadata: serde_json::Value) -> AppResult<()> {
        self.api
            .send(&|http| {
                http.patch(format!("{FILES_URL}/{id}"))
                    .query(&[("fields", "id")])
                    .json(&metadata)
            })
            .await?;
        Ok(())
    }
}

#[async_trait]
impl RemoteFileSystem for DriveFs {
    fn protocol(&self) -> Protocol {
        Protocol::GoogleDrive
    }

    fn home(&self) -> &str {
        "/"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            ranged_reads: true,
            resumable_writes: false,
        }
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        let path = protocol::normalize(&self.resolve(path));
        match self.folder_id(&path).await? {
            Some(_) => Ok(path),
            None => Err(super::not_found(&path)),
        }
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let directory = protocol::normalize(&self.resolve(path));
        let Some(folder_id) = self.folder_id(&directory).await? else {
            return Err(match self.lookup(&directory).await? {
                Some(_) => super::not_a_folder(&directory),
                None => super::not_found(&directory),
            });
        };
        let query = format!("{} in parents and trashed = false", quoted(&folder_id));
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        let mut page_token: Option<String> = None;
        loop {
            let list: FileList = self
                .api
                .json(&|http| {
                    let request = http.get(FILES_URL).query(&[
                        ("q", query.as_str()),
                        ("fields", LIST_FIELDS),
                        ("orderBy", ORDER),
                        ("pageSize", PAGE_SIZE),
                    ]);
                    match &page_token {
                        Some(token) => request.query(&[("pageToken", token.as_str())]),
                        None => request,
                    }
                })
                .await?;
            for file in list.files {
                // A slash cannot be part of a path name, and only the first of several files
                // with one name can be reached by path.
                if file.name.contains('/') || !seen.insert(file.name.clone()) {
                    continue;
                }
                let path = remote_path::join(&directory, &file.name);
                if file.is_folder() {
                    self.folders.insert(&path, &file.id);
                }
                entries.push(file.entry(&path));
            }
            match list.next_page_token.filter(|token| !token.is_empty()) {
                Some(token) => page_token = Some(token),
                None => break,
            }
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
        let _creating = self.folder_creation.lock().await;
        let parent_id = self
            .folder_id(&parent)
            .await?
            .ok_or_else(|| super::not_found(&parent))?;
        if self.child(&parent_id, name).await?.is_some() {
            return Err(protocol::already_exists(&path));
        }
        let folder = self.create_folder(&parent_id, name).await?;
        self.folders.insert(&path, &folder.id);
        Ok(path)
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let path = protocol::normalize(&self.resolve(path));
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root folder"))?;
        let target = remote_path::join(&parent, new_name);
        let file = self.existing(&path).await?;
        let parent_id = self
            .folder_id(&parent)
            .await?
            .ok_or_else(|| super::not_found(&parent))?;
        if self.child(&parent_id, new_name).await?.is_some() {
            return Err(protocol::already_exists(&target));
        }
        self.update(&file.id, json!({ "name": new_name })).await?;
        self.folders.forget(&path);
        Ok(target)
    }

    /// Moves the items to the Drive trash, where they can be restored for 30 days.
    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        for path in protocol::outermost(&resolved) {
            if path == "/" {
                return Err(AppError::invalid("Refusing to delete /"));
            }
            let file = self.existing(&path).await?;
            self.update(&file.id, json!({ "trashed": true })).await?;
            self.folders.forget(&path);
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        Ok(self
            .lookup(&self.resolve(path))
            .await?
            .map(|file| file.stat()))
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" || self.folders.get(&path).is_some() {
            return Ok(());
        }
        let _creating = self.folder_creation.lock().await;
        let mut current = "/".to_string();
        let mut id = self.root.id.clone();
        for name in path.split('/').filter(|name| !name.is_empty()) {
            current = remote_path::join(&current, name);
            if let Some(cached) = self.folders.get(&current) {
                id = cached;
                continue;
            }
            let folder = match self.child(&id, name).await? {
                Some(existing) if existing.is_folder() => existing,
                Some(_) => return Err(protocol::not_a_folder(&current)),
                None => self.create_folder(&id, name).await?,
            };
            self.folders.insert(&current, &folder.id);
            id = folder.id;
        }
        Ok(())
    }

    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        _permissions: Option<u32>,
    ) -> AppResult<()> {
        let Some(modified) = modified else {
            return Ok(());
        };
        let file = self.existing(&self.resolve(path)).await?;
        self.update(
            &file.id,
            json!({ "modifiedTime": timestamp::format_rfc3339(modified) }),
        )
        .await
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let file = self.existing(path).await?;
        if file.is_folder() {
            return Err(super::is_a_folder(path));
        }
        if file.is_native() {
            return Err(AppError::unsupported(format!(
                "{} is a Google Docs file, which has no file content to download; export it from Google Drive instead",
                file.name
            ))
            .with_path(path));
        }
        let size = file.size();
        let (start, end) = super::clamp_range(&range, size);
        if start >= end {
            return Ok(Box::new(HttpReader::new(None, 0)));
        }
        let requested_range = super::range_header(start, end, size);
        let response = self
            .api
            .send(&|http| {
                let request = http
                    .get(format!("{FILES_URL}/{}", file.id))
                    .query(&[("alt", "media")]);
                match &requested_range {
                    Some(value) => request.header(RANGE, value),
                    None => request,
                }
            })
            .await?;
        Ok(Box::new(HttpReader::new(Some(response), end - start)))
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        if request.offset > 0 {
            return Err(super::cannot_resume(Protocol::GoogleDrive));
        }
        let path = protocol::normalize(&request.path);
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot write to the root folder"))?;
        let name = remote_path::file_name(&path).to_string();
        let parent_id = self
            .folder_id(&parent)
            .await?
            .ok_or_else(|| super::not_found(&parent))?;
        let existing = self.child(&parent_id, &name).await?;
        if existing.as_ref().is_some_and(DriveFile::is_folder) {
            return Err(super::is_a_folder(&path));
        }
        let mut metadata = serde_json::Map::new();
        if existing.is_none() {
            metadata.insert("name".into(), json!(name));
            metadata.insert("parents".into(), json!([parent_id]));
        }
        if let Some(modified) = request.modified {
            metadata.insert(
                "modifiedTime".into(),
                json!(timestamp::format_rfc3339(modified)),
            );
        }
        let target = DriveUpload {
            api: self.api.clone(),
            existing_id: existing.map(|file| file.id),
            metadata: serde_json::Value::Object(metadata),
            size: request.size,
            session: None,
        };
        Ok(Box::new(ChunkedUpload::new(target, request.size)))
    }
}

/// Creates a new file, or replaces the content of the one already at the path so it keeps its
/// id, sharing and history.
struct DriveUpload {
    api: Api,
    existing_id: Option<String>,
    metadata: serde_json::Value,
    size: u64,
    session: Option<String>,
}

impl DriveUpload {
    fn method_and_url(&self) -> (Method, String) {
        match &self.existing_id {
            Some(id) => (Method::PATCH, format!("{UPLOAD_URL}/{id}")),
            None => (Method::POST, UPLOAD_URL.to_string()),
        }
    }
}

#[async_trait]
impl UploadTarget for DriveUpload {
    async fn upload_whole(&mut self, data: Bytes) -> AppResult<()> {
        let boundary = format!("poros-{}", uuid::Uuid::new_v4().simple());
        let metadata = serde_json::to_vec(&self.metadata)
            .map_err(|error| AppError::invalid(error.to_string()))?;
        let mut body = BytesMut::with_capacity(data.len() + metadata.len() + 256);
        body.put_slice(
            format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n")
                .as_bytes(),
        );
        body.put_slice(&metadata);
        body.put_slice(
            format!("\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n")
                .as_bytes(),
        );
        body.put_slice(&data);
        body.put_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let body = body.freeze();
        let content_type = format!("multipart/related; boundary={boundary}");
        let (method, url) = self.method_and_url();
        self.api
            .send(&|http| {
                http.request(method.clone(), &url)
                    .query(&[("uploadType", "multipart"), ("fields", "id")])
                    .header(CONTENT_TYPE, &content_type)
                    .body(body.clone())
            })
            .await?;
        Ok(())
    }

    async fn start_session(&mut self) -> AppResult<()> {
        let (method, url) = self.method_and_url();
        let size = self.size.to_string();
        let response = self
            .api
            .send(&|http| {
                http.request(method.clone(), &url)
                    .query(&[("uploadType", "resumable"), ("fields", "id")])
                    .header("X-Upload-Content-Type", "application/octet-stream")
                    .header("X-Upload-Content-Length", &size)
                    .json(&self.metadata)
            })
            .await?;
        let session = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::Cloud,
                    "Google Drive did not open an upload session",
                )
            })?;
        self.session = Some(session.to_string());
        Ok(())
    }

    async fn send_chunk(&mut self, offset: u64, data: Bytes, last: bool) -> AppResult<()> {
        let session = self
            .session
            .clone()
            .ok_or_else(|| AppError::new(ErrorKind::Cloud, "No upload session is open"))?;
        let (mut offset, mut data) = (offset, data);
        loop {
            let end = offset + data.len() as u64;
            let range = content_range(offset, data.len() as u64, last.then_some(end));
            let response = self
                .api
                .send(&|http| {
                    http.put(&session)
                        .header(CONTENT_RANGE, &range)
                        .body(data.clone())
                })
                .await?;
            if response.status() != StatusCode::PERMANENT_REDIRECT {
                return Ok(());
            }
            let received = received_bytes(&response);
            if received >= end {
                return match last {
                    true => Err(AppError::new(
                        ErrorKind::Cloud,
                        "Google Drive did not complete the upload",
                    )),
                    false => Ok(()),
                };
            }
            if received < offset {
                return Err(AppError::new(
                    ErrorKind::Cloud,
                    "Google Drive lost part of the upload",
                ));
            }
            data = data.slice((received - offset) as usize..);
            offset = received;
        }
    }
}

/// How much of an upload Google has, from the `Range: bytes=0-N` of a 308 answer.
fn received_bytes(response: &Response) -> u64 {
    response
        .headers()
        .get(RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit('-').next())
        .and_then(|last| last.trim().parse::<u64>().ok())
        .map_or(0, |last| last + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_search_values() {
        assert_eq!(quoted("O'Brien"), r"'O\'Brien'");
        assert_eq!(quoted(r"a\b"), r"'a\\b'");
    }

    #[test]
    fn reads_drive_files() {
        let file: DriveFile = serde_json::from_str(
            r#"{"id":"1","name":"a.txt","mimeType":"text/plain","size":"42","modifiedTime":"2024-05-01T10:00:00.000Z"}"#,
        )
        .unwrap();
        assert_eq!(file.size(), 42);
        assert_eq!(file.kind(), EntryKind::File);
        assert!(file.modified().is_some());
        let document: DriveFile = serde_json::from_str(
            r#"{"id":"2","name":"Notes","mimeType":"application/vnd.google-apps.document"}"#,
        )
        .unwrap();
        assert!(document.is_native());
        assert_eq!(document.kind(), EntryKind::Other);
    }
}
