//! `versions`: the history a bucket keeps of a file, as a list, and one version of it made
//! current again or downloaded through the transfer queue.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;

use super::{absolute_local, missing};
use crate::cli::args::VersionsArgs;
use crate::cli::context::{Connected, Context};
use crate::cli::output::{time_iso, time_string, Reply};
use crate::cli::transfers;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol;
use crate::transfer::{TransferManager, VersionDownload};
use crate::versions::{self, ObjectVersion, ObjectVersions, VersionKind, Versioning};

pub async fn versions(context: &Context, arguments: &VersionsArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    let versioned = versions::of(files.as_ref())?;
    let path = connected.resolve(&arguments.path);
    if let Some(version_id) = &arguments.restore {
        versions::validate_version_id(version_id)?;
        versioned.restore_version(&path, version_id).await?;
        let text = format!(
            "Restored version {version_id} of {path}; what it replaced stays in the history"
        );
        return Ok(Reply::new(
            json!({ "path": path, "restored": version_id }),
            text,
        ));
    }
    if let Some([version_id, destination]) = arguments.get.as_deref() {
        return download(
            context,
            connected,
            versioned,
            &path,
            version_id,
            destination,
            arguments,
        )
        .await;
    }
    list(versioned, path).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionView {
    #[serde(flatten)]
    version: ObjectVersion,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_iso: Option<String>,
}

async fn list(versioned: &dyn ObjectVersions, path: String) -> AppResult<Reply> {
    let versioning = versioned.versioning(&path).await?;
    let keeps_none = |path: String| {
        Reply::new(
            json!({ "path": path, "versioning": Versioning::Off, "versions": [] }),
            versions::NO_VERSIONS,
        )
    };
    if !versioning.keeps_versions() {
        return Ok(keeps_none(path));
    }
    let history = match versioned.list_versions(&path).await {
        Ok(history) => history,
        Err(error) if error.kind == ErrorKind::Unsupported => return Ok(keeps_none(path)),
        Err(error) => return Err(error),
    };
    if history.is_empty() {
        return Err(missing(&path));
    }
    let text = render(&history);
    let views: Vec<VersionView> = history
        .into_iter()
        .map(|version| VersionView {
            modified_iso: time_iso(version.modified),
            version,
        })
        .collect();
    Ok(Reply::new(
        json!({ "path": path, "versioning": versioning, "versions": views }),
        text,
    ))
}

/// What a row of the list says after the version's id.
fn note(version: &ObjectVersion) -> String {
    let state = match (version.kind, version.latest) {
        (VersionKind::Version, true) => "current",
        (VersionKind::Version, false) => "",
        (VersionKind::DeleteMarker, true) => "deleted",
        (VersionKind::DeleteMarker, false) => "delete marker",
    };
    [state, version.storage_class.as_deref().unwrap_or_default()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
}

fn render(history: &[ObjectVersion]) -> String {
    let size_of = |version: &ObjectVersion| match version.kind {
        VersionKind::Version => version.size.to_string(),
        VersionKind::DeleteMarker => "-".to_string(),
    };
    let size_width = history
        .iter()
        .map(|version| size_of(version).len())
        .max()
        .unwrap_or(0);
    let id_width = history
        .iter()
        .map(|version| version.version_id.chars().count())
        .max()
        .unwrap_or(0);
    history
        .iter()
        .map(|version| {
            format!(
                "{}  {:>size_width$}  {:<id_width$}  {}",
                time_string(version.modified),
                size_of(version),
                version.version_id,
                note(version)
            )
            .trim_end()
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where one downloaded file goes, by `cp`'s rules: into a folder that exists or is named with
/// a trailing separator, otherwise to the path as the file's new name.
fn placement(destination: &str) -> AppResult<(PathBuf, Option<String>)> {
    let target = absolute_local(destination)?;
    let meant_as_folder =
        destination.ends_with('/') || destination.ends_with(std::path::MAIN_SEPARATOR);
    if target.is_dir() || meant_as_folder {
        return Ok((target, None));
    }
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    match (target.parent(), name) {
        (Some(folder), Some(name)) => Ok((folder.to_path_buf(), Some(name))),
        _ => Err(AppError::invalid(format!(
            "{destination} has no folder above it"
        ))),
    }
}

async fn download(
    context: &Context,
    connected: &Connected,
    versioned: &dyn ObjectVersions,
    path: &str,
    version_id: &str,
    destination: &str,
    arguments: &VersionsArgs,
) -> AppResult<Reply> {
    versions::validate_version_id(version_id)?;
    let version = versioned.stat_version(path, version_id).await?;
    let (folder, name) = placement(destination)?;
    let shown = folder.to_string_lossy().into_owned();
    if !folder.is_dir() {
        if folder.exists() {
            return Err(protocol::not_a_folder(&shown));
        }
        if !arguments.parents {
            return Err(AppError::new(
                ErrorKind::NotFound,
                format!("The folder {shown} does not exist; add --parents to create it"),
            )
            .with_path(shown));
        }
        std::fs::create_dir_all(&folder)
            .map_err(|error| AppError::from(error).with_path(shown.clone()))?;
    }
    let settings = transfers::settings(&context.settings.transfers, &arguments.options);
    let (workers, verified) = (settings.workers, settings.verify_checksums);
    let manager = TransferManager::new(context.sessions.clone(), context.events.clone(), settings);
    manager
        .enqueue_version(VersionDownload {
            session_id: connected.info.id.clone(),
            path: path.to_string(),
            version_id: version_id.to_string(),
            target_directory: shown,
            name,
            size: version.size,
        })
        .await?;
    let summary = transfers::wait(context, &manager, workers, verified).await?;
    Ok(summary.into_reply("downloaded"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, kind: VersionKind, latest: bool, size: u64) -> ObjectVersion {
        ObjectVersion {
            version_id: id.to_string(),
            kind,
            latest,
            modified: None,
            size,
            etag: None,
            storage_class: None,
        }
    }

    #[test]
    fn lists_versions_in_columns() {
        let mut archived = entry("v1", VersionKind::Version, false, 1200);
        archived.storage_class = Some("GLACIER".into());
        let history = [
            entry("marker-3", VersionKind::DeleteMarker, true, 0),
            entry("v2", VersionKind::Version, false, 7),
            archived,
            entry("null", VersionKind::Version, false, 0),
        ];
        let blank = "-".repeat(16);
        assert_eq!(
            render(&history),
            format!(
                "{blank}     -  marker-3  deleted\n{blank}     7  v2\n{blank}  1200  v1        GLACIER\n{blank}     0  null"
            )
        );
        assert_eq!(note(&entry("v", VersionKind::Version, true, 1)), "current");
        assert_eq!(
            note(&entry("m", VersionKind::DeleteMarker, false, 0)),
            "delete marker"
        );
    }

    #[test]
    fn places_one_download() {
        let folder = tempfile::tempdir().unwrap();
        let inside = folder.path().to_string_lossy().into_owned();
        let (target, name) = placement(&inside).unwrap();
        assert_eq!(target, absolute_local(&inside).unwrap());
        assert_eq!(name, None);
        let renamed = folder.path().join("old.txt");
        let (target, name) = placement(&renamed.to_string_lossy()).unwrap();
        assert_eq!(target, absolute_local(&inside).unwrap());
        assert_eq!(name.as_deref(), Some("old.txt"));
        let new_folder = format!("{}{}", renamed.to_string_lossy(), std::path::MAIN_SEPARATOR);
        let (_, name) = placement(&new_folder).unwrap();
        assert_eq!(name, None);
    }
}
