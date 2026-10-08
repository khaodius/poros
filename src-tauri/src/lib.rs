pub mod commands;
pub mod connections;
pub mod error;
pub mod events;
pub mod fonts;
pub mod format;
pub mod local;
pub mod model;
pub mod remote_path;
pub mod rsync;
pub mod session;
pub mod settings;
pub mod sftp;
pub mod ssh;
pub mod storage;
pub mod sync;
pub mod themes;
pub mod transfer;

use std::sync::Arc;
use std::time::Duration;

use tauri::webview::PageLoadEvent;
use tauri::{Manager, RunEvent, WindowEvent};

use commands::PendingWindows;
use connections::{ConnectionStore, Keychain};
use events::Events;
use session::SessionManager;
use settings::SettingsStore;
use sync::SyncManager;
use themes::ThemeStore;
use transfer::TransferManager;

const MAIN_WINDOW: &str = "main";
/// Windows open hidden and their page shows them once its first frame is ready, so they never
/// flash white. A page that fails before then still gets its window shown after this long.
const REVEAL_FALLBACK: Duration = Duration::from_secs(2);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            let events = Events::new(app.handle().clone());
            let settings = SettingsStore::load(config_dir.join("settings.json"));
            let sessions = Arc::new(SessionManager::new(
                config_dir.join("known_hosts"),
                events.clone(),
            ));
            let transfers =
                TransferManager::new(sessions.clone(), events.clone(), settings.get().transfers);
            app.manage(transfers);
            app.manage(SyncManager::new(sessions.clone(), events.clone()));
            app.manage(sessions);
            app.manage(settings);
            app.manage(Arc::new(ConnectionStore::new(
                config_dir.join("connections.json"),
                Box::new(Keychain),
            )));
            app.manage(ThemeStore::new(config_dir.join("themes")));
            app.manage(PendingWindows::default());
            app.manage(events);
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let window = webview.window();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(REVEAL_FALLBACK).await;
                if !window.is_visible().unwrap_or(true) {
                    let _ = window.show();
                }
            });
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { .. } if window.label() == MAIN_WINDOW => {
                window.app_handle().exit(0);
            }
            WindowEvent::Destroyed => {
                let app = window.app_handle().clone();
                let label = window.label().to_string();
                tauri::async_runtime::spawn(async move {
                    let sessions = app.state::<Arc<SessionManager>>();
                    sessions.disconnect_owned_by(&label).await;
                });
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::local_home,
            commands::local_roots,
            commands::local_list,
            commands::local_stat,
            commands::local_mkdir,
            commands::local_rename,
            commands::local_delete,
            commands::connect,
            commands::reconnect,
            commands::adopt_session,
            commands::disconnect,
            commands::remote_list,
            commands::remote_mkdir,
            commands::remote_rename,
            commands::remote_delete,
            commands::transfer_enqueue,
            commands::transfer_list,
            commands::transfer_set_paused,
            commands::transfer_pause,
            commands::transfer_resume,
            commands::transfer_remove,
            commands::transfer_clear,
            commands::transfer_move,
            commands::transfer_resolve,
            commands::transfer_session_jobs,
            commands::transfer_pause_session,
            commands::sync_compare,
            commands::sync_cancel,
            commands::sync_discard,
            commands::sync_run,
            commands::settings_get,
            commands::settings_set,
            commands::connections_list,
            commands::connections_save,
            commands::connections_delete,
            commands::themes_list,
            commands::theme_save,
            commands::theme_delete,
            commands::theme_import,
            commands::themes_open_folder,
            commands::fonts_list,
            commands::save_text_file,
            commands::window_open,
            commands::window_initial_layout,
            commands::app_restart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Poros");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            let sessions = handle.state::<Arc<SessionManager>>();
            tauri::async_runtime::block_on(sessions.disconnect_all());
        }
    });
}
