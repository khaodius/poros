//! Bringing back what a delete only hid in a bucket that keeps versions, or removing it for
//! good: single files and whole folders, as one operation that reports progress and can be
//! cancelled.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{Failure, Progress};
use crate::error::{AppError, AppResult};
use crate::protocol::RemoteFileSystem;
use crate::remote_path;
use crate::versions::{self, HiddenFile, ObjectVersions, VersionRef};

/// Files handled between two looks at whether the operation was cancelled. Services that
/// remove many versions in one request get this many files' worth at a time.
const BATCH: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeletedAction {
    /// Removes the delete markers, so the newest version shows again.
    Restore,
    /// Removes every version and marker, so nothing is left to bring back.
    Purge,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedItem {
    pub path: String,
    /// A folder stands for every deleted file under it.
    pub is_dir: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRequest {
    /// Names this operation for progress events and cancelling.
    pub operation_id: String,
    pub session_id: String,
    pub action: DeletedAction,
    pub items: Vec<DeletedItem>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedSummary {
    /// Files brought back or removed, counting those inside folders.
    pub files: u64,
    /// Files that could not be, of which `failures` holds the first ones.
    pub failed: u64,
    pub failures: Vec<Failure>,
}

pub async fn run(
    files: &dyn RemoteFileSystem,
    request: &DeletedRequest,
    cancel: &CancellationToken,
    progress: &Progress,
) -> AppResult<DeletedSummary> {
    let versioned = versions::of(files)?;
    progress.set_counting(true);
    let mut hidden: Vec<HiddenFile> = Vec::new();
    for item in &request.items {
        let path = files.resolve(&item.path);
        progress.working_on(&path);
        let found = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(AppError::cancelled()),
            found = find(versioned, &path, item.is_dir, request.action) => found,
        };
        match found {
            Ok(found) => {
                progress.found(found.len() as u64);
                hidden.extend(found);
            }
            Err(error) => progress.fail(&path, error),
        }
    }
    progress.set_counting(false);
    for batch in hidden.chunks(BATCH) {
        if cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        progress.working_on(&batch[0].path);
        let references: Vec<VersionRef> = batch
            .iter()
            .flat_map(|file| match request.action {
                DeletedAction::Restore => file.marker_references(),
                DeletedAction::Purge => file.every_reference(),
            })
            .collect();
        let mut refused: HashMap<String, AppError> = HashMap::new();
        for (version, error) in versioned.delete_versions(&references).await {
            refused.entry(version.path).or_insert(error);
        }
        for file in batch {
            match refused.remove(&file.path) {
                Some(error) => progress.fail(&file.path, error),
                None => progress.file_done(),
            }
        }
    }
    Ok(DeletedSummary {
        files: progress.files_done(),
        failed: progress.failed(),
        failures: progress.take_failures(),
    })
}

/// The hidden files an item stands for: itself, or everything deleted under a folder.
async fn find(
    versioned: &dyn ObjectVersions,
    path: &str,
    is_dir: bool,
    action: DeletedAction,
) -> AppResult<Vec<HiddenFile>> {
    if is_dir {
        let mut hidden = versioned.deleted_under(path).await?;
        if action == DeletedAction::Restore {
            // A marker with no version under it hides nothing that could come back.
            hidden.retain(|file| file.newest_version().is_some());
        }
        return Ok(hidden);
    }
    let file = HiddenFile {
        path: path.to_string(),
        history: versioned.list_versions(path).await?,
    };
    if action == DeletedAction::Restore {
        if file.markers().next().is_none() {
            return Err(AppError::invalid(format!(
                "{} is not deleted",
                remote_path::file_name(path)
            ))
            .with_path(path));
        }
        if file.newest_version().is_none() {
            return Err(versions::nothing_to_restore(path));
        }
    }
    Ok(vec![file])
}
