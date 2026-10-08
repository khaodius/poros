//! OneDrive over Microsoft Graph, for personal, work and school accounts. Graph addresses
//! items by path, so unlike Google Drive no lookups are needed to reach a file.

use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE, RANGE};
use reqwest::StatusCode;
use serde::de::IgnoredAny;
use serde::Deserialize;
use serde_json::json;

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

const DRIVE_URL: &str = "https://graph.microsoft.com/v1.0/me/drive";
const ITEM_FIELDS: &str = "id,name,size,folder,package,fileSystemInfo,lastModifiedDateTime";
const PAGE_SIZE: u32 = 1000;
/// Everything but unreserved characters is escaped in a path segment.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveItem {
    #[serde(default)]
    id: String,
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    folder: Option<IgnoredAny>,
    /// OneNote notebooks and similar bundles, which are neither files nor folders.
    #[serde(default)]
    package: Option<IgnoredAny>,
    #[serde(default)]
    file_system_info: Option<FileSystemInfo>,
    #[serde(default)]
    last_modified_date_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSystemInfo {
    #[serde(default)]
    last_modified_date_time: Option<String>,
}

impl DriveItem {
    fn is_folder(&self) -> bool {
        self.folder.is_some() && self.package.is_none()
    }

    fn kind(&self) -> EntryKind {
        if self.package.is_some() {
            EntryKind::Other
        } else if self.folder.is_some() {
            EntryKind::Dir
        } else {
            EntryKind::File
        }
    }

    /// The time the file itself last changed, which uploads can set, rather than when OneDrive
    /// last touched the item.
    fn modified(&self) -> Option<i64> {
        self.file_system_info
            .as_ref()
            .and_then(|info| info.last_modified_date_time.as_deref())
            .or(self.last_modified_date_time.as_deref())
            .and_then(timestamp::parse_rfc3339)
    }

    fn size(&self) -> u64 {
        if self.is_folder() {
            0
        } else {
            self.size
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
struct Children {
    #[serde(default)]
    value: Vec<DriveItem>,
    #[serde(rename = "@odata.nextLink", default)]
    next_link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadSession {
    upload_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadProgress {
    #[serde(default)]
    next_expected_ranges: Vec<String>,
}

/// The Graph address of the item at `path`, followed by `action` (`/children`, `/content`).
fn item_url(path: &str, action: &str) -> String {
    let path = protocol::normalize(path);
    if path == "/" {
        return format!("{DRIVE_URL}/root{action}");
    }
    let encoded: String = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| format!("/{}", utf8_percent_encode(segment, PATH_SEGMENT)))
        .collect();
    if action.is_empty() {
        format!("{DRIVE_URL}/root:{encoded}")
    } else {
        format!("{DRIVE_URL}/root:{encoded}:{action}")
    }
}

fn file_system_info(modified: i64) -> serde_json::Value {
    json!({ "lastModifiedDateTime": timestamp::format_rfc3339(modified) })
}

pub struct OneDriveFs {
    api: Api,
    folders: FolderCache,
    /// Keeps parallel uploads from racing to create the same folder.
    folder_creation: tokio::sync::Mutex<()>,
}

impl OneDriveFs {
    pub async fn connect(api: Api) -> AppResult<Self> {
        api.send(&|http| http.get(format!("{DRIVE_URL}?$select=id,driveType")))
            .await?;
        Ok(Self {
            api,
            folders: FolderCache::default(),
            folder_creation: tokio::sync::Mutex::new(()),
        })
    }

    async fn item(&self, path: &str) -> AppResult<Option<DriveItem>> {
        let url = format!("{}?$select={ITEM_FIELDS}", item_url(path, ""));
        match self.api.json::<DriveItem>(&|http| http.get(&url)).await {
            Ok(item) => {
                if item.is_folder() {
                    self.folders.insert(&protocol::normalize(path), &item.id);
                }
                Ok(Some(item))
            }
            Err(error) if error.kind == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn existing(&self, path: &str) -> AppResult<DriveItem> {
        self.item(path).await?.ok_or_else(|| super::not_found(path))
    }

    /// Creates a folder, failing if anything has the name already.
    async fn create_folder(&self, parent: &str, name: &str) -> AppResult<DriveItem> {
        let body = json!({
            "name": name,
            "folder": {},
            "@microsoft.graph.conflictBehavior": "fail",
        });
        let url = item_url(parent, "/children");
        self.api
            .json(&|http| http.post(&url).json(&body))
            .await
            .map_err(|error| match error.kind {
                ErrorKind::AlreadyExists => {
                    protocol::already_exists(&remote_path::join(parent, name))
                }
                ErrorKind::NotFound => super::not_found(parent),
                _ => error,
            })
    }

    async fn update(&self, path: &str, body: serde_json::Value) -> AppResult<()> {
        let url = item_url(path, "");
        self.api.send(&|http| http.patch(&url).json(&body)).await?;
        Ok(())
    }
}

#[async_trait]
impl RemoteFileSystem for OneDriveFs {
    fn protocol(&self) -> Protocol {
        Protocol::OneDrive
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
        match self.item(&path).await? {
            Some(item) if item.is_folder() => Ok(path),
            Some(_) => Err(super::not_a_folder(&path)),
            None => Err(super::not_found(&path)),
        }
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let directory = protocol::normalize(&self.resolve(path));
        if !self.existing(&directory).await?.is_folder() {
            return Err(super::not_a_folder(&directory));
        }
        let mut entries = Vec::new();
        let mut next = Some(format!(
            "{}?$select={ITEM_FIELDS}&$top={PAGE_SIZE}",
            item_url(&directory, "/children")
        ));
        while let Some(url) = next.take() {
            let page: Children = self.api.json(&|http| http.get(&url)).await?;
            for item in page.value {
                let path = remote_path::join(&directory, &item.name);
                if item.is_folder() {
                    self.folders.insert(&path, &item.id);
                }
                entries.push(item.entry(&path));
            }
            next = page.next_link;
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
        let folder = self.create_folder(&parent, name).await?;
        self.folders.insert(&path, &folder.id);
        Ok(path)
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let path = protocol::normalize(&self.resolve(path));
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root folder"))?;
        let target = remote_path::join(&parent, new_name);
        self.update(&path, json!({ "name": new_name }))
            .await
            .map_err(|error| match error.kind {
                ErrorKind::AlreadyExists => protocol::already_exists(&target),
                ErrorKind::NotFound => super::not_found(&path),
                _ => error,
            })?;
        self.folders.forget(&path);
        Ok(target)
    }

    /// Moves the items to the OneDrive recycle bin.
    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        for path in protocol::outermost(&resolved) {
            if path == "/" {
                return Err(AppError::invalid("Refusing to delete /"));
            }
            let url = item_url(&path, "");
            self.api
                .send(&|http| http.delete(&url))
                .await
                .map_err(|error| match error.kind {
                    ErrorKind::NotFound => super::not_found(&path),
                    _ => error,
                })?;
            self.folders.forget(&path);
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        Ok(self
            .item(&self.resolve(path))
            .await?
            .map(|item| item.stat()))
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let path = protocol::normalize(&self.resolve(path));
        if path == "/" || self.folders.get(&path).is_some() {
            return Ok(());
        }
        let _creating = self.folder_creation.lock().await;
        let mut current = "/".to_string();
        for name in path.split('/').filter(|name| !name.is_empty()) {
            let parent = current.clone();
            current = remote_path::join(&current, name);
            if self.folders.get(&current).is_some() {
                continue;
            }
            match self.item(&current).await? {
                Some(item) if item.is_folder() => {}
                Some(_) => return Err(protocol::not_a_folder(&current)),
                None => {
                    let folder = self.create_folder(&parent, name).await?;
                    self.folders.insert(&current, &folder.id);
                }
            }
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
        self.update(
            &self.resolve(path),
            json!({ "fileSystemInfo": file_system_info(modified) }),
        )
        .await
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let item = self.existing(path).await?;
        if item.kind() != EntryKind::File {
            return Err(super::is_a_folder(path));
        }
        let size = item.size();
        let (start, end) = super::clamp_range(&range, size);
        if start >= end {
            return Ok(Box::new(HttpReader::new(None, 0)));
        }
        let requested_range = super::range_header(start, end, size);
        let url = item_url(path, "/content");
        // Graph redirects to a short-lived download address, which is fetched without the
        // access token since it is for another host.
        let response = self
            .api
            .send(&|http| match &requested_range {
                Some(value) => http.get(&url).header(RANGE, value),
                None => http.get(&url),
            })
            .await?;
        Ok(Box::new(HttpReader::new(Some(response), end - start)))
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        if request.offset > 0 {
            return Err(super::cannot_resume(Protocol::OneDrive));
        }
        let path = protocol::normalize(&request.path);
        if path == "/" {
            return Err(AppError::invalid("Cannot write to the root folder"));
        }
        let target = OneDriveUpload {
            api: self.api.clone(),
            path,
            size: request.size,
            modified: request.modified,
            upload_url: None,
        };
        Ok(Box::new(ChunkedUpload::new(target, request.size)))
    }
}

/// Replaces whatever file is at the path.
struct OneDriveUpload {
    api: Api,
    path: String,
    size: u64,
    modified: Option<i64>,
    upload_url: Option<String>,
}

#[async_trait]
impl UploadTarget for OneDriveUpload {
    async fn upload_whole(&mut self, data: Bytes) -> AppResult<()> {
        let url = format!(
            "{}?@microsoft.graph.conflictBehavior=replace",
            item_url(&self.path, "/content")
        );
        self.api
            .send(&|http| {
                http.put(&url)
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(data.clone())
            })
            .await?;
        // A single-request upload cannot carry the modification time.
        if let Some(modified) = self.modified {
            let url = item_url(&self.path, "");
            let body = json!({ "fileSystemInfo": file_system_info(modified) });
            self.api.send(&|http| http.patch(&url).json(&body)).await?;
        }
        Ok(())
    }

    async fn start_session(&mut self) -> AppResult<()> {
        let mut item = serde_json::Map::new();
        item.insert("@microsoft.graph.conflictBehavior".into(), json!("replace"));
        if let Some(modified) = self.modified {
            item.insert("fileSystemInfo".into(), file_system_info(modified));
        }
        let body = json!({ "item": item });
        let url = item_url(&self.path, "/createUploadSession");
        let session: UploadSession = self.api.json(&|http| http.post(&url).json(&body)).await?;
        self.upload_url = Some(session.upload_url);
        Ok(())
    }

    async fn send_chunk(&mut self, offset: u64, data: Bytes, last: bool) -> AppResult<()> {
        let upload_url = self
            .upload_url
            .clone()
            .ok_or_else(|| AppError::new(ErrorKind::Cloud, "No upload session is open"))?;
        let (mut offset, mut data) = (offset, data);
        loop {
            let end = offset + data.len() as u64;
            let total = if last { end } else { self.size.max(end + 1) };
            let range = content_range(offset, data.len() as u64, Some(total));
            let response = self
                .api
                .send_unauthorized(&|http| {
                    http.put(&upload_url)
                        .header(CONTENT_RANGE, &range)
                        .body(data.clone())
                })
                .await?;
            if response.status() != StatusCode::ACCEPTED {
                return Ok(());
            }
            let progress: UploadProgress = api::decode(response).await?;
            let next_expected = progress
                .next_expected_ranges
                .first()
                .and_then(|range| range.split('-').next())
                .and_then(|start| start.trim().parse::<u64>().ok())
                .unwrap_or(end);
            if next_expected >= end {
                return match last {
                    true => Err(AppError::new(
                        ErrorKind::Cloud,
                        "OneDrive did not complete the upload",
                    )),
                    false => Ok(()),
                };
            }
            if next_expected < offset {
                return Err(AppError::new(
                    ErrorKind::Cloud,
                    "OneDrive lost part of the upload",
                ));
            }
            data = data.slice((next_expected - offset) as usize..);
            offset = next_expected;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_items_by_path() {
        assert_eq!(
            item_url("/", "/children"),
            format!("{DRIVE_URL}/root/children")
        );
        assert_eq!(
            item_url("/Documents/a b#1.txt", ""),
            format!("{DRIVE_URL}/root:/Documents/a%20b%231.txt")
        );
        assert_eq!(
            item_url("/Photos/", "/children"),
            format!("{DRIVE_URL}/root:/Photos:/children")
        );
    }

    #[test]
    fn prefers_the_files_own_modification_time() {
        let item: DriveItem = serde_json::from_str(
            r#"{"id":"1","name":"a","size":3,"file":{},"lastModifiedDateTime":"2024-01-02T00:00:00Z","fileSystemInfo":{"lastModifiedDateTime":"2020-01-02T00:00:00Z"}}"#,
        )
        .unwrap();
        assert_eq!(
            item.modified(),
            timestamp::parse_rfc3339("2020-01-02T00:00:00Z")
        );
        assert_eq!(item.kind(), EntryKind::File);
        let folder: DriveItem =
            serde_json::from_str(r#"{"id":"2","name":"f","size":99,"folder":{"childCount":1}}"#)
                .unwrap();
        assert!(folder.is_folder());
        assert_eq!(folder.size(), 0);
    }
}
