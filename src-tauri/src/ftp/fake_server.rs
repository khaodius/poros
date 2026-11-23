//! A scripted FTP server for tests. It listens on a loopback port, speaks enough of the
//! protocol for `Control` and `FtpFs` to work against it, records every command, and behaves
//! in ways real servers do but vsftpd, which the end-to-end suite runs against, does not.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::FtpFs;
use crate::events::Events;
use crate::protocol::Protocol;
use crate::ssh::{AuthMethod, ConnectProfile, Route};
use crate::tls::TrustedCertificates;

#[derive(Debug, Clone, Default)]
pub struct Behavior {
    /// Answers `EPSV` with a refusal, so the client falls back to `PASV`.
    pub refuses_extended_passive: bool,
    /// The address `PASV` replies name instead of the server's own. The server still listens
    /// on its own.
    pub passive_address: Option<Ipv4Addr>,
    /// Stops listening on the port a `PASV` reply names before the client can connect.
    pub closes_passive_port: bool,
    /// Bytes of made-up entries that every folder listing carries besides the files.
    pub listing_padding: usize,
    /// A command the server takes this long to answer.
    pub slow_command: Option<(&'static str, Duration)>,
    /// Announces a transfer only once its data connection is made, as vsftpd does in passive
    /// mode, rather than at once.
    pub announces_once_connected: bool,
    /// What the server does about a data connection it is to make itself, after `PORT`.
    pub connecting_back: ConnectingBack,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectingBack {
    #[default]
    Connects,
    /// Announces the transfer, then says it could not connect, as Pure-FTPd does.
    FailsAfterAnnouncing,
    /// Announces the transfer, then neither connects nor says anything more until aborted.
    HangsAfterAnnouncing,
}

#[derive(Default)]
struct State {
    commands: Vec<String>,
    files: HashMap<String, Vec<u8>>,
}

pub struct FakeServer {
    address: SocketAddr,
    state: Arc<Mutex<State>>,
}

impl FakeServer {
    pub async fn start(behavior: Behavior) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let session = Session {
                    control: BufReader::new(stream),
                    state: shared.clone(),
                    behavior: behavior.clone(),
                    data: None,
                    restart: 0,
                    directory: "/".to_string(),
                };
                // A client that hangs up mid-command ends its session, not the test.
                tokio::spawn(async move { session.run().await.ok() });
            }
        });
        Self { address, state }
    }

    pub fn profile(&self) -> ConnectProfile {
        ConnectProfile {
            protocol: Protocol::Ftp,
            host: self.address.ip().to_string(),
            port: self.address.port(),
            username: "poros".into(),
            auth: AuthMethod::Password {
                password: "poros-pass".into(),
            },
            initial_path: None,
            timeout_secs: Some(5),
            keepalive_secs: Some(0),
            compression: false,
            receive_buffer_kib: None,
            send_buffer_kib: None,
            saved_connection_id: None,
            ftp_active: false,
            bypass_proxy: false,
            jump_connection_id: None,
            rsync_path: None,
            s3: Default::default(),
            route: Route::default(),
        }
    }

    pub async fn connect(&self, profile: &ConnectProfile) -> Arc<FtpFs> {
        // Never read: only FTPS looks up certificates.
        let trusted = TrustedCertificates::new(PathBuf::new());
        let (files, _) = FtpFs::connect("fake", profile, &trusted, None, &Events::default())
            .await
            .unwrap();
        files
    }

    pub fn put_file(&self, path: &str, contents: &[u8]) {
        let mut state = self.state.lock().unwrap();
        state.files.insert(path.to_string(), contents.to_vec());
    }

    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.state.lock().unwrap().files.get(path).cloned()
    }

    /// The commands received on every connection since the last call.
    pub fn take_commands(&self) -> Vec<String> {
        std::mem::take(&mut self.state.lock().unwrap().commands)
    }

    /// Whether `command` arrives within a few seconds, on any connection.
    pub async fn receives(&self, command: &str) -> bool {
        for _ in 0..200 {
            if self.has_received(command) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    fn has_received(&self, command: &str) -> bool {
        let state = self.state.lock().unwrap();
        state.commands.iter().any(|received| received == command)
    }
}

/// Where the data of the next transfer goes.
enum DataPort {
    Passive(TcpListener),
    Active(SocketAddr),
}

struct Session {
    control: BufReader<TcpStream>,
    state: Arc<Mutex<State>>,
    behavior: Behavior,
    data: Option<DataPort>,
    restart: usize,
    directory: String,
}

impl Session {
    async fn run(mut self) -> io::Result<()> {
        self.reply("220 Scripted server ready").await?;
        let mut line = String::new();
        loop {
            line.clear();
            if self.control.read_line(&mut line).await? == 0 {
                return Ok(());
            }
            let command = line.trim_end().to_string();
            self.state.lock().unwrap().commands.push(command.clone());
            let (verb, argument) = command.split_once(' ').unwrap_or((&command, ""));
            // As RFC 959 has it, and as Apache FtpServer does, only the command right after
            // `REST` sees the restart marker; vsftpd keeps it until a transfer uses it.
            let restart = std::mem::take(&mut self.restart);
            if let Some((_, delay)) = self.behavior.slow_command.filter(|(slow, _)| *slow == verb) {
                tokio::time::sleep(delay).await;
            }
            match verb {
                "USER" => self.reply("331 Password required").await?,
                "PASS" => self.reply("230 Logged in").await?,
                "FEAT" => {
                    self.reply("211-Features:\r\n REST STREAM\r\n SIZE\r\n211 End")
                        .await?
                }
                "OPTS" | "TYPE" | "NOOP" => self.reply("200 Done").await?,
                "PWD" => {
                    let reply = format!("257 \"{}\" is the current directory", self.directory);
                    self.reply(&reply).await?
                }
                "CWD" if self.size_of(argument).is_some() => {
                    self.reply("550 Not a directory").await?
                }
                "CWD" => {
                    self.directory = argument.to_string();
                    self.reply("250 Directory changed").await?
                }
                "SIZE" => match self.size_of(argument) {
                    Some(size) => self.reply(&format!("213 {size}")).await?,
                    None => self.reply("550 No such file").await?,
                },
                "EPSV" if self.behavior.refuses_extended_passive => {
                    self.reply("500 Unknown command").await?
                }
                "EPSV" => {
                    let port = self.listen().await?;
                    let reply = format!("229 Entering Extended Passive Mode (|||{port}|)");
                    self.reply(&reply).await?
                }
                "PASV" => {
                    let port = self.listen().await?;
                    if self.behavior.closes_passive_port {
                        self.data = None;
                    }
                    let named = self.behavior.passive_address.unwrap_or(Ipv4Addr::LOCALHOST);
                    let [first, second, third, fourth] = named.octets();
                    let reply = format!(
                        "227 Entering Passive Mode ({first},{second},{third},{fourth},{},{}).",
                        port >> 8,
                        port & 0xff
                    );
                    self.reply(&reply).await?
                }
                "PORT" => match port_address(argument) {
                    Some(address) => {
                        self.data = Some(DataPort::Active(address));
                        self.reply("200 Port set").await?
                    }
                    None => self.reply("501 Not an address").await?,
                },
                "REST" => match argument.parse() {
                    Ok(offset) => {
                        self.restart = offset;
                        self.reply("350 Restart position accepted").await?
                    }
                    Err(_) => self.reply("501 Not a position").await?,
                },
                "LIST" => self.send_listing().await?,
                "RETR" => self.send_file(argument, restart).await?,
                "STOR" => self.receive_file(argument, restart).await?,
                "ABOR" => self.reply("226 Nothing left to abort").await?,
                "QUIT" => return self.reply("221 Goodbye").await,
                _ => self.reply("502 Not implemented").await?,
            }
        }
    }

    async fn reply(&mut self, reply: &str) -> io::Result<()> {
        let stream = self.control.get_mut();
        stream.write_all(format!("{reply}\r\n").as_bytes()).await?;
        stream.flush().await
    }

    fn size_of(&self, path: &str) -> Option<usize> {
        self.state.lock().unwrap().files.get(path).map(Vec::len)
    }

    async fn listen(&mut self) -> io::Result<u16> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        self.data = Some(DataPort::Passive(listener));
        Ok(port)
    }

    /// Announces a transfer and opens its data connection, in the order and with the outcome
    /// the behavior sets. `None` when the transfer ended without one.
    async fn start_transfer(&mut self) -> io::Result<Option<TcpStream>> {
        let connecting_back = matches!(self.data, Some(DataPort::Active(_)));
        let behavior = self.behavior.connecting_back;
        if connecting_back && behavior != ConnectingBack::Connects {
            self.data = None;
            self.reply("150 Connecting to the data port").await?;
            if behavior == ConnectingBack::HangsAfterAnnouncing {
                self.wait_for_abort().await?;
            } else {
                self.reply("425 Could not open the data connection").await?;
            }
            return Ok(None);
        }
        if !self.behavior.announces_once_connected {
            self.reply("150 Opening the data connection").await?;
            return self.open_data().await;
        }
        let data = self.open_data().await?;
        if data.is_some() {
            self.reply("150 Opening the data connection").await?;
        }
        Ok(data)
    }

    /// The data connection set up for this transfer, or `None` once the transfer has ended
    /// without it: the client aborted it, or it could not be made.
    async fn open_data(&mut self) -> io::Result<Option<TcpStream>> {
        let port = self.data.take();
        let connecting = async move {
            match port {
                Some(DataPort::Passive(listener)) => Ok(listener.accept().await?.0),
                Some(DataPort::Active(address)) => TcpStream::connect(address).await,
                None => Err(io::Error::other("no data port was set")),
            }
        };
        let connected = tokio::select! {
            connected = connecting => connected,
            aborted = self.wait_for_abort() => return aborted.map(|()| None),
        };
        match connected {
            Ok(stream) => Ok(Some(stream)),
            Err(_) => {
                self.reply("425 Could not open the data connection").await?;
                Ok(None)
            }
        }
    }

    /// Reads commands until `ABOR`, and ends the transfer it stops.
    async fn wait_for_abort(&mut self) -> io::Result<()> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.control.read_line(&mut line).await? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let command = line.trim_end().to_string();
            self.state.lock().unwrap().commands.push(command.clone());
            if command == "ABOR" {
                self.reply("426 Transfer aborted").await?;
                return self.reply("226 Abort successful").await;
            }
            self.reply("503 A transfer is in progress").await?;
        }
    }

    async fn send_file(&mut self, path: &str, restart: usize) -> io::Result<()> {
        let Some(contents) = self.state.lock().unwrap().files.get(path).cloned() else {
            return self.reply("550 No such file").await;
        };
        let rest = contents[restart.min(contents.len())..].to_vec();
        self.send_data(&[rest]).await
    }

    /// Every file as a line of `ls -l`, whatever folder is asked for, then the padding.
    async fn send_listing(&mut self) -> io::Result<()> {
        let mut listing = String::new();
        for (path, contents) in &self.state.lock().unwrap().files {
            let name = path.trim_start_matches('/');
            let size = contents.len();
            listing.push_str(&format!(
                "-rw-r--r-- 1 poros poros {size} Jan 01 2024 {name}\r\n"
            ));
        }
        let filler = "-rw-r--r-- 1 poros poros 0 Jan 01 2024 filler\r\n".repeat(1024);
        let mut parts = vec![listing.into_bytes()];
        parts.resize(
            1 + self.behavior.listing_padding.div_ceil(filler.len()),
            filler.into_bytes(),
        );
        self.send_data(&parts).await
    }

    /// Announces a transfer, sends `parts` over the data connection, and says how it ended.
    async fn send_data(&mut self, parts: &[Vec<u8>]) -> io::Result<()> {
        let Some(mut data) = self.start_transfer().await? else {
            return Ok(());
        };
        let sent = async {
            for part in parts {
                data.write_all(part).await?;
            }
            data.shutdown().await
        }
        .await;
        match sent {
            Ok(()) => self.reply("226 Transfer complete").await,
            Err(_) => self.reply("426 Transfer aborted").await,
        }
    }

    /// Keeps the part of the file before the restart marker and replaces the rest.
    async fn receive_file(&mut self, path: &str, restart: usize) -> io::Result<()> {
        let Some(mut data) = self.start_transfer().await? else {
            return Ok(());
        };
        let mut received = Vec::new();
        if data.read_to_end(&mut received).await.is_err() {
            return self.reply("426 Transfer aborted").await;
        }
        {
            let mut state = self.state.lock().unwrap();
            let file = state.files.entry(path.to_string()).or_default();
            file.truncate(restart);
            file.extend_from_slice(&received);
        }
        self.reply("226 Transfer complete").await
    }
}

/// The address in `PORT 127,0,0,1,19,136`.
fn port_address(argument: &str) -> Option<SocketAddr> {
    let numbers: Vec<u8> = argument
        .split(',')
        .map(|number| number.parse().ok())
        .collect::<Option<_>>()?;
    let [first, second, third, fourth, high, low] = numbers[..] else {
        return None;
    };
    let port = u16::from(high) << 8 | u16::from(low);
    Some((Ipv4Addr::new(first, second, third, fourth), port).into())
}
