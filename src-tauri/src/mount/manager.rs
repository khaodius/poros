//! The drives mounted in one run of Poros, each tied to the session it shows.
//!
//! A drive follows its session through a reconnect and goes when the session is closed, its
//! vault locked, or Poros quits. Unmounting on request is refused while programs have files
//! open on the drive or uploads are pending, unless forced; a forced unmount, like every other
//! end, keeps the files that are not on the server yet on this computer, with a note of where
//! they belong, and they are listed until they are saved elsewhere or discarded.
//!
//! What a program wrote waits in a spool folder of its drive, under the app's cache folder,
//! named after the process and the drive so a later run can tell the files of drives that are
//! still mounted, here or by another process, from those left behind.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::backend::Backend;
use super::platform::{self, Missing, SystemMount};
use super::session_backend::SessionBackend;
use super::spool::{self, UnsentFile};
use super::upload;
use super::vfs::{FsOptions, MountFs};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::protocol::{self, Protocol};
use crate::session::SessionManager;

/// The checks the transfer settings ask for: whether uploads are verified, and read back
/// where the server cannot hash them.
pub type UploadChecks = Arc<dyn Fn() -> (bool, bool) + Send + Sync>;

/// The folder under the app's cache folder that holds every drive's spool folder.
pub const SPOOL_FOLDER: &str = "drives";
/// The folder under the home folder that new drives go in by default.
const DEFAULT_PARENT: &str = "Poros";
const MAX_FOLDER_NAME: usize = 80;
/// Names shown for open files and uploads in a message, before "and N more".
const NAMED_IN_MESSAGES: usize = 5;
/// How long the changes a drive makes are gathered before the app is told of them, so a
/// program writing many files sends one message rather than one for each.
const CHANGES_GATHERED: Duration = Duration::from_millis(250);

/// What the dialog asks for. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountRequest {
    pub session_id: String,
    /// The folder the drive is shown at (a drive letter on Windows).
    pub mount_point: String,
    /// The folder on the server the drive shows.
    pub remote_root: String,
    pub read_only: bool,
}

/// Whether this computer can mount drives. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountSupport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<Missing>,
    pub uses_drive_letters: bool,
}

/// A file a drive has not finished sending. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveUpload {
    /// Below the drive's root.
    pub path: String,
    pub size: u64,
    pub sent: u64,
    pub uploading: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A mounted drive as the app shows it. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveView {
    pub id: String,
    pub session_id: String,
    pub label: String,
    pub protocol: Protocol,
    pub mount_point: String,
    pub remote_root: String,
    pub read_only: bool,
    pub bytes_read: u64,
    pub bytes_written: u64,
    /// Files programs have open, below the drive's root.
    pub open_files: Vec<String>,
    pub uploads: Vec<DriveUpload>,
}

/// Folders a drive changed on the server, told to the app so the listings it remembers of
/// them are read again. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveChanges {
    pub session_id: String,
    pub folders: Vec<ChangedFolder>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFolder {
    pub folder: String,
    /// Paths in the folder whose own contents changed too: folders made, moved or removed,
    /// and files removed.
    pub entries: Vec<String>,
}

/// Changes gathered since the app was last told.
#[derive(Default)]
struct GatheredChanges {
    folders: BTreeMap<String, BTreeSet<String>>,
    sending: bool,
}

struct Drive {
    id: String,
    label: String,
    protocol: Protocol,
    /// For a vault's drive, the session of the server the vault is on.
    server_session_id: Mutex<Option<String>>,
    mount_point: PathBuf,
    remote_root: String,
    read_only: bool,
    core: Arc<MountFs>,
    backend: Arc<SessionBackend>,
    system: SystemMount,
}

impl Drive {
    fn below_root(&self, path: &str) -> String {
        below(&self.remote_root, path)
    }

    fn view(&self) -> DriveView {
        let activity = self.core.activity();
        DriveView {
            id: self.id.clone(),
            session_id: self.backend.session_id(),
            label: self.label.clone(),
            protocol: self.protocol,
            mount_point: self.mount_point.to_string_lossy().into_owned(),
            remote_root: self.remote_root.clone(),
            read_only: self.read_only,
            bytes_read: activity.bytes_read,
            bytes_written: activity.bytes_written,
            open_files: self
                .core
                .open_paths()
                .iter()
                .map(|path| self.below_root(path))
                .collect(),
            uploads: self
                .core
                .pending_uploads()
                .into_iter()
                .map(|pending| DriveUpload {
                    path: self.below_root(&pending.path),
                    size: pending.size,
                    sent: pending.sent,
                    uploading: pending.uploading,
                    error: pending.error,
                })
                .collect(),
        }
    }

    fn shows(&self, session_id: &str) -> bool {
        self.backend.session_id() == session_id
            || self.server_session_id.lock().unwrap().as_deref() == Some(session_id)
    }
}

/// A remote path below a drive's root, without a leading slash.
fn below(root: &str, path: &str) -> String {
    let relative = if root == "/" {
        path
    } else {
        path.strip_prefix(root).unwrap_or(path)
    };
    relative.trim_start_matches('/').to_string()
}

/// "a, b and 3 more".
fn named(paths: &[String]) -> String {
    let shown: Vec<&str> = paths
        .iter()
        .take(NAMED_IN_MESSAGES)
        .map(String::as_str)
        .collect();
    let more = paths.len().saturating_sub(NAMED_IN_MESSAGES);
    if more > 0 {
        format!("{} and {more} more", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

pub struct MountManager {
    sessions: Arc<SessionManager>,
    events: Events,
    spool_root: PathBuf,
    upload_checks: UploadChecks,
    drives: Mutex<BTreeMap<String, Arc<Drive>>>,
    this: Weak<MountManager>,
}

impl MountManager {
    pub fn new(
        sessions: Arc<SessionManager>,
        events: Events,
        spool_root: PathBuf,
        upload_checks: UploadChecks,
    ) -> Arc<Self> {
        Arc::new_cyclic(|this| Self {
            sessions,
            events,
            spool_root,
            upload_checks,
            drives: Mutex::new(BTreeMap::new()),
            this: this.clone(),
        })
    }

    /// Where a drive of this process keeps what programs wrote until it is uploaded.
    pub fn spool_folder(&self, drive_id: &str) -> PathBuf {
        self.spool_root
            .join(format!("{}-{drive_id}", std::process::id()))
    }

    pub fn support() -> MountSupport {
        MountSupport {
            missing: platform::missing(),
            uses_drive_letters: platform::USES_DRIVE_LETTERS,
        }
    }

    /// Detaches drives an earlier run left behind and clears the spool files nobody can use,
    /// on a thread of its own, since a drive that hangs could hold up whoever asks it.
    pub fn tidy_up_in_background(&self) {
        let spool_root = self.spool_root.clone();
        let events = self.events.clone();
        let spawned = std::thread::Builder::new()
            .name("poros-drives-tidy".into())
            .spawn(move || {
                for left in platform::clear_dead_drives() {
                    events.log(
                        LogLevel::Info,
                        None,
                        format!(
                            "Removed a drive an earlier run of Poros left at {}",
                            left.display()
                        ),
                    );
                }
                for (folder, owner) in spool_folders(&spool_root) {
                    if owner.is_none_or(|pid| !process_alive(pid)) {
                        spool::remove_orphans(&folder);
                        // Only goes if nothing is left in it.
                        let _ = std::fs::remove_dir(&folder);
                    }
                }
            });
        if let Err(error) = spawned {
            log::warn!("Could not check for drives left behind: {error}");
        }
    }

    pub fn list(&self) -> Vec<DriveView> {
        self.drives
            .lock()
            .unwrap()
            .values()
            .map(|drive| drive.view())
            .collect()
    }

    fn drive(&self, id: &str) -> AppResult<Arc<Drive>> {
        self.drives
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "That drive is not mounted"))
    }

    /// A folder for a new drive named `name`: under `~/Poros`, numbered if one of that name
    /// is in use.
    pub fn default_mount_point(&self, name: &str) -> AppResult<String> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "No home folder to mount in"))?;
        let parent = home.join(DEFAULT_PARENT);
        let used: HashSet<PathBuf> = self
            .drives
            .lock()
            .unwrap()
            .values()
            .map(|drive| drive.mount_point.clone())
            .collect();
        let base = folder_name(name);
        let free = (1..100)
            .map(|number| {
                if number == 1 {
                    parent.join(&base)
                } else {
                    parent.join(format!("{base} {number}"))
                }
            })
            .find(|candidate| !used.contains(candidate) && is_free_folder(candidate))
            .ok_or_else(|| AppError::invalid("Choose a folder to mount the drive at"))?;
        Ok(free.to_string_lossy().into_owned())
    }

    pub async fn mount(&self, request: MountRequest) -> AppResult<DriveView> {
        if let Some(missing) = platform::missing() {
            return Err(AppError::unsupported(format!(
                "{} {}",
                missing.problem, missing.help
            )));
        }
        let session = self.sessions.get(&request.session_id).await?;
        let files = session.files();
        let root = protocol::normalize(&files.resolve(&request.remote_root));
        if root != "/" {
            match files.stat(&root).await? {
                Some(found) if found.is_dir => {}
                Some(_) => {
                    return Err(AppError::invalid(format!(
                        "{root} is a file. A drive shows a folder."
                    )))
                }
                None => {
                    return Err(AppError::new(
                        ErrorKind::NotFound,
                        format!("{root} is not on the server"),
                    ))
                }
            }
        }
        let mount_point = self.check_mount_point(&request.mount_point)?;

        let id = uuid::Uuid::new_v4().to_string();
        let label = session.info().label.clone();
        let mut options = FsOptions::new(label.clone(), self.spool_folder(&id));
        options.read_only = request.read_only;
        let checks = self.upload_checks.clone();
        options.upload_check = Arc::new(move |files| {
            let (verify, read_back) = checks();
            upload::check_for(files, verify, read_back)
        });
        let backend = SessionBackend::new(self.sessions.clone(), &session);
        let core = MountFs::new(backend.clone(), &root, options);
        {
            let (events, backend, label) = (self.events.clone(), backend.clone(), label.clone());
            core.report_upload_failures(Arc::new(move |path: &str, reason: &str| {
                events.log(
                    LogLevel::Error,
                    Some(&backend.session_id()),
                    format!(
                        "{path} on the drive of {label} could not be uploaded: {reason}. \
                         Poros keeps it on this computer and lists it under the drive."
                    ),
                );
                events.mounts_changed();
            }));
        }
        core.report_changes(self.change_report(backend.clone()));
        let on_end: Box<dyn Fn() + Send> = {
            let (manager, id) = (self.this.clone(), id.clone());
            let runtime = tokio::runtime::Handle::current();
            Box::new(move || {
                let (manager, id) = (manager.clone(), id.clone());
                runtime.spawn(async move {
                    if let Some(manager) = manager.upgrade() {
                        manager.ended_elsewhere(&id).await;
                    }
                });
            })
        };
        let system = {
            let (core, mount_point, label) = (core.clone(), mount_point.clone(), label.clone());
            tokio::task::spawn_blocking(move || {
                SystemMount::start(core, &mount_point, &label, on_end)
            })
            .await??
        };
        let drive = Arc::new(Drive {
            id: id.clone(),
            label,
            protocol: session.protocol(),
            server_session_id: Mutex::new(session.server_session().map(str::to_string)),
            mount_point,
            remote_root: root,
            read_only: request.read_only,
            core,
            backend,
            system,
        });
        self.drives.lock().unwrap().insert(id, drive.clone());
        self.events.log(
            LogLevel::Info,
            Some(&request.session_id),
            format!(
                "Mounted {} of {} at {}{}",
                drive.remote_root,
                drive.label,
                drive.mount_point.display(),
                if drive.read_only { ", read-only" } else { "" }
            ),
        );
        self.events.mounts_changed();
        Ok(drive.view())
    }

    /// Gathers the changes a drive makes and tells the app of them a moment later, under the
    /// session the drive follows by then.
    fn change_report(&self, backend: Arc<SessionBackend>) -> super::vfs::ChangeReport {
        let gathered = Arc::new(Mutex::new(GatheredChanges::default()));
        let events = self.events.clone();
        let runtime = tokio::runtime::Handle::current();
        Arc::new(move |folder: &str, below: Option<&str>| {
            let mut changes = gathered.lock().unwrap();
            let entries = changes.folders.entry(folder.to_string()).or_default();
            if let Some(path) = below {
                entries.insert(path.to_string());
            }
            if std::mem::replace(&mut changes.sending, true) {
                return;
            }
            let (gathered, events, backend) = (gathered.clone(), events.clone(), backend.clone());
            runtime.spawn(async move {
                tokio::time::sleep(CHANGES_GATHERED).await;
                let folders = {
                    let mut changes = gathered.lock().unwrap();
                    changes.sending = false;
                    std::mem::take(&mut changes.folders)
                };
                events.drive_changed(DriveChanges {
                    session_id: backend.session_id(),
                    folders: folders
                        .into_iter()
                        .map(|(folder, entries)| ChangedFolder {
                            folder,
                            entries: entries.into_iter().collect(),
                        })
                        .collect(),
                });
            });
        })
    }

    /// The folder a drive is to be shown at, refused if another drive is there or inside it.
    fn check_mount_point(&self, requested: &str) -> AppResult<PathBuf> {
        let requested = requested.trim();
        let mount_point = match requested.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => dirs::home_dir()
                .ok_or_else(|| AppError::invalid("No home folder to mount in"))?
                .join(rest.trim_start_matches('/')),
            _ => PathBuf::from(requested),
        };
        if requested.is_empty() || !mount_point.is_absolute() {
            return Err(AppError::invalid(
                "The folder to mount at must be a full path",
            ));
        }
        let taken = self.drives.lock().unwrap().values().any(|drive| {
            drive.mount_point.starts_with(&mount_point)
                || mount_point.starts_with(&drive.mount_point)
        });
        if taken {
            return Err(AppError::invalid(format!(
                "{} is taken by another drive",
                mount_point.display()
            )));
        }
        Ok(mount_point)
    }

    /// Unmounts a drive. Without `force` it is refused while programs have files open on it
    /// or uploads are pending, saying which; with `force` it goes at once and keeps what was
    /// not uploaded.
    pub async fn unmount(&self, id: &str, force: bool) -> AppResult<()> {
        let drive = self.drive(id)?;
        if !force {
            let open = drive.core.open_paths();
            if !open.is_empty() {
                let open: Vec<String> = open.iter().map(|path| drive.below_root(path)).collect();
                return Err(AppError::new(
                    ErrorKind::Io,
                    format!(
                        "Programs have files open on the drive: {}. Close them first, or \
                         unmount by force.",
                        named(&open)
                    ),
                ));
            }
            let pending: Vec<String> = drive
                .core
                .pending_uploads()
                .iter()
                .map(|upload| drive.below_root(&upload.path))
                .collect();
            if !pending.is_empty() {
                return Err(AppError::new(
                    ErrorKind::Io,
                    format!(
                        "Not on the server yet: {}. Wait for the upload, or unmount by force \
                         and Poros keeps the files on this computer.",
                        named(&pending)
                    ),
                ));
            }
        }
        let kept = if force {
            drive.core.keep_unsent()
        } else {
            Vec::new()
        };
        let detaching = drive.clone();
        tokio::task::spawn_blocking(move || detaching.system.unmount(force)).await??;
        self.finish(&drive, &kept, "Unmounted").await;
        Ok(())
    }

    async fn finish(&self, drive: &Arc<Drive>, kept: &[String], how: &str) {
        self.drives.lock().unwrap().remove(&drive.id);
        drive.backend.close().await;
        let session_id = drive.backend.session_id();
        self.events.log(
            LogLevel::Info,
            Some(&session_id),
            format!(
                "{how} the drive of {} at {}",
                drive.label,
                drive.mount_point.display()
            ),
        );
        if !kept.is_empty() {
            let kept: Vec<String> = kept.iter().map(|path| drive.below_root(path)).collect();
            self.events.log(
                LogLevel::Warn,
                Some(&session_id),
                format!(
                    "Not uploaded before the drive went, and kept on this computer: {}",
                    named(&kept)
                ),
            );
        }
        self.events.mounts_changed();
    }

    /// The drive was detached by something other than Poros, such as the file manager.
    async fn ended_elsewhere(&self, id: &str) {
        let Ok(drive) = self.drive(id) else {
            return;
        };
        let kept = drive.core.keep_unsent();
        let detaching = drive.clone();
        let _ = tokio::task::spawn_blocking(move || detaching.system.unmount(true)).await;
        self.finish(&drive, &kept, "Something outside Poros unmounted")
            .await;
    }

    /// Unmounts, by force, every drive showing a session or a vault on it: the session is
    /// about to close.
    pub async fn end_for_session(&self, session_id: &str) {
        let showing: Vec<String> = self
            .drives
            .lock()
            .unwrap()
            .values()
            .filter(|drive| drive.shows(session_id))
            .map(|drive| drive.id.clone())
            .collect();
        for id in showing {
            if let Err(error) = self.unmount(&id, true).await {
                log::warn!("Could not unmount a drive: {}", error.message);
            }
        }
    }

    /// Moves drives onto the session that replaced `old` after a reconnect, and sends again
    /// what could not be sent while it was away.
    pub fn follow(&self, old: &str, new: &str) {
        let drives: Vec<Arc<Drive>> = self.drives.lock().unwrap().values().cloned().collect();
        let mut moved = false;
        for drive in drives {
            if drive.backend.session_id() == old {
                drive.backend.follow(new);
            } else {
                let mut server = drive.server_session_id.lock().unwrap();
                if server.as_deref() != Some(old) {
                    continue;
                }
                *server = Some(new.to_string());
                drop(server);
                // The vault keeps its session; its connections were the old server's.
                drive.backend.follow(&drive.backend.session_id());
            }
            drive.core.retry_unsent();
            moved = true;
        }
        if moved {
            self.events.mounts_changed();
        }
    }

    /// Tries again the uploads of a drive that failed.
    pub fn retry(&self, id: &str) -> AppResult<()> {
        self.drive(id)?.core.retry_unsent();
        Ok(())
    }

    pub fn mount_point(&self, id: &str) -> AppResult<PathBuf> {
        Ok(self.drive(id)?.mount_point.clone())
    }

    /// Unmounts every drive, by force: Poros is quitting.
    pub async fn unmount_all(&self) {
        let ids: Vec<String> = self.drives.lock().unwrap().keys().cloned().collect();
        for id in ids {
            if let Err(error) = self.unmount(&id, true).await {
                log::warn!("Could not unmount a drive: {}", error.message);
            }
        }
    }

    /// Files kept from drives that are gone, newest first.
    pub fn unsent(&self) -> Vec<UnsentFile> {
        let live: HashSet<String> = self.drives.lock().unwrap().keys().cloned().collect();
        let own = std::process::id();
        let mut found: Vec<UnsentFile> = spool_folders(&self.spool_root)
            .into_iter()
            .filter(|(folder, owner)| match owner {
                Some(pid) if *pid == own => !folder
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.split_once('-'))
                    .is_some_and(|(_, drive)| live.contains(drive)),
                Some(pid) => !process_alive(*pid),
                None => true,
            })
            .flat_map(|(folder, _)| spool::unsent_files(&folder))
            .collect();
        found.sort_by_key(|file| std::cmp::Reverse(file.note.kept_at));
        found
    }

    fn kept_file(&self, path: &str) -> AppResult<PathBuf> {
        let path = PathBuf::from(path);
        let listed = self
            .unsent()
            .iter()
            .any(|file| Path::new(&file.path) == path);
        if !listed {
            return Err(AppError::new(
                ErrorKind::NotFound,
                "That file is no longer kept",
            ));
        }
        Ok(path)
    }

    /// Copies a kept file to `destination`, a path on this computer the user chose.
    pub fn save_unsent(&self, path: &str, destination: &str) -> AppResult<()> {
        let kept = self.kept_file(path)?;
        std::fs::copy(&kept, destination).map_err(|error| {
            AppError::new(
                ErrorKind::Io,
                format!("Could not save a copy at {destination}: {error}"),
            )
        })?;
        Ok(())
    }

    pub fn discard_unsent(&self, path: &str) -> AppResult<()> {
        let kept = self.kept_file(path)?;
        spool::discard_unsent(&self.spool_root, &kept)?;
        if let Some(folder) = kept.parent() {
            // Only goes if nothing is left in it.
            let _ = std::fs::remove_dir(folder);
        }
        Ok(())
    }
}

/// Every spool folder under `root`, with the process that made it where its name says.
fn spool_folders(root: &Path) -> Vec<(PathBuf, Option<u32>)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .map(|folder| {
            let owner = folder
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.split_once('-'))
                .and_then(|(pid, _)| pid.parse().ok());
            (folder, owner)
        })
        .collect()
}

/// Whether a process is running. Where that cannot be told, it counts as running, so its
/// files are left alone.
fn process_alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new(&format!("/proc/{pid}")).exists()
    } else {
        true
    }
}

/// A folder name for a connection: what file managers and command lines take without
/// quoting trouble. A colon, which `scp` and `rsync` read as a host, becomes a dash.
fn folder_name(name: &str) -> String {
    let name = name.split_once("://").map_or(name, |(_, rest)| rest);
    let mut cleaned = String::new();
    for character in name.chars() {
        let kept = if character.is_alphanumeric() || " ._-@()+,".contains(character) {
            character
        } else {
            '-'
        };
        if kept == '-' && cleaned.ends_with('-') {
            continue;
        }
        cleaned.push(kept);
        if cleaned.chars().count() >= MAX_FOLDER_NAME {
            break;
        }
    }
    let cleaned = cleaned.trim_matches(|character: char| {
        character == '-' || character.is_whitespace() || character == '.'
    });
    if cleaned.is_empty() {
        "Server".to_string()
    } else {
        cleaned.to_string()
    }
}

/// Missing, or an empty folder.
fn is_free_folder(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_drop_what_command_lines_trip_over() {
        assert_eq!(
            folder_name("poros@example.com:2222"),
            "poros@example.com-2222"
        );
        assert_eq!(
            folder_name("ftp://poros@127.0.0.1:2121"),
            "poros@127.0.0.1-2121"
        );
        assert_eq!(folder_name("Photos on poros@nas"), "Photos on poros@nas");
        assert_eq!(folder_name("S3 (AKIA1234)"), "S3 (AKIA1234)");
        assert_eq!(folder_name("a/b\\c*?<>|\"d"), "a-b-c-d");
        assert_eq!(folder_name("../.."), "Server");
        assert_eq!(folder_name("   "), "Server");
        assert_eq!(folder_name(&"x".repeat(300)).len(), MAX_FOLDER_NAME);
    }

    #[test]
    fn paths_are_shown_below_the_drive_root() {
        assert_eq!(below("/", "/docs/a.txt"), "docs/a.txt");
        assert_eq!(below("/srv/www", "/srv/www/index.html"), "index.html");
        assert_eq!(below("/srv/www", "/srv/www/css/site.css"), "css/site.css");
    }

    #[test]
    fn messages_name_a_few_files_and_count_the_rest() {
        let paths: Vec<String> = (1..=7).map(|number| format!("{number}.txt")).collect();
        assert_eq!(named(&paths[..2]), "1.txt, 2.txt");
        assert_eq!(
            named(&paths),
            "1.txt, 2.txt, 3.txt, 4.txt, 5.txt and 2 more"
        );
    }

    #[test]
    fn spool_folders_say_which_process_made_them() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("4242-abc-def")).unwrap();
        std::fs::create_dir(root.path().join("stray")).unwrap();
        std::fs::write(root.path().join("file.spool"), b"").unwrap();
        let mut found = spool_folders(root.path());
        found.sort();
        assert_eq!(
            found,
            vec![
                (root.path().join("4242-abc-def"), Some(4242)),
                (root.path().join("stray"), None),
            ]
        );
        assert!(process_alive(std::process::id()));
    }

    #[test]
    fn a_folder_is_free_when_missing_or_empty() {
        let root = tempfile::tempdir().unwrap();
        assert!(is_free_folder(&root.path().join("new")));
        assert!(is_free_folder(root.path()));
        std::fs::write(root.path().join("file"), b"x").unwrap();
        assert!(!is_free_folder(root.path()));
        assert!(!is_free_folder(&root.path().join("file")));
    }
}
