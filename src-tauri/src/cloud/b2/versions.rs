//! File versions on Backblaze B2, which keeps every upload of a name until it is deleted by
//! its id, and hides a deleted file under a marker instead of removing it.

use std::ops::Range;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, RANGE};
use reqwest::StatusCode;
use serde_json::json;

use super::{failure, B2File, B2Fs, VersionList, API_PATH, FOLDER_MARKER, PAGE_SIZE};
use crate::cloud::s3::split;
use crate::cloud::{api, HttpReader};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::FileEntry;
use crate::protocol::{self, ReadStream, RemoteFileSystem, RemoteStat};
use crate::remote_path;
use crate::versions::{
    self, DeletedEntry, HiddenFile, HiddenFiles, HistoryRecord, ObjectVersion, ObjectVersions,
    VersionKind, Versioning,
};

/// Where a listing of versions continues: the next file name and file id.
type Start = (String, String);

/// What a key needs for each thing done with versions, and how the refusal reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Need {
    List,
    Write,
    Delete,
}

impl Need {
    fn capability(self) -> &'static str {
        match self {
            Need::List => "listFiles",
            Need::Write => "writeFiles",
            Need::Delete => "deleteFiles",
        }
    }

    fn refusal(self) -> &'static str {
        match self {
            Need::List => "This application key cannot list file versions",
            Need::Write => "This application key cannot restore a version, which writes a file",
            Need::Delete => {
                "This application key cannot remove versions or bring back deleted files"
            }
        }
    }
}

/// Why the key cannot do this with the file names starting with `name`, if it cannot. A key
/// whose capabilities Backblaze did not list is left for the service to judge.
fn refusal(
    capabilities: &[String],
    name_prefix: Option<&str>,
    need: Need,
    name: &str,
) -> Option<String> {
    if let Some(prefix) = name_prefix.filter(|prefix| !name.starts_with(prefix)) {
        return Some(format!(
            "This application key only reaches files whose names start with {prefix}"
        ));
    }
    let lacking = !capabilities.is_empty()
        && !capabilities
            .iter()
            .any(|capability| capability == need.capability());
    lacking.then(|| {
        format!(
            "{}: it lacks the {} capability",
            need.refusal(),
            need.capability()
        )
    })
}

/// The body of `b2_list_file_versions`.
fn versions_request(
    bucket_id: &str,
    prefix: &str,
    grouped: bool,
    start: Option<&Start>,
    count: u32,
) -> serde_json::Value {
    let mut body = json!({
        "bucketId": bucket_id,
        "prefix": prefix,
        "maxFileCount": count,
    });
    if grouped {
        body["delimiter"] = json!("/");
    }
    if let Some((file_name, file_id)) = start {
        body["startFileName"] = json!(file_name);
        if !file_id.is_empty() {
            body["startFileId"] = json!(file_id);
        }
    }
    body
}

/// The body of `b2_delete_file_version`.
fn delete_request(file_name: &str, file_id: &str) -> serde_json::Value {
    json!({ "fileName": file_name, "fileId": file_id })
}

/// The address `b2_download_file_by_id` reads one version from.
fn download_address(download_url: &str, file_id: &str) -> String {
    format!(
        "{download_url}{API_PATH}/b2_download_file_by_id?fileId={}",
        percent_encoding::utf8_percent_encode(file_id, crate::cloud::URL_SEGMENT)
    )
}

/// A listed upload or hide marker as an entry of a file's history; `None` for folders and
/// unfinished large files, which are neither.
fn history_entry(file: &B2File, latest: bool) -> Option<ObjectVersion> {
    let kind = match file.action.as_str() {
        "upload" => VersionKind::Version,
        "hide" => VersionKind::DeleteMarker,
        _ => return None,
    };
    Some(ObjectVersion {
        version_id: file.file_id.clone()?,
        kind,
        latest,
        modified: (file.upload_timestamp > 0).then(|| file.upload_timestamp.div_euclid(1000)),
        size: file.content_length,
        etag: None,
        storage_class: None,
    })
}

/// Turns a page of `b2_list_file_versions` into history records and the folders a delimiter
/// grouped. B2 lists each name's versions newest first, so the first of a name is its newest;
/// `newest_seen` carries the last name across pages.
fn history_records(
    files: &[B2File],
    newest_seen: &mut Option<String>,
) -> (Vec<HistoryRecord>, Vec<String>) {
    let mut records = Vec::new();
    let mut folders = Vec::new();
    for file in files {
        if file.action == "folder" {
            folders.push(file.file_name.clone());
            continue;
        }
        let latest = newest_seen.as_deref() != Some(file.file_name.as_str());
        if let Some(version) = history_entry(file, latest) {
            *newest_seen = Some(file.file_name.clone());
            records.push(HistoryRecord {
                key: file.file_name.clone(),
                version,
            });
        }
    }
    (records, folders)
}

/// A path's bucket and file name, with the trailing slash kept of a name that has one, as the
/// placeholders tools write for folders do.
fn name_keeping_slash(path: &str) -> (Option<String>, String) {
    let (bucket, mut name) = split(&protocol::normalize(path));
    if path.ends_with('/') && !name.is_empty() {
        name.push('/');
    }
    (bucket, name)
}

fn prefix_of(key: &str) -> String {
    match key {
        "" => String::new(),
        key => format!("{key}/"),
    }
}

impl B2Fs {
    fn permit(&self, need: Need, name: &str, path: &str) -> AppResult<()> {
        match refusal(
            &self.key_capabilities,
            self.name_prefix.as_deref(),
            need,
            name,
        ) {
            Some(message) => {
                Err(AppError::new(ErrorKind::PermissionDenied, message).with_path(path))
            }
            None => Ok(()),
        }
    }

    /// The bucket and name of a file, which a version belongs to. A `.` or `..` part is
    /// refused, not followed, so no other file's history is read or changed in its place.
    fn file_named(&self, path: &str) -> AppResult<(String, String, String)> {
        let resolved = self.resolve(path);
        let path = protocol::normalize_exact(&resolved)?;
        match name_keeping_slash(&resolved) {
            (Some(bucket), key) if !key.is_empty() => Ok((bucket, key, path)),
            _ => Err(AppError::invalid("Versions belong to files inside a bucket").with_path(path)),
        }
    }

    async fn versions_page(
        &self,
        bucket_id: &str,
        prefix: &str,
        grouped: bool,
        start: Option<&Start>,
        count: u32,
        path: &str,
    ) -> AppResult<VersionList> {
        let body = versions_request(bucket_id, prefix, grouped, start, count);
        self.call("b2_list_file_versions", body, path, true).await
    }

    /// One version of a file, by its id.
    async fn version_at(
        &self,
        bucket_id: &str,
        key: &str,
        version_id: &str,
        path: &str,
    ) -> AppResult<B2File> {
        let start = (key.to_string(), version_id.to_string());
        let page = self
            .versions_page(bucket_id, key, false, Some(&start), 1, path)
            .await?;
        page.files
            .into_iter()
            .find(|file| file.file_name == key && file.file_id.as_deref() == Some(version_id))
            .ok_or_else(|| crate::cloud::not_found(path))
    }

    /// An uploaded version, which has content, by its id.
    async fn upload_at(&self, path: &str, version_id: &str) -> AppResult<(String, B2File)> {
        versions::validate_version_id(version_id)?;
        let (bucket, key, path) = self.file_named(path)?;
        self.permit(Need::List, &key, &path)?;
        let bucket_id = self.existing_bucket(&bucket).await?;
        let version = self.version_at(&bucket_id, &key, version_id, &path).await?;
        match version.action.as_str() {
            "upload" => Ok((bucket_id, version)),
            _ => Err(versions::marker_has_no_content(&path)),
        }
    }

    /// The hidden files under a prefix, and the folders deeper names fall into when `grouped`.
    async fn hidden_under(
        &self,
        bucket: &str,
        prefix: &str,
        grouped: bool,
        path: &str,
    ) -> AppResult<(Vec<HiddenFile>, Vec<String>)> {
        self.permit(Need::List, prefix, path)?;
        let bucket_id = self.existing_bucket(bucket).await?;
        let mut hidden = HiddenFiles::default();
        let mut folders = Vec::new();
        let mut newest_seen = None;
        let mut start: Option<Start> = None;
        loop {
            let page = self
                .versions_page(&bucket_id, prefix, grouped, start.as_ref(), PAGE_SIZE, path)
                .await?;
            let (records, grouped_names) = history_records(&page.files, &mut newest_seen);
            hidden.add_page(records);
            folders.extend(
                grouped_names
                    .into_iter()
                    .map(|name| format!("/{bucket}/{name}")),
            );
            let next = page
                .next_file_name
                .map(|file_name| (file_name, page.next_file_id.unwrap_or_default()));
            if next.is_none() || next == start {
                break;
            }
            start = next;
        }
        Ok((hidden.finish(bucket), folders))
    }
}

#[async_trait]
impl ObjectVersions for B2Fs {
    /// B2 keeps versions in every bucket.
    async fn versioning(&self, path: &str) -> AppResult<Versioning> {
        Ok(match split(&self.resolve(path)) {
            (Some(_), _) => Versioning::Enabled,
            (None, _) => Versioning::Off,
        })
    }

    async fn list_versions(&self, path: &str) -> AppResult<Vec<ObjectVersion>> {
        let (bucket, key, path) = self.file_named(path)?;
        self.permit(Need::List, &key, &path)?;
        let bucket_id = self.existing_bucket(&bucket).await?;
        let mut history = Vec::new();
        let mut start: Option<Start> = Some((key.clone(), String::new()));
        loop {
            let page = self
                .versions_page(&bucket_id, &key, false, start.as_ref(), PAGE_SIZE, &path)
                .await?;
            // Names come in order, the name itself before every longer one starting with it.
            let mut reached_other_names = false;
            for file in &page.files {
                if file.file_name != key {
                    reached_other_names = true;
                } else if let Some(version) = history_entry(file, history.is_empty()) {
                    history.push(version);
                }
            }
            let next = page
                .next_file_name
                .map(|file_name| (file_name, page.next_file_id.unwrap_or_default()));
            if reached_other_names || next.is_none() || next == start {
                return Ok(history);
            }
            start = next;
        }
    }

    async fn list_deleted(&self, folder: &str, live: &[FileEntry]) -> AppResult<Vec<DeletedEntry>> {
        let directory = protocol::normalize(&self.resolve(folder));
        let (Some(bucket), key) = split(&directory) else {
            return Ok(Vec::new());
        };
        let (mut hidden, folders) = self
            .hidden_under(&bucket, &prefix_of(&key), true, &directory)
            .await?;
        // The file that keeps an empty folder comes back with the folder, not on its own.
        hidden.retain(|file| remote_path::file_name(&file.path) != FOLDER_MARKER);
        Ok(versions::deleted_entries(
            &directory, &hidden, &folders, live,
        ))
    }

    async fn deleted_under(&self, folder: &str) -> AppResult<Vec<HiddenFile>> {
        let directory = protocol::normalize_exact(&self.resolve(folder))?;
        let (Some(bucket), key) = split(&directory) else {
            return Err(AppError::invalid("Deleted files are kept inside a bucket"));
        };
        let (hidden, _) = self
            .hidden_under(&bucket, &prefix_of(&key), false, &directory)
            .await?;
        Ok(hidden)
    }

    async fn restore_version(&self, path: &str, version_id: &str) -> AppResult<()> {
        let (_, key, resolved) = self.file_named(path)?;
        self.permit(Need::Write, &key, &resolved)?;
        let (bucket_id, version) = self.upload_at(path, version_id).await?;
        self.copy_file(&bucket_id, &version, &key, &resolved).await
    }

    async fn stat_version(&self, path: &str, version_id: &str) -> AppResult<RemoteStat> {
        let (_, version) = self.upload_at(path, version_id).await?;
        Ok(version.stat())
    }

    async fn open_version(
        &self,
        path: &str,
        version_id: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        versions::validate_version_id(version_id)?;
        let (_, _, path) = self.file_named(path)?;
        if range.start >= range.end {
            self.upload_at(&path, version_id).await?;
            return Ok(Box::new(HttpReader::new(None, 0)));
        }
        let requested_range = (range.start > 0 || range.end != u64::MAX).then(|| {
            let last = match range.end {
                u64::MAX => String::new(),
                end => (end - 1).to_string(),
            };
            format!("bytes={}-{last}", range.start)
        });
        let response = self
            .send(&path, true, &|session| {
                let request = self
                    .http
                    .get(download_address(&session.download_url, version_id))
                    .header(AUTHORIZATION, &session.token);
                let request = match &requested_range {
                    Some(value) => request.header(RANGE, value),
                    None => request,
                };
                request.build().map_err(api::transport_error)
            })
            .await?;
        let length = range.end - range.start;
        match response.status() {
            StatusCode::PARTIAL_CONTENT => Ok(Box::new(HttpReader::new(Some(response), length))),
            StatusCode::OK => Ok(Box::new(HttpReader::skipping(
                response,
                range.start,
                length,
            ))),
            StatusCode::RANGE_NOT_SATISFIABLE => Ok(Box::new(HttpReader::new(None, 0))),
            _ => Err(failure(response, &path).await),
        }
    }

    async fn delete_version(&self, path: &str, version_id: &str) -> AppResult<()> {
        versions::validate_version_id(version_id)?;
        let (_, key, path) = self.file_named(path)?;
        self.permit(Need::Delete, &key, &path)?;
        let body = delete_request(&key, version_id);
        // B2 answers the same for a version already gone and for a name that is not the
        // version's, so neither counts as done.
        self.call::<serde_json::Value>("b2_delete_file_version", body, &path, false)
            .await
            .map(drop)
    }

    /// B2 deletes a bucket with every version in it already.
    async fn delete_with_versions(&self, paths: &[String]) -> AppResult<()> {
        RemoteFileSystem::delete(self, paths).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed() -> VersionList {
        serde_json::from_str(
            r#"{"files":[
                {"accountId":"a","action":"folder","bucketId":"b","contentLength":0,"fileId":null,"fileInfo":{},"fileName":"docs/old/","uploadTimestamp":0},
                {"accountId":"a","action":"start","bucketId":"b","contentLength":0,"fileId":"4_large","fileInfo":{},"fileName":"docs/a.txt","uploadTimestamp":1710000009000},
                {"accountId":"a","action":"hide","bucketId":"b","contentLength":0,"contentSha1":null,"fileId":"4_hide","fileInfo":{},"fileName":"docs/a.txt","uploadTimestamp":1710000003000},
                {"accountId":"a","action":"upload","bucketId":"b","contentLength":2048,"contentSha1":"x","contentType":"text/plain","fileId":"4_v2","fileInfo":{"src_last_modified_millis":"1700000000123"},"fileName":"docs/a.txt","uploadTimestamp":1710000002999},
                {"accountId":"a","action":"upload","bucketId":"b","contentLength":1024,"fileId":"4_v1","fileInfo":{},"fileName":"docs/a.txt","uploadTimestamp":1710000001000},
                {"accountId":"a","action":"upload","bucketId":"b","contentLength":5,"fileId":"4_b1","fileInfo":{},"fileName":"docs/b.txt","uploadTimestamp":1710000000000}
            ],"nextFileName":"docs/b.txt","nextFileId":"4_b0"}"#,
        )
        .unwrap()
    }

    #[test]
    fn reads_versions_and_hide_markers() {
        let page = listed();
        assert_eq!(page.next_file_name.as_deref(), Some("docs/b.txt"));
        assert_eq!(page.next_file_id.as_deref(), Some("4_b0"));
        let mut newest_seen = None;
        let (records, folders) = history_records(&page.files, &mut newest_seen);
        assert_eq!(folders, ["docs/old/"]);
        let summary: Vec<_> = records
            .iter()
            .map(|record| {
                (
                    record.key.as_str(),
                    record.version.version_id.as_str(),
                    record.version.kind,
                    record.version.latest,
                )
            })
            .collect();
        // The unfinished large file is no version, so the hide marker is the newest entry.
        assert_eq!(
            summary,
            [
                ("docs/a.txt", "4_hide", VersionKind::DeleteMarker, true),
                ("docs/a.txt", "4_v2", VersionKind::Version, false),
                ("docs/a.txt", "4_v1", VersionKind::Version, false),
                ("docs/b.txt", "4_b1", VersionKind::Version, true),
            ]
        );
        // A version's time is when it was uploaded, not the source file's own.
        assert_eq!(records[1].version.modified, Some(1_710_000_002));
        assert_eq!(records[1].version.size, 2048);
        assert_eq!(newest_seen.as_deref(), Some("docs/b.txt"));

        // The next page goes on with older versions of the last name.
        let more: VersionList = serde_json::from_str(
            r#"{"files":[{"action":"upload","contentLength":4,"fileId":"4_b0","fileName":"docs/b.txt","uploadTimestamp":1700000000000}],"nextFileName":null,"nextFileId":null}"#,
        )
        .unwrap();
        let (records, _) = history_records(&more.files, &mut newest_seen);
        assert!(!records[0].version.latest);
        assert!(more.next_file_name.is_none());

        let mut hidden = HiddenFiles::default();
        let (records, _) = history_records(&page.files, &mut None);
        hidden.add_page(records);
        let files = hidden.finish("bucket");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "/bucket/docs/a.txt");
        assert_eq!(files[0].deleted(), Some(1_710_000_003));
        assert_eq!(files[0].newest_version().unwrap().version_id, "4_v2");
    }

    #[test]
    fn keeps_the_slash_of_a_folder_placeholder() {
        assert_eq!(
            name_keeping_slash("/photos/2024/"),
            (Some("photos".into()), "2024/".into())
        );
        assert_eq!(
            name_keeping_slash("/photos/2024/a.txt"),
            (Some("photos".into()), "2024/a.txt".into())
        );
        assert_eq!(
            name_keeping_slash("/photos/"),
            (Some("photos".into()), String::new())
        );
        assert_eq!(
            delete_request("2024/", "4_hidden")["fileName"],
            serde_json::json!("2024/")
        );
    }

    #[test]
    fn builds_requests() {
        assert_eq!(
            versions_request("b1", "docs/", true, None, 1000),
            json!({ "bucketId": "b1", "prefix": "docs/", "maxFileCount": 1000, "delimiter": "/" })
        );
        let start = ("docs/a.txt".to_string(), "4_v1".to_string());
        assert_eq!(
            versions_request("b1", "docs/a.txt", false, Some(&start), 1),
            json!({
                "bucketId": "b1",
                "prefix": "docs/a.txt",
                "maxFileCount": 1,
                "startFileName": "docs/a.txt",
                "startFileId": "4_v1",
            })
        );
        // The first page of one file's versions names no id, which starts at its newest.
        let first = ("docs/a.txt".to_string(), String::new());
        let body = versions_request("b1", "docs/a.txt", false, Some(&first), 1000);
        assert_eq!(body["startFileName"], "docs/a.txt");
        assert!(body.get("startFileId").is_none());
        assert!(body.get("delimiter").is_none());
        assert_eq!(
            delete_request("docs/a b.txt", "4_hide"),
            json!({ "fileName": "docs/a b.txt", "fileId": "4_hide" })
        );
        assert_eq!(
            download_address("https://f005.backblazeb2.com", "4_z27c88f1d182b150646ff0b16_f200ec353a2184825_d20160409_m004829_c000_v0001016_t0028"),
            "https://f005.backblazeb2.com/b2api/v4/b2_download_file_by_id?fileId=4_z27c88f1d182b150646ff0b16_f200ec353a2184825_d20160409_m004829_c000_v0001016_t0028"
        );
        assert_eq!(
            download_address("https://d", "a b&c"),
            "https://d/b2api/v4/b2_download_file_by_id?fileId=a%20b%26c"
        );
    }

    #[test]
    fn explains_what_a_restricted_key_cannot_do() {
        let all: Vec<String> = ["listFiles", "readFiles", "writeFiles", "deleteFiles"]
            .map(String::from)
            .to_vec();
        assert_eq!(refusal(&all, None, Need::Delete, "docs/a.txt"), None);
        let read_only: Vec<String> = ["listFiles", "readFiles"].map(String::from).to_vec();
        assert_eq!(refusal(&read_only, None, Need::List, "docs/a.txt"), None);
        assert_eq!(
            refusal(&read_only, None, Need::Write, "docs/a.txt").as_deref(),
            Some("This application key cannot restore a version, which writes a file: it lacks the writeFiles capability")
        );
        assert_eq!(
            refusal(&read_only, None, Need::Delete, "docs/a.txt").as_deref(),
            Some("This application key cannot remove versions or bring back deleted files: it lacks the deleteFiles capability")
        );
        let write_only: Vec<String> = ["writeFiles".to_string()].to_vec();
        assert_eq!(
            refusal(&write_only, None, Need::List, "docs/").as_deref(),
            Some(
                "This application key cannot list file versions: it lacks the listFiles capability"
            )
        );
        // A key for part of a bucket, asked about a name outside it.
        assert_eq!(
            refusal(&all, Some("reports/"), Need::List, "docs/a.txt").as_deref(),
            Some("This application key only reaches files whose names start with reports/")
        );
        assert_eq!(
            refusal(&all, Some("reports/"), Need::List, "reports/2024/a.txt"),
            None
        );
        // Backblaze named no capabilities: the service decides.
        assert_eq!(refusal(&[], None, Need::Delete, "docs/a.txt"), None);
    }
}
