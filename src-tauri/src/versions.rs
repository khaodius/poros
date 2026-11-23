//! The history a versioned S3 bucket or Backblaze B2 keeps of each file: the versions that
//! writes left behind and the markers that deletes put over them. An optional ability beside
//! `RemoteFileSystem`, reached through `RemoteFileSystem::as_versioned`.

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{DirListing, FileEntry};
use crate::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use crate::remote_path;

/// What the menus and the command line say where nothing is kept.
pub const NO_VERSIONS: &str = "This bucket keeps no versions";
/// Longer than any service's version ids, short enough for a query string.
const MAX_VERSION_ID_BYTES: usize = 1024;

/// Whether the bucket a path is in keeps versions. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Versioning {
    /// Every write adds a version and a delete only puts a marker over the file.
    Enabled,
    /// Versions written earlier are kept, and new writes replace the `null` version.
    Suspended,
    /// Never turned on, or the service has no versioning.
    Off,
    /// The keys may not ask, so versions are offered and the service has the last word.
    Unknown,
}

impl Versioning {
    pub fn keeps_versions(self) -> bool {
        self != Versioning::Off
    }
}

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VersionKind {
    Version,
    /// What a delete leaves on top of the versions: an S3 delete marker or a B2 hide marker.
    DeleteMarker,
}

/// One entry of a file's history. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectVersion {
    /// `null` on S3 for what was written before versioning was turned on or while it was
    /// suspended.
    pub version_id: String,
    pub kind: VersionKind,
    /// The newest entry: the file as it is now, or the marker that hides it.
    pub latest: bool,
    /// When the version was stored, in seconds since the Unix epoch.
    pub modified: Option<i64>,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
}

impl ObjectVersion {
    pub fn is_marker(&self) -> bool {
        self.kind == VersionKind::DeleteMarker
    }
}

/// One entry of a listing that covers the histories of many files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    /// The object's key, or the file's name, within its bucket.
    pub key: String,
    pub version: ObjectVersion,
}

/// One version of a file, named for deleting it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRef {
    pub path: String,
    pub version_id: String,
}

/// A file a delete hid, with its history newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiddenFile {
    /// Ends in a slash for the empty object that keeps a folder.
    pub path: String,
    pub history: Vec<ObjectVersion>,
}

impl HiddenFile {
    /// The markers over the newest version; removing them brings the file back.
    pub fn markers(&self) -> impl Iterator<Item = &ObjectVersion> {
        self.history.iter().take_while(|entry| entry.is_marker())
    }

    /// The version a restore brings back.
    pub fn newest_version(&self) -> Option<&ObjectVersion> {
        self.history.iter().find(|entry| !entry.is_marker())
    }

    pub fn deleted(&self) -> Option<i64> {
        self.history.first().and_then(|marker| marker.modified)
    }

    fn references<'a>(
        &'a self,
        entries: impl Iterator<Item = &'a ObjectVersion> + 'a,
    ) -> impl Iterator<Item = VersionRef> + 'a {
        entries.map(|entry| VersionRef {
            path: self.path.clone(),
            version_id: entry.version_id.clone(),
        })
    }

    pub fn marker_references(&self) -> Vec<VersionRef> {
        self.references(self.markers()).collect()
    }

    pub fn every_reference(&self) -> Vec<VersionRef> {
        self.references(self.history.iter()).collect()
    }
}

/// A file or folder a delete hid, as a pane shows it. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// When it was deleted; a folder has no time of its own.
    pub deleted: Option<i64>,
    /// The size of the version a restore brings back.
    pub size: u64,
}

/// A folder's listing with what was deleted from it. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingWithDeleted {
    pub listing: DirListing,
    pub deleted: Vec<DeletedEntry>,
}

#[async_trait]
pub trait ObjectVersions: Send + Sync {
    /// Whether the bucket `path` is in keeps versions.
    async fn versioning(&self, path: &str) -> AppResult<Versioning>;

    /// Every version and delete marker of the file at `path`, newest first.
    async fn list_versions(&self, path: &str) -> AppResult<Vec<ObjectVersion>>;

    /// The files and folders directly in `folder` that a delete hid. `live` is what the folder
    /// lists now: a folder that still holds something is not deleted.
    async fn list_deleted(&self, folder: &str, live: &[FileEntry]) -> AppResult<Vec<DeletedEntry>>;

    /// Every hidden file under `folder`, at any depth.
    async fn deleted_under(&self, folder: &str) -> AppResult<Vec<HiddenFile>>;

    /// Makes a version the current one again by copying it over the file. Nothing is removed:
    /// the content it replaces stays in the history.
    async fn restore_version(&self, path: &str, version_id: &str) -> AppResult<()>;

    async fn stat_version(&self, path: &str, version_id: &str) -> AppResult<RemoteStat>;

    async fn open_version(
        &self,
        path: &str,
        version_id: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>>;

    /// Removes one version or delete marker for good.
    async fn delete_version(&self, path: &str, version_id: &str) -> AppResult<()>;

    /// Removes many versions, in as few requests as the service allows, and returns the ones it
    /// could not remove.
    async fn delete_versions(&self, versions: &[VersionRef]) -> Vec<(VersionRef, AppError)> {
        let mut refused = Vec::new();
        for version in versions {
            if let Err(error) = self
                .delete_version(&version.path, &version.version_id)
                .await
            {
                refused.push((version.clone(), error));
            }
        }
        refused
    }

    /// Brings back a deleted file by removing the markers over its newest version.
    async fn undelete(&self, path: &str) -> AppResult<()> {
        let hidden = HiddenFile {
            path: path.to_string(),
            history: self.list_versions(path).await?,
        };
        let markers = hidden.marker_references();
        if markers.is_empty() {
            return Err(AppError::invalid(format!(
                "{} is not deleted",
                remote_path::file_name(path)
            ))
            .with_path(path));
        }
        if hidden.newest_version().is_none() {
            return Err(nothing_to_restore(path));
        }
        match self.delete_versions(&markers).await.into_iter().next() {
            Some((_, error)) => Err(error),
            None => Ok(()),
        }
    }
}

pub fn nothing_to_restore(path: &str) -> AppError {
    AppError::new(
        ErrorKind::NotFound,
        format!(
            "No version of {} is left to bring back",
            remote_path::file_name(path)
        ),
    )
    .with_path(path)
}

/// The error for a service or bucket without versions.
pub fn no_versions(path: &str) -> AppError {
    AppError::unsupported(NO_VERSIONS).with_path(path)
}

pub fn marker_has_no_content(path: &str) -> AppError {
    AppError::invalid("This entry marks a delete and has no content").with_path(path)
}

/// The history ability of a connection, or why it has none.
pub fn of(files: &dyn RemoteFileSystem) -> AppResult<&dyn ObjectVersions> {
    files.as_versioned().ok_or_else(|| {
        AppError::unsupported(match files.protocol() {
            // The bucket keeps versions of the encrypted files, which only the vault can read.
            Protocol::Cryptomator => "Versions of files inside a vault cannot be shown".to_string(),
            protocol => format!("{} keeps no versions of files", protocol.display_name()),
        })
    })
}

/// A version id as a request may carry it. Ids come from the page, so they are checked before
/// they reach a query string or a JSON body.
pub fn validate_version_id(version_id: &str) -> AppResult<()> {
    let acceptable = !version_id.is_empty()
        && version_id.len() <= MAX_VERSION_ID_BYTES
        && !version_id.chars().any(char::is_control);
    if acceptable {
        Ok(())
    } else {
        Err(AppError::invalid("That is not a version of this file"))
    }
}

/// Orders a file's history newest first. The newest entry leads; the rest go by time, and
/// entries stored within the same second keep the order the service listed them in, which is
/// newest first on Amazon.
pub fn newest_first(history: &mut [ObjectVersion]) {
    history
        .sort_by_key(|entry| std::cmp::Reverse((entry.latest, entry.modified.unwrap_or(i64::MIN))));
}

/// Whether a key can be reached by a path. Keys with empty, `.` or `..` parts cannot, and are
/// left alone the way listings leave them out.
pub fn reachable(key: &str) -> bool {
    let name = key.strip_suffix('/').unwrap_or(key);
    !name.is_empty() && protocol::normalize(&format!("/{name}")) == format!("/{name}")
}

/// Finds the hidden files in a listing of histories, page by page, keeping only what belongs
/// to them so a large bucket does not have to fit in memory.
#[derive(Debug, Default)]
pub struct HiddenFiles {
    histories: BTreeMap<String, Vec<ObjectVersion>>,
}

impl HiddenFiles {
    /// Takes the next page. A file's newest entry comes no later than its older ones, so
    /// entries of a file not yet known to be hidden belong to one that is still there.
    pub fn add_page(&mut self, records: Vec<HistoryRecord>) {
        for record in &records {
            if record.version.latest && record.version.is_marker() && reachable(&record.key) {
                self.histories.entry(record.key.clone()).or_default();
            }
        }
        for record in records {
            if let Some(history) = self.histories.get_mut(&record.key) {
                history.push(record.version);
            }
        }
    }

    pub fn finish(self, bucket: &str) -> Vec<HiddenFile> {
        self.histories
            .into_iter()
            .map(|(key, mut history)| {
                newest_first(&mut history);
                HiddenFile {
                    path: format!("/{bucket}/{key}"),
                    history,
                }
            })
            .collect()
    }
}

/// What a pane adds to the listing of `directory`: the hidden files directly in it, and the
/// folders its history knows that hold nothing now. A name the folder still lists stays what it
/// is, and a file with no version left is not shown, since nothing would come back.
pub fn deleted_entries(
    directory: &str,
    hidden: &[HiddenFile],
    folders: &[String],
    live: &[FileEntry],
) -> Vec<DeletedEntry> {
    let mut taken: HashSet<String> = live.iter().map(|entry| entry.name.clone()).collect();
    let prefix = match directory {
        "/" => "/".to_string(),
        directory => format!("{directory}/"),
    };
    let name_in_directory = |path: &str| {
        let name = path.strip_prefix(&prefix)?;
        protocol::validate_name(name).ok()?;
        Some(name.to_string())
    };
    let mut entries = Vec::new();
    for file in hidden {
        let (Some(name), Some(newest)) = (name_in_directory(&file.path), file.newest_version())
        else {
            continue;
        };
        if taken.insert(name.clone()) {
            entries.push(DeletedEntry {
                path: remote_path::join(directory, &name),
                name,
                is_dir: false,
                deleted: file.deleted(),
                size: newest.size,
            });
        }
    }
    for folder in folders {
        let Some(name) = name_in_directory(folder.trim_end_matches('/')) else {
            continue;
        };
        if taken.insert(name.clone()) {
            entries.push(DeletedEntry {
                path: remote_path::join(directory, &name),
                name,
                is_dir: true,
                deleted: None,
                size: 0,
            });
        }
    }
    entries
}

/// Lists a folder once and adds what was deleted from it.
pub async fn list_with_deleted(
    files: &dyn RemoteFileSystem,
    path: &str,
) -> AppResult<ListingWithDeleted> {
    let versioned = of(files)?;
    let listing = files.list_dir(path).await?;
    let deleted = versioned
        .list_deleted(&listing.path, &listing.entries)
        .await?;
    Ok(ListingWithDeleted { listing, deleted })
}

/// A file system that reads one file as an older version of itself and is the server's own for
/// everything else, so a download, the editor and a comparison read history the way they read
/// any file.
pub struct PinnedVersion {
    files: Arc<dyn RemoteFileSystem>,
    path: String,
    version_id: String,
}

impl PinnedVersion {
    pub fn over(
        files: Arc<dyn RemoteFileSystem>,
        path: &str,
        version_id: &str,
    ) -> AppResult<Arc<dyn RemoteFileSystem>> {
        validate_version_id(version_id)?;
        of(files.as_ref())?;
        Ok(Arc::new(Self {
            path: protocol::normalize(&files.resolve(path)),
            files,
            version_id: version_id.to_string(),
        }))
    }

    fn is_pinned(&self, path: &str) -> bool {
        protocol::normalize(&self.files.resolve(path)) == self.path
    }
}

#[async_trait]
impl RemoteFileSystem for PinnedVersion {
    fn protocol(&self) -> Protocol {
        self.files.protocol()
    }

    fn transport(&self) -> Protocol {
        self.files.transport()
    }

    fn connection_identity(&self) -> usize {
        self.files.connection_identity()
    }

    fn home(&self) -> &str {
        self.files.home()
    }

    fn capabilities(&self) -> Capabilities {
        self.files.capabilities()
    }

    fn resolve(&self, path: &str) -> String {
        self.files.resolve(path)
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        self.files.canonicalize(path).await
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        self.files.list_dir(path).await
    }

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        self.files.make_dir(parent, name).await
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        self.files.rename(path, new_name).await
    }

    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        self.files.delete(paths).await
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        if self.is_pinned(path) {
            let versioned = of(self.files.as_ref())?;
            return versioned
                .stat_version(&self.path, &self.version_id)
                .await
                .map(Some);
        }
        self.files.stat(path).await
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        self.files.ensure_dir(path).await
    }

    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        permissions: Option<u32>,
    ) -> AppResult<()> {
        self.files.set_attributes(path, modified, permissions).await
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        if self.is_pinned(path) {
            let versioned = of(self.files.as_ref())?;
            return versioned
                .open_version(&self.path, &self.version_id, range)
                .await;
        }
        self.files.clone().open_read(path, range).await
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        self.files.clone().open_write(request).await
    }

    fn as_ftp(&self) -> Option<&crate::ftp::FtpFs> {
        self.files.as_ftp()
    }

    fn as_versioned(&self) -> Option<&dyn ObjectVersions> {
        self.files.as_versioned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntryKind;

    fn entry(id: &str, kind: VersionKind, latest: bool, modified: i64, size: u64) -> ObjectVersion {
        ObjectVersion {
            version_id: id.to_string(),
            kind,
            latest,
            modified: Some(modified),
            size,
            etag: None,
            storage_class: None,
        }
    }

    fn version(key: &str, id: &str, latest: bool, modified: i64, size: u64) -> HistoryRecord {
        HistoryRecord {
            key: key.to_string(),
            version: entry(id, VersionKind::Version, latest, modified, size),
        }
    }

    fn marker(key: &str, id: &str, latest: bool, modified: i64) -> HistoryRecord {
        HistoryRecord {
            key: key.to_string(),
            version: entry(id, VersionKind::DeleteMarker, latest, modified, 0),
        }
    }

    fn live(name: &str, kind: EntryKind) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            path: format!("/photos/2024/{name}"),
            kind,
            link_target: None,
            size: 1,
            modified: None,
            permissions: None,
            owner: None,
            group: None,
            hidden: false,
        }
    }

    fn ids(history: &[ObjectVersion]) -> Vec<&str> {
        history
            .iter()
            .map(|entry| entry.version_id.as_str())
            .collect()
    }

    #[test]
    fn orders_history_newest_first() {
        let mut history = vec![
            entry("old", VersionKind::Version, false, 100, 1),
            entry("same-second-newer", VersionKind::Version, false, 200, 1),
            entry("same-second-older", VersionKind::Version, false, 200, 1),
            entry("marker", VersionKind::DeleteMarker, true, 200, 0),
            entry("null", VersionKind::Version, false, 50, 1),
        ];
        newest_first(&mut history);
        assert_eq!(
            ids(&history),
            [
                "marker",
                "same-second-newer",
                "same-second-older",
                "old",
                "null"
            ]
        );
    }

    #[test]
    fn finds_hidden_files_across_pages() {
        let mut hidden = HiddenFiles::default();
        // The order Amazon lists in: by key, newest first.
        hidden.add_page(vec![
            version("2024/kept.txt", "k2", true, 300, 9),
            version("2024/kept.txt", "k1", false, 100, 8),
            marker("2024/lost.txt", "m2", true, 400),
            version("2024/lost.txt", "v2", false, 300, 7),
        ]);
        // The file's older entries arrive on the next page, without its newest one.
        hidden.add_page(vec![
            version("2024/lost.txt", "v1", false, 200, 6),
            version("2024/other.txt", "o1", true, 100, 5),
        ]);
        let files = hidden.finish("photos");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "/photos/2024/lost.txt");
        assert_eq!(ids(&files[0].history), ["m2", "v2", "v1"]);
        assert_eq!(files[0].deleted(), Some(400));
        assert_eq!(files[0].newest_version().unwrap().size, 7);
        assert_eq!(
            files[0].marker_references(),
            [VersionRef {
                path: "/photos/2024/lost.txt".into(),
                version_id: "m2".into()
            }]
        );
        assert_eq!(files[0].every_reference().len(), 3);
    }

    #[test]
    fn finds_hidden_files_when_markers_follow_the_versions() {
        let mut hidden = HiddenFiles::default();
        // Some services list a page's versions before its delete markers.
        hidden.add_page(vec![
            version("a.txt", "a1", false, 100, 1),
            version("b.txt", "b1", true, 100, 2),
            marker("a.txt", "a-older-marker", false, 150),
            marker("a.txt", "a-marker", true, 200),
        ]);
        let files = hidden.finish("bucket");
        assert_eq!(files.len(), 1);
        assert_eq!(ids(&files[0].history), ["a-marker", "a-older-marker", "a1"]);
        // Both markers sit over the version, so both go when the file comes back.
        assert_eq!(files[0].marker_references().len(), 2);
    }

    #[test]
    fn leaves_out_keys_no_path_reaches() {
        assert!(reachable("a/b.txt"));
        assert!(reachable("a/folder/"));
        assert!(!reachable("a//b.txt"));
        assert!(!reachable("a/../b.txt"));
        assert!(!reachable("./b.txt"));
        assert!(!reachable("a//"));
        assert!(!reachable(""));
        let mut hidden = HiddenFiles::default();
        hidden.add_page(vec![
            marker("a//b.txt", "m", true, 2),
            version("a//b.txt", "null", false, 1, 1),
        ]);
        assert!(hidden.finish("bucket").is_empty());
    }

    #[test]
    fn groups_deleted_entries_of_a_folder() {
        let file = |name: &str, history: Vec<ObjectVersion>| HiddenFile {
            path: format!("/photos/2024/{name}"),
            history,
        };
        let hidden = vec![
            file(
                "lost.txt",
                vec![
                    entry("m", VersionKind::DeleteMarker, true, 400, 0),
                    entry("v2", VersionKind::Version, false, 300, 7),
                    entry("v1", VersionKind::Version, false, 200, 6),
                ],
            ),
            // Only the marker is left: nothing would come back.
            file(
                "purged.txt",
                vec![entry("m", VersionKind::DeleteMarker, true, 400, 0)],
            ),
            // A folder of that name is there now.
            file(
                "reports",
                vec![
                    entry("m", VersionKind::DeleteMarker, true, 400, 0),
                    entry("v", VersionKind::Version, false, 300, 1),
                ],
            ),
            // Deeper than the folder, and a name no path can hold.
            file(
                "sub/deep.txt",
                vec![
                    entry("m", VersionKind::DeleteMarker, true, 400, 0),
                    entry("v", VersionKind::Version, false, 300, 1),
                ],
            ),
            file(
                "..",
                vec![
                    entry("m", VersionKind::DeleteMarker, true, 400, 0),
                    entry("v", VersionKind::Version, false, 300, 1),
                ],
            ),
        ];
        let folders = vec![
            "/photos/2024/reports/".to_string(),
            "/photos/2024/gone/".to_string(),
            "/photos/2024/lost.txt/".to_string(),
            "/photos/elsewhere/".to_string(),
        ];
        let live = vec![
            live("reports", EntryKind::Dir),
            live("kept.txt", EntryKind::File),
        ];
        let deleted = deleted_entries("/photos/2024", &hidden, &folders, &live);
        assert_eq!(
            deleted,
            [
                DeletedEntry {
                    name: "lost.txt".into(),
                    path: "/photos/2024/lost.txt".into(),
                    is_dir: false,
                    deleted: Some(400),
                    size: 7,
                },
                DeletedEntry {
                    name: "gone".into(),
                    path: "/photos/2024/gone".into(),
                    is_dir: true,
                    deleted: None,
                    size: 0,
                },
            ]
        );
    }

    #[test]
    fn groups_deleted_entries_at_the_top_of_a_bucket() {
        let hidden = vec![HiddenFile {
            path: "/photos/a.txt".into(),
            history: vec![
                entry("m", VersionKind::DeleteMarker, true, 2, 0),
                entry("null", VersionKind::Version, false, 1, 3),
            ],
        }];
        let deleted = deleted_entries("/photos", &hidden, &["/photos/old/".to_string()], &[]);
        assert_eq!(deleted.len(), 2);
        assert_eq!(deleted[0].path, "/photos/a.txt");
        assert_eq!(deleted[1].path, "/photos/old");
    }

    #[test]
    fn checks_version_ids_from_the_page() {
        assert!(validate_version_id("null").is_ok());
        assert!(validate_version_id(
            "3HL4kqtJlcpXroDTDmJ+rmSpXd3dIbrHY+MTRCxf3vjVBH40Nr8X8gdRQBpUMLUo"
        )
        .is_ok());
        assert!(validate_version_id("").is_err());
        assert!(validate_version_id("a\nb").is_err());
        assert!(validate_version_id(&"x".repeat(MAX_VERSION_ID_BYTES + 1)).is_err());
    }
}
