//! Box over its v2.0 API. Box addresses items by id and keeps names unique within a folder, so
//! paths are resolved one folder at a time from the folders' listings. A listing answers
//! lookups for a few seconds, so a transfer of many files into one folder lists it once
//! rather than once per file.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine;
use bytes::{BufMut, Bytes, BytesMut};
use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE, RANGE, RETRY_AFTER};
use reqwest::{Response, StatusCode};
use serde::Deserialize;
use serde_json::json;
use sha1::{Digest, Sha1};

use super::api::{self, Api};
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

const API_URL: &str = "https://api.box.com/2.0";
const UPLOAD_URL: &str = "https://upload.box.com/api/2.0";
/// "All Files", the top of every account.
const ROOT_ID: &str = "0";
const ITEM_FIELDS: &str = "type,id,name,size,modified_at,content_modified_at";
const PAGE_SIZE: &str = "1000";
/// Box takes files of 20 MB and more in parts, and smaller ones only whole.
const SINGLE_REQUEST_LIMIT: usize = 20 * 1024 * 1024;
/// How long a folder's listing answers lookups by name before Box is asked again.
const CONTENTS_TTL: Duration = Duration::from_secs(15);
/// Box may still be putting the parts of an upload together when it is committed.
const COMMIT_ATTEMPTS: u32 = 60;

#[derive(Debug, Clone, Deserialize)]
struct BoxItem {
    /// `file`, `folder` or `web_link`.
    #[serde(rename = "type")]
    item_type: String,
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    modified_at: Option<String>,
    #[serde(default)]
    content_modified_at: Option<String>,
}

impl BoxItem {
    fn root() -> Self {
        Self {
            item_type: "folder".into(),
            id: ROOT_ID.into(),
            name: String::new(),
            size: None,
            modified_at: None,
            content_modified_at: None,
        }
    }

    fn is_folder(&self) -> bool {
        self.item_type == "folder"
    }

    fn is_file(&self) -> bool {
        self.item_type == "file"
    }

    fn kind(&self) -> EntryKind {
        match self.item_type.as_str() {
            "folder" => EntryKind::Dir,
            "file" => EntryKind::File,
            _ => EntryKind::Other,
        }
    }

    /// Folders report the size of everything in them, which a listing does not show.
    fn size(&self) -> u64 {
        match self.is_file() {
            true => self.size.unwrap_or(0),
            false => 0,
        }
    }

    /// When the file itself last changed, which uploads set, rather than when Box last
    /// touched the item.
    fn modified(&self) -> Option<i64> {
        self.content_modified_at
            .as_deref()
            .or(self.modified_at.as_deref())
            .and_then(timestamp::parse_rfc3339)
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

    /// The API collection the item belongs to.
    fn collection(&self) -> &'static str {
        match self.item_type.as_str() {
            "folder" => "folders",
            "web_link" => "web_links",
            _ => "files",
        }
    }
}

#[derive(Debug, Deserialize)]
struct ItemPage {
    #[serde(default)]
    entries: Vec<BoxItem>,
    #[serde(default)]
    next_marker: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Uploaded {
    #[serde(default)]
    entries: Vec<BoxItem>,
}

#[derive(Debug, Deserialize)]
struct UploadSession {
    part_size: usize,
    session_endpoints: SessionEndpoints,
}

#[derive(Debug, Deserialize)]
struct SessionEndpoints {
    upload_part: String,
    commit: String,
    abort: String,
}

#[derive(Debug, Default, Deserialize)]
struct ConflictBody {
    #[serde(default)]
    context_info: ConflictInfo,
}

#[derive(Debug, Default, Deserialize)]
struct ConflictInfo {
    /// One item for a file upload, a list for a new folder.
    #[serde(default)]
    conflicts: serde_json::Value,
}

/// The item whose name a refused request wanted, from Box's `409 Conflict` answer.
async fn conflicting_item(response: Response) -> AppResult<Option<BoxItem>> {
    let body: ConflictBody = api::decode(response).await.unwrap_or_default();
    let item = match body.context_info.conflicts {
        serde_json::Value::Array(items) => items.into_iter().next(),
        item @ serde_json::Value::Object(_) => Some(item),
        _ => None,
    };
    Ok(item.and_then(|item| serde_json::from_value(item).ok()))
}

fn sha1_base64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha1::digest(data))
}

fn retry_after(response: &Response) -> Duration {
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map_or(Duration::from_secs(1), Duration::from_secs)
        .min(Duration::from_secs(30))
}

/// A folder's items by name, as last listed.
struct Contents {
    listed: Instant,
    items: HashMap<String, BoxItem>,
}

pub struct BoxFs {
    api: Api,
    folders: FolderCache,
    /// Listings by folder id, kept in step with this session's own changes.
    contents: Mutex<HashMap<String, Contents>>,
    /// Parallel uploads would otherwise each try to create a missing folder.
    folder_creation: tokio::sync::Mutex<()>,
}

impl BoxFs {
    pub async fn connect(api: Api) -> AppResult<Self> {
        api.send(&|http| {
            http.get(format!("{API_URL}/folders/{ROOT_ID}"))
                .query(&[("fields", "id")])
        })
        .await?;
        Ok(Self {
            api,
            folders: FolderCache::default(),
            contents: Mutex::new(HashMap::new()),
            folder_creation: tokio::sync::Mutex::new(()),
        })
    }

    /// Lists a folder from Box, and keeps the listing for lookups.
    async fn list_folder(&self, folder_id: &str, path: &str) -> AppResult<Vec<BoxItem>> {
        let url = format!("{API_URL}/folders/{folder_id}/items");
        let mut items = Vec::new();
        let mut marker: Option<String> = None;
        loop {
            let page: ItemPage = self
                .api
                .json(&|http| {
                    let request = http.get(&url).query(&[
                        ("fields", ITEM_FIELDS),
                        ("limit", PAGE_SIZE),
                        ("usemarker", "true"),
                    ]);
                    match &marker {
                        Some(marker) => request.query(&[("marker", marker.as_str())]),
                        None => request,
                    }
                })
                .await
                .map_err(|error| match error.kind {
                    ErrorKind::NotFound => {
                        self.folders.forget(path);
                        super::not_found(path)
                    }
                    _ => error,
                })?;
            items.extend(page.entries);
            match page.next_marker.filter(|marker| !marker.is_empty()) {
                Some(next) => marker = Some(next),
                None => break,
            }
        }
        let by_name = items
            .iter()
            .map(|item| (item.name.clone(), item.clone()))
            .collect();
        self.contents.lock().unwrap().insert(
            folder_id.to_string(),
            Contents {
                listed: Instant::now(),
                items: by_name,
            },
        );
        Ok(items)
    }

    /// The item called `name` in a folder, from a recent listing when there is one.
    async fn child(
        &self,
        folder_id: &str,
        folder_path: &str,
        name: &str,
    ) -> AppResult<Option<BoxItem>> {
        {
            let contents = self.contents.lock().unwrap();
            if let Some(listed) = contents
                .get(folder_id)
                .filter(|listed| listed.listed.elapsed() < CONTENTS_TTL)
            {
                return Ok(listed.items.get(name).cloned());
            }
        }
        let items = self.list_folder(folder_id, folder_path).await?;
        Ok(items.into_iter().find(|item| item.name == name))
    }

    /// Records an item this session created or changed in its folder's listing.
    fn remember(&self, folder_id: &str, item: &BoxItem) {
        if let Some(listed) = self.contents.lock().unwrap().get_mut(folder_id) {
            listed.items.insert(item.name.clone(), item.clone());
        }
    }

    fn forget_child(&self, folder_id: &str, name: &str) {
        if let Some(listed) = self.contents.lock().unwrap().get_mut(folder_id) {
            listed.items.remove(name);
        }
    }

    /// The id of the folder at `path`, or `None` when no folder is there.
    async fn folder_id(&self, path: &str) -> AppResult<Option<String>> {
        let path = protocol::normalize(path);
        if path == "/" {
            return Ok(Some(ROOT_ID.to_string()));
        }
        if let Some(id) = self.folders.get(&path) {
            return Ok(Some(id));
        }
        let names: Vec<&str> = path.split('/').filter(|name| !name.is_empty()).collect();
        let (mut depth, mut id) = (0, ROOT_ID.to_string());
        for known in (1..names.len()).rev() {
            if let Some(cached) = self.folders.get(&format!("/{}", names[..known].join("/"))) {
                (depth, id) = (known, cached);
                break;
            }
        }
        for walked in depth..names.len() {
            let parent_path = format!("/{}", names[..walked].join("/"));
            match self.child(&id, &parent_path, names[walked]).await? {
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

    /// The item at `path` and the id of the folder holding it.
    async fn lookup(&self, path: &str) -> AppResult<Option<(BoxItem, String)>> {
        let path = protocol::normalize(path);
        let Some(parent) = remote_path::parent(&path) else {
            return Ok(Some((BoxItem::root(), ROOT_ID.to_string())));
        };
        let Some(parent_id) = self.folder_id(&parent).await? else {
            return Ok(None);
        };
        let found = self
            .child(&parent_id, &parent, remote_path::file_name(&path))
            .await?;
        if let Some(folder) = found.as_ref().filter(|item| item.is_folder()) {
            self.folders.insert(&path, &folder.id);
        }
        Ok(found.map(|item| (item, parent_id)))
    }

    async fn existing(&self, path: &str) -> AppResult<(BoxItem, String)> {
        self.lookup(path)
            .await?
            .ok_or_else(|| super::not_found(path))
    }

    /// Creates a folder, or returns the item already holding the name with `false`.
    async fn create_folder(&self, parent_id: &str, name: &str) -> AppResult<(BoxItem, bool)> {
        let body = json!({ "name": name, "parent": { "id": parent_id } });
        let url = format!("{API_URL}/folders");
        let response = self
            .api
            .send_expecting(
                &|http| {
                    http.post(&url)
                        .query(&[("fields", ITEM_FIELDS)])
                        .json(&body)
                },
                &[StatusCode::CONFLICT],
            )
            .await?;
        if response.status() == StatusCode::CONFLICT {
            let existing = conflicting_item(response).await?.ok_or_else(|| {
                AppError::new(ErrorKind::AlreadyExists, format!("{name} already exists"))
            })?;
            return Ok((existing, false));
        }
        let folder: BoxItem = api::decode(response).await?;
        self.remember(parent_id, &folder);
        Ok((folder, true))
    }
}

#[async_trait]
impl RemoteFileSystem for BoxFs {
    fn protocol(&self) -> Protocol {
        Protocol::Box
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
            None => Err(match self.lookup(&path).await? {
                Some(_) => super::not_a_folder(&path),
                None => super::not_found(&path),
            }),
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
        let mut entries = Vec::new();
        for item in self.list_folder(&folder_id, &directory).await? {
            // Box refuses slashes in names, but a path could not reach one anyway.
            if item.name.is_empty() || item.name.contains('/') {
                continue;
            }
            let path = remote_path::join(&directory, &item.name);
            if item.is_folder() {
                self.folders.insert(&path, &item.id);
            }
            entries.push(item.entry(&path));
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
        let parent_id = self
            .folder_id(&parent)
            .await?
            .ok_or_else(|| super::not_found(&parent))?;
        let (folder, created) = self.create_folder(&parent_id, name).await?;
        if !created {
            return Err(protocol::already_exists(&path));
        }
        self.folders.insert(&path, &folder.id);
        Ok(path)
    }

    /// Box refuses a new file whose name is taken, so a file that appeared since the folder
    /// was listed is never overwritten.
    async fn make_file(self: Arc<Self>, parent: &str, name: &str) -> AppResult<String> {
        protocol::validate_name(name)?;
        let parent = protocol::normalize(&self.resolve(parent));
        let path = remote_path::join(&parent, name);
        let parent_id = self
            .folder_id(&parent)
            .await?
            .ok_or_else(|| super::not_found(&parent))?;
        if self.child(&parent_id, &parent, name).await?.is_some() {
            return Err(protocol::already_exists(&path));
        }
        let mut upload = BoxUpload {
            files: self,
            parent_id,
            name: name.to_string(),
            path: path.clone(),
            existing_id: None,
            replace: false,
            size: 0,
            modified: None,
            session: None,
            parts: Vec::new(),
            whole: Sha1::new(),
        };
        upload.upload_whole(Bytes::new()).await?;
        Ok(path)
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let path = protocol::normalize(&self.resolve(path));
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root folder"))?;
        let target = remote_path::join(&parent, new_name);
        let (item, parent_id) = self.existing(&path).await?;
        let url = format!("{API_URL}/{}/{}", item.collection(), item.id);
        let body = json!({ "name": new_name });
        let renamed: BoxItem = self
            .api
            .json(&|http| http.put(&url).query(&[("fields", ITEM_FIELDS)]).json(&body))
            .await
            .map_err(|error| match error.kind {
                ErrorKind::AlreadyExists => protocol::already_exists(&target),
                ErrorKind::NotFound => super::not_found(&path),
                _ => error,
            })?;
        self.forget_child(&parent_id, &item.name);
        self.remember(&parent_id, &renamed);
        self.folders.forget(&path);
        Ok(target)
    }

    /// Moves the items to the Box trash, where they can be restored.
    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        for path in protocol::outermost(&resolved) {
            if path == "/" {
                return Err(AppError::invalid("Refusing to delete /"));
            }
            let (item, parent_id) = self.existing(&path).await?;
            let url = format!("{API_URL}/{}/{}", item.collection(), item.id);
            let recursive = item.is_folder();
            self.api
                .send(&|http| match recursive {
                    true => http.delete(&url).query(&[("recursive", "true")]),
                    false => http.delete(&url),
                })
                .await
                .map_err(|error| match error.kind {
                    ErrorKind::NotFound => super::not_found(&path),
                    _ => error,
                })?;
            self.forget_child(&parent_id, &item.name);
            self.folders.forget(&path);
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        Ok(self
            .lookup(&self.resolve(path))
            .await?
            .map(|(item, _)| item.stat()))
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" || self.folders.get(&path).is_some() {
            return Ok(());
        }
        let _creating = self.folder_creation.lock().await;
        let mut current = "/".to_string();
        let mut id = ROOT_ID.to_string();
        for name in path.split('/').filter(|name| !name.is_empty()) {
            let parent = current.clone();
            current = remote_path::join(&current, name);
            if let Some(cached) = self.folders.get(&current) {
                id = cached;
                continue;
            }
            let folder = match self.child(&id, &parent, name).await? {
                Some(existing) => existing,
                // Another device may have made it since the listing.
                None => self.create_folder(&id, name).await?.0,
            };
            if !folder.is_folder() {
                return Err(protocol::not_a_folder(&current));
            }
            self.folders.insert(&current, &folder.id);
            id = folder.id;
        }
        Ok(())
    }

    /// A file's time is set when a version is uploaded and cannot change on its own.
    async fn set_attributes(
        &self,
        _path: &str,
        _modified: Option<i64>,
        _permissions: Option<u32>,
    ) -> AppResult<()> {
        Ok(())
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let (item, _) = self.existing(path).await?;
        if item.is_folder() {
            return Err(super::is_a_folder(path));
        }
        if !item.is_file() {
            return Err(AppError::unsupported(format!(
                "{} is a Box web link, which has no file content to download",
                item.name
            ))
            .with_path(path));
        }
        let size = item.size();
        let (start, end) = super::clamp_range(&range, size);
        if start >= end {
            return Ok(Box::new(HttpReader::new(None, 0)));
        }
        let requested_range = super::range_header(start, end, size);
        let url = format!("{API_URL}/files/{}/content", item.id);
        // Box redirects to a download address that carries its own authorization.
        let response = self
            .api
            .send(&|http| match &requested_range {
                Some(value) => http.get(&url).header(RANGE, value),
                None => http.get(&url),
            })
            .await?;
        match response.status() {
            StatusCode::ACCEPTED => Err(AppError::new(
                ErrorKind::Cloud,
                format!(
                    "Box is still preparing {} for download; try again shortly",
                    item.name
                ),
            )
            .with_path(path)),
            StatusCode::OK if start > 0 => {
                Ok(Box::new(HttpReader::skipping(response, start, end - start)))
            }
            _ => Ok(Box::new(HttpReader::new(Some(response), end - start))),
        }
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        if request.offset > 0 {
            return Err(super::cannot_resume(Protocol::Box));
        }
        let path = protocol::normalize(&request.path);
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot write to the root folder"))?;
        let name = remote_path::file_name(&path).to_string();
        protocol::validate_name(&name)?;
        let parent_id = self
            .folder_id(&parent)
            .await?
            .ok_or_else(|| super::not_found(&parent))?;
        let existing = self.child(&parent_id, &parent, &name).await?;
        match &existing {
            Some(item) if item.is_folder() => return Err(super::is_a_folder(&path)),
            Some(item) if !item.is_file() => return Err(protocol::already_exists(&path)),
            _ => {}
        }
        let target = BoxUpload {
            files: self,
            parent_id,
            name,
            path,
            existing_id: existing.map(|item| item.id),
            replace: true,
            size: request.size,
            modified: request.modified,
            session: None,
            parts: Vec::new(),
            whole: Sha1::new(),
        };
        Ok(Box::new(ChunkedUpload::new(target, request.size)))
    }
}

/// Creates a file, or adds a version to the one already at the path so it keeps its id,
/// sharing and history.
struct BoxUpload {
    files: Arc<BoxFs>,
    parent_id: String,
    name: String,
    path: String,
    existing_id: Option<String>,
    /// A file that appears at the path meanwhile gets a new version, rather than the upload
    /// failing because the name is taken.
    replace: bool,
    size: u64,
    modified: Option<i64>,
    /// The chunked upload in progress, aborted if the upload does not finish.
    session: Option<UploadSession>,
    parts: Vec<serde_json::Value>,
    /// The hash of everything sent so far, which the commit carries.
    whole: Sha1,
}

impl BoxUpload {
    fn attributes(&self, with_place: bool) -> serde_json::Value {
        let mut attributes = serde_json::Map::new();
        if with_place {
            attributes.insert("name".into(), json!(self.name));
            attributes.insert("parent".into(), json!({ "id": self.parent_id }));
        }
        if let Some(modified) = self.modified {
            attributes.insert(
                "content_modified_at".into(),
                json!(timestamp::format_rfc3339(modified)),
            );
        }
        serde_json::Value::Object(attributes)
    }

    /// Records the uploaded file in its folder's listing.
    async fn finished(&mut self, response: Response) -> AppResult<()> {
        let uploaded: Uploaded = api::decode(response).await?;
        if let Some(file) = uploaded.entries.first() {
            self.files.remember(&self.parent_id, file);
        }
        Ok(())
    }
}

impl Drop for BoxUpload {
    fn drop(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let api = self.files.api.clone();
        // Box keeps the parts of an unfinished upload for a week otherwise.
        runtime.spawn(async move {
            let abort = session.session_endpoints.abort;
            let _ = api.send(&|http| http.delete(&abort)).await;
        });
    }
}

#[async_trait]
impl UploadTarget for BoxUpload {
    fn single_request_limit(&self) -> usize {
        SINGLE_REQUEST_LIMIT
    }

    fn chunk_size(&self) -> usize {
        self.session
            .as_ref()
            .map_or(SINGLE_REQUEST_LIMIT, |session| session.part_size)
    }

    async fn upload_whole(&mut self, data: Bytes) -> AppResult<()> {
        loop {
            let (url, attributes) = match &self.existing_id {
                Some(id) => (
                    format!("{UPLOAD_URL}/files/{id}/content"),
                    self.attributes(false),
                ),
                None => (format!("{UPLOAD_URL}/files/content"), self.attributes(true)),
            };
            let boundary = format!("poros-{}", uuid::Uuid::new_v4().simple());
            let attributes = serde_json::to_vec(&attributes)
                .map_err(|error| AppError::invalid(error.to_string()))?;
            let mut body = BytesMut::with_capacity(data.len() + attributes.len() + 256);
            body.put_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"attributes\"\r\n\r\n"
                )
                .as_bytes(),
            );
            body.put_slice(&attributes);
            body.put_slice(
                format!(
                    "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            );
            body.put_slice(&data);
            body.put_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            let body = body.freeze();
            let content_type = format!("multipart/form-data; boundary={boundary}");
            let response = self
                .files
                .api
                .send_expecting(
                    &|http| {
                        http.post(&url)
                            .query(&[("fields", ITEM_FIELDS)])
                            .header(CONTENT_TYPE, &content_type)
                            .body(body.clone())
                    },
                    &[StatusCode::CONFLICT],
                )
                .await?;
            if response.status() == StatusCode::CONFLICT {
                // Something of that name appeared since the folder was listed.
                let conflict = conflicting_item(response).await?;
                match conflict.filter(|item| item.is_file() && self.replace) {
                    Some(file) if self.existing_id.is_none() => {
                        self.existing_id = Some(file.id);
                        continue;
                    }
                    _ => return Err(protocol::already_exists(&self.path)),
                }
            }
            return self.finished(response).await;
        }
    }

    async fn start_session(&mut self) -> AppResult<()> {
        loop {
            let (url, body) = match &self.existing_id {
                Some(id) => (
                    format!("{UPLOAD_URL}/files/{id}/upload_sessions"),
                    json!({ "file_size": self.size }),
                ),
                None => (
                    format!("{UPLOAD_URL}/files/upload_sessions"),
                    json!({
                        "folder_id": self.parent_id,
                        "file_size": self.size,
                        "file_name": self.name,
                    }),
                ),
            };
            let response = self
                .files
                .api
                .send_expecting(&|http| http.post(&url).json(&body), &[StatusCode::CONFLICT])
                .await?;
            if response.status() == StatusCode::CONFLICT {
                let conflict = conflicting_item(response).await?;
                match conflict.filter(|item| item.is_file() && self.replace) {
                    Some(file) if self.existing_id.is_none() => {
                        self.existing_id = Some(file.id);
                        continue;
                    }
                    _ => return Err(protocol::already_exists(&self.path)),
                }
            }
            let session: UploadSession = api::decode(response).await?;
            if session.part_size == 0 {
                return Err(AppError::new(
                    ErrorKind::Cloud,
                    "Box opened an upload without a part size",
                ));
            }
            self.session = Some(session);
            return Ok(());
        }
    }

    async fn send_chunk(&mut self, offset: u64, data: Bytes, last: bool) -> AppResult<()> {
        let (upload_part, commit) = match &self.session {
            Some(session) => (
                session.session_endpoints.upload_part.clone(),
                session.session_endpoints.commit.clone(),
            ),
            None => return Err(AppError::new(ErrorKind::Cloud, "No upload session is open")),
        };
        let digest = format!("sha={}", sha1_base64(&data));
        let range = content_range(offset, data.len() as u64, Some(self.size));
        let response = self
            .files
            .api
            .send(&|http| {
                http.put(&upload_part)
                    .header("Digest", &digest)
                    .header(CONTENT_RANGE, &range)
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(data.clone())
            })
            .await?;
        let answer: serde_json::Value = api::decode(response).await?;
        let part = match answer.get("part") {
            Some(part) => part.clone(),
            None => answer,
        };
        self.parts.push(part);
        self.whole.update(&data);
        if !last {
            return Ok(());
        }
        let digest = format!(
            "sha={}",
            base64::engine::general_purpose::STANDARD.encode(self.whole.finalize_reset())
        );
        let body = json!({ "parts": self.parts, "attributes": self.attributes(false) });
        for _ in 0..COMMIT_ATTEMPTS {
            let response = self
                .files
                .api
                .send(&|http| http.post(&commit).header("Digest", &digest).json(&body))
                .await?;
            if response.status() == StatusCode::ACCEPTED {
                tokio::time::sleep(retry_after(&response)).await;
                continue;
            }
            self.session = None;
            return self.finished(response).await;
        }
        Err(AppError::new(
            ErrorKind::Cloud,
            "Box did not finish putting the upload together",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_items() {
        let page: ItemPage = serde_json::from_str(
            r#"{"total_count":3,"entries":[
                {"type":"folder","id":"11","etag":"0","name":"Photos","size":5000},
                {"type":"file","id":"12","name":"a.txt","size":42,"modified_at":"2024-05-01T10:00:00-07:00","content_modified_at":"2020-01-02T00:00:00-08:00"},
                {"type":"web_link","id":"13","name":"Site"}
            ],"limit":1000,"next_marker":"abc"}"#,
        )
        .unwrap();
        let [folder, file, link] = &page.entries[..] else {
            panic!("three entries");
        };
        assert!(folder.is_folder());
        assert_eq!(folder.size(), 0);
        assert_eq!(file.size(), 42);
        assert_eq!(
            file.modified(),
            timestamp::parse_rfc3339("2020-01-02T08:00:00Z")
        );
        assert_eq!(link.kind(), EntryKind::Other);
        assert_eq!(link.collection(), "web_links");
        assert_eq!(page.next_marker.as_deref(), Some("abc"));
    }

    #[test]
    fn reads_conflicts_of_both_shapes() {
        let read = |body: &str| {
            let body: ConflictBody = serde_json::from_str(body).unwrap();
            match body.context_info.conflicts {
                serde_json::Value::Array(items) => items.into_iter().next(),
                item @ serde_json::Value::Object(_) => Some(item),
                _ => None,
            }
            .and_then(|item| serde_json::from_value::<BoxItem>(item).ok())
        };
        let file = read(
            r#"{"type":"error","status":409,"code":"item_name_in_use","context_info":{"conflicts":{"type":"file","id":"7","name":"a.txt"}}}"#,
        )
        .unwrap();
        assert!(file.is_file());
        let folder = read(
            r#"{"type":"error","status":409,"code":"item_name_in_use","context_info":{"conflicts":[{"type":"folder","id":"8","name":"Docs"}]}}"#,
        )
        .unwrap();
        assert_eq!(folder.id, "8");
        assert!(read(r#"{"type":"error","status":409}"#).is_none());
    }

    #[test]
    fn hashes_parts_for_the_digest() {
        assert_eq!(sha1_base64(b""), "2jmj7l5rSw0yVb/vlWAYkK/YBwk=");
    }
}
