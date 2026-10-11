//! Installing an update the release server offers, or another release the user picks. The page
//! finds a newer one with the updater plugin's check, or a picked one with [`find_release`], and
//! hands it over by its resource id; downloading, installing and restarting happen here, so an
//! update left for later on Windows can still install as Poros quits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::header::ACCEPT;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::utils::config::BundleType;
use tauri::utils::platform::bundle_type;
use tauri::{AppHandle, Manager, ResourceId, Url, Webview};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::transfer::TransferManager;

/// How often the download reports how far it has got.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

const RELEASES_URL: &str = "https://api.github.com/repos/khaodius/poros/releases?per_page=100";

/// The release a user can go back to at the earliest: the latest one when picking a release
/// arrived. Older releases are never offered.
const OLDEST_INSTALLABLE: Version = Version::new(0, 4, 5);

/// What the updater reads to install a release; every release since 0.3.0 publishes one.
const MANIFEST_NAME: &str = "latest.json";

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

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
}

/// The releases this install can switch to, newest first, as plain versions such as `0.4.5`.
pub async fn installable_versions(app: &AppHandle) -> AppResult<Vec<String>> {
    let releases: Vec<Release> = crate::cloud::http_client()?
        .get(RELEASES_URL)
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(crate::cloud::transport_error)?
        .json()
        .await
        .map_err(crate::cloud::transport_error)?;
    // rpm refuses to install an older version over a newer one.
    let older_allowed = bundle_type() != Some(BundleType::Rpm);
    Ok(installable(
        releases,
        &app.package_info().version,
        older_allowed,
    ))
}

fn installable(releases: Vec<Release>, running: &Version, older_allowed: bool) -> Vec<String> {
    let mut versions: Vec<Version> = releases
        .into_iter()
        .filter(|release| {
            !release.prerelease
                && release
                    .assets
                    .iter()
                    .any(|asset| asset.name == MANIFEST_NAME)
        })
        .filter_map(|release| Version::parse(release.tag_name.strip_prefix('v')?).ok())
        .filter(|version| {
            *version >= OLDEST_INSTALLABLE
                && version != running
                && (older_allowed || version > running)
        })
        .collect();
    versions.sort_unstable_by(|first, second| second.cmp(first));
    versions.iter().map(Version::to_string).collect()
}

/// What the updater plugin's own check returns, so the page can treat a picked release like an
/// update the check found.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseMetadata {
    rid: ResourceId,
    current_version: String,
    version: String,
    body: Option<String>,
    raw_json: serde_json::Value,
}

/// Reads the manifest of one release, older or newer than this version, so it installs the way
/// an update does. Its signature still has to match the key built into this version.
pub async fn find_release(webview: &Webview, version: &str) -> AppResult<ReleaseMetadata> {
    let version = Version::parse(version)
        .ok()
        .filter(|version| *version >= OLDEST_INSTALLABLE)
        .ok_or_else(|| {
            AppError::new(
                ErrorKind::InvalidInput,
                format!("Poros {version} is not offered here"),
            )
        })?;
    let manifest = Url::parse(&format!(
        "https://github.com/khaodius/poros/releases/download/v{version}/{MANIFEST_NAME}"
    ))
    .expect("a release manifest's address is valid");
    let update = webview
        .updater_builder()
        .endpoints(vec![manifest])
        .and_then(|builder| {
            builder
                .version_comparator(|running, release| release.version != running)
                .build()
        })
        .map_err(update_error)?
        .check()
        .await
        .map_err(update_error)?
        .ok_or_else(|| {
            AppError::new(
                ErrorKind::Update,
                format!("Poros {version} is already running"),
            )
        })?;
    Ok(ReleaseMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    })
}

fn update_error(error: tauri_plugin_updater::Error) -> AppError {
    AppError::new(ErrorKind::Update, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.to_string(),
            prerelease,
            assets: assets
                .iter()
                .map(|name| ReleaseAsset {
                    name: name.to_string(),
                })
                .collect(),
        }
    }

    fn releases() -> Vec<Release> {
        vec![
            release("v0.4.5", false, &["Poros.AppImage", MANIFEST_NAME]),
            release("v0.5.0-beta.1", true, &[MANIFEST_NAME]),
            release("v0.4.8", false, &[MANIFEST_NAME]),
            release("v0.4.7", false, &["Poros.AppImage"]),
            release("v0.4.6", false, &[MANIFEST_NAME]),
            release("v0.4.4", false, &[MANIFEST_NAME]),
            release("nightly", false, &[MANIFEST_NAME]),
        ]
    }

    #[test]
    fn lists_released_versions_from_the_oldest_installable_on() {
        let running = Version::new(0, 4, 6);
        assert_eq!(installable(releases(), &running, true), ["0.4.8", "0.4.5"]);
    }

    #[test]
    fn lists_only_newer_versions_when_older_ones_cannot_install() {
        let running = Version::new(0, 4, 6);
        assert_eq!(installable(releases(), &running, false), ["0.4.8"]);
    }
}
