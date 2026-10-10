//! Saved connections, in `connections.json` in the app config folder. Passwords and key
//! passphrases are never written there: when the user opts in they go to the system keychain
//! (Windows Credential Manager, or the Secret Service on Linux), and they never travel back to
//! the frontend. Each is kept for the server, account and sign-in method it was saved with, so
//! a connection edited to point elsewhere does not take it along. Mirrored in
//! `src/lib/types.ts`.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol::Protocol;
use crate::ssh::{AuthMethod, ConnectProfile};
use crate::storage;

const KEYCHAIN_SERVICE: &str = "io.github.khaodius.poros";
/// The keychain entry of the password for the proxy in the connection settings.
const PROXY_SECRET: &str = "proxy";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthType {
    Password,
    PublicKey,
    Agent,
    /// A Google or Microsoft account signed in through the browser.
    #[serde(rename = "oauth")]
    OAuth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub protocol: Protocol,
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
    /// FTP data connections come from the server (active mode).
    #[serde(default)]
    pub ftp_active: bool,
    /// Connects without the proxy from the connection settings.
    #[serde(default)]
    pub bypass_proxy: bool,
    /// Another saved connection to tunnel through.
    #[serde(default)]
    pub jump_connection_id: Option<String>,
}

/// What a stored secret may be used for: the server, account and sign-in method it was saved
/// with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretScope {
    host: String,
    port: u16,
    username: String,
    auth_type: AuthType,
}

impl SecretScope {
    fn new(protocol: Protocol, host: &str, port: u16, username: &str, auth_type: AuthType) -> Self {
        // A cloud service has one address, whatever a profile carries.
        let (host, port) = match protocol.service_host() {
            Some(service_host) => (service_host.to_string(), protocol.default_port()),
            None => (host.trim().to_ascii_lowercase(), port),
        };
        Self {
            host,
            port,
            username: username.trim().to_string(),
            auth_type,
        }
    }

    pub fn of_saved(saved: &SavedConnection) -> Self {
        Self::new(
            saved.protocol,
            &saved.host,
            saved.port,
            &saved.username,
            saved.auth_type,
        )
    }

    pub fn of_profile(profile: &ConnectProfile) -> Self {
        let auth_type = match profile.auth {
            AuthMethod::Password { .. } => AuthType::Password,
            AuthMethod::PublicKey { .. } => AuthType::PublicKey,
            AuthMethod::Agent => AuthType::Agent,
            AuthMethod::OAuth { .. } => AuthType::OAuth,
        };
        Self::new(
            profile.protocol,
            &profile.host,
            profile.port,
            &profile.username,
            auth_type,
        )
    }

    /// Keeps host and user names out of the keychain entry names, which the system lists.
    fn fingerprint(&self) -> String {
        let canonical = serde_json::to_vec(self).expect("a scope always serializes");
        Sha256::digest(canonical)[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// Where secrets live, by account name. The keychain in the app, memory in tests.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> AppResult<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> AppResult<()>;
    fn delete(&self, account: &str) -> AppResult<()>;
}

pub struct Keychain;

impl Keychain {
    fn entry(account: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(keychain_error)
    }
}

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> AppResult<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_error(error)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> AppResult<()> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(keychain_error)
    }

    fn delete(&self, account: &str) -> AppResult<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        }
    }
}

/// Secrets are looked up by the scope they are wanted for, so one is only ever found for the
/// scope it was saved with.
fn connection_account(id: &str, scope: &SecretScope) -> String {
    format!("connection:{id}:{}", scope.fingerprint())
}

/// Where versions up to 0.3.1 kept a connection's secret, whatever the connection pointed at.
fn unscoped_account(id: &str) -> String {
    format!("connection:{id}")
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

    /// `secret` replaces the stored one when given; `save_secret: false` deletes it, and so does
    /// a change of server, account or sign-in method without a new one.
    pub fn save(
        &self,
        mut connection: SavedConnection,
        secret: Option<String>,
    ) -> AppResult<SavedConnection> {
        connection.name = connection.name.trim().to_string();
        connection.host = connection.host.trim().to_string();
        if let Some(host) = connection.protocol.service_host() {
            connection.host = host.to_string();
            connection.port = connection.protocol.default_port();
            connection.key_path = None;
        }
        if connection.host.is_empty() {
            return Err(AppError::invalid("Host is required"));
        }
        if connection.name.is_empty() {
            connection.name = connection.host.clone();
        }
        match connection.auth_type {
            AuthType::Agent => connection.save_secret = false,
            // Without the stored sign-in the connection could not be opened again.
            AuthType::OAuth => connection.save_secret = true,
            AuthType::Password | AuthType::PublicKey => {}
        }
        if connection.protocol.is_cloud() != (connection.auth_type == AuthType::OAuth) {
            return Err(AppError::invalid(format!(
                "{} connections cannot use this sign-in method",
                connection.protocol.display_name()
            )));
        }
        let _guard = self.lock.lock().unwrap();
        let mut connections = self.read()?;
        if connection.id.is_empty() {
            connection.id = uuid::Uuid::new_v4().to_string();
        }
        if connection.protocol != Protocol::Sftp
            || connection
                .jump_connection_id
                .as_deref()
                .is_some_and(|jump| jump.is_empty() || jump == connection.id)
        {
            connection.jump_connection_id = None;
        }
        // The proxy only carries SSH connections.
        if connection.protocol != Protocol::Sftp {
            connection.bypass_proxy = false;
        }

        let existing = connections
            .iter()
            .position(|existing| existing.id == connection.id);
        let scope = SecretScope::of_saved(&connection);
        let previous_scope = existing
            .map(|index| SecretScope::of_saved(&connections[index]))
            .filter(|previous| *previous != scope);
        let id = &connection.id;
        let secret = secret.filter(|secret| !secret.is_empty());
        match (&secret, connection.save_secret) {
            (_, false) => self.secrets.delete(&connection_account(id, &scope))?,
            (Some(secret), true) => self.secrets.set(&connection_account(id, &scope), secret)?,
            (None, true) => {}
        }
        if let Some(previous_scope) = &previous_scope {
            self.secrets
                .delete(&connection_account(id, previous_scope))?;
            // What is stored was for the server, account or sign-in method of before.
            connection.save_secret &= secret.is_some();
        }
        if !connection.save_secret || secret.is_some() {
            self.secrets.delete(&unscoped_account(id))?;
        }

        match existing {
            Some(index) => connections[index] = connection.clone(),
            None => connections.push(connection.clone()),
        }
        storage::write_json(&self.file, &connections)?;
        Ok(connection)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let _guard = self.lock.lock().unwrap();
        let mut connections = self.read()?;
        let removed = connections
            .iter()
            .position(|connection| connection.id == id)
            .map(|index| connections.remove(index));
        storage::write_json(&self.file, &connections)?;
        if let Some(removed) = removed {
            self.secrets
                .delete(&connection_account(id, &SecretScope::of_saved(&removed)))?;
        }
        self.secrets.delete(&unscoped_account(id))
    }

    /// The stored secret of a saved connection, for connecting to it as saved.
    pub fn secret(&self, id: &str) -> AppResult<Option<String>> {
        let saved = self
            .list()?
            .into_iter()
            .find(|connection| connection.id == id);
        match saved {
            Some(saved) => self.secret_for(id, &SecretScope::of_saved(&saved)),
            None => Ok(None),
        }
    }

    /// The stored secret of a saved connection, if it was saved for `scope`.
    pub fn secret_for(&self, id: &str, scope: &SecretScope) -> AppResult<Option<String>> {
        let _guard = self.lock.lock().unwrap();
        let Some(saved) = self
            .read()?
            .into_iter()
            .find(|connection| connection.id == id && connection.save_secret)
        else {
            return Ok(None);
        };
        let account = connection_account(id, scope);
        if let Some(secret) = self.secrets.get(&account)? {
            return Ok(Some(secret));
        }
        // An unscoped secret belongs to the connection as saved now, since saving it with
        // another server, account or sign-in method deletes it.
        if *scope != SecretScope::of_saved(&saved) {
            return Ok(None);
        }
        let unscoped = unscoped_account(id);
        let Some(secret) = self.secrets.get(&unscoped)? else {
            return Ok(None);
        };
        if self.secrets.set(&account, &secret).is_ok() {
            // Best effort: the scoped copy is found first from now on.
            let _ = self.secrets.delete(&unscoped);
        }
        Ok(Some(secret))
    }

    /// Replaces the stored secret of a saved connection that keeps one, as when a cloud
    /// provider issues a new refresh token. A session opened with details other than the saved
    /// ones, such as another account, leaves it alone.
    pub fn update_secret(&self, id: &str, scope: &SecretScope, secret: &str) -> AppResult<()> {
        let _guard = self.lock.lock().unwrap();
        let keeps_secret = self.read()?.iter().any(|connection| {
            connection.id == id
                && connection.save_secret
                && SecretScope::of_saved(connection) == *scope
        });
        if keeps_secret {
            self.secrets.set(&connection_account(id, scope), secret)?;
        }
        Ok(())
    }

    pub fn proxy_password(&self) -> AppResult<Option<String>> {
        self.secrets.get(PROXY_SECRET)
    }

    /// An empty or missing password deletes the stored one.
    pub fn set_proxy_password(&self, password: Option<&str>) -> AppResult<()> {
        match password.filter(|password| !password.is_empty()) {
            Some(password) => self.secrets.set(PROXY_SECRET, password),
            None => self.secrets.delete(PROXY_SECRET),
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
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    pub(crate) struct MemorySecrets(Mutex<HashMap<String, String>>);

    impl SecretStore for MemorySecrets {
        fn get(&self, account: &str) -> AppResult<Option<String>> {
            Ok(self.0.lock().unwrap().get(account).cloned())
        }
        fn set(&self, account: &str, secret: &str) -> AppResult<()> {
            self.0.lock().unwrap().insert(account.into(), secret.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> AppResult<()> {
            self.0.lock().unwrap().remove(account);
            Ok(())
        }
    }

    pub(crate) fn connection() -> SavedConnection {
        SavedConnection {
            id: String::new(),
            protocol: Protocol::Sftp,
            name: " ".into(),
            host: " example.com ".into(),
            port: 22,
            username: "alice".into(),
            auth_type: AuthType::Password,
            key_path: None,
            remote_path: None,
            save_secret: true,
            last_used: None,
            ftp_active: false,
            bypass_proxy: false,
            jump_connection_id: None,
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

        store
            .update_secret(&saved.id, &SecretScope::of_saved(&saved), "rotated")
            .unwrap();
        assert_eq!(store.secret(&saved.id).unwrap(), None);

        store.touch(&saved.id, 42).unwrap();
        assert_eq!(store.list().unwrap()[0].last_used, Some(42));
        store.delete(&saved.id).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn cloud_connections_keep_their_sign_in() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        let drive = SavedConnection {
            protocol: Protocol::GoogleDrive,
            host: String::new(),
            username: "ada@example.com".into(),
            auth_type: AuthType::OAuth,
            save_secret: false,
            ..connection()
        };
        let saved = store.save(drive, Some("refresh".into())).unwrap();
        assert!(saved.save_secret);
        assert_eq!(saved.host, "drive.google.com");
        assert_eq!(saved.port, 443);
        store
            .update_secret(&saved.id, &SecretScope::of_saved(&saved), "rotated")
            .unwrap();
        assert_eq!(store.secret(&saved.id).unwrap().as_deref(), Some("rotated"));

        // A session signed in to another account does not replace the saved account's sign-in.
        let other_account = SecretScope::of_saved(&SavedConnection {
            username: "eve@example.com".into(),
            ..saved.clone()
        });
        store
            .update_secret(&saved.id, &other_account, "other")
            .unwrap();
        assert_eq!(store.secret(&saved.id).unwrap().as_deref(), Some("rotated"));
        assert_eq!(store.secret_for(&saved.id, &other_account).unwrap(), None);

        let mismatched = SavedConnection {
            auth_type: AuthType::Password,
            ..saved
        };
        assert!(store.save(mismatched, None).is_err());
    }

    fn profile_scope(
        host: &str,
        port: u16,
        username: &str,
        auth: serde_json::Value,
    ) -> SecretScope {
        let profile: ConnectProfile = serde_json::from_value(serde_json::json!({
            "host": host,
            "port": port,
            "username": username,
            "auth": auth,
        }))
        .unwrap();
        SecretScope::of_profile(&profile)
    }

    fn password() -> serde_json::Value {
        serde_json::json!({ "type": "password", "password": "" })
    }

    #[test]
    fn a_saved_secret_is_only_released_to_the_server_it_was_saved_for() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        let saved = store.save(connection(), Some("hunter2".into())).unwrap();
        let secret_for = |scope: SecretScope| store.secret_for(&saved.id, &scope).unwrap();

        assert_eq!(
            secret_for(profile_scope(" EXAMPLE.com ", 22, "alice", password())).as_deref(),
            Some("hunter2")
        );
        assert_eq!(
            secret_for(profile_scope("evil.example", 22, "alice", password())),
            None
        );
        assert_eq!(
            secret_for(profile_scope("example.com", 2222, "alice", password())),
            None
        );
        assert_eq!(
            secret_for(profile_scope("example.com", 22, "root", password())),
            None
        );
        let key = serde_json::json!({ "type": "publicKey", "keyPath": "/k", "passphrase": null });
        assert_eq!(
            secret_for(profile_scope("example.com", 22, "alice", key)),
            None
        );

        // Editing anything else keeps the secret.
        let renamed = SavedConnection {
            name: "Renamed".into(),
            remote_path: Some("/srv".into()),
            ..saved.clone()
        };
        assert!(store.save(renamed, None).unwrap().save_secret);
        assert_eq!(store.secret(&saved.id).unwrap().as_deref(), Some("hunter2"));

        // Pointing the connection at another server without a new password forgets the old one.
        let moved = SavedConnection {
            host: "evil.example".into(),
            ..saved.clone()
        };
        let moved = store.save(moved, None).unwrap();
        assert!(!moved.save_secret);
        assert_eq!(store.secret(&saved.id).unwrap(), None);
        assert_eq!(
            store
                .secret_for(&saved.id, &SecretScope::of_saved(&saved))
                .unwrap(),
            None
        );

        // A password typed for the new server is kept for that server only.
        let retyped = SavedConnection {
            save_secret: true,
            ..moved
        };
        let retyped = store.save(retyped, Some("new-pass".into())).unwrap();
        assert!(retyped.save_secret);
        assert_eq!(
            store.secret(&saved.id).unwrap().as_deref(),
            Some("new-pass")
        );
        let back = SavedConnection {
            host: "example.com".into(),
            ..retyped
        };
        assert!(!store.save(back, None).unwrap().save_secret);
        assert_eq!(
            secret_for(profile_scope("evil.example", 22, "alice", password())),
            None
        );
    }

    #[test]
    fn secrets_saved_before_scopes_keep_working_for_unchanged_connections() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        let saved = store.save(connection(), None).unwrap();
        assert!(saved.save_secret);
        store
            .secrets
            .set(&unscoped_account(&saved.id), "from-0.3.1")
            .unwrap();

        let elsewhere = profile_scope("evil.example", 22, "alice", password());
        assert_eq!(store.secret_for(&saved.id, &elsewhere).unwrap(), None);
        assert_eq!(
            store.secret(&saved.id).unwrap().as_deref(),
            Some("from-0.3.1")
        );
        // Moved under its scope on first use.
        assert_eq!(
            store.secrets.get(&unscoped_account(&saved.id)).unwrap(),
            None
        );
        assert_eq!(
            store.secret(&saved.id).unwrap().as_deref(),
            Some("from-0.3.1")
        );

        let other = store.save(connection(), None).unwrap();
        store
            .secrets
            .set(&unscoped_account(&other.id), "older")
            .unwrap();
        let moved = SavedConnection {
            host: "evil.example".into(),
            ..other.clone()
        };
        assert!(!store.save(moved, None).unwrap().save_secret);
        assert_eq!(
            store.secrets.get(&unscoped_account(&other.id)).unwrap(),
            None
        );
        assert_eq!(store.secret_for(&other.id, &elsewhere).unwrap(), None);

        store.delete(&saved.id).unwrap();
        let scope = SecretScope::of_saved(&saved);
        assert_eq!(
            store
                .secrets
                .get(&connection_account(&saved.id, &scope))
                .unwrap(),
            None
        );
    }

    #[test]
    fn a_connection_cannot_jump_through_itself() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        let saved = store.save(connection(), None).unwrap();
        let looped = SavedConnection {
            jump_connection_id: Some(saved.id.clone()),
            ..saved.clone()
        };
        assert_eq!(store.save(looped, None).unwrap().jump_connection_id, None);

        let ftp = SavedConnection {
            protocol: Protocol::Ftp,
            port: 21,
            bypass_proxy: true,
            jump_connection_id: Some(saved.id.clone()),
            ..connection()
        };
        let ftp = store.save(ftp, None).unwrap();
        assert_eq!(ftp.jump_connection_id, None);
        assert!(!ftp.bypass_proxy);
    }

    #[test]
    fn proxy_password_lives_beside_connection_secrets() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        assert_eq!(store.proxy_password().unwrap(), None);
        store.set_proxy_password(Some("pw")).unwrap();
        assert_eq!(store.proxy_password().unwrap().as_deref(), Some("pw"));
        store.set_proxy_password(Some("")).unwrap();
        assert_eq!(store.proxy_password().unwrap(), None);
    }
}
