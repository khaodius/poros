//! Downloads of one version of a file from a bucket that keeps versions. The version's id
//! travels in the job's options, which the queue already saves, so nothing about a job or the
//! saved queue changes for transfers that name no version. The worker reads the source through
//! a view of the server in which the file is that version, and so splits it between workers,
//! limits, resumes and checks it like any download.

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use super::options::JobOptions;
use super::queue::{Direction, JobKind, JobSpec};
use super::{Placement, TransferManager};
use crate::error::AppResult;
use crate::protocol::{self, RemoteFileSystem};
use crate::versions::{self, PinnedVersion};
use crate::{local, remote_path};

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDownload {
    pub session_id: String,
    pub path: String,
    pub version_id: String,
    /// The folder on this computer the version goes into.
    pub target_directory: String,
    /// The name it gets there; the file's own when left out.
    #[serde(default)]
    pub name: Option<String>,
    /// The version's size where the caller knows it, for the queue's totals before it starts.
    #[serde(default)]
    pub size: u64,
}

impl TransferManager {
    /// Queues the download of one version of a file.
    pub async fn enqueue_version(&self, request: VersionDownload) -> AppResult<()> {
        versions::validate_version_id(&request.version_id)?;
        let session = self.shared.sessions.get(&request.session_id).await?;
        let files = session.files();
        versions::of(files.as_ref())?;
        let source = protocol::normalize(&files.resolve(&request.path));
        let name = match request.name {
            Some(name) => name,
            None => remote_path::file_name(&source).to_string(),
        };
        // The name comes from the server or the page: it must stay inside the target folder.
        local::validate_name(&name)?;
        let target = Path::new(&request.target_directory)
            .join(&name)
            .to_string_lossy()
            .into_owned();
        let spec = JobSpec {
            session_id: session.id.clone(),
            source_session_id: None,
            direction: Direction::Download,
            kind: JobKind::File,
            name,
            source,
            target,
            target_directory: request.target_directory,
            size: request.size,
            ancestors: Arc::from([]),
            options: Some(Arc::new(JobOptions {
                source_version: Some(request.version_id),
                ..JobOptions::default()
            })),
        };
        self.register(&session);
        self.add_jobs(vec![spec], Placement::Last);
        Ok(())
    }
}

/// The version of its source a download asks for, if it asks for one.
pub(super) fn source_version(spec: &JobSpec) -> Option<&str> {
    match spec.direction {
        Direction::Download => spec.options.as_deref()?.source_version.as_deref(),
        Direction::Upload | Direction::Relay => None,
    }
}

/// The file system a job reads its source from: the server's own, with the source showing as
/// the version the job asks for.
pub(super) fn source_files(
    spec: &JobSpec,
    files: Arc<dyn RemoteFileSystem>,
) -> AppResult<Arc<dyn RemoteFileSystem>> {
    match source_version(spec) {
        Some(version_id) => PinnedVersion::over(files, &spec.source, version_id),
        None => Ok(files),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn download(options: Option<JobOptions>) -> JobSpec {
        JobSpec {
            session_id: "s".into(),
            source_session_id: None,
            direction: Direction::Download,
            kind: JobKind::File,
            name: "a.txt".into(),
            source: "/bucket/a.txt".into(),
            target: "/tmp/a.txt".into(),
            target_directory: "/tmp".into(),
            size: 1,
            ancestors: Arc::from([]),
            options: options.map(Arc::new),
        }
    }

    #[test]
    fn only_downloads_name_a_version() {
        let versioned = JobOptions {
            source_version: Some("v1".into()),
            ..JobOptions::default()
        };
        assert_eq!(source_version(&download(None)), None);
        assert_eq!(source_version(&download(Some(JobOptions::default()))), None);
        assert_eq!(
            source_version(&download(Some(versioned.clone()))),
            Some("v1")
        );
        let mut upload = download(Some(versioned));
        upload.direction = Direction::Upload;
        assert_eq!(source_version(&upload), None);
    }

    #[test]
    fn options_without_a_version_are_saved_as_before() {
        let plain = serde_json::to_value(JobOptions::default()).unwrap();
        assert!(plain.get("sourceVersion").is_none());
        let versioned = JobOptions {
            source_version: Some("null".into()),
            ..JobOptions::default()
        };
        let saved = serde_json::to_value(&versioned).unwrap();
        assert_eq!(saved["sourceVersion"], "null");
        let loaded: JobOptions = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded, versioned);
        // What 0.7.0 wrote has no such field.
        let older: JobOptions = serde_json::from_value(plain).unwrap();
        assert_eq!(older.source_version, None);
    }
}
