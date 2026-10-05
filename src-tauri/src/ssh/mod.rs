//! SSH transport: connection setup, host key verification and user authentication.
//!
//! `connect` is deliberately self-contained (profile in, authenticated handle out) so the
//! transfer engine can open extra connections for parallel workers with the same profile.

mod auth;
pub mod keys;
pub mod known_hosts;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::PublicKeyOrCertificate;
use serde::Deserialize;

use crate::error::{AppError, AppResult, ErrorKind, HostKeyInfo};
use crate::events::{Events, LogLevel};
use known_hosts::{fingerprint, HostKeyStatus, KnownHosts};

pub const DEFAULT_TIMEOUT_SECS: u64 = 20;
pub const DEFAULT_KEEPALIVE_SECS: u64 = 30;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthMethod {
    Password {
        password: String,
    },
    #[serde(rename_all = "camelCase")]
    PublicKey {
        key_path: String,
        passphrase: Option<String>,
    },
    Agent,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectProfile {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    /// Remote directory to open after connecting. Defaults to the login directory.
    #[serde(default)]
    pub initial_path: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub keepalive_secs: Option<u64>,
}

impl ConnectProfile {
    pub fn label(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.username, self.host)
        } else {
            format!("{}@{}:{}", self.username, self.host, self.port)
        }
    }

    fn validate(&self) -> AppResult<()> {
        if self.host.trim().is_empty() {
            return Err(AppError::invalid("Host is required"));
        }
        if self.username.trim().is_empty() {
            return Err(AppError::invalid("Username is required"));
        }
        if self.port == 0 {
            return Err(AppError::invalid("Port must be between 1 and 65535"));
        }
        Ok(())
    }
}

/// The user's answer to a host key prompt, sent back on the retried connect.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyApproval {
    pub fingerprint: String,
    /// Save to the app's known_hosts. `false` trusts the key for this connection only.
    pub remember: bool,
}

pub type SshHandle = Handle<ClientHandler>;

pub struct ClientHandler {
    host: String,
    port: u16,
    session_id: String,
    known_hosts: KnownHosts,
    approval: Option<HostKeyApproval>,
    /// Why the host key was rejected, read by `connect` after the handshake fails.
    rejection: Arc<Mutex<Option<AppError>>>,
    events: Events,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_key.public_key();
        let info = HostKeyInfo {
            host: self.host.clone(),
            port: self.port,
            algorithm: key.algorithm().as_str().to_string(),
            fingerprint: fingerprint(&key),
        };
        let status = self.known_hosts.check(&self.host, self.port, &key);
        if status == HostKeyStatus::Trusted {
            self.log(
                LogLevel::Info,
                format!("Host key verified ({} {})", info.algorithm, info.fingerprint),
            );
            return Ok(true);
        }

        if let Some(approval) = &self.approval {
            if approval.fingerprint == info.fingerprint {
                if approval.remember {
                    if let Err(e) = self.known_hosts.trust(&self.host, self.port, &key) {
                        self.log(LogLevel::Warn, format!("Could not save host key: {e}"));
                    }
                }
                self.log(
                    LogLevel::Info,
                    format!("Host key accepted by user ({} {})", info.algorithm, info.fingerprint),
                );
                return Ok(true);
            }
        }

        let kind = match status {
            HostKeyStatus::Changed => ErrorKind::HostKeyChanged,
            _ => ErrorKind::HostKeyUnknown,
        };
        *self.rejection.lock().unwrap() = Some(AppError::host_key(kind, info));
        Ok(false)
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let banner = banner.trim_end();
        if !banner.is_empty() {
            self.log(LogLevel::Server, banner.to_string());
        }
        Ok(())
    }

    async fn disconnected(
        &mut self,
        reason: client::DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        let text = match &reason {
            client::DisconnectReason::ReceivedDisconnect(info) => {
                format!("Server closed the connection: {}", info.message)
            }
            client::DisconnectReason::Error(e) => format!("Connection lost: {e}"),
        };
        self.log(LogLevel::Warn, text.clone());
        self.events.session_closed(&self.session_id, text);
        match reason {
            client::DisconnectReason::ReceivedDisconnect(_) => Ok(()),
            client::DisconnectReason::Error(e) => Err(e),
        }
    }
}

impl ClientHandler {
    fn log(&self, level: LogLevel, message: String) {
        self.events.log(level, Some(&self.session_id), message);
    }
}

/// Opens a TCP connection, verifies the host key and authenticates.
pub async fn connect(
    session_id: &str,
    profile: &ConnectProfile,
    known_hosts: &KnownHosts,
    approval: Option<HostKeyApproval>,
    events: &Events,
) -> AppResult<SshHandle> {
    profile.validate()?;
    let timeout = Duration::from_secs(profile.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let keepalive = profile.keepalive_secs.unwrap_or(DEFAULT_KEEPALIVE_SECS);

    let config = client::Config {
        client_id: russh::SshId::Standard(
            concat!("SSH-2.0-Poros_", env!("CARGO_PKG_VERSION")).into(),
        ),
        keepalive_interval: (keepalive > 0).then(|| Duration::from_secs(keepalive)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    };

    let rejection = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        host: profile.host.trim().to_string(),
        port: profile.port,
        session_id: session_id.to_string(),
        known_hosts: known_hosts.clone(),
        approval,
        rejection: rejection.clone(),
        events: events.clone(),
    };

    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("Connecting to {}:{}", profile.host.trim(), profile.port),
    );
    let addr = (profile.host.trim().to_string(), profile.port);
    let connecting = client::connect(Arc::new(config), addr, handler);
    let mut handle = match tokio::time::timeout(timeout, connecting).await {
        Err(_) => {
            return Err(AppError::new(
                ErrorKind::Timeout,
                format!("Timed out after {}s connecting to {}", timeout.as_secs(), profile.host),
            ))
        }
        Ok(Err(e)) => {
            if let Some(rejected) = rejection.lock().unwrap().take() {
                return Err(rejected);
            }
            return Err(connection_error(e, profile));
        }
        Ok(Ok(handle)) => handle,
    };

    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("Authenticating as {}", profile.username),
    );
    let authenticating = auth::authenticate(&mut handle, profile, events, session_id);
    match tokio::time::timeout(timeout, authenticating).await {
        Err(_) => Err(AppError::new(ErrorKind::Timeout, "Timed out during authentication")),
        Ok(result) => result,
    }?;
    events.log(LogLevel::Info, Some(session_id), "Authenticated");
    Ok(handle)
}

fn connection_error(e: russh::Error, profile: &ConnectProfile) -> AppError {
    match e {
        russh::Error::IO(io) => AppError::new(
            ErrorKind::Connection,
            format!("Could not connect to {}:{}: {io}", profile.host, profile.port),
        ),
        other => other.into(),
    }
}

pub async fn disconnect(handle: &SshHandle) {
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;
}
