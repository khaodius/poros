use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::ipc::Channel;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::automation::remote_command::{CommandRequest, CommandResult, CommandRunner};
use crate::automation::scheduler::{ScheduledTask, Scheduler, TaskView};
use crate::automation::system::{self, PowerAction};
use crate::cloud::{self, CloudProvider, OAuthClient, OAuthVault, ProviderStatus, SignedIn};
use crate::connections::{ConnectionStore, SavedConnection, SecretScope};
use crate::desktop::{self, DragPreview, PreviewPlacement, WindowCorners};
use crate::editor::{
    DocumentInfo, EditorManager, FileLocation, SaveOutcome, SaveRequest, TextDocument,
};
use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel, LogRecord, Store};
use crate::file_ops::{
    CompareRequest, Comparison, Details, FileOperations, Location, MoveCopyRequest,
    OperationSummary, PermissionRequest, PermissionSummary, Usage,
};
use crate::fonts::{self, FontFamily};
use crate::local;
use crate::model::{DirListing, FileEntry};
use crate::route;
use crate::session::{SessionInfo, SessionManager};
use crate::settings::{Settings, SettingsStore};
use crate::ssh::{AuthMethod, ConnectProfile, HostKeyApproval};
use crate::sync::{SyncManager, SyncPlanView, SyncRequest, SyncRunRequest, SyncRunSummary};
use crate::terminal::{Sink, TerminalEvent, TerminalInfo, TerminalManager};
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
    settings: State<'_, SettingsStore>,
    events: State<'_, Events>,
    mut profile: ConnectProfile,
    host_key_approval: Option<HostKeyApproval>,
) -> AppResult<SessionInfo> {
    prepare_cloud_sign_in(&window, &mut profile)?;
    if let Some(saved_id) = profile.saved_connection_id.clone() {
        if profile.lacks_secret() {
            // The profile may have been edited to point elsewhere since the secret was saved.
            let scope = SecretScope::of_profile(&profile);
            let store = connections.inner().clone();
            match tokio::task::spawn_blocking(move || store.secret_for(&saved_id, &scope)).await? {
                Ok(Some(secret)) => profile.set_secret(secret),
                Ok(None) => {}
                Err(error) => events.log(LogLevel::Warn, None, error.message),
            }
        }
    }
    let connection_settings = settings.get().connection;
    let store = connections.inner().clone();
    let profile = tokio::task::spawn_blocking(move || {
        route::resolve(&mut profile, &connection_settings, &store).map(|()| profile)
    })
    .await??;
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

/// Gives a cloud profile the app to sign in as and, after a fresh browser sign-in, its
/// account. A saved account comes from the keychain like a password.
fn prepare_cloud_sign_in(window: &WebviewWindow, profile: &mut ConnectProfile) -> AppResult<()> {
    let Some(provider) = CloudProvider::for_protocol(profile.protocol) else {
        return Ok(());
    };
    let AuthMethod::OAuth {
        grant_id,
        refresh_token,
        client,
    } = &mut profile.auth
    else {
        return Ok(());
    };
    if let Some(grant_id) = grant_id {
        *refresh_token = window
            .state::<OAuthVault>()
            .refresh_token(grant_id, provider)?;
    }
    *client = OAuthClient::configured(provider, &window.state::<SettingsStore>().get().cloud);
    Ok(())
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
    sessions
        .get(&session_id)
        .await?
        .files()
        .list_dir(&path)
        .await
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
        .files()
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
        .files()
        .rename(&path, &new_name)
        .await
}

#[tauri::command]
pub async fn remote_delete(
    sessions: State<'_, Arc<SessionManager>>,
    session_id: String,
    paths: Vec<String>,
) -> AppResult<()> {
    sessions
        .get(&session_id)
        .await?
        .files()
        .delete(&paths)
        .await
}

#[tauri::command]
pub async fn files_conflicts(
    operations: State<'_, FileOperations>,
    location: Location,
    names: Vec<String>,
    directory: String,
) -> AppResult<Vec<String>> {
    operations.conflicts(location, names, directory).await
}

#[tauri::command]
pub async fn files_move_or_copy(
    operations: State<'_, FileOperations>,
    request: MoveCopyRequest,
) -> AppResult<OperationSummary> {
    operations.move_or_copy(request).await
}

#[tauri::command]
pub fn files_cancel(operations: State<'_, FileOperations>, operation_id: String) {
    operations.cancel(&operation_id);
}

#[tauri::command]
pub async fn files_details(
    operations: State<'_, FileOperations>,
    session_id: String,
    path: String,
) -> AppResult<Details> {
    operations.details(&session_id, &path).await
}

#[tauri::command]
pub async fn files_measure(
    operations: State<'_, FileOperations>,
    operation_id: String,
    session_id: String,
    paths: Vec<String>,
) -> AppResult<Usage> {
    operations.measure(&operation_id, &session_id, &paths).await
}

#[tauri::command]
pub async fn files_set_permissions(
    operations: State<'_, FileOperations>,
    request: PermissionRequest,
) -> AppResult<PermissionSummary> {
    operations.set_permissions(request).await
}

#[tauri::command]
pub async fn files_compare(
    operations: State<'_, FileOperations>,
    request: CompareRequest,
) -> AppResult<Comparison> {
    operations.compare(request).await
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

/// Writes the unfinished transfers to disk now. Used before an installer closes the app without
/// the normal exit.
#[tauri::command]
pub fn transfer_save_queue(transfers: State<'_, TransferManager>) {
    transfers.save_queue();
}

#[tauri::command]
pub async fn sync_compare(
    sync: State<'_, SyncManager>,
    request: SyncRequest,
) -> AppResult<SyncPlanView> {
    sync.compare(request).await
}

#[tauri::command]
pub fn sync_cancel(sync: State<'_, SyncManager>, request_id: String) {
    sync.cancel(&request_id);
}

#[tauri::command]
pub fn sync_discard(sync: State<'_, SyncManager>, plan_id: String) {
    sync.discard(&plan_id);
}

#[tauri::command]
pub async fn sync_run(
    sync: State<'_, SyncManager>,
    transfers: State<'_, TransferManager>,
    request: SyncRunRequest,
) -> AppResult<SyncRunSummary> {
    sync.run(&transfers, request).await
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

/// Keeps the proxy password in the system keychain; an empty one deletes it.
#[tauri::command]
pub async fn proxy_password_set(
    settings: State<'_, SettingsStore>,
    connections: State<'_, Arc<ConnectionStore>>,
    events: State<'_, Events>,
    password: String,
) -> AppResult<Settings> {
    let store = connections.inner().clone();
    let has_password = !password.is_empty();
    tokio::task::spawn_blocking(move || store.set_proxy_password(Some(&password))).await??;
    let saved = settings.set_proxy_has_password(has_password)?;
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

/// `oauth_grant` names a browser sign-in whose account the connection keeps.
#[tauri::command]
pub async fn connections_save(
    connections: State<'_, Arc<ConnectionStore>>,
    vault: State<'_, OAuthVault>,
    events: State<'_, Events>,
    connection: SavedConnection,
    secret: Option<String>,
    oauth_grant: Option<String>,
) -> AppResult<SavedConnection> {
    let secret = match oauth_grant {
        Some(grant_id) => {
            let provider = CloudProvider::for_protocol(connection.protocol)
                .ok_or_else(|| AppError::invalid("Only cloud storage connections sign in"))?;
            Some(vault.refresh_token(&grant_id, provider)?)
        }
        None => secret,
    };
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

/// Which cloud providers have an app to sign in with.
#[tauri::command]
pub fn cloud_providers(settings: State<'_, SettingsStore>) -> Vec<ProviderStatus> {
    cloud::oauth::provider_statuses(&settings.get().cloud)
}

/// Opens the provider's sign-in page in the browser and waits for the account to come back.
#[tauri::command]
pub async fn cloud_sign_in(
    settings: State<'_, SettingsStore>,
    vault: State<'_, OAuthVault>,
    request_id: String,
    provider: CloudProvider,
) -> AppResult<SignedIn> {
    let client = OAuthClient::configured(provider, &settings.get().cloud)
        .ok_or_else(|| cloud::missing_client(provider))?;
    let cancel = vault.begin(&request_id);
    let result = cloud::sign_in(&client, &cancel).await;
    vault.end(&request_id);
    let authorization = result?;
    Ok(SignedIn {
        grant_id: vault.add(provider, authorization.refresh_token),
        provider,
        account: authorization.account,
    })
}

#[tauri::command]
pub fn cloud_cancel_sign_in(vault: State<'_, OAuthVault>, request_id: String) {
    vault.cancel(&request_id);
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
pub async fn editor_open(
    window: WebviewWindow,
    editor: State<'_, EditorManager>,
    location: FileLocation,
) -> AppResult<DocumentInfo> {
    editor.open(location, window.label()).await
}

#[tauri::command]
pub async fn editor_load(
    window: WebviewWindow,
    editor: State<'_, EditorManager>,
    document_id: String,
) -> AppResult<TextDocument> {
    editor.load(&document_id, window.label()).await
}

#[tauri::command]
pub fn editor_adopt(
    window: WebviewWindow,
    editor: State<'_, EditorManager>,
    document_id: String,
) -> AppResult<()> {
    editor.adopt(&document_id, window.label())
}

#[tauri::command]
pub async fn editor_save(
    editor: State<'_, EditorManager>,
    request: SaveRequest,
) -> AppResult<SaveOutcome> {
    editor.save(request).await
}

#[tauri::command]
pub async fn editor_close(editor: State<'_, EditorManager>, document_id: String) -> AppResult<()> {
    editor.close(&document_id).await;
    Ok(())
}

fn terminal_sink(output: Channel<TerminalEvent>) -> Sink {
    Arc::new(move |event| {
        let _ = output.send(event);
    })
}

#[tauri::command]
pub async fn terminal_open(
    window: WebviewWindow,
    terminals: State<'_, TerminalManager>,
    session_id: String,
    columns: u32,
    rows: u32,
    output: Channel<TerminalEvent>,
) -> AppResult<TerminalInfo> {
    terminals
        .open(
            &session_id,
            columns,
            rows,
            terminal_sink(output),
            window.label(),
        )
        .await
}

#[tauri::command]
pub fn terminal_attach(
    window: WebviewWindow,
    terminals: State<'_, TerminalManager>,
    terminal_id: String,
    output: Channel<TerminalEvent>,
) -> AppResult<()> {
    terminals.attach(&terminal_id, terminal_sink(output), window.label())
}

#[tauri::command]
pub fn terminal_write(
    terminals: State<'_, TerminalManager>,
    terminal_id: String,
    data: String,
) -> AppResult<()> {
    terminals.write(&terminal_id, data.into_bytes())
}

#[tauri::command]
pub fn terminal_resize(
    terminals: State<'_, TerminalManager>,
    terminal_id: String,
    columns: u32,
    rows: u32,
) -> AppResult<()> {
    terminals.resize(&terminal_id, columns, rows)
}

#[tauri::command]
pub async fn terminal_restart(
    terminals: State<'_, TerminalManager>,
    terminal_id: String,
) -> AppResult<()> {
    terminals.restart(&terminal_id).await
}

#[tauri::command]
pub async fn terminal_close(
    terminals: State<'_, TerminalManager>,
    terminal_id: String,
) -> AppResult<()> {
    terminals.close(&terminal_id).await;
    Ok(())
}

#[tauri::command]
pub async fn save_text_file(path: String, contents: String) -> AppResult<()> {
    tokio::task::spawn_blocking(move || {
        std::fs::write(&path, contents).map_err(|error| AppError::from(error).with_path(path))
    })
    .await?
}

#[tauri::command]
pub async fn remote_command_run(
    commands: State<'_, CommandRunner>,
    request: CommandRequest,
) -> AppResult<CommandResult> {
    commands.run(request).await
}

#[tauri::command]
pub fn remote_command_stop(commands: State<'_, CommandRunner>, run_id: String) {
    commands.stop(&run_id);
}

#[tauri::command]
pub async fn power_action(action: PowerAction) -> AppResult<()> {
    tokio::task::spawn_blocking(move || system::perform(action)).await?
}

/// Runs the user's command through the system shell, with `environment` added.
#[tauri::command]
pub async fn local_command_run(
    events: State<'_, Events>,
    command: String,
    environment: HashMap<String, String>,
) -> AppResult<()> {
    system::run_local_command(&command, &environment, &events).await
}

#[tauri::command]
pub fn app_exit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn schedules_list(scheduler: State<'_, Scheduler>) -> Vec<TaskView> {
    scheduler.list()
}

#[tauri::command]
pub fn schedule_save(scheduler: State<'_, Scheduler>, task: ScheduledTask) -> AppResult<TaskView> {
    scheduler.save(task)
}

#[tauri::command]
pub fn schedule_delete(scheduler: State<'_, Scheduler>, id: String) -> AppResult<()> {
    scheduler.delete(&id)
}

#[tauri::command]
pub fn schedule_run_now(
    app: AppHandle,
    scheduler: State<'_, Scheduler>,
    id: String,
) -> AppResult<()> {
    scheduler.run_now(&id, app)
}

/// Layouts handed from a window to the window it tears a tab out into.
#[derive(Default)]
pub struct PendingWindows(Mutex<HashMap<String, serde_json::Value>>);

#[tauri::command]
pub async fn window_open(
    app: AppHandle,
    pending: State<'_, PendingWindows>,
    layout: serde_json::Value,
    x: Option<i32>,
    y: Option<i32>,
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
        .visible(false)
        .inner_size(width, height)
        .min_inner_size(480.0, 360.0);
    let corner = match (x, y) {
        (Some(x), Some(y)) if desktop::can_place_windows() => Some(PhysicalPosition::new(x, y)),
        _ => None,
    };
    builder = match corner {
        Some(corner) => {
            let opening = desktop::opening_corner(&app, corner);
            builder.position(opening.x, opening.y)
        }
        None => builder.center(),
    };
    let opened = builder.build().and_then(|window| match corner {
        Some(corner) => desktop::place(&window, corner, LogicalSize::new(width, height)),
        None => Ok(()),
    });
    if let Err(error) = opened {
        pending.0.lock().unwrap().remove(&label);
        return Err(AppError::invalid(format!(
            "Could not open a window: {error}"
        )));
    }
    Ok(label)
}

/// Rounds the calling window's corners to match the app's corner radius, where the system can.
#[tauri::command]
pub fn window_set_corners(window: WebviewWindow, corners: WindowCorners) -> AppResult<()> {
    desktop::set_corners(&window, corners).map_err(window_error)
}

#[tauri::command]
pub async fn drag_preview_place(
    app: AppHandle,
    preview: State<'_, DragPreview>,
    placement: PreviewPlacement,
) -> AppResult<()> {
    preview.place(&app, placement).map_err(window_error)
}

#[tauri::command]
pub fn drag_preview_available() -> bool {
    desktop::can_place_windows()
}

#[tauri::command]
pub async fn drag_preview_hide(app: AppHandle, preview: State<'_, DragPreview>) -> AppResult<()> {
    preview.hide(&app).map_err(window_error)
}

#[tauri::command]
pub async fn drag_preview_ready(
    preview: State<'_, DragPreview>,
) -> AppResult<Option<serde_json::Value>> {
    Ok(preview.ready())
}

#[tauri::command]
pub async fn drag_preview_reveal(
    window: WebviewWindow,
    preview: State<'_, DragPreview>,
) -> AppResult<()> {
    preview.reveal(&window).map_err(window_error)
}

fn window_error(error: tauri::Error) -> AppError {
    AppError::invalid(format!("Window error: {error}"))
}

#[tauri::command]
pub fn window_initial_layout(
    window: WebviewWindow,
    pending: State<'_, PendingWindows>,
) -> Option<serde_json::Value> {
    pending.0.lock().unwrap().remove(window.label())
}

/// What was logged before the main window listened. Called once it does.
#[tauri::command]
pub fn log_startup(events: State<'_, Events>) -> Vec<LogRecord> {
    events.take_startup_log()
}

/// Starts the app again after an update replaced it. Goes through the normal exit, so open
/// sessions are closed first.
#[tauri::command]
pub fn app_restart(app: AppHandle) {
    app.request_restart();
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}
