//! The optional contents check after a file is complete: the server hashes its copy with its own
//! command while Poros hashes the local one. Both OpenSSH shells and SFTPGo offer `sha256sum`.

use std::path::PathBuf;

use super::worker::JobContext;
use crate::checksum::{hash_file, parse_output, Algorithm};
use crate::error::{AppError, AppResult};
use crate::events::LogLevel;
use crate::rsync::{run_command, shell_quote};

const OUTPUT_LIMIT: usize = 64 * 1024;

pub enum Checked {
    Same,
    Different,
    /// The server could not hash this file, or has no checksum command at all.
    Unavailable,
}

pub(super) async fn same_contents(
    context: &JobContext<'_>,
    remote_path: &str,
    local_path: &str,
) -> AppResult<Checked> {
    let Some(algorithm) = server_algorithm(context).await else {
        return Ok(Checked::Unavailable);
    };
    let local = {
        let path = PathBuf::from(local_path);
        tokio::task::spawn_blocking(move || hash_file(&path, algorithm))
    };
    // One file per command: SFTPGo hashes only the last argument.
    let command = format!("{} -- {}", algorithm.command(), shell_quote(remote_path));
    let remote = async {
        let channel = context.command_channel().await?;
        run_command(channel, &command, OUTPUT_LIMIT).await
    };
    let (local, remote) = tokio::select! {
        biased;
        _ = context.run.cancel.cancelled() => return Err(AppError::cancelled()),
        both = async { tokio::join!(local, remote) } => both,
    };
    let local = local?.map_err(|error| AppError::from(error).with_path(local_path))?;
    let output = remote?;
    let text = String::from_utf8_lossy(&output.output);
    match parse_output(&text, algorithm).into_iter().next() {
        Some((_, digest)) if output.exit_status == Some(0) => Ok(if digest == local {
            Checked::Same
        } else {
            Checked::Different
        }),
        _ => {
            context.shared.events.log(
                LogLevel::Warn,
                Some(&context.spec.session_id),
                format!(
                    "The server could not compute a checksum of {remote_path}: {}",
                    text.trim()
                ),
            );
            Ok(Checked::Unavailable)
        }
    }
}

/// The server's strongest checksum command, looked for once per server.
async fn server_algorithm(context: &JobContext<'_>) -> Option<Algorithm> {
    let target = context.shared.target(&context.spec.session_id).ok()?;
    let mut known = target.checksum.lock().await;
    if let Some(algorithm) = *known {
        return algorithm;
    }
    let mut found = None;
    for algorithm in Algorithm::PREFERRED {
        // With no file named, the commands hash their empty input, so the answer is known.
        let probed = async {
            let channel = context.command_channel().await?;
            run_command(channel, algorithm.command(), OUTPUT_LIMIT).await
        };
        let output = tokio::select! {
            biased;
            _ = context.run.cancel.cancelled() => return None,
            output = probed => output.ok()?,
        };
        let text = String::from_utf8_lossy(&output.output);
        let empty = algorithm.hasher().finish();
        let works = output.exit_status == Some(0)
            && parse_output(&text, algorithm)
                .first()
                .is_some_and(|(_, digest)| *digest == empty);
        if works {
            found = Some(algorithm);
            break;
        }
    }
    let message = match found {
        Some(algorithm) => format!(
            "Transfers to and from {} are checked with {} ({})",
            target.label,
            algorithm.label(),
            algorithm.command()
        ),
        None => format!(
            "{} has no sha256sum, sha1sum or md5sum command, so its transfers are checked by size only",
            target.label
        ),
    };
    context
        .shared
        .events
        .log(LogLevel::Info, Some(&context.spec.session_id), message);
    *known = Some(found);
    found
}
