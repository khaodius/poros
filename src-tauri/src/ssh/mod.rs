//! `connect` is self-contained (profile in, authenticated handle out) so transfer workers
//! can open extra connections with the same profile, including its proxy and jump hosts.

mod auth;
pub mod keys;
pub mod known_hosts;
pub mod proxy;

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::PublicKeyOrCertificate;
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpSocket, TcpStream};

use crate::error::{AppError, AppResult, ErrorKind, HostKeyInfo};
use crate::events::{Events, LogLevel};
use crate::protocol::Protocol;
use crate::settings::{MAX_SOCKET_BUFFER_KIB, MIN_SOCKET_BUFFER_KIB};
use known_hosts::{fingerprint, HostKeyStatus, KnownHosts};
use proxy::{Destination, Proxy};

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
    /// Connects without the proxy from the connection settings.
    #[serde(default)]
    pub bypass_proxy: bool,
    /// A saved connection to tunnel through, as with OpenSSH's ProxyJump.
    #[serde(default)]
    pub jump_connection_id: Option<String>,
    /// How to reach the server. The backend works it out from the settings and the saved
    /// connections, so secrets never pass through the frontend.
    #[serde(skip)]
    pub route: Route,
}

#[derive(Debug, Clone, Default)]
pub struct Route {
    /// Used for the first connection this computer makes: to the outermost jump host, or to
    /// the server when there is none.
    pub proxy: Option<Proxy>,
    /// Servers to tunnel through, outermost first.
    pub jump_hosts: Vec<ConnectProfile>,
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

/// Whether a connection is the one asked for or a jump host on the way to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hop {
    Server,
    JumpHost,
}

pub struct ClientHandler {
    host: String,
    port: u16,
    hop: Hop,
    session_id: String,
    known_hosts: KnownHosts,
    approval: Option<HostKeyApproval>,
    /// Read by `connect` after a failed handshake to report why the key was rejected.
    rejection: Arc<Mutex<Option<AppError>>>,
    accepted_fingerprint: Arc<Mutex<Option<String>>>,
    events: Events,
    /// The jump host this connection runs through, kept open for as long as it is needed.
    _tunnel: Option<SshHandle>,
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
                } else if self.hop == Hop::JumpHost {
                    // Transfer connections and the next attempt, which may stop to ask about
                    // the server behind it, have to get through this jump host again.
                    self.known_hosts
                        .trust_until_exit(&self.host, self.port, &key);
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
        if self.hop == Hop::JumpHost {
            // The connection through it ends too and reports that itself.
            self.log(
                LogLevel::Info,
                format!("Jump host {}:{} closed", self.host, self.port),
            );
            return Ok(());
        }
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

/// Anything an SSH connection can run over: a TCP socket, or a channel through a jump host.
trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Transport for T {}

pub async fn connect(
    session_id: &str,
    profile: &ConnectProfile,
    known_hosts: &KnownHosts,
    approval: Option<HostKeyApproval>,
    events: &Events,
) -> AppResult<Connection> {
    profile.validate()?;
    let attempt = Attempt {
        session_id,
        profile,
        known_hosts,
        approval,
        events,
    };
    let mut tunnel: Option<(SshHandle, String)> = None;
    for jump_host in &profile.route.jump_hosts {
        jump_host.validate()?;
        let transport = attempt
            .open_transport(jump_host, tunnel.as_ref())
            .await
            .map_err(|error| on_jump_host(jump_host, error))?;
        let previous = tunnel.take().map(|(handle, _)| handle);
        let (handle, _) = attempt
            .establish(Hop::JumpHost, jump_host, transport, previous)
            .await
            .map_err(|error| on_jump_host(jump_host, error))?;
        tunnel = Some((handle, jump_host.label()));
    }

    let transport = attempt.open_transport(profile, tunnel.as_ref()).await?;
    let previous = tunnel.map(|(handle, _)| handle);
    let (handle, host_key_fingerprint) = attempt
        .establish(Hop::Server, profile, transport, previous)
        .await?;
    Ok(Connection {
        handle,
        host_key_fingerprint,
    })
}

/// Says which jump host an error came from; host key questions already name it.
fn on_jump_host(jump_host: &ConnectProfile, mut error: AppError) -> AppError {
    if error.host_key.is_none() {
        error.message = format!("Jump host {}: {}", jump_host.label(), error.message);
    }
    error
}

fn client_config(profile: &ConnectProfile) -> client::Config {
    let keepalive = profile.keepalive_secs.unwrap_or(DEFAULT_KEEPALIVE_SECS);
    let mut preferred = russh::Preferred::default();
    if profile.compression {
        preferred.compression = Cow::Borrowed(&[
            russh::compression::ZLIB_LEGACY,
            russh::compression::ZLIB,
            russh::compression::NONE,
        ]);
    }
    client::Config {
        client_id: russh::SshId::Standard(
            concat!("SSH-2.0-Poros_", env!("CARGO_PKG_VERSION")).into(),
        ),
        keepalive_interval: (keepalive > 0).then(|| Duration::from_secs(keepalive)),
        keepalive_max: 3,
        nodelay: true,
        window_size: CHANNEL_WINDOW_BYTES,
        preferred,
        ..Default::default()
    }
}

fn timeout_of(profile: &ConnectProfile) -> Duration {
    Duration::from_secs(profile.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS))
}

/// One connection attempt along a route: what every hop of it shares.
struct Attempt<'a> {
    session_id: &'a str,
    /// The server asked for; its timeouts, buffers and route apply to every hop.
    profile: &'a ConnectProfile,
    known_hosts: &'a KnownHosts,
    approval: Option<HostKeyApproval>,
    events: &'a Events,
}

impl Attempt<'_> {
    /// The byte stream to `hop`: a channel through the previous jump host, or a TCP connection,
    /// through the proxy when there is one.
    async fn open_transport(
        &self,
        hop: &ConnectProfile,
        tunnel: Option<&(SshHandle, String)>,
    ) -> AppResult<Box<dyn Transport>> {
        let profile = self.profile;
        let host = hop.host.trim();
        let log = |message: String| {
            self.events
                .log(LogLevel::Info, Some(self.session_id), message)
        };
        let timed_out = || {
            AppError::new(
                ErrorKind::Timeout,
                format!(
                    "Timed out after {}s connecting to {host}",
                    timeout_of(profile).as_secs()
                ),
            )
        };

        if let Some((jump_handle, jump_label)) = tunnel {
            log(format!(
                "Connecting to {host}:{} through {jump_label}",
                hop.port
            ));
            let opening =
                jump_handle.channel_open_direct_tcpip(host, u32::from(hop.port), "127.0.0.1", 0);
            let channel = tokio::time::timeout(timeout_of(profile), opening)
                .await
                .map_err(|_| timed_out())?
                .map_err(|error| match error {
                    russh::Error::ChannelOpenFailure(reason) => AppError::new(
                        ErrorKind::Connection,
                        format!(
                            "{jump_label} could not reach {host}:{} ({reason:?})",
                            hop.port
                        ),
                    ),
                    other => other.into(),
                })?;
            return Ok(Box::new(channel.into_stream()));
        }

        match &profile.route.proxy {
            Some(proxy) => {
                log(format!(
                    "Connecting to {host}:{} through the {}",
                    hop.port,
                    proxy.label()
                ));
                let connecting = async {
                    let mut socket = open_socket_to(&proxy.host, proxy.port, profile)
                        .await
                        .map_err(|error| {
                            AppError::new(
                                ErrorKind::Connection,
                                format!("Could not connect to the {}: {error}", proxy.label()),
                            )
                        })?;
                    let destination = Destination::resolve(proxy, host, hop.port).await?;
                    proxy::handshake(&mut socket, proxy, &destination, hop.port).await?;
                    AppResult::Ok(socket)
                };
                let socket = tokio::time::timeout(timeout_of(profile), connecting)
                    .await
                    .map_err(|_| timed_out())??;
                Ok(Box::new(socket))
            }
            None => {
                log(format!("Connecting to {host}:{}", hop.port));
                let socket = tokio::time::timeout(
                    timeout_of(profile),
                    open_socket_to(host, hop.port, profile),
                )
                .await
                .map_err(|_| timed_out())?
                .map_err(|error| {
                    AppError::new(
                        ErrorKind::Connection,
                        format!("Could not connect to {host}:{}: {error}", hop.port),
                    )
                })?;
                Ok(Box::new(socket))
            }
        }
    }

    /// Runs the SSH handshake and authentication for one hop over `transport`.
    async fn establish(
        &self,
        hop: Hop,
        hop_profile: &ConnectProfile,
        transport: Box<dyn Transport>,
        tunnel: Option<SshHandle>,
    ) -> AppResult<(SshHandle, String)> {
        let (session_id, events) = (self.session_id, self.events);
        let timeout = timeout_of(self.profile);
        let rejection = Arc::new(Mutex::new(None));
        let accepted_fingerprint = Arc::new(Mutex::new(None));
        let host = hop_profile.host.trim().to_string();
        let mut config = client_config(self.profile);
        config.preferred.key = Cow::Owned(self.known_hosts.preferred_algorithms(
            &host,
            hop_profile.port,
            &config.preferred.key,
        ));
        let handler = ClientHandler {
            host,
            port: hop_profile.port,
            hop,
            session_id: session_id.to_string(),
            known_hosts: self.known_hosts.clone(),
            approval: self.approval.clone(),
            rejection: rejection.clone(),
            accepted_fingerprint: accepted_fingerprint.clone(),
            events: events.clone(),
            _tunnel: tunnel,
        };

        let handshake = client::connect_stream(Arc::new(config), transport, handler);
        let mut handle = match tokio::time::timeout(timeout, handshake).await {
            Err(_) => {
                return Err(AppError::new(
                    ErrorKind::Timeout,
                    format!(
                        "Timed out after {}s connecting to {}",
                        timeout.as_secs(),
                        hop_profile.host.trim()
                    ),
                ))
            }
            Ok(Err(error)) => {
                if let Some(rejected) = rejection.lock().unwrap().take() {
                    return Err(rejected);
                }
                return Err(connection_error(error, hop_profile));
            }
            Ok(Ok(handle)) => handle,
        };

        events.log(
            LogLevel::Info,
            Some(session_id),
            format!("Authenticating as {}", hop_profile.username),
        );
        let authenticating = auth::authenticate(&mut handle, hop_profile, events, session_id);
        match tokio::time::timeout(timeout, authenticating).await {
            Err(_) => Err(AppError::new(
                ErrorKind::Timeout,
                "Timed out during authentication",
            )),
            Ok(result) => result,
        }?;
        events.log(
            LogLevel::Info,
            Some(session_id),
            match hop {
                Hop::Server => "Authenticated".to_string(),
                Hop::JumpHost => format!("Authenticated on jump host {}", hop_profile.label()),
            },
        );
        let host_key_fingerprint = accepted_fingerprint
            .lock()
            .unwrap()
            .take()
            .unwrap_or_default();
        Ok((handle, host_key_fingerprint))
    }
}

/// Connects to the first address that answers.
pub(crate) async fn open_socket(profile: &ConnectProfile) -> std::io::Result<TcpStream> {
    open_socket_to(&profile.host, profile.port, profile).await
}

/// Connects to the first address of `host` that answers, with `profile`'s buffer sizes.
async fn open_socket_to(
    host: &str,
    port: u16,
    profile: &ConnectProfile,
) -> std::io::Result<TcpStream> {
    let mut last_error = None;
    for address in tokio::net::lookup_host((host.trim(), port)).await? {
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
