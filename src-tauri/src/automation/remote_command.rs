//! Commands the user runs on a server over SSH. Output streams to the window that asked for
//! it and to the log.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::rsync::shell_quote;
use crate::session::{Session, SessionManager};

/// Output lines copied to the log per run; the output window has the rest.
const MAX_LOGGED_LINES: usize = 200;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    /// Names this run for its output events and for stopping it.
    pub run_id: String,
    pub session_id: String,
    pub command: String,
    /// The folder to run the command in; the login folder when absent.
    #[serde(default)]
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunk {
    pub run_id: String,
    pub stream: OutputStream,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    /// Absent when the server did not say, for example after a stop.
    pub exit_status: Option<u32>,
    /// The signal that ended the command, if one did.
    pub signal: Option<String>,
    pub stopped: bool,
    pub elapsed_millis: u64,
}

impl CommandResult {
    pub fn succeeded(&self) -> bool {
        !self.stopped && self.signal.is_none() && self.exit_status.unwrap_or(0) == 0
    }

    pub fn describe(&self) -> String {
        if self.stopped {
            "stopped".into()
        } else if let Some(signal) = &self.signal {
            format!("ended by signal {signal}")
        } else {
            match self.exit_status {
                Some(0) | None => "finished".into(),
                Some(status) => format!("exited with status {status}"),
            }
        }
    }
}

pub struct CommandRunner {
    sessions: Arc<SessionManager>,
    events: Events,
    running: Mutex<HashMap<String, CancellationToken>>,
}

impl CommandRunner {
    pub fn new(sessions: Arc<SessionManager>, events: Events) -> Self {
        Self {
            sessions,
            events,
            running: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(&self, request: CommandRequest) -> AppResult<CommandResult> {
        let command = request.command.trim();
        if command.is_empty() {
            return Err(AppError::invalid("Enter a command to run"));
        }
        let session = self.sessions.get(&request.session_id).await?;
        let cancel = CancellationToken::new();
        self.running
            .lock()
            .unwrap()
            .insert(request.run_id.clone(), cancel.clone());

        let session_id = Some(request.session_id.as_str());
        self.events.log(
            LogLevel::Info,
            session_id,
            format!("Running on the server: {command}"),
        );
        let mut logged_lines = 0;
        let mut lines = [String::new(), String::new()];
        let mut log_line = |line: &str| {
            if logged_lines < MAX_LOGGED_LINES {
                self.events.log(LogLevel::Server, session_id, line);
            } else if logged_lines == MAX_LOGGED_LINES {
                self.events.log(
                    LogLevel::Info,
                    session_id,
                    "The rest of the output is in the output window",
                );
            }
            logged_lines += 1;
        };
        let result = run_on(
            &session,
            command,
            request.directory.as_deref(),
            &cancel,
            |stream, text| {
                self.events.command_output(&OutputChunk {
                    run_id: request.run_id.clone(),
                    stream,
                    text: text.to_string(),
                });
                let pending = &mut lines[stream as usize];
                pending.push_str(text);
                while let Some(end) = pending.find('\n') {
                    log_line(pending[..end].trim_end_matches('\r'));
                    pending.drain(..=end);
                }
            },
        )
        .await;
        for rest in &lines {
            if !rest.is_empty() {
                log_line(rest);
            }
        }
        self.running.lock().unwrap().remove(&request.run_id);

        match &result {
            Ok(outcome) => self.events.log(
                if outcome.succeeded() {
                    LogLevel::Info
                } else {
                    LogLevel::Warn
                },
                session_id,
                format!("Command {}", outcome.describe()),
            ),
            Err(error) => self.events.log(
                LogLevel::Error,
                session_id,
                format!("Command failed: {}", error.message),
            ),
        }
        result
    }

    pub fn stop(&self, run_id: &str) {
        if let Some(cancel) = self.running.lock().unwrap().remove(run_id) {
            cancel.cancel();
        }
    }
}

/// Runs `command` through the server's shell on a channel of the session's connection,
/// handing output to `on_output` as it arrives.
pub async fn run_on(
    session: &Session,
    command: &str,
    directory: Option<&str>,
    cancel: &CancellationToken,
    mut on_output: impl FnMut(OutputStream, &str),
) -> AppResult<CommandResult> {
    let started = Instant::now();
    let command = match directory.map(str::trim).filter(|folder| !folder.is_empty()) {
        Some(folder) => format!("cd {} && {command}", shell_quote(folder)),
        None => command.to_string(),
    };
    let mut channel = session.open_command_channel().await?;
    channel.exec(true, command).await?;

    let mut decoders = [Utf8Decoder::default(), Utf8Decoder::default()];
    let mut result = CommandResult::default();
    loop {
        let message = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = channel.close().await;
                result.stopped = true;
                break;
            }
            message = channel.wait() => message,
        };
        let (stream, data) = match message {
            Some(ChannelMsg::Data { data }) => (OutputStream::Stdout, data),
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => (OutputStream::Stderr, data),
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                result.exit_status = Some(exit_status);
                continue;
            }
            Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                result.signal = Some(format!("{signal_name:?}"));
                continue;
            }
            Some(ChannelMsg::Failure) => {
                return Err(AppError::new(
                    ErrorKind::Ssh,
                    "The server does not allow running commands",
                ))
            }
            Some(ChannelMsg::Close) | None => break,
            Some(_) => continue,
        };
        let text = decoders[stream as usize].decode(&data);
        if !text.is_empty() {
            on_output(stream, &text);
        }
    }
    for (stream, decoder) in [OutputStream::Stdout, OutputStream::Stderr]
        .into_iter()
        .zip(&mut decoders)
    {
        let rest = decoder.finish();
        if !rest.is_empty() {
            on_output(stream, &rest);
        }
    }
    result.elapsed_millis = started.elapsed().as_millis() as u64;
    Ok(result)
}

/// Turns output into text without splitting a character that spans two packets.
#[derive(Default)]
struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let complete = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            // An incomplete character at the end waits for the next packet.
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => self.pending.len(),
        };
        let text = String::from_utf8_lossy(&self.pending[..complete]).into_owned();
        self.pending.drain(..complete);
        text
    }

    fn finish(&mut self) -> String {
        let text = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_split_across_packets_stay_whole() {
        let mut decoder = Utf8Decoder::default();
        let text = "größe ✓";
        let bytes = text.as_bytes();
        let split = text.find('ö').unwrap() + 1;
        let mut decoded = decoder.decode(&bytes[..split]);
        assert_eq!(decoded, "gr");
        decoded.push_str(&decoder.decode(&bytes[split..bytes.len() - 1]));
        decoded.push_str(&decoder.decode(&bytes[bytes.len() - 1..]));
        assert_eq!(decoded, text);
        assert_eq!(decoder.finish(), "");

        assert_eq!(decoder.decode(b"bad \xff byte"), "bad \u{fffd} byte");
        assert_eq!(decoder.decode(b"cut \xe2\x9c"), "cut ");
        assert_eq!(decoder.finish(), "\u{fffd}");
    }

    #[test]
    fn results_describe_how_the_command_ended() {
        let finished = CommandResult {
            exit_status: Some(0),
            ..Default::default()
        };
        assert!(finished.succeeded());
        assert_eq!(finished.describe(), "finished");
        let failed = CommandResult {
            exit_status: Some(2),
            ..Default::default()
        };
        assert!(!failed.succeeded());
        assert_eq!(failed.describe(), "exited with status 2");
        let stopped = CommandResult {
            stopped: true,
            ..Default::default()
        };
        assert_eq!(stopped.describe(), "stopped");
    }
}
