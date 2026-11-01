//! Files dragged from a pane out of Poros: onto the desktop, into Windows Explorer, GNOME Files,
//! Dolphin or any other program that takes files. At the window's edge the page hands the drag
//! to the system's own drag and drop. Files on this computer go as they are. Files on a server
//! are downloaded through the transfer queue, ahead of everything else, into a staging folder:
//! Explorer reads them from there once they are dropped, other file managers are given them
//! once they are in, and Dolphin and the KDE desktop say where they were dropped, so Poros
//! downloads them straight there instead.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, WebviewWindow};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::transfer::{
    Direction, EnqueueRequest, FolderProgress, TransferItem, TransferManager, TransferWatch,
};

/// Mirrored in `src/lib/ipc.ts`. Tells every window whether files are being dragged out of
/// Poros, so none of them takes the drag back as files dropped in from outside.
pub const DRAG_OUT_EVENT: &str = "poros://drag-out";
/// Downloads still running for a drop are checked this often while something waits for them.
const STAGING_POLL: Duration = Duration::from_millis(100);
/// A Poros window can report a drop on it a moment after the drag that carried it has ended.
const LANDING_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragOutRequest {
    /// The server the files are on; `None` for files on this computer.
    pub session_id: Option<String>,
    pub items: Vec<TransferItem>,
}

/// A point in a window's page, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PagePoint {
    pub x: f64,
    pub y: f64,
}

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DragOutOutcome {
    /// Another program took the files.
    Dropped,
    Cancelled,
    /// Let go over the window the drag came from. `at` is where, for the page to drop the files
    /// itself; `None` when the page was told of the drop directly.
    Returned {
        at: Option<PagePoint>,
        copy: bool,
    },
}

/// The system's drag and drop failed.
pub(crate) fn drag_failed(reason: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorKind::Io,
        format!("Could not drag the files out of Poros: {reason}"),
    )
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct DragOutState {
    active: bool,
}

/// What a drag carries out of Poros.
pub(crate) enum Dragged {
    Local(Vec<PathBuf>),
    Remote(Arc<Staging>),
}

/// Files of a server downloading into a folder of their own for a drop.
pub(crate) struct Staging {
    pub folder: PathBuf,
    pub session_id: String,
    pub items: Vec<TransferItem>,
    transfers: TransferWatch,
}

impl Staging {
    pub fn progress(&self) -> FolderProgress {
        self.transfers.downloads_into(&self.folder)
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn is_finished(&self) -> bool {
        self.progress().unfinished == 0
    }

    /// Blocks the calling thread, which must not be the one the windows run on, until the
    /// downloads end.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn wait_blocking(&self) -> FolderProgress {
        loop {
            let progress = self.progress();
            if progress.unfinished == 0 {
                return progress;
            }
            std::thread::sleep(STAGING_POLL);
        }
    }

    /// The dragged files and folders that arrived in the staging folder.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn arrived(&self) -> Vec<PathBuf> {
        self.items
            .iter()
            .map(|item| self.folder.join(&item.name))
            .filter(|path| path.exists())
            .collect()
    }

    /// Stops the downloads and deletes what arrived.
    pub fn discard(&self) {
        self.transfers.discard_downloads_into(&self.folder);
        let folder = self.folder.clone();
        // A running download may still be closing its file, which Windows will not delete.
        tauri::async_runtime::spawn(async move {
            for _ in 0..10 {
                match tokio::fs::remove_dir_all(&folder).await {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        tokio::time::sleep(Duration::from_millis(500)).await
                    }
                    _ => return,
                }
            }
        });
    }
}

/// A file or folder in a staging folder, for programs that take each file's contents in turn.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct StagedEntry {
    /// From the staging folder, such as `photos/2024/beach.jpg`.
    pub relative: PathBuf,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Every file and folder under the dragged items, each folder before what it holds. Temporary
/// files of downloads that did not finish are left out.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn staged_entries(folder: &Path, names: &[String]) -> Vec<StagedEntry> {
    let mut entries = Vec::new();
    for name in names {
        collect_entries(folder, Path::new(name), &mut entries);
    }
    entries
}

fn collect_entries(folder: &Path, relative: &Path, entries: &mut Vec<StagedEntry>) {
    let path = folder.join(relative);
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return;
    };
    if metadata.is_dir() {
        entries.push(StagedEntry {
            relative: relative.to_path_buf(),
            path: path.clone(),
            is_dir: true,
            size: 0,
            modified: metadata.modified().ok(),
        });
        let Ok(children) = std::fs::read_dir(&path) else {
            return;
        };
        let mut names: Vec<_> = children
            .filter_map(Result::ok)
            .map(|child| child.file_name())
            .filter(|name| !crate::transfer::is_partial_name(&name.to_string_lossy()))
            .collect();
        names.sort();
        for name in names {
            collect_entries(folder, &relative.join(name), entries);
        }
    } else if metadata.is_file() {
        entries.push(StagedEntry {
            relative: relative.to_path_buf(),
            path,
            is_dir: false,
            size: metadata.len(),
            modified: metadata.modified().ok(),
        });
    }
}

/// Clears this flag when the drag it was set for ends, however it ends.
struct RunningGuard<'a>(&'a AtomicBool);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The staging folder of the last drag from a server, until the next one.
struct Landing {
    staging: Arc<Staging>,
    ended: Option<Instant>,
}

pub struct DragOut {
    staging_root: PathBuf,
    next_id: AtomicU64,
    /// The system has one pointer, so one drag runs at a time.
    running: AtomicBool,
    landing: Mutex<Option<Landing>>,
    #[cfg(target_os = "linux")]
    kde: linux::KdeDrops,
}

impl DragOut {
    /// Downloads for drops go into folders under `staging_root`. What an earlier run left
    /// there is deleted.
    pub fn new(staging_root: PathBuf) -> Self {
        let leftovers = staging_root.clone();
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(leftovers);
        });
        Self {
            staging_root,
            next_id: AtomicU64::new(1),
            running: AtomicBool::new(false),
            landing: Mutex::new(None),
            #[cfg(target_os = "linux")]
            kde: linux::KdeDrops::default(),
        }
    }

    pub fn staging_root(&self) -> &Path {
        &self.staging_root
    }

    /// Runs the system's drag with the files until they are dropped or the drag is cancelled.
    pub async fn run(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        transfers: &TransferManager,
        events: &Events,
        request: DragOutRequest,
    ) -> AppResult<DragOutOutcome> {
        if request.items.is_empty() {
            return Err(AppError::invalid("Nothing to drag"));
        }
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(AppError::invalid(
                "Files are already being dragged out of Poros",
            ));
        }
        let _running = RunningGuard(&self.running);
        let dragged = match request.session_id {
            None => Dragged::Local(
                request
                    .items
                    .iter()
                    .map(|item| PathBuf::from(&item.path))
                    .collect(),
            ),
            Some(session_id) => {
                let staging = self
                    .stage(transfers, events, session_id, request.items)
                    .await?;
                Dragged::Remote(staging)
            }
        };
        if let Dragged::Remote(staging) = &dragged {
            *self.landing.lock().unwrap() = Some(Landing {
                staging: staging.clone(),
                ended: None,
            });
        }
        let _ = app.emit(DRAG_OUT_EVENT, DragOutState { active: true });
        let outcome = self.drag(app, window, &dragged).await;
        let _ = app.emit(DRAG_OUT_EVENT, DragOutState { active: false });
        if let Some(landing) = self.landing.lock().unwrap().as_mut() {
            landing.ended = Some(Instant::now());
        }
        if let Dragged::Remote(staging) = &dragged {
            match &outcome {
                Ok(DragOutOutcome::Dropped) => {}
                _ => staging.discard(),
            }
        }
        outcome
    }

    /// Files dropped on a Poros window. When they are the ones a drag just carried out of
    /// Poros, the page drops them itself, so their downloads stop.
    pub fn dropped_on_poros(&self, paths: &[PathBuf]) {
        let mut landing = self.landing.lock().unwrap();
        let Some(current) = landing.as_ref() else {
            return;
        };
        let recent = current
            .ended
            .is_none_or(|ended| ended.elapsed() < LANDING_WINDOW);
        let carried = paths
            .iter()
            .all(|path| path.starts_with(&current.staging.folder));
        if recent && carried {
            if let Some(landed) = landing.take() {
                landed.staging.discard();
            }
        }
    }

    async fn stage(
        &self,
        transfers: &TransferManager,
        events: &Events,
        session_id: String,
        items: Vec<TransferItem>,
    ) -> AppResult<Arc<Staging>> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let folder = self.staging_root.join(format!("drag-{id}"));
        tokio::fs::create_dir_all(&folder).await?;
        let request = EnqueueRequest {
            session_id: session_id.clone(),
            source_session_id: None,
            direction: Direction::Download,
            target_directory: folder.to_string_lossy().into_owned(),
            items: items.clone(),
        };
        if let Err(error) = transfers.enqueue_first(request).await {
            let _ = tokio::fs::remove_dir_all(&folder).await;
            return Err(error);
        }
        let staging = Arc::new(Staging {
            folder,
            session_id,
            items,
            transfers: transfers.watch(),
        });
        if staging.progress().queue_paused {
            events.log(
                LogLevel::Warn,
                Some(&staging.session_id),
                "The transfer queue is paused: files dragged out of Poros arrive once it runs again",
            );
        }
        Ok(staging)
    }

    #[cfg(windows)]
    async fn drag(
        &self,
        _app: &AppHandle,
        window: &WebviewWindow,
        dragged: &Dragged,
    ) -> AppResult<DragOutOutcome> {
        windows::drag(window, dragged).await
    }

    #[cfg(target_os = "linux")]
    async fn drag(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        dragged: &Dragged,
    ) -> AppResult<DragOutOutcome> {
        linux::drag(app, window, dragged, &self.kde).await
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    async fn drag(
        &self,
        _app: &AppHandle,
        _window: &WebviewWindow,
        _dragged: &Dragged,
    ) -> AppResult<DragOutOutcome> {
        Err(AppError::invalid(
            "Dragging files out of Poros works on Windows and Linux",
        ))
    }
}

/// A `file://` URI for an absolute path, as file managers take them.
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn file_uri(path: &Path) -> String {
    use percent_encoding::{percent_encode, AsciiSet, NON_ALPHANUMERIC};
    use std::os::unix::ffi::OsStrExt;
    // RFC 3986 unreserved characters and the path separator stay as they are.
    const KEPT: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'/')
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    format!(
        "file://{}",
        percent_encode(path.as_os_str().as_bytes(), KEPT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_entries_list_folders_before_their_files_and_skip_partials() {
        let root = tempfile::tempdir().unwrap();
        let photos = root.path().join("photos");
        std::fs::create_dir_all(photos.join("2024")).unwrap();
        std::fs::write(photos.join("2024").join("beach.jpg"), b"sand").unwrap();
        std::fs::write(photos.join("a.txt"), b"one").unwrap();
        std::fs::write(photos.join(".b.txt.poros-part"), b"half").unwrap();
        std::fs::write(root.path().join("notes.md"), b"hello").unwrap();

        let entries = staged_entries(
            root.path(),
            &["photos".into(), "notes.md".into(), "missing".into()],
        );
        let listed: Vec<(PathBuf, bool, u64)> = entries
            .iter()
            .map(|entry| (entry.relative.clone(), entry.is_dir, entry.size))
            .collect();
        assert_eq!(
            listed,
            vec![
                (PathBuf::from("photos"), true, 0),
                (PathBuf::from("photos/2024"), true, 0),
                (PathBuf::from("photos/2024/beach.jpg"), false, 4),
                (PathBuf::from("photos/a.txt"), false, 3),
                (PathBuf::from("notes.md"), false, 5),
            ]
        );
        assert_eq!(entries[2].path, photos.join("2024").join("beach.jpg"));
    }

    #[cfg(unix)]
    #[test]
    fn file_uris_encode_what_is_not_plain() {
        assert_eq!(
            file_uri(Path::new(
                "/home/me/.cache/poros/drag-1/report 2024 (final)#1.pdf"
            )),
            "file:///home/me/.cache/poros/drag-1/report%202024%20%28final%29%231.pdf"
        );
        assert_eq!(
            file_uri(Path::new("/tmp/caf\u{e9}.txt")),
            "file:///tmp/caf%C3%A9.txt"
        );
    }
}
