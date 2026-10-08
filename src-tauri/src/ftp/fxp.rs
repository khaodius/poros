//! FXP: one FTP server sends a file straight to another while Poros only issues the commands.
//! The source listens (`PASV`), the target connects to it (`PORT`), and the data never passes
//! through this computer. Most servers refuse it unless configured to allow it, so a refusal
//! comes back as `Unsupported` and the caller relays the file instead.

use std::net::IpAddr;

use tokio_util::sync::CancellationToken;

use super::control::{port_command, reply_error, Control};
use super::FtpFs;
use crate::error::{AppError, AppResult, ErrorKind};

/// Whether two connections could try FXP at all: both FTP, and either both in the clear or
/// both encrypted with servers that can take the TLS client role (`SSCN`).
pub fn possible(source: &FtpFs, target: &FtpFs) -> bool {
    let source_tls = source.profile.protocol != crate::protocol::Protocol::Ftp;
    let target_tls = target.profile.protocol != crate::protocol::Protocol::Ftp;
    match (source_tls, target_tls) {
        (false, false) => true,
        (true, true) => source.features.sscn && target.features.sscn,
        _ => false,
    }
}

/// Copies `source_path` to `target_path` from `offset` on.
pub async fn copy(
    source: &FtpFs,
    source_path: &str,
    target: &FtpFs,
    target_path: &str,
    offset: u64,
    cancel: &CancellationToken,
) -> AppResult<()> {
    let mut source_guard = source.connection().await?;
    let mut target_guard = target.connection().await?;
    let source_control = source_guard.as_mut().expect("connection() always connects");
    let target_control = target_guard.as_mut().expect("connection() always connects");
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(AppError::cancelled()),
        result = run(source_control, source_path, target_control, target_path, offset) => result,
    };
    // A refused or interrupted exchange leaves both servers mid-conversation; start afresh.
    if result.is_err() {
        *source_guard = None;
        *target_guard = None;
    }
    result
}

async fn run(
    source: &mut Control,
    source_path: &str,
    target: &mut Control,
    target_path: &str,
    offset: u64,
) -> AppResult<()> {
    super::control::check_argument(source_path)?;
    super::control::check_argument(target_path)?;
    let encrypted = source.is_encrypted() && target.is_encrypted();
    if encrypted {
        // The target opens the data connection, so it plays the TLS client.
        target.expect("SSCN ON", 2).await.map_err(refused)?;
    }
    let address = source.passive_address().await.map_err(refused)?;
    let port = match address.ip() {
        IpAddr::V4(ip) => port_command(ip, address.port()),
        IpAddr::V6(ip) => format!("EPRT |2|{ip}|{}|", address.port()),
    };
    target.expect(&port, 2).await.map_err(refused)?;
    if offset > 0 {
        source
            .expect(&format!("REST {offset}"), 3)
            .await
            .map_err(refused)?;
        target
            .expect(&format!("REST {offset}"), 3)
            .await
            .map_err(refused)?;
    }
    target.send(&format!("STOR {target_path}")).await?;
    source.send(&format!("RETR {source_path}")).await?;
    // A target that refuses never connects, and the source would wait for it until it gives
    // up, so its answer is not awaited then.
    let target_reply = target.read_reply().await?;
    if target_reply.class() != 1 {
        return Err(refused(reply_error(&target_reply)));
    }
    let source_reply = source.read_reply().await?;
    if source_reply.class() != 1 {
        return Err(refused(reply_error(&source_reply)));
    }
    let (source_done, target_done) = tokio::join!(
        source.finish_direct_transfer(),
        target.finish_direct_transfer()
    );
    source_done?;
    target_done?;
    if encrypted {
        let _ = target.command("SSCN OFF").await;
    }
    Ok(())
}

/// Errors before the data starts moving mean the servers will not do FXP with each other.
fn refused(error: AppError) -> AppError {
    match error.kind {
        ErrorKind::Disconnected | ErrorKind::Timeout | ErrorKind::Cancelled => error,
        _ => AppError::unsupported(format!(
            "The servers would not transfer directly: {}",
            error.message
        )),
    }
}
