//! `connect` is self-contained (profile in, authenticated handle out) so transfer workers
//! can open extra connections with the same profile.

mod auth;
pub mod keys;
pub mod known_hosts;

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::PublicKeyOrCertificate;
use serde::Deserialize;
use tokio::net::{TcpSocket, TcpStream};

use crate::error::{AppError, AppResult, ErrorKind, HostKeyInfo};
use crate::events::{Events, LogLevel};
use crate::protocol::Protocol;
use crate::settings::{MAX_SOCKET_BUFFER_KIB, MIN_SOCKET_BUFFER_KIB};
use known_hosts::{fingerprint, HostKeyStatus, KnownHosts};

pub const DEFAULT_TIMEOUT_SECS: u64 = 20;
pub const DEFAULT_KEEPALIVE_SECS: u64 = 30;
/// Larger than russh's 2 MiB default so a single download channel is not window-bound on
/// high-latency links.
const CHANNEL_WINDOW_BYTES: u32 = 16 * 1024 * 1024;

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
    /// A Google or Microsoft account signed in through the browser. The frontend names a
    /// fresh sign-in by `grant_id`; the refresh token itself stays in the backend.
    #[serde(rename = "oauth", rename_all = "camelCase")]
    OAuth {
        #[serde(default)]
        grant_id: Option<String>,
        #[serde(skip)]
        refresh_token: String,
        /// The app registration to sign in as, filled from Settings before connecting.
        #[serde(skip)]
        client: Option<crate::cloud::OAuthClient>,
    },
}

/// What to connect to and how. Despite living with the SSH code, it describes a connection
/// of any protocol; fields that do not apply to a protocol are ignored.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectProfile {
    #[serde(default)]
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    #[serde(default)]
    pub initial_path: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub keepalive_secs: Option<u64>,
    #[serde(default)]
    pub compression: bool,
    /// Fixed TCP receive buffer (SO_RCVBUF); `None` leaves it to the system's auto-tuning.
    #[serde(default)]
    pub receive_buffer_kib: Option<u32>,
    /// Fixed TCP send buffer (SO_SNDBUF); `None` leaves it to the system's auto-tuning.
    #[serde(default)]
    pub send_buffer_kib: Option<u32>,
    /// Fills an empty password or passphrase from the system keychain.
    #[serde(default)]
    pub saved_connection_id: Option<String>,
    /// FTP data connections come from the server to Poros (active mode) instead of the other
    /// way round (passive mode).
    #[serde(default)]
    pub ftp_active: bool,
}

impl ConnectProfile {
    /// The password or passphrase is missing, so a saved one may fill it.
    pub fn lacks_secret(&self) -> bool {
        match &self.auth {
            AuthMethod::Password { password } => password.is_empty(),
            AuthMethod::PublicKey { passphrase, .. } => {
                passphrase.as_deref().unwrap_or("").is_empty()
            }
            AuthMethod::Agent => false,
            AuthMethod::OAuth { refresh_token, .. } => refresh_token.is_empty(),
        }
    }

    pub fn set_secret(&mut self, secret: String) {
        match &mut self.auth {
            AuthMethod::Password { password } => *password = secret,
            AuthMethod::PublicKey { passphrase, .. } => *passphrase = Some(secret),
            AuthMethod::Agent => {}
            AuthMethod::OAuth { refresh_token, .. } => *refresh_token = secret,
        }
    }

    /// FTP servers take `anonymous` when no username is given.
    pub fn login_name(&self) -> &str {
        match self.username.trim() {
            "" if self.protocol.is_ftp() => "anonymous",
            username => username,
        }
    }

    pub fn label(&self) -> String {
        if self.protocol.is_cloud() {
            return format!("{} ({})", self.protocol.display_name(), self.username);
        }
        let scheme = match self.protocol {
            Protocol::Sftp => "",
            Protocol::Ftp => "ftp://",
            _ => "ftps://",
        };
        if self.port == self.protocol.default_port() {
            format!("{scheme}{}@{}", self.login_name(), self.host)
        } else {
            format!("{scheme}{}@{}:{}", self.login_name(), self.host, self.port)
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.protocol.is_cloud() {
            return Ok(());
        }
        if self.host.trim().is_empty() {
            return Err(AppError::invalid("Host is required"));
        }
        if self.protocol == Protocol::Sftp && self.username.trim().is_empty() {
            return Err(AppError::invalid("Username is required"));
        }
        if self.port == 0 {
            return Err(AppError::invalid("Port must be between 1 and 65535"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyApproval {
    pub fingerprint: String,
    /// Save to the app's known_hosts. `false` trusts the key for this connection only.
    pub remember: bool,
}

pub type SshHandle = Handle<ClientHandler>;

pub struct Connection {
    pub handle: SshHandle,
    /// The key the server proved it holds. Extra connections pin it, so a host key that was
    /// trusted for this session only is still accepted by transfer workers.
    pub host_key_fingerprint: String,
}

pub struct ClientHandler {
    host: String,
    port: u16,
    session_id: String,
    known_hosts: KnownHosts,
    approval: Option<HostKeyApproval>,
    /// Read by `connect` after a failed handshake to report why the key was rejected.
    rejection: Arc<Mutex<Option<AppError>>>,
    accepted_fingerprint: Arc<Mutex<Option<String>>>,
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
            certificate_problem: None,
        };
        let status = self.known_hosts.check(&self.host, self.port, &key);
        if status == HostKeyStatus::Trusted {
            self.log(
                LogLevel::Info,
                format!(
                    "Host key verified ({} {})",
                    info.algorithm, info.fingerprint
                ),
            );
            *self.accepted_fingerprint.lock().unwrap() = Some(info.fingerprint);
            return Ok(true);
        }

        if let Some(approval) = &self.approval {
            if approval.fingerprint == info.fingerprint {
                if approval.remember {
                    if let Err(error) = self.known_hosts.trust(&self.host, self.port, &key) {
                        self.log(LogLevel::Warn, format!("Could not save host key: {error}"));
                    }
                }
                self.log(
                    LogLevel::Info,
                    format!(
                        "Host key accepted by user ({} {})",
                        info.algorithm, info.fingerprint
                    ),
                );
                *self.accepted_fingerprint.lock().unwrap() = Some(info.fingerprint);
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
            client::DisconnectReason::Error(error) => format!("Connection lost: {error}"),
        };
        self.log(LogLevel::Warn, text.clone());
        self.events.session_closed(&self.session_id, text);
        match reason {
            client::DisconnectReason::ReceivedDisconnect(_) => Ok(()),
            client::DisconnectReason::Error(error) => Err(error),
        }
    }
}

impl ClientHandler {
    fn log(&self, level: LogLevel, message: String) {
        self.events.log(level, Some(&self.session_id), message);
    }
}

pub async fn connect(
    session_id: &str,
    profile: &ConnectProfile,
    known_hosts: &KnownHosts,
    approval: Option<HostKeyApproval>,
    events: &Events,
) -> AppResult<Connection> {
    profile.validate()?;
    let timeout = Duration::from_secs(profile.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let keepalive = profile.keepalive_secs.unwrap_or(DEFAULT_KEEPALIVE_SECS);

    let mut preferred = russh::Preferred::default();
    if profile.compression {
        preferred.compression = Cow::Borrowed(&[
            russh::compression::ZLIB_LEGACY,
            russh::compression::ZLIB,
            russh::compression::NONE,
        ]);
    }
    let config = client::Config {
        client_id: russh::SshId::Standard(
            concat!("SSH-2.0-Poros_", env!("CARGO_PKG_VERSION")).into(),
        ),
        keepalive_interval: (keepalive > 0).then(|| Duration::from_secs(keepalive)),
        keepalive_max: 3,
        nodelay: true,
        window_size: CHANNEL_WINDOW_BYTES,
        preferred,
        ..Default::default()
    };

    let rejection = Arc::new(Mutex::new(None));
    let accepted_fingerprint = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        host: profile.host.trim().to_string(),
        port: profile.port,
        session_id: session_id.to_string(),
        known_hosts: known_hosts.clone(),
        approval,
        rejection: rejection.clone(),
        accepted_fingerprint: accepted_fingerprint.clone(),
        events: events.clone(),
    };

    events.log(
        LogLevel::Info,
        Some(session_id),
        format!("Connecting to {}:{}", profile.host.trim(), profile.port),
    );
    let connecting = async {
        let socket = open_socket(profile).await.map_err(russh::Error::IO)?;
        client::connect_stream(Arc::new(config), socket, handler).await
    };
    let mut handle = match tokio::time::timeout(timeout, connecting).await {
        Err(_) => {
            return Err(AppError::new(
                ErrorKind::Timeout,
                format!(
                    "Timed out after {}s connecting to {}",
                    timeout.as_secs(),
                    profile.host
                ),
            ))
        }
        Ok(Err(error)) => {
            if let Some(rejected) = rejection.lock().unwrap().take() {
                return Err(rejected);
            }
            return Err(connection_error(error, profile));
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
        Err(_) => Err(AppError::new(
            ErrorKind::Timeout,
            "Timed out during authentication",
        )),
        Ok(result) => result,
    }?;
    events.log(LogLevel::Info, Some(session_id), "Authenticated");
    let host_key_fingerprint = accepted_fingerprint
        .lock()
        .unwrap()
        .take()
        .unwrap_or_default();
    Ok(Connection {
        handle,
        host_key_fingerprint,
    })
}

/// Connects to the first address that answers.
pub(crate) async fn open_socket(profile: &ConnectProfile) -> std::io::Result<TcpStream> {
    let mut last_error = None;
    for address in tokio::net::lookup_host((profile.host.trim(), profile.port)).await? {
        match connect_socket(address, profile).await {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "the host name has no address")
    }))
}

/// Buffer sizes are set before connecting so the TCP window scale the handshake negotiates
/// can use them.
pub(crate) async fn connect_socket(
    address: SocketAddr,
    profile: &ConnectProfile,
) -> std::io::Result<TcpStream> {
    let socket = if address.is_ipv4() {
        TcpSocket::new_v4()?
    } else {
        TcpSocket::new_v6()?
    };
    if let Some(kib) = profile.receive_buffer_kib {
        socket.set_recv_buffer_size(socket_buffer_bytes(kib))?;
    }
    if let Some(kib) = profile.send_buffer_kib {
        socket.set_send_buffer_size(socket_buffer_bytes(kib))?;
    }
    let stream = socket.connect(address).await?;
    stream.set_nodelay(true)?;
    Ok(stream)
}

fn socket_buffer_bytes(kib: u32) -> u32 {
    kib.clamp(MIN_SOCKET_BUFFER_KIB, MAX_SOCKET_BUFFER_KIB) * 1024
}

fn connection_error(e: russh::Error, profile: &ConnectProfile) -> AppError {
    match e {
        russh::Error::IO(io) => AppError::new(
            ErrorKind::Connection,
            format!(
                "Could not connect to {}:{}: {io}",
                profile.host, profile.port
            ),
        ),
        other => other.into(),
    }
}

pub async fn disconnect(handle: &SshHandle) {
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;
}
