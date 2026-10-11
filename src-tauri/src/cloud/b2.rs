//! Backblaze B2 over its native API, signed in with an application key. The top folder lists
//! the buckets the key can reach; inside one, the `/` in file names makes the folders, and an
//! empty `.bzEmpty` file keeps an empty folder the way Backblaze's own web interface does.
//! B2 keeps old versions of a file: replacing one adds a version, and deleting hides it, after
//! which the bucket's lifecycle rules decide how long the old versions stay.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, RANGE};
use reqwest::{Client, Request, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use sha1::{Digest, Sha1};

use super::api;
use super::s3::split;
use super::upload::{chunk_size_within, ChunkedUpload, UploadTarget};
use super::{FolderCache, HttpReader};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{DirListing, EntryKind, FileEntry};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;
use crate::ssh::ConnectProfile;

const AUTHORIZE_URL: &str = "https://api.backblazeb2.com/b2api/v4/b2_authorize_account";
const API_PATH: &str = "/b2api/v4";
/// The empty file Backblaze's web interface puts in a folder it creates.
const FOLDER_MARKER: &str = ".bzEmpty";
/// The file's own modification time in milliseconds, under the name Backblaze recommends.
const MTIME_INFO: &str = "src_last_modified_millis";
/// B2 bills listings per thousand names, so a page asks for no more.
const PAGE_SIZE: u32 = 1000;
/// `b2_copy_file` copies files up to this size; larger ones are copied in parts.
const COPY_LIMIT: u64 = 5_000_000_000;
const COPY_PART: u64 = 1024 * 1024 * 1024;
const MAX_PARTS: u64 = 10_000;
/// Lets B2 choose the content type from the file name.
const AUTO_CONTENT_TYPE: &str = "b2/x-auto";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Authorization {
    account_id: String,
    authorization_token: String,
    api_info: ApiInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiInfo {
    storage_api: StorageApi,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageApi {
    api_url: String,
    download_url: String,
    #[serde(default)]
    absolute_minimum_part_size: u64,
    #[serde(default)]
    allowed: Allowed,
}

/// What the application key may reach.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Allowed {
    /// `None` for every bucket in the account.
    #[serde(default)]
    buckets: Option<Vec<AllowedBucket>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AllowedBucket {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    bucket_id: String,
    bucket_name: String,
}

#[derive(Debug, Deserialize)]
struct BucketList {
    #[serde(default)]
    buckets: Vec<Bucket>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2File {
    /// `upload` for a file, `folder` for a name a delimiter grouped, `hide` and `start` for
    /// hidden files and unfinished large files.
    #[serde(default)]
    action: String,
    file_name: String,
    #[serde(default)]
    file_id: Option<String>,
    #[serde(default)]
    content_length: u64,
    #[serde(default)]
    upload_timestamp: i64,
    #[serde(default)]
    file_info: HashMap<String, String>,
}

impl B2File {
    fn is_file(&self) -> bool {
        self.action == "upload"
    }

    /// The source file's own time when the upload carried one, otherwise the upload's.
    fn modified(&self) -> Option<i64> {
        let millis = self
            .file_info
            .get(MTIME_INFO)
            .and_then(|millis| millis.trim().parse::<i64>().ok())
            .or((self.upload_timestamp > 0).then_some(self.upload_timestamp))?;
        Some(millis.div_euclid(1000))
    }

    fn stat(&self) -> RemoteStat {
        RemoteStat {
            is_dir: false,
            size: self.content_length,
            modified: self.modified(),
            permissions: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<B2File>,
    #[serde(default)]
    next_file_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionList {
    #[serde(default)]
    files: Vec<B2File>,
    #[serde(default)]
    next_file_name: Option<String>,
    #[serde(default)]
    next_file_id: Option<String>,
}

/// Where file content goes, with the token that authorizes it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadAddress {
    upload_url: String,
    authorization_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LargeFile {
    file_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopiedPart {
    content_sha1: String,
}

#[derive(Debug, Default, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

/// The account's addresses and token, renewed when the token expires (after a day at most).
struct Session {
    api_url: String,
    download_url: String,
    token: String,
}

/// What an upload sends its content to.
#[derive(Clone, Copy)]
enum Destination<'upload> {
    File { bucket_id: &'upload str },
    Part { file_id: &'upload str },
}

fn sha1_hex(data: &[u8]) -> String {
    Sha1::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A file name as the `X-Bz-File-Name` header takes it: percent-encoded, slashes kept.
fn encode_name(key: &str) -> String {
    key.split('/')
        .map(|segment| {
            percent_encoding::utf8_percent_encode(segment, super::URL_SEGMENT).to_string()
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub struct B2Fs {
    http: Client,
    key_id: String,
    key: String,
    account_id: String,
    session: Mutex<Arc<Session>>,
    /// Held while a new token is fetched, so parallel requests fetch only one.
    reauthorizing: tokio::sync::Mutex<()>,
    /// The buckets a restricted key can reach; `None` for all of the account's.
    allowed: Option<Vec<AllowedBucket>>,
    minimum_part_size: u64,
    home: String,
    bucket_ids: Mutex<HashMap<String, String>>,
    /// Upload addresses not in use, by bucket id. Each takes one upload at a time.
    upload_addresses: Mutex<HashMap<String, Vec<UploadAddress>>>,
    folders: FolderCache,
    folder_creation: tokio::sync::Mutex<()>,
}

impl B2Fs {
    pub async fn connect(profile: &ConnectProfile) -> AppResult<Self> {
        let http = api::http_client()?;
        let key_id = profile.username.trim().to_string();
        let key = super::password_of(profile)?;
        let authorization = authorize(&http, &key_id, &key).await?;
        let storage = authorization.api_info.storage_api;
        let allowed = storage.allowed.buckets;
        let (initial_bucket, _) = split(profile.initial_path.as_deref().unwrap_or_default());
        // A key for a single bucket starts in it.
        let only_bucket = match allowed.as_deref() {
            Some([only]) => only.name.clone(),
            _ => None,
        };
        let home_bucket = initial_bucket.or(only_bucket);
        let files = Self {
            http,
            key_id,
            key,
            account_id: authorization.account_id,
            session: Mutex::new(Arc::new(Session {
                api_url: storage.api_url,
                download_url: storage.download_url,
                token: authorization.authorization_token,
            })),
            reauthorizing: tokio::sync::Mutex::new(()),
            allowed,
            minimum_part_size: storage.absolute_minimum_part_size,
            home: home_bucket
                .as_deref()
                .map_or_else(|| "/".to_string(), |bucket| format!("/{bucket}")),
            bucket_ids: Mutex::new(HashMap::new()),
            upload_addresses: Mutex::new(HashMap::new()),
            folders: FolderCache::default(),
            folder_creation: tokio::sync::Mutex::new(()),
        };
        match &home_bucket {
            Some(bucket) => {
                if files.bucket_id(bucket).await?.is_none() {
                    return Err(no_such_bucket(bucket));
                }
            }
            None => {
                files.buckets().await?;
            }
        }
        Ok(files)
    }

    fn session(&self) -> Arc<Session> {
        self.session.lock().unwrap().clone()
    }

    /// Fetches a new token unless another request already replaced `stale`.
    async fn reauthorize(&self, stale: &str) -> AppResult<()> {
        let _reauthorizing = self.reauthorizing.lock().await;
        if self.session().token != stale {
            return Ok(());
        }
        let authorization = authorize(&self.http, &self.key_id, &self.key).await?;
        let storage = authorization.api_info.storage_api;
        *self.session.lock().unwrap() = Arc::new(Session {
            api_url: storage.api_url,
            download_url: storage.download_url,
            token: authorization.authorization_token,
        });
        // Upload addresses belong to the old token's session.
        self.upload_addresses.lock().unwrap().clear();
        Ok(())
    }

    /// Sends a request built for the current session, retrying what is safe to retry, and
    /// once more with a new token when B2 says the token expired.
    async fn send(
        &self,
        path: &str,
        idempotent: bool,
        build: &(dyn Fn(&Session) -> AppResult<Request> + Send + Sync),
    ) -> AppResult<Response> {
        let mut renewed = false;
        loop {
            let session = self.session();
            let attempt = || build(&session);
            let response = match idempotent {
                true => api::send_retrying_idempotent(&self.http, &attempt).await?,
                false => api::send_retrying(&self.http, &attempt).await?,
            };
            if response.status() != StatusCode::UNAUTHORIZED || renewed {
                return Ok(response);
            }
            let body = api::body_bytes(response).await?;
            let error: ErrorBody = serde_json::from_slice(&body).unwrap_or_default();
            // A `HEAD` answer has no body to say why.
            let expired = matches!(
                error.code.as_str(),
                "expired_auth_token" | "bad_auth_token" | ""
            );
            if !expired {
                return Err(service_error(StatusCode::UNAUTHORIZED, error, path));
            }
            self.reauthorize(&session.token).await?;
            renewed = true;
        }
    }

    /// Calls an API operation with a JSON body. `idempotent` operations only read, so they
    /// are retried whatever went wrong.
    async fn call<T: DeserializeOwned>(
        &self,
        operation: &str,
        body: serde_json::Value,
        path: &str,
        idempotent: bool,
    ) -> AppResult<T> {
        let response = self
            .send(path, idempotent, &|session| {
                self.http
                    .post(format!("{}{API_PATH}/{operation}", session.api_url))
                    .header(AUTHORIZATION, &session.token)
                    .json(&body)
                    .build()
                    .map_err(api::transport_error)
            })
            .await?;
        if !response.status().is_success() {
            return Err(failure(response, path).await);
        }
        api::decode(response).await
    }

    async fn buckets(&self) -> AppResult<Vec<Bucket>> {
        let buckets = match &self.allowed {
            None => {
                let body = json!({ "accountId": self.account_id });
                self.call::<BucketList>("b2_list_buckets", body, "/", true)
                    .await?
                    .buckets
            }
            // A restricted key may list only the buckets it names.
            Some(allowed) => {
                let mut buckets = Vec::new();
                for bucket in allowed {
                    match &bucket.name {
                        Some(name) => buckets.push(Bucket {
                            bucket_id: bucket.id.clone(),
                            bucket_name: name.clone(),
                        }),
                        None => {
                            let body =
                                json!({ "accountId": self.account_id, "bucketId": bucket.id });
                            let list: BucketList =
                                self.call("b2_list_buckets", body, "/", true).await?;
                            buckets.extend(list.buckets);
                        }
                    }
                }
                buckets
            }
        };
        let mut ids = self.bucket_ids.lock().unwrap();
        for bucket in &buckets {
            ids.insert(bucket.bucket_name.clone(), bucket.bucket_id.clone());
        }
        Ok(buckets)
    }

    async fn bucket_id(&self, name: &str) -> AppResult<Option<String>> {
        if let Some(id) = self.bucket_ids.lock().unwrap().get(name) {
            return Ok(Some(id.clone()));
        }
        let body = json!({ "accountId": self.account_id, "bucketName": name });
        let list: BucketList = self
            .call("b2_list_buckets", body, &format!("/{name}"), true)
            .await?;
        let Some(bucket) = list
            .buckets
            .into_iter()
            .find(|bucket| bucket.bucket_name == name)
        else {
            return Ok(None);
        };
        self.bucket_ids
            .lock()
            .unwrap()
            .insert(name.to_string(), bucket.bucket_id.clone());
        Ok(Some(bucket.bucket_id))
    }

    async fn existing_bucket(&self, name: &str) -> AppResult<String> {
        self.bucket_id(name)
            .await?
            .ok_or_else(|| no_such_bucket(name))
    }

    /// One page of the names under `prefix`, grouped into folders when `grouped`.
    async fn list_page(
        &self,
        bucket_id: &str,
        prefix: &str,
        grouped: bool,
        start: Option<&str>,
        count: u32,
        path: &str,
    ) -> AppResult<FileList> {
        let mut body = json!({
            "bucketId": bucket_id,
            "prefix": prefix,
            "maxFileCount": count,
        });
        if grouped {
            body["delimiter"] = json!("/");
        }
        if let Some(start) = start {
            body["startFileName"] = json!(start);
        }
        self.call("b2_list_file_names", body, path, true).await
    }

    /// Every file under `prefix`, page by page.
    async fn list_all(&self, bucket_id: &str, prefix: &str, path: &str) -> AppResult<Vec<B2File>> {
        let mut files = Vec::new();
        let mut start: Option<String> = None;
        loop {
            let page = self
                .list_page(bucket_id, prefix, false, start.as_deref(), PAGE_SIZE, path)
                .await?;
            files.extend(page.files.into_iter().filter(B2File::is_file));
            match page.next_file_name {
                Some(next) => start = Some(next),
                None => return Ok(files),
            }
        }
    }

    /// The file named exactly `key`, if there is one.
    async fn file_at(&self, bucket_id: &str, key: &str, path: &str) -> AppResult<Option<B2File>> {
        let page = self
            .list_page(bucket_id, key, false, Some(key), 1, path)
            .await?;
        Ok(page
            .files
            .into_iter()
            .find(|file| file.file_name == key && file.is_file()))
    }

    /// Whether anything is stored under `key/`, a folder marker included.
    async fn folder_exists(&self, bucket_id: &str, key: &str, path: &str) -> AppResult<bool> {
        let page = self
            .list_page(bucket_id, &format!("{key}/"), true, None, 1, path)
            .await?;
        Ok(!page.files.is_empty())
    }

    async fn stat_parts(&self, bucket: Option<&str>, key: &str) -> AppResult<Option<RemoteStat>> {
        let folder = RemoteStat {
            is_dir: true,
            size: 0,
            modified: None,
            permissions: None,
        };
        let Some(bucket) = bucket else {
            return Ok(Some(folder));
        };
        let Some(bucket_id) = self.bucket_id(bucket).await? else {
            return Ok(None);
        };
        if key.is_empty() {
            return Ok(Some(folder));
        }
        let path = format!("/{bucket}/{key}");
        if let Some(file) = self.file_at(&bucket_id, key, &path).await? {
            return Ok(Some(file.stat()));
        }
        Ok(self
            .folder_exists(&bucket_id, key, &path)
            .await?
            .then_some(folder))
    }

    async fn create_bucket(&self, name: &str) -> AppResult<()> {
        let body = json!({
            "accountId": self.account_id,
            "bucketName": name,
            "bucketType": "allPrivate",
        });
        let bucket: Bucket = self
            .call("b2_create_bucket", body, &format!("/{name}"), false)
            .await?;
        self.bucket_ids
            .lock()
            .unwrap()
            .insert(bucket.bucket_name, bucket.bucket_id);
        Ok(())
    }

    /// Hides a file, so it no longer shows while its versions stay until lifecycle rules
    /// remove them. One that is already gone counts as hidden.
    async fn hide(&self, bucket_id: &str, key: &str, path: &str) -> AppResult<()> {
        let body = json!({ "bucketId": bucket_id, "fileName": key });
        match self.call::<B2File>("b2_hide_file", body, path, false).await {
            Ok(_) => Ok(()),
            Err(error) if error.kind == ErrorKind::NotFound => Ok(()),
            Err(error) if error.message.contains("already_hidden") => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Empties a bucket of every version of every file, then deletes it.
    async fn delete_bucket(&self, name: &str, bucket_id: &str) -> AppResult<()> {
        let path = format!("/{name}");
        let mut start: Option<(String, String)> = None;
        loop {
            let mut body = json!({ "bucketId": bucket_id, "maxFileCount": PAGE_SIZE });
            if let Some((file_name, file_id)) = &start {
                body["startFileName"] = json!(file_name);
                body["startFileId"] = json!(file_id);
            }
            let page: VersionList = self
                .call("b2_list_file_versions", body, &path, true)
                .await?;
            for version in &page.files {
                let Some(file_id) = &version.file_id else {
                    continue;
                };
                if version.action == "start" {
                    self.cancel_large_file(file_id, &path).await?;
                } else {
                    let body = json!({ "fileName": version.file_name, "fileId": file_id });
                    match self
                        .call::<serde_json::Value>("b2_delete_file_version", body, &path, false)
                        .await
                    {
                        Ok(_) => {}
                        Err(error) if error.kind == ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
            }
            match (page.next_file_name, page.next_file_id) {
                (Some(file_name), Some(file_id)) => start = Some((file_name, file_id)),
                _ => break,
            }
        }
        let body = json!({ "accountId": self.account_id, "bucketId": bucket_id });
        self.call::<serde_json::Value>("b2_delete_bucket", body, &path, false)
            .await?;
        self.bucket_ids.lock().unwrap().remove(name);
        Ok(())
    }

    async fn start_large_file(
        &self,
        bucket_id: &str,
        key: &str,
        modified: Option<i64>,
        path: &str,
    ) -> AppResult<String> {
        let mut info = serde_json::Map::new();
        if let Some(modified) = modified {
            info.insert(MTIME_INFO.into(), json!((modified * 1000).to_string()));
        }
        let body = json!({
            "bucketId": bucket_id,
            "fileName": key,
            "contentType": AUTO_CONTENT_TYPE,
            "fileInfo": info,
        });
        let started: LargeFile = self.call("b2_start_large_file", body, path, false).await?;
        Ok(started.file_id)
    }

    async fn finish_large_file(
        &self,
        file_id: &str,
        sha1s: &[String],
        path: &str,
    ) -> AppResult<()> {
        let body = json!({ "fileId": file_id, "partSha1Array": sha1s });
        self.call::<serde_json::Value>("b2_finish_large_file", body, path, false)
            .await
            .map(drop)
    }

    async fn cancel_large_file(&self, file_id: &str, path: &str) -> AppResult<()> {
        let body = json!({ "fileId": file_id });
        match self
            .call::<serde_json::Value>("b2_cancel_large_file", body, path, false)
            .await
        {
            Err(error) if error.kind != ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    /// Copies a file within its bucket, in parts when it is too large for one request.
    async fn copy_file(
        &self,
        bucket_id: &str,
        source: &B2File,
        target: &str,
        path: &str,
    ) -> AppResult<()> {
        let source_id = source
            .file_id
            .as_deref()
            .ok_or_else(|| super::not_found(path))?;
        if source.content_length <= COPY_LIMIT {
            let body = json!({
                "sourceFileId": source_id,
                "fileName": target,
                "metadataDirective": "COPY",
            });
            return self
                .call::<B2File>("b2_copy_file", body, path, false)
                .await
                .map(drop);
        }
        let mut info = serde_json::Map::new();
        if let Some(millis) = source.file_info.get(MTIME_INFO) {
            info.insert(MTIME_INFO.into(), json!(millis));
        }
        let body = json!({
            "bucketId": bucket_id,
            "fileName": target,
            "contentType": AUTO_CONTENT_TYPE,
            "fileInfo": info,
        });
        let large: LargeFile = self.call("b2_start_large_file", body, path, false).await?;
        let size = source.content_length;
        let part_size = COPY_PART.max(size.div_ceil(MAX_PARTS));
        let copied: AppResult<()> = async {
            let mut sha1s = Vec::new();
            for (index, start) in (0..size).step_by(part_size as usize).enumerate() {
                let end = (start + part_size).min(size) - 1;
                let body = json!({
                    "sourceFileId": source_id,
                    "largeFileId": large.file_id,
                    "partNumber": index + 1,
                    "range": format!("bytes={start}-{end}"),
                });
                let part: CopiedPart = self.call("b2_copy_part", body, path, false).await?;
                sha1s.push(part.content_sha1);
            }
            self.finish_large_file(&large.file_id, &sha1s, path).await
        }
        .await;
        if copied.is_err() {
            let _ = self.cancel_large_file(&large.file_id, path).await;
        }
        copied
    }

    /// An address to upload to: a spare one for a whole file, a new one for a part.
    async fn upload_address(
        &self,
        destination: Destination<'_>,
        path: &str,
    ) -> AppResult<UploadAddress> {
        match destination {
            Destination::File { bucket_id } => {
                let spare = self
                    .upload_addresses
                    .lock()
                    .unwrap()
                    .get_mut(bucket_id)
                    .and_then(Vec::pop);
                match spare {
                    Some(address) => Ok(address),
                    None => {
                        let body = json!({ "bucketId": bucket_id });
                        self.call("b2_get_upload_url", body, path, true).await
                    }
                }
            }
            Destination::Part { file_id } => {
                let body = json!({ "fileId": file_id });
                self.call("b2_get_upload_part_url", body, path, true).await
            }
        }
    }

    /// Sends content to an upload address, taking a fresh address after a failure as B2
    /// asks: each serves one upload at a time and may be busy or expire.
    async fn send_content(
        &self,
        destination: Destination<'_>,
        address: &mut Option<UploadAddress>,
        headers: &[(&'static str, String)],
        data: &Bytes,
        path: &str,
    ) -> AppResult<()> {
        let checksum = sha1_hex(data);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let current = match address.take() {
                Some(current) => current,
                None => self.upload_address(destination, path).await?,
            };
            let mut builder = self
                .http
                .post(&current.upload_url)
                .header(AUTHORIZATION, &current.authorization_token)
                .header(CONTENT_LENGTH, data.len())
                .header("X-Bz-Content-Sha1", &checksum)
                .body(data.clone());
            for (name, value) in headers {
                builder = builder.header(*name, value);
            }
            let request = builder.build().map_err(api::transport_error)?;
            match api::send_request(&self.http, request, api::STALL_TIMEOUT).await {
                Ok(response) if response.status().is_success() => {
                    *address = Some(current);
                    return Ok(());
                }
                Ok(response)
                    if attempt < api::ATTEMPTS && upload_should_retry(response.status()) => {}
                Ok(response) => return Err(failure(response, path).await),
                Err(_) if attempt < api::ATTEMPTS => {}
                Err(failure) => return Err(failure.into()),
            }
            tokio::time::sleep(api::backoff(attempt)).await;
        }
    }

    /// Keeps an upload address for the next whole-file upload to the bucket.
    fn spare_address(&self, bucket_id: &str, address: UploadAddress) {
        self.upload_addresses
            .lock()
            .unwrap()
            .entry(bucket_id.to_string())
            .or_default()
            .push(address);
    }

    async fn upload_small(
        &self,
        bucket_id: &str,
        key: &str,
        modified: Option<i64>,
        data: Bytes,
        path: &str,
    ) -> AppResult<()> {
        let mut headers = vec![
            ("X-Bz-File-Name", encode_name(key)),
            (CONTENT_TYPE.as_str(), AUTO_CONTENT_TYPE.to_string()),
        ];
        if let Some(modified) = modified {
            headers.push((
                "X-Bz-Info-src_last_modified_millis",
                (modified * 1000).to_string(),
            ));
        }
        let mut address = None;
        self.send_content(
            Destination::File { bucket_id },
            &mut address,
            &headers,
            &data,
            path,
        )
        .await?;
        if let Some(address) = address {
            self.spare_address(bucket_id, address);
        }
        Ok(())
    }

    fn bucket_and_key(&self, path: &str) -> (Option<String>, String) {
        split(&self.resolve(path))
    }
}

async fn authorize(http: &Client, key_id: &str, key: &str) -> AppResult<Authorization> {
    let build = || {
        http.get(AUTHORIZE_URL)
            .basic_auth(key_id, Some(key))
            .build()
            .map_err(api::transport_error)
    };
    let response = api::send_retrying(http, &build).await?;
    match response.status() {
        status if status.is_success() => api::decode(response).await,
        StatusCode::UNAUTHORIZED => Err(AppError::new(
            ErrorKind::AuthFailed,
            "Backblaze refused the application key; check the key ID and the key",
        )),
        _ => Err(failure(response, "/").await),
    }
}

/// A failed upload goes again to a fresh address: the old one may be busy, expired, or on a
/// server that is going away.
fn upload_should_retry(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn no_such_bucket(bucket: &str) -> AppError {
    AppError::new(
        ErrorKind::NotFound,
        format!("The bucket {bucket} does not exist"),
    )
    .with_path(format!("/{bucket}"))
}

/// An error for a failed answer, from the JSON B2 sends with one.
async fn failure(response: Response, path: &str) -> AppError {
    let status = response.status();
    let error = match api::body_bytes(response).await {
        Ok(body) => serde_json::from_slice(&body).unwrap_or_default(),
        Err(error) => return error,
    };
    service_error(status, error, path)
}

fn service_error(status: StatusCode, error: ErrorBody, path: &str) -> AppError {
    let message = match error.message.trim() {
        "" if error.code.is_empty() => format!("Backblaze B2 answered {status}"),
        "" => format!("Backblaze B2 refused the request ({})", error.code),
        message => format!("{message} ({})", error.code),
    };
    let kind = match error.code.as_str() {
        "not_found" | "file_not_present" | "no_such_file" => return super::not_found(path),
        "bad_bucket_id" | "invalid_bucket_id" => {
            let (bucket, _) = split(path);
            return no_such_bucket(bucket.as_deref().unwrap_or_default());
        }
        "duplicate_bucket_name" => ErrorKind::AlreadyExists,
        "bad_auth_token" | "expired_auth_token" => ErrorKind::AuthFailed,
        "unauthorized" | "access_denied" => {
            return AppError::new(
                ErrorKind::PermissionDenied,
                format!("The application key does not allow this: {message}"),
            )
            .with_path(path)
        }
        "cap_exceeded"
        | "storage_cap_exceeded"
        | "transaction_cap_exceeded"
        | "download_cap_exceeded" => ErrorKind::PermissionDenied,
        "too_many_requests" | "service_unavailable" | "internal_error" | "request_timeout" => {
            ErrorKind::Cloud
        }
        _ => match status {
            StatusCode::UNAUTHORIZED => ErrorKind::AuthFailed,
            StatusCode::FORBIDDEN => ErrorKind::PermissionDenied,
            StatusCode::NOT_FOUND => return super::not_found(path),
            StatusCode::CONFLICT => ErrorKind::AlreadyExists,
            status if status.is_server_error() => ErrorKind::Cloud,
            _ => ErrorKind::InvalidInput,
        },
    };
    AppError::new(kind, message).with_path(path)
}

fn folder_entry(path: &str, name: &str) -> FileEntry {
    super::entry(path, name, EntryKind::Dir, 0, None)
}

#[async_trait]
impl RemoteFileSystem for B2Fs {
    fn protocol(&self) -> Protocol {
        Protocol::BackblazeB2
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
        let (bucket, key) = split(&path);
        match self.stat_parts(bucket.as_deref(), &key).await? {
            Some(stat) if stat.is_dir => Ok(path),
            Some(_) => Err(super::not_a_folder(&path)),
            None if key.is_empty() => Err(no_such_bucket(bucket.as_deref().unwrap_or_default())),
            None => Err(super::not_found(&path)),
        }
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let directory = protocol::normalize(&self.resolve(path));
        let (bucket, key) = split(&directory);
        let mut entries = Vec::new();
        match &bucket {
            None => {
                for bucket in self.buckets().await? {
                    let path = format!("/{}", bucket.bucket_name);
                    entries.push(folder_entry(&path, &bucket.bucket_name));
                }
            }
            Some(bucket) => {
                let bucket_id = self.existing_bucket(bucket).await?;
                let prefix = match key.as_str() {
                    "" => String::new(),
                    key => format!("{key}/"),
                };
                let mut start: Option<String> = None;
                loop {
                    let page = self
                        .list_page(
                            &bucket_id,
                            &prefix,
                            true,
                            start.as_deref(),
                            PAGE_SIZE,
                            &directory,
                        )
                        .await?;
                    for file in &page.files {
                        let Some(name) = file.file_name.strip_prefix(&prefix) else {
                            continue;
                        };
                        if file.action == "folder" {
                            let name = name.trim_end_matches('/');
                            if name.is_empty() || name.contains('/') {
                                continue;
                            }
                            let path = remote_path::join(&directory, name);
                            self.folders.insert(&path, "");
                            entries.push(folder_entry(&path, name));
                        } else if file.is_file()
                            && !name.is_empty()
                            && !name.contains('/')
                            && name != FOLDER_MARKER
                        {
                            let path = remote_path::join(&directory, name);
                            entries.push(super::entry(
                                &path,
                                name,
                                EntryKind::File,
                                file.content_length,
                                file.modified(),
                            ));
                        }
                    }
                    match page.next_file_name {
                        Some(next) => start = Some(next),
                        None => break,
                    }
                }
            }
        }
        Ok(DirListing {
            parent: remote_path::parent(&directory),
            path: directory,
            entries,
        })
    }

    /// A folder at the top is a new private bucket.
    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        protocol::validate_name(name)?;
        let parent = protocol::normalize(&self.resolve(parent));
        let path = remote_path::join(&parent, name);
        let (bucket, key) = split(&path);
        let bucket = bucket.unwrap_or_default();
        if key.is_empty() {
            self.create_bucket(&bucket).await?;
            return Ok(path);
        }
        if self.stat_parts(Some(&bucket), &key).await?.is_some() {
            return Err(protocol::already_exists(&path));
        }
        let bucket_id = self.existing_bucket(&bucket).await?;
        let marker = format!("{key}/{FOLDER_MARKER}");
        self.upload_small(&bucket_id, &marker, None, Bytes::new(), &path)
            .await?;
        self.folders.insert(&path, "");
        Ok(path)
    }

    /// B2 has no rename: files are copied to the new name, then hidden.
    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let path = protocol::normalize(&self.resolve(path));
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root folder"))?;
        let target = remote_path::join(&parent, new_name);
        let (bucket, key) = split(&path);
        let (Some(bucket), false) = (bucket, key.is_empty()) else {
            return Err(AppError::unsupported(
                "Backblaze B2 buckets cannot be renamed",
            ));
        };
        let bucket_id = self.existing_bucket(&bucket).await?;
        let (_, target_key) = split(&target);
        if self.stat_parts(Some(&bucket), &target_key).await?.is_some() {
            return Err(protocol::already_exists(&target));
        }
        if let Some(file) = self.file_at(&bucket_id, &key, &path).await? {
            self.copy_file(&bucket_id, &file, &target_key, &path)
                .await?;
            self.hide(&bucket_id, &key, &path).await?;
            return Ok(target);
        }
        let prefix = format!("{key}/");
        let files = self.list_all(&bucket_id, &prefix, &path).await?;
        if files.is_empty() {
            return Err(super::not_found(&path));
        }
        for file in &files {
            let moved = format!("{target_key}/{}", &file.file_name[prefix.len()..]);
            self.copy_file(&bucket_id, file, &moved, &path).await?;
        }
        for file in &files {
            self.hide(&bucket_id, &file.file_name, &path).await?;
        }
        self.folders.forget(&path);
        Ok(target)
    }

    /// Hides the files, as deleting does in Backblaze's web interface without versions shown;
    /// the bucket's lifecycle rules decide when their versions go. Deleting a bucket removes
    /// every version of every file in it for good.
    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        for path in protocol::outermost(&resolved) {
            let (Some(bucket), key) = split(&path) else {
                return Err(AppError::invalid("Refusing to delete /"));
            };
            let bucket_id = self.existing_bucket(&bucket).await?;
            if key.is_empty() {
                self.delete_bucket(&bucket, &bucket_id).await?;
                self.folders.forget(&path);
                continue;
            }
            if self.file_at(&bucket_id, &key, &path).await?.is_some() {
                self.hide(&bucket_id, &key, &path).await?;
            }
            for file in self.list_all(&bucket_id, &format!("{key}/"), &path).await? {
                self.hide(&bucket_id, &file.file_name, &path).await?;
            }
            self.folders.forget(&path);
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        let (bucket, key) = self.bucket_and_key(path);
        self.stat_parts(bucket.as_deref(), &key).await
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let path = protocol::normalize(&self.resolve(path));
        if self.folders.get(&path).is_some() {
            return Ok(());
        }
        let (Some(bucket), key) = split(&path) else {
            return Ok(());
        };
        let _creating = self.folder_creation.lock().await;
        let bucket_id = match self.bucket_id(&bucket).await? {
            Some(id) => id,
            None => {
                self.create_bucket(&bucket).await?;
                self.existing_bucket(&bucket).await?
            }
        };
        if !key.is_empty() && !self.folder_exists(&bucket_id, &key, &path).await? {
            if self.file_at(&bucket_id, &key, &path).await?.is_some() {
                return Err(protocol::not_a_folder(&path));
            }
            let marker = format!("{key}/{FOLDER_MARKER}");
            self.upload_small(&bucket_id, &marker, None, Bytes::new(), &path)
                .await?;
        }
        self.folders.insert(&path, "");
        Ok(())
    }

    /// A file's time is set when it is uploaded and cannot change without copying it.
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
        let path = protocol::normalize(&self.resolve(path));
        let (Some(bucket), key) = split(&path) else {
            return Err(super::is_a_folder(&path));
        };
        if key.is_empty() {
            return Err(super::is_a_folder(&path));
        }
        if range.start >= range.end {
            let bucket_id = self.existing_bucket(&bucket).await?;
            if self.file_at(&bucket_id, &key, &path).await?.is_none() {
                return Err(super::not_found(&path));
            }
            return Ok(Box::new(HttpReader::new(None, 0)));
        }
        let requested_range = (range.start > 0 || range.end != u64::MAX).then(|| {
            let last = match range.end {
                u64::MAX => String::new(),
                end => (end - 1).to_string(),
            };
            format!("bytes={}-{last}", range.start)
        });
        let file_path = format!(
            "/file/{}{}",
            percent_encoding::utf8_percent_encode(&bucket, super::URL_SEGMENT),
            super::encode_path(&key)
        );
        let response = self
            .send(&path, true, &|session| {
                let request = self
                    .http
                    .get(format!("{}{file_path}", session.download_url))
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

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        if request.offset > 0 {
            return Err(super::cannot_resume(Protocol::BackblazeB2));
        }
        let path = protocol::normalize(&request.path);
        let (Some(bucket), key) = split(&path) else {
            return Err(AppError::invalid("Files go inside a bucket"));
        };
        if key.is_empty() {
            return Err(AppError::invalid("Files go inside a bucket"));
        }
        let bucket_id = self.existing_bucket(&bucket).await?;
        let minimum = usize::try_from(self.minimum_part_size).unwrap_or(usize::MAX);
        let chunk_size = chunk_size_within(request.size, MAX_PARTS).max(minimum);
        let target = B2Upload {
            files: self,
            bucket_id,
            key,
            path,
            modified: request.modified,
            chunk_size,
            large_file: None,
            part_address: None,
            sha1s: Vec::new(),
        };
        Ok(Box::new(ChunkedUpload::new(target, request.size)))
    }
}

/// Adds a version at the file's name: in one request up to a chunk's size, as a large file in
/// parts above it.
struct B2Upload {
    files: Arc<B2Fs>,
    bucket_id: String,
    key: String,
    path: String,
    modified: Option<i64>,
    chunk_size: usize,
    /// The large file in progress, cancelled if the upload does not finish.
    large_file: Option<String>,
    part_address: Option<UploadAddress>,
    sha1s: Vec<String>,
}

impl Drop for B2Upload {
    fn drop(&mut self) {
        let Some(file_id) = self.large_file.take() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (files, path) = (self.files.clone(), self.path.clone());
        // The parts of an unfinished large file are kept, and billed, until it is cancelled.
        runtime.spawn(async move {
            let _ = files.cancel_large_file(&file_id, &path).await;
        });
    }
}

#[async_trait]
impl UploadTarget for B2Upload {
    fn single_request_limit(&self) -> usize {
        self.chunk_size
    }

    fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    async fn upload_whole(&mut self, data: Bytes) -> AppResult<()> {
        self.files
            .upload_small(&self.bucket_id, &self.key, self.modified, data, &self.path)
            .await
    }

    async fn start_session(&mut self) -> AppResult<()> {
        let file_id = self
            .files
            .start_large_file(&self.bucket_id, &self.key, self.modified, &self.path)
            .await?;
        self.large_file = Some(file_id);
        Ok(())
    }

    async fn send_chunk(&mut self, _offset: u64, data: Bytes, last: bool) -> AppResult<()> {
        let file_id = self
            .large_file
            .clone()
            .ok_or_else(|| AppError::new(ErrorKind::Cloud, "No upload is open"))?;
        let part_number = (self.sha1s.len() + 1).to_string();
        self.files
            .send_content(
                Destination::Part { file_id: &file_id },
                &mut self.part_address,
                &[("X-Bz-Part-Number", part_number)],
                &data,
                &self.path,
            )
            .await?;
        self.sha1s.push(sha1_hex(&data));
        if last {
            self.files
                .finish_large_file(&file_id, &self.sha1s, &self.path)
                .await?;
            self.large_file = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_authorization() {
        let unrestricted: Authorization = serde_json::from_str(
            r#"{"accountId":"a1","authorizationToken":"t","apiInfo":{"groupsApi":{},"storageApi":{"absoluteMinimumPartSize":5000000,"apiUrl":"https://api005.backblazeb2.com","downloadUrl":"https://f005.backblazeb2.com","recommendedPartSize":100000000,"s3ApiUrl":"https://s3.us-east-005.backblazeb2.com","allowed":{"buckets":null,"capabilities":["listBuckets"],"namePrefix":null}}},"applicationKeyExpirationTimestamp":null}"#,
        )
        .unwrap();
        let storage = unrestricted.api_info.storage_api;
        assert_eq!(storage.api_url, "https://api005.backblazeb2.com");
        assert_eq!(storage.absolute_minimum_part_size, 5_000_000);
        assert!(storage.allowed.buckets.is_none());
        let restricted: Authorization = serde_json::from_str(
            r#"{"accountId":"a1","authorizationToken":"t","apiInfo":{"storageApi":{"apiUrl":"u","downloadUrl":"d","allowed":{"buckets":[{"id":"b1","name":"photos"},{"id":"b2","name":null}],"capabilities":[]}}}}"#,
        )
        .unwrap();
        let buckets = restricted.api_info.storage_api.allowed.buckets.unwrap();
        assert_eq!(buckets[0].name.as_deref(), Some("photos"));
        assert_eq!(buckets[1].name, None);
    }

    #[test]
    fn reads_listed_files() {
        let list: FileList = serde_json::from_str(
            r#"{"files":[
                {"accountId":"a","action":"folder","bucketId":"b","contentLength":0,"contentSha1":null,"fileId":null,"fileInfo":{},"fileName":"photos/cats/","uploadTimestamp":0},
                {"accountId":"a","action":"upload","bucketId":"b","contentLength":2048,"contentSha1":"x","contentType":"image/jpeg","fileId":"4_z1","fileInfo":{"src_last_modified_millis":"1700000000123"},"fileName":"photos/a.jpg","uploadTimestamp":1710000000000},
                {"accountId":"a","action":"upload","bucketId":"b","contentLength":1,"fileId":"4_z2","fileInfo":{},"fileName":"photos/b.jpg","uploadTimestamp":1710000000999}
            ],"nextFileName":"photos/c"}"#,
        )
        .unwrap();
        assert!(!list.files[0].is_file());
        assert_eq!(list.files[1].modified(), Some(1_700_000_000));
        assert_eq!(list.files[2].modified(), Some(1_710_000_000));
        assert_eq!(list.files[1].stat().size, 2048);
        assert_eq!(list.next_file_name.as_deref(), Some("photos/c"));
    }

    #[test]
    fn encodes_file_names_for_the_header() {
        assert_eq!(encode_name("a b/c+d&é.txt"), "a%20b/c%2Bd%26%C3%A9.txt");
        assert_eq!(encode_name("dir/.bzEmpty"), "dir/.bzEmpty");
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn maps_service_errors() {
        let error = |code: &str| ErrorBody {
            code: code.into(),
            message: "text".into(),
        };
        assert_eq!(
            service_error(StatusCode::NOT_FOUND, error("not_found"), "/b/k").kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            service_error(StatusCode::UNAUTHORIZED, error("unauthorized"), "/b").kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            service_error(StatusCode::FORBIDDEN, error("storage_cap_exceeded"), "/b").kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            service_error(
                StatusCode::BAD_REQUEST,
                error("duplicate_bucket_name"),
                "/b"
            )
            .kind,
            ErrorKind::AlreadyExists
        );
        let missing = service_error(StatusCode::BAD_REQUEST, error("bad_bucket_id"), "/gone/k");
        assert_eq!(missing.message, "The bucket gone does not exist");
        assert!(upload_should_retry(StatusCode::SERVICE_UNAVAILABLE));
        assert!(upload_should_retry(StatusCode::UNAUTHORIZED));
        assert!(!upload_should_retry(StatusCode::BAD_REQUEST));
    }
}
