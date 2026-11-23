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
use reqwest::header::{HeaderMap, CONTENT_LENGTH, DATE, ETAG, LAST_MODIFIED};
use reqwest::{Client, Method, Request, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};

use super::api;
use super::upload::{chunk_size_within, ChunkedUpload, UploadTarget};
use super::{FolderCache, HttpReader, Login};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::Events;
use crate::model::{DirListing, EntryKind, FileEntry};
use crate::protocol::{
    self, Capabilities, DeepPage, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;
use crate::share::{
    self, Audience, Expiry, LinkKind, LinkSharing, ShareCapabilities, ShareLink, ShareOptions,
};
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
/// How far this computer's clock may be from the service's before it refuses a signature.
const MAX_CLOCK_SKEW_SECS: u64 = 15 * 60;
/// A difference between the clocks that changes when a link ends enough to say so.
const NOTABLE_CLOCK_SKEW_SECS: u64 = 30;

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

/// The time of a request as signatures have it: `20130524T000000Z`.
fn signing_time(seconds: i64) -> String {
    timestamp::format_rfc3339(seconds).replace(['-', ':'], "")
}

/// Where a request goes.
struct Address {
    scheme: &'static str,
    /// The `host` header: the name, with the port when it is not the scheme's own.
    authority: String,
    /// Escaped, starting with a slash.
    path: String,
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

    /// Where `key` in `bucket` is reached in `region`: under the bucket's own host name or
    /// under a path, as the connection is set up.
    fn address(&self, bucket: Option<&str>, key: &str, region: &str) -> Address {
        let aws = self.endpoint.aws_region();
        let base_host = match (&aws, bucket) {
            // Amazon serves each bucket from its region's endpoint.
            (Some(_), Some(_)) => format!("s3.{region}.amazonaws.com"),
            _ => self.endpoint.host.clone(),
        };
        let virtual_hosted = match (bucket, self.addressing) {
            (None, _) | (_, Addressing::Path) => false,
            (Some(_), Addressing::VirtualHosted) => true,
            (Some(bucket), Addressing::Automatic) => aws.is_some() && fits_in_host_name(bucket),
        };
        let encoded_key = super::encode_path(key);
        let (host, path) = match bucket {
            Some(bucket) if virtual_hosted => (format!("{bucket}.{base_host}"), encoded_key),
            Some(bucket) => (
                base_host,
                format!("/{}{encoded_key}", signing::escape(bucket)),
            ),
            None => (base_host, String::new()),
        };
        let mut path = if path.is_empty() {
            "/".to_string()
        } else {
            path
        };
        // A folder marker keeps its trailing slash.
        if key.ends_with('/') && !path.ends_with('/') {
            path.push('/');
        }
        Address {
            scheme: if self.endpoint.secure {
                "https"
            } else {
                "http"
            },
            authority: self.endpoint.authority(&host),
            path,
        }
    }

    /// The address of a request, signed for `region`.
    fn build(&self, request: &S3Request, region: &str) -> AppResult<Request> {
        let Address {
            scheme,
            authority,
            path,
        } = self.address(request.bucket.as_deref(), &request.key, region);
        let query = signing::canonical_query(&request.query);
        let address = match query.as_str() {
            "" => format!("{scheme}://{authority}{path}"),
            query => format!("{scheme}://{authority}{path}?{query}"),
        };
        let url = Url::parse(&address)
            .map_err(|error| AppError::invalid(format!("Invalid S3 address: {error}")))?;
        let date_time = signing_time(timestamp::now_seconds());
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
        let mut copy_source = format!("/{}{}", signing::escape(bucket), super::encode_path(source));
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

    /// An address that reads the object without credentials, from `signed_at` until
    /// `lifetime_secs` later. With `download_as`, browsers save it under that name.
    fn presigned_address(
        &self,
        bucket: &str,
        key: &str,
        region: &str,
        signed_at: i64,
        lifetime_secs: u64,
        download_as: Option<&str>,
    ) -> String {
        let Address {
            scheme,
            authority,
            path,
        } = self.address(Some(bucket), key, region);
        let parameters: Vec<(String, String)> = download_as
            .map(|name| ("response-content-disposition".to_string(), attachment(name)))
            .into_iter()
            .collect();
        let query = signing::presigned_query(
            &signing::Presigning {
                access_key: &self.access_key,
                secret_key: &self.secret_key,
                // Connections hold long-term keys, which come without a session token.
                session_token: None,
                region,
                date_time: &signing_time(signed_at),
                expires_secs: lifetime_secs,
            },
            Method::GET.as_str(),
            &authority,
            &path,
            &parameters,
        );
        format!("{scheme}://{authority}{path}?{query}")
    }
}

/// A `Content-Disposition` that has browsers save the file under `name`. A name beyond
/// plain ASCII goes in the extended form too (RFC 6266), beside a stand-in for clients
/// that do not read it.
fn attachment(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|character| match character {
            '"' | '\\' => '_',
            character if character.is_ascii() && !character.is_ascii_control() => character,
            _ => '_',
        })
        .collect();
    if plain == name {
        format!("attachment; filename=\"{plain}\"")
    } else {
        format!(
            "attachment; filename=\"{plain}\"; filename*=UTF-8''{}",
            signing::escape(name)
        )
    }
}

fn span_text(seconds: u64) -> String {
    match seconds {
        0..=119 => format!("{seconds} seconds"),
        120..=7199 => format!("{} minutes", seconds / 60),
        _ => format!("{} hours", seconds / 3600),
    }
}

/// What a gap between this computer's clock and the service's does to an address signed
/// now. The service counts the lifetime from the time written into the address, so a clock
/// that is behind shortens the link and one that is ahead lengthens it; too far apart and
/// the service refuses the signature outright.
fn clock_note(
    local_time: i64,
    server_time: Option<i64>,
    lifetime_secs: u64,
) -> AppResult<Option<String>> {
    let Some(server_time) = server_time else {
        return Ok(None);
    };
    let behind = local_time < server_time;
    let apart = local_time.abs_diff(server_time);
    let direction = if behind { "behind" } else { "ahead of" };
    // A gap of a few seconds is the answer's travel time and the header's whole seconds.
    let expired_at_once = behind && apart >= NOTABLE_CLOCK_SKEW_SECS && apart >= lifetime_secs;
    if apart >= MAX_CLOCK_SKEW_SECS || expired_at_once {
        return Err(AppError::new(
            ErrorKind::AuthFailed,
            format!(
                "This computer's clock is {} {direction} the server's, so a signed link would not work. Set the clock to the right time and try again.",
                span_text(apart)
            ),
        ));
    }
    if apart < NOTABLE_CLOCK_SKEW_SECS {
        return Ok(None);
    }
    let effect = if behind {
        "stops working that much sooner"
    } else {
        "works that much longer"
    };
    Ok(Some(format!(
        "This computer's clock is {} {direction} the server's, so the link {effect} than chosen.",
        span_text(apart)
    )))
}

fn files_only(path: &str) -> AppError {
    AppError::unsupported(
        "S3 signs links for single files, not for folders or buckets. Select the files inside instead.",
    )
    .with_path(path)
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

/// An object in a listing of everything below `prefix`: a file, or the marker of a folder
/// somebody made by hand, whose key ends in a slash.
fn deep_entry(bucket: &str, prefix: &str, object: &xml::Object) -> Option<FileEntry> {
    let below = object.key.strip_prefix(prefix)?;
    let (below, kind) = match below.strip_suffix('/') {
        Some(folder) => (folder, EntryKind::Dir),
        None => (below, EntryKind::File),
    };
    // The folder's own marker, or a name no path can reach.
    if below.split('/').any(str::is_empty) {
        return None;
    }
    let name = remote_path::file_name(below);
    let path = format!("/{bucket}/{prefix}{below}");
    Some(match kind {
        EntryKind::Dir => folder_entry(&path, name, None),
        _ => super::entry(
            &path,
            name,
            EntryKind::File,
            object.size,
            object
                .last_modified
                .as_deref()
                .and_then(timestamp::parse_rfc3339),
        ),
    })
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

    fn link_sharing(&self) -> Option<&dyn LinkSharing> {
        Some(self)
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

    async fn list_deep(&self, path: &str, cursor: Option<&str>) -> AppResult<Option<DeepPage>> {
        let directory = protocol::normalize(&self.resolve(path));
        let (Some(bucket), key) = split(&directory) else {
            return Ok(None);
        };
        let prefix = match key.as_str() {
            "" => String::new(),
            key => format!("{key}/"),
        };
        let page = self
            .list_page(&bucket, &prefix, true, cursor, PAGE_SIZE)
            .await?;
        let entries = page
            .contents
            .iter()
            .filter_map(|object| deep_entry(&bucket, &prefix, object))
            .collect();
        Ok(Some(DeepPage {
            folder: directory,
            entries,
            next: page.next_continuation_token.filter(|_| page.is_truncated),
        }))
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

    /// Writes the empty object with `If-None-Match: *`, which Amazon and most compatible
    /// services refuse when the key is taken, so an object that appeared since the check is
    /// never replaced. A service without conditional writes gets a plain write after the check.
    async fn make_file(self: Arc<Self>, parent: &str, name: &str) -> AppResult<String> {
        protocol::validate_name(name)?;
        let parent = protocol::normalize(&self.resolve(parent));
        let path = remote_path::join(&parent, name);
        let (Some(bucket), key) = split(&path) else {
            return Err(AppError::invalid("Files go inside a bucket"));
        };
        if key.is_empty() {
            return Err(AppError::invalid("Files go inside a bucket"));
        }
        if self.stat_parts(Some(&bucket), &key).await?.is_some() {
            return Err(protocol::already_exists(&path));
        }
        let request = S3Request::new(Method::PUT, Some(&bucket), &key)
            .header("content-type", "application/octet-stream");
        let conditional = S3Request::new(Method::PUT, Some(&bucket), &key)
            .header("content-type", "application/octet-stream")
            .header("if-none-match", "*");
        let response = self.send(&conditional).await?;
        match response.status() {
            status if status.is_success() => Ok(path),
            StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => {
                Err(protocol::already_exists(&path))
            }
            StatusCode::NOT_IMPLEMENTED | StatusCode::BAD_REQUEST => {
                drop(response);
                self.send_for_body(&request, &path).await?;
                Ok(path)
            }
            _ => Err(failure(response, &path).await),
        }
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

#[async_trait]
impl LinkSharing for S3Fs {
    fn share_capabilities(&self) -> ShareCapabilities {
        ShareCapabilities {
            kinds: vec![LinkKind::View, LinkKind::Download],
            audiences: vec![Audience::Anyone],
            expiry: Expiry::Required,
            max_lifetime_secs: Some(share::MAX_SIGNED_LIFETIME_SECS),
            password: false,
            revocable: false,
            folders: false,
            single_link: false,
            organization: None,
        }
    }

    /// Signs an address that reads the object. Nothing is kept on the service, so the link
    /// cannot be taken back: it works until it expires or the keys that signed it are
    /// removed.
    async fn create_share_link(&self, path: &str, options: &ShareOptions) -> AppResult<ShareLink> {
        let path = protocol::normalize(&self.resolve(path));
        let (Some(bucket), key) = split(&path) else {
            return Err(files_only(&path));
        };
        if key.is_empty() {
            return Err(files_only(&path));
        }
        let lifetime_secs = options
            .lifetime_secs
            .unwrap_or(share::DEFAULT_LIFETIME_SECS)
            .clamp(1, share::MAX_SIGNED_LIFETIME_SECS);
        // Asking for the object finds the bucket's region, shows that these keys may read
        // it, and brings the service's clock.
        let response = self
            .send(&S3Request::new(Method::HEAD, Some(&bucket), &key))
            .await?;
        let signed_at = timestamp::now_seconds();
        let server_time = header_text(response.headers(), DATE.as_str())
            .and_then(|date| timestamp::parse_http_date(&date));
        // An answer to `HEAD` has no body to name the reason, so a refusal over the clock
        // is told from one over permissions by the clocks themselves.
        let note = clock_note(signed_at, server_time, lifetime_secs)?;
        match response.status() {
            status if status.is_success() => {}
            StatusCode::NOT_FOUND => {
                return Err(match self.folder_exists(&bucket, &key).await? {
                    true => files_only(&path),
                    false => super::not_found(&path),
                })
            }
            StatusCode::FORBIDDEN => {
                return Err(AppError::new(
                    ErrorKind::PermissionDenied,
                    "These keys may not read this file, so a link signed with them would not open it. Use keys with read access (s3:GetObject) to the bucket.",
                )
                .with_path(path))
            }
            _ => return Err(failure(response, &path).await),
        }
        let download_as =
            (options.kind == LinkKind::Download).then(|| remote_path::file_name(&path));
        let url = self.presigned_address(
            &bucket,
            &key,
            &self.region_of(Some(&bucket)),
            signed_at,
            lifetime_secs,
            download_as,
        );
        Ok(ShareLink {
            id: String::new(),
            url,
            kind: options.kind,
            audience: Audience::Anyone,
            expires_at: Some(signed_at.saturating_add_unsigned(lifetime_secs)),
            has_password: false,
            revocable: false,
            notes: note.into_iter().collect(),
        })
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

    #[test]
    fn a_listing_of_everything_names_files_and_folders_made_by_hand() {
        let object = |key: &str| xml::Object {
            key: key.to_string(),
            last_modified: Some("2024-03-01T10:00:00.000Z".to_string()),
            size: 42,
        };
        let entry = |key: &str| deep_entry("media", "photos/", &object(key));

        let file = entry("photos/2024/cats/tom.jpg").unwrap();
        assert_eq!(file.path, "/media/photos/2024/cats/tom.jpg");
        assert_eq!(file.name, "tom.jpg");
        assert_eq!(file.kind, EntryKind::File);
        assert_eq!(file.size, 42);
        assert_eq!(
            file.modified,
            timestamp::parse_rfc3339("2024-03-01T10:00:00.000Z")
        );

        let folder = entry("photos/2024/empty/").unwrap();
        assert_eq!(folder.path, "/media/photos/2024/empty");
        assert_eq!(folder.name, "empty");
        assert_eq!(folder.kind, EntryKind::Dir);
        assert_eq!(folder.size, 0);

        assert!(entry("photos/").is_none(), "the folder's own marker");
        assert!(entry("photos//nameless").is_none());
        assert!(entry("photos/a//b").is_none());
        assert!(entry("elsewhere/x").is_none());
        let at_top = deep_entry("media", "", &object(".hidden")).unwrap();
        assert_eq!(at_top.path, "/media/.hidden");
        assert!(at_top.hidden);
    }

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

    /// 24 May 2013, midnight UTC: the time of the examples in the S3 API reference.
    const EXAMPLE_TIME: i64 = 1_369_353_600;

    fn example_keys(files: S3Fs) -> S3Fs {
        S3Fs {
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            ..files
        }
    }

    /// The signatures are the ones botocore, Amazon's own client library, makes for the
    /// same keys, time and address.
    #[test]
    fn presigns_path_style_addresses_on_a_custom_endpoint() {
        let minio = example_keys(files("http://127.0.0.1:9000", 443, Addressing::Path));
        assert_eq!(
            minio.presigned_address("data", "a b/c+d.txt", "us-east-1", EXAMPLE_TIME, 3600, None),
            "http://127.0.0.1:9000/data/a%20b/c%2Bd.txt\
             ?X-Amz-Algorithm=AWS4-HMAC-SHA256\
             &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request\
             &X-Amz-Date=20130524T000000Z\
             &X-Amz-Expires=3600\
             &X-Amz-SignedHeaders=host\
             &X-Amz-Signature=4a1c7ec6f82db0d55a622e9bc01e413dfd6fe8991a9ba11d289eefea88ee3cd7"
        );
        let download = minio.presigned_address(
            "data",
            "a b/c+d.txt",
            "us-east-1",
            EXAMPLE_TIME,
            60,
            Some("c+d.txt"),
        );
        assert!(download
            .contains("&response-content-disposition=attachment%3B%20filename%3D%22c%2Bd.txt%22&"));
        assert!(download.ends_with(
            "&X-Amz-Signature=5c450c7af230f55979c29afaac5a037c39c1b3bb44959a127c1e72d7a6ee7dda"
        ));
    }

    #[test]
    fn presigns_keys_with_unicode_and_reserved_characters() {
        let server = example_keys(files(
            "https://minio.example.com:9443",
            443,
            Addressing::Path,
        ));
        assert_eq!(
            server.presigned_address(
                "data",
                "reports/2024/\u{dc}bersicht (final) #1 & more=yes?.pdf",
                "eu-central-1",
                EXAMPLE_TIME,
                604_800,
                None,
            ),
            "https://minio.example.com:9443/data/reports/2024/%C3%9Cbersicht%20%28final%29%20%231%20%26%20more%3Dyes%3F.pdf\
             ?X-Amz-Algorithm=AWS4-HMAC-SHA256\
             &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Feu-central-1%2Fs3%2Faws4_request\
             &X-Amz-Date=20130524T000000Z\
             &X-Amz-Expires=604800\
             &X-Amz-SignedHeaders=host\
             &X-Amz-Signature=2486537d815d09a3c32b40255ede63d8260f08bcfc06485e3e958f3586a7da75"
        );
    }

    #[test]
    fn presigns_virtual_hosted_addresses() {
        let wasabi = example_keys(files(
            "https://s3.eu-central-1.wasabisys.com",
            443,
            Addressing::VirtualHosted,
        ));
        assert_eq!(
            wasabi.presigned_address(
                "my-bucket",
                "tilde~star*quote'(paren)!.txt",
                "eu-central-1",
                EXAMPLE_TIME,
                1,
                None,
            ),
            "https://my-bucket.s3.eu-central-1.wasabisys.com/tilde~star%2Aquote%27%28paren%29%21.txt\
             ?X-Amz-Algorithm=AWS4-HMAC-SHA256\
             &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Feu-central-1%2Fs3%2Faws4_request\
             &X-Amz-Date=20130524T000000Z\
             &X-Amz-Expires=1\
             &X-Amz-SignedHeaders=host\
             &X-Amz-Signature=e63566a601b0dc38f43ac702d294b2eca29f7629cec17a046b215d5ac8e18378"
        );
        // On Amazon the bucket's host name carries its region, which the signature covers.
        let aws = example_keys(files("", 443, Addressing::Automatic));
        let regional = aws.presigned_address(
            "my-bucket",
            "folder/file name.txt",
            "eu-west-1",
            EXAMPLE_TIME,
            900,
            None,
        );
        assert!(regional
            .starts_with("https://my-bucket.s3.eu-west-1.amazonaws.com/folder/file%20name.txt?"));
        assert!(regional.contains("%2F20130524%2Feu-west-1%2Fs3%2Faws4_request&"));
        let dotted = aws.presigned_address("my.bucket", "k", "us-east-1", EXAMPLE_TIME, 900, None);
        assert!(dotted.starts_with("https://s3.us-east-1.amazonaws.com/my.bucket/k?"));
    }

    #[test]
    fn names_the_download_in_a_content_disposition() {
        assert_eq!(
            attachment("report.pdf"),
            "attachment; filename=\"report.pdf\""
        );
        assert_eq!(
            attachment("a \"quoted\" \\ name.txt"),
            "attachment; filename=\"a _quoted_ _ name.txt\"; filename*=UTF-8''a%20%22quoted%22%20%5C%20name.txt"
        );
        assert_eq!(
            attachment("\u{dc}bersicht\r\n.pdf"),
            "attachment; filename=\"_bersicht__.pdf\"; filename*=UTF-8''%C3%9Cbersicht%0D%0A.pdf"
        );
    }

    #[test]
    fn weighs_this_computer_s_clock_against_the_server_s() {
        let now = EXAMPLE_TIME;
        assert_eq!(clock_note(now, None, 3600).unwrap(), None);
        assert_eq!(clock_note(now, Some(now + 5), 3600).unwrap(), None);
        assert_eq!(
            clock_note(now, Some(now + 180), 3600).unwrap().unwrap(),
            "This computer's clock is 3 minutes behind the server's, so the link stops working that much sooner than chosen."
        );
        assert_eq!(
            clock_note(now + 45, Some(now), 3600).unwrap().unwrap(),
            "This computer's clock is 45 seconds ahead of the server's, so the link works that much longer than chosen."
        );
        // The link would have expired before anyone could open it.
        let expired = clock_note(now, Some(now + 90), 60).unwrap_err();
        assert_eq!(expired.kind, ErrorKind::AuthFailed);
        assert!(expired.message.contains("90 seconds behind"));
        assert!(expired.message.contains("Set the clock"));
        let refused = clock_note(now + 3 * 3600, Some(now), 86_400).unwrap_err();
        assert!(refused.message.contains("3 hours ahead of"));
        // A second or two is how long the answer took, not a wrong clock.
        assert_eq!(clock_note(now, Some(now + 2), 1).unwrap(), None);
    }

    #[test]
    fn offers_signed_links_for_files_only() {
        let capabilities = files("", 443, Addressing::Automatic).share_capabilities();
        assert_eq!(capabilities.kinds, [LinkKind::View, LinkKind::Download]);
        assert_eq!(capabilities.audiences, [Audience::Anyone]);
        assert_eq!(capabilities.expiry, Expiry::Required);
        assert_eq!(capabilities.max_lifetime_secs, Some(604_800));
        assert!(!capabilities.revocable && !capabilities.folders && !capabilities.password);
    }
}
