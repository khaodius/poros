//! Amazon S3 and S3-compatible services (MinIO, Wasabi, Cloudflare R2, Backblaze B2's S3 API,
//! DigitalOcean Spaces, Ceph and the like). The top folder lists the buckets; inside one, the
//! `/` in object keys makes the folders, with an empty `name/` object keeping an empty folder.
//! Requests carry Signature Version 4, and a bucket in another region is found from the
//! service's answer.

mod signing;
mod xml;

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::header::{HeaderMap, CONTENT_LENGTH, ETAG, LAST_MODIFIED};
use reqwest::{Client, Method, Request, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};

use super::api;
use super::upload::{chunk_size_within, ChunkedUpload, UploadTarget};
use super::{FolderCache, HttpReader, Login};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::Events;
use crate::model::{DirListing, EntryKind, FileEntry};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;
use crate::ssh::ConnectProfile;
use crate::timestamp;
use crate::tls::ServerTls;

/// Amazon's own endpoint, used when the connection names none.
pub const AWS_ENDPOINT: &str = "s3.amazonaws.com";
const DEFAULT_REGION: &str = "us-east-1";
const BUCKET_REGION_HEADER: &str = "x-amz-bucket-region";
/// The modification time of the uploaded file, in seconds, where rclone and others keep it.
const MTIME_HEADER: &str = "x-amz-meta-mtime";
const PAGE_SIZE: &str = "1000";
/// `DeleteObjects` takes this many keys at once.
const DELETE_BATCH: usize = 1000;
/// `CopyObject` copies objects up to this size; larger ones are copied in parts.
const COPY_LIMIT: u64 = 5 * 1024 * 1024 * 1024;
const COPY_PART: u64 = 1024 * 1024 * 1024;
const MAX_PARTS: u64 = 10_000;

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct S3Options {
    /// Empty takes it from the endpoint's name, or else `us-east-1`; a bucket in another region
    /// is found from the service's answers.
    pub region: String,
    pub addressing: Addressing,
}

/// Where the bucket goes in a request's address. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Addressing {
    /// The bucket's own host name on Amazon when the name allows it, a path elsewhere.
    #[default]
    Automatic,
    /// `https://endpoint/bucket/key`, which MinIO and most self-hosted servers expect.
    Path,
    /// `https://bucket.endpoint/key`.
    VirtualHosted,
}

/// The endpoint a connection's label shows.
pub fn endpoint_label(profile: &ConnectProfile) -> String {
    match Endpoint::parse(profile) {
        Ok(endpoint) if endpoint.port == endpoint.default_port() => endpoint.host,
        Ok(endpoint) => format!("{}:{}", endpoint.host, endpoint.port),
        Err(_) => profile.host.trim().to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Endpoint {
    secure: bool,
    host: String,
    port: u16,
}

impl Endpoint {
    /// The host field may hold just a name, or a URL with `http://` for a server without TLS.
    fn parse(profile: &ConnectProfile) -> AppResult<Self> {
        let typed = profile.host.trim();
        let (secure, rest) = match typed.split_once("://") {
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => (false, rest),
            Some((_, rest)) => (true, rest),
            None => (true, typed),
        };
        let authority = rest.split('/').next().unwrap_or_default();
        let authority = match authority {
            "" => AWS_ENDPOINT,
            authority => authority,
        };
        let scheme = if secure { "https" } else { "http" };
        let url = Url::parse(&format!("{scheme}://{authority}/"))
            .ok()
            .filter(|url| url.host_str().is_some_and(|host| !host.is_empty()))
            .ok_or_else(|| AppError::invalid(format!("{typed} is not a valid endpoint")))?;
        let port = match (url.port(), profile.port) {
            (Some(port), _) => port,
            // The port field still holds the HTTPS default after typing `http://`.
            (None, 443) if !secure => 80,
            (None, port) => port,
        };
        Ok(Self {
            secure,
            host: url.host_str().unwrap_or_default().to_ascii_lowercase(),
            port,
        })
    }

    fn default_port(&self) -> u16 {
        if self.secure {
            443
        } else {
            80
        }
    }

    /// The `host` header: the name, with the port when it is not the scheme's own.
    fn authority(&self, host: &str) -> String {
        if self.port == self.default_port() {
            host.to_string()
        } else {
            format!("{host}:{}", self.port)
        }
    }

    /// Amazon's own endpoints, whose host names follow the bucket's region. `Some(None)` for
    /// the global `s3.amazonaws.com`.
    fn aws_region(&self) -> Option<Option<String>> {
        let rest = self.host.strip_suffix(".amazonaws.com")?;
        if rest == "s3" {
            return Some(None);
        }
        let region = rest
            .strip_prefix("s3.")
            .or_else(|| rest.strip_prefix("s3-"))?;
        let is_region = !region.is_empty()
            && region.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            });
        is_region.then(|| Some(region.to_string()))
    }
}

/// A bucket name that can be a host name, without dots that the certificate would not cover.
fn fits_in_host_name(bucket: &str) -> bool {
    (3..=63).contains(&bucket.len())
        && bucket.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && !bucket.starts_with('-')
        && !bucket.ends_with('-')
}

/// A path's bucket and key: `/photos/2024/a.jpg` is `photos` and `2024/a.jpg`.
pub(super) fn split(path: &str) -> (Option<String>, String) {
    let path = protocol::normalize(path);
    let trimmed = path.trim_start_matches('/');
    match trimmed.split_once('/') {
        Some((bucket, key)) => (Some(bucket.to_string()), key.to_string()),
        None if trimmed.is_empty() => (None, String::new()),
        None => (Some(trimmed.to_string()), String::new()),
    }
}

/// Escapes a query value the way the signature expects.
fn escape(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, super::URL_SEGMENT).to_string()
}

/// A request, kept so it can be signed again for a retry or another region.
struct S3Request {
    method: Method,
    bucket: Option<String>,
    key: String,
    query: Vec<(String, String)>,
    /// Lower-case names.
    headers: Vec<(String, String)>,
    body: Bytes,
    payload_hash: String,
}

impl S3Request {
    fn new(method: Method, bucket: Option<&str>, key: &str) -> Self {
        Self {
            method,
            bucket: bucket.map(str::to_string),
            key: key.to_string(),
            query: Vec::new(),
            headers: Vec::new(),
            body: Bytes::new(),
            payload_hash: signing::sha256_hex(b""),
        }
    }

    fn query(mut self, name: &str, value: &str) -> Self {
        self.query.push((name.to_string(), value.to_string()));
        self
    }

    fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    fn body(mut self, body: Bytes) -> Self {
        self.payload_hash = signing::sha256_hex(&body);
        self.body = body;
        self
    }

    /// Sorted and escaped, as both the address and the signature have it.
    fn canonical_query(&self) -> String {
        let mut pairs: Vec<(String, String)> = self
            .query
            .iter()
            .map(|(name, value)| (escape(name), escape(value)))
            .collect();
        pairs.sort();
        pairs
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&")
    }
}

pub struct S3Fs {
    http: Client,
    tls: Option<ServerTls>,
    endpoint: Endpoint,
    access_key: String,
    secret_key: String,
    region: String,
    addressing: Addressing,
    home: String,
    /// Regions of buckets that are not in `region`.
    bucket_regions: Mutex<HashMap<String, String>>,
    folders: FolderCache,
    folder_creation: tokio::sync::Mutex<()>,
}

impl S3Fs {
    pub async fn connect(
        session_id: &str,
        profile: &ConnectProfile,
        login: &Login<'_>,
        events: &Events,
    ) -> AppResult<(Self, String)> {
        let endpoint = Endpoint::parse(profile)?;
        let tls = match endpoint.secure {
            true => Some(login.server_tls(&endpoint.host, endpoint.port, session_id, events)?),
            false => None,
        };
        let http = match &tls {
            Some(tls) => api::approving_client(tls)?,
            None => api::http_client()?,
        };
        let region = match profile.s3.region.trim() {
            "" => endpoint
                .aws_region()
                .flatten()
                .unwrap_or_else(|| DEFAULT_REGION.to_string()),
            region => region.to_string(),
        };
        let (bucket, _) = split(profile.initial_path.as_deref().unwrap_or_default());
        let files = Self {
            http,
            tls,
            endpoint,
            access_key: profile.username.trim().to_string(),
            secret_key: super::password_of(profile)?,
            region,
            addressing: profile.s3.addressing,
            home: bucket
                .as_deref()
                .map_or_else(|| "/".to_string(), |bucket| format!("/{bucket}")),
            bucket_regions: Mutex::new(HashMap::new()),
            folders: FolderCache::default(),
            folder_creation: tokio::sync::Mutex::new(()),
        };
        // Checks the keys, and that the bucket to start in exists.
        match &bucket {
            Some(bucket) => {
                if !files.bucket_exists(bucket).await? {
                    return Err(no_such_bucket(bucket));
                }
            }
            None => {
                files.list_buckets().await?;
            }
        }
        let fingerprint = files
            .tls
            .as_ref()
            .and_then(ServerTls::accepted_fingerprint)
            .unwrap_or_default();
        Ok((files, fingerprint))
    }

    fn region_of(&self, bucket: Option<&str>) -> String {
        bucket
            .and_then(|bucket| self.bucket_regions.lock().unwrap().get(bucket).cloned())
            .unwrap_or_else(|| self.region.clone())
    }

    /// The address of a request, signed for `region`.
    fn build(&self, request: &S3Request, region: &str) -> AppResult<Request> {
        let aws = self.endpoint.aws_region();
        let base_host = match (&aws, &request.bucket) {
            // Amazon serves each bucket from its region's endpoint.
            (Some(_), Some(_)) => format!("s3.{region}.amazonaws.com"),
            _ => self.endpoint.host.clone(),
        };
        let virtual_hosted = match (&request.bucket, self.addressing) {
            (None, _) | (_, Addressing::Path) => false,
            (Some(_), Addressing::VirtualHosted) => true,
            (Some(bucket), Addressing::Automatic) => aws.is_some() && fits_in_host_name(bucket),
        };
        let encoded_key = super::encode_path(&request.key);
        let (host, path) = match &request.bucket {
            Some(bucket) if virtual_hosted => (format!("{bucket}.{base_host}"), encoded_key),
            Some(bucket) => (base_host, format!("/{}{encoded_key}", escape(bucket))),
            None => (base_host, String::new()),
        };
        let mut path = if path.is_empty() {
            "/".to_string()
        } else {
            path
        };
        // A folder marker keeps its trailing slash.
        if request.key.ends_with('/') && !path.ends_with('/') {
            path.push('/');
        }
        let query = request.canonical_query();
        let authority = self.endpoint.authority(&host);
        let scheme = if self.endpoint.secure {
            "https"
        } else {
            "http"
        };
        let address = match query.as_str() {
            "" => format!("{scheme}://{authority}{path}"),
            query => format!("{scheme}://{authority}{path}?{query}"),
        };
        let url = Url::parse(&address)
            .map_err(|error| AppError::invalid(format!("Invalid S3 address: {error}")))?;
        let date_time = timestamp::format_rfc3339(timestamp::now_seconds()).replace(['-', ':'], "");
        let mut headers: Vec<(String, String)> = vec![
            ("host".to_string(), authority),
            (
                "x-amz-content-sha256".to_string(),
                request.payload_hash.clone(),
            ),
            ("x-amz-date".to_string(), date_time.clone()),
        ];
        headers.extend(request.headers.iter().cloned());
        headers.sort();
        let authorization = signing::authorization(
            &self.access_key,
            &self.secret_key,
            region,
            &date_time,
            &signing::Signable {
                method: request.method.as_str(),
                path: &path,
                query: &query,
                headers: &headers,
                payload_hash: &request.payload_hash,
            },
        );
        let mut builder = self.http.request(request.method.clone(), url);
        for (name, value) in headers.iter().filter(|(name, _)| name != "host") {
            builder = builder.header(name, value);
        }
        builder
            .header(reqwest::header::AUTHORIZATION, authorization)
            .header(CONTENT_LENGTH, request.body.len())
            .body(request.body.clone())
            .build()
            .map_err(api::transport_error)
    }

    /// Sends a request, retrying what is safe to retry, and once more in the bucket's own
    /// region when the service says it is elsewhere.
    async fn send(&self, request: &S3Request) -> AppResult<Response> {
        let mut region = self.region_of(request.bucket.as_deref());
        let mut moved = false;
        loop {
            let build = || self.build(request, &region);
            let response = api::send_retrying(&self.http, &build)
                .await
                .map_err(|error| super::certificate_rejection(error, self.tls.as_ref()))?;
            let status = response.status();
            let wrong_region = matches!(
                status,
                StatusCode::MOVED_PERMANENTLY
                    | StatusCode::TEMPORARY_REDIRECT
                    | StatusCode::BAD_REQUEST
            );
            if moved || !wrong_region {
                return Ok(response);
            }
            let named = header_text(response.headers(), BUCKET_REGION_HEADER);
            let actual = match named {
                Some(actual) => actual,
                None => {
                    let body = api::body_bytes(response).await?;
                    let error = xml::embedded_error(&body).unwrap_or_default();
                    if error.region.is_empty() || error.region == region {
                        return Err(service_error(status, error, &request.key));
                    }
                    error.region
                }
            };
            if actual == region {
                return Err(AppError::new(
                    ErrorKind::Cloud,
                    format!("S3 sent the request elsewhere ({status})"),
                ));
            }
            if let Some(bucket) = &request.bucket {
                self.bucket_regions
                    .lock()
                    .unwrap()
                    .insert(bucket.clone(), actual.clone());
            }
            region = actual;
            moved = true;
        }
    }

    /// Sends a request and reads the whole answer, which must be a success.
    async fn send_for_body(&self, request: &S3Request, path: &str) -> AppResult<Vec<u8>> {
        let response = self.send(request).await?;
        if !response.status().is_success() {
            return Err(failure(response, path).await);
        }
        let body = api::body_bytes(response).await?;
        match xml::embedded_error(&body) {
            Some(error) => Err(service_error(StatusCode::OK, error, path)),
            None => Ok(body),
        }
    }

    async fn list_buckets(&self) -> AppResult<Vec<xml::Bucket>> {
        let request = S3Request::new(Method::GET, None, "");
        let response = self.send(&request).await?;
        if response.status() == StatusCode::FORBIDDEN {
            return Err(AppError::new(
                ErrorKind::PermissionDenied,
                "These keys cannot list buckets. Enter a bucket as the remote folder, such as /my-bucket",
            ));
        }
        if !response.status().is_success() {
            return Err(failure(response, "/").await);
        }
        let list: xml::ListAllMyBucketsResult = xml::parse(&api::body_bytes(response).await?)?;
        Ok(list.buckets.buckets)
    }

    async fn bucket_exists(&self, bucket: &str) -> AppResult<bool> {
        let request = S3Request::new(Method::HEAD, Some(bucket), "");
        let response = self.send(&request).await?;
        match response.status() {
            status if status.is_success() => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            _ => Err(failure(response, &format!("/{bucket}")).await),
        }
    }

    /// One page of the objects under `prefix`, grouped into folders unless `recursive`.
    async fn list_page(
        &self,
        bucket: &str,
        prefix: &str,
        recursive: bool,
        token: Option<&str>,
        max_keys: &str,
    ) -> AppResult<xml::ListBucketResult> {
        let mut request = S3Request::new(Method::GET, Some(bucket), "")
            .query("list-type", "2")
            .query("prefix", prefix)
            .query("max-keys", max_keys);
        if !recursive {
            request = request.query("delimiter", "/");
        }
        if let Some(token) = token {
            request = request.query("continuation-token", token);
        }
        let path = format!("/{bucket}/{prefix}");
        let body = self.send_for_body(&request, &path).await?;
        xml::parse(&body)
    }

    /// Every object under `prefix`, page by page.
    async fn list_all(&self, bucket: &str, prefix: &str) -> AppResult<Vec<xml::Object>> {
        let mut objects = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page = self
                .list_page(bucket, prefix, true, token.as_deref(), PAGE_SIZE)
                .await?;
            objects.extend(page.contents);
            match page.next_continuation_token.filter(|_| page.is_truncated) {
                Some(next) => token = Some(next),
                None => return Ok(objects),
            }
        }
    }

    /// Whether anything is stored under `key/`, a folder marker included.
    async fn folder_exists(&self, bucket: &str, key: &str) -> AppResult<bool> {
        let page = self
            .list_page(bucket, &format!("{key}/"), false, None, "1")
            .await?;
        Ok(!page.contents.is_empty() || !page.common_prefixes.is_empty())
    }

    /// The size and time of an object; `None` when there is none.
    async fn head_object(&self, bucket: &str, key: &str) -> AppResult<Option<RemoteStat>> {
        let request = S3Request::new(Method::HEAD, Some(bucket), key);
        let response = self.send(&request).await?;
        match response.status() {
            status if status.is_success() => {
                let headers = response.headers();
                Ok(Some(RemoteStat {
                    is_dir: false,
                    size: header_text(headers, CONTENT_LENGTH.as_str())
                        .and_then(|length| length.parse().ok())
                        .unwrap_or(0),
                    // The file's own time when the upload kept it, as rclone and Poros do.
                    modified: header_text(headers, MTIME_HEADER)
                        .and_then(|seconds| seconds.parse::<f64>().ok())
                        .filter(|seconds| seconds.is_finite())
                        .map(|seconds| seconds.floor() as i64)
                        .or_else(|| {
                            header_text(headers, LAST_MODIFIED.as_str())
                                .and_then(|date| timestamp::parse_http_date(&date))
                        }),
                    permissions: None,
                }))
            }
            StatusCode::NOT_FOUND => Ok(None),
            _ => Err(failure(response, &format!("/{bucket}/{key}")).await),
        }
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
        if key.is_empty() {
            return Ok(self.bucket_exists(bucket).await?.then_some(folder));
        }
        if let Some(object) = self.head_object(bucket, key).await? {
            return Ok(Some(object));
        }
        Ok(self.folder_exists(bucket, key).await?.then_some(folder))
    }

    async fn put_empty(&self, bucket: &str, key: &str) -> AppResult<()> {
        let request = S3Request::new(Method::PUT, Some(bucket), key)
            .header("content-type", "application/x-directory");
        self.send_for_body(&request, &format!("/{bucket}/{key}"))
            .await
            .map(drop)
    }

    async fn create_bucket(&self, bucket: &str) -> AppResult<()> {
        let mut request = S3Request::new(Method::PUT, Some(bucket), "");
        // Without a location the bucket lands in us-east-1; R2 has only `auto`.
        if !matches!(self.region.as_str(), "us-east-1" | "auto") {
            request = request.body(Bytes::from(format!(
                "<CreateBucketConfiguration><LocationConstraint>{}</LocationConstraint></CreateBucketConfiguration>",
                xml::escape(&self.region)
            )));
        }
        self.send_for_body(&request, &format!("/{bucket}"))
            .await
            .map(drop)
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> AppResult<()> {
        let request = S3Request::new(Method::DELETE, Some(bucket), key);
        let response = self.send(&request).await?;
        match response.status() {
            status if status.is_success() || status == StatusCode::NOT_FOUND => Ok(()),
            _ => Err(failure(response, &format!("/{bucket}/{key}")).await),
        }
    }

    /// Deletes the keys, a thousand to a request where the service can.
    async fn delete_keys(&self, bucket: &str, keys: &[String]) -> AppResult<()> {
        for batch in keys.chunks(DELETE_BATCH) {
            let body = Bytes::from(xml::delete_objects(batch));
            let checksum = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                <md5::Md5 as md5::Digest>::digest(&body),
            );
            let request = S3Request::new(Method::POST, Some(bucket), "")
                .query("delete", "")
                .header("content-md5", checksum)
                .header("content-type", "application/xml")
                .body(body);
            let response = self.send(&request).await?;
            if response.status() == StatusCode::NOT_IMPLEMENTED {
                for key in batch {
                    self.delete_object(bucket, key).await?;
                }
                continue;
            }
            if !response.status().is_success() {
                return Err(failure(response, &format!("/{bucket}")).await);
            }
            let result: xml::DeleteResult = xml::parse(&api::body_bytes(response).await?)?;
            if let Some(refused) = result.errors.into_iter().next() {
                let path = format!("/{bucket}/{}", refused.key);
                let error = xml::ErrorBody {
                    code: refused.code,
                    message: refused.message,
                    region: String::new(),
                };
                return Err(service_error(StatusCode::OK, error, &path));
            }
        }
        Ok(())
    }

    /// Copies an object within the service, in parts when it is too large for one request.
    async fn copy_object(
        &self,
        bucket: &str,
        source: &str,
        size: u64,
        target: &str,
    ) -> AppResult<()> {
        let mut copy_source = format!("/{}{}", escape(bucket), super::encode_path(source));
        // A folder marker keeps its trailing slash.
        if source.ends_with('/') {
            copy_source.push('/');
        }
        let target_path = format!("/{bucket}/{target}");
        if size <= COPY_LIMIT {
            let request = S3Request::new(Method::PUT, Some(bucket), target)
                .header("x-amz-copy-source", copy_source);
            return self.send_for_body(&request, &target_path).await.map(drop);
        }
        let upload_id = self.start_multipart(bucket, target, &[]).await?;
        let part_size = COPY_PART.max(size.div_ceil(MAX_PARTS));
        let mut etags = Vec::new();
        let copied: AppResult<()> = async {
            for (index, start) in (0..size).step_by(part_size as usize).enumerate() {
                let end = (start + part_size).min(size) - 1;
                let request = S3Request::new(Method::PUT, Some(bucket), target)
                    .query("partNumber", &(index + 1).to_string())
                    .query("uploadId", &upload_id)
                    .header("x-amz-copy-source", copy_source.clone())
                    .header("x-amz-copy-source-range", format!("bytes={start}-{end}"));
                let result: xml::CopyResult =
                    xml::parse(&self.send_for_body(&request, &target_path).await?)?;
                etags.push(result.etag);
            }
            self.complete_multipart(bucket, target, &upload_id, &etags)
                .await
        }
        .await;
        if copied.is_err() {
            let _ = self.abort_multipart(bucket, target, &upload_id).await;
        }
        copied
    }

    async fn start_multipart(
        &self,
        bucket: &str,
        key: &str,
        headers: &[(String, String)],
    ) -> AppResult<String> {
        let mut request = S3Request::new(Method::POST, Some(bucket), key).query("uploads", "");
        request.headers.extend(headers.iter().cloned());
        let body = self
            .send_for_body(&request, &format!("/{bucket}/{key}"))
            .await?;
        let started: xml::InitiateMultipartUploadResult = xml::parse(&body)?;
        Ok(started.upload_id)
    }

    async fn complete_multipart(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        etags: &[String],
    ) -> AppResult<()> {
        let request = S3Request::new(Method::POST, Some(bucket), key)
            .query("uploadId", upload_id)
            .header("content-type", "application/xml")
            .body(Bytes::from(xml::complete_multipart(etags)));
        self.send_for_body(&request, &format!("/{bucket}/{key}"))
            .await
            .map(drop)
    }

    async fn abort_multipart(&self, bucket: &str, key: &str, upload_id: &str) -> AppResult<()> {
        let request =
            S3Request::new(Method::DELETE, Some(bucket), key).query("uploadId", upload_id);
        self.send(&request).await.map(drop)
    }

    fn bucket_and_key(&self, path: &str) -> (Option<String>, String) {
        split(&self.resolve(path))
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn no_such_bucket(bucket: &str) -> AppError {
    AppError::new(
        ErrorKind::NotFound,
        format!("The bucket {bucket} does not exist"),
    )
    .with_path(format!("/{bucket}"))
}

/// An error for a failed answer, from the service's `<Error>` body when it sent one.
async fn failure(response: Response, path: &str) -> AppError {
    let status = response.status();
    let error = match api::body_bytes(response).await {
        Ok(body) => xml::embedded_error(&body).unwrap_or_default(),
        Err(error) => return error,
    };
    service_error(status, error, path)
}

fn service_error(status: StatusCode, error: xml::ErrorBody, path: &str) -> AppError {
    let message = match error.message.trim() {
        "" if error.code.is_empty() => format!("S3 answered {status}"),
        "" => format!("S3 refused the request ({})", error.code),
        message => message.to_string(),
    };
    let kind = match error.code.as_str() {
        "NoSuchKey" | "NoSuchUpload" => return super::not_found(path),
        "NoSuchBucket" => {
            let (bucket, _) = split(path);
            return no_such_bucket(bucket.as_deref().unwrap_or_default());
        }
        "InvalidAccessKeyId" | "SignatureDoesNotMatch" | "InvalidToken" | "ExpiredToken" => {
            ErrorKind::AuthFailed
        }
        "RequestTimeTooSkewed" => {
            return AppError::new(
                ErrorKind::AuthFailed,
                "This computer's clock is too far off for S3 to accept the request; set it to the right time",
            )
        }
        "AccessDenied" | "AllAccessDisabled" | "AccountProblem" => ErrorKind::PermissionDenied,
        "BucketAlreadyExists" | "BucketAlreadyOwnedByYou" => ErrorKind::AlreadyExists,
        "SlowDown" | "InternalError" | "ServiceUnavailable" | "RequestTimeout" => ErrorKind::Cloud,
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

fn folder_entry(path: &str, name: &str, modified: Option<i64>) -> FileEntry {
    super::entry(path, name, EntryKind::Dir, 0, modified)
}

#[async_trait]
impl RemoteFileSystem for S3Fs {
    fn protocol(&self) -> Protocol {
        Protocol::S3
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
                for bucket in self.list_buckets().await? {
                    let path = format!("/{}", bucket.name);
                    let created = bucket
                        .creation_date
                        .as_deref()
                        .and_then(timestamp::parse_rfc3339);
                    entries.push(folder_entry(&path, &bucket.name, created));
                }
            }
            Some(bucket) => {
                let prefix = match key.as_str() {
                    "" => String::new(),
                    key => format!("{key}/"),
                };
                let mut token: Option<String> = None;
                loop {
                    let page = self
                        .list_page(bucket, &prefix, false, token.as_deref(), PAGE_SIZE)
                        .await?;
                    for folder in &page.common_prefixes {
                        let name = folder.prefix[prefix.len().min(folder.prefix.len())..]
                            .trim_end_matches('/');
                        if name.is_empty() || name.contains('/') {
                            continue;
                        }
                        let path = remote_path::join(&directory, name);
                        self.folders.insert(&path, "");
                        entries.push(folder_entry(&path, name, None));
                    }
                    for object in &page.contents {
                        let Some(name) = object.key.strip_prefix(&prefix) else {
                            continue;
                        };
                        // The folder's own marker, or a name no path can reach.
                        if name.is_empty() || name.contains('/') {
                            continue;
                        }
                        let path = remote_path::join(&directory, name);
                        let modified = object
                            .last_modified
                            .as_deref()
                            .and_then(timestamp::parse_rfc3339);
                        entries.push(super::entry(
                            &path,
                            name,
                            EntryKind::File,
                            object.size,
                            modified,
                        ));
                    }
                    match page.next_continuation_token.filter(|_| page.is_truncated) {
                        Some(next) => token = Some(next),
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

    /// A folder at the top is a new bucket.
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
        self.put_empty(&bucket, &format!("{key}/")).await?;
        self.folders.insert(&path, "");
        Ok(path)
    }

    /// S3 has no rename: objects are copied to the new name, then deleted.
    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        protocol::validate_name(new_name)?;
        let path = protocol::normalize(&self.resolve(path));
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root folder"))?;
        let target = remote_path::join(&parent, new_name);
        let (bucket, key) = split(&path);
        let (Some(bucket), false) = (bucket, key.is_empty()) else {
            return Err(AppError::unsupported("S3 buckets cannot be renamed"));
        };
        let (_, target_key) = split(&target);
        if self.stat_parts(Some(&bucket), &target_key).await?.is_some() {
            return Err(protocol::already_exists(&target));
        }
        if let Some(object) = self.head_object(&bucket, &key).await? {
            self.copy_object(&bucket, &key, object.size, &target_key)
                .await?;
            self.delete_object(&bucket, &key).await?;
            return Ok(target);
        }
        let prefix = format!("{key}/");
        let objects = self.list_all(&bucket, &prefix).await?;
        if objects.is_empty() {
            return Err(super::not_found(&path));
        }
        for object in &objects {
            let moved = format!("{target_key}/{}", &object.key[prefix.len()..]);
            self.copy_object(&bucket, &object.key, object.size, &moved)
                .await?;
        }
        let keys: Vec<String> = objects.into_iter().map(|object| object.key).collect();
        self.delete_keys(&bucket, &keys).await?;
        self.folders.forget(&path);
        Ok(target)
    }

    /// Deletes for good: S3 has no recycle bin, though a versioned bucket keeps old versions.
    /// Deleting a bucket empties it first.
    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        let resolved: Vec<String> = paths.iter().map(|path| self.resolve(path)).collect();
        for path in protocol::outermost(&resolved) {
            let (Some(bucket), key) = split(&path) else {
                return Err(AppError::invalid("Refusing to delete /"));
            };
            let prefix = match key.as_str() {
                "" => String::new(),
                key => format!("{key}/"),
            };
            if !key.is_empty() {
                self.delete_object(&bucket, &key).await?;
            }
            let keys: Vec<String> = self
                .list_all(&bucket, &prefix)
                .await?
                .into_iter()
                .map(|object| object.key)
                .collect();
            self.delete_keys(&bucket, &keys).await?;
            if key.is_empty() {
                let request = S3Request::new(Method::DELETE, Some(&bucket), "");
                self.send_for_body(&request, &path).await?;
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
        if key.is_empty() {
            if !self.bucket_exists(&bucket).await? {
                self.create_bucket(&bucket).await?;
            }
        } else if !self.folder_exists(&bucket, &key).await? {
            if self.head_object(&bucket, &key).await?.is_some() {
                return Err(protocol::not_a_folder(&path));
            }
            self.put_empty(&bucket, &format!("{key}/")).await?;
        }
        self.folders.insert(&path, "");
        Ok(())
    }

    /// An object's time is set when it is uploaded and cannot change without copying it.
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
            if self.head_object(&bucket, &key).await?.is_none() {
                return Err(super::not_found(&path));
            }
            return Ok(Box::new(HttpReader::new(None, 0)));
        }
        let mut request = S3Request::new(Method::GET, Some(&bucket), &key);
        if range.start > 0 || range.end != u64::MAX {
            let last = match range.end {
                u64::MAX => String::new(),
                end => (end - 1).to_string(),
            };
            request = request.header("range", format!("bytes={}-{last}", range.start));
        }
        let response = self.send(&request).await?;
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
            return Err(super::cannot_resume(Protocol::S3));
        }
        let path = protocol::normalize(&request.path);
        let (Some(bucket), key) = split(&path) else {
            return Err(AppError::invalid("Files go inside a bucket"));
        };
        if key.is_empty() {
            return Err(AppError::invalid("Files go inside a bucket"));
        }
        let chunk_size = chunk_size_within(request.size, MAX_PARTS);
        let target = S3Upload {
            files: self,
            bucket,
            key,
            modified: request.modified,
            chunk_size,
            upload_id: None,
            etags: Vec::new(),
        };
        Ok(Box::new(ChunkedUpload::new(target, request.size)))
    }
}

/// Replaces whatever object is at the key: in one request up to a chunk's size, as a
/// multipart upload above it.
struct S3Upload {
    files: Arc<S3Fs>,
    bucket: String,
    key: String,
    modified: Option<i64>,
    chunk_size: usize,
    /// The multipart upload in progress, aborted if the upload does not finish.
    upload_id: Option<String>,
    etags: Vec<String>,
}

impl S3Upload {
    fn path(&self) -> String {
        format!("/{}/{}", self.bucket, self.key)
    }

    fn object_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![(
            "content-type".to_string(),
            "application/octet-stream".to_string(),
        )];
        if let Some(modified) = self.modified {
            headers.push((MTIME_HEADER.to_string(), modified.to_string()));
        }
        headers
    }
}

impl Drop for S3Upload {
    fn drop(&mut self) {
        let Some(upload_id) = self.upload_id.take() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (files, bucket, key) = (self.files.clone(), self.bucket.clone(), self.key.clone());
        // Parts of an unfinished upload are kept, and billed, until it is aborted.
        runtime.spawn(async move {
            let _ = files.abort_multipart(&bucket, &key, &upload_id).await;
        });
    }
}

#[async_trait]
impl UploadTarget for S3Upload {
    fn single_request_limit(&self) -> usize {
        self.chunk_size
    }

    fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    async fn upload_whole(&mut self, data: Bytes) -> AppResult<()> {
        let mut request = S3Request::new(Method::PUT, Some(&self.bucket), &self.key).body(data);
        request.headers = self.object_headers();
        self.files
            .send_for_body(&request, &self.path())
            .await
            .map(drop)
    }

    async fn start_session(&mut self) -> AppResult<()> {
        let upload_id = self
            .files
            .start_multipart(&self.bucket, &self.key, &self.object_headers())
            .await?;
        self.upload_id = Some(upload_id);
        Ok(())
    }

    async fn send_chunk(&mut self, _offset: u64, data: Bytes, last: bool) -> AppResult<()> {
        let upload_id = self
            .upload_id
            .clone()
            .ok_or_else(|| AppError::new(ErrorKind::Cloud, "No upload is open"))?;
        let part_number = (self.etags.len() + 1).to_string();
        let request = S3Request::new(Method::PUT, Some(&self.bucket), &self.key)
            .query("partNumber", &part_number)
            .query("uploadId", &upload_id)
            .body(data);
        let response = self.files.send(&request).await?;
        if !response.status().is_success() {
            return Err(failure(response, &self.path()).await);
        }
        let etag = header_text(response.headers(), ETAG.as_str()).ok_or_else(|| {
            AppError::new(ErrorKind::Cloud, "S3 did not confirm a part of the upload")
        })?;
        self.etags.push(etag);
        if last {
            self.files
                .complete_multipart(&self.bucket, &self.key, &upload_id, &self.etags)
                .await?;
            self.upload_id = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(host: &str, port: u16) -> ConnectProfile {
        serde_json::from_value(serde_json::json!({
            "protocol": "s3",
            "host": host,
            "port": port,
            "username": "AKIAIOSFODNN7EXAMPLE",
            "auth": { "type": "password", "password": "secret" },
            "s3": { "region": "", "addressing": "automatic" },
        }))
        .unwrap()
    }

    fn files(host: &str, port: u16, addressing: Addressing) -> S3Fs {
        let endpoint = Endpoint::parse(&profile(host, port)).unwrap();
        S3Fs {
            http: api::http_client().unwrap(),
            tls: None,
            region: endpoint
                .aws_region()
                .flatten()
                .unwrap_or_else(|| DEFAULT_REGION.into()),
            endpoint,
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "secret".into(),
            addressing,
            home: "/".into(),
            bucket_regions: Mutex::new(HashMap::new()),
            folders: FolderCache::default(),
            folder_creation: tokio::sync::Mutex::new(()),
        }
    }

    #[test]
    fn reads_endpoints() {
        let aws = Endpoint::parse(&profile("", 443)).unwrap();
        assert_eq!(aws.host, AWS_ENDPOINT);
        assert_eq!(aws.aws_region(), Some(None));
        let regional = Endpoint::parse(&profile("s3.eu-west-1.amazonaws.com", 443)).unwrap();
        assert_eq!(regional.aws_region(), Some(Some("eu-west-1".into())));
        let minio = Endpoint::parse(&profile("http://localhost:9000", 443)).unwrap();
        assert_eq!(
            minio,
            Endpoint {
                secure: false,
                host: "localhost".into(),
                port: 9000
            }
        );
        assert_eq!(minio.aws_region(), None);
        let plain = Endpoint::parse(&profile("http://minio.lan", 443)).unwrap();
        assert_eq!(plain.port, 80);
        let wasabi =
            Endpoint::parse(&profile("https://s3.eu-central-1.wasabisys.com/", 443)).unwrap();
        assert_eq!(wasabi.host, "s3.eu-central-1.wasabisys.com");
        assert_eq!(
            endpoint_label(&profile("http://localhost:9000", 443)),
            "localhost:9000"
        );
    }

    #[test]
    fn splits_paths_into_buckets_and_keys() {
        assert_eq!(split("/"), (None, String::new()));
        assert_eq!(split("/photos"), (Some("photos".into()), String::new()));
        assert_eq!(
            split("/photos/2024/a b.jpg"),
            (Some("photos".into()), "2024/a b.jpg".into())
        );
    }

    #[test]
    fn addresses_buckets_by_host_or_path() {
        let aws = files("", 443, Addressing::Automatic);
        let request = S3Request::new(Method::GET, Some("my-bucket"), "a b/c+d.txt");
        let built = aws.build(&request, "eu-west-1").unwrap();
        assert_eq!(
            built.url().as_str(),
            "https://my-bucket.s3.eu-west-1.amazonaws.com/a%20b/c%2Bd.txt"
        );
        // Dots would fall outside the certificate's wildcard.
        let dotted = S3Request::new(Method::GET, Some("my.bucket"), "k");
        assert_eq!(
            aws.build(&dotted, "us-east-1").unwrap().url().as_str(),
            "https://s3.us-east-1.amazonaws.com/my.bucket/k"
        );
        let minio = files("http://localhost:9000", 443, Addressing::Automatic);
        let listing = S3Request::new(Method::GET, Some("data"), "")
            .query("prefix", "a b/")
            .query("list-type", "2");
        let built = minio.build(&listing, "us-east-1").unwrap();
        assert_eq!(
            built.url().as_str(),
            "http://localhost:9000/data?list-type=2&prefix=a%20b%2F"
        );
        let authorization = built.headers()["authorization"].to_str().unwrap();
        assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/"));
        assert!(authorization.contains("/us-east-1/s3/aws4_request"));
        assert!(authorization.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        let marker = S3Request::new(Method::PUT, Some("data"), "new folder/");
        assert_eq!(
            minio.build(&marker, "us-east-1").unwrap().url().as_str(),
            "http://localhost:9000/data/new%20folder/"
        );
        let buckets = S3Request::new(Method::GET, None, "");
        assert_eq!(
            files("", 443, Addressing::VirtualHosted)
                .build(&buckets, "us-east-1")
                .unwrap()
                .url()
                .as_str(),
            "https://s3.amazonaws.com/"
        );
    }

    #[test]
    fn maps_service_errors() {
        let error = |code: &str| xml::ErrorBody {
            code: code.into(),
            message: "text".into(),
            region: String::new(),
        };
        assert_eq!(
            service_error(StatusCode::NOT_FOUND, error("NoSuchKey"), "/b/k").kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            service_error(StatusCode::FORBIDDEN, error("SignatureDoesNotMatch"), "/b").kind,
            ErrorKind::AuthFailed
        );
        assert_eq!(
            service_error(StatusCode::FORBIDDEN, error("AccessDenied"), "/b").kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            service_error(StatusCode::SERVICE_UNAVAILABLE, error("SlowDown"), "/b").kind,
            ErrorKind::Cloud
        );
        let missing = service_error(StatusCode::NOT_FOUND, error("NoSuchBucket"), "/gone/k");
        assert_eq!(missing.message, "The bucket gone does not exist");
        assert!(fits_in_host_name("my-bucket-1"));
        assert!(!fits_in_host_name("My_Bucket"));
        assert!(!fits_in_host_name("a.b.c"));
    }
}
