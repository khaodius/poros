//! End-to-end tests against WebDAV and S3 servers. Each test is skipped unless the servers it
//! needs are configured:
//!
//! - `POROS_TEST_WEBDAV_PORT`: WebDAV over plain HTTP, with Basic and Digest sign-in.
//! - `POROS_TEST_WEBDAVS_PORT`: WebDAV over HTTPS with a self-signed certificate.
//! - `POROS_TEST_S3_PORT`: an S3-compatible service over plain HTTP that takes any keys.
//!
//! The servers listen on 127.0.0.1. The WebDAV ones accept `POROS_TEST_WEBDAV_USER` with
//! `POROS_TEST_WEBDAV_PASSWORD`. The CI workflow shows a throwaway setup with WsgiDAV and moto.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::cloud::s3::{Addressing, S3Options};
use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::model::EntryKind;
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::share::{self, Audience, Expiry, LinkKind, ShareOptions};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobState, TransferItem, TransferList, TransferManager,
};

const OWNER: &str = "main";
const MEBIBYTE: usize = 1024 * 1024;

#[derive(Clone)]
struct Server {
    protocol: Protocol,
    port: u16,
    user: String,
    password: String,
}

fn server(port_variable: &str, protocol: Protocol) -> Option<Server> {
    let port = std::env::var(port_variable).ok()?.parse().ok()?;
    let (user, password) = match protocol {
        Protocol::S3 => (
            "poros-access-key".to_string(),
            "poros-secret-key".to_string(),
        ),
        _ => (
            std::env::var("POROS_TEST_WEBDAV_USER").unwrap_or_else(|_| "poros".into()),
            std::env::var("POROS_TEST_WEBDAV_PASSWORD").unwrap_or_else(|_| "poros-pass".into()),
        ),
    };
    Some(Server {
        protocol,
        port,
        user,
        password,
    })
}

fn webdav_servers() -> Vec<Server> {
    [
        server("POROS_TEST_WEBDAV_PORT", Protocol::WebDav),
        server("POROS_TEST_WEBDAVS_PORT", Protocol::WebDavs),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn every_server() -> Vec<Server> {
    let mut servers = webdav_servers();
    servers.extend(server("POROS_TEST_S3_PORT", Protocol::S3));
    servers
}

fn profile(server: &Server, password: &str) -> ConnectProfile {
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
            password: password.to_string(),
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

/// Connects, approving the server's certificate if the manager asks.
async fn connect_with(manager: &SessionManager, profile: ConnectProfile) -> SessionInfo {
    let label = profile.label();
    match manager.connect(profile.clone(), None, OWNER).await {
        Ok(info) => info,
        Err(error) => {
            let certificate = error
                .host_key
                .unwrap_or_else(|| panic!("{label}: {}", error.message));
            manager
                .connect(
                    profile,
                    Some(HostKeyApproval {
                        fingerprint: certificate.fingerprint,
                        remember: true,
                    }),
                    OWNER,
                )
                .await
                .unwrap_or_else(|error| panic!("{label}: {}", error.message))
        }
    }
}

async fn files_of(manager: &SessionManager, session: &SessionInfo) -> Arc<dyn RemoteFileSystem> {
    manager.get(&session.id).await.unwrap().files()
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

async fn read_remote(
    files: &Arc<dyn RemoteFileSystem>,
    path: &str,
    range: std::ops::Range<u64>,
) -> Vec<u8> {
    let mut stream = files.clone().open_read(path, range).await.unwrap();
    let mut contents = Vec::new();
    while let Some(chunk) = stream.next_chunk().await.unwrap() {
        contents.extend_from_slice(&chunk);
    }
    stream.finish().await.unwrap();
    contents
}

async fn write_remote(
    files: &Arc<dyn RemoteFileSystem>,
    path: &str,
    contents: &[u8],
    modified: Option<i64>,
) {
    let mut stream = files
        .clone()
        .open_write(WriteRequest {
            path: path.to_string(),
            offset: 0,
            size: contents.len() as u64,
            modified,
            permissions: None,
        })
        .await
        .unwrap();
    for chunk in contents.chunks(100_000) {
        stream.write(Bytes::copy_from_slice(chunk)).await.unwrap();
    }
    stream.finish().await.unwrap();
}

/// A session with a scratch folder (a bucket on S3) in its home folder.
async fn open_scratch(
    manager: &SessionManager,
    server: &Server,
    name: &str,
) -> (SessionInfo, Arc<dyn RemoteFileSystem>, String) {
    let session = connect_with(manager, profile(server, &server.password)).await;
    let files = files_of(manager, &session).await;
    let scratch = format!("poros-{name}-{}", std::process::id());
    let existing = remote_path::join(&session.home, &scratch);
    if files.stat(&existing).await.unwrap().is_some() {
        files.delete(std::slice::from_ref(&existing)).await.unwrap();
    }
    let remote = files.make_dir("~", &scratch).await.unwrap();
    (session, files, remote)
}

#[tokio::test]
async fn connects_and_refuses_a_wrong_password() {
    for server in webdav_servers() {
        let (_config, manager) = new_manager();
        let info = connect_with(&manager, profile(&server, &server.password)).await;
        assert_eq!(info.protocol, server.protocol);
        let files = files_of(&manager, &info).await;
        let listing = files.list_dir("~").await.unwrap();
        assert_eq!(listing.path, info.home);
        assert!(files.capabilities().ranged_reads);
        manager.disconnect(&info.id).await.unwrap();

        // The certificate is remembered by now, so only the password is wrong.
        let error = manager
            .connect(profile(&server, "wrong-password"), None, OWNER)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::AuthFailed, "{}", error.message);
    }
}

#[tokio::test]
async fn manages_files_and_folders() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let (session, files, root) = open_scratch(&manager, &server, "files").await;

        let folder = files.make_dir(&root, "a folder ✓").await.unwrap();
        assert_eq!(
            files.make_dir(&root, "a folder ✓").await.unwrap_err().kind,
            ErrorKind::AlreadyExists,
            "{label}"
        );
        let small = remote_path::join(&folder, "small & odd+name.txt");
        let contents = pattern(70_000, 1);
        write_remote(&files, &small, &contents, Some(1_600_000_000)).await;
        let stat = files.stat(&small).await.unwrap().unwrap();
        assert!(!stat.is_dir, "{label}");
        assert_eq!(stat.size, contents.len() as u64, "{label}");
        if server.protocol.uploads_carry_time() {
            assert_eq!(stat.modified, Some(1_600_000_000), "{label}");
        } else {
            // Best effort: many WebDAV servers keep their own time, and say so once.
            if let Err(error) = files
                .set_attributes(&small, Some(1_600_000_000), None)
                .await
            {
                assert_eq!(error.kind, ErrorKind::Unsupported, "{}", error.message);
                files
                    .set_attributes(&small, Some(1_600_000_000), None)
                    .await
                    .unwrap();
            }
        }
        assert_eq!(read_remote(&files, &small, 0..u64::MAX).await, contents);
        assert_eq!(
            read_remote(&files, &small, 1000..1100).await,
            contents[1000..1100],
            "{label}"
        );

        let created = files.clone().make_file(&folder, "new.txt").await.unwrap();
        assert_eq!(created, remote_path::join(&folder, "new.txt"));
        assert_eq!(files.stat(&created).await.unwrap().unwrap().size, 0);
        let taken = files
            .clone()
            .make_file(&folder, "small & odd+name.txt")
            .await
            .unwrap_err();
        assert_eq!(taken.kind, ErrorKind::AlreadyExists, "{label}");
        assert_eq!(read_remote(&files, &small, 0..u64::MAX).await, contents);
        let folder_taken = files
            .clone()
            .make_file(&root, "a folder ✓")
            .await
            .unwrap_err();
        assert_eq!(folder_taken.kind, ErrorKind::AlreadyExists, "{label}");

        // Large enough for S3 to take it in parts.
        let large = remote_path::join(&root, "large.bin");
        let large_contents = pattern(23 * MEBIBYTE + 17, 2);
        write_remote(&files, &large, &large_contents, None).await;
        let tail = (large_contents.len() - 5000) as u64..u64::MAX;
        assert_eq!(
            read_remote(&files, &large, tail).await,
            large_contents[large_contents.len() - 5000..],
            "{label}"
        );
        assert!(read_remote(&files, &large, 0..u64::MAX).await == large_contents);

        let empty = remote_path::join(&root, "empty.txt");
        write_remote(&files, &empty, b"", None).await;
        assert_eq!(files.stat(&empty).await.unwrap().unwrap().size, 0);
        assert!(read_remote(&files, &empty, 0..u64::MAX).await.is_empty());

        files
            .ensure_dir(&remote_path::join(&root, "x/y/z"))
            .await
            .unwrap();
        files
            .ensure_dir(&remote_path::join(&root, "x/y"))
            .await
            .unwrap();
        let listing = files.list_dir(&root).await.unwrap();
        let mut names: Vec<(String, EntryKind)> = listing
            .entries
            .iter()
            .map(|entry| (entry.name.clone(), entry.kind))
            .collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            names,
            vec![
                ("a folder ✓".to_string(), EntryKind::Dir),
                ("empty.txt".to_string(), EntryKind::File),
                ("large.bin".to_string(), EntryKind::File),
                ("x".to_string(), EntryKind::Dir),
            ],
            "{label}"
        );
        assert_eq!(
            files
                .list_dir(&remote_path::join(&root, "x/y"))
                .await
                .unwrap()
                .entries[0]
                .name,
            "z"
        );

        let renamed = files.rename(&small, "renamed.txt").await.unwrap();
        assert_eq!(renamed, remote_path::join(&folder, "renamed.txt"));
        assert!(files.stat(&small).await.unwrap().is_none(), "{label}");
        assert_eq!(read_remote(&files, &renamed, 0..u64::MAX).await, contents);
        let moved_folder = files.rename(&folder, "b folder").await.unwrap();
        assert_eq!(
            read_remote(
                &files,
                &remote_path::join(&moved_folder, "renamed.txt"),
                0..u64::MAX
            )
            .await,
            contents,
            "{label}"
        );
        assert!(files.stat(&folder).await.unwrap().is_none(), "{label}");
        assert_eq!(
            files.rename(&empty, "large.bin").await.unwrap_err().kind,
            ErrorKind::AlreadyExists,
            "{label}"
        );

        files
            .delete(&[moved_folder.clone(), large.clone()])
            .await
            .unwrap();
        assert!(files.stat(&moved_folder).await.unwrap().is_none());
        assert!(files.stat(&large).await.unwrap().is_none());
        assert_eq!(
            files
                .clone()
                .open_read(&large, 0..10)
                .await
                .err()
                .map(|error| error.kind),
            Some(ErrorKind::NotFound),
            "{label}"
        );

        files.delete(std::slice::from_ref(&root)).await.unwrap();
        assert!(files.stat(&root).await.unwrap().is_none(), "{label}");
        manager.disconnect(&session.id).await.unwrap();
    }
}

fn write_tree(root: &Path) {
    std::fs::create_dir_all(root.join("nested/deeper")).unwrap();
    std::fs::create_dir_all(root.join("empty folder")).unwrap();
    std::fs::write(root.join("a.txt"), pattern(1024, 1)).unwrap();
    std::fs::write(root.join("empty.bin"), b"").unwrap();
    std::fs::write(root.join("large.bin"), pattern(14 * MEBIBYTE, 2)).unwrap();
    std::fs::write(root.join("nested/b with spaces.txt"), pattern(70_000, 3)).unwrap();
    std::fs::write(root.join("nested/ünïcödé ✓.txt"), pattern(500, 11)).unwrap();
    std::fs::write(root.join("nested/deeper/c.bin"), pattern(3 * MEBIBYTE, 4)).unwrap();
}

type Tree = Vec<(String, Option<Vec<u8>>)>;

fn read_tree(root: &Path) -> Tree {
    let mut items = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                items.push((relative, None));
                pending.push(path);
            } else {
                items.push((relative, Some(std::fs::read(&path).unwrap())));
            }
        }
    }
    items.sort();
    items
}

async fn wait_until_settled(transfers: &TransferManager) -> TransferList {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let list = transfers.list();
        let counts = &list.stats.counts;
        if counts.queued == 0 && counts.running == 0 {
            return list;
        }
        assert!(
            Instant::now() < deadline,
            "transfers did not finish: {counts:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn assert_all_done(list: &TransferList) {
    let unfinished: Vec<_> = list
        .jobs
        .iter()
        .filter(|job| job.state != JobState::Done)
        .collect();
    assert!(unfinished.is_empty(), "{unfinished:#?}");
}

fn items_for(sources: &[PathBuf]) -> Vec<TransferItem> {
    sources
        .iter()
        .map(|source| TransferItem {
            path: source.to_string_lossy().into_owned(),
            name: source.file_name().unwrap().to_string_lossy().into_owned(),
            is_dir: source.is_dir(),
            size: source.metadata().unwrap().len(),
        })
        .collect()
}

#[tokio::test]
async fn uploads_and_downloads_a_tree() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let (session, files, root) = open_scratch(&manager, &server, "transfers").await;
        let local = tempfile::tempdir().unwrap();
        let source = local.path().join("tree");
        write_tree(&source);
        let transfers = TransferManager::new(
            manager.clone(),
            Events::default(),
            TransferSettings {
                workers: 4,
                segment_threshold_mib: 1,
                exists_action: ExistsAction::Overwrite,
                retry_attempts: 0,
                fxp: false,
                ..TransferSettings::default()
            },
        );
        transfers
            .enqueue(EnqueueRequest {
                session_id: session.id.clone(),
                source_session_id: None,
                direction: Direction::Upload,
                target_directory: root.clone(),
                items: items_for(std::slice::from_ref(&source)),
            })
            .await
            .unwrap();
        assert_all_done(&wait_until_settled(&transfers).await);

        let back = local.path().join("back");
        std::fs::create_dir(&back).unwrap();
        let tree = remote_path::join(&root, "tree");
        let entry = files
            .list_dir(&root)
            .await
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.name == "tree")
            .unwrap_or_else(|| panic!("{label}: the tree was not uploaded"));
        transfers
            .enqueue(EnqueueRequest {
                session_id: session.id.clone(),
                source_session_id: None,
                direction: Direction::Download,
                target_directory: back.to_string_lossy().into_owned(),
                items: vec![TransferItem {
                    path: tree,
                    name: entry.name,
                    is_dir: true,
                    size: 0,
                }],
            })
            .await
            .unwrap();
        assert_all_done(&wait_until_settled(&transfers).await);
        assert!(
            read_tree(&source) == read_tree(&back.join("tree")),
            "{label}: the downloaded tree differs"
        );

        files.delete(std::slice::from_ref(&root)).await.unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}

/// A bare HTTP/1.1 `GET` over a socket: nothing but the address and the `Host` header go
/// out, so what comes back is what anyone holding the address gets. Returns the status, the
/// headers in lower case, and the body.
async fn plain_get(address: &str) -> (u16, String, Vec<u8>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let rest = address
        .strip_prefix("http://")
        .expect("a plain HTTP address");
    let (authority, target) = rest.split_once('/').unwrap();
    let mut stream = tokio::net::TcpStream::connect(authority).await.unwrap();
    let request =
        format!("GET /{target} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let head_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("an HTTP response");
    let head = String::from_utf8_lossy(&response[..head_end]).to_ascii_lowercase();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head, response[head_end + 4..].to_vec())
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Accepted,
    SignatureDoesNotMatch,
    Expired,
}

/// What a service that checks signatures says to a presigned address at `now`, worked out
/// from the address alone as the S3 API reference describes, with other primitives than the
/// code under test. The local server takes any signature at any time, so this stands in
/// for the checks it does not make.
fn verdict(address: &str, secret_key: &str, now: i64) -> Verdict {
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};
    let hex = |bytes: &[u8]| -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() };
    let keyed = |key: &[u8], text: &str| -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
        mac.update(text.as_bytes());
        mac.finalize().into_bytes().to_vec()
    };
    let (location, query) = address.split_once('?').unwrap();
    let (authority, path) = location
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap();
    let mut parameters: Vec<(&str, &str)> = query
        .split('&')
        .map(|pair| pair.split_once('=').unwrap())
        .collect();
    let value_of = |name: &str| -> String {
        let (_, value) = parameters.iter().find(|(key, _)| *key == name).unwrap();
        percent_encoding::percent_decode_str(value)
            .decode_utf8()
            .unwrap()
            .into_owned()
    };
    let signature = value_of("X-Amz-Signature");
    let date_time = value_of("X-Amz-Date");
    let lifetime: i64 = value_of("X-Amz-Expires").parse().unwrap();
    let credential = value_of("X-Amz-Credential");
    let scope = credential.split_once('/').unwrap().1.to_string();
    let scope_parts: Vec<&str> = scope.split('/').collect();
    assert_eq!(value_of("X-Amz-Algorithm"), "AWS4-HMAC-SHA256");
    assert_eq!(value_of("X-Amz-SignedHeaders"), "host");
    parameters.retain(|(name, _)| *name != "X-Amz-Signature");
    parameters.sort();
    let canonical_query = parameters
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_request =
        format!("GET\n/{path}\n{canonical_query}\nhost:{authority}\n\nhost\nUNSIGNED-PAYLOAD");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{date_time}\n{scope}\n{}",
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let mut key = format!("AWS4{secret_key}").into_bytes();
    for part in &scope_parts {
        key = keyed(&key, part);
    }
    if hex(&keyed(&key, &string_to_sign)) != signature {
        return Verdict::SignatureDoesNotMatch;
    }
    let signed_at = poros_lib::timestamp::parse_rfc3339(&format!(
        "{}-{}-{}T{}:{}:{}Z",
        &date_time[0..4],
        &date_time[4..6],
        &date_time[6..8],
        &date_time[9..11],
        &date_time[11..13],
        &date_time[13..15]
    ))
    .unwrap();
    if now > signed_at + lifetime {
        return Verdict::Expired;
    }
    Verdict::Accepted
}

/// A file uploaded through Poros is read back through its share link by a client that
/// sends no credentials. What the local server (moto) enforces is checked against it: the
/// link opens the file, and the same address without the signed query is refused. It takes
/// any signature at any time, so a tampered signature and an expired link are judged by
/// `verdict` instead, here on this machine.
#[tokio::test]
async fn s3_share_links_open_a_file_without_credentials() {
    let Some(server) = server("POROS_TEST_S3_PORT", Protocol::S3) else {
        return;
    };
    let (_config, manager) = new_manager();
    let (session, files, bucket) = open_scratch(&manager, &server, "share-links").await;
    let events = Events::keeping_startup_log();

    let capabilities = session
        .sharing
        .clone()
        .expect("an S3 session can make links");
    assert_eq!(capabilities.kinds, [LinkKind::View, LinkKind::Download]);
    assert_eq!(capabilities.audiences, [Audience::Anyone]);
    assert_eq!(capabilities.expiry, Expiry::Required);
    assert_eq!(capabilities.max_lifetime_secs, Some(7 * 24 * 3600));
    assert!(!capabilities.revocable && !capabilities.folders);

    // A name with a space, a plus sign, characters that mean something in an address, and
    // letters beyond ASCII, inside a folder.
    let name = "c+d & \u{e9}t\u{e9} #1 (final)?.bin";
    let folder = remote_path::join(&bucket, "a b");
    let path = remote_path::join(&folder, name);
    let contents = pattern(300_000, 7);
    write_remote(&files, &path, &contents, None).await;

    let options = |kind: LinkKind, lifetime_secs: Option<u64>| ShareOptions {
        kind,
        audience: Audience::Anyone,
        lifetime_secs,
        password: None,
    };
    let before = poros_lib::timestamp::now_seconds();
    let link = share::create(
        files.as_ref(),
        &events,
        &session.id,
        &path,
        &options(LinkKind::View, Some(600)),
    )
    .await
    .unwrap();
    let after = poros_lib::timestamp::now_seconds();
    assert_eq!(link.kind, LinkKind::View);
    assert_eq!(link.audience, Audience::Anyone);
    assert!(!link.revocable && !link.has_password && link.id.is_empty());
    assert!(link.notes.is_empty(), "{:?}", link.notes);
    let expires_at = link.expires_at.unwrap();
    assert!((before + 600..=after + 600).contains(&expires_at));
    assert!(link.url.starts_with(&format!(
        "http://127.0.0.1:{}{}/a%20b/c%2Bd%20%26%20%C3%A9t%C3%A9%20%231%20%28final%29%3F.bin?",
        server.port, bucket
    )));
    assert!(link.url.contains("&X-Amz-Expires=600&"));

    // The server gives the bytes to someone with nothing but the address.
    let (status, _, body) = plain_get(&link.url).await;
    assert_eq!(status, 200);
    assert!(body == contents, "the link returned other bytes");
    // And refuses the same address once the signed query is gone.
    let (unsigned, _) = link.url.split_once('?').unwrap();
    let (status, _, _) = plain_get(unsigned).await;
    assert_eq!(status, 403);

    // The checks the local server does not make, made here.
    assert_eq!(
        verdict(&link.url, &server.password, after),
        Verdict::Accepted
    );
    assert_eq!(
        verdict(&link.url, &server.password, expires_at),
        Verdict::Accepted
    );
    assert_eq!(
        verdict(&link.url, &server.password, expires_at + 1),
        Verdict::Expired
    );
    let last = link.url.chars().last().unwrap();
    let flipped = if last == '0' { '1' } else { '0' };
    let tampered_signature = format!("{}{flipped}", &link.url[..link.url.len() - 1]);
    assert_eq!(
        verdict(&tampered_signature, &server.password, after),
        Verdict::SignatureDoesNotMatch
    );
    let longer_lived = link
        .url
        .replace("X-Amz-Expires=600", "X-Amz-Expires=604800");
    assert_eq!(
        verdict(&longer_lived, &server.password, after),
        Verdict::SignatureDoesNotMatch
    );
    let other_file = link.url.replace("c%2Bd", "c%2Be");
    assert_eq!(
        verdict(&other_file, &server.password, after),
        Verdict::SignatureDoesNotMatch
    );
    assert_eq!(
        verdict(&link.url, "another-secret-key", after),
        Verdict::SignatureDoesNotMatch
    );

    // A download link has the server name the file, and still verifies.
    let download = share::create(
        files.as_ref(),
        &events,
        &session.id,
        &path,
        &options(LinkKind::Download, None),
    )
    .await
    .unwrap();
    assert_eq!(download.kind, LinkKind::Download);
    assert!(download.url.contains("&X-Amz-Expires=3600&"));
    let (status, head, body) = plain_get(&download.url).await;
    assert_eq!(status, 200);
    assert!(body == contents);
    assert!(
        head.contains(
            "content-disposition: attachment; filename=\"c+d & _t_ #1 (final)?.bin\"; filename*=utf-8''c%2bd%20%26%20%c3%a9t%c3%a9%20%231%20%28final%29%3f.bin"
        ),
        "{head}"
    );
    assert_eq!(
        verdict(&download.url, &server.password, after),
        Verdict::Accepted
    );

    // A lifetime past the limit is cut to it, and the link says so.
    let clamped = share::create(
        files.as_ref(),
        &events,
        &session.id,
        &path,
        &options(LinkKind::View, Some(30 * 24 * 3600)),
    )
    .await
    .unwrap();
    assert!(clamped.url.contains("&X-Amz-Expires=604800&"));
    assert_eq!(
        clamped.notes,
        ["S3 links last at most 7 days, so this one ends then."]
    );
    let (status, _, _) = plain_get(&clamped.url).await;
    assert_eq!(status, 200);

    // What cannot be shared says why.
    let folder_error = share::create(
        files.as_ref(),
        &events,
        &session.id,
        &folder,
        &options(LinkKind::View, None),
    )
    .await
    .unwrap_err();
    assert_eq!(folder_error.kind, ErrorKind::Unsupported);
    assert!(folder_error.message.contains("single files"));
    let bucket_error = share::create(
        files.as_ref(),
        &events,
        &session.id,
        &bucket,
        &options(LinkKind::View, None),
    )
    .await
    .unwrap_err();
    assert_eq!(bucket_error.kind, ErrorKind::Unsupported);
    let missing = share::create(
        files.as_ref(),
        &events,
        &session.id,
        &remote_path::join(&bucket, "missing.bin"),
        &options(LinkKind::View, None),
    )
    .await
    .unwrap_err();
    assert_eq!(missing.kind, ErrorKind::NotFound);
    let edit = share::create(
        files.as_ref(),
        &events,
        &session.id,
        &path,
        &options(LinkKind::Edit, None),
    )
    .await
    .unwrap_err();
    assert_eq!(edit.kind, ErrorKind::InvalidInput);
    assert!(share::links(files.as_ref(), &path)
        .await
        .unwrap()
        .is_empty());
    let removal = share::remove(files.as_ref(), &events, &session.id, &path, "any")
        .await
        .unwrap_err();
    assert_eq!(removal.kind, ErrorKind::Unsupported);

    // The log says that links were made, and for what, but never holds one.
    let log = events.take_startup_log();
    assert_eq!(log.len(), 3, "{log:?}");
    for record in &log {
        assert_eq!(record.session_id.as_deref(), Some(session.id.as_str()));
        assert!(record.message.contains(&path), "{}", record.message);
        assert!(record.message.contains("valid until"), "{}", record.message);
        for secret in [
            "X-Amz",
            "Signature",
            "http://",
            "?",
            server.password.as_str(),
        ] {
            assert!(
                !record.message.replace(&path, "").contains(secret),
                "{}",
                record.message
            );
        }
    }

    // A vault on the same server has no links: the server holds its files encrypted.
    let vault = manager
        .create_vault(
            &session.id,
            &bucket,
            "Vault",
            "a vault's password".to_string(),
            OWNER,
        )
        .await
        .unwrap();
    assert!(vault.sharing.is_none());
    let vault_files = files_of(&manager, &vault).await;
    let secret_note = remote_path::join(&vault.home, "note.txt");
    write_remote(&vault_files, &secret_note, b"inside the vault", None).await;
    let refused = share::create(
        vault_files.as_ref(),
        &events,
        &vault.id,
        &secret_note,
        &options(LinkKind::View, None),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::Unsupported);
    assert_eq!(
        refused.message,
        "Files in a vault cannot be shared with a link: the server holds them encrypted"
    );

    files.delete(std::slice::from_ref(&bucket)).await.unwrap();
    manager.disconnect(&session.id).await.unwrap();
}
