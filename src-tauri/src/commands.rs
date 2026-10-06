use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::connections::{ConnectionStore, SavedConnection};
use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel, Store};
use crate::fonts::{self, FontFamily};
use crate::local;
use crate::model::{DirListing, FileEntry};
use crate::session::{SessionInfo, SessionManager};
use crate::settings::{Settings, SettingsStore};
use crate::ssh::{ConnectProfile, HostKeyApproval};
use crate::themes::{self, ThemeFile, ThemeStore};
use crate::transfer::{
    EnqueueRequest, ExistsAction, JobId, JobState, TransferList, TransferManager,
};

#[tauri::command]
pub async fn local_home() -> AppResult<String> {
    local::home_dir()
}

#[tauri::command]
pub async fn local_roots() -> AppResult<Vec<String>> {
    Ok(local::roots())
}

#[tauri::command]
pub async fn local_list(path: String) -> AppResult<DirListing> {
    tokio::task::spawn_blocking(move || local::list_dir(&path)).await?
}

#[tauri::command]
pub async fn local_stat(paths: Vec<String>) -> AppResult<Vec<FileEntry>> {
    Ok(tokio::task::spawn_blocking(move || local::stat_paths(&paths)).await?)
}

#[tauri::command]
pub async fn local_mkdir(parent: String, name: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || local::make_dir(&parent, &name)).await?
}

#[tauri::command]
pub async fn local_rename(path: String, new_name: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || local::rename(&path, &new_name)).await?
}

#[tauri::command]
pub async fn local_delete(paths: Vec<String>) -> AppResult<()> {
    tokio::task::spawn_blocking(move || local::delete(&paths)).await?
}

#[tauri::command]
pub async fn connect(
    window: WebviewWindow,
    sessions: State<'_, Arc<SessionManager>>,
    connections: State<'_, Arc<ConnectionStore>>,
    events: State<'_, Events>,
    mut profile: ConnectProfile,
    host_key_approval: Option<HostKeyApproval>,
) -> AppResult<SessionInfo> {
    if let Some(saved_id) = profile.saved_connection_id.clone() {
        if profile.lacks_secret() {
            let store = connections.inner().clone();
            match tokio::task::spawn_blocking(move || store.secret(&saved_id)).await? {
                Ok(Some(secret)) => profile.set_secret(secret),
                Ok(None) => {}
                Err(error) => events.log(LogLevel::Warn, None, error.message),
            }
        }
    }
    let info = sessions
        .connect(profile, host_key_approval, window.label())
        .await?;
    if let Some(saved_id) = info.saved_connection_id.clone() {
        let store = connections.inner().clone();
        let touched =
            tokio::task::spawn_blocking(move || store.touch(&saved_id, now_millis())).await?;
        if touched.is_ok() {
            events.store_changed(Store::Connections);
        }
    }
    Ok(info)
}

#[tauri::command]
pub async fn reconnect(
    window: WebviewWindow,
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
    host_key_approval: Option<HostKeyApproval>,
) -> AppResult<SessionInfo> {
    sessions
        .reconnect(&session_id, host_key_approval, window.label())
        .await
}

#[tauri::command]
pub async fn adopt_session(
    window: WebviewWindow,
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> AppResult<SessionInfo> {
    sessions.adopt(&session_id, window.label()).await
}

#[tauri::command]
pub async fn disconnect(
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> AppResult<()> {
    sessions.disconnect(&session_id).await
}

#[tauri::command]
pub async fn remote_list(
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
    path: String,
) -> AppResult<DirListing> {
    sessions.get(&session_id).await?.fs.list_dir(&path).await
}

#[tauri::command]
pub async fn remote_mkdir(
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
    parent: String,
    name: String,
) -> AppResult<String> {
    sessions
        .get(&session_id)
        .await?
        .fs
        .make_dir(&parent, &name)
        .await
}

#[tauri::command]
pub async fn remote_rename(
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
    path: String,
    new_name: String,
) -> AppResult<String> {
    sessions
        .get(&session_id)
        .await?
        .fs
        .rename(&path, &new_name)
        .await
}

#[tauri::command]
pub async fn remote_delete(
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
    paths: Vec<String>,
) -> AppResult<()> {
    sessions.get(&session_id).await?.fs.delete(&paths).await
}

#[tauri::command]
pub async fn transfer_enqueue(
    transfers: State<'_, TransferManager>,
    request: EnqueueRequest,
) -> AppResult<usize> {
    transfers.enqueue(request).await
}

#[tauri::command]
pub fn transfer_list(transfers: State<'_, TransferManager>) -> TransferList {
    transfers.list()
}

#[tauri::command]
pub fn transfer_set_paused(transfers: State<'_, TransferManager>, paused: bool) {
    transfers.set_paused(paused);
}

#[tauri::command]
pub fn transfer_pause(transfers: State<'_, TransferManager>, ids: Vec<JobId>) {
    transfers.pause(&ids);
}

#[tauri::command]
pub fn transfer_resume(transfers: State<'_, TransferManager>, ids: Vec<JobId>) {
    transfers.resume(&ids);
}

#[tauri::command]
pub fn transfer_remove(transfers: State<'_, TransferManager>, ids: Vec<JobId>) {
    transfers.remove(&ids);
}

#[tauri::command]
pub fn transfer_clear(transfers: State<'_, TransferManager>, states: Vec<JobState>) {
    transfers.clear(&states);
}

#[tauri::command]
pub fn transfer_move(transfers: State<'_, TransferManager>, ids: Vec<JobId>, to_top: bool) {
    transfers.move_to(&ids, to_top);
}

#[tauri::command]
pub fn transfer_resolve(
    transfers: State<'_, TransferManager>,
    id: JobId,
    action: ExistsAction,
    apply_to_all: bool,
) {
    transfers.resolve(id, action, apply_to_all);
}

#[tauri::command]
pub fn transfer_session_jobs(transfers: State<'_, TransferManager>, session_id: String) -> usize {
    transfers.unfinished_session_jobs(&session_id)
}

#[tauri::command]
pub fn transfer_pause_session(transfers: State<'_, TransferManager>, session_id: String) -> usize {
    transfers.pause_session(&session_id)
}

#[tauri::command]
pub fn settings_get(settings: State<'_, SettingsStore>) -> Settings {
    settings.get()
}

#[tauri::command]
pub fn settings_set(
    settings: State<'_, SettingsStore>,
    transfers: State<'_, TransferManager>,
    events: State<'_, Events>,
    value: Settings,
) -> AppResult<Settings> {
    let saved = settings.set(value)?;
    transfers.configure(saved.transfers.clone());
    events.store_changed(Store::Settings);
    Ok(saved)
}

#[tauri::command]
pub async fn connections_list(
    connections: State<'_, Arc<ConnectionStore>>,
) -> AppResult<Vec<SavedConnection>> {
    let store = connections.inner().clone();
    tokio::task::spawn_blocking(move || store.list()).await?
}

#[tauri::command]
pub async fn connections_save(
    connections: State<'_, Arc<ConnectionStore>>,
    events: State<'_, Events>,
    connection: SavedConnection,
    secret: Option<String>,
) -> AppResult<SavedConnection> {
    let store = connections.inner().clone();
    let saved = tokio::task::spawn_blocking(move || store.save(connection, secret)).await??;
    events.store_changed(Store::Connections);
    Ok(saved)
}

#[tauri::command]
pub async fn connections_delete(
    connections: State<'_, Arc<ConnectionStore>>,
    events: State<'_, Events>,
    id: String,
) -> AppResult<()> {
    let store = connections.inner().clone();
    tokio::task::spawn_blocking(move || store.delete(&id)).await??;
    events.store_changed(Store::Connections);
    Ok(())
}

#[tauri::command]
pub fn themes_list(themes: State<'_, ThemeStore>) -> AppResult<Vec<ThemeFile>> {
    themes.list()
}

#[tauri::command]
pub fn theme_save(
    themes: State<'_, ThemeStore>,
    events: State<'_, Events>,
    id: Option<String>,
    theme: serde_json::Value,
) -> AppResult<String> {
    let id = themes.save(id, theme)?;
    events.store_changed(Store::Themes);
    Ok(id)
}

#[tauri::command]
pub fn theme_delete(
    themes: State<'_, ThemeStore>,
    events: State<'_, Events>,
    id: String,
) -> AppResult<()> {
    themes.delete(&id)?;
    events.store_changed(Store::Themes);
    Ok(())
}

#[tauri::command]
pub fn theme_import(
    themes: State<'_, ThemeStore>,
    events: State<'_, Events>,
    path: String,
) -> AppResult<String> {
    let id = themes.import(&PathBuf::from(path))?;
    events.store_changed(Store::Themes);
    Ok(id)
}

#[tauri::command]
pub fn themes_open_folder(themes: State<'_, ThemeStore>) -> AppResult<()> {
    themes::open_folder(themes.dir())
}

#[tauri::command]
pub async fn fonts_list() -> AppResult<Vec<FontFamily>> {
    Ok(tokio::task::spawn_blocking(fonts::installed_families).await?)
}

#[tauri::command]
pub async fn save_text_file(path: String, contents: String) -> AppResult<()> {
    tokio::task::spawn_blocking(move || {
        std::fs::write(&path, contents).map_err(|error| AppError::from(error).with_path(path))
    })
    .await?
}

/// Layouts handed from a window to the window it tears a tab out into.
#[derive(Default)]
pub struct PendingWindows(Mutex<HashMap<String, serde_json::Value>>);

#[tauri::command]
pub async fn window_open(
    app: AppHandle,
    pending: State<'_, PendingWindows>,
    layout: serde_json::Value,
    x: Option<f64>,
    y: Option<f64>,
    width: f64,
    height: f64,
) -> AppResult<String> {
    let label = format!(
        "workspace-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    pending.0.lock().unwrap().insert(label.clone(), layout);
    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("Poros")
        .decorations(false)
        .inner_size(width, height)
        .min_inner_size(480.0, 360.0);
    builder = match (x, y) {
        (Some(x), Some(y)) => builder.position(x, y),
        _ => builder.center(),
    };
    if let Err(error) = builder.build() {
        pending.0.lock().unwrap().remove(&label);
        return Err(AppError::invalid(format!(
            "Could not open a window: {error}"
        )));
    }
    Ok(label)
}

#[tauri::command]
pub fn window_initial_layout(
    window: WebviewWindow,
    pending: State<'_, PendingWindows>,
) -> Option<serde_json::Value> {
    pending.0.lock().unwrap().remove(window.label())
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}
