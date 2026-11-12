//! A shell kept open on an SFTP server for checksum commands. A command of its own costs a
//! channel, a few round trips and a fresh login shell on the server for every file; through one
//! shell per worker connection, a file's checksum costs one round trip and the command itself.

use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

use russh::client::Msg;
use russh::Channel;

use super::worker::JobContext;
use crate::error::{AppError, AppResult};
use crate::rsync::{CommandOutput, RemoteProcess};

/// A server that has not answered by then is taken not to run a shell.
const START_WAIT: Duration = Duration::from_secs(15);

/// Where a worker connection keeps its shell between files.
pub(super) type ShellSlot = Mutex<Option<HashShell>>;

pub(super) struct HashShell {
    process: RemoteProcess,
    /// Ends each command's output. Random, so no file name the server prints can pass for it.
    marker: String,
}

impl HashShell {
    async fn start(channel: Channel<Msg>, limit: usize) -> AppResult<Self> {
        let process = RemoteProcess::start(channel, "sh").await?;
        let mut shell = Self {
            process,
            marker: format!("poros-end-{}", uuid::Uuid::new_v4().simple()),
        };
        // Anything a login script prints comes before the first answer, and is passed over.
        let (_, status) = shell.run("true", limit).await?;
        if status != 0 {
            return Err(AppError::unsupported(
                "The server's shell does not run commands",
            ));
        }
        Ok(shell)
    }

    /// Runs one command with no input, and returns what it printed, errors included, and its
    /// exit status.
    async fn run(&mut self, command: &str, limit: usize) -> AppResult<(Vec<u8>, i32)> {
        let line = format!(
            "{command} </dev/null 2>&1; printf '\\n%s %s\\n' {} \"$?\"\n",
            self.marker
        );
        self.process.write_bytes(line.as_bytes());
        self.process.flush().await?;
        let end = format!("\n{} ", self.marker);
        let mut output = Vec::new();
        loop {
            let chunk = self.process.read_chunk().await?;
            output.extend_from_slice(&chunk);
            if let Some(answer) = answer(&output, end.as_bytes())? {
                return Ok(answer);
            }
            if output.len() > limit + end.len() + 16 {
                return Err(AppError::unsupported("The server's shell printed too much"));
            }
        }
    }
}

/// The output before the end marker and the status after it, once the status line is whole.
fn answer(output: &[u8], end: &[u8]) -> AppResult<Option<(Vec<u8>, i32)>> {
    let Some(at) = output.windows(end.len()).position(|window| window == end) else {
        return Ok(None);
    };
    let rest = &output[at + end.len()..];
    let Some(line_end) = rest.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };
    let status = std::str::from_utf8(&rest[..line_end])
        .ok()
        .and_then(|status| status.trim().parse().ok());
    // Anything after the status line would mean the shell and Poros no longer agree on
    // which output is whose.
    match status {
        Some(status) if line_end + 1 == rest.len() => Ok(Some((output[..at].to_vec(), status))),
        _ => Err(AppError::unsupported(
            "The server's shell answered out of turn",
        )),
    }
}

/// Runs a checksum command through the worker connection's shell, starting one if there is
/// none yet. `None` when the command has to run on a channel of its own instead: the worker
/// borrows the browsing connection, the server runs no shell, or the shell went away.
pub(super) async fn run(
    context: &JobContext<'_>,
    command: &str,
    limit: usize,
) -> Option<CommandOutput> {
    let (slot, handle) = (context.hash_shell?, context.handle?);
    let target = context.shared.target(&context.spec.session_id).ok()?;
    if target.shell_refused.load(Ordering::Relaxed) {
        return None;
    }
    let kept = slot.lock().unwrap().take();
    let mut shell = match kept {
        Some(shell) => shell,
        None => {
            let channel = handle.channel_open_session().await.ok()?;
            match tokio::time::timeout(START_WAIT, HashShell::start(channel, limit)).await {
                Ok(Ok(shell)) => shell,
                _ => {
                    target.shell_refused.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        }
    };
    // A shell dropped mid-command, as when the transfer is cancelled, closes its channel.
    let (output, status) = shell.run(command, limit).await.ok()?;
    *slot.lock().unwrap() = Some(shell);
    Some(CommandOutput {
        exit_status: u32::try_from(status).ok(),
        output,
        error_output: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_answer_once_its_status_line_is_whole() {
        let end = b"\nMARK ";
        assert!(answer(b"abc  file\n", end).unwrap().is_none());
        assert!(answer(b"abc  file\n\nMARK 0", end).unwrap().is_none());
        let (output, status) = answer(b"abc  file\n\nMARK 0\n", end).unwrap().unwrap();
        assert_eq!(output, b"abc  file\n");
        assert_eq!(status, 0);
        let (output, status) = answer(b"no newline\nMARK 1\n", end).unwrap().unwrap();
        assert_eq!(output, b"no newline");
        assert_eq!(status, 1);
        assert!(answer(b"\nMARK 0\nmore", end).is_err());
        assert!(answer(b"\nMARK x\n", end).is_err());
    }
}
