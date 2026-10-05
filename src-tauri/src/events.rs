//! Backend-to-frontend events. Event names and payloads are mirrored in `src/lib/events.ts`.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const LOG_EVENT: &str = "poros://log";
pub const SESSION_CLOSED_EVENT: &str = "poros://session-closed";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    /// Text the server sent (auth banners).
    Server,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    /// Milliseconds since the Unix epoch.
    pub ts: u64,
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

/// Emits connection log lines to the UI log panel and the process logger.
/// Without an `AppHandle` (unit tests) it only writes to the process logger.
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
                ts: now_millis(),
                level,
                session_id: session_id.map(str::to_owned),
                message,
            };
            let _ = app.emit(LOG_EVENT, record);
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
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
