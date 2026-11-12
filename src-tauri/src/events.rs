use std::sync::{Arc, Mutex};

use serde::Serialize;
#[cfg(feature = "gui")]
use tauri::{AppHandle, Emitter};

// Mirrored in `src/lib/ipc.ts`.
pub const LOG_EVENT: &str = "poros://log";
pub const SESSION_CLOSED_EVENT: &str = "poros://session-closed";
pub const TRANSFERS_EVENT: &str = "poros://transfers";
/// Tells every window to reload settings, saved connections or themes another window changed.
pub const STORE_CHANGED_EVENT: &str = "poros://store-changed";
pub const SYNC_PROGRESS_EVENT: &str = "poros://sync-progress";
pub const QUEUE_FINISHED_EVENT: &str = "poros://queue-finished";
pub const COMMAND_OUTPUT_EVENT: &str = "poros://command-output";
pub const FILE_OPERATION_EVENT: &str = "poros://file-operation";
/// Records kept for the main window while it loads; later ones past this are dropped.
const MAX_STARTUP_RECORDS: usize = 500;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Store {
    Settings,
    Connections,
    Themes,
    Schedules,
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

/// Without the app, as in the headless command line, records go only to the `log` crate.
#[derive(Clone, Default)]
pub struct Events {
    #[cfg(feature = "gui")]
    app: Option<AppHandle>,
    /// What was logged before the main window listened, such as a settings file that could
    /// not be read at start-up. Events sent then would reach no one, so the records wait here
    /// until the window takes them, and later ones go out as events.
    startup_log: Arc<Mutex<Option<Vec<LogRecord>>>>,
}

impl Events {
    #[cfg(feature = "gui")]
    pub fn new(app: AppHandle) -> Self {
        Self {
            app: Some(app),
            startup_log: Arc::new(Mutex::new(Some(Vec::new()))),
        }
    }

    /// Keeps every record for `take_startup_log`, as the app's own events do at start-up, so
    /// tests can read the log.
    pub fn keeping_startup_log() -> Self {
        Self {
            #[cfg(feature = "gui")]
            app: None,
            startup_log: Arc::new(Mutex::new(Some(Vec::new()))),
        }
    }

    pub fn log(&self, level: LogLevel, session_id: Option<&str>, message: impl Into<String>) {
        let message = message.into();
        match level {
            LogLevel::Error => log::error!("{message}"),
            LogLevel::Warn => log::warn!("{message}"),
            _ => log::info!("{message}"),
        }
        let record = LogRecord {
            timestamp: now_millis(),
            level,
            session_id: session_id.map(str::to_owned),
            message,
        };
        if let Some(kept) = self.startup_log.lock().unwrap().as_mut() {
            if kept.len() < MAX_STARTUP_RECORDS {
                kept.push(record);
            }
            return;
        }
        self.emit(LOG_EVENT, record);
    }

    /// Hands over the records logged before the main window listened; from then on, records
    /// go out as events.
    pub fn take_startup_log(&self) -> Vec<LogRecord> {
        self.startup_log.lock().unwrap().take().unwrap_or_default()
    }

    pub fn transfers(&self, update: &crate::transfer::TransferUpdate) {
        self.emit(TRANSFERS_EVENT, update);
    }

    pub fn sync_progress(&self, progress: &crate::sync::SyncProgress) {
        self.emit(SYNC_PROGRESS_EVENT, progress);
    }

    pub fn queue_finished(&self, finished: &crate::transfer::QueueFinished) {
        self.emit(QUEUE_FINISHED_EVENT, finished);
    }

    pub fn command_output(&self, chunk: &crate::automation::remote_command::OutputChunk) {
        self.emit(COMMAND_OUTPUT_EVENT, chunk);
    }

    pub fn file_operation(&self, progress: &crate::file_ops::OperationProgress) {
        self.emit(FILE_OPERATION_EVENT, progress);
    }

    pub fn store_changed(&self, store: Store) {
        self.emit(STORE_CHANGED_EVENT, store);
    }

    pub fn session_closed(&self, session_id: &str, reason: impl Into<String>) {
        self.emit(
            SESSION_CLOSED_EVENT,
            SessionClosed {
                session_id: session_id.to_owned(),
                reason: reason.into(),
            },
        );
    }

    #[cfg(feature = "gui")]
    fn emit(&self, event: &str, payload: impl Serialize + Clone) {
        if let Some(app) = &self.app {
            let _ = app.emit(event, payload);
        }
    }

    #[cfg(not(feature = "gui"))]
    fn emit(&self, _event: &str, _payload: impl Serialize + Clone) {}
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_from_before_the_window_listened_are_handed_over_once() {
        let events = Events::keeping_startup_log();
        events.log(LogLevel::Warn, None, "settings.json is not valid");
        events.log(LogLevel::Info, Some("session"), "Connected");
        let kept = events.take_startup_log();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].message, "settings.json is not valid");
        assert!(matches!(kept[0].level, LogLevel::Warn));
        assert_eq!(kept[1].session_id.as_deref(), Some("session"));

        events.log(LogLevel::Info, None, "Later lines go out as events");
        assert!(events.take_startup_log().is_empty());
    }
}
