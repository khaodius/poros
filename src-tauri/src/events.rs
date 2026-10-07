use serde::Serialize;
use tauri::{AppHandle, Emitter};

// Mirrored in `src/lib/ipc.ts`.
pub const LOG_EVENT: &str = "poros://log";
pub const SESSION_CLOSED_EVENT: &str = "poros://session-closed";
pub const TRANSFERS_EVENT: &str = "poros://transfers";
/// Tells every window to reload settings, saved connections or themes another window changed.
pub const STORE_CHANGED_EVENT: &str = "poros://store-changed";
pub const SYNC_PROGRESS_EVENT: &str = "poros://sync-progress";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Store {
    Settings,
    Connections,
    Themes,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Server,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    /// Milliseconds since the Unix epoch.
    pub timestamp: u64,
    pub level: LogLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionClosed {
    pub session_id: String,
    pub reason: String,
}

#[derive(Clone, Default)]
pub struct Events {
    app: Option<AppHandle>,
}

impl Events {
    pub fn new(app: AppHandle) -> Self {
        Self { app: Some(app) }
    }

    pub fn log(&self, level: LogLevel, session_id: Option<&str>, message: impl Into<String>) {
        let message = message.into();
        match level {
            LogLevel::Error => log::error!("{message}"),
            LogLevel::Warn => log::warn!("{message}"),
            _ => log::info!("{message}"),
        }
        if let Some(app) = &self.app {
            let record = LogRecord {
                timestamp: now_millis(),
                level,
                session_id: session_id.map(str::to_owned),
                message,
            };
            let _ = app.emit(LOG_EVENT, record);
        }
    }

    pub fn transfers(&self, update: &crate::transfer::TransferUpdate) {
        if let Some(app) = &self.app {
            let _ = app.emit(TRANSFERS_EVENT, update);
        }
    }

    pub fn sync_progress(&self, progress: &crate::sync::SyncProgress) {
        if let Some(app) = &self.app {
            let _ = app.emit(SYNC_PROGRESS_EVENT, progress);
        }
    }

    pub fn store_changed(&self, store: Store) {
        if let Some(app) = &self.app {
            let _ = app.emit(STORE_CHANGED_EVENT, store);
        }
    }

    pub fn session_closed(&self, session_id: &str, reason: impl Into<String>) {
        if let Some(app) = &self.app {
            let _ = app.emit(
                SESSION_CLOSED_EVENT,
                SessionClosed {
                    session_id: session_id.to_owned(),
                    reason: reason.into(),
                },
            );
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}
