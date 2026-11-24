//! Cryptomator vaults end to end: made, unlocked, filled and emptied through sessions and the
//! transfer queue on real servers of every kind. Each server is skipped unless configured, with
//! the variables the other end-to-end suites use:
//!
//! - `POROS_TEST_SSH_PORT`: SFTP.
//! - `POROS_TEST_FTP_PORT` and `POROS_TEST_FTPS_PORT`: plain FTP and explicit FTPS.
//! - `POROS_TEST_WEBDAV_PORT`: WebDAV over plain HTTP.
//! - `POROS_TEST_S3_PORT`: an S3-compatible service over plain HTTP that takes any keys.
//!
//! With `POROS_TEST_CRYPTOMATOR_JAR` naming the harness built from `tests/cryptomator-harness`,
//! a vault whose server keeps its files where this test can read them is also read there by
//! Cryptomator's own library. The SFTP account's home folder must then be readable to the user
//! running the tests.

use std::collections::BTreeMap;
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
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobState, TransferItem, TransferList, TransferManager,
};
use sha2::{Digest, Sha256};

const OWNER: &str = "main";
const MEBIBYTE: usize = 1024 * 1024;
const PASSWORD: &str = "a vault's p\u{e4}ssword";

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
            let approval = HostKeyApproval::of(&host_key, true);
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
    let scratch = format!("poros-vault-{name}-{}", std::process::id());
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

/// A name whose encrypted form is longer than a vault stores as a file name.
fn long_name(stem: &str) -> String {
    format!("{stem} {}.bin", "long ".repeat(36))
}

fn write_tree(root: &Path) {
    let long_folder = root.join(long_name("folder"));
    std::fs::create_dir_all(root.join("nested/deeper")).unwrap();
    std::fs::create_dir_all(root.join("empty folder")).unwrap();
    std::fs::create_dir_all(&long_folder).unwrap();
    std::fs::write(root.join("a.txt"), pattern(1024, 1)).unwrap();
    std::fs::write(root.join("empty.bin"), b"").unwrap();
    std::fs::write(root.join("chunk.bin"), pattern(32 * 1024, 5)).unwrap();
    std::fs::write(root.join("large.bin"), pattern(14 * MEBIBYTE + 77, 2)).unwrap();
    std::fs::write(root.join(long_name("file")), pattern(40_000, 6)).unwrap();
    std::fs::write(root.join("nested/b with spaces.txt"), pattern(70_000, 3)).unwrap();
    std::fs::write(
        root.join("nested/\u{fc}n\u{ef}c\u{f6}d\u{e9} \u{2713}.txt"),
        pattern(500, 11),
    )
    .unwrap();
    std::fs::write(root.join("nested/deeper/c.bin"), pattern(3 * MEBIBYTE, 4)).unwrap();
    std::fs::write(long_folder.join("inside.txt"), pattern(9, 7)).unwrap();
    for number in 0..40 {
        let small = root.join(format!("nested/small-{number:02}.txt"));
        std::fs::write(small, pattern(100 + number, number as u64)).unwrap();
    }
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
    let deadline = Instant::now() + Duration::from_secs(180);
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

fn assert_all_done(label: &str, list: &TransferList) {
    let unfinished: Vec<_> = list
        .jobs
        .iter()
        .filter(|job| job.state != JobState::Done)
        .collect();
    assert!(unfinished.is_empty(), "{label}: {unfinished:#?}");
}

fn new_transfers(manager: &Arc<SessionManager>, separate_connections: bool) -> TransferManager {
    TransferManager::new(
        manager.clone(),
        Events::default(),
        TransferSettings {
            workers: 4,
            segment_threshold_mib: 1,
            exists_action: ExistsAction::Overwrite,
            retry_attempts: 0,
            fxp: false,
            separate_connections,
            ..TransferSettings::default()
        },
    )
}

fn local_item(source: &Path) -> TransferItem {
    TransferItem {
        path: source.to_string_lossy().into_owned(),
        name: source.file_name().unwrap().to_string_lossy().into_owned(),
        is_dir: source.is_dir(),
        size: source.metadata().unwrap().len(),
    }
}

fn remote_folder(path: &str) -> TransferItem {
    TransferItem {
        path: path.to_string(),
        name: remote_path::file_name(path).to_string(),
        is_dir: true,
        size: 0,
    }
}

async fn transfer(
    label: &str,
    transfers: &TransferManager,
    direction: Direction,
    (session_id, source_session_id): (&str, Option<&str>),
    item: TransferItem,
    target_directory: &str,
) {
    transfers
        .enqueue(EnqueueRequest {
            session_id: session_id.to_string(),
            source_session_id: source_session_id.map(str::to_string),
            direction,
            target_directory: target_directory.to_string(),
            items: vec![item],
        })
        .await
        .unwrap_or_else(|error| panic!("{label}: {}", error.message));
    assert_all_done(label, &wait_until_settled(transfers).await);
}

async fn names_in(files: &Arc<dyn RemoteFileSystem>, folder: &str) -> Vec<String> {
    let mut names: Vec<String> = files
        .list_dir(folder)
        .await
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    names
}

/// Every name the server itself holds below `folder`.
async fn stored_names(files: &Arc<dyn RemoteFileSystem>, folder: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut pending = vec![folder.to_string()];
    while let Some(folder) = pending.pop() {
        for entry in files.list_dir(&folder).await.unwrap().entries {
            if entry.kind == EntryKind::Dir {
                pending.push(entry.path.clone());
            }
            names.push(entry.name);
        }
    }
    names
}

async fn read_remote(files: &Arc<dyn RemoteFileSystem>, path: &str) -> Vec<u8> {
    let mut stream = files.clone().open_read(path, 0..u64::MAX).await.unwrap();
    let mut contents = Vec::new();
    while let Some(chunk) = stream.next_chunk().await.unwrap() {
        contents.extend_from_slice(&chunk);
    }
    stream.finish().await.unwrap();
    contents
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

/// Where a server of this test setup keeps the folder at `path`, if this test can read it there.
fn on_disk(server: &Server, path: &str) -> Option<PathBuf> {
    if !matches!(
        server.protocol,
        Protocol::Sftp | Protocol::Ftp | Protocol::Ftps
    ) {
        return None;
    }
    let on_disk = PathBuf::from(path);
    let readable = std::fs::read_dir(&on_disk).is_ok();
    // An SFTP server's paths are this computer's own. Were its folders closed to this test, the
    // vaults in them would go unread by Cryptomator without a word.
    let expected = server.protocol == Protocol::Sftp
        && std::env::var_os("POROS_TEST_CRYPTOMATOR_JAR").is_some();
    assert!(
        readable || !expected,
        "{path} cannot be read: open the test account's home folder to this user"
    );
    readable.then_some(on_disk)
}

/// One entry of what the harness on Cryptomator's own library reads out of a vault.
#[derive(serde::Deserialize)]
struct Listed {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha256: String,
}

/// What Cryptomator reads out of the vault in `folder`: each folder, and each file's SHA-256.
fn read_by_cryptomator(folder: &Path) -> Option<BTreeMap<String, Option<String>>> {
    let jar = std::env::var_os("POROS_TEST_CRYPTOMATOR_JAR")?;
    let java = std::env::var_os("POROS_TEST_JAVA").unwrap_or_else(|| "java".into());
    let output = std::process::Command::new(java)
        .arg("-jar")
        .arg(jar)
        .args(["dump", folder.to_str().unwrap(), PASSWORD])
        // Java reads its arguments, the password among them, in the locale's character set.
        .env("LC_ALL", "C.UTF-8")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listed: Vec<Listed> = serde_json::from_slice(&output.stdout).unwrap();
    Some(
        listed
            .into_iter()
            .map(|listed| {
                let hash = (listed.kind == "file").then_some(listed.sha256);
                (listed.path, hash)
            })
            .collect(),
    )
}

fn hashed(tree: &Tree, below: &str) -> BTreeMap<String, Option<String>> {
    let hash = |contents: &Vec<u8>| -> String {
        Sha256::digest(contents)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    let mut hashed: BTreeMap<String, Option<String>> = tree
        .iter()
        .map(|(path, contents)| (format!("{below}/{path}"), contents.as_ref().map(hash)))
        .collect();
    hashed.insert(below.to_string(), None);
    hashed
}

#[tokio::test]
async fn a_vault_on_every_server_takes_a_tree_and_gives_it_back() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let (session, server_files, scratch) = open_scratch(&manager, &server, "tree").await;
        let local = tempfile::tempdir().unwrap();
        let source = local.path().join("tree");
        write_tree(&source);
        let tree = read_tree(&source);

        let vault = manager
            .create_vault(&session.id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        let root = remote_path::join(&scratch, "Vault");
        let described = vault
            .vault
            .clone()
            .expect("the session describes its vault");
        assert_eq!(vault.protocol, Protocol::Cryptomator, "{label}");
        assert_eq!(described.parent_session_id, session.id, "{label}");
        assert_eq!(
            (described.root.as_str(), described.name.as_str()),
            (&*root, "Vault")
        );
        assert_eq!(
            (vault.home.as_str(), vault.initial_path.as_str()),
            (&*root, &*root)
        );
        assert_ne!(vault.id, session.id);
        let vault_files = files_of(&manager, &vault.id).await;
        assert!(names_in(&vault_files, &root).await.is_empty(), "{label}");

        // Four workers at once, small files beside ones large enough to be split.
        let transfers = new_transfers(&manager, true);
        let upload = (vault.id.as_str(), None);
        transfer(
            label,
            &transfers,
            Direction::Upload,
            upload,
            local_item(&source),
            &root,
        )
        .await;

        // The server holds the vault's own files and nothing that gives a name away.
        let mut at_root = names_in(&server_files, &root).await;
        at_root.retain(|name| !name.ends_with(".bkup"));
        assert_eq!(
            at_root,
            ["d", "masterkey.cryptomator", "vault.cryptomator"],
            "{label}"
        );
        for name in stored_names(&server_files, &remote_path::join(&root, "d")).await {
            let encrypted = name.len() == 2
                || name.len() == 30
                || name.ends_with(".c9r")
                || name.ends_with(".c9s");
            assert!(encrypted, "{label}: {name} is stored in the vault");
        }
        if let Some(read) = on_disk(&server, &root).and_then(|folder| read_by_cryptomator(&folder))
        {
            assert_eq!(
                read,
                hashed(&tree, "/tree"),
                "{label}: as Cryptomator reads it"
            );
        }

        let back = local.path().join("back");
        std::fs::create_dir(&back).unwrap();
        let in_vault = remote_path::join(&root, "tree");
        let download = (vault.id.as_str(), None);
        let target = back.to_string_lossy().into_owned();
        transfer(
            label,
            &transfers,
            Direction::Download,
            download,
            remote_folder(&in_vault),
            &target,
        )
        .await;
        assert!(
            tree == read_tree(&back.join("tree")),
            "{label}: the downloaded tree differs"
        );

        // Out of the vault to its own server, where the copy is readable to anyone.
        let beside = server_files.make_dir(&scratch, "decrypted").await.unwrap();
        let out = (session.id.as_str(), Some(vault.id.as_str()));
        transfer(
            label,
            &transfers,
            Direction::Relay,
            out,
            remote_folder(&in_vault),
            &beside,
        )
        .await;
        let decrypted = remote_path::join(&beside, "tree/nested/b with spaces.txt");
        assert_eq!(
            read_remote(&server_files, &decrypted).await,
            pattern(70_000, 3),
            "{label}"
        );

        // And from the server into the vault again, under another name.
        vault_files.make_dir(&root, "copied in").await.unwrap();
        let copied_in = remote_path::join(&root, "copied in");
        let into = (vault.id.as_str(), Some(session.id.as_str()));
        let decrypted_tree = remote_path::join(&beside, "tree");
        transfer(
            label,
            &transfers,
            Direction::Relay,
            into,
            remote_folder(&decrypted_tree),
            &copied_in,
        )
        .await;
        let again = local.path().join("again");
        std::fs::create_dir(&again).unwrap();
        let target = again.to_string_lossy().into_owned();
        let copied_tree = remote_path::join(&copied_in, "tree");
        transfer(
            label,
            &transfers,
            Direction::Download,
            download,
            remote_folder(&copied_tree),
            &target,
        )
        .await;
        assert!(
            tree == read_tree(&again.join("tree")),
            "{label}: the copied tree differs"
        );

        // From one folder of the vault into another, decrypted and encrypted again on the way.
        let within = (vault.id.as_str(), Some(vault.id.as_str()));
        let nested = remote_path::join(&in_vault, "nested");
        transfer(
            label,
            &transfers,
            Direction::Relay,
            within,
            remote_folder(&nested),
            &copied_in,
        )
        .await;
        let copy = remote_path::join(&copied_in, "nested/deeper/c.bin");
        assert!(
            read_remote(&vault_files, &copy).await == pattern(3 * MEBIBYTE, 4),
            "{label}: the copy within the vault differs"
        );

        // Locked, the vault's session is gone, and its files with it.
        manager.disconnect(&vault.id).await.unwrap();
        let gone = manager.get(&vault.id).await.err().unwrap();
        assert_eq!(gone.kind, ErrorKind::SessionNotFound, "{label}");
        let locked = vault_files.list_dir(&root).await.err().unwrap();
        assert_eq!(locked.kind, ErrorKind::SessionNotFound, "{label}");
        assert!(
            manager.get(&session.id).await.is_ok(),
            "{label}: the server stays connected"
        );

        let wrong = manager
            .unlock_vault(&session.id, &root, "not the password".to_string(), OWNER)
            .await
            .err()
            .unwrap();
        assert_eq!(wrong.kind, ErrorKind::WrongPassword, "{label}");
        let not_a_vault = manager
            .unlock_vault(&session.id, &scratch, PASSWORD.to_string(), OWNER)
            .await
            .err()
            .unwrap();
        assert_eq!(not_a_vault.kind, ErrorKind::NotFound, "{label}");

        let unlocked = manager
            .unlock_vault(&session.id, &root, PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_ne!(unlocked.id, vault.id);
        let unlocked_files = files_of(&manager, &unlocked.id).await;
        assert_eq!(
            names_in(&unlocked_files, &root).await,
            ["copied in", "tree"],
            "{label}"
        );
        let large = remote_path::join(&in_vault, "large.bin");
        assert_eq!(
            unlocked_files.stat(&large).await.unwrap().unwrap().size,
            (14 * MEBIBYTE + 77) as u64,
            "{label}"
        );

        server_files
            .delete(std::slice::from_ref(&scratch))
            .await
            .unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}

#[tokio::test]
async fn a_vault_keeps_files_whatever_is_done_to_their_names() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let (session, server_files, scratch) = open_scratch(&manager, &server, "names").await;
        let vault = manager
            .create_vault(&session.id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        let root = remote_path::join(&scratch, "Vault");
        let files = files_of(&manager, &vault.id).await;

        // Larger than the piece an FTP connection, which cannot read and write at once, copies
        // at a time.
        let contents = pattern(5 * MEBIBYTE + 3, 8);
        let short = remote_path::join(&root, "short.bin");
        write_remote(&files, &short, &contents).await;
        let long = files.rename(&short, &long_name("renamed")).await.unwrap();
        assert_eq!(files.stat(&short).await.unwrap(), None, "{label}");
        assert_eq!(
            read_remote(&files, &long).await,
            contents,
            "{label}: short to long"
        );
        let longer = files
            .rename(&long, &long_name("renamed again"))
            .await
            .unwrap();
        assert_eq!(
            read_remote(&files, &longer).await,
            contents,
            "{label}: long to long"
        );
        let short_again = files.rename(&longer, "short again.bin").await.unwrap();
        assert_eq!(
            read_remote(&files, &short_again).await,
            contents,
            "{label}: long to short"
        );
        assert_eq!(
            names_in(&files, &root).await,
            ["short again.bin"],
            "{label}"
        );

        files
            .ensure_dir(&remote_path::join(&root, "folder/inner"))
            .await
            .unwrap();
        let inner = remote_path::join(&root, "folder/inner/file.txt");
        write_remote(&files, &inner, b"inner").await;
        let renamed = files
            .rename(&remote_path::join(&root, "folder"), &long_name("folder"))
            .await
            .unwrap();
        let moved = remote_path::join(&renamed, "inner/file.txt");
        assert_eq!(read_remote(&files, &moved).await, b"inner", "{label}");
        let taken = files
            .rename(&renamed, "short again.bin")
            .await
            .err()
            .unwrap();
        assert_eq!(taken.kind, ErrorKind::AlreadyExists, "{label}");

        let stored_before = stored_names(&server_files, &root).await.len();
        files.delete(&[renamed, short_again]).await.unwrap();
        assert!(names_in(&files, &root).await.is_empty(), "{label}");
        let stored_after = stored_names(&server_files, &root).await.len();
        assert!(
            stored_after < stored_before,
            "{label}: nothing was removed from the server"
        );
        if let Some(read) = on_disk(&server, &root).and_then(|folder| read_by_cryptomator(&folder))
        {
            assert!(read.is_empty(), "{label}: Cryptomator still reads {read:?}");
        }

        server_files
            .delete(std::slice::from_ref(&scratch))
            .await
            .unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}

#[tokio::test]
async fn a_vault_stays_open_over_a_reconnect_and_closes_with_its_server() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let (session, _server_files, scratch) = open_scratch(&manager, &server, "sessions").await;
        let vault = manager
            .create_vault(&session.id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        let root = remote_path::join(&scratch, "Vault");
        let files = files_of(&manager, &vault.id).await;
        write_remote(&files, &remote_path::join(&root, "kept.txt"), b"kept").await;

        // The vault is open once: unlocking it again answers with the same session.
        let same = manager
            .unlock_vault(&session.id, &root, PASSWORD.to_string(), OWNER)
            .await
            .unwrap();
        assert_eq!(same.id, vault.id, "{label}");
        let nested = manager
            .unlock_vault(&vault.id, &root, PASSWORD.to_string(), OWNER)
            .await
            .err()
            .unwrap();
        assert_eq!(nested.kind, ErrorKind::InvalidInput, "{label}");
        let not_a_server = manager
            .reconnect(&vault.id, None, OWNER)
            .await
            .err()
            .unwrap();
        assert_eq!(not_a_server.kind, ErrorKind::InvalidInput, "{label}");
        // A transfer looking for the server's session is never handed the vault's.
        let live = manager.find_live(&profile(&server), None).await.unwrap();
        assert_eq!(live.id, session.id, "{label}");

        // A reconnect gives the server a new session, and the vault moves to it with its keys.
        let reconnected = manager.reconnect(&session.id, None, OWNER).await.unwrap();
        assert_ne!(reconnected.id, session.id);
        manager.disconnect(&session.id).await.unwrap();
        let moved = manager.adopt(&vault.id, OWNER).await.unwrap();
        assert_eq!(moved.id, vault.id, "{label}");
        assert_eq!(
            moved.vault.unwrap().parent_session_id,
            reconnected.id,
            "{label}"
        );
        let files = files_of(&manager, &vault.id).await;
        assert_eq!(names_in(&files, &root).await, ["kept.txt"], "{label}");
        assert_eq!(
            read_remote(&files, &remote_path::join(&root, "kept.txt")).await,
            b"kept"
        );

        // Transfers queued for the vault run on after the reconnect, and fail once it is locked.
        let local = tempfile::tempdir().unwrap();
        let source = local.path().join("after.bin");
        std::fs::write(&source, pattern(200_000, 9)).unwrap();
        let transfers = new_transfers(&manager, true);
        let upload = (vault.id.as_str(), None);
        transfer(
            label,
            &transfers,
            Direction::Upload,
            upload,
            local_item(&source),
            &root,
        )
        .await;
        assert_eq!(
            names_in(&files, &root).await,
            ["after.bin", "kept.txt"],
            "{label}"
        );

        // Closing the server's session locks the vaults on it.
        let server_files = files_of(&manager, &reconnected.id).await;
        server_files
            .delete(std::slice::from_ref(&scratch))
            .await
            .unwrap();
        manager.disconnect(&reconnected.id).await.unwrap();
        let gone = manager.get(&vault.id).await.err().unwrap();
        assert_eq!(gone.kind, ErrorKind::SessionNotFound, "{label}");
        let locked = files.list_dir(&root).await.err().unwrap();
        assert_eq!(locked.kind, ErrorKind::SessionNotFound, "{label}");

        std::fs::write(&source, pattern(1000, 10)).unwrap();
        let refused = transfers
            .enqueue(EnqueueRequest {
                session_id: vault.id.clone(),
                source_session_id: None,
                direction: Direction::Upload,
                target_directory: root.clone(),
                items: vec![local_item(&source)],
            })
            .await
            .err()
            .unwrap();
        assert_eq!(refused.kind, ErrorKind::SessionNotFound, "{label}");
    }
}

#[tokio::test]
async fn transfers_for_a_locked_vault_fail_until_it_is_unlocked_again() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let (session, server_files, scratch) = open_scratch(&manager, &server, "locked").await;
        let vault = manager
            .create_vault(&session.id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        let root = remote_path::join(&scratch, "Vault");
        let local = tempfile::tempdir().unwrap();
        let early = local.path().join("early.bin");
        let late = local.path().join("late.bin");
        std::fs::write(&early, pattern(20_000, 13)).unwrap();
        std::fs::write(&late, pattern(50_000, 12)).unwrap();

        // One worker, which keeps the connection its first transfer opened into the vault.
        let transfers = TransferManager::new(
            manager.clone(),
            Events::default(),
            TransferSettings {
                workers: 1,
                retry_attempts: 0,
                ..TransferSettings::default()
            },
        );
        let upload = (vault.id.as_str(), None);
        transfer(
            label,
            &transfers,
            Direction::Upload,
            upload,
            local_item(&early),
            &root,
        )
        .await;

        // Queued while the vault is open, and run once it is locked.
        transfers.set_paused(true);
        transfers
            .enqueue(EnqueueRequest {
                session_id: vault.id.clone(),
                source_session_id: None,
                direction: Direction::Upload,
                target_directory: root.clone(),
                items: vec![local_item(&late)],
            })
            .await
            .unwrap();
        manager.disconnect(&vault.id).await.unwrap();
        transfers.set_paused(false);
        let list = wait_until_settled(&transfers).await;
        let failed = list
            .jobs
            .iter()
            .find(|job| job.name == "late.bin")
            .expect("the queued transfer is listed");
        assert_eq!(failed.state, JobState::Failed, "{label}");
        let error = failed.error.clone().unwrap_or_default();
        assert!(error.contains("locked"), "{label}: {error}");

        // Nothing reached the server around the vault's encryption.
        let mut at_root = names_in(&server_files, &root).await;
        at_root.retain(|name| !name.ends_with(".bkup"));
        assert_eq!(
            at_root,
            ["d", "masterkey.cryptomator", "vault.cryptomator"],
            "{label}"
        );

        // Unlocked again, the vault has a new session, and the transfer goes into it.
        let unlocked = manager
            .unlock_vault(&session.id, &root, PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_ne!(unlocked.id, vault.id);
        transfers.resume(&[failed.id]);
        assert_all_done(label, &wait_until_settled(&transfers).await);
        let files = files_of(&manager, &unlocked.id).await;
        assert_eq!(
            names_in(&files, &root).await,
            ["early.bin", "late.bin"],
            "{label}"
        );
        let stored = read_remote(&files, &remote_path::join(&root, "late.bin")).await;
        assert_eq!(stored, pattern(50_000, 12), "{label}");

        server_files
            .delete(std::slice::from_ref(&scratch))
            .await
            .unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}

#[tokio::test]
async fn transfers_share_the_browsing_connection_when_told_to() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let (session, server_files, scratch) = open_scratch(&manager, &server, "shared").await;
        let vault = manager
            .create_vault(&session.id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        let root = remote_path::join(&scratch, "Vault");
        let local = tempfile::tempdir().unwrap();
        let source = local.path().join("tree");
        write_tree(&source);

        // No connections of their own: SFTP workers open channels on the server's connection,
        // FTP workers take turns on it.
        let transfers = new_transfers(&manager, false);
        let upload = (vault.id.as_str(), None);
        transfer(
            label,
            &transfers,
            Direction::Upload,
            upload,
            local_item(&source),
            &root,
        )
        .await;
        let back = local.path().join("back");
        std::fs::create_dir(&back).unwrap();
        let target = back.to_string_lossy().into_owned();
        let in_vault = remote_path::join(&root, "tree");
        transfer(
            label,
            &transfers,
            Direction::Download,
            upload,
            remote_folder(&in_vault),
            &target,
        )
        .await;
        assert!(
            read_tree(&source) == read_tree(&back.join("tree")),
            "{label}"
        );

        // Within the vault. One FTP connection cannot read and write at once, so on FTP the copy
        // is refused before anything is written.
        let vault_files = files_of(&manager, &vault.id).await;
        let copies = vault_files.make_dir(&root, "copies").await.unwrap();
        transfers
            .enqueue(EnqueueRequest {
                session_id: vault.id.clone(),
                source_session_id: Some(vault.id.clone()),
                direction: Direction::Relay,
                target_directory: copies.clone(),
                items: vec![remote_folder(&remote_path::join(&in_vault, "nested"))],
            })
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        let list = wait_until_settled(&transfers).await;
        if server.protocol.is_ftp() {
            let refused: Vec<_> = list
                .jobs
                .iter()
                .filter(|job| job.direction == Direction::Relay)
                .collect();
            assert!(!refused.is_empty(), "{label}");
            for job in refused {
                assert_eq!(job.state, JobState::Failed, "{label}: {job:#?}");
                let error = job.error.as_deref().unwrap_or_default();
                assert!(
                    error.contains("needs a second connection"),
                    "{label}: {error}"
                );
            }
            assert!(names_in(&vault_files, &copies).await.is_empty(), "{label}");
        } else {
            assert_all_done(label, &list);
            let copy = remote_path::join(&copies, "nested/deeper/c.bin");
            assert!(
                read_remote(&vault_files, &copy).await == pattern(3 * MEBIBYTE, 4),
                "{label}: the copy within the vault differs"
            );
        }

        let mut at_root = names_in(&server_files, &root).await;
        at_root.retain(|name| !name.ends_with(".bkup"));
        assert_eq!(
            at_root,
            ["d", "masterkey.cryptomator", "vault.cryptomator"],
            "{label}"
        );
        server_files
            .delete(std::slice::from_ref(&scratch))
            .await
            .unwrap();
        manager.disconnect(&session.id).await.unwrap();
    }
}
