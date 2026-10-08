//! One FTP control connection (RFC 959 with the RFC 2228, 2428, 3659 and 4217 extensions):
//! replies, login, TLS, and the data connections that listings and transfers run on.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use super::listing::{self, RawEntry};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::model::EntryKind;
use crate::protocol::{Protocol, RemoteStat};
use crate::ssh::{self, AuthMethod, ConnectProfile, DEFAULT_TIMEOUT_SECS};
use crate::timestamp::{format_ftp_time, now_seconds, parse_ftp_time};
use crate::tls::ServerTls;

const MAX_REPLY_LINE: u64 = 64 * 1024;
const MAX_REPLY_LINES: usize = 10_000;
const MAX_LISTING_BYTES: usize = 64 * 1024 * 1024;
/// Servers may take a while to answer once a large upload ends, while they flush or scan it.
const FINAL_REPLY_TIMEOUT_FACTOR: u32 = 6;

/// A control or data connection, in the clear or inside TLS.
pub enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    /// Stands in for the control connection while it switches to TLS.
    Detached,
}

fn detached() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotConnected,
        "the connection is switching to TLS",
    )
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(stream) => Pin::new(stream).poll_read(context, buffer),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_read(context, buffer),
            Stream::Detached => Poll::Ready(Err(detached())),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(stream) => Pin::new(stream).poll_write(context, data),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_write(context, data),
            Stream::Detached => Poll::Ready(Err(detached())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(stream) => Pin::new(stream).poll_flush(context),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(context),
            Stream::Detached => Poll::Ready(Err(detached())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(stream) => Pin::new(stream).poll_shutdown(context),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(context),
            Stream::Detached => Poll::Ready(Ok(())),
        }
    }
}

impl Stream {
    /// Reads into `buffer`. A TLS peer that closes without a close_notify alert counts as the
    /// end of the data; the server's final reply still says whether the transfer completed.
    pub async fn read_some(&mut self, buffer: &mut [u8]) -> AppResult<usize> {
        match self.read(buffer).await {
            Ok(read) => Ok(read),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(0),
            Err(error) => Err(data_error(error)),
        }
    }

    /// Ends an upload: closes the sending side, then reads until the server closes its side.
    /// TLS 1.3 servers send session tickets on data connections, and closing a socket with
    /// unread data resets it, which can lose the end of the file on the server. Reading them
    /// also keeps tickets for later data connections, which servers may require to resume.
    pub async fn finish_sending(mut self, limit: Duration) {
        let _ = self.shutdown().await;
        let mut buffer = [0u8; 4096];
        let _ = tokio::time::timeout(limit, async {
            while matches!(self.read_some(&mut buffer).await, Ok(read) if read > 0) {}
        })
        .await;
    }
}

fn data_error(error: io::Error) -> AppError {
    AppError::new(
        ErrorKind::Connection,
        format!("The data connection failed: {error}"),
    )
}

fn control_error(error: io::Error) -> AppError {
    AppError::new(
        ErrorKind::Disconnected,
        format!("The connection to the server was lost: {error}"),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: u16,
    pub lines: Vec<String>,
}

impl Reply {
    /// The server's words without the reply codes.
    pub fn text(&self) -> String {
        let code = self.code.to_string();
        self.lines
            .iter()
            .map(|line| {
                line.strip_prefix(code.as_str())
                    .map(|rest| rest.trim_start_matches(['-', ' ']))
                    .unwrap_or(line.trim_start())
            })
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn class(&self) -> u16 {
        self.code / 100
    }
}

/// The error for a reply that refused a command.
pub fn reply_error(reply: &Reply) -> AppError {
    let text = reply.text();
    let lower = text.to_ascii_lowercase();
    let kind = match reply.code {
        421 => ErrorKind::Disconnected,
        425 | 426 => ErrorKind::Connection,
        530 | 532 => ErrorKind::AuthFailed,
        450 | 550 if lower.contains("exist") && !lower.contains("not exist") => {
            ErrorKind::AlreadyExists
        }
        450 | 550
            if lower.contains("permission")
                || lower.contains("denied")
                || lower.contains("access") =>
        {
            ErrorKind::PermissionDenied
        }
        550 => ErrorKind::NotFound,
        553 => ErrorKind::PermissionDenied,
        500..=504 => ErrorKind::Unsupported,
        400..=499 => ErrorKind::Ftp,
        _ => ErrorKind::Io,
    };
    let message = if text.is_empty() {
        format!("The server answered {}", reply.code)
    } else {
        text
    };
    AppError::new(kind, message)
}

/// What the server listed in its `FEAT` reply, plus what Poros learned by trying.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Features {
    pub mlsd: bool,
    pub size: bool,
    pub mdtm: bool,
    pub mfmt: bool,
    pub rest_stream: bool,
    pub utf8: bool,
    /// The server can be told to act as the TLS client of a data connection, which encrypted
    /// transfers between two servers need.
    pub sscn: bool,
}

impl Features {
    fn parse(reply: &Reply) -> Self {
        let mut features = Self::default();
        for line in reply.lines.iter().skip(1) {
            let feature = line.trim().to_ascii_uppercase();
            let name = feature.split_whitespace().next().unwrap_or("");
            match name {
                "MLST" | "MLSD" => features.mlsd = true,
                "SIZE" => features.size = true,
                "MDTM" => features.mdtm = true,
                "MFMT" => features.mfmt = true,
                "REST" if feature.contains("STREAM") => features.rest_stream = true,
                "UTF8" => features.utf8 = true,
                "SSCN" => features.sscn = true,
                _ => {}
            }
        }
        features
    }
}

/// Opened before the command that uses it: passive connects out, active waits for the server.
enum PreparedData {
    Passive(TcpStream),
    Active(TcpListener),
}

/// How a command that moves data started.
pub enum DataStart {
    Opened(Stream),
    /// The server answered without opening a data connection, as some do for empty folders.
    Finished,
}

pub struct Control {
    stream: BufReader<Stream>,
    tls: Option<Arc<ServerTls>>,
    pub features: Features,
    encrypted_data: bool,
    profile: ConnectProfile,
    peer: SocketAddr,
    local: SocketAddr,
    timeout: Duration,
    /// Cleared once the server refuses `EPSV`.
    extended_passive: bool,
    /// Cleared once the server refuses to set a time with `MDTM`.
    sets_time_with_mdtm: bool,
    last_used: Instant,
}

impl Control {
    /// Connects, logs in and reads the server's features. Returns the connection and the
    /// folder the server started in.
    pub async fn connect(
        profile: &ConnectProfile,
        tls: Option<ServerTls>,
        session_id: &str,
        events: &Events,
    ) -> AppResult<(Self, String)> {
        profile.validate()?;
        let timeout = Duration::from_secs(profile.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
        let host = profile.host.trim();
        let tcp = match tokio::time::timeout(timeout, ssh::open_socket(profile)).await {
            Err(_) => {
                return Err(AppError::new(
                    ErrorKind::Timeout,
                    format!(
                        "Timed out after {}s connecting to {host}",
                        timeout.as_secs()
                    ),
                ))
            }
            Ok(Err(error)) => {
                return Err(AppError::new(
                    ErrorKind::Connection,
                    format!("Could not connect to {host}:{}: {error}", profile.port),
                ))
            }
            Ok(Ok(tcp)) => tcp,
        };
        let peer = tcp.peer_addr().map_err(control_error)?;
        let local = tcp.local_addr().map_err(control_error)?;
        let tls = tls.map(Arc::new);
        let stream = match (&tls, profile.protocol) {
            (Some(tls), Protocol::FtpsImplicit) => {
                Stream::Tls(Box::new(handshake(tcp, tls, timeout).await?))
            }
            _ => Stream::Plain(tcp),
        };
        let mut control = Self {
            stream: BufReader::new(stream),
            tls,
            features: Features::default(),
            encrypted_data: false,
            profile: profile.clone(),
            peer,
            local,
            timeout,
            extended_passive: true,
            sets_time_with_mdtm: true,
            last_used: Instant::now(),
        };

        let mut welcome = control.read_reply().await?;
        while welcome.code == 120 {
            welcome = control.read_reply().await?;
        }
        if welcome.code != 220 {
            return Err(AppError::new(
                ErrorKind::Connection,
                format!("The server refused the connection: {}", welcome.text()),
            ));
        }
        for line in welcome.lines.iter() {
            let text = line.get(4..).unwrap_or("").trim_end();
            if !text.is_empty() {
                events.log(LogLevel::Server, Some(session_id), text.to_string());
            }
        }

        if profile.protocol == Protocol::Ftps {
            let reply = control.command("AUTH TLS").await?;
            if reply.code != 234 {
                return Err(AppError::unsupported(format!(
                    "The server does not offer TLS: {}",
                    reply.text()
                )));
            }
            control.upgrade_to_tls().await?;
        }

        control.log_in(events, session_id).await?;
        if control.tls.is_some() {
            let _ = control.command("PBSZ 0").await?;
            control.expect("PROT P", 2).await.map_err(|error| {
                AppError::unsupported(format!(
                    "The server would not encrypt file data: {}",
                    error.message
                ))
            })?;
            control.encrypted_data = true;
        }
        let features = control.command("FEAT").await?;
        if features.class() == 2 {
            control.features = Features::parse(&features);
        }
        if control.features.utf8 {
            let _ = control.command("OPTS UTF8 ON").await?;
        }
        control.expect("TYPE I", 2).await?;
        let home = control
            .working_directory()
            .await
            .unwrap_or_else(|_| "/".into());
        Ok((control, home))
    }

    async fn upgrade_to_tls(&mut self) -> AppResult<()> {
        let tls = self
            .tls
            .clone()
            .ok_or_else(|| AppError::invalid("TLS is not set up for this connection"))?;
        let reader = std::mem::replace(&mut self.stream, BufReader::new(Stream::Detached));
        if !reader.buffer().is_empty() {
            return Err(AppError::new(
                ErrorKind::Ftp,
                "The server sent unexpected data before the TLS handshake",
            ));
        }
        let Stream::Plain(tcp) = reader.into_inner() else {
            return Err(AppError::invalid("The connection already uses TLS"));
        };
        let encrypted = handshake(tcp, &tls, self.timeout).await?;
        self.stream = BufReader::new(Stream::Tls(Box::new(encrypted)));
        Ok(())
    }

    async fn log_in(&mut self, events: &Events, session_id: &str) -> AppResult<()> {
        let user = self.profile.login_name().to_string();
        let password = match &self.profile.auth {
            AuthMethod::Password { password } => password.clone(),
            _ => String::new(),
        };
        check_argument(&user)?;
        check_argument(&password)?;
        events.log(
            LogLevel::Info,
            Some(session_id),
            format!("Logging in as {user}"),
        );
        let reply = self.command(&format!("USER {user}")).await?;
        let reply = match reply.code {
            230 => reply,
            331 | 332 => self.command(&format!("PASS {password}")).await?,
            _ => return Err(login_error(&reply)),
        };
        match reply.code {
            230 | 202 => Ok(()),
            332 => Err(AppError::unsupported(
                "The server asks for an account name, which Poros does not send",
            )),
            _ => Err(login_error(&reply)),
        }
    }

    pub fn idle_for(&self) -> Duration {
        self.last_used.elapsed()
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    async fn read_line(&mut self) -> AppResult<String> {
        let mut bytes = Vec::new();
        let read = (&mut self.stream)
            .take(MAX_REPLY_LINE)
            .read_until(b'\n', &mut bytes)
            .await
            .map_err(control_error)?;
        if read == 0 {
            return Err(AppError::new(
                ErrorKind::Disconnected,
                "The server closed the connection",
            ));
        }
        let line = String::from_utf8_lossy(&bytes);
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }

    async fn read_reply_untimed(&mut self) -> AppResult<Reply> {
        let first = self.read_line().await?;
        let code: u16 = first
            .get(..3)
            .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|digits| digits.parse().ok())
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::Ftp,
                    format!("The server sent something that is not an FTP reply: {first}"),
                )
            })?;
        let multiline = first.as_bytes().get(3) == Some(&b'-');
        let mut lines = vec![first];
        if multiline {
            let end = format!("{code} ");
            let bare = code.to_string();
            loop {
                let line = self.read_line().await?;
                let finished = line.starts_with(&end) || line == bare;
                lines.push(line);
                if finished {
                    break;
                }
                if lines.len() > MAX_REPLY_LINES {
                    return Err(AppError::new(
                        ErrorKind::Ftp,
                        "The server's reply is too long",
                    ));
                }
            }
        }
        Ok(Reply { code, lines })
    }

    pub async fn read_reply(&mut self) -> AppResult<Reply> {
        self.read_reply_within(self.timeout).await
    }

    async fn read_reply_within(&mut self, limit: Duration) -> AppResult<Reply> {
        match tokio::time::timeout(limit, self.read_reply_untimed()).await {
            Ok(reply) => reply,
            Err(_) => Err(AppError::new(
                ErrorKind::Timeout,
                "The server did not answer in time",
            )),
        }
    }

    pub async fn send(&mut self, line: &str) -> AppResult<()> {
        self.last_used = Instant::now();
        let stream = self.stream.get_mut();
        stream
            .write_all(format!("{line}\r\n").as_bytes())
            .await
            .map_err(control_error)?;
        stream.flush().await.map_err(control_error)
    }

    pub async fn command(&mut self, line: &str) -> AppResult<Reply> {
        self.send(line).await?;
        self.read_reply().await
    }

    /// Fails with the server's message unless the reply is in `class` (2 for done, 3 for more
    /// to come).
    pub async fn expect(&mut self, line: &str, class: u16) -> AppResult<Reply> {
        let reply = self.command(line).await?;
        if reply.class() == class {
            Ok(reply)
        } else {
            Err(reply_error(&reply))
        }
    }

    pub async fn noop(&mut self) -> AppResult<()> {
        self.expect("NOOP", 2).await.map(|_| ())
    }

    pub async fn quit(&mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(2), self.command("QUIT")).await;
    }

    pub async fn working_directory(&mut self) -> AppResult<String> {
        let reply = self.expect("PWD", 2).await?;
        parse_quoted_path(&reply.text())
            .ok_or_else(|| AppError::new(ErrorKind::Ftp, "The server did not say where it is"))
    }

    /// The server's own spelling of a folder path, which also proves the folder exists.
    pub async fn change_directory(&mut self, path: &str) -> AppResult<String> {
        check_argument(path)?;
        self.expect(&format!("CWD {path}"), 2)
            .await
            .map_err(|error| error.with_path(path))?;
        self.working_directory().await
    }

    async fn prepare_data(&mut self) -> AppResult<PreparedData> {
        if self.profile.ftp_active {
            return self.prepare_active().await;
        }
        if self.extended_passive {
            let reply = self.command("EPSV").await?;
            match (reply.code, parse_epsv_port(&reply.text())) {
                (229, Some(port)) => {
                    let address = SocketAddr::new(self.peer.ip(), port);
                    return Ok(PreparedData::Passive(self.connect_data(address).await?));
                }
                (code, _) if code / 100 == 5 => self.extended_passive = false,
                _ => return Err(reply_error(&reply)),
            }
        }
        if self.peer.is_ipv6() {
            return Err(AppError::unsupported(
                "The server reached over IPv6 refuses extended passive mode (EPSV)",
            ));
        }
        let reply = self.command("PASV").await?;
        let offered = match (reply.code, parse_pasv_address(&reply.text())) {
            (227, Some(address)) => address,
            _ => return Err(reply_error(&reply)),
        };
        let ip = passive_ip(*offered.ip(), self.peer.ip());
        Ok(PreparedData::Passive(
            self.connect_data(SocketAddr::new(ip, offered.port()))
                .await?,
        ))
    }

    async fn connect_data(&self, address: SocketAddr) -> AppResult<TcpStream> {
        match tokio::time::timeout(self.timeout, ssh::connect_socket(address, &self.profile)).await
        {
            Ok(Ok(stream)) => Ok(stream),
            Ok(Err(error)) => Err(AppError::new(
                ErrorKind::Connection,
                format!("Could not open a data connection to {address}: {error}"),
            )),
            Err(_) => Err(AppError::new(
                ErrorKind::Timeout,
                format!("Timed out opening a data connection to {address}"),
            )),
        }
    }

    async fn prepare_active(&mut self) -> AppResult<PreparedData> {
        let listener = TcpListener::bind(SocketAddr::new(self.local.ip(), 0))
            .await
            .map_err(data_error)?;
        let port = listener.local_addr().map_err(data_error)?.port();
        let command = match self.local.ip() {
            IpAddr::V4(ip) => port_command(ip, port),
            IpAddr::V6(ip) => format!("EPRT |2|{ip}|{port}|"),
        };
        self.expect(&command, 2).await?;
        Ok(PreparedData::Active(listener))
    }

    /// Sends a command that moves data and returns the open data connection.
    pub async fn open_transfer(&mut self, command: &str) -> AppResult<DataStart> {
        let prepared = self.prepare_data().await?;
        let reply = self.command(command).await?;
        match reply.class() {
            1 => {}
            2 => return Ok(DataStart::Finished),
            _ => return Err(reply_error(&reply)),
        }
        let tcp = match prepared {
            PreparedData::Passive(stream) => stream,
            PreparedData::Active(listener) => {
                let (stream, address) = tokio::time::timeout(self.timeout, listener.accept())
                    .await
                    .map_err(|_| {
                        AppError::new(
                            ErrorKind::Timeout,
                            "The server did not connect back for the transfer; passive mode may work better",
                        )
                    })?
                    .map_err(data_error)?;
                if address.ip() != self.peer.ip() {
                    return Err(AppError::new(
                        ErrorKind::Connection,
                        format!(
                            "Refused a data connection from {address}, which is not the server"
                        ),
                    ));
                }
                stream
            }
        };
        self.wrap_data(tcp).await.map(DataStart::Opened)
    }

    pub async fn wrap_data(&self, tcp: TcpStream) -> AppResult<Stream> {
        match (&self.tls, self.encrypted_data) {
            (Some(tls), true) => Ok(Stream::Tls(Box::new(
                handshake(tcp, tls, self.timeout).await?,
            ))),
            _ => Ok(Stream::Plain(tcp)),
        }
    }

    /// The reply that ends a transfer, once its data connection is closed.
    pub async fn finish_transfer(&mut self) -> AppResult<()> {
        let reply = self
            .read_reply_within(self.timeout * FINAL_REPLY_TIMEOUT_FACTOR)
            .await?;
        if reply.class() == 2 {
            Ok(())
        } else {
            Err(reply_error(&reply))
        }
    }

    /// The reply that ends a transfer between two servers. It comes once the whole file has
    /// moved, however long that takes, so only cancelling the transfer stops the wait.
    pub async fn finish_direct_transfer(&mut self) -> AppResult<()> {
        let reply = self.read_reply_untimed().await?;
        if reply.class() == 2 {
            Ok(())
        } else {
            Err(reply_error(&reply))
        }
    }

    /// Stops a transfer early. Servers answer an abort in different ways (426 then 226, or
    /// 226 alone, or 225), so a `NOOP` afterwards finds where the replies end. Some servers
    /// read commands in bulk during a transfer and drop whatever follows `ABOR`, so the
    /// `NOOP` waits for the first answer.
    pub async fn abort_transfer(&mut self, data: Stream) -> AppResult<()> {
        self.send("ABOR").await?;
        drop(data);
        let first = self.read_reply().await?;
        if first.code == 421 {
            return Err(reply_error(&first));
        }
        self.resynchronize().await
    }

    async fn resynchronize(&mut self) -> AppResult<()> {
        self.send("NOOP").await?;
        for _ in 0..6 {
            let reply = self.read_reply().await?;
            if reply.code == 200 {
                return Ok(());
            }
            if reply.code == 421 {
                return Err(reply_error(&reply));
            }
        }
        Err(AppError::new(
            ErrorKind::Disconnected,
            "Lost track of the server's replies",
        ))
    }

    async fn read_listing(&mut self, start: DataStart) -> AppResult<String> {
        let Some(mut data) = (match start {
            DataStart::Opened(data) => Some(data),
            DataStart::Finished => None,
        }) else {
            return Ok(String::new());
        };
        let mut bytes = Vec::new();
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let read = data.read_some(&mut buffer).await?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if bytes.len() > MAX_LISTING_BYTES {
                return Err(AppError::new(
                    ErrorKind::Ftp,
                    "The folder listing is too large",
                ));
            }
        }
        drop(data);
        self.finish_transfer().await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub async fn list(&mut self, directory: &str) -> AppResult<Vec<RawEntry>> {
        check_argument(directory)?;
        if self.features.mlsd {
            match self.open_transfer(&format!("MLSD {directory}")).await {
                Ok(start) => {
                    let text = self.read_listing(start).await?;
                    return Ok(listing::parse_mlsd(&text));
                }
                Err(error) if error.kind == ErrorKind::Unsupported => self.features.mlsd = false,
                Err(error) => return Err(error.with_path(directory)),
            }
        }
        // LIST output is meant for people and its path argument is not always honored, so list
        // the working folder instead.
        self.change_directory(directory).await?;
        let start = match self.open_transfer("LIST -a").await {
            Err(error) if error.kind == ErrorKind::Unsupported => self.open_transfer("LIST").await,
            other => other,
        }
        .map_err(|error| error.with_path(directory))?;
        let text = self.read_listing(start).await?;
        Ok(listing::parse_list(&text, now_seconds()))
    }

    /// `None` when nothing is at `path`.
    pub async fn stat(&mut self, path: &str) -> AppResult<Option<RemoteStat>> {
        check_argument(path)?;
        if self.features.mlsd {
            let reply = self.command(&format!("MLST {path}")).await?;
            match reply.code {
                250 => {
                    let facts = reply
                        .lines
                        .iter()
                        .skip(1)
                        .find(|line| line.starts_with(' '))
                        .map(|line| line.trim_start());
                    // Facts about a link describe the link itself, which `SIZE` and `CWD`
                    // below see through.
                    if let Some(entry) = facts
                        .and_then(listing::parse_mlsd_line)
                        .filter(|entry| entry.kind != EntryKind::Symlink)
                    {
                        return Ok(Some(entry.stat()));
                    }
                }
                450 | 550 => return Ok(None),
                500..=504 => self.features.mlsd = false,
                _ => return Err(reply_error(&reply).with_path(path)),
            }
        }
        if self.features.size {
            let reply = self.command(&format!("SIZE {path}")).await?;
            if reply.code == 213 {
                let size = reply.text().trim().parse().unwrap_or(0);
                let modified = self.modified_time(path).await?;
                return Ok(Some(RemoteStat {
                    is_dir: false,
                    size,
                    modified,
                    permissions: None,
                }));
            }
        }
        if self.command(&format!("CWD {path}")).await?.class() == 2 {
            return Ok(Some(RemoteStat {
                is_dir: true,
                size: 0,
                modified: None,
                permissions: None,
            }));
        }
        if self.features.size {
            return Ok(None);
        }
        // Without SIZE, the parent's listing is the only way to learn about a file.
        let Some(parent) = crate::remote_path::parent(path) else {
            return Ok(None);
        };
        let name = crate::remote_path::file_name(path);
        Ok(self
            .list(&parent)
            .await?
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.stat()))
    }

    async fn modified_time(&mut self, path: &str) -> AppResult<Option<i64>> {
        if !self.features.mdtm {
            return Ok(None);
        }
        let reply = self.command(&format!("MDTM {path}")).await?;
        Ok((reply.code == 213)
            .then(|| parse_ftp_time(&reply.text()))
            .flatten())
    }

    /// Whether a symbolic link leads to a folder, a file, or nowhere.
    pub async fn link_target(&mut self, path: &str) -> AppResult<(EntryKind, u64)> {
        check_argument(path)?;
        if self.command(&format!("CWD {path}")).await?.class() == 2 {
            return Ok((EntryKind::Dir, 0));
        }
        if self.features.size {
            let reply = self.command(&format!("SIZE {path}")).await?;
            if reply.code == 213 {
                return Ok((EntryKind::File, reply.text().trim().parse().unwrap_or(0)));
            }
            return Ok((EntryKind::Other, 0));
        }
        Ok((EntryKind::File, 0))
    }

    pub async fn make_dir(&mut self, path: &str) -> AppResult<()> {
        check_argument(path)?;
        self.expect(&format!("MKD {path}"), 2)
            .await
            .map(|_| ())
            .map_err(|error| error.with_path(path))
    }

    /// Moves to the folder above `path`, in case the session is in it or below it. Some
    /// servers cannot remove that folder, and others lose their place once it moves away.
    async fn step_out_of(&mut self, path: &str) -> AppResult<()> {
        if let Some(parent) = crate::remote_path::parent(path) {
            self.command(&format!("CWD {parent}")).await?;
        }
        Ok(())
    }

    pub async fn rename(&mut self, from: &str, to: &str) -> AppResult<()> {
        check_argument(from)?;
        check_argument(to)?;
        self.step_out_of(from).await?;
        self.expect(&format!("RNFR {from}"), 3)
            .await
            .map_err(|error| error.with_path(from))?;
        self.expect(&format!("RNTO {to}"), 2)
            .await
            .map(|_| ())
            .map_err(|error| error.with_path(from))
    }

    pub async fn delete_file(&mut self, path: &str) -> AppResult<()> {
        check_argument(path)?;
        self.expect(&format!("DELE {path}"), 2)
            .await
            .map(|_| ())
            .map_err(|error| error.with_path(path))
    }

    pub async fn remove_dir(&mut self, path: &str) -> AppResult<()> {
        check_argument(path)?;
        self.step_out_of(path).await?;
        self.expect(&format!("RMD {path}"), 2)
            .await
            .map(|_| ())
            .map_err(|error| error.with_path(path))
    }

    /// Uses `MFMT`, or else the older `MDTM <time> <path>` form that servers such as vsftpd
    /// and ProFTPD take. A server without either keeps the time it wrote the file at.
    pub async fn set_modified(&mut self, path: &str, modified: i64) -> AppResult<()> {
        check_argument(path)?;
        let time = format_ftp_time(modified);
        if self.features.mfmt {
            return self
                .expect(&format!("MFMT {time} {path}"), 2)
                .await
                .map(|_| ());
        }
        if self.features.mdtm && self.sets_time_with_mdtm {
            let reply = self.command(&format!("MDTM {time} {path}")).await?;
            // A server that only reads times takes the whole argument for a file name.
            self.sets_time_with_mdtm = reply.class() == 2;
        }
        Ok(())
    }

    pub async fn set_permissions(&mut self, path: &str, permissions: u32) -> AppResult<()> {
        check_argument(path)?;
        self.expect(&format!("SITE CHMOD {:o} {path}", permissions & 0o7777), 2)
            .await
            .map(|_| ())
    }

    /// Starts sending a file from `offset`.
    pub async fn retrieve(&mut self, path: &str, offset: u64) -> AppResult<DataStart> {
        check_argument(path)?;
        if offset > 0 {
            self.expect(&format!("REST {offset}"), 3).await?;
        }
        self.open_transfer(&format!("RETR {path}"))
            .await
            .map_err(|error| error.with_path(path))
    }

    /// Starts receiving a file, replacing it from `offset` on.
    pub async fn store(&mut self, path: &str, offset: u64) -> AppResult<DataStart> {
        check_argument(path)?;
        if offset > 0 {
            self.expect(&format!("REST {offset}"), 3).await?;
        }
        self.open_transfer(&format!("STOR {path}"))
            .await
            .map_err(|error| error.with_path(path))
    }

    /// The address the server listens on after `PASV`, as the other server of a
    /// server-to-server transfer must reach it.
    pub async fn passive_address(&mut self) -> AppResult<SocketAddr> {
        let reply = self.command("PASV").await?;
        let offered = match (reply.code, parse_pasv_address(&reply.text())) {
            (227, Some(address)) => address,
            _ => return Err(reply_error(&reply)),
        };
        Ok(SocketAddr::new(
            passive_ip(*offered.ip(), self.peer.ip()),
            offered.port(),
        ))
    }

    /// The fingerprint of the TLS certificate this connection accepted.
    pub fn certificate_fingerprint(&self) -> Option<String> {
        self.tls.as_ref().and_then(|tls| tls.accepted_fingerprint())
    }

    pub fn is_encrypted(&self) -> bool {
        self.encrypted_data
    }
}

async fn handshake(
    tcp: TcpStream,
    tls: &ServerTls,
    timeout: Duration,
) -> AppResult<TlsStream<TcpStream>> {
    let connector = TlsConnector::from(tls.config.clone());
    match tokio::time::timeout(timeout, connector.connect(tls.server_name.clone(), tcp)).await {
        Err(_) => Err(AppError::new(
            ErrorKind::Timeout,
            "Timed out during the TLS handshake",
        )),
        Ok(Err(error)) => Err(tls.rejection().unwrap_or_else(|| {
            AppError::new(
                ErrorKind::Connection,
                format!("The TLS handshake failed: {error}"),
            )
        })),
        Ok(Ok(stream)) => Ok(stream),
    }
}

/// Commands end at a line break, so a path or password with one could smuggle in another.
pub fn check_argument(argument: &str) -> AppResult<()> {
    if argument.contains(['\r', '\n', '\0']) {
        Err(AppError::invalid(
            "Names sent to an FTP server cannot contain line breaks",
        ))
    } else {
        Ok(())
    }
}

fn login_error(reply: &Reply) -> AppError {
    AppError::new(
        ErrorKind::AuthFailed,
        format!("The server refused the login: {}", reply.text()),
    )
}

pub fn port_command(ip: Ipv4Addr, port: u16) -> String {
    let [first, second, third, fourth] = ip.octets();
    format!(
        "PORT {first},{second},{third},{fourth},{},{}",
        port >> 8,
        port & 0xff
    )
}

/// `257 "/home/o""brien" is the current directory`: doubled quotes stand for one.
fn parse_quoted_path(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let mut path = String::new();
    let mut characters = text[start..].chars().peekable();
    while let Some(character) = characters.next() {
        if character == '"' {
            if characters.peek() == Some(&'"') {
                characters.next();
                path.push('"');
            } else {
                return Some(path);
            }
        } else {
            path.push(character);
        }
    }
    None
}

/// `Entering Extended Passive Mode (|||6446|)`.
fn parse_epsv_port(text: &str) -> Option<u16> {
    let start = text.find('(')? + 1;
    let end = text[start..].find(')')? + start;
    let inner = &text[start..end];
    let delimiter = inner.chars().next()?;
    let parts: Vec<&str> = inner.split(delimiter).collect();
    parts.get(3)?.parse().ok()
}

/// `Entering Passive Mode (192,168,1,2,19,136)`; some servers leave out the parentheses.
fn parse_pasv_address(text: &str) -> Option<std::net::SocketAddrV4> {
    let numbers: Vec<u16> = text
        .split(|character: char| !character.is_ascii_digit() && character != ',')
        .find(|part| part.matches(',').count() == 5)?
        .split(',')
        .map(|number| number.parse().ok())
        .collect::<Option<Vec<u16>>>()?;
    if numbers.iter().any(|number| *number > 255) {
        return None;
    }
    let ip = Ipv4Addr::new(
        numbers[0] as u8,
        numbers[1] as u8,
        numbers[2] as u8,
        numbers[3] as u8,
    );
    Some(std::net::SocketAddrV4::new(
        ip,
        numbers[4] << 8 | numbers[5],
    ))
}

fn is_unroutable(ip: Ipv4Addr) -> bool {
    let [first, second, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || (first == 100 && (64..128).contains(&second))
}

/// Servers behind NAT often offer their private address for passive mode; the address Poros
/// reached them at works instead.
fn passive_ip(offered: Ipv4Addr, peer: IpAddr) -> IpAddr {
    let peer_unroutable = match peer {
        IpAddr::V4(peer) => is_unroutable(peer),
        IpAddr::V6(peer) => peer.is_loopback(),
    };
    if offered.is_unspecified() || (is_unroutable(offered) && !peer_unroutable) {
        peer
    } else {
        IpAddr::V4(offered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_text_drops_codes() {
        let reply = Reply {
            code: 211,
            lines: vec![
                "211-Features:".into(),
                " MDTM".into(),
                " REST STREAM".into(),
                " SIZE".into(),
                " MLST type*;size*;modify*;".into(),
                " UTF8".into(),
                "211 End".into(),
            ],
        };
        let features = Features::parse(&reply);
        assert!(features.mdtm && features.rest_stream && features.size && features.mlsd);
        assert!(features.utf8 && !features.mfmt && !features.sscn);
        assert_eq!(
            Reply {
                code: 550,
                lines: vec!["550 Permission denied.".into()]
            }
            .text(),
            "Permission denied."
        );
    }

    #[test]
    fn maps_reply_codes_to_errors() {
        let reply = |code: u16, text: &str| Reply {
            code,
            lines: vec![format!("{code} {text}")],
        };
        assert_eq!(
            reply_error(&reply(550, "No such file or directory")).kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            reply_error(&reply(550, "Permission denied")).kind,
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            reply_error(&reply(550, "Directory already exists")).kind,
            ErrorKind::AlreadyExists
        );
        assert_eq!(
            reply_error(&reply(421, "Timeout")).kind,
            ErrorKind::Disconnected
        );
        assert_eq!(
            reply_error(&reply(500, "Unknown command")).kind,
            ErrorKind::Unsupported
        );
        assert_eq!(reply_error(&reply(451, "Local error")).kind, ErrorKind::Ftp);
        assert!(reply_error(&reply(426, "Connection closed")).is_retryable());
    }

    #[test]
    fn parses_passive_replies() {
        assert_eq!(
            parse_pasv_address("Entering Passive Mode (192,168,1,2,19,136)."),
            Some("192.168.1.2:5000".parse().unwrap())
        );
        assert_eq!(
            parse_pasv_address("Entering Passive Mode 10,0,0,1,4,1"),
            Some("10.0.0.1:1025".parse().unwrap())
        );
        assert_eq!(parse_pasv_address("Entering Passive Mode (1,2,3)"), None);
        assert_eq!(
            parse_epsv_port("Entering Extended Passive Mode (|||6446|)"),
            Some(6446)
        );
        assert_eq!(parse_epsv_port("Entering Extended Passive Mode"), None);
        assert_eq!(
            port_command(Ipv4Addr::new(10, 0, 0, 1), 1025),
            "PORT 10,0,0,1,4,1"
        );
    }

    #[test]
    fn replaces_private_passive_addresses() {
        let public: IpAddr = "203.0.113.5".parse().unwrap();
        assert_eq!(passive_ip(Ipv4Addr::new(192, 168, 1, 2), public), public);
        assert_eq!(passive_ip(Ipv4Addr::UNSPECIFIED, public), public);
        assert_eq!(
            passive_ip(Ipv4Addr::new(198, 51, 100, 7), public),
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7))
        );
        let private: IpAddr = "192.168.1.9".parse().unwrap();
        assert_eq!(
            passive_ip(Ipv4Addr::new(192, 168, 1, 2), private),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))
        );
    }

    #[test]
    fn parses_quoted_paths() {
        assert_eq!(
            parse_quoted_path("\"/home/o\"\"brien\" is the current directory").as_deref(),
            Some("/home/o\"brien")
        );
        assert_eq!(parse_quoted_path("no quotes"), None);
        assert!(check_argument("a\r\nDELE b").is_err());
    }
}
