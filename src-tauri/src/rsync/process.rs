//! An rsync server process at the far end of an SSH exec channel, and the integer and
//! message framing of its protocol.

use std::time::Duration;

use bytes::{Buf, Bytes};
use russh::client::Msg;
use russh::{Channel, ChannelMsg, ChannelReadHalf, ChannelWriteHalf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{AppError, AppResult, ErrorKind};

/// Channel messages buffered ahead of the reader; beyond this the connection waits, which
/// keeps memory bounded and lets a speed limit hold back the server.
const RECEIVE_BUFFER_MESSAGES: usize = 256;
const WRITE_BUFFER_BYTES: usize = 256 * 1024;
const MAX_ERROR_TEXT: usize = 8 * 1024;
const MAX_SERVER_MESSAGES: usize = 20;
const EXIT_WAIT: Duration = Duration::from_secs(10);

const MULTIPLEX_BASE: u32 = 7;
const MESSAGE_DATA: u32 = 0;
const MESSAGE_ERROR_TRANSFER: u32 = 1;
const MESSAGE_INFO: u32 = 2;
const MESSAGE_ERROR: u32 = 3;
const MESSAGE_WARNING: u32 = 4;
const MESSAGE_ERROR_EXIT: u32 = 86;

/// Exit status of a shell that could not find the command.
const COMMAND_NOT_FOUND: u32 = 127;

#[derive(Debug, Default)]
struct Ending {
    exit_status: Option<u32>,
    signal: Option<String>,
    refused: bool,
    error_output: String,
}

pub struct RemoteProcess {
    incoming: mpsc::Receiver<Bytes>,
    current: Bytes,
    writer: Option<ChannelWriteHalf<Msg>>,
    outgoing: Vec<u8>,
    relay: Option<JoinHandle<Ending>>,
    ending: Option<Ending>,
    multiplexed: bool,
    frame_left: usize,
    /// Errors and warnings the server sent within the protocol.
    messages: Vec<String>,
}

impl RemoteProcess {
    pub async fn start(channel: Channel<Msg>, command: &str) -> AppResult<Self> {
        channel.exec(true, command).await?;
        let (reader, writer) = channel.split();
        let (sender, incoming) = mpsc::channel(RECEIVE_BUFFER_MESSAGES);
        Ok(Self {
            incoming,
            current: Bytes::new(),
            writer: Some(writer),
            outgoing: Vec::new(),
            relay: Some(tokio::spawn(relay(reader, sender))),
            ending: None,
            multiplexed: false,
            frame_left: 0,
            messages: Vec::new(),
        })
    }

    /// The server frames everything after the handshake.
    pub fn start_multiplexed_input(&mut self) {
        self.multiplexed = true;
    }

    pub async fn read_exact(&mut self, buffer: &mut [u8]) -> AppResult<()> {
        if !self.multiplexed {
            return self.read_raw(buffer).await;
        }
        let mut filled = 0;
        while filled < buffer.len() {
            if self.frame_left == 0 {
                self.read_frame_header().await?;
                continue;
            }
            let count = (buffer.len() - filled).min(self.frame_left);
            self.read_raw(&mut buffer[filled..filled + count]).await?;
            self.frame_left -= count;
            filled += count;
        }
        Ok(())
    }

    pub async fn read_byte(&mut self) -> AppResult<u8> {
        let mut buffer = [0; 1];
        self.read_exact(&mut buffer).await?;
        Ok(buffer[0])
    }

    pub async fn read_short(&mut self) -> AppResult<u16> {
        let mut buffer = [0; 2];
        self.read_exact(&mut buffer).await?;
        Ok(u16::from_le_bytes(buffer))
    }

    pub async fn read_int(&mut self) -> AppResult<i32> {
        let mut buffer = [0; 4];
        self.read_exact(&mut buffer).await?;
        Ok(i32::from_le_bytes(buffer))
    }

    /// A 32-bit value, or -1 followed by the full 64-bit one.
    pub async fn read_long(&mut self) -> AppResult<i64> {
        let short = self.read_int().await?;
        if short != -1 {
            return Ok(i64::from(short));
        }
        let mut buffer = [0; 8];
        self.read_exact(&mut buffer).await?;
        Ok(i64::from_le_bytes(buffer))
    }

    pub async fn read_bytes(&mut self, length: usize) -> AppResult<Vec<u8>> {
        let mut buffer = vec![0; length];
        self.read_exact(&mut buffer).await?;
        Ok(buffer)
    }

    /// A string with a one or two byte length.
    pub async fn read_short_string(&mut self) -> AppResult<Vec<u8>> {
        let mut length = usize::from(self.read_byte().await?);
        if length & 0x80 != 0 {
            length = (length & 0x7f) * 0x100 + usize::from(self.read_byte().await?);
        }
        self.read_bytes(length).await
    }

    pub fn write_byte(&mut self, value: u8) {
        self.outgoing.push(value);
    }

    pub fn write_short(&mut self, value: u16) {
        self.outgoing.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_int(&mut self, value: i32) {
        self.outgoing.extend_from_slice(&value.to_le_bytes());
    }

    pub fn write_long(&mut self, value: i64) {
        match i32::try_from(value) {
            Ok(short) if short >= 0 => self.write_int(short),
            _ => {
                self.write_int(-1);
                self.outgoing.extend_from_slice(&value.to_le_bytes());
            }
        }
    }

    pub fn write_bytes(&mut self, data: &[u8]) {
        self.outgoing.extend_from_slice(data);
    }

    pub fn write_short_string(&mut self, data: &[u8]) {
        if data.len() > 0x7f {
            self.write_byte(0x80 | (data.len() / 0x100) as u8);
        }
        self.write_byte(data.len() as u8);
        self.write_bytes(data);
    }

    pub async fn flush_if_full(&mut self) -> AppResult<()> {
        if self.outgoing.len() >= WRITE_BUFFER_BYTES {
            self.flush().await?;
        }
        Ok(())
    }

    pub async fn flush(&mut self) -> AppResult<()> {
        if self.outgoing.is_empty() {
            return Ok(());
        }
        let data = Bytes::from(std::mem::take(&mut self.outgoing));
        let (written, ending) = {
            let Some(writer) = self.writer.as_ref() else {
                return Err(self.ended_early().await);
            };
            // A process that dies never opens the window again, so watch for its end too.
            match self.relay.as_mut() {
                Some(relay) => tokio::select! {
                    written = writer.data_bytes(data) => (written.is_ok(), None),
                    ending = relay => (false, Some(ending.unwrap_or_default())),
                },
                None => (writer.data_bytes(data).await.is_ok(), None),
            }
        };
        if let Some(ending) = ending {
            self.relay = None;
            self.ending = Some(ending);
        }
        if written {
            Ok(())
        } else {
            Err(self.ended_early().await)
        }
    }

    /// Tells the process no input follows.
    pub async fn close_input(&mut self) {
        if let Some(writer) = self.writer.as_ref() {
            let _ = writer.eof().await;
        }
    }

    /// All output until the process closes it, up to `limit` bytes.
    pub async fn read_to_end(&mut self, limit: usize) -> Vec<u8> {
        let mut output = std::mem::take(&mut self.current).to_vec();
        while let Some(chunk) = self.incoming.recv().await {
            if output.len() < limit {
                output.extend_from_slice(&chunk[..chunk.len().min(limit - output.len())]);
            }
        }
        output
    }

    /// Ends the input and waits for the process to exit cleanly.
    pub async fn finish(mut self) -> AppResult<()> {
        self.flush().await?;
        if let Some(writer) = self.writer.as_ref() {
            let _ = writer.eof().await;
        }
        self.incoming.close();
        let ending = self.wait_for_end().await;
        if ending.exit_status == Some(0) {
            if let Some(writer) = self.writer.take() {
                let _ = writer.close().await;
            }
            Ok(())
        } else {
            Err(self.describe(&ending))
        }
    }

    /// How the process ended, once it does; empty if it does not end in time.
    async fn end(mut self) -> Ending {
        if let Some(writer) = self.writer.as_ref() {
            let _ = writer.eof().await;
        }
        self.incoming.close();
        self.wait_for_end().await
    }

    async fn read_raw(&mut self, mut buffer: &mut [u8]) -> AppResult<()> {
        while !buffer.is_empty() {
            if self.current.is_empty() {
                match self.incoming.recv().await {
                    Some(chunk) => self.current = chunk,
                    None => return Err(self.ended_early().await),
                }
                continue;
            }
            let count = buffer.len().min(self.current.len());
            buffer[..count].copy_from_slice(&self.current[..count]);
            self.current.advance(count);
            buffer = &mut buffer[count..];
        }
        Ok(())
    }

    async fn read_frame_header(&mut self) -> AppResult<()> {
        let mut header = [0; 4];
        self.read_raw(&mut header).await?;
        let header = u32::from_le_bytes(header);
        let length = (header & 0x00ff_ffff) as usize;
        let tag = (header >> 24).wrapping_sub(MULTIPLEX_BASE);
        if tag == MESSAGE_DATA {
            self.frame_left = length;
            return Ok(());
        }
        let mut payload = vec![0; length];
        self.read_raw(&mut payload).await?;
        match tag {
            MESSAGE_ERROR_TRANSFER | MESSAGE_ERROR | MESSAGE_WARNING | MESSAGE_INFO => {
                let text = String::from_utf8_lossy(&payload).trim().to_string();
                if !text.is_empty() && self.messages.len() < MAX_SERVER_MESSAGES {
                    self.messages.push(text);
                }
            }
            MESSAGE_ERROR_EXIT => return Err(self.ended_early().await),
            // Logging, statistics and keep-alives carry nothing a single file needs.
            _ => {}
        }
        Ok(())
    }

    /// What the server reported, or `fallback` when it said nothing.
    pub fn error(&self, fallback: &str) -> AppError {
        let message = if self.messages.is_empty() {
            fallback.to_string()
        } else {
            self.messages.join("; ")
        };
        AppError::new(ErrorKind::Rsync, message)
    }

    async fn ended_early(&mut self) -> AppError {
        let ending = self.wait_for_end().await;
        self.describe(&ending)
    }

    async fn wait_for_end(&mut self) -> Ending {
        if let Some(ending) = self.ending.take() {
            return ending;
        }
        let Some(relay) = self.relay.take() else {
            return Ending::default();
        };
        self.incoming.close();
        match tokio::time::timeout(EXIT_WAIT, relay).await {
            Ok(Ok(ending)) => ending,
            _ => Ending::default(),
        }
    }

    fn describe(&self, ending: &Ending) -> AppError {
        let error_output = ending.error_output.trim();
        let message = if ending.refused {
            "The server refused to run rsync".to_string()
        } else if ending.exit_status == Some(COMMAND_NOT_FOUND) {
            "rsync is not installed on the server".to_string()
        } else if !self.messages.is_empty() {
            self.messages.join("; ")
        } else if !error_output.is_empty() {
            error_output
                .lines()
                .last()
                .unwrap_or(error_output)
                .to_string()
        } else if let Some(signal) = &ending.signal {
            format!("rsync on the server was stopped by signal {signal}")
        } else if let Some(status) = ending.exit_status {
            format!("rsync on the server exited with status {status}")
        } else {
            "The rsync connection closed unexpectedly".to_string()
        };
        AppError::new(ErrorKind::Rsync, message)
    }
}

impl Drop for RemoteProcess {
    fn drop(&mut self) {
        if let Some(relay) = self.relay.take() {
            relay.abort();
        }
        // Closing the channel ends a server process still waiting for input.
        if let (Some(writer), Ok(runtime)) =
            (self.writer.take(), tokio::runtime::Handle::try_current())
        {
            runtime.spawn(async move {
                let _ = writer.close().await;
            });
        }
    }
}

/// Moves output from the channel into a bounded queue and keeps what stderr and the exit
/// status say, so the connection's other channels never wait on this one's reader.
async fn relay(mut reader: ChannelReadHalf, sender: mpsc::Sender<Bytes>) -> Ending {
    let mut sender = Some(sender);
    let mut ending = Ending::default();
    while let Some(message) = reader.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if let Some(open) = &sender {
                    if open.send(data).await.is_err() {
                        sender = None;
                    }
                }
            }
            ChannelMsg::ExtendedData { data, ext: 1 } => {
                let room = MAX_ERROR_TEXT.saturating_sub(ending.error_output.len());
                ending
                    .error_output
                    .push_str(&String::from_utf8_lossy(&data[..data.len().min(room)]));
            }
            ChannelMsg::ExitStatus { exit_status } => ending.exit_status = Some(exit_status),
            ChannelMsg::ExitSignal { signal_name, .. } => {
                ending.signal = Some(format!("{signal_name:?}"));
            }
            ChannelMsg::Failure => {
                ending.refused = true;
                sender = None;
            }
            ChannelMsg::Eof => sender = None,
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    ending
}

/// What a command printed and how it ended.
pub struct CommandOutput {
    pub exit_status: Option<u32>,
    pub output: Vec<u8>,
    /// The start of what it wrote to stderr.
    pub error_output: String,
}

/// Runs a command with no input and collects up to `limit` bytes of its output.
pub async fn run_command(
    channel: Channel<Msg>,
    command: &str,
    limit: usize,
) -> AppResult<CommandOutput> {
    let mut process = RemoteProcess::start(channel, command).await?;
    process.close_input().await;
    let output = process.read_to_end(limit).await;
    let ending = process.end().await;
    Ok(CommandOutput {
        exit_status: ending.exit_status,
        output,
        error_output: ending.error_output,
    })
}

/// Quotes an argument for a POSIX shell.
pub fn shell_quote(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_for_the_shell() {
        assert_eq!(shell_quote("/srv/a b"), "'/srv/a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
