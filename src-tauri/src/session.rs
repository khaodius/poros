use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use russh::client::Msg;
use russh::Channel;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::sftp::RemoteFs;
use crate::ssh::known_hosts::KnownHosts;
use crate::ssh::{self, ConnectProfile, HostKeyApproval, SshHandle};

pub struct Session {
    pub id: String,
    /// Kept (with credentials) so transfer workers can open more connections.
    pub profile: ConnectProfile,
    pub host_key_fingerprint: String,
    /// Label of the window showing this session; closing that window disconnects it.
    owner: Mutex<String>,
    info: SessionInfo,
    handle: SshHandle,
    pub fs: RemoteFs,
    pub opened_at: Instant,
}

impl Session {
    /// A further SFTP channel multiplexed over this session's connection, for transfer workers
    /// when the server refuses extra connections.
    pub async fn open_channel(&self) -> AppResult<RemoteFs> {
        open_sftp(&self.handle).await
    }

    /// A channel on this session's connection for running a command on the server.
    pub async fn open_command_channel(&self) -> AppResult<Channel<Msg>> {
        Ok(self.handle.channel_open_session().await?)
    }

    pub fn target(&self) -> RemoteTarget {
        RemoteTarget {
            session_id: self.id.clone(),
            label: self.info.label.clone(),
            profile: self.profile.clone(),
            host_key_fingerprint: self.host_key_fingerprint.clone(),
        }
    }
}

/// What it takes to reach a session's server again after the session itself is gone: its login
/// and the host key it was trusted with.
#[derive(Clone)]
pub struct RemoteTarget {
    pub session_id: String,
    pub label: String,
    profile: ConnectProfile,
    host_key_fingerprint: String,
}

impl RemoteTarget {
    /// A connection of its own that accepts the same host key the session did, logging nothing
    /// but failures. `purpose` tells its log lines and close events apart from the session's.
    pub async fn connect(&self, known_hosts: &KnownHosts, purpose: &str) -> AppResult<SshHandle> {
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
}

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    /// Sessions whose connection dropped, kept until closed so they can reconnect.
    lost: RwLock<HashMap<String, Arc<Session>>>,
    pub known_hosts: KnownHosts,
    events: Events,
}

impl SessionManager {
    pub fn new(known_hosts_file: PathBuf, events: Events) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            lost: RwLock::new(HashMap::new()),
            known_hosts: KnownHosts::with_defaults(known_hosts_file),
            events,
        }
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
        let approval = approval.or_else(|| {
            (!previous.host_key_fingerprint.is_empty()).then(|| HostKeyApproval {
                fingerprint: previous.host_key_fingerprint.clone(),
                remember: false,
            })
        });
        let info = self
            .connect(previous.profile.clone(), approval, owner)
            .await?;
        if self.lost.write().await.remove(id).is_some() {
            previous.fs.close();
        }
        Ok(info)
    }

    async fn open(
        &self,
        id: &str,
        profile: ConnectProfile,
        approval: Option<HostKeyApproval>,
        owner: &str,
    ) -> AppResult<SessionInfo> {
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

        let initial_path = match profile.initial_path.as_deref().map(str::trim) {
            Some(requested) if !requested.is_empty() => {
                match fs.canonicalize(&fs.resolve(requested)).await {
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
                        fs.home.clone()
                    }
                }
            }
            _ => fs.home.clone(),
        };

        let info = SessionInfo {
            id: id.to_string(),
            label: profile.label(),
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            home: fs.home.clone(),
            initial_path,
            saved_connection_id: profile.saved_connection_id.clone(),
        };
        self.events.log(
            LogLevel::Info,
            Some(id),
            format!("SFTP session ready, home directory {}", fs.home),
        );
        let session = Arc::new(Session {
            id: id.to_string(),
            profile,
            host_key_fingerprint,
            owner: Mutex::new(owner.to_string()),
            info: info.clone(),
            handle,
            fs,
            opened_at: Instant::now(),
        });
        self.sessions.write().await.insert(id.to_string(), session);
        Ok(info)
    }

    pub async fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        let session = self
            .sessions
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(AppError::session_not_found)?;
        if session.handle.is_closed() {
            if let Some(session) = self.sessions.write().await.remove(id) {
                self.lost.write().await.insert(id.to_string(), session);
            }
            return Err(AppError::new(
                ErrorKind::Disconnected,
                "The connection was closed",
            ));
        }
        Ok(session)
    }

    /// Whether the session is open and its connection still up.
    pub async fn is_live(&self, id: &str) -> bool {
        self.sessions
            .read()
            .await
            .get(id)
            .is_some_and(|session| !session.handle.is_closed())
    }

    /// An open session to the same account on the same server, if there is one, opened after
    /// `opened_after` when given.
    pub async fn find_live(
        &self,
        host: &str,
        port: u16,
        username: &str,
        opened_after: Option<Instant>,
    ) -> Option<Arc<Session>> {
        self.sessions
            .read()
            .await
            .values()
            .find(|session| {
                let profile = &session.profile;
                profile.host == host
                    && profile.port == port
                    && profile.username == username
                    && opened_after.is_none_or(|since| session.opened_at > since)
                    && !session.handle.is_closed()
            })
            .cloned()
    }

    /// The session's details, for a window taking over a tab; the window becomes its owner.
    pub async fn adopt(&self, id: &str, owner: &str) -> AppResult<SessionInfo> {
        let session = self.get(id).await?;
        *session.owner.lock().unwrap() = owner.to_string();
        Ok(session.info.clone())
    }

    pub async fn disconnect(&self, id: &str) -> AppResult<()> {
        let session = self.sessions.write().await.remove(id);
        self.lost.write().await.remove(id);
        if let Some(session) = session {
            session.fs.close();
            ssh::disconnect(&session.handle).await;
            self.events.log(LogLevel::Info, Some(id), "Disconnected");
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

pub async fn open_sftp(handle: &SshHandle) -> AppResult<RemoteFs> {
    let channel = handle.channel_open_session().await?;
    RemoteFs::open(channel).await
}
