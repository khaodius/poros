//! Installing an update the release server offers. The page finds it with the updater plugin's
//! check and hands it over by its resource id; downloading, installing and restarting happen
//! here, so an update left for later on Windows can still install as Poros quits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, ResourceId, Webview};
use tauri_plugin_updater::Update;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::transfer::TransferManager;

/// How often the download reports how far it has got.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum UpdateProgress {
    /// `total` is null when the server does not say how large the download is.
    Downloading {
        received: u64,
        total: Option<u64>,
    },
    Installing,
}

/// On Windows, installing hands Poros over to the installer, which closes it. So a downloaded
/// and verified update waits here until the user restarts or quits.
#[derive(Default)]
pub struct PendingUpdate {
    downloaded: Mutex<Option<(Update, Vec<u8>)>>,
    /// Set when the user quits, as opposed to Windows ending Poros as it shuts down.
    quitting: AtomicBool,
}

impl PendingUpdate {
    fn take(&self) -> Option<(Update, Vec<u8>)> {
        self.downloaded.lock().unwrap().take()
    }

    /// Called when Poros is asked to exit for any reason other than restarting.
    pub fn note_quit(&self) {
        self.quitting.store(true, Ordering::Relaxed);
    }

    /// Called as Poros exits. An update left for later installs now, without starting Poros
    /// again, if the user quit.
    pub fn install_on_exit(&self) {
        if !self.quitting.load(Ordering::Relaxed) {
            return;
        }
        if let Some((update, bytes)) = self.take() {
            if let Err(error) = update.restart_after_install(false).install(bytes) {
                log::warn!("Could not install the update: {error}");
            }
        }
    }
}

/// Downloads the update and checks its signature against the key built into this version. On
/// Windows it then waits for [`restart`] or the exit; elsewhere it installs over this version at
/// once, which keeps running until it restarts.
pub async fn download(
    webview: &Webview,
    update_id: ResourceId,
    progress: Channel<UpdateProgress>,
    pending: &PendingUpdate,
) -> AppResult<()> {
    let update = webview
        .resources_table()
        .get::<Update>(update_id)
        .map_err(|_| AppError::new(ErrorKind::Update, "The update is no longer available"))?;
    let update = (*update).clone();

    let mut received = 0u64;
    let mut reported_at: Option<Instant> = None;
    let bytes = update
        .download(
            |chunk_length, total| {
                received += chunk_length as u64;
                if reported_at.is_some_and(|at| at.elapsed() < PROGRESS_INTERVAL) {
                    return;
                }
                reported_at = Some(Instant::now());
                let _ = progress.send(UpdateProgress::Downloading { received, total });
            },
            || (),
        )
        .await
        .map_err(update_error)?;

    if cfg!(windows) {
        *pending.downloaded.lock().unwrap() = Some((update, bytes));
        return Ok(());
    }
    let _ = progress.send(UpdateProgress::Installing);
    // Installing a .deb or .rpm waits for the system to ask for a password.
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|error| AppError::new(ErrorKind::Update, error.to_string()))?
        .map_err(update_error)
}

/// Starts the new version. On Windows the installer closes Poros, installs the update and starts
/// it; elsewhere the update is in place already and Poros restarts through the normal exit.
pub fn restart(
    app: &AppHandle,
    pending: &PendingUpdate,
    transfers: &TransferManager,
) -> AppResult<()> {
    if let Some((update, bytes)) = pending.take() {
        // The installer ends Poros without the normal exit, which would save the queue.
        transfers.save_queue();
        update.install(bytes).map_err(update_error)?;
    }
    app.request_restart();
    Ok(())
}

fn update_error(error: tauri_plugin_updater::Error) -> AppError {
    AppError::new(ErrorKind::Update, error.to_string())
}
