pub mod commands;
pub mod error;
pub mod events;
pub mod local;
pub mod model;
pub mod remote_path;
pub mod session;
pub mod sftp;
pub mod ssh;

use tauri::{Manager, RunEvent};

use events::Events;
use session::SessionManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            let events = Events::new(app.handle().clone());
            app.manage(SessionManager::new(
                config_dir.join("known_hosts"),
                events,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::local_home,
            commands::local_roots,
            commands::local_list,
            commands::local_mkdir,
            commands::local_rename,
            commands::local_delete,
            commands::connect,
            commands::disconnect,
            commands::remote_list,
            commands::remote_mkdir,
            commands::remote_rename,
            commands::remote_delete,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Poros");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            let sessions = handle.state::<SessionManager>();
            tauri::async_runtime::block_on(sessions.disconnect_all());
        }
    });
}
