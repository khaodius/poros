use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock as SyncRwLock};
use std::time::Instant;

use russh::client::Msg;
use russh::Channel;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::cloud;
use crate::connections::SecretScope;
use crate::cryptomator::VaultFs;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::ftp::FtpFs;
use crate::protocol::{self, Protocol, RemoteFileSystem};
use crate::sftp::RemoteFs;
use crate::share::{self, ShareCapabilities};
use crate::ssh::known_hosts::KnownHosts;
use crate::ssh::{self, ConnectProfile, HostKeyApproval, SshHandle};
use crate::tls::TrustedCertificates;

/// Called with a saved connection's id, what the session signed in to and its new secret when
/// a cloud provider replaces a refresh token, so the keychain keeps the current one.
pub type SecretSink = Arc<dyn Fn(&str, &SecretScope, &str) + Send + Sync>;

/// Called with a browser sign-in's grant id and the refresh token that replaced its own, so a
/// connection saved from the session later keeps one that still works.
pub type GrantSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub struct Session {
    pub id: String,
    /// Kept (with credentials) so transfer workers can open more connections. An unlocked
    /// vault's is that of the server it is on.
    pub profile: ConnectProfile,
    /// The SSH host key or FTPS certificate the server proved it holds; extra connections pin
    /// it.
    pub host_key_fingerprint: String,
    /// Label of the window showing this session; closing that window disconnects it.
    owner: Mutex<String>,
    info: SessionInfo,
    link: Link,
    pub opened_at: Instant,
}

enum Link {
    Ssh {
        handle: SshHandle,
        fs: Arc<RemoteFs>,
    },
    Direct(Arc<dyn RemoteFileSystem>),
    /// An unlocked vault, reached over the connection of the session of the server it is on.
    /// The session shows the vault's decrypted files and nothing else of that server, so
    /// nothing that takes a session can write past the vault's encryption.
    Vault {
        files: Arc<VaultFs>,
        server_session: String,
    },
}

impl Session {
    /// The session's files, whatever the protocol.
    pub fn files(&self) -> Arc<dyn RemoteFileSystem> {
        match &self.link {
            Link::Ssh { fs, .. } => fs.clone(),
            Link::Direct(files) => files.clone(),
            Link::Vault { files, .. } => files.clone(),
        }
    }

    pub fn protocol(&self) -> Protocol {
        match &self.link {
            Link::Vault { .. } => Protocol::Cryptomator,
            _ => self.profile.protocol,
        }
    }

    /// The unlocked vault this session shows, if it is a vault's.
    pub fn vault(&self) -> Option<&Arc<VaultFs>> {
        match &self.link {
            Link::Vault { files, .. } => Some(files),
            _ => None,
        }
    }

    /// The session of the server a vault is on, for a vault's session.
    pub fn server_session(&self) -> Option<&str> {
        match &self.link {
            Link::Vault { server_session, .. } => Some(server_session),
            _ => None,
        }
    }

    /// The SFTP channel of an SSH session, for what only SFTP can do.
    pub fn sftp(&self) -> AppResult<&Arc<RemoteFs>> {
        match &self.link {
            Link::Ssh { fs, .. } => Ok(fs),
            Link::Direct(_) | Link::Vault { .. } => Err(self.needs_ssh()),
        }
    }

    /// A further SFTP channel multiplexed over this session's connection, for transfer workers
    /// when the server refuses extra connections.
    pub async fn open_channel(&self) -> AppResult<RemoteFs> {
        match &self.link {
            Link::Ssh { handle, .. } => open_sftp(handle).await,
            Link::Direct(_) | Link::Vault { .. } => Err(self.needs_ssh()),
        }
    }

    /// A channel on this session's connection for running a command on the server.
    pub async fn open_command_channel(&self) -> AppResult<Channel<Msg>> {
        match &self.link {
            Link::Ssh { handle, .. } => Ok(handle.channel_open_session().await?),
            Link::Direct(_) | Link::Vault { .. } => Err(self.needs_ssh()),
        }
    }

    fn needs_ssh(&self) -> AppError {
        AppError::unsupported(format!(
            "This needs an SFTP connection; {} uses {}",
            self.info.label,
            self.protocol().display_name()
        ))
    }

    fn is_closed(&self) -> bool {
        match &self.link {
            Link::Ssh { handle, .. } => handle.is_closed(),
            // FTP connections reopen by themselves, and HTTP services have no connection to lose.
            Link::Direct(_) => false,
            // A vault has no connection of its own: it is lost and back with its server's.
            Link::Vault { .. } => false,
        }
    }

    async fn close(&self) {
        match &self.link {
            Link::Ssh { handle, fs } => {
                fs.close();
                ssh::disconnect(handle).await;
            }
            Link::Direct(files) => files.close().await,
            Link::Vault { files, .. } => files.lock(),
        }
    }

    pub fn target(&self) -> RemoteTarget {
        RemoteTarget {
            session_id: self.id.clone(),
            label: self.info.label.clone(),
            protocol: self.protocol(),
            profile: self.profile.clone(),
            host_key_fingerprint: self.host_key_fingerprint.clone(),
            vault_root: self.vault().map(|vault| vault.root().to_string()),
        }
    }
}

/// What it takes to reach a session's server again after the session itself is gone: its login
/// and the host key it was trusted with.
#[derive(Clone)]
pub struct RemoteTarget {
    pub session_id: String,
    pub label: String,
    /// The session's protocol, which for an unlocked vault is not its server's.
    protocol: Protocol,
    profile: ConnectProfile,
    host_key_fingerprint: String,
    /// The vault's folder, for the session of an unlocked vault.
    vault_root: Option<String>,
}

impl RemoteTarget {
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// A connection of its own that accepts the same host key the session did, logging nothing
    /// but failures. `purpose` tells its log lines and close events apart from the session's.
    pub async fn connect(&self, known_hosts: &KnownHosts, purpose: &str) -> AppResult<SshHandle> {
        // A connection to a vault's server would reach around the vault's encryption.
        if self.protocol != Protocol::Sftp {
            return Err(AppError::unsupported(format!(
                "This needs an SFTP connection; {} uses {}",
                self.label,
                self.protocol.display_name()
            )));
        }
        let approval = (!self.host_key_fingerprint.is_empty()).then(|| HostKeyApproval {
            fingerprint: self.host_key_fingerprint.clone(),
            remember: false,
        });
        let connection_id = format!("{}#{purpose}", self.session_id);
        let connection = ssh::connect(
            &connection_id,
            &self.profile,
            known_hosts,
            approval,
            &Events::default(),
        )
        .await?;
        Ok(connection.handle)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub home: String,
    pub initial_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_connection_id: Option<String>,
    pub protocol: Protocol,
    /// Set for the session of an unlocked vault, whose host, port and username are its
    /// server's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultInfo>,
    /// The share links the session can make; absent when the service has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharing: Option<ShareCapabilities>,
    /// Listings give times only to the minute, or to the day, as FTP's `LIST` does.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub coarse_times: bool,
}

/// An unlocked Cryptomator vault, as its session describes it. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    /// The session of the server the vault's files are on.
    pub parent_session_id: String,
    /// The vault's folder on that server. The decrypted files have this path and those below.
    pub root: String,
    /// That folder's name.
    pub name: String,
}

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    /// Sessions whose connection dropped, kept until closed so they can reconnect.
    lost: RwLock<HashMap<String, Arc<Session>>>,
    pub known_hosts: KnownHosts,
    pub certificates: TrustedCertificates,
    secret_sink: SyncRwLock<Option<SecretSink>>,
    grant_sink: SyncRwLock<Option<GrantSink>>,
    events: Events,
}

impl SessionManager {
    /// Trusted FTPS certificates are kept beside the app's known_hosts file.
    pub fn new(known_hosts_file: PathBuf, events: Events) -> Self {
        let certificates_file = known_hosts_file.with_file_name("trusted_certificates");
        Self {
            sessions: RwLock::new(HashMap::new()),
            lost: RwLock::new(HashMap::new()),
            known_hosts: KnownHosts::with_defaults(known_hosts_file),
            certificates: TrustedCertificates::new(certificates_file),
            secret_sink: SyncRwLock::new(None),
            grant_sink: SyncRwLock::new(None),
            events,
        }
    }

    pub fn set_secret_sink(&self, sink: SecretSink) {
        *self.secret_sink.write().unwrap() = Some(sink);
    }

    pub fn set_grant_sink(&self, sink: GrantSink) {
        *self.grant_sink.write().unwrap() = Some(sink);
    }

    /// Where a session's rotated refresh tokens go: the keychain entry of its saved connection,
    /// and the browser sign-in it was opened with.
    fn rotation_hook(&self, profile: &ConnectProfile) -> Option<cloud::TokenRotation> {
        let saved = profile
            .saved_connection_id
            .clone()
            .zip(self.secret_sink.read().unwrap().clone());
        let scope = SecretScope::of_profile(profile);
        let grant = match &profile.auth {
            ssh::AuthMethod::OAuth {
                grant_id: Some(grant_id),
                ..
            } => Some(grant_id.clone()).zip(self.grant_sink.read().unwrap().clone()),
            _ => None,
        };
        if saved.is_none() && grant.is_none() {
            return None;
        }
        Some(Arc::new(move |secret: &str| {
            if let Some((saved_id, sink)) = &saved {
                sink(saved_id, &scope, secret);
            }
            if let Some((grant_id, sink)) = &grant {
                sink(grant_id, secret);
            }
        }))
    }

    pub async fn connect(
        &self,
        profile: ConnectProfile,
        approval: Option<HostKeyApproval>,
        owner: &str,
    ) -> AppResult<SessionInfo> {
        let id = uuid::Uuid::new_v4().to_string();
        let result = self.open(&id, profile, approval, owner).await;
        if let Err(error) = &result {
            let level = match error.kind {
                ErrorKind::HostKeyUnknown | ErrorKind::PassphraseRequired => LogLevel::Warn,
                _ => LogLevel::Error,
            };
            self.events.log(level, Some(&id), error.message.clone());
        }
        result
    }

    /// Opens a new session with the profile of an existing or dropped one, pinning the host
    /// key it was trusted with unless the caller approves another.
    pub async fn reconnect(
        &self,
        id: &str,
        approval: Option<HostKeyApproval>,
        owner: &str,
    ) -> AppResult<SessionInfo> {
        let previous = match self.sessions.read().await.get(id) {
            Some(session) => Some(session.clone()),
            None => self.lost.read().await.get(id).cloned(),
        }
        .ok_or_else(AppError::session_not_found)?;
        if previous.vault().is_some() {
            return Err(AppError::invalid(
                "A vault comes back with its server: reconnect the server",
            ));
        }
        let approval = approval.or_else(|| {
            (!previous.host_key_fingerprint.is_empty()).then(|| HostKeyApproval {
                fingerprint: previous.host_key_fingerprint.clone(),
                remember: false,
            })
        });
        let info = self
            .connect(previous.profile.clone(), approval, owner)
            .await?;
        self.move_vaults(id, &info.id).await;
        if self.lost.write().await.remove(id).is_some() {
            previous.close().await;
        }
        Ok(info)
    }

    /// Hands the vaults unlocked on one session of a server to another session of the same
    /// server, keys and all, so they stay open over a reconnect. Their own sessions keep their
    /// ids.
    async fn move_vaults(&self, from: &str, to: &str) {
        let mut sessions = self.sessions.write().await;
        let Some(server) = sessions.get(to).cloned() else {
            return;
        };
        let moving: Vec<Arc<Session>> = sessions
            .values()
            .filter(|session| session.server_session() == Some(from))
            .cloned()
            .collect();
        for vault_session in moving {
            let Some(files) = vault_session.vault().cloned() else {
                continue;
            };
            files.reattach(server.files());
            let owner = vault_session.owner.lock().unwrap().clone();
            let mut moved = vault_session_of(&vault_session.id, &server, files, &owner);
            moved.opened_at = vault_session.opened_at;
            sessions.insert(vault_session.id.clone(), Arc::new(moved));
        }
    }

    /// The session of a server on which a vault can be opened: not itself a vault's.
    async fn server_for_vault(&self, session_id: &str) -> AppResult<Arc<Session>> {
        let server = self.get(session_id).await?;
        if server.vault().is_some() {
            return Err(AppError::invalid(
                "A vault cannot be opened inside another vault",
            ));
        }
        Ok(server)
    }

    /// Opens the vault in the folder `path` on a session's server with its password. The vault
    /// gets a session of its own, which a vault that is open already answers with.
    pub async fn unlock_vault(
        &self,
        session_id: &str,
        path: &str,
        password: String,
        owner: &str,
    ) -> AppResult<SessionInfo> {
        let server = self.server_for_vault(session_id).await?;
        let files = server.files();
        let root = protocol::normalize(&files.resolve(path));
        if let Some(open) = self.open_vault(&server.id, &root).await {
            return Ok(open.info.clone());
        }
        let vault = VaultFs::unlock(files, &root, password).await?;
        self.add_vault(&server, vault, owner).await
    }

    /// Makes the new folder `name` in `parent` on a session's server a vault that `password`
    /// opens, and opens it in a session of its own.
    pub async fn create_vault(
        &self,
        session_id: &str,
        parent: &str,
        name: &str,
        password: String,
        owner: &str,
    ) -> AppResult<SessionInfo> {
        let server = self.server_for_vault(session_id).await?;
        let vault = VaultFs::create(server.files(), parent, name, password).await?;
        self.add_vault(&server, vault, owner).await
    }

    /// The session of the vault in `root` that is unlocked on a server's session, if one is.
    async fn open_vault(&self, server_session: &str, root: &str) -> Option<Arc<Session>> {
        self.sessions
            .read()
            .await
            .values()
            .find(|session| {
                session.server_session() == Some(server_session)
                    && session.vault().is_some_and(|vault| vault.root() == root)
            })
            .cloned()
    }

    async fn add_vault(
        &self,
        server: &Session,
        vault: Arc<VaultFs>,
        owner: &str,
    ) -> AppResult<SessionInfo> {
        let mut sessions = self.sessions.write().await;
        // Deriving the vault's key takes a moment, in which the same vault may have been
        // unlocked by another request or its server closed.
        let unlocked_meanwhile = sessions.values().find(|session| {
            session.server_session() == Some(server.id.as_str())
                && session
                    .vault()
                    .is_some_and(|open| open.root() == vault.root())
        });
        if let Some(open) = unlocked_meanwhile {
            vault.lock();
            return Ok(open.info.clone());
        }
        if !sessions.contains_key(&server.id) {
            vault.lock();
            return Err(AppError::session_not_found());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let session = vault_session_of(&id, server, vault, owner);
        let info = session.info.clone();
        sessions.insert(id, Arc::new(session));
        Ok(info)
    }

    async fn open(
        &self,
        id: &str,
        profile: ConnectProfile,
        approval: Option<HostKeyApproval>,
        owner: &str,
    ) -> AppResult<SessionInfo> {
        let (link, host_key_fingerprint) = match profile.protocol {
            Protocol::Cryptomator => return Err(crate::cryptomator::not_a_connection()),
            Protocol::Sftp => {
                let ssh::Connection {
                    handle,
                    host_key_fingerprint,
                } = ssh::connect(id, &profile, &self.known_hosts, approval, &self.events).await?;
                let fs = match open_sftp(&handle).await {
                    Ok(fs) => fs,
                    Err(error) => {
                        ssh::disconnect(&handle).await;
                        return Err(error);
                    }
                };
                self.events.log(
                    LogLevel::Info,
                    Some(id),
                    format!("SFTP session ready, home directory {}", fs.home),
                );
                let fs = Arc::new(fs);
                (Link::Ssh { handle, fs }, host_key_fingerprint)
            }
            protocol if protocol.is_ftp() => {
                let (files, fingerprint) =
                    FtpFs::connect(id, &profile, &self.certificates, approval, &self.events)
                        .await?;
                (Link::Direct(files), fingerprint)
            }
            _ => {
                let login = cloud::Login {
                    certificates: &self.certificates,
                    approval,
                    rotation: self.rotation_hook(&profile),
                };
                let (files, fingerprint) =
                    cloud::connect(id, &profile, login, &self.events).await?;
                (Link::Direct(files), fingerprint)
            }
        };
        self.add(id, profile, link, host_key_fingerprint, owner)
            .await
    }

    /// Keeps a session over the connection `link` open, in the folder its profile starts in.
    async fn add(
        &self,
        id: &str,
        profile: ConnectProfile,
        link: Link,
        host_key_fingerprint: String,
        owner: &str,
    ) -> AppResult<SessionInfo> {
        let files = match &link {
            Link::Ssh { fs, .. } => fs.clone() as Arc<dyn RemoteFileSystem>,
            Link::Direct(files) => files.clone(),
            Link::Vault { files, .. } => files.clone(),
        };

        let initial_path = match profile.initial_path.as_deref().map(str::trim) {
            Some(requested) if !requested.is_empty() => {
                match files.canonicalize(&files.resolve(requested)).await {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        self.events.log(
                            LogLevel::Warn,
                            Some(id),
                            format!(
                                "Initial directory {requested} is not accessible: {}",
                                error.message
                            ),
                        );
                        files.home().to_string()
                    }
                }
            }
            _ => files.home().to_string(),
        };

        let info = SessionInfo {
            id: id.to_string(),
            label: profile.label(),
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            home: files.home().to_string(),
            initial_path,
            saved_connection_id: profile.saved_connection_id.clone(),
            protocol: profile.protocol,
            vault: None,
            sharing: share::capabilities(files.as_ref()),
            coarse_times: files.coarse_times(),
        };
        let session = Arc::new(Session {
            id: id.to_string(),
            profile,
            host_key_fingerprint,
            owner: Mutex::new(owner.to_string()),
            info: info.clone(),
            link,
            opened_at: Instant::now(),
        });
        self.sessions.write().await.insert(id.to_string(), session);
        Ok(info)
    }

    pub async fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        let (session, connection) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(id)
                .cloned()
                .ok_or_else(AppError::session_not_found)?;
            // A vault's connection is that of its server's session.
            let connection = match session.server_session() {
                Some(server_session) => sessions.get(server_session).cloned(),
                None => Some(session.clone()),
            };
            (session, connection)
        };
        if connection
            .as_ref()
            .is_none_or(|carrier| carrier.is_closed())
        {
            if let Some(carrier) = connection {
                if let Some(lost) = self.sessions.write().await.remove(&carrier.id) {
                    self.lost.write().await.insert(carrier.id.clone(), lost);
                }
            }
            return Err(AppError::new(
                ErrorKind::Disconnected,
                "The connection was closed",
            ));
        }
        Ok(session)
    }

    /// Whether the session is open and its connection, or for a vault its server's, still up.
    pub async fn is_live(&self, id: &str) -> bool {
        let sessions = self.sessions.read().await;
        let Some(session) = sessions.get(id) else {
            return false;
        };
        match session.server_session() {
            Some(server_session) => sessions
                .get(server_session)
                .is_some_and(|server| !server.is_closed()),
            None => !session.is_closed(),
        }
    }

    /// An open session to the same account on the same server over the same protocol, if there
    /// is one, opened after `opened_after` when given. Never a vault's session, which shows
    /// other files than its server does.
    pub async fn find_live(
        &self,
        like: &ConnectProfile,
        opened_after: Option<Instant>,
    ) -> Option<Arc<Session>> {
        self.sessions
            .read()
            .await
            .values()
            .find(|session| {
                session.vault().is_none()
                    && same_account(&session.profile, like)
                    && opened_after.is_none_or(|since| session.opened_at > since)
                    && !session.is_closed()
            })
            .cloned()
    }

    /// The session of the vault in the folder `root` on the server `like` connects to, if the
    /// vault is unlocked. A vault that was locked comes back in a new session.
    pub async fn find_vault(&self, like: &ConnectProfile, root: &str) -> Option<Arc<Session>> {
        self.sessions
            .read()
            .await
            .values()
            .find(|session| {
                same_account(&session.profile, like)
                    && session
                        .vault()
                        .is_some_and(|vault| vault.root() == root && !vault.is_locked())
            })
            .cloned()
    }

    /// The session to reach a target's files through now: its own while it is open, else an open
    /// session that stands in for it (see `stands_in_for`), such as the one a reconnect or
    /// connecting again opened, in whichever window. A vault's files are reached through the
    /// vault alone, unlocked again when it was locked, never through its server's session.
    pub async fn reach(&self, target: &RemoteTarget) -> AppResult<Arc<Session>> {
        let error = match self.get(&target.session_id).await {
            Ok(session) => return Ok(session),
            Err(error) => error,
        };
        let mut stand_ins: Vec<Arc<Session>> = self
            .sessions
            .read()
            .await
            .values()
            .filter(|session| stands_in_for(session, target))
            .cloned()
            .collect();
        // One opened from the same saved connection comes first.
        let saved = target.profile.saved_connection_id.as_deref();
        stand_ins.sort_by_key(|session| {
            saved.is_none() || session.profile.saved_connection_id.as_deref() != saved
        });
        for stand_in in stand_ins {
            if let Ok(session) = self.get(&stand_in.id).await {
                return Ok(session);
            }
        }
        if error.kind != ErrorKind::SessionNotFound {
            return Err(error);
        }
        let advice = if target.vault_root.is_some() {
            "is locked. Unlock the vault again"
        } else {
            "is no longer connected. Connect to it again"
        };
        Err(AppError::new(
            ErrorKind::SessionNotFound,
            format!("{} {advice} to reach this file", target.label),
        ))
    }

    /// The session's details, for a window taking over a tab; the window becomes its owner,
    /// and the owner of the vaults unlocked on it.
    pub async fn adopt(&self, id: &str, owner: &str) -> AppResult<SessionInfo> {
        let session = self.get(id).await?;
        *session.owner.lock().unwrap() = owner.to_string();
        for vault_session in self.sessions.read().await.values() {
            if vault_session.server_session() == Some(id) {
                *vault_session.owner.lock().unwrap() = owner.to_string();
            }
        }
        Ok(session.info.clone())
    }

    /// Closes a session. The vaults unlocked on a server are locked with it, and a vault's own
    /// session closes by locking the vault.
    pub async fn disconnect(&self, id: &str) -> AppResult<()> {
        let (session, vault_sessions) = {
            let mut sessions = self.sessions.write().await;
            let session = sessions.remove(id);
            let on_it: Vec<String> = sessions
                .values()
                .filter(|vault_session| vault_session.server_session() == Some(id))
                .map(|vault_session| vault_session.id.clone())
                .collect();
            let vault_sessions: Vec<Arc<Session>> = on_it
                .iter()
                .filter_map(|vault_id| sessions.remove(vault_id))
                .collect();
            (session, vault_sessions)
        };
        self.lost.write().await.remove(id);
        for vault_session in vault_sessions {
            vault_session.close().await;
        }
        if let Some(session) = session {
            session.close().await;
            if session.vault().is_none() {
                self.events.log(LogLevel::Info, Some(id), "Disconnected");
            }
        }
        Ok(())
    }

    pub async fn disconnect_owned_by(&self, owner: &str) {
        let owned = |sessions: &HashMap<String, Arc<Session>>| -> Vec<String> {
            sessions
                .values()
                .filter(|session| *session.owner.lock().unwrap() == owner)
                .map(|session| session.id.clone())
                .collect()
        };
        let mut ids = owned(&*self.sessions.read().await);
        ids.extend(owned(&*self.lost.read().await));
        for id in ids {
            let _ = self.disconnect(&id).await;
        }
    }

    pub async fn disconnect_all(&self) {
        let ids: Vec<String> = self.sessions.read().await.keys().cloned().collect();
        for id in ids {
            let _ = self.disconnect(&id).await;
        }
    }
}

#[cfg(test)]
impl SessionManager {
    /// A session over files that stand in for a server, for tests that need no real one.
    pub(crate) async fn open_over(
        &self,
        profile: ConnectProfile,
        files: Arc<dyn RemoteFileSystem>,
    ) -> SessionInfo {
        let id = uuid::Uuid::new_v4().to_string();
        self.add(&id, profile, Link::Direct(files), String::new(), "test")
            .await
            .unwrap()
    }
}

/// Whether two profiles sign in to the same account on the same server over the same protocol.
fn same_account(profile: &ConnectProfile, like: &ConnectProfile) -> bool {
    profile.protocol == like.protocol
        && profile.host == like.host
        && profile.port == like.port
        && profile.username == like.username
}

/// Whether `session` reaches the very machine a target's session was on, the same way, and shows
/// the same files: the same account, through the same jump host and proxy setting, holding the
/// host key or certificate the target was trusted with when it had one, and the same unlocked
/// vault for a vault's target or the server's own files for a server's.
fn stands_in_for(session: &Session, target: &RemoteTarget) -> bool {
    let shows_the_same_files = match (&target.vault_root, session.vault()) {
        (Some(root), Some(vault)) => vault.root() == root && !vault.is_locked(),
        (None, None) => true,
        _ => false,
    };
    session.id != target.session_id
        && same_account(&session.profile, &target.profile)
        && session.profile.jump_connection_id == target.profile.jump_connection_id
        && session.profile.bypass_proxy == target.profile.bypass_proxy
        && (target.host_key_fingerprint.is_empty()
            || session.host_key_fingerprint == target.host_key_fingerprint)
        && shows_the_same_files
}

/// The session of an unlocked vault on `server`.
fn vault_session_of(id: &str, server: &Session, files: Arc<VaultFs>, owner: &str) -> Session {
    let info = SessionInfo {
        id: id.to_string(),
        label: format!("{} on {}", files.name(), server.info.label),
        host: server.info.host.clone(),
        port: server.info.port,
        username: server.info.username.clone(),
        home: files.root().to_string(),
        initial_path: files.root().to_string(),
        saved_connection_id: None,
        protocol: Protocol::Cryptomator,
        vault: Some(VaultInfo {
            parent_session_id: server.id.clone(),
            root: files.root().to_string(),
            name: files.name().to_string(),
        }),
        // The server holds a vault's files encrypted, so a link to one would be unreadable.
        sharing: None,
        coarse_times: files.coarse_times(),
    };
    Session {
        id: id.to_string(),
        profile: server.profile.clone(),
        host_key_fingerprint: server.host_key_fingerprint.clone(),
        owner: Mutex::new(owner.to_string()),
        info,
        link: Link::Vault {
            files,
            server_session: server.id.clone(),
        },
        opened_at: Instant::now(),
    }
}

pub async fn open_sftp(handle: &SshHandle) -> AppResult<RemoteFs> {
    let channel = handle.channel_open_session().await?;
    RemoteFs::open(channel).await
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use async_trait::async_trait;

    use super::*;
    use crate::model::DirListing;
    use crate::protocol::{Capabilities, ReadStream, RemoteStat, WriteRequest, WriteStream};

    /// Files for a session that is never asked for any: these tests only pick sessions.
    struct NoFiles;

    fn untouched<T>() -> AppResult<T> {
        Err(AppError::unsupported("not used by these tests"))
    }

    #[async_trait]
    impl RemoteFileSystem for NoFiles {
        fn protocol(&self) -> Protocol {
            Protocol::Sftp
        }

        fn home(&self) -> &str {
            "/"
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }

        async fn canonicalize(&self, _path: &str) -> AppResult<String> {
            untouched()
        }

        async fn list_dir(&self, _path: &str) -> AppResult<DirListing> {
            untouched()
        }

        async fn make_dir(&self, _parent: &str, _name: &str) -> AppResult<String> {
            untouched()
        }

        async fn rename(&self, _path: &str, _new_name: &str) -> AppResult<String> {
            untouched()
        }

        async fn delete(&self, _paths: &[String]) -> AppResult<()> {
            untouched()
        }

        async fn stat(&self, _path: &str) -> AppResult<Option<RemoteStat>> {
            untouched()
        }

        async fn ensure_dir(&self, _path: &str) -> AppResult<()> {
            untouched()
        }

        async fn set_attributes(
            &self,
            _path: &str,
            _modified: Option<i64>,
            _permissions: Option<u32>,
        ) -> AppResult<()> {
            untouched()
        }

        async fn open_read(
            self: Arc<Self>,
            _path: &str,
            _range: Range<u64>,
        ) -> AppResult<Box<dyn ReadStream>> {
            untouched()
        }

        async fn open_write(
            self: Arc<Self>,
            _request: WriteRequest,
        ) -> AppResult<Box<dyn WriteStream>> {
            untouched()
        }
    }

    /// root@10.0.0.5 over SFTP, through the saved jump host `jump` when given.
    fn profile(jump: Option<&str>, saved: Option<&str>) -> ConnectProfile {
        serde_json::from_value(serde_json::json!({
            "protocol": "sftp",
            "host": "10.0.0.5",
            "port": 22,
            "username": "root",
            "auth": { "type": "agent" },
            "jumpConnectionId": jump,
            "savedConnectionId": saved,
        }))
        .unwrap()
    }

    fn session(id: &str, profile: ConnectProfile, host_key: &str) -> Session {
        Session {
            id: id.to_string(),
            info: SessionInfo {
                id: id.to_string(),
                label: "root@10.0.0.5".into(),
                host: profile.host.clone(),
                port: profile.port,
                username: profile.username.clone(),
                home: "/root".into(),
                initial_path: "/root".into(),
                saved_connection_id: profile.saved_connection_id.clone(),
                protocol: profile.protocol,
                vault: None,
                coarse_times: NoFiles.coarse_times(),
                sharing: None,
            },
            profile,
            host_key_fingerprint: host_key.to_string(),
            owner: Mutex::new("main".into()),
            link: Link::Direct(Arc::new(NoFiles)),
            opened_at: Instant::now(),
        }
    }

    /// What a document opened on the session `prod`, now closed, remembers of it.
    fn closed_target(profile: ConnectProfile, host_key: &str) -> RemoteTarget {
        session("prod", profile, host_key).target()
    }

    async fn manager_with(sessions: Vec<Session>) -> (tempfile::TempDir, SessionManager) {
        let config = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(config.path().join("known_hosts"), Events::default());
        for session in sessions {
            let id = session.id.clone();
            manager.sessions.write().await.insert(id, Arc::new(session));
        }
        (config, manager)
    }

    async fn reached(manager: &SessionManager, target: &RemoteTarget) -> Result<String, ErrorKind> {
        match manager.reach(target).await {
            Ok(session) => Ok(session.id.clone()),
            Err(error) => Err(error.kind),
        }
    }

    #[tokio::test]
    async fn a_session_through_another_jump_host_does_not_stand_in() {
        let target = closed_target(profile(Some("bastion-prod"), None), "SHA256:prod");
        let (_config, manager) = manager_with(vec![session(
            "staging",
            profile(Some("bastion-staging"), None),
            "SHA256:prod",
        )])
        .await;
        assert_eq!(
            reached(&manager, &target).await,
            Err(ErrorKind::SessionNotFound)
        );

        let mut without_proxy = profile(Some("bastion-prod"), None);
        without_proxy.bypass_proxy = true;
        let (_config, manager) =
            manager_with(vec![session("direct", without_proxy, "SHA256:prod")]).await;
        assert_eq!(
            reached(&manager, &target).await,
            Err(ErrorKind::SessionNotFound)
        );

        let (_config, manager) = manager_with(vec![
            session(
                "staging",
                profile(Some("bastion-staging"), None),
                "SHA256:prod",
            ),
            session("again", profile(Some("bastion-prod"), None), "SHA256:prod"),
        ])
        .await;
        assert_eq!(reached(&manager, &target).await, Ok("again".into()));
    }

    #[tokio::test]
    async fn a_session_holding_another_host_key_does_not_stand_in() {
        let target = closed_target(profile(None, None), "SHA256:prod");
        let (_config, manager) = manager_with(vec![session(
            "forwarded",
            profile(None, None),
            "SHA256:staging",
        )])
        .await;
        assert_eq!(
            reached(&manager, &target).await,
            Err(ErrorKind::SessionNotFound)
        );

        let (_config, manager) = manager_with(vec![
            session("forwarded", profile(None, None), "SHA256:staging"),
            session("again", profile(None, None), "SHA256:prod"),
        ])
        .await;
        assert_eq!(reached(&manager, &target).await, Ok("again".into()));
    }

    #[tokio::test]
    async fn the_same_saved_connection_stands_in_first() {
        let target = closed_target(profile(None, Some("saved-prod")), "SHA256:prod");
        let mut sessions: Vec<Session> = (0..8)
            .map(|index| {
                let id = format!("quick-{index}");
                session(&id, profile(None, None), "SHA256:prod")
            })
            .collect();
        sessions.push(session(
            "saved",
            profile(None, Some("saved-prod")),
            "SHA256:prod",
        ));
        let (_config, manager) = manager_with(sessions).await;
        assert_eq!(reached(&manager, &target).await, Ok("saved".into()));
    }

    #[tokio::test]
    async fn its_own_session_comes_first_while_it_is_open() {
        let target = closed_target(profile(None, None), "SHA256:prod");
        let (_config, manager) = manager_with(vec![
            session("again", profile(None, None), "SHA256:prod"),
            session("prod", profile(None, None), "SHA256:prod"),
        ])
        .await;
        assert_eq!(reached(&manager, &target).await, Ok("prod".into()));
    }
}
