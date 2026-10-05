//! Saved connections, in `connections.json` in the app config folder. Passwords and key
//! passphrases are never written there: when the user opts in they go to the system keychain
//! (Windows Credential Manager, or the Secret Service on Linux), and they never travel back to
//! the frontend. Mirrored in `src/lib/types.ts`.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::storage;

const KEYCHAIN_SERVICE: &str = "io.github.khaodius.poros";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthType {
    Password,
    PublicKey,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub remote_path: Option<String>,
    /// A password or passphrase is in the keychain.
    #[serde(default)]
    pub save_secret: bool,
    /// Milliseconds since the Unix epoch.
    #[serde(default)]
    pub last_used: Option<u64>,
}

/// Where secrets live. The keychain in the app, memory in tests.
pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> AppResult<Option<String>>;
    fn set(&self, id: &str, secret: &str) -> AppResult<()>;
    fn delete(&self, id: &str) -> AppResult<()>;
}

pub struct Keychain;

impl Keychain {
    fn entry(id: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, &format!("connection:{id}")).map_err(keychain_error)
    }
}

impl SecretStore for Keychain {
    fn get(&self, id: &str) -> AppResult<Option<String>> {
        match Self::entry(id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_error(error)),
        }
    }

    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        Self::entry(id)?
            .set_password(secret)
            .map_err(keychain_error)
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        }
    }
}

fn keychain_error(error: keyring::Error) -> AppError {
    AppError::new(
        ErrorKind::Keychain,
        format!("The system keychain is not available: {error}"),
    )
}

pub struct ConnectionStore {
    file: PathBuf,
    secrets: Box<dyn SecretStore>,
    /// Serializes read-modify-write cycles on the file.
    lock: Mutex<()>,
}

impl ConnectionStore {
    pub fn new(file: PathBuf, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            file,
            secrets,
            lock: Mutex::new(()),
        }
    }

    pub fn list(&self) -> AppResult<Vec<SavedConnection>> {
        let _guard = self.lock.lock().unwrap();
        self.read()
    }

    /// `secret` replaces the stored one when given; `save_secret: false` deletes it.
    pub fn save(
        &self,
        mut connection: SavedConnection,
        secret: Option<String>,
    ) -> AppResult<SavedConnection> {
        connection.name = connection.name.trim().to_string();
        connection.host = connection.host.trim().to_string();
        if connection.host.is_empty() {
            return Err(AppError::invalid("Host is required"));
        }
        if connection.name.is_empty() {
            connection.name = connection.host.clone();
        }
        if connection.auth_type == AuthType::Agent {
            connection.save_secret = false;
        }
        let _guard = self.lock.lock().unwrap();
        let mut connections = self.read()?;
        if connection.id.is_empty() {
            connection.id = uuid::Uuid::new_v4().to_string();
        }

        if !connection.save_secret {
            self.secrets.delete(&connection.id)?;
        } else if let Some(secret) = secret.filter(|secret| !secret.is_empty()) {
            self.secrets.set(&connection.id, &secret)?;
        }

        match connections
            .iter_mut()
            .find(|existing| existing.id == connection.id)
        {
            Some(existing) => *existing = connection.clone(),
            None => connections.push(connection.clone()),
        }
        storage::write_json(&self.file, &connections)?;
        Ok(connection)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let _guard = self.lock.lock().unwrap();
        let mut connections = self.read()?;
        connections.retain(|connection| connection.id != id);
        storage::write_json(&self.file, &connections)?;
        self.secrets.delete(id)
    }

    pub fn secret(&self, id: &str) -> AppResult<Option<String>> {
        let saved = self
            .list()?
            .into_iter()
            .any(|connection| connection.id == id && connection.save_secret);
        if saved {
            self.secrets.get(id)
        } else {
            Ok(None)
        }
    }

    pub fn touch(&self, id: &str, now_millis: u64) -> AppResult<()> {
        let _guard = self.lock.lock().unwrap();
        let mut connections = self.read()?;
        let Some(connection) = connections
            .iter_mut()
            .find(|connection| connection.id == id)
        else {
            return Ok(());
        };
        connection.last_used = Some(now_millis);
        storage::write_json(&self.file, &connections)
    }

    fn read(&self) -> AppResult<Vec<SavedConnection>> {
        match std::fs::read(&self.file) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                AppError::invalid(format!("{} is not valid: {error}", self.file.display()))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(AppError::from(error).with_path(self.file.display().to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemorySecrets(Mutex<HashMap<String, String>>);

    impl SecretStore for MemorySecrets {
        fn get(&self, id: &str) -> AppResult<Option<String>> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn set(&self, id: &str, secret: &str) -> AppResult<()> {
            self.0.lock().unwrap().insert(id.into(), secret.into());
            Ok(())
        }
        fn delete(&self, id: &str) -> AppResult<()> {
            self.0.lock().unwrap().remove(id);
            Ok(())
        }
    }

    fn connection() -> SavedConnection {
        SavedConnection {
            id: String::new(),
            name: " ".into(),
            host: " example.com ".into(),
            port: 22,
            username: "alice".into(),
            auth_type: AuthType::Password,
            key_path: None,
            remote_path: None,
            save_secret: true,
            last_used: None,
        }
    }

    #[test]
    fn secrets_stay_out_of_the_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("connections.json");
        let store = ConnectionStore::new(file.clone(), Box::<MemorySecrets>::default());

        let saved = store.save(connection(), Some("hunter2".into())).unwrap();
        assert!(!saved.id.is_empty());
        assert_eq!(saved.name, "example.com");
        assert_eq!(saved.host, "example.com");
        assert_eq!(store.secret(&saved.id).unwrap().as_deref(), Some("hunter2"));
        assert!(!std::fs::read_to_string(&file).unwrap().contains("hunter2"));

        // Saving again without a new secret keeps the stored one.
        store.save(saved.clone(), None).unwrap();
        assert_eq!(store.secret(&saved.id).unwrap().as_deref(), Some("hunter2"));

        let forgetful = SavedConnection {
            save_secret: false,
            ..saved.clone()
        };
        store.save(forgetful, None).unwrap();
        assert_eq!(store.secret(&saved.id).unwrap(), None);
        assert_eq!(store.list().unwrap().len(), 1);

        store.touch(&saved.id, 42).unwrap();
        assert_eq!(store.list().unwrap()[0].last_used, Some(42));
        store.delete(&saved.id).unwrap();
        assert!(store.list().unwrap().is_empty());
    }
}
