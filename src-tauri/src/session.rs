use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
    handle: SshHandle,
    pub fs: RemoteFs,
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
}

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    known_hosts: KnownHosts,
    events: Events,
}

impl SessionManager {
    pub fn new(known_hosts_file: PathBuf, events: Events) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            known_hosts: KnownHosts::with_defaults(known_hosts_file),
            events,
        }
    }

    pub async fn connect(
        &self,
        profile: ConnectProfile,
        approval: Option<HostKeyApproval>,
    ) -> AppResult<SessionInfo> {
        let id = uuid::Uuid::new_v4().to_string();
        let result = self.open(&id, profile, approval).await;
        if let Err(error) = &result {
            let level = match error.kind {
                ErrorKind::HostKeyUnknown | ErrorKind::PassphraseRequired => LogLevel::Warn,
                _ => LogLevel::Error,
            };
            self.events.log(level, Some(&id), error.message.clone());
        }
        result
    }

    async fn open(
        &self,
        id: &str,
        profile: ConnectProfile,
        approval: Option<HostKeyApproval>,
    ) -> AppResult<SessionInfo> {
        let handle = ssh::connect(id, &profile, &self.known_hosts, approval, &self.events).await?;
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
        };
        self.events.log(
            LogLevel::Info,
            Some(id),
            format!("SFTP session ready, home directory {}", fs.home),
        );
        let session = Arc::new(Session {
            id: id.to_string(),
            profile,
            handle,
            fs,
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
            self.sessions.write().await.remove(id);
            return Err(AppError::new(
                crate::error::ErrorKind::Disconnected,
                "The connection was closed",
            ));
        }
        Ok(session)
    }

    pub async fn disconnect(&self, id: &str) -> AppResult<()> {
        let session = self.sessions.write().await.remove(id);
        if let Some(session) = session {
            session.fs.close();
            ssh::disconnect(&session.handle).await;
            self.events.log(LogLevel::Info, Some(id), "Disconnected");
        }
        Ok(())
    }

    pub async fn disconnect_all(&self) {
        let ids: Vec<String> = self.sessions.read().await.keys().cloned().collect();
        for id in ids {
            let _ = self.disconnect(&id).await;
        }
    }
}

async fn open_sftp(handle: &SshHandle) -> AppResult<RemoteFs> {
    let channel = handle.channel_open_session().await?;
    RemoteFs::open(channel).await
}
