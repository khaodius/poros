//! The preview pane's reads end to end: the start of a large file and a whole small image from
//! real servers of every kind, and from a vault on one of them. Each server is skipped unless
//! configured, with the variables the other end-to-end suites use:
//!
//! - `POROS_TEST_SSH_PORT`: SFTP.
//! - `POROS_TEST_FTP_PORT` and `POROS_TEST_FTPS_PORT`: plain FTP and explicit FTPS.
//! - `POROS_TEST_WEBDAV_PORT`: WebDAV over plain HTTP.
//! - `POROS_TEST_S3_PORT`: an S3-compatible service over plain HTTP that takes any keys.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::cloud::s3::{Addressing, S3Options};
use poros_lib::editor::FileLocation;
use poros_lib::error::{AppResult, ErrorKind};
use poros_lib::events::Events;
use poros_lib::file_preview::{FilePreviews, ReadReply, ReadRequest, MAX_READ_BYTES};
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobState, TransferItem, TransferManager,
};

const OWNER: &str = "main";
const MEBIBYTE: usize = 1024 * 1024;
const PASSWORD: &str = "a vault's p\u{e4}ssword";
/// A real picture, four pixels wide and high, part of it see-through.
const PICTURE: &[u8] = include_bytes!("fixtures/preview/checker.png");
/// As much of a text file as the preview reads unless told otherwise.
const TEXT_PART: u64 = 256 * 1024;

#[derive(Clone)]
struct Server {
    protocol: Protocol,
    port: u16,
    user: String,
    password: String,
}

fn server(port_variable: &str, protocol: Protocol) -> Option<Server> {
    let port = std::env::var(port_variable).ok()?.parse().ok()?;
    let variable =
        |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    let (user, password) = match protocol {
        Protocol::S3 => ("poros-access-key".into(), "poros-secret-key".into()),
        Protocol::Sftp => (
            variable("POROS_TEST_SSH_USER", "poros"),
            variable("POROS_TEST_SSH_PASSWORD", "poros-pass"),
        ),
        Protocol::WebDav => (
            variable("POROS_TEST_WEBDAV_USER", "poros"),
            variable("POROS_TEST_WEBDAV_PASSWORD", "poros-pass"),
        ),
        _ => (
            variable("POROS_TEST_FTP_USER", "poros"),
            variable("POROS_TEST_FTP_PASSWORD", "poros-pass"),
        ),
    };
    Some(Server {
        protocol,
        port,
        user,
        password,
    })
}

fn every_server() -> Vec<Server> {
    [
        server("POROS_TEST_SSH_PORT", Protocol::Sftp),
        server("POROS_TEST_FTP_PORT", Protocol::Ftp),
        server("POROS_TEST_FTPS_PORT", Protocol::Ftps),
        server("POROS_TEST_WEBDAV_PORT", Protocol::WebDav),
        server("POROS_TEST_S3_PORT", Protocol::S3),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn profile(server: &Server) -> ConnectProfile {
    let host = match server.protocol {
        Protocol::S3 => format!("http://127.0.0.1:{}", server.port),
        _ => "127.0.0.1".to_string(),
    };
    ConnectProfile {
        protocol: server.protocol,
        host,
        port: server.port,
        username: server.user.clone(),
        auth: AuthMethod::Password {
            password: server.password.clone(),
        },
        initial_path: None,
        timeout_secs: Some(10),
        keepalive_secs: None,
        compression: false,
        receive_buffer_kib: None,
        send_buffer_kib: None,
        saved_connection_id: None,
        ftp_active: false,
        bypass_proxy: false,
        jump_connection_id: None,
        rsync_path: None,
        s3: S3Options {
            region: "us-east-1".into(),
            addressing: Addressing::Path,
        },
        route: Route::default(),
    }
}

fn new_manager() -> (tempfile::TempDir, Arc<SessionManager>) {
    let config = tempfile::tempdir().unwrap();
    let manager = SessionManager::new(config.path().join("known_hosts"), Events::default());
    (config, Arc::new(manager))
}

/// Connects, approving the server's host key or certificate if the manager asks.
async fn connect(manager: &SessionManager, server: &Server) -> SessionInfo {
    let profile = profile(server);
    let label = profile.label();
    match manager.connect(profile.clone(), None, OWNER).await {
        Ok(info) => info,
        Err(error) => {
            let host_key = error
                .host_key
                .unwrap_or_else(|| panic!("{label}: {}", error.message));
            let approval = HostKeyApproval {
                fingerprint: host_key.fingerprint,
                remember: true,
            };
            manager
                .connect(profile, Some(approval), OWNER)
                .await
                .unwrap_or_else(|error| panic!("{label}: {}", error.message))
        }
    }
}

async fn files_of(manager: &SessionManager, session_id: &str) -> Arc<dyn RemoteFileSystem> {
    manager.get(session_id).await.unwrap().files()
}

/// A session with an empty scratch folder (a bucket on S3) in its home folder.
async fn open_scratch(
    manager: &SessionManager,
    server: &Server,
    name: &str,
) -> (SessionInfo, Arc<dyn RemoteFileSystem>, String) {
    let session = connect(manager, server).await;
    let files = files_of(manager, &session.id).await;
    let scratch = format!("poros-preview-pane-{name}-{}", std::process::id());
    let existing = remote_path::join(&session.home, &scratch);
    if files.stat(&existing).await.unwrap().is_some() {
        files.delete(std::slice::from_ref(&existing)).await.unwrap();
    }
    let remote = files.make_dir("~", &scratch).await.unwrap();
    (session, files, remote)
}

/// Deterministic bytes that do not compress, so a misplaced piece shows up.
fn pattern(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

async fn write_remote(files: &Arc<dyn RemoteFileSystem>, path: &str, contents: &[u8]) {
    let mut stream = files
        .clone()
        .open_write(WriteRequest {
            path: path.to_string(),
            offset: 0,
            size: contents.len() as u64,
            modified: None,
            permissions: None,
        })
        .await
        .unwrap();
    for chunk in contents.chunks(100_000) {
        stream.write(Bytes::copy_from_slice(chunk)).await.unwrap();
    }
    stream.finish().await.unwrap();
}

fn request(session_id: &str, path: &str, offset: u64, length: u64) -> ReadRequest {
    static READS: AtomicU64 = AtomicU64::new(0);
    ReadRequest {
        request_id: format!("read-{}", READS.fetch_add(1, Ordering::Relaxed)),
        location: FileLocation::Remote {
            session_id: session_id.to_string(),
            path: path.to_string(),
        },
        offset,
        length,
    }
}

/// A read with no transfers about.
async fn read(
    previews: &FilePreviews,
    session_id: &str,
    path: &str,
    offset: u64,
    length: u64,
) -> AppResult<ReadReply> {
    previews
        .read(request(session_id, path, offset, length), |_| false)
        .await
}

#[tokio::test]
async fn the_start_of_a_large_file_and_a_whole_picture_are_read_from_every_server() {
    for server in every_server() {
        let label = format!("{:?}", server.protocol);
        let (_config, manager) = new_manager();
        let (session, files, scratch) = open_scratch(&manager, &server, "reads").await;
        let previews = FilePreviews::new(manager.clone());

        let large = pattern(6 * MEBIBYTE + 321, 3);
        let large_path = remote_path::join(&scratch, "server.log");
        write_remote(&files, &large_path, &large).await;
        let picture_path = remote_path::join(&scratch, "picture with spaces \u{2713}.png");
        write_remote(&files, &picture_path, PICTURE).await;
        let empty_path = remote_path::join(&scratch, "empty.txt");
        write_remote(&files, &empty_path, b"").await;

        // Only the beginning of the large file, with the size the server has for it.
        let started = Instant::now();
        let start = read(&previews, &session.id, &large_path, 0, TEXT_PART)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_eq!(start.contents.len() as u64, TEXT_PART, "{label}");
        assert!(
            start.contents == large[..TEXT_PART as usize],
            "{label}: the start of the file differs"
        );
        assert_eq!(start.stamp.size, large.len() as u64, "{label}");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{label}: reading the start took {:?}",
            started.elapsed()
        );

        // The whole picture, asked for with far more room than it needs.
        let picture = read(&previews, &session.id, &picture_path, 0, MAX_READ_BYTES)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_eq!(picture.contents, PICTURE, "{label}");
        assert_eq!(picture.stamp.size, PICTURE.len() as u64, "{label}");
        if server.protocol != Protocol::Ftp && server.protocol != Protocol::Ftps {
            assert!(picture.stamp.modified.is_some(), "{label}");
        }

        // A range from the middle, and one that runs past the end.
        let middle = read(&previews, &session.id, &large_path, 2_000_001, 70_000)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert!(
            middle.contents == large[2_000_001..2_070_001],
            "{label}: the middle of the file differs"
        );
        let tail_from = large.len() as u64 - 1000;
        let tail = read(&previews, &session.id, &large_path, tail_from, 1 << 20)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert!(
            tail.contents == large[large.len() - 1000..],
            "{label}: the end of the file differs"
        );

        let empty = read(&previews, &session.id, &empty_path, 0, TEXT_PART)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert!(empty.contents.is_empty(), "{label}");
        assert_eq!(empty.stamp.size, 0, "{label}");

        // A file deleted since its folder was listed, and a folder in place of a file.
        let missing_path = remote_path::join(&scratch, "deleted meanwhile.png");
        let missing = read(&previews, &session.id, &missing_path, 0, TEXT_PART)
            .await
            .unwrap_err();
        assert_eq!(
            missing.kind,
            ErrorKind::NotFound,
            "{label}: {}",
            missing.message
        );
        let folder = read(&previews, &session.id, &scratch, 0, TEXT_PART)
            .await
            .unwrap_err();
        assert_eq!(
            folder.kind,
            ErrorKind::InvalidInput,
            "{label}: {}",
            folder.message
        );
        let climbing = format!("{scratch}/../{}", remote_path::file_name(&scratch));
        let refused = read(&previews, &session.id, &climbing, 0, TEXT_PART)
            .await
            .unwrap_err();
        assert_eq!(refused.kind, ErrorKind::InvalidInput, "{label}");

        // The connection still lists after all of that.
        let names: Vec<String> = files
            .list_dir(&scratch)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message))
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names.len(), 3, "{label}: {names:?}");

        files.delete(std::slice::from_ref(&scratch)).await.unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}

#[tokio::test]
async fn a_cancelled_read_leaves_the_connection_ready_for_the_next() {
    for server in every_server() {
        let label = format!("{:?}", server.protocol);
        let (_config, manager) = new_manager();
        let (session, files, scratch) = open_scratch(&manager, &server, "cancel").await;
        let previews = Arc::new(FilePreviews::new(manager.clone()));

        let large = pattern(12 * MEBIBYTE, 9);
        let large_path = remote_path::join(&scratch, "photo.raw");
        write_remote(&files, &large_path, &large).await;

        // Cancelled at different moments: before the server is asked, while it is, and while
        // the bytes arrive.
        for delay_millis in [0, 1, 3, 10] {
            let mut whole = request(&session.id, &large_path, 0, MAX_READ_BYTES);
            whole.request_id = format!("cancelled-{delay_millis}");
            let reading = {
                let (previews, whole) = (previews.clone(), whole.clone());
                tokio::spawn(async move { previews.read(whole, |_| false).await })
            };
            tokio::time::sleep(Duration::from_millis(delay_millis)).await;
            previews.cancel(&whole.request_id);
            match reading.await.unwrap() {
                // A fast server may have sent everything already.
                Ok(reply) => assert_eq!(reply.contents.len(), large.len(), "{label}"),
                Err(error) => assert_eq!(
                    error.kind,
                    ErrorKind::Cancelled,
                    "{label}: {}",
                    error.message
                ),
            }

            let next = read(&previews, &session.id, &large_path, 5_000_000, 4096)
                .await
                .unwrap_or_else(|error| panic!("{label} after {delay_millis} ms: {error:?}"));
            assert!(
                next.contents == large[5_000_000..5_004_096],
                "{label}: the read after a cancelled one differs"
            );
        }
        let listed = files
            .list_dir(&scratch)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_eq!(listed.entries.len(), 1, "{label}");

        files.delete(std::slice::from_ref(&scratch)).await.unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}

#[tokio::test]
async fn a_file_in_a_vault_is_read_decrypted_until_the_vault_is_locked() {
    let Some(server) = server("POROS_TEST_SSH_PORT", Protocol::Sftp) else {
        return;
    };
    let (_config, manager) = new_manager();
    let (session, server_files, scratch) = open_scratch(&manager, &server, "vault").await;
    let vault = manager
        .create_vault(&session.id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    let root = remote_path::join(&scratch, "Vault");
    let vault_files = files_of(&manager, &vault.id).await;
    let previews = FilePreviews::new(manager.clone());

    // Several of the pieces a vault encrypts a file in, the last one partly filled.
    let source = pattern(200_000, 5);
    let source_path = remote_path::join(&root, "main.rs");
    write_remote(&vault_files, &source_path, &source).await;
    let picture_path = remote_path::join(&root, "logo.png");
    write_remote(&vault_files, &picture_path, PICTURE).await;

    let start = read(&previews, &vault.id, &source_path, 0, 50_000)
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    assert!(start.contents == source[..50_000]);
    assert_eq!(start.stamp.size, source.len() as u64);
    // From inside one piece to inside another.
    let across = read(&previews, &vault.id, &source_path, 30_000, 100_000)
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    assert!(across.contents == source[30_000..130_000]);
    let picture = read(&previews, &vault.id, &picture_path, 0, MAX_READ_BYTES)
        .await
        .unwrap_or_else(|error| panic!("{}", error.message));
    assert_eq!(picture.contents, PICTURE);

    // The server's own session shows the vault's folder, with nothing readable in it.
    let on_server = read(&previews, &session.id, &source_path, 0, 50_000)
        .await
        .unwrap_err();
    assert_eq!(on_server.kind, ErrorKind::NotFound);

    manager.disconnect(&vault.id).await.unwrap();
    let locked = read(&previews, &vault.id, &source_path, 0, 50_000)
        .await
        .unwrap_err();
    assert_eq!(
        locked.kind,
        ErrorKind::SessionNotFound,
        "{}",
        locked.message
    );

    server_files
        .delete(std::slice::from_ref(&scratch))
        .await
        .unwrap();
    manager.disconnect(&session.id).await.unwrap();
}

async fn wait_until_settled(transfers: &TransferManager) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let list = transfers.list();
        let counts = &list.stats.counts;
        if counts.queued == 0 && counts.running == 0 {
            let unfinished: Vec<_> = list
                .jobs
                .iter()
                .filter(|job| job.state != JobState::Done)
                .collect();
            assert!(unfinished.is_empty(), "{unfinished:#?}");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "transfers did not finish: {counts:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// On FTP a transfer that borrows the browsing connection keeps it for the whole file, so a
/// preview is told the connection is busy instead of waiting; with connections of their own,
/// transfers leave the browsing connection to the preview.
#[tokio::test]
async fn a_preview_does_not_wait_on_the_connection_ftp_transfers_borrow() {
    let Some(server) = server("POROS_TEST_FTP_PORT", Protocol::Ftp) else {
        return;
    };
    for separate_connections in [false, true] {
        let label = format!("separate connections: {separate_connections}");
        let (_config, manager) = new_manager();
        let (session, files, scratch) = open_scratch(&manager, &server, "busy").await;
        let previews = FilePreviews::new(manager.clone());
        let notes_path = remote_path::join(&scratch, "notes.txt");
        write_remote(&files, &notes_path, b"Read between transfers").await;

        let local = tempfile::tempdir().unwrap();
        let upload = local.path().join("upload.bin");
        std::fs::write(&upload, pattern(6 * MEBIBYTE, 4)).unwrap();
        let transfers = TransferManager::new(
            manager.clone(),
            Events::default(),
            TransferSettings {
                auto_start: false,
                separate_connections,
                // Slow enough that the upload is still going when the preview asks.
                upload_limit_kib: 4096,
                exists_action: ExistsAction::Overwrite,
                retry_attempts: 0,
                ..TransferSettings::default()
            },
        );
        let read_notes = || async {
            previews
                .read(request(&session.id, &notes_path, 0, TEXT_PART), |files| {
                    transfers.borrows_connection_of(files)
                })
                .await
        };

        transfers
            .enqueue(EnqueueRequest {
                session_id: session.id.clone(),
                source_session_id: None,
                direction: Direction::Upload,
                target_directory: scratch.clone(),
                items: vec![TransferItem {
                    path: upload.to_string_lossy().into_owned(),
                    name: "upload.bin".into(),
                    is_dir: false,
                    size: 6 * MEBIBYTE as u64,
                }],
            })
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));

        // Transfers waiting for Start use no connection.
        let waiting = read_notes()
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_eq!(waiting.contents, b"Read between transfers", "{label}");

        transfers.set_paused(false);
        let during = read_notes().await;
        if separate_connections {
            let reply = during.unwrap_or_else(|error| panic!("{label}: {}", error.message));
            assert_eq!(reply.contents, b"Read between transfers", "{label}");
        } else {
            let error = during.unwrap_err();
            assert_eq!(error.kind, ErrorKind::Busy, "{label}: {}", error.message);
        }

        wait_until_settled(&transfers).await;
        let after = read_notes()
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_eq!(after.contents, b"Read between transfers", "{label}");

        files.delete(std::slice::from_ref(&scratch)).await.unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}
