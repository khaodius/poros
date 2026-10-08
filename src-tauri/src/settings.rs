//! Persisted in `settings.json` in the app config folder. The backend reads the transfer and
//! connection sections; the interface, appearance, log, sync and updates sections belong to the
//! frontend and are stored as given. Mirrored in `src/lib/settings.ts`.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage;
use crate::transfer::ExistsAction;

pub const MAX_WORKERS: u32 = 16;
pub const MIN_SOCKET_BUFFER_KIB: u32 = 4;
pub const MAX_SOCKET_BUFFER_KIB: u32 = 64 * 1024;
const DEFAULT_RSYNC_PATH: &str = "rsync";
const MAX_RSYNC_PATH_CHARS: usize = 1024;

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
    /// Sends only the changed parts of a file the other side already has, through rsync on
    /// the server.
    pub delta_transfers: bool,
    /// Smaller files are always sent whole.
    pub delta_threshold_kib: u32,
    /// The command that starts rsync on the server.
    pub rsync_path: String,
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
            delta_transfers: true,
            delta_threshold_kib: 1024,
            rsync_path: DEFAULT_RSYNC_PATH.to_string(),
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
        // The path becomes part of a shell command, so it stays on one line.
        let rsync_path = self.rsync_path.lines().next().unwrap_or("").trim();
        self.rsync_path = if rsync_path.is_empty() {
            DEFAULT_RSYNC_PATH.to_string()
        } else {
            rsync_path.chars().take(MAX_RSYNC_PATH_CHARS).collect()
        };
    }

    pub fn request_size(&self) -> u32 {
        self.request_size_kib * 1024
    }

    pub fn segment_threshold(&self) -> u64 {
        u64::from(self.segment_threshold_mib) * 1024 * 1024
    }

    pub fn delta_threshold(&self) -> u64 {
        u64::from(self.delta_threshold_kib) * 1024
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConnectionSettings {
    pub timeout_secs: u64,
    pub keepalive_secs: u64,
    pub compression: bool,
    /// Lets the system grow the TCP receive window with the connection; off uses the size below.
    pub auto_tune_receive_buffer: bool,
    pub receive_buffer_kib: u32,
    /// Lets the system size the send buffer; off uses the size below.
    pub auto_tune_send_buffer: bool,
    pub send_buffer_kib: u32,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            timeout_secs: crate::ssh::DEFAULT_TIMEOUT_SECS,
            keepalive_secs: crate::ssh::DEFAULT_KEEPALIVE_SECS,
            compression: false,
            auto_tune_receive_buffer: true,
            receive_buffer_kib: 128,
            auto_tune_send_buffer: true,
            send_buffer_kib: 128,
        }
    }
}

impl ConnectionSettings {
    fn sanitize(&mut self) {
        self.timeout_secs = self.timeout_secs.clamp(3, 300);
        self.keepalive_secs = self.keepalive_secs.min(3600);
        self.receive_buffer_kib = self
            .receive_buffer_kib
            .clamp(MIN_SOCKET_BUFFER_KIB, MAX_SOCKET_BUFFER_KIB);
        self.send_buffer_kib = self
            .send_buffer_kib
            .clamp(MIN_SOCKET_BUFFER_KIB, MAX_SOCKET_BUFFER_KIB);
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
    /// Defaults for folder synchronization.
    pub sync: serde_json::Value,
    pub updates: serde_json::Value,
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
            sync: empty(),
            updates: empty(),
        }
    }
}

impl Settings {
    fn sanitize(mut self) -> Self {
        self.transfers.sanitize();
        self.connection.sanitize();
        for section in [
            &mut self.interface,
            &mut self.appearance,
            &mut self.log,
            &mut self.sync,
            &mut self.updates,
        ] {
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
            r#"{"transfers": {"workers": 99, "requestSizeKib": 1}, "connection": {"receiveBufferKib": 0}, "unknown": true}"#,
        )
        .unwrap();
        let settings = parsed.sanitize();
        assert_eq!(settings.transfers.workers, MAX_WORKERS);
        assert_eq!(settings.transfers.request_size_kib, 4);
        assert_eq!(settings.transfers.retry_attempts, 3);
        assert_eq!(
            settings.connection.receive_buffer_kib,
            MIN_SOCKET_BUFFER_KIB
        );
        assert!(settings.connection.auto_tune_receive_buffer);
        assert_eq!(settings.connection.send_buffer_kib, 128);
        assert!(settings.interface.is_object());
        assert!(settings.transfers.delta_transfers);
        assert_eq!(settings.transfers.rsync_path, "rsync");
    }

    #[test]
    fn rsync_path_stays_on_one_line() {
        let mut transfers = TransferSettings {
            rsync_path: "  /opt/bin/rsync \nrm -rf ~".into(),
            ..TransferSettings::default()
        };
        transfers.sanitize();
        assert_eq!(transfers.rsync_path, "/opt/bin/rsync");
        transfers.rsync_path = " ".into();
        transfers.sanitize();
        assert_eq!(transfers.rsync_path, "rsync");
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
        changed.updates = serde_json::json!({ "checkOnStart": false });
        store.set(changed).unwrap();

        let reloaded = SettingsStore::load(file);
        assert_eq!(reloaded.get().transfers.workers, 8);
        assert_eq!(reloaded.get().appearance["theme"], "dusk");
        assert_eq!(reloaded.get().updates["checkOnStart"], false);
    }
}
