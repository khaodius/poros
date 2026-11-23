//! Object versions on S3: a bucket's versioning status, the history `ListObjectVersions`
//! gives, and reading, restoring and removing single versions. Services that have no
//! versioning answer these requests in assorted ways, all of which end in a bucket that keeps
//! no versions.

use std::collections::HashMap;
use std::ops::Range;

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::{Method, StatusCode};

use super::xml::{self, VersionMarker, VersionPage};
use super::{failure, service_error, split, S3Fs, S3Request, DELETE_BATCH, PAGE_SIZE};
use crate::cloud::api;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::FileEntry;
use crate::protocol::{self, ReadStream, RemoteFileSystem, RemoteStat};
use crate::versions::{
    self, DeletedEntry, HiddenFile, HiddenFiles, ObjectVersion, ObjectVersions, VersionRef,
    Versioning,
};

/// What `GetBucketVersioning` answered. `None` is a failure to report as it is. A service
/// without versioning says so with an error, an empty body, or a document without a status.
fn versioning_of(status: StatusCode, error_code: &str, body: &[u8]) -> Option<Versioning> {
    if status.is_success() {
        let configuration: xml::VersioningConfiguration = xml::parse(body).unwrap_or_default();
        return Some(match configuration.status.as_deref().map(str::trim) {
            Some(state) if state.eq_ignore_ascii_case("Enabled") => Versioning::Enabled,
            Some(state) if state.eq_ignore_ascii_case("Suspended") => Versioning::Suspended,
            _ => Versioning::Off,
        });
    }
    if error_code == "NoSuchBucket" {
        return None;
    }
    match status {
        StatusCode::FORBIDDEN => Some(Versioning::Unknown),
        _ if not_implemented(status, error_code) => Some(Versioning::Off),
        _ => None,
    }
}

/// Whether a refusal means the service does not know the request at all.
fn not_implemented(status: StatusCode, error_code: &str) -> bool {
    match error_code {
        "NotImplemented" | "MethodNotAllowed" => true,
        "NoSuchBucket" | "NoSuchKey" => false,
        _ => matches!(
            status,
            StatusCode::NOT_IMPLEMENTED | StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_FOUND
        ),
    }
}

/// Says in plain words that a version cannot be removed without the owner's device.
fn mfa_refusal(bucket: &str, path: &str) -> AppError {
    AppError::new(
        ErrorKind::PermissionDenied,
        format!(
            "The bucket {bucket} has MFA delete turned on, so versions and delete markers can \
             only be removed with a code from its owner's authentication device. Poros cannot \
             send one; remove them with the AWS command line and its --mfa option."
        ),
    )
    .with_path(path)
}

/// A path's bucket and key, with the trailing slash of a folder's own object kept.
fn split_keeping_slash(path: &str) -> (Option<String>, String) {
    let (bucket, mut key) = split(path);
    if path.ends_with('/') && !key.is_empty() {
        key.push('/');
    }
    (bucket, key)
}

fn prefix_of(key: &str) -> String {
    match key {
        "" => String::new(),
        key => format!("{key}/"),
    }
}

impl S3Fs {
    /// The bucket and key of a file, which a version belongs to.
    fn object_at(&self, path: &str) -> AppResult<(String, String, String)> {
        let path = protocol::normalize(&self.resolve(path));
        match split(&path) {
            (Some(bucket), key) if !key.is_empty() => Ok((bucket, key, path)),
            _ => Err(AppError::invalid("Versions belong to files inside a bucket").with_path(path)),
        }
    }

    async fn bucket_versioning(
        &self,
        bucket: &str,
    ) -> AppResult<Option<xml::VersioningConfiguration>> {
        let request = S3Request::new(Method::GET, Some(bucket), "").query("versioning", "");
        let response = self.send(&request).await?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let body = api::body_bytes(response).await?;
        Ok(xml::parse(&body).ok())
    }

    /// Whether a version could not be removed because the bucket asks for MFA.
    async fn refused_for_mfa(&self, bucket: &str, error: &AppError) -> bool {
        if error.kind != ErrorKind::PermissionDenied {
            return false;
        }
        if error.message.to_ascii_lowercase().contains("mfa") {
            return true;
        }
        matches!(
            self.bucket_versioning(bucket).await,
            Ok(Some(configuration))
                if configuration
                    .mfa_delete
                    .as_deref()
                    .is_some_and(|state| state.trim().eq_ignore_ascii_case("Enabled"))
        )
    }

    /// One page of the versions and delete markers under `prefix`, deeper keys grouped into
    /// folders when `grouped`.
    async fn versions_page(
        &self,
        bucket: &str,
        prefix: &str,
        grouped: bool,
        marker: Option<&VersionMarker>,
    ) -> AppResult<VersionPage> {
        let mut request = S3Request::new(Method::GET, Some(bucket), "")
            .query("versions", "")
            .query("prefix", prefix)
            .query("max-keys", PAGE_SIZE)
            // Keys may hold characters XML cannot carry.
            .query("encoding-type", "url");
        if grouped {
            request = request.query("delimiter", "/");
        }
        if let Some(marker) = marker {
            request = request.query("key-marker", &marker.key);
            if !marker.version_id.is_empty() {
                request = request.query("version-id-marker", &marker.version_id);
            }
        }
        let path = format!("/{bucket}/{prefix}");
        let response = self.send(&request).await?;
        let status = response.status();
        let body = api::body_bytes(response).await?;
        match xml::embedded_error(&body) {
            Some(error) if not_implemented(status, &error.code) => {
                Err(versions::no_versions(&path))
            }
            Some(error) => Err(service_error(status, error, &path)),
            None if status.is_success() => xml::version_page(&body),
            None if not_implemented(status, "") => Err(versions::no_versions(&path)),
            None => Err(service_error(status, xml::ErrorBody::default(), &path)),
        }
    }

    /// Walks the versions under `prefix` page by page until `visit` returns `false`.
    async fn each_versions_page(
        &self,
        bucket: &str,
        prefix: &str,
        grouped: bool,
        mut visit: impl FnMut(VersionPage) -> bool + Send,
    ) -> AppResult<()> {
        let mut marker: Option<VersionMarker> = None;
        loop {
            let mut page = self
                .versions_page(bucket, prefix, grouped, marker.as_ref())
                .await?;
            let next = page.next.take();
            // A service that repeats a marker would never finish.
            let more = visit(page) && next.is_some() && next != marker;
            if !more {
                return Ok(());
            }
            marker = next;
        }
    }

    /// The history of one key, newest first.
    async fn history_of(&self, bucket: &str, key: &str) -> AppResult<Vec<ObjectVersion>> {
        let mut history = Vec::new();
        self.each_versions_page(bucket, key, false, |page| {
            // Keys come in order, the key itself before every longer one that starts with it.
            let mut reached_other_keys = false;
            for record in page.records {
                if record.key == key {
                    history.push(record.version);
                } else {
                    reached_other_keys = true;
                }
            }
            !reached_other_keys
        })
        .await?;
        versions::newest_first(&mut history);
        Ok(history)
    }

    /// The hidden files under a prefix, and the folders deeper keys fall into when `grouped`.
    async fn hidden_under(
        &self,
        bucket: &str,
        prefix: &str,
        grouped: bool,
    ) -> AppResult<(Vec<HiddenFile>, Vec<String>)> {
        let mut hidden = HiddenFiles::default();
        let mut folders = Vec::new();
        self.each_versions_page(bucket, prefix, grouped, |page| {
            folders.extend(
                page.folders
                    .into_iter()
                    .map(|folder| format!("/{bucket}/{folder}")),
            );
            hidden.add_page(page.records);
            true
        })
        .await?;
        Ok((hidden.finish(bucket), folders))
    }

    /// Removes versions of objects in one bucket, a thousand to a request where the service
    /// can, and returns the ones it refused.
    async fn delete_bucket_versions(
        &self,
        bucket: &str,
        versions: &[(String, String)],
    ) -> Vec<((String, String), AppError)> {
        let mut refused = Vec::new();
        for batch in versions.chunks(DELETE_BATCH) {
            match self.delete_version_batch(bucket, batch).await {
                Ok(errors) => refused.extend(errors),
                Err(error) => {
                    refused.extend(batch.iter().map(|version| (version.clone(), error.clone())))
                }
            }
        }
        if let Some((_, first)) = refused.first() {
            if self.refused_for_mfa(bucket, first).await {
                for ((key, _), error) in &mut refused {
                    if error.kind == ErrorKind::PermissionDenied {
                        *error = mfa_refusal(bucket, &format!("/{bucket}/{key}"));
                    }
                }
            }
        }
        refused
    }

    async fn delete_version_batch(
        &self,
        bucket: &str,
        batch: &[(String, String)],
    ) -> AppResult<Vec<((String, String), AppError)>> {
        let body = Bytes::from(xml::delete_versions(batch));
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
            let mut refused = Vec::new();
            for (key, version_id) in batch {
                if let Err(error) = self.delete_one_version(bucket, key, version_id).await {
                    refused.push(((key.clone(), version_id.clone()), error));
                }
            }
            return Ok(refused);
        }
        if !response.status().is_success() {
            return Err(failure(response, &format!("/{bucket}")).await);
        }
        let result: xml::DeleteResult = xml::parse(&api::body_bytes(response).await?)?;
        Ok(result
            .errors
            .into_iter()
            .map(|refused| {
                let path = format!("/{bucket}/{}", refused.key);
                let error = xml::ErrorBody {
                    code: refused.code,
                    message: refused.message,
                    region: String::new(),
                };
                let version_id = refused
                    .version_id
                    .unwrap_or_else(|| xml::NULL_VERSION.to_string());
                (
                    (refused.key, version_id),
                    service_error(StatusCode::OK, error, &path),
                )
            })
            .collect())
    }

    async fn delete_one_version(&self, bucket: &str, key: &str, version_id: &str) -> AppResult<()> {
        let request =
            S3Request::new(Method::DELETE, Some(bucket), key).query("versionId", version_id);
        let response = self.send(&request).await?;
        match response.status() {
            status if status.is_success() || status == StatusCode::NOT_FOUND => Ok(()),
            _ => Err(failure(response, &format!("/{bucket}/{key}")).await),
        }
    }

    /// Removes every version and delete marker in a bucket, so that it can be deleted. Each
    /// round lists from the start again: a marker into entries just removed is not one every
    /// service can continue from.
    pub(super) async fn purge_versions(&self, bucket: &str) -> AppResult<()> {
        let mut previous_first: Option<(String, String)> = None;
        loop {
            let page = self.versions_page(bucket, "", false, None).await?;
            let versions: Vec<(String, String)> = page
                .records
                .into_iter()
                .map(|record| (record.key, record.version.version_id))
                .collect();
            let Some(first) = versions.first().cloned() else {
                return Ok(());
            };
            if previous_first.as_ref() == Some(&first) {
                return Err(AppError::new(
                    ErrorKind::Cloud,
                    format!("The versions in the bucket {bucket} could not be removed"),
                )
                .with_path(format!("/{bucket}")));
            }
            if let Some((_, error)) = self
                .delete_bucket_versions(bucket, &versions)
                .await
                .into_iter()
                .next()
            {
                return Err(error);
            }
            previous_first = Some(first);
        }
    }
}

#[async_trait]
impl ObjectVersions for S3Fs {
    async fn versioning(&self, path: &str) -> AppResult<Versioning> {
        let (Some(bucket), _) = split(&self.resolve(path)) else {
            return Ok(Versioning::Off);
        };
        let request = S3Request::new(Method::GET, Some(&bucket), "").query("versioning", "");
        let response = self.send(&request).await?;
        let status = response.status();
        let body = api::body_bytes(response).await?;
        let error = match status.is_success() {
            true => xml::ErrorBody::default(),
            false => xml::embedded_error(&body).unwrap_or_default(),
        };
        match versioning_of(status, &error.code, &body) {
            Some(versioning) => Ok(versioning),
            None => Err(service_error(status, error, &format!("/{bucket}"))),
        }
    }

    async fn list_versions(&self, path: &str) -> AppResult<Vec<ObjectVersion>> {
        let (bucket, key, _) = self.object_at(path)?;
        self.history_of(&bucket, &key).await
    }

    async fn list_deleted(&self, folder: &str, live: &[FileEntry]) -> AppResult<Vec<DeletedEntry>> {
        let directory = protocol::normalize(&self.resolve(folder));
        let (Some(bucket), key) = split(&directory) else {
            return Ok(Vec::new());
        };
        match self.hidden_under(&bucket, &prefix_of(&key), true).await {
            Ok((hidden, folders)) => Ok(versions::deleted_entries(
                &directory, &hidden, &folders, live,
            )),
            // Nothing is kept, so nothing was only hidden.
            Err(error) if error.kind == ErrorKind::Unsupported => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    async fn deleted_under(&self, folder: &str) -> AppResult<Vec<HiddenFile>> {
        let directory = protocol::normalize(&self.resolve(folder));
        let (Some(bucket), key) = split(&directory) else {
            return Err(AppError::invalid("Deleted files are kept inside a bucket"));
        };
        let (hidden, _) = self.hidden_under(&bucket, &prefix_of(&key), false).await?;
        Ok(hidden)
    }

    async fn restore_version(&self, path: &str, version_id: &str) -> AppResult<()> {
        versions::validate_version_id(version_id)?;
        let (bucket, key, path) = self.object_at(path)?;
        let version = self
            .head_version(&bucket, &key, Some(version_id))
            .await?
            .ok_or_else(|| crate::cloud::not_found(&path))?;
        self.copy_object(&bucket, &key, Some(version_id), version.size, &key)
            .await
    }

    async fn stat_version(&self, path: &str, version_id: &str) -> AppResult<RemoteStat> {
        versions::validate_version_id(version_id)?;
        let (bucket, key, path) = self.object_at(path)?;
        self.head_version(&bucket, &key, Some(version_id))
            .await?
            .ok_or_else(|| crate::cloud::not_found(&path))
    }

    async fn open_version(
        &self,
        path: &str,
        version_id: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        versions::validate_version_id(version_id)?;
        self.read_object(path, Some(version_id), range).await
    }

    async fn delete_version(&self, path: &str, version_id: &str) -> AppResult<()> {
        versions::validate_version_id(version_id)?;
        let (bucket, key, path) = self.object_at(path)?;
        match self.delete_one_version(&bucket, &key, version_id).await {
            Err(error) if self.refused_for_mfa(&bucket, &error).await => {
                Err(mfa_refusal(&bucket, &path))
            }
            deleted => deleted,
        }
    }

    async fn delete_with_versions(&self, paths: &[String]) -> AppResult<()> {
        self.delete_paths(paths, true).await
    }

    async fn delete_versions(&self, versions: &[VersionRef]) -> Vec<(VersionRef, AppError)> {
        let mut by_bucket: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut paths: HashMap<(String, String, String), &VersionRef> = HashMap::new();
        let mut refused = Vec::new();
        for version in versions {
            let resolved = self.resolve(&version.path);
            match split_keeping_slash(&resolved) {
                (Some(bucket), key) if !key.is_empty() && versions::reachable(&key) => {
                    paths.insert(
                        (bucket.clone(), key.clone(), version.version_id.clone()),
                        version,
                    );
                    by_bucket
                        .entry(bucket)
                        .or_default()
                        .push((key, version.version_id.clone()));
                }
                _ => refused.push((
                    version.clone(),
                    AppError::invalid("Versions belong to files inside a bucket")
                        .with_path(&version.path),
                )),
            }
        }
        for (bucket, bucket_versions) in by_bucket {
            for ((key, version_id), error) in
                self.delete_bucket_versions(&bucket, &bucket_versions).await
            {
                if let Some(version) = paths.get(&(bucket.clone(), key, version_id)) {
                    refused.push(((*version).clone(), error));
                }
            }
        }
        refused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENABLED: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Status>Enabled</Status></VersioningConfiguration>"#;
    const SUSPENDED: &[u8] = b"<VersioningConfiguration><Status>Suspended</Status><MfaDelete>Disabled</MfaDelete></VersioningConfiguration>";
    const NEVER: &[u8] =
        br#"<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"/>"#;

    #[test]
    fn reads_the_versioning_status() {
        let ok = StatusCode::OK;
        assert_eq!(versioning_of(ok, "", ENABLED), Some(Versioning::Enabled));
        assert_eq!(
            versioning_of(ok, "", SUSPENDED),
            Some(Versioning::Suspended)
        );
        assert_eq!(versioning_of(ok, "", NEVER), Some(Versioning::Off));
    }

    #[test]
    fn services_without_versioning_keep_no_versions() {
        let off = Some(Versioning::Off);
        // An empty answer, one that is not XML, and one without a status.
        assert_eq!(versioning_of(StatusCode::OK, "", b""), off);
        assert_eq!(versioning_of(StatusCode::OK, "", b"{}"), off);
        assert_eq!(
            versioning_of(
                StatusCode::OK,
                "",
                b"<ListBucketResult><Name>b</Name></ListBucketResult>"
            ),
            off
        );
        assert_eq!(
            versioning_of(StatusCode::NOT_IMPLEMENTED, "NotImplemented", b""),
            off
        );
        assert_eq!(versioning_of(StatusCode::NOT_FOUND, "", b""), off);
        assert_eq!(
            versioning_of(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed", b""),
            off
        );
        assert_eq!(
            versioning_of(StatusCode::BAD_REQUEST, "NotImplemented", b""),
            off
        );
        // Keys that may not ask leave it open, and real failures stay failures.
        assert_eq!(
            versioning_of(StatusCode::FORBIDDEN, "AccessDenied", b""),
            Some(Versioning::Unknown)
        );
        assert_eq!(
            versioning_of(StatusCode::NOT_FOUND, "NoSuchBucket", b""),
            None
        );
        assert_eq!(
            versioning_of(StatusCode::SERVICE_UNAVAILABLE, "SlowDown", b""),
            None
        );
        assert!(not_implemented(StatusCode::NOT_IMPLEMENTED, ""));
        assert!(!not_implemented(StatusCode::NOT_FOUND, "NoSuchBucket"));
        assert!(!not_implemented(StatusCode::FORBIDDEN, "AccessDenied"));
    }

    #[test]
    fn keeps_the_slash_of_a_folder_object() {
        assert_eq!(
            split_keeping_slash("/photos/2024/"),
            (Some("photos".into()), "2024/".into())
        );
        assert_eq!(
            split_keeping_slash("/photos/2024/a.txt"),
            (Some("photos".into()), "2024/a.txt".into())
        );
        assert_eq!(
            split_keeping_slash("/photos/"),
            (Some("photos".into()), String::new())
        );
        assert_eq!(prefix_of(""), "");
        assert_eq!(prefix_of("2024/march"), "2024/march/");
    }

    #[test]
    fn explains_mfa_delete() {
        let error = mfa_refusal("photos", "/photos/a.txt");
        assert_eq!(error.kind, ErrorKind::PermissionDenied);
        assert!(error.message.contains("MFA delete"));
        assert_eq!(error.path.as_deref(), Some("/photos/a.txt"));
    }
}
