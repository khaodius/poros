//! Shells on the server, each in a pseudo-terminal on a connection of its own so it outlives the
//! tab it was opened from. When the server allows no second connection, the shell shares the
//! session's. Output is kept in a backlog, so a terminal moved to another window shows what it
//! showed before.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use russh::client::Msg;
use russh::{Channel, ChannelMsg};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::protocol::Protocol;
use crate::session::{RemoteTarget, SessionManager};
use crate::ssh::{self, SshHandle};

const TERMINAL_TYPE: &str = "xterm-256color";
/// Output kept for a window that attaches later. Trimmed to three quarters when exceeded, at
/// a line break, so trimming runs rarely and seldom splits an escape sequence.
const BACKLOG_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    Output {
        data: String,
    },
    /// The shell ended; `message` says how.
    Exit {
        message: String,
    },
}

/// Where a terminal's output goes: the window showing it.
pub type Sink = Arc<dyn Fn(TerminalEvent) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    /// The server's label.
    pub label: String,
}

enum Input {
    Data(Vec<u8>),
    Resize { columns: u32, rows: u32 },
}

struct Screen {
    backlog: String,
    sink: Option<Sink>,
    /// Set once the shell ended, with how.
    ended: Option<String>,
    decoder: Utf8Stream,
}

impl Screen {
    fn emit(&self, event: TerminalEvent) {
        if let Some(sink) = &self.sink {
            sink(event);
        }
    }
}

struct Terminal {
    id: String,
    target: RemoteTarget,
    size: Mutex<(u32, u32)>,
    owner: Mutex<String>,
    screen: Mutex<Screen>,
    /// `None` while no shell runs.
    input: Mutex<Option<mpsc::UnboundedSender<Input>>>,
    /// The terminal's own connection; `None` when it shares the session's.
    connection: Mutex<Option<SshHandle>>,
}

pub struct TerminalManager {
    terminals: Mutex<HashMap<String, Arc<Terminal>>>,
    sessions: Arc<SessionManager>,
    events: Events,
}

impl TerminalManager {
    pub fn new(sessions: Arc<SessionManager>, events: Events) -> Self {
        Self {
            terminals: Mutex::new(HashMap::new()),
            sessions,
            events,
        }
    }

    pub async fn open(
        &self,
        session_id: &str,
        columns: u32,
        rows: u32,
        sink: Sink,
        owner: &str,
    ) -> AppResult<TerminalInfo> {
        let target = self.sessions.get(session_id).await?.target();
        if target.protocol() != Protocol::Sftp {
            return Err(AppError::unsupported(format!(
                "Terminals need an SSH connection; {} uses {}",
                target.label,
                target.protocol().display_name()
            )));
        }
        let terminal = Arc::new(Terminal {
            id: uuid::Uuid::new_v4().to_string(),
            target,
            size: Mutex::new((columns.max(1), rows.max(1))),
            owner: Mutex::new(owner.to_string()),
            screen: Mutex::new(Screen {
                backlog: String::new(),
                sink: Some(sink),
                ended: None,
                decoder: Utf8Stream::default(),
            }),
            input: Mutex::new(None),
            connection: Mutex::new(None),
        });
        self.start(&terminal).await?;
        let info = TerminalInfo {
            id: terminal.id.clone(),
            label: terminal.target.label.clone(),
        };
        self.terminals
            .lock()
            .unwrap()
            .insert(terminal.id.clone(), terminal);
        Ok(info)
    }

    /// Sends the terminal's output to `sink` from now on, starting with the backlog, and makes
    /// the calling window its owner.
    pub fn attach(&self, id: &str, sink: Sink, owner: &str) -> AppResult<()> {
        let terminal = self.terminal(id)?;
        *terminal.owner.lock().unwrap() = owner.to_string();
        let mut screen = terminal.screen.lock().unwrap();
        screen.sink = Some(sink);
        if !screen.backlog.is_empty() {
            screen.emit(TerminalEvent::Output {
                data: screen.backlog.clone(),
            });
        }
        if let Some(message) = screen.ended.clone() {
            screen.emit(TerminalEvent::Exit { message });
        }
        Ok(())
    }

    pub fn write(&self, id: &str, data: Vec<u8>) -> AppResult<()> {
        self.send(id, Input::Data(data))
    }

    pub fn resize(&self, id: &str, columns: u32, rows: u32) -> AppResult<()> {
        let (columns, rows) = (columns.max(1), rows.max(1));
        let terminal = self.terminal(id)?;
        {
            let mut size = terminal.size.lock().unwrap();
            if *size == (columns, rows) {
                return Ok(());
            }
            *size = (columns, rows);
        }
        self.send(id, Input::Resize { columns, rows })
    }

    /// Starts a new shell in a terminal whose shell ended.
    pub async fn restart(&self, id: &str) -> AppResult<()> {
        let terminal = self.terminal(id)?;
        if terminal.input.lock().unwrap().is_some() {
            return Ok(());
        }
        terminal.screen.lock().unwrap().ended = None;
        self.start(&terminal).await
    }

    pub async fn close(&self, id: &str) {
        let terminal = self.terminals.lock().unwrap().remove(id);
        if let Some(terminal) = terminal {
            stop(&terminal).await;
        }
    }

    pub async fn close_owned_by(&self, owner: &str) {
        let owned: Vec<Arc<Terminal>> = {
            let mut terminals = self.terminals.lock().unwrap();
            let ids: Vec<String> = terminals
                .iter()
                .filter(|(_, terminal)| *terminal.owner.lock().unwrap() == owner)
                .map(|(id, _)| id.clone())
                .collect();
            ids.iter().filter_map(|id| terminals.remove(id)).collect()
        };
        for terminal in owned {
            stop(&terminal).await;
        }
    }

    pub async fn close_all(&self) {
        let all: Vec<Arc<Terminal>> = self
            .terminals
            .lock()
            .unwrap()
            .drain()
            .map(|(_, terminal)| terminal)
            .collect();
        for terminal in all {
            stop(&terminal).await;
        }
    }

    fn terminal(&self, id: &str) -> AppResult<Arc<Terminal>> {
        self.terminals
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "This terminal is closed"))
    }

    fn send(&self, id: &str, input: Input) -> AppResult<()> {
        let terminal = self.terminal(id)?;
        let sender = terminal.input.lock().unwrap().clone();
        match sender {
            Some(sender) if sender.send(input).is_ok() => Ok(()),
            _ => Err(AppError::new(
                ErrorKind::Disconnected,
                "The shell has ended",
            )),
        }
    }

    async fn start(&self, terminal: &Arc<Terminal>) -> AppResult<()> {
        let mut channel = self.open_channel(terminal).await?;
        let (columns, rows) = *terminal.size.lock().unwrap();
        let started = async {
            channel
                .request_pty(true, TERMINAL_TYPE, columns, rows, 0, 0, &[])
                .await?;
            expect_success(&mut channel, "a terminal").await?;
            channel.request_shell(true).await?;
            expect_success(&mut channel, "a shell").await
        }
        .await;
        if let Err(error) = started {
            disconnect(terminal).await;
            return Err(error);
        }

        let (reader, writer) = channel.split();
        let (sender, receiver) = mpsc::unbounded_channel();
        *terminal.input.lock().unwrap() = Some(sender);
        tokio::spawn(forward_input(writer, receiver));
        tokio::spawn(read_output(terminal.clone(), reader, self.events.clone()));
        Ok(())
    }

    async fn open_channel(&self, terminal: &Terminal) -> AppResult<Channel<Msg>> {
        let target = &terminal.target;
        let refusal = match target.connect(&self.sessions.known_hosts, "terminal").await {
            Ok(handle) => match handle.channel_open_session().await {
                Ok(channel) => {
                    *terminal.connection.lock().unwrap() = Some(handle);
                    self.events.log(
                        LogLevel::Info,
                        Some(&target.session_id),
                        format!("Terminal connection to {} opened", target.label),
                    );
                    return Ok(channel);
                }
                Err(error) => {
                    ssh::disconnect(&handle).await;
                    AppError::from(error)
                }
            },
            Err(error) if error.kind == ErrorKind::HostKeyChanged => return Err(error),
            Err(error) => error,
        };
        let session = self
            .sessions
            .get(&target.session_id)
            .await
            .map_err(|_| refusal.clone())?;
        self.events.log(
            LogLevel::Warn,
            Some(&target.session_id),
            format!(
                "The server did not accept another connection ({}); the terminal shares the browsing connection",
                refusal.message
            ),
        );
        session.open_command_channel().await
    }
}

async fn expect_success(channel: &mut Channel<Msg>, what: &str) -> AppResult<()> {
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return Ok(()),
            Some(ChannelMsg::Failure) => {
                return Err(AppError::new(
                    ErrorKind::Ssh,
                    format!("The server refused to open {what}"),
                ))
            }
            Some(ChannelMsg::Close) | Some(ChannelMsg::Eof) | None => {
                return Err(AppError::new(
                    ErrorKind::Disconnected,
                    format!("The server closed the channel while opening {what}"),
                ))
            }
            Some(_) => {}
        }
    }
}

async fn forward_input(
    writer: russh::ChannelWriteHalf<Msg>,
    mut receiver: mpsc::UnboundedReceiver<Input>,
) {
    while let Some(input) = receiver.recv().await {
        let sent = match input {
            Input::Data(data) => writer.data_bytes(data).await,
            Input::Resize { columns, rows } => writer.window_change(columns, rows, 0, 0).await,
        };
        if sent.is_err() {
            break;
        }
    }
}

async fn read_output(terminal: Arc<Terminal>, mut reader: russh::ChannelReadHalf, events: Events) {
    let mut exit_status = None;
    let mut exit_signal = None;
    while let Some(message) = reader.wait().await {
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                let mut screen = terminal.screen.lock().unwrap();
                let text = screen.decoder.push(&data);
                if text.is_empty() {
                    continue;
                }
                append_backlog(&mut screen.backlog, &text);
                screen.emit(TerminalEvent::Output { data: text });
            }
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => exit_status = Some(status),
            ChannelMsg::ExitSignal { signal_name, .. } => exit_signal = Some(signal_name),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    let message = match (exit_status, exit_signal) {
        (_, Some(signal)) => format!("The shell was ended by signal {signal:?}"),
        (Some(0), None) => "The shell exited".to_string(),
        (Some(status), None) => format!("The shell exited with status {status}"),
        (None, None) => "The connection closed".to_string(),
    };
    terminal.input.lock().unwrap().take();
    disconnect(&terminal).await;
    events.log(
        LogLevel::Info,
        Some(&terminal.target.session_id),
        format!(
            "Terminal on {}: {}",
            terminal.target.label,
            message.to_lowercase()
        ),
    );
    let mut screen = terminal.screen.lock().unwrap();
    screen.ended = Some(message.clone());
    screen.emit(TerminalEvent::Exit { message });
}

/// Ends the shell and forgets where output went, so nothing reaches a tab that closed.
async fn stop(terminal: &Terminal) {
    terminal.screen.lock().unwrap().sink = None;
    terminal.input.lock().unwrap().take();
    disconnect(terminal).await;
}

async fn disconnect(terminal: &Terminal) {
    let connection = terminal.connection.lock().unwrap().take();
    if let Some(handle) = connection {
        ssh::disconnect(&handle).await;
    }
}

fn append_backlog(backlog: &mut String, text: &str) {
    backlog.push_str(text);
    if backlog.len() <= BACKLOG_BYTES {
        return;
    }
    let mut keep_from = backlog.len() - BACKLOG_BYTES * 3 / 4;
    while !backlog.is_char_boundary(keep_from) {
        keep_from += 1;
    }
    let cut = backlog[keep_from..]
        .find('\n')
        .map_or(keep_from, |offset| keep_from + offset + 1);
    backlog.drain(..cut);
}

/// Decodes UTF-8 arriving in pieces, holding back a character split between two pieces.
#[derive(Default)]
struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    fn push(&mut self, data: &[u8]) -> String {
        self.pending.extend_from_slice(data);
        let mut text = String::with_capacity(self.pending.len());
        let mut rest: &[u8] = &self.pending;
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    rest = &[];
                    break;
                }
                Err(error) => {
                    let (valid, after) = rest.split_at(error.valid_up_to());
                    text.push_str(std::str::from_utf8(valid).unwrap_or_default());
                    match error.error_len() {
                        Some(length) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            rest = &after[length..];
                        }
                        None => {
                            rest = after;
                            break;
                        }
                    }
                }
            }
        }
        self.pending = rest.to_vec();
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_split_across_reads_are_joined() {
        let mut stream = Utf8Stream::default();
        let bytes = "héllo ✓".as_bytes();
        let (first, second) = bytes.split_at(2);
        assert_eq!(stream.push(first), "h");
        assert_eq!(stream.push(second), "éllo ✓");
        let check = "✓".as_bytes();
        assert_eq!(stream.push(&check[..1]), "");
        assert_eq!(stream.push(&check[1..2]), "");
        assert_eq!(stream.push(&check[2..]), "✓");
    }

    #[test]
    fn invalid_bytes_become_replacement_characters() {
        let mut stream = Utf8Stream::default();
        assert_eq!(stream.push(b"a\xFFb"), "a\u{FFFD}b");
        assert_eq!(stream.push(b"\xE2\x9C"), "");
        assert_eq!(stream.push(b"x"), "\u{FFFD}x");
    }

    #[test]
    fn backlog_is_trimmed_at_a_line_break() {
        let mut backlog = String::new();
        let line = format!("{}\n", "x".repeat(99));
        for _ in 0..(BACKLOG_BYTES / line.len() + 2) {
            append_backlog(&mut backlog, &line);
        }
        assert!(backlog.len() <= BACKLOG_BYTES);
        assert!(backlog.len() >= BACKLOG_BYTES / 2);
        assert!(backlog.starts_with('x'));
        assert!(backlog.ends_with('\n'));
    }

    #[test]
    fn backlog_without_line_breaks_is_trimmed_on_a_character_boundary() {
        let mut backlog = String::new();
        append_backlog(&mut backlog, &"é".repeat(BACKLOG_BYTES));
        assert!(backlog.len() <= BACKLOG_BYTES);
        assert!(backlog.chars().all(|character| character == 'é'));

        // An odd length puts the first byte kept in the middle of a two-byte character.
        let mut backlog = String::new();
        append_backlog(
            &mut backlog,
            &format!("{}a", "é".repeat(BACKLOG_BYTES / 2 + 1)),
        );
        assert!(backlog.len() <= BACKLOG_BYTES);
        assert!(backlog.starts_with('é'));
        assert!(backlog.ends_with("éa"));
    }
}
