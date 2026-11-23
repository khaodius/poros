//! FXP: one FTP server sends a file straight to another while Poros only issues the commands.
//! The source listens (`PASV`), the target connects to it (`PORT`), and the data never passes
//! through this computer. Most servers refuse it unless configured to allow it, so a refusal
//! comes back as `Unsupported` and the caller relays the file instead.

use std::net::IpAddr;

use tokio_util::sync::CancellationToken;

use super::control::{port_command, reply_error, Control, Reply};
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
    let finished = finish(source, target).await;
    if finished
        .as_ref()
        .is_err_and(|error| error.kind == ErrorKind::Unsupported)
    {
        // Neither server is left waiting for a connection that will not come.
        for control in [source, target] {
            let limit = control.timeout();
            let _ = tokio::time::timeout(limit, control.send("ABOR")).await;
        }
        return finished;
    }
    finished?;
    if encrypted {
        let _ = target.command("SSCN OFF").await;
    }
    Ok(())
}

/// Waits for both servers to end a transfer the target has begun.
///
/// The source announces the transfer once the target has connected to it, or at once and then
/// waits for the connection. A target that cannot connect ends its side with an error, or says
/// nothing more, and the source would wait on: the copy is then refused, so the caller relays
/// the file. A source that announces at once facing a target that falls silent cannot be told
/// from a long transfer, and is waited on like one.
async fn finish(source: &mut Control, target: &mut Control) -> AppResult<()> {
    let target_end = target.direct_transfer_end();
    tokio::pin!(target_end);
    let mut target_ended = None;
    let started = tokio::select! {
        biased;
        reply = source.read_reply() => reply,
        reply = &mut target_end => {
            let reply = reply?;
            if reply.class() != 2 {
                // The target gave up before the source began, so it never connected.
                return Err(refused(reply_error(&reply)));
            }
            target_ended = Some(reply);
            source.read_reply().await
        }
    };
    let started = started.map_err(|error| match error.kind {
        ErrorKind::Timeout => refused(AppError::new(
            ErrorKind::Ftp,
            "the target server did not connect to the source",
        )),
        _ => error,
    })?;
    if started.class() != 1 {
        return Err(refused(reply_error(&started)));
    }
    let source_end = source.direct_transfer_end();
    tokio::pin!(source_end);
    let (source_ended, target_ended) = match target_ended {
        Some(target_ended) => (source_end.await?, target_ended),
        None => tokio::select! {
            biased;
            reply = &mut target_end => {
                let reply = reply?;
                if never_connected(&reply) {
                    return Err(refused(reply_error(&reply)));
                }
                (source_end.await?, reply)
            }
            reply = &mut source_end => {
                let reply = reply?;
                if never_connected(&reply) {
                    return Err(refused(reply_error(&reply)));
                }
                (reply, target_end.await?)
            }
        },
    };
    for reply in [source_ended, target_ended] {
        if reply.class() != 2 {
            return Err(reply_error(&reply));
        }
    }
    Ok(())
}

/// `425`: the data connection was never made, so nothing moved.
fn never_connected(reply: &Reply) -> bool {
    reply.code == 425
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::fake_server::{Behavior, ConnectingBack, FakeServer};
    use super::*;

    /// A source that announces the transfer at once and waits for the target, and one that
    /// announces it only once the target has connected, as vsftpd does.
    fn sources() -> [Behavior; 2] {
        [
            Behavior::default(),
            Behavior {
                announces_once_connected: true,
                ..Behavior::default()
            },
        ]
    }

    /// Each server gets its restart position right before its own transfer command, so a
    /// server that forgets the position at any other command still continues the file.
    #[tokio::test]
    async fn a_direct_copy_continues_where_it_stopped_on_strict_servers() {
        for source_behavior in sources() {
            copy_from_an_offset(source_behavior).await;
        }
    }

    async fn copy_from_an_offset(source_behavior: Behavior) {
        let contents: Vec<u8> = (0..5000u32).map(|index| (index % 251) as u8).collect();
        let offset = 1000;
        let source_server = FakeServer::start(source_behavior).await;
        let target_server = FakeServer::start(Behavior::default()).await;
        source_server.put_file("/large.bin", &contents);
        let mut partial = contents[..offset].to_vec();
        partial[..100].fill(0);
        target_server.put_file("/copy.bin", &partial);
        let source = source_server.connect(&source_server.profile()).await;
        let target = target_server.connect(&target_server.profile()).await;
        source_server.take_commands();
        target_server.take_commands();

        copy(
            &source,
            "/large.bin",
            &target,
            "/copy.bin",
            offset as u64,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        let mut expected = contents.clone();
        expected[..100].fill(0);
        assert_eq!(target_server.file("/copy.bin"), Some(expected));
        assert_eq!(
            source_server.take_commands(),
            ["PASV", "REST 1000", "RETR /large.bin"]
        );
        let target_commands = target_server.take_commands();
        assert!(
            target_commands[0].starts_with("PORT "),
            "{target_commands:?}"
        );
        assert_eq!(target_commands[1..], ["REST 1000", "STOR /copy.bin"]);
    }

    /// Copies a file between a source and a target that announces the transfer and then does
    /// not connect, and returns how the copy ended, failing if it waits past `limit`.
    async fn copy_to_a_target_that_does_not_connect(
        source_behavior: Behavior,
        connecting_back: ConnectingBack,
        reply_timeout_secs: u64,
        limit: Duration,
    ) -> AppError {
        let source_server = FakeServer::start(source_behavior).await;
        let target_server = FakeServer::start(Behavior {
            connecting_back,
            ..Behavior::default()
        })
        .await;
        source_server.put_file("/notes.txt", b"abc");
        let mut source_profile = source_server.profile();
        source_profile.timeout_secs = Some(reply_timeout_secs);
        let source = source_server.connect(&source_profile).await;
        let target = target_server.connect(&target_server.profile()).await;
        let copied = tokio::time::timeout(
            limit,
            copy(
                &source,
                "/notes.txt",
                &target,
                "/notes.txt",
                0,
                &CancellationToken::new(),
            ),
        )
        .await
        .expect("the copy gives up on the target without waiting for it");
        let error = copied.unwrap_err();
        // Neither server is left waiting for a connection that will not come.
        assert!(source_server.receives("ABOR").await, "{error:?}");
        assert!(target_server.receives("ABOR").await, "{error:?}");
        assert_eq!(target_server.file("/notes.txt"), None);
        error
    }

    /// A target that says it cannot connect, before or after the source has announced the
    /// transfer, refuses the copy at once, so the caller relays the file instead.
    #[tokio::test]
    async fn a_target_that_cannot_connect_hands_the_copy_back_to_be_relayed() {
        for source_behavior in sources() {
            let error = copy_to_a_target_that_does_not_connect(
                source_behavior.clone(),
                ConnectingBack::FailsAfterAnnouncing,
                60,
                Duration::from_secs(10),
            )
            .await;
            assert_eq!(
                error.kind,
                ErrorKind::Unsupported,
                "{source_behavior:?}: {error:?}"
            );
        }
    }

    /// A target that falls silent leaves a source that announces only once connected without
    /// a reply, which then means the target never connected.
    #[tokio::test]
    async fn a_target_that_never_connects_hands_the_copy_back_to_be_relayed() {
        let error = copy_to_a_target_that_does_not_connect(
            Behavior {
                announces_once_connected: true,
                ..Behavior::default()
            },
            ConnectingBack::HangsAfterAnnouncing,
            1,
            Duration::from_secs(10),
        )
        .await;
        assert_eq!(error.kind, ErrorKind::Unsupported, "{error:?}");
    }
}
