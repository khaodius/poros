//! Asks a SOCKS5, SOCKS4 or HTTP proxy to open the TCP connection to the first SSH server.

use std::net::IpAddr;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{AppError, AppResult, ErrorKind};

/// Longest HTTP response head accepted from a proxy.
const MAX_HTTP_HEAD_BYTES: usize = 16 * 1024;

const SOCKS5_VERSION: u8 = 5;
const SOCKS5_NO_AUTHENTICATION: u8 = 0x00;
const SOCKS5_USERNAME_PASSWORD: u8 = 0x02;
const SOCKS5_NO_ACCEPTABLE_METHOD: u8 = 0xff;
const SOCKS5_CONNECT: u8 = 0x01;
const SOCKS5_IPV4: u8 = 0x01;
const SOCKS5_DOMAIN: u8 = 0x03;
const SOCKS5_IPV6: u8 = 0x04;
const SOCKS4_VERSION: u8 = 4;
const SOCKS4_CONNECT: u8 = 0x01;
const SOCKS4_GRANTED: u8 = 0x5a;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyKind {
    #[default]
    None,
    Socks5,
    /// SOCKS4, or SOCKS4a when the proxy looks up names.
    Socks4,
    /// An HTTP proxy that allows the CONNECT method.
    Http,
}

impl ProxyKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "no proxy",
            Self::Socks5 => "SOCKS5",
            Self::Socks4 => "SOCKS4",
            Self::Http => "HTTP",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Http => 8080,
            _ => 1080,
        }
    }
}

/// A proxy to reach servers through, with its password when it has one.
#[derive(Clone, PartialEq, Eq)]
pub struct Proxy {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// The proxy looks up the server's name, so this computer's DNS never sees it. HTTP
    /// proxies always do.
    pub remote_dns: bool,
}

impl std::fmt::Debug for Proxy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Proxy")
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("remote_dns", &self.remote_dns)
            .finish_non_exhaustive()
    }
}

impl Proxy {
    pub fn label(&self) -> String {
        format!("{} proxy {}:{}", self.kind.label(), self.host, self.port)
    }
}

/// Where the proxy should connect: a name it looks up itself, or an address found here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Name(String),
    Address(IpAddr),
}

impl Destination {
    /// Looks the name up on this computer unless the proxy is to do it.
    pub async fn resolve(proxy: &Proxy, host: &str, port: u16) -> AppResult<Self> {
        let host = host.trim();
        if let Ok(address) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
            return Ok(Self::Address(address));
        }
        if proxy.remote_dns || proxy.kind == ProxyKind::Http {
            return Ok(Self::Name(host.to_string()));
        }
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorKind::Connection,
                    format!("Could not look up {host}: {error}"),
                )
            })?;
        let mut first = None;
        for address in addresses {
            // SOCKS4 can only carry IPv4 addresses.
            if proxy.kind != ProxyKind::Socks4 || address.is_ipv4() {
                first = Some(address.ip());
                break;
            }
        }
        first.map(Self::Address).ok_or_else(|| {
            AppError::new(
                ErrorKind::Connection,
                format!("{host} has no address the {} can use", proxy.label()),
            )
        })
    }
}

/// Asks the proxy at the other end of `stream` to connect to the destination. Afterwards the
/// stream carries that connection.
pub async fn handshake<S>(
    stream: &mut S,
    proxy: &Proxy,
    destination: &Destination,
    port: u16,
) -> AppResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let result = match proxy.kind {
        ProxyKind::Socks5 => socks5(stream, proxy, destination, port).await,
        ProxyKind::Socks4 => socks4(stream, proxy, destination, port).await,
        ProxyKind::Http => http_connect(stream, proxy, destination, port).await,
        ProxyKind::None => Ok(()),
    };
    result.map_err(|mut error| {
        error.message = format!("{}: {}", proxy.label(), error.message);
        error
    })
}

async fn socks5<S>(
    stream: &mut S,
    proxy: &Proxy,
    destination: &Destination,
    port: u16,
) -> AppResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let with_login = !proxy.username.is_empty();
    let greeting: &[u8] = if with_login {
        &[
            SOCKS5_VERSION,
            2,
            SOCKS5_NO_AUTHENTICATION,
            SOCKS5_USERNAME_PASSWORD,
        ]
    } else {
        &[SOCKS5_VERSION, 1, SOCKS5_NO_AUTHENTICATION]
    };
    stream.write_all(greeting).await.map_err(io_error)?;
    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await.map_err(io_error)?;
    if choice[0] != SOCKS5_VERSION {
        return Err(protocol_error("it did not answer as a SOCKS5 proxy"));
    }
    match choice[1] {
        SOCKS5_NO_AUTHENTICATION => {}
        SOCKS5_USERNAME_PASSWORD if with_login => {
            socks5_login(stream, proxy).await?;
        }
        SOCKS5_NO_ACCEPTABLE_METHOD if !with_login => {
            return Err(AppError::new(
                ErrorKind::AuthFailed,
                "it requires a username and password",
            ))
        }
        _ => {
            return Err(AppError::new(
                ErrorKind::AuthFailed,
                "it accepts none of the sign-in methods Poros offers",
            ))
        }
    }

    let mut request = vec![SOCKS5_VERSION, SOCKS5_CONNECT, 0];
    match destination {
        Destination::Address(IpAddr::V4(address)) => {
            request.push(SOCKS5_IPV4);
            request.extend_from_slice(&address.octets());
        }
        Destination::Address(IpAddr::V6(address)) => {
            request.push(SOCKS5_IPV6);
            request.extend_from_slice(&address.octets());
        }
        Destination::Name(name) => {
            let length = u8::try_from(name.len())
                .map_err(|_| AppError::invalid("the host name is too long for SOCKS5"))?;
            request.push(SOCKS5_DOMAIN);
            request.push(length);
            request.extend_from_slice(name.as_bytes());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await.map_err(io_error)?;

    let mut reply = [0u8; 4];
    stream.read_exact(&mut reply).await.map_err(io_error)?;
    if reply[0] != SOCKS5_VERSION {
        return Err(protocol_error("it did not answer as a SOCKS5 proxy"));
    }
    if reply[1] != 0 {
        return Err(AppError::new(
            ErrorKind::Connection,
            socks5_failure(reply[1]),
        ));
    }
    // The address the proxy connected from, which nothing here needs.
    let bound_length = match reply[3] {
        SOCKS5_IPV4 => 4,
        SOCKS5_IPV6 => 16,
        SOCKS5_DOMAIN => usize::from(stream.read_u8().await.map_err(io_error)?),
        _ => return Err(protocol_error("its reply has an unknown address type")),
    };
    let mut bound = vec![0u8; bound_length + 2];
    stream.read_exact(&mut bound).await.map_err(io_error)?;
    Ok(())
}

/// RFC 1929 username and password sign-in.
async fn socks5_login<S>(stream: &mut S, proxy: &Proxy) -> AppResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let too_long =
        || AppError::invalid("the proxy username and password must each be at most 255 bytes");
    let username = u8::try_from(proxy.username.len()).map_err(|_| too_long())?;
    let password = u8::try_from(proxy.password.len()).map_err(|_| too_long())?;
    let mut request = vec![1, username];
    request.extend_from_slice(proxy.username.as_bytes());
    request.push(password);
    request.extend_from_slice(proxy.password.as_bytes());
    stream.write_all(&request).await.map_err(io_error)?;
    let mut status = [0u8; 2];
    stream.read_exact(&mut status).await.map_err(io_error)?;
    if status[1] != 0 {
        return Err(AppError::new(
            ErrorKind::AuthFailed,
            "it rejected the username or password",
        ));
    }
    Ok(())
}

fn socks5_failure(code: u8) -> String {
    let reason = match code {
        1 => "general failure",
        2 => "the proxy's rules do not allow this connection",
        3 => "the network is unreachable",
        4 => "the host is unreachable",
        5 => "the server refused the connection",
        6 => "the connection timed out",
        7 => "the proxy does not support connecting",
        8 => "the proxy does not support this address type",
        _ => "unknown error",
    };
    format!("it could not connect to the server ({reason})")
}

async fn socks4<S>(
    stream: &mut S,
    proxy: &Proxy,
    destination: &Destination,
    port: u16,
) -> AppResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut request = vec![SOCKS4_VERSION, SOCKS4_CONNECT];
    request.extend_from_slice(&port.to_be_bytes());
    match destination {
        Destination::Address(IpAddr::V4(address)) => request.extend_from_slice(&address.octets()),
        Destination::Address(IpAddr::V6(_)) => {
            return Err(AppError::invalid(
                "SOCKS4 cannot reach IPv6 addresses; use SOCKS5",
            ))
        }
        // SOCKS4a: an address of 0.0.0.x asks the proxy to look up the name sent after the
        // user id.
        Destination::Name(_) => request.extend_from_slice(&[0, 0, 0, 1]),
    }
    request.extend_from_slice(proxy.username.as_bytes());
    request.push(0);
    if let Destination::Name(name) = destination {
        request.extend_from_slice(name.as_bytes());
        request.push(0);
    }
    stream.write_all(&request).await.map_err(io_error)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply).await.map_err(io_error)?;
    if reply[0] != 0 {
        return Err(protocol_error("it did not answer as a SOCKS4 proxy"));
    }
    match reply[1] {
        SOCKS4_GRANTED => Ok(()),
        0x5c | 0x5d => Err(AppError::new(
            ErrorKind::AuthFailed,
            "it could not confirm the user id",
        )),
        _ => Err(AppError::new(
            ErrorKind::Connection,
            "it refused or could not make the connection",
        )),
    }
}

async fn http_connect<S>(
    stream: &mut S,
    proxy: &Proxy,
    destination: &Destination,
    port: u16,
) -> AppResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let authority = match destination {
        Destination::Name(name) => format!("{name}:{port}"),
        Destination::Address(IpAddr::V4(address)) => format!("{address}:{port}"),
        Destination::Address(IpAddr::V6(address)) => format!("[{address}]:{port}"),
    };
    let mut request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: Poros/{}\r\n",
        env!("CARGO_PKG_VERSION")
    );
    if !proxy.username.is_empty() {
        let credentials = BASE64.encode(format!("{}:{}", proxy.username, proxy.password));
        request.push_str(&format!("Proxy-Authorization: Basic {credentials}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(io_error)?;

    // Byte by byte, so nothing the SSH server sends right after the head is consumed here.
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_HTTP_HEAD_BYTES {
            return Err(protocol_error("its reply is too long"));
        }
        head.push(stream.read_u8().await.map_err(io_error)?);
    }
    let head = String::from_utf8_lossy(&head);
    let status_line = head.lines().next().unwrap_or_default().trim();
    let mut parts = status_line.split_whitespace();
    let version = parts.next().unwrap_or_default();
    let status: u16 = parts.next().and_then(|code| code.parse().ok()).unwrap_or(0);
    if !version.starts_with("HTTP/") || status == 0 {
        return Err(protocol_error("it did not answer as an HTTP proxy"));
    }
    match status {
        200..=299 => Ok(()),
        407 if proxy.username.is_empty() => Err(AppError::new(
            ErrorKind::AuthFailed,
            "it requires a username and password",
        )),
        407 => Err(AppError::new(
            ErrorKind::AuthFailed,
            "it rejected the username or password",
        )),
        _ => Err(AppError::new(
            ErrorKind::Connection,
            format!("it refused the connection ({status_line})"),
        )),
    }
}

fn io_error(error: std::io::Error) -> AppError {
    let message = if error.kind() == std::io::ErrorKind::UnexpectedEof {
        "it closed the connection".to_string()
    } else {
        error.to_string()
    };
    AppError::new(ErrorKind::Connection, message)
}

fn protocol_error(message: &str) -> AppError {
    AppError::new(ErrorKind::Connection, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use tokio::io::DuplexStream;

    fn proxy(kind: ProxyKind, username: &str, password: &str) -> Proxy {
        Proxy {
            kind,
            host: "proxy.test".into(),
            port: 1080,
            username: username.into(),
            password: password.into(),
            remote_dns: true,
        }
    }

    /// Runs `server` as the proxy end of a pipe while the client side does its handshake,
    /// then checks the tunnel carries data both ways.
    async fn exchange<F>(
        proxy: Proxy,
        destination: Destination,
        server: impl FnOnce(DuplexStream) -> F,
    ) -> AppResult<()>
    where
        F: std::future::Future<Output = DuplexStream> + Send + 'static,
    {
        let (mut client, server_end) = tokio::io::duplex(4096);
        let server = tokio::spawn(server(server_end));
        let result = handshake(&mut client, &proxy, &destination, 22).await;
        let mut server_end = server.await.unwrap();
        if result.is_ok() {
            server_end.write_all(b"SSH-2.0-test\r\n").await.unwrap();
            let mut banner = [0u8; 14];
            client.read_exact(&mut banner).await.unwrap();
            assert_eq!(&banner, b"SSH-2.0-test\r\n");
        }
        result
    }

    async fn read_vec(stream: &mut DuplexStream, length: usize) -> Vec<u8> {
        let mut buffer = vec![0u8; length];
        stream.read_exact(&mut buffer).await.unwrap();
        buffer
    }

    #[tokio::test]
    async fn socks5_with_login_and_a_name() {
        let result = exchange(
            proxy(ProxyKind::Socks5, "alice", "secret"),
            Destination::Name("example.com".into()),
            |mut server| async move {
                assert_eq!(read_vec(&mut server, 4).await, [5, 2, 0, 2]);
                server.write_all(&[5, 2]).await.unwrap();
                assert_eq!(read_vec(&mut server, 2).await, [1, 5]);
                assert_eq!(read_vec(&mut server, 5).await, b"alice");
                assert_eq!(read_vec(&mut server, 1).await, [6]);
                assert_eq!(read_vec(&mut server, 6).await, b"secret");
                server.write_all(&[1, 0]).await.unwrap();
                assert_eq!(read_vec(&mut server, 5).await, [5, 1, 0, 3, 11]);
                assert_eq!(read_vec(&mut server, 11).await, b"example.com");
                assert_eq!(read_vec(&mut server, 2).await, [0, 22]);
                server
                    .write_all(&[5, 0, 0, 3, 4, b'h', b'o', b's', b't', 0, 80])
                    .await
                    .unwrap();
                server
            },
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn socks5_without_login_to_an_ipv6_address() {
        let address = Ipv6Addr::LOCALHOST;
        let result = exchange(
            proxy(ProxyKind::Socks5, "", ""),
            Destination::Address(IpAddr::V6(address)),
            move |mut server| async move {
                assert_eq!(read_vec(&mut server, 3).await, [5, 1, 0]);
                server.write_all(&[5, 0]).await.unwrap();
                assert_eq!(read_vec(&mut server, 4).await, [5, 1, 0, 4]);
                assert_eq!(read_vec(&mut server, 16).await, address.octets());
                assert_eq!(read_vec(&mut server, 2).await, [0, 22]);
                server
                    .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                    .await
                    .unwrap();
                server
            },
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn socks5_failures_are_explained() {
        let refused = exchange(
            proxy(ProxyKind::Socks5, "", ""),
            Destination::Address(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            |mut server| async move {
                read_vec(&mut server, 3).await;
                server.write_all(&[5, 0]).await.unwrap();
                read_vec(&mut server, 10).await;
                server
                    .write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0])
                    .await
                    .unwrap();
                server
            },
        )
        .await
        .unwrap_err();
        assert_eq!(refused.kind, ErrorKind::Connection);
        assert!(refused
            .message
            .starts_with("SOCKS5 proxy proxy.test:1080: "));
        assert!(refused.message.contains("refused the connection"));

        let needs_login = exchange(
            proxy(ProxyKind::Socks5, "", ""),
            Destination::Name("example.com".into()),
            |mut server| async move {
                read_vec(&mut server, 3).await;
                server.write_all(&[5, 0xff]).await.unwrap();
                server
            },
        )
        .await
        .unwrap_err();
        assert_eq!(needs_login.kind, ErrorKind::AuthFailed);
        assert!(needs_login.message.contains("requires a username"));

        let wrong_password = exchange(
            proxy(ProxyKind::Socks5, "alice", "wrong"),
            Destination::Name("example.com".into()),
            |mut server| async move {
                read_vec(&mut server, 4).await;
                server.write_all(&[5, 2]).await.unwrap();
                read_vec(&mut server, 2 + 5 + 1 + 5).await;
                server.write_all(&[1, 1]).await.unwrap();
                server
            },
        )
        .await
        .unwrap_err();
        assert_eq!(wrong_password.kind, ErrorKind::AuthFailed);
    }

    #[tokio::test]
    async fn socks4a_sends_the_name_after_the_user_id() {
        let result = exchange(
            proxy(ProxyKind::Socks4, "bob", ""),
            Destination::Name("example.com".into()),
            |mut server| async move {
                assert_eq!(read_vec(&mut server, 8).await, [4, 1, 0, 22, 0, 0, 0, 1]);
                assert_eq!(read_vec(&mut server, 4).await, b"bob\0");
                assert_eq!(read_vec(&mut server, 12).await, b"example.com\0");
                server
                    .write_all(&[0, 0x5a, 0, 0, 0, 0, 0, 0])
                    .await
                    .unwrap();
                server
            },
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn socks4_rejections_and_ipv6() {
        let rejected = exchange(
            proxy(ProxyKind::Socks4, "", ""),
            Destination::Address(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2))),
            |mut server| async move {
                assert_eq!(
                    read_vec(&mut server, 9).await,
                    [4, 1, 0, 22, 10, 0, 0, 2, 0]
                );
                server
                    .write_all(&[0, 0x5b, 0, 0, 0, 0, 0, 0])
                    .await
                    .unwrap();
                server
            },
        )
        .await
        .unwrap_err();
        assert_eq!(rejected.kind, ErrorKind::Connection);

        let (mut client, _server) = tokio::io::duplex(64);
        let ipv6 = handshake(
            &mut client,
            &proxy(ProxyKind::Socks4, "", ""),
            &Destination::Address(IpAddr::V6(Ipv6Addr::LOCALHOST)),
            22,
        )
        .await
        .unwrap_err();
        assert_eq!(ipv6.kind, ErrorKind::InvalidInput);
    }

    async fn read_http_head(server: &mut DuplexStream) -> String {
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(server.read_u8().await.unwrap());
        }
        String::from_utf8(head).unwrap()
    }

    #[tokio::test]
    async fn http_connect_with_basic_auth() {
        let result = exchange(
            proxy(ProxyKind::Http, "alice", "secret"),
            Destination::Address(IpAddr::V6(Ipv6Addr::LOCALHOST)),
            |mut server| async move {
                let head = read_http_head(&mut server).await;
                assert!(head.starts_with("CONNECT [::1]:22 HTTP/1.1\r\nHost: [::1]:22\r\n"));
                // "alice:secret"
                assert!(head.contains("Proxy-Authorization: Basic YWxpY2U6c2VjcmV0\r\n"));
                server
                    .write_all(b"HTTP/1.1 200 Connection established\r\nVia: test\r\n\r\n")
                    .await
                    .unwrap();
                server
            },
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn http_refusals() {
        let refused = |reply: &'static [u8], username: &'static str| {
            exchange(
                proxy(ProxyKind::Http, username, "pw"),
                Destination::Name("example.com".into()),
                move |mut server| async move {
                    let head = read_http_head(&mut server).await;
                    assert!(head.starts_with("CONNECT example.com:22 HTTP/1.1\r\n"));
                    assert_eq!(head.contains("Proxy-Authorization"), !username.is_empty());
                    server.write_all(reply).await.unwrap();
                    server
                },
            )
        };
        let forbidden = refused(b"HTTP/1.1 403 Forbidden\r\n\r\n", "")
            .await
            .unwrap_err();
        assert_eq!(forbidden.kind, ErrorKind::Connection);
        assert!(forbidden.message.contains("HTTP/1.1 403 Forbidden"));

        let login = refused(b"HTTP/1.0 407 Proxy Authentication Required\r\n\r\n", "")
            .await
            .unwrap_err();
        assert_eq!(login.kind, ErrorKind::AuthFailed);
        assert!(login.message.contains("requires a username"));

        let wrong = refused(b"HTTP/1.1 407 Nope\r\n\r\n", "alice")
            .await
            .unwrap_err();
        assert!(wrong.message.contains("rejected the username"));

        let garbage = refused(b"SSH-2.0-OpenSSH\r\n\r\n", "").await.unwrap_err();
        assert!(garbage.message.contains("did not answer as an HTTP proxy"));
    }

    #[tokio::test]
    async fn addresses_skip_the_lookup_and_names_go_to_the_proxy() {
        let remote = proxy(ProxyKind::Socks5, "", "");
        assert_eq!(
            Destination::resolve(&remote, " 192.0.2.7 ", 22)
                .await
                .unwrap(),
            Destination::Address("192.0.2.7".parse().unwrap())
        );
        assert_eq!(
            Destination::resolve(&remote, "[::1]", 22).await.unwrap(),
            Destination::Address(IpAddr::V6(Ipv6Addr::LOCALHOST))
        );
        assert_eq!(
            Destination::resolve(&remote, "example.com", 22)
                .await
                .unwrap(),
            Destination::Name("example.com".into())
        );
        let local_lookup = Proxy {
            remote_dns: false,
            ..proxy(ProxyKind::Socks4, "", "")
        };
        assert_eq!(
            Destination::resolve(&local_lookup, "localhost", 22)
                .await
                .unwrap(),
            Destination::Address(IpAddr::V4(Ipv4Addr::LOCALHOST))
        );
    }
}
