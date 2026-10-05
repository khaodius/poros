use tauri::State;

use crate::error::AppResult;
use crate::local;
use crate::model::DirListing;
use crate::session::{SessionInfo, SessionManager};
use crate::ssh::{ConnectProfile, HostKeyApproval};

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
    sessions: State<'_, SessionManager>,
    profile: ConnectProfile,
    host_key_approval: Option<HostKeyApproval>,
) -> AppResult<SessionInfo> {
    sessions.connect(profile, host_key_approval).await
}

#[tauri::command]
pub async fn disconnect(sessions: State<'_, SessionManager>, session_id: String) -> AppResult<()> {
    sessions.disconnect(&session_id).await
}

#[tauri::command]
pub async fn remote_list(
    sessions: State<'_, SessionManager>,
    session_id: String,
    path: String,
) -> AppResult<DirListing> {
    sessions.get(&session_id).await?.fs.list_dir(&path).await
}

#[tauri::command]
pub async fn remote_mkdir(
    sessions: State<'_, SessionManager>,
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
    sessions: State<'_, SessionManager>,
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
    sessions: State<'_, SessionManager>,
    session_id: String,
    paths: Vec<String>,
) -> AppResult<()> {
    sessions.get(&session_id).await?.fs.delete(&paths).await
}
