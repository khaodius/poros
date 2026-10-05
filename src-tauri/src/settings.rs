//! Persisted in `settings.json` in the app config folder. The backend reads the transfer and
//! connection sections; the interface, appearance and log sections belong to the frontend and
//! are stored as given. Mirrored in `src/lib/settings.ts`.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage;
use crate::transfer::ExistsAction;

pub const MAX_WORKERS: u32 = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TransferSettings {
    /// Transfers that run at once, each on its own connection.
    pub workers: u32,
    pub auto_start: bool,
    /// Lets idle workers join a large file, each moving a different part of it.
    pub segmented: bool,
    pub segment_threshold_mib: u32,
    pub max_segments: u32,
    /// Bytes per SFTP read or write request.
    pub request_size_kib: u32,
    /// Requests kept outstanding per connection, hiding network latency.
    pub requests_in_flight: u32,
    /// Zero means unlimited.
    pub upload_limit_kib: u32,
    pub download_limit_kib: u32,
    pub exists_action: ExistsAction,
    pub preserve_timestamps: bool,
    pub preserve_permissions: bool,
    pub retry_attempts: u32,
    pub retry_delay_secs: u32,
    pub keep_completed: bool,
    pub separate_connections: bool,
    pub log_each_file: bool,
}

impl Default for TransferSettings {
    fn default() -> Self {
        Self {
            workers: 4,
            auto_start: true,
            segmented: true,
            segment_threshold_mib: 32,
            max_segments: 4,
            request_size_kib: 32,
            requests_in_flight: 64,
            upload_limit_kib: 0,
            download_limit_kib: 0,
            exists_action: ExistsAction::Ask,
            preserve_timestamps: true,
            preserve_permissions: false,
            retry_attempts: 3,
            retry_delay_secs: 5,
            keep_completed: true,
            separate_connections: true,
            log_each_file: false,
        }
    }
}

impl TransferSettings {
    fn sanitize(&mut self) {
        self.workers = self.workers.clamp(1, MAX_WORKERS);
        self.segment_threshold_mib = self.segment_threshold_mib.clamp(1, 1024 * 1024);
        self.max_segments = self.max_segments.clamp(2, MAX_WORKERS);
        self.request_size_kib = self.request_size_kib.clamp(4, 255);
        self.requests_in_flight = self.requests_in_flight.clamp(1, 256);
        self.retry_attempts = self.retry_attempts.min(20);
        self.retry_delay_secs = self.retry_delay_secs.min(600);
    }

    pub fn request_size(&self) -> u32 {
        self.request_size_kib * 1024
    }

    pub fn segment_threshold(&self) -> u64 {
        u64::from(self.segment_threshold_mib) * 1024 * 1024
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConnectionSettings {
    pub timeout_secs: u64,
    pub keepalive_secs: u64,
    pub compression: bool,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            timeout_secs: crate::ssh::DEFAULT_TIMEOUT_SECS,
            keepalive_secs: crate::ssh::DEFAULT_KEEPALIVE_SECS,
            compression: false,
        }
    }
}

impl ConnectionSettings {
    fn sanitize(&mut self) {
        self.timeout_secs = self.timeout_secs.clamp(3, 300);
        self.keepalive_secs = self.keepalive_secs.min(3600);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub transfers: TransferSettings,
    pub connection: ConnectionSettings,
    pub interface: serde_json::Value,
    pub appearance: serde_json::Value,
    pub log: serde_json::Value,
}

impl Default for Settings {
    fn default() -> Self {
        let empty = || serde_json::Value::Object(Default::default());
        Self {
            transfers: TransferSettings::default(),
            connection: ConnectionSettings::default(),
            interface: empty(),
            appearance: empty(),
            log: empty(),
        }
    }
}

impl Settings {
    fn sanitize(mut self) -> Self {
        self.transfers.sanitize();
        self.connection.sanitize();
        for section in [&mut self.interface, &mut self.appearance, &mut self.log] {
            if !section.is_object() {
                *section = serde_json::Value::Object(Default::default());
            }
        }
        self
    }
}

pub struct SettingsStore {
    file: PathBuf,
    current: RwLock<Settings>,
}

impl SettingsStore {
    /// A missing or unreadable file yields defaults; it is rewritten on the next save.
    pub fn load(file: PathBuf) -> Self {
        let current = read(&file).unwrap_or_else(|error| {
            log::warn!("Using default settings, {}: {error}", file.display());
            Settings::default()
        });
        Self {
            file,
            current: RwLock::new(current),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.read().unwrap().clone()
    }

    pub fn set(&self, settings: Settings) -> AppResult<Settings> {
        let settings = settings.sanitize();
        storage::write_json(&self.file, &settings)?;
        *self.current.write().unwrap() = settings.clone();
        Ok(settings)
    }
}

fn read(file: &Path) -> Result<Settings, String> {
    match std::fs::read(file) {
        Ok(bytes) => serde_json::from_slice::<Settings>(&bytes)
            .map(Settings::sanitize)
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults_and_values_are_clamped() {
        let parsed: Settings = serde_json::from_str(
            r#"{"transfers": {"workers": 99, "requestSizeKib": 1}, "unknown": true}"#,
        )
        .unwrap();
        let settings = parsed.sanitize();
        assert_eq!(settings.transfers.workers, MAX_WORKERS);
        assert_eq!(settings.transfers.request_size_kib, 4);
        assert_eq!(settings.transfers.retry_attempts, 3);
        assert!(settings.interface.is_object());
    }

    #[test]
    fn round_trips_through_the_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("settings.json");
        let store = SettingsStore::load(file.clone());
        assert_eq!(store.get(), Settings::default());

        let mut changed = store.get();
        changed.transfers.workers = 8;
        changed.appearance = serde_json::json!({ "theme": "dusk" });
        store.set(changed).unwrap();

        let reloaded = SettingsStore::load(file);
        assert_eq!(reloaded.get().transfers.workers, 8);
        assert_eq!(reloaded.get().appearance["theme"], "dusk");
    }
}
