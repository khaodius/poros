//! End-to-end tests against real FTP servers. Each test is skipped unless the servers it needs
//! are configured:
//!
//! - `POROS_TEST_FTP_PORT`: plain FTP.
//! - `POROS_TEST_FTP_SECOND_PORT`: a second plain FTP server, which accepts files sent straight
//!   from the first one (FXP).
//! - `POROS_TEST_FTPS_PORT`: explicit FTPS (`AUTH TLS`) with a self-signed certificate.
//! - `POROS_TEST_FTPS_IMPLICIT_PORT`: implicit FTPS with a self-signed certificate.
//! - `POROS_TEST_SSH_PORT`: the SFTP test server, for copies between FTP and SFTP.
//!
//! The servers listen on 127.0.0.1 and accept `POROS_TEST_FTP_USER` with
//! `POROS_TEST_FTP_PASSWORD`. The CI workflow shows a throwaway vsftpd setup.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::model::{EntryKind, LinkTarget};
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval};
use poros_lib::tls::CERTIFICATE_ALGORITHM;
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobSnapshot, JobState, TransferItem, TransferList,
    TransferManager,
};

/// The window label sessions belong to.
const OWNER: &str = "main";
const MEBIBYTE: usize = 1024 * 1024;

struct Server {
    protocol: Protocol,
    port: u16,
    user: String,
    password: String,
}

fn server(port_variable: &str, protocol: Protocol) -> Option<Server> {
    let port = std::env::var(port_variable).ok()?.parse().ok()?;
    let (user_variable, password_variable) = if protocol == Protocol::Sftp {
        ("POROS_TEST_SSH_USER", "POROS_TEST_SSH_PASSWORD")
    } else {
        ("POROS_TEST_FTP_USER", "POROS_TEST_FTP_PASSWORD")
    };
    Some(Server {
        protocol,
        port,
        user: std::env::var(user_variable).unwrap_or_else(|_| "poros".into()),
        password: std::env::var(password_variable).unwrap_or_else(|_| "poros-pass".into()),
    })
}

fn plain_server() -> Option<Server> {
    server("POROS_TEST_FTP_PORT", Protocol::Ftp)
}

fn second_server() -> Option<Server> {
    server("POROS_TEST_FTP_SECOND_PORT", Protocol::Ftp)
}

fn explicit_server() -> Option<Server> {
    server("POROS_TEST_FTPS_PORT", Protocol::Ftps)
}

fn implicit_server() -> Option<Server> {
    server("POROS_TEST_FTPS_IMPLICIT_PORT", Protocol::FtpsImplicit)
}

fn ssh_server() -> Option<Server> {
    server("POROS_TEST_SSH_PORT", Protocol::Sftp)
}

/// Every configured FTP server, plain and encrypted.
fn every_ftp_server() -> Vec<Server> {
    [plain_server(), explicit_server(), implicit_server()]
        .into_iter()
        .flatten()
        .collect()
}

fn tls_servers() -> Vec<Server> {
    [explicit_server(), implicit_server()]
        .into_iter()
        .flatten()
        .collect()
}

fn profile(server: &Server, password: &str) -> ConnectProfile {
    ConnectProfile {
        protocol: server.protocol,
        host: "127.0.0.1".into(),
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
    }
}

/// A session manager with its own known_hosts and trusted certificates files.
fn new_manager() -> (tempfile::TempDir, Arc<SessionManager>) {
    let config = tempfile::tempdir().unwrap();
    let manager = SessionManager::new(config.path().join("known_hosts"), Events::default());
    (config, Arc::new(manager))
}

/// Connects, approving the server's certificate or SSH host key if the manager asks.
async fn connect(manager: &SessionManager, server: &Server) -> SessionInfo {
    connect_with(manager, profile(server, &server.password)).await
}

async fn connect_with(manager: &SessionManager, profile: ConnectProfile) -> SessionInfo {
    let label = profile.label();
    match manager.connect(profile.clone(), None, OWNER).await {
        Ok(info) => info,
        Err(error) => {
            let host_key = error
                .host_key
                .unwrap_or_else(|| panic!("{label}: {}", error.message));
            manager
                .connect(
                    profile,
                    Some(HostKeyApproval {
                        fingerprint: host_key.fingerprint,
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

fn write_tree(root: &Path, large_size: usize) {
    std::fs::create_dir_all(root.join("nested/deeper")).unwrap();
    std::fs::create_dir_all(root.join("empty folder")).unwrap();
    std::fs::write(root.join("a.txt"), pattern(1024, 1)).unwrap();
    std::fs::write(root.join("empty.bin"), b"").unwrap();
    std::fs::write(root.join("large.bin"), pattern(large_size, 2)).unwrap();
    std::fs::write(root.join("nested/b with spaces.txt"), pattern(70_000, 3)).unwrap();
    std::fs::write(root.join("nested/ünïcödé ✓.txt"), pattern(500, 11)).unwrap();
    std::fs::write(root.join("nested/deeper/c.bin"), pattern(3 * MEBIBYTE, 4)).unwrap();
}

type Tree = Vec<(String, Option<Vec<u8>>)>;

/// Every file and folder under `root`, relative to it, with file contents.
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

/// The same as `read_tree`, for a folder on a server.
async fn read_remote_tree(files: &Arc<dyn RemoteFileSystem>, root: &str) -> Tree {
    let mut items = Vec::new();
    let mut pending = vec![root.to_string()];
    while let Some(folder) = pending.pop() {
        for entry in files.list_dir(&folder).await.unwrap().entries {
            let relative = entry.path[root.len() + 1..].to_string();
            if entry.kind == EntryKind::Dir {
                items.push((relative, None));
                pending.push(entry.path);
            } else {
                items.push((relative, Some(read_remote(files, &entry.path).await)));
            }
        }
    }
    items.sort();
    items
}

async fn read_remote(files: &Arc<dyn RemoteFileSystem>, path: &str) -> Vec<u8> {
    let size = files
        .stat(path)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("{path} is not on the server"))
        .size;
    let mut stream = files.clone().open_read(path, 0..size).await.unwrap();
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

fn transfer_settings() -> TransferSettings {
    TransferSettings {
        workers: 4,
        segment_threshold_mib: 1,
        exists_action: ExistsAction::Overwrite,
        retry_attempts: 0,
        fxp: false,
        ..TransferSettings::default()
    }
}

/// Waits until nothing is queued or running, tracking the most workers any job had at once.
async fn wait_until_settled(transfers: &TransferManager) -> (TransferList, u32) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut most_connections = 0;
    loop {
        let list = transfers.list();
        most_connections = list
            .jobs
            .iter()
            .map(|job| job.connections)
            .fold(most_connections, u32::max);
        let counts = &list.stats.counts;
        if counts.queued == 0 && counts.running == 0 {
            return (list, most_connections);
        }
        assert!(
            Instant::now() < deadline,
            "transfers did not finish: {counts:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn assert_all_done(list: &TransferList) {
    let unfinished: Vec<&JobSnapshot> = list
        .jobs
        .iter()
        .filter(|job| job.state != JobState::Done)
        .collect();
    assert!(unfinished.is_empty(), "{unfinished:#?}");
}

/// Waits until the only job has moved more than `bytes`, and returns its id.
async fn wait_for_progress(transfers: &TransferManager, bytes: u64) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(job) = transfers.list().jobs.first() {
            if job.transferred > bytes {
                return job.id;
            }
            assert!(
                job.state == JobState::Running || job.state == JobState::Queued,
                "{job:#?}"
            );
        }
        assert!(Instant::now() < deadline, "the transfer did not start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A server session with a scratch folder that is removed at the end.
struct Connected {
    session: SessionInfo,
    files: Arc<dyn RemoteFileSystem>,
    remote: String,
}

struct Fixture {
    _config: tempfile::TempDir,
    local: tempfile::TempDir,
    manager: Arc<SessionManager>,
    connected: Vec<Connected>,
}

impl Fixture {
    fn new() -> Self {
        let (config, manager) = new_manager();
        Self {
            _config: config,
            local: tempfile::tempdir().unwrap(),
            manager,
            connected: Vec::new(),
        }
    }

    /// Connects to `server` and makes a fresh scratch folder in its home folder.
    async fn open(&mut self, server: &Server, name: &str) -> usize {
        self.open_with(profile(server, &server.password), name)
            .await
    }

    async fn open_with(&mut self, profile: ConnectProfile, name: &str) -> usize {
        let session = connect_with(&self.manager, profile).await;
        let files = files_of(&self.manager, &session).await;
        let scratch = format!("poros-{name}-{}", std::process::id());
        let existing = remote_path::join(&session.home, &scratch);
        if files.stat(&existing).await.unwrap().is_some() {
            files.delete(std::slice::from_ref(&existing)).await.unwrap();
        }
        let remote = files.make_dir("~", &scratch).await.unwrap();
        self.connected.push(Connected {
            session,
            files,
            remote,
        });
        self.connected.len() - 1
    }

    fn at(&self, index: usize) -> &Connected {
        &self.connected[index]
    }

    fn transfers(&self, settings: TransferSettings) -> TransferManager {
        TransferManager::new(self.manager.clone(), Events::default(), settings)
    }

    async fn upload(
        &self,
        transfers: &TransferManager,
        index: usize,
        sources: &[PathBuf],
        target: &str,
    ) {
        let items = sources
            .iter()
            .map(|source| TransferItem {
                path: source.to_string_lossy().into_owned(),
                name: source.file_name().unwrap().to_string_lossy().into_owned(),
                is_dir: source.is_dir(),
                size: source.metadata().unwrap().len(),
            })
            .collect();
        transfers
            .enqueue(EnqueueRequest {
                session_id: self.at(index).session.id.clone(),
                source_session_id: None,
                direction: Direction::Upload,
                target_directory: target.to_string(),
                items,
            })
            .await
            .unwrap();
    }

    async fn download(
        &self,
        transfers: &TransferManager,
        index: usize,
        sources: &[&str],
        target: &Path,
    ) {
        let items = self.items(index, sources).await;
        transfers
            .enqueue(EnqueueRequest {
                session_id: self.at(index).session.id.clone(),
                source_session_id: None,
                direction: Direction::Download,
                target_directory: target.to_string_lossy().into_owned(),
                items,
            })
            .await
            .unwrap();
    }

    /// Copies from one server to another, or within one when both indexes are the same.
    async fn relay(
        &self,
        transfers: &TransferManager,
        source: usize,
        sources: &[&str],
        target: usize,
        target_directory: &str,
    ) {
        let items = self.items(source, sources).await;
        transfers
            .enqueue(EnqueueRequest {
                session_id: self.at(target).session.id.clone(),
                source_session_id: Some(self.at(source).session.id.clone()),
                direction: Direction::Relay,
                target_directory: target_directory.to_string(),
                items,
            })
            .await
            .unwrap();
    }

    /// Transfer items for paths on a server, as its listing shows them.
    async fn items(&self, index: usize, paths: &[&str]) -> Vec<TransferItem> {
        let files = &self.at(index).files;
        let mut items = Vec::new();
        for path in paths {
            let parent = remote_path::parent(path).unwrap();
            let entry = files
                .list_dir(&parent)
                .await
                .unwrap()
                .entries
                .into_iter()
                .find(|entry| entry.path == *path)
                .unwrap_or_else(|| panic!("{path} is not on the server"));
            items.push(TransferItem {
                path: entry.path,
                name: entry.name,
                is_dir: entry.kind == EntryKind::Dir,
                size: entry.size,
            });
        }
        items
    }

    async fn close(self) {
        for connected in &self.connected {
            connected
                .files
                .delete(std::slice::from_ref(&connected.remote))
                .await
                .unwrap();
            self.manager
                .disconnect(&connected.session.id)
                .await
                .unwrap();
        }
    }
}

#[tokio::test]
async fn connects_lists_and_refuses_a_wrong_password() {
    for server in every_ftp_server() {
        let (_config, manager) = new_manager();
        let info = connect(&manager, &server).await;
        assert_eq!(info.protocol, server.protocol);
        assert!(info.home.starts_with('/'), "{}", info.home);
        assert_eq!(info.initial_path, info.home);

        let session = manager.get(&info.id).await.unwrap();
        assert_eq!(
            session.sftp().err().map(|error| error.kind),
            Some(ErrorKind::Unsupported)
        );
        let files = session.files();
        assert_eq!(files.protocol(), server.protocol);
        let listing = files.list_dir("~").await.unwrap();
        assert_eq!(listing.path, info.home);
        assert_eq!(
            listing.parent,
            remote_path::parent(&info.home),
            "{listing:?}"
        );
        assert!(listing
            .entries
            .iter()
            .all(|entry| entry.name != "." && entry.name != ".."));
        assert!(files.capabilities().ranged_reads);
        assert!(files.capabilities().resumable_writes);
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
async fn starts_in_the_initial_folder() {
    let Some(server) = plain_server() else { return };
    let (_config, manager) = new_manager();
    let info = connect_with(
        &manager,
        ConnectProfile {
            initial_path: Some("/tmp/../tmp".into()),
            ..profile(&server, &server.password)
        },
    )
    .await;
    assert_eq!(info.initial_path, "/tmp");
    manager.disconnect(&info.id).await.unwrap();
}

#[tokio::test]
async fn manages_files_and_folders() {
    for server in every_ftp_server() {
        let mut fixture = Fixture::new();
        let index = fixture.open(&server, "manage").await;
        let Connected {
            session,
            files,
            remote: root,
        } = fixture.at(index);

        let sub = files.make_dir(root, "sub").await.unwrap();
        assert_eq!(sub, remote_path::join(root, "sub"));
        let deeper = files.make_dir(&sub, "deeper").await.unwrap();
        let duplicate = files.make_dir(root, "sub").await.unwrap_err();
        assert_eq!(duplicate.kind, ErrorKind::AlreadyExists);
        let invalid = files.make_dir(root, "a/b").await.unwrap_err();
        assert_eq!(invalid.kind, ErrorKind::InvalidInput);

        let file = remote_path::join(root, "file name.txt");
        let contents = pattern(250_000, 7);
        write_remote(files, &file, &contents).await;
        write_remote(files, &remote_path::join(&deeper, "inner.bin"), b"inner").await;
        write_remote(files, &remote_path::join(&sub, "empty.bin"), b"").await;

        let listing = files.list_dir(root).await.unwrap();
        assert_eq!(listing.path, *root);
        assert_eq!(listing.parent.as_deref(), Some(session.home.as_str()));
        let mut entries: Vec<(String, EntryKind, u64)> = listing
            .entries
            .iter()
            .map(|entry| (entry.name.clone(), entry.kind, entry.size))
            .collect();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            entries,
            [
                ("file name.txt".to_string(), EntryKind::File, 250_000),
                ("sub".to_string(), EntryKind::Dir, 0),
            ]
        );
        let listed = listing
            .entries
            .iter()
            .find(|entry| entry.kind == EntryKind::File)
            .unwrap();
        assert_eq!(listed.path, file);
        assert!(listed.modified.is_some());
        assert!(listed.permissions.is_some());

        let stat = files.stat(&file).await.unwrap().unwrap();
        assert!(!stat.is_dir);
        assert_eq!(stat.size, 250_000);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let modified = stat.modified.expect("the server reports file times");
        assert!((modified - now).abs() < 300, "{modified} vs {now}");
        assert!(files.stat(&sub).await.unwrap().unwrap().is_dir);
        let missing = remote_path::join(root, "missing");
        assert_eq!(files.stat(&missing).await.unwrap(), None);
        assert_eq!(read_remote(files, &file).await, contents);

        // A range reads part of the file and leaves the connection usable.
        let mut middle = files.clone().open_read(&file, 1000..2000).await.unwrap();
        let mut part = Vec::new();
        while let Some(chunk) = middle.next_chunk().await.unwrap() {
            part.extend_from_slice(&chunk);
        }
        middle.finish().await.unwrap();
        assert_eq!(part, contents[1000..2000]);
        assert_eq!(files.stat(&file).await.unwrap().unwrap().size, 250_000);

        // Listing a folder moves the session into it, which must not upset renaming or
        // removing that folder.
        files.list_dir(&deeper).await.unwrap();
        let renamed = files.rename(&sub, "renamed").await.unwrap();
        assert_eq!(renamed, remote_path::join(root, "renamed"));
        assert_eq!(files.stat(&sub).await.unwrap(), None);
        let taken = files.rename(&file, "renamed").await.unwrap_err();
        assert_eq!(taken.kind, ErrorKind::AlreadyExists);
        let renamed_file = files.rename(&file, "moved.txt").await.unwrap();
        assert_eq!(read_remote(files, &renamed_file).await, contents);

        files
            .delete(std::slice::from_ref(&renamed_file))
            .await
            .unwrap();
        assert_eq!(files.stat(&renamed_file).await.unwrap(), None);
        let gone = files
            .delete(std::slice::from_ref(&renamed_file))
            .await
            .unwrap_err();
        assert_eq!(gone.kind, ErrorKind::NotFound);

        // A folder goes with everything in it, even when listed with its contents.
        let inner = remote_path::join(&renamed, "deeper/inner.bin");
        files
            .list_dir(&remote_path::join(&renamed, "deeper"))
            .await
            .unwrap();
        files.delete(&[inner, renamed.clone()]).await.unwrap();
        assert_eq!(files.stat(&renamed).await.unwrap(), None);
        assert!(files.list_dir(root).await.unwrap().entries.is_empty());
        let error = files.list_dir(&renamed).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::NotFound, "{}", error.message);
        let refused = files.delete(&["/".to_string()]).await.unwrap_err();
        assert_eq!(refused.kind, ErrorKind::InvalidInput);

        // The CI setup puts links in the home folder. Like SFTP's, they are listed as links,
        // and `stat` describes what they point to.
        let home = files.list_dir("~").await.unwrap();
        if let Some(link) = home.entries.iter().find(|entry| entry.name == "data-link") {
            assert_eq!(link.kind, EntryKind::Symlink);
            assert_eq!(link.link_target, Some(LinkTarget::Dir));
            assert!(link.is_dir_like());
            assert!(files.stat(&link.path).await.unwrap().unwrap().is_dir);
        }
        if let Some(link) = home
            .entries
            .iter()
            .find(|entry| entry.name == "broken-link")
        {
            assert_eq!(link.link_target, Some(LinkTarget::Broken));
            assert_eq!(files.stat(&link.path).await.unwrap(), None);
        }
        if let Some(link) = home.entries.iter().find(|entry| entry.name == "file-link") {
            assert_eq!(link.link_target, Some(LinkTarget::File));
            let target = remote_path::join(&session.home, "data/linked.bin");
            let target_stat = files.stat(&target).await.unwrap().unwrap();
            assert_eq!(link.size, target_stat.size);
            let link_stat = files.stat(&link.path).await.unwrap().unwrap();
            assert!(!link_stat.is_dir);
            assert_eq!(link_stat.size, target_stat.size);
            assert_eq!(
                read_remote(files, &link.path).await,
                read_remote(files, &target).await
            );
        }
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfers_a_tree_both_ways() {
    for server in every_ftp_server() {
        let mut fixture = Fixture::new();
        let index = fixture.open(&server, "tree").await;
        let remote = fixture.at(index).remote.clone();
        let source = fixture.local.path().join("tree");
        write_tree(&source, 20 * MEBIBYTE + 12345);
        let dated = "nested/b with spaces.txt";
        let file_time = 1_500_000_000;
        std::fs::File::options()
            .write(true)
            .open(source.join(dated))
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(file_time as u64))
            .unwrap();

        let transfers = fixture.transfers(transfer_settings());
        fixture
            .upload(&transfers, index, std::slice::from_ref(&source), &remote)
            .await;
        let (list, _) = wait_until_settled(&transfers).await;
        assert_all_done(&list);
        let remote_tree = remote_path::join(&remote, "tree");
        let files = &fixture.at(index).files;
        assert_eq!(
            read_remote_tree(files, &remote_tree).await,
            read_tree(&source)
        );
        // Uploads keep the local file's time.
        let remote_modified = files
            .stat(&remote_path::join(&remote_tree, dated))
            .await
            .unwrap()
            .unwrap()
            .modified;
        assert_eq!(remote_modified, Some(file_time));
        drop(transfers);

        // Slow enough that idle workers have time to join the large file.
        let transfers = fixture.transfers(TransferSettings {
            download_limit_kib: 24 * 1024,
            ..transfer_settings()
        });
        let downloaded = fixture.local.path().join("downloaded");
        std::fs::create_dir_all(&downloaded).unwrap();
        fixture
            .download(&transfers, index, &[&remote_tree], &downloaded)
            .await;
        let (list, most_connections) = wait_until_settled(&transfers).await;
        assert_all_done(&list);
        assert!(most_connections > 1, "the large download was not split");
        assert_eq!(read_tree(&downloaded.join("tree")), read_tree(&source));
        drop(transfers);
        // Downloads keep the time the server reports.
        let local_modified = std::fs::metadata(downloaded.join("tree").join(dated))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(local_modified, file_time);

        // Without separate connections every worker takes turns on the browsing connection.
        let shared = fixture.local.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let transfers = fixture.transfers(TransferSettings {
            separate_connections: false,
            ..transfer_settings()
        });
        let large = remote_path::join(&remote_tree, "large.bin");
        let small = remote_path::join(&remote_tree, "a.txt");
        fixture
            .download(&transfers, index, &[&large, &small], &shared)
            .await;
        let (list, _) = wait_until_settled(&transfers).await;
        assert_all_done(&list);
        assert_eq!(
            std::fs::read(shared.join("large.bin")).unwrap(),
            std::fs::read(source.join("large.bin")).unwrap()
        );
        assert_eq!(
            std::fs::read(shared.join("a.txt")).unwrap(),
            std::fs::read(source.join("a.txt")).unwrap()
        );
        fixture
            .upload(&transfers, index, &[shared.join("a.txt")], &remote)
            .await;
        assert_all_done(&wait_until_settled(&transfers).await.0);
        drop(transfers);

        // The browsing connection still works after the workers used it.
        let files = &fixture.at(index).files;
        assert_eq!(
            read_remote(files, &remote_path::join(&remote, "a.txt")).await,
            std::fs::read(source.join("a.txt")).unwrap()
        );
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfers_in_active_mode() {
    for server in every_ftp_server() {
        let mut fixture = Fixture::new();
        let index = fixture
            .open_with(
                ConnectProfile {
                    ftp_active: true,
                    ..profile(&server, &server.password)
                },
                "active",
            )
            .await;
        let remote = fixture.at(index).remote.clone();
        let source = fixture.local.path().join("tree");
        write_tree(&source, 6 * MEBIBYTE + 1);

        let transfers = fixture.transfers(transfer_settings());
        fixture
            .upload(&transfers, index, std::slice::from_ref(&source), &remote)
            .await;
        assert_all_done(&wait_until_settled(&transfers).await.0);
        let downloaded = fixture.local.path().join("downloaded");
        std::fs::create_dir_all(&downloaded).unwrap();
        let remote_tree = remote_path::join(&remote, "tree");
        fixture
            .download(&transfers, index, &[&remote_tree], &downloaded)
            .await;
        assert_all_done(&wait_until_settled(&transfers).await.0);
        drop(transfers);
        assert_eq!(read_tree(&downloaded.join("tree")), read_tree(&source));
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resumes_interrupted_transfers() {
    let Some(server) = plain_server() else { return };
    let mut fixture = Fixture::new();
    let index = fixture.open(&server, "resume").await;
    let remote = fixture.at(index).remote.clone();
    let files = fixture.at(index).files.clone();
    let local = fixture.local.path().join("local");
    std::fs::create_dir_all(&local).unwrap();
    let large = local.join("large.bin");
    let large_bytes = pattern(12 * MEBIBYTE + 4321, 5);
    std::fs::write(&large, &large_bytes).unwrap();
    let remote_large = remote_path::join(&remote, "large.bin");

    // Resuming an upload keeps what is on the server, so a marked prefix survives.
    let mut partial = large_bytes[..5 * MEBIBYTE].to_vec();
    partial[..512].fill(0);
    write_remote(&files, &remote_large, &partial).await;
    let transfers = fixture.transfers(TransferSettings {
        exists_action: ExistsAction::Resume,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, index, std::slice::from_ref(&large), &remote)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    let mut expected = large_bytes.clone();
    expected[..512].fill(0);
    assert_eq!(read_remote(&files, &remote_large).await, expected);
    transfers.clear(&[JobState::Done]);

    // The same for a download.
    let downloads = fixture.local.path().join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let mut partial = large_bytes[..7 * MEBIBYTE].to_vec();
    partial[..512].fill(1);
    std::fs::write(downloads.join("large.bin"), &partial).unwrap();
    fixture
        .download(&transfers, index, &[&remote_large], &downloads)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    let mut expected_download = expected.clone();
    expected_download[..512].fill(1);
    assert_eq!(
        std::fs::read(downloads.join("large.bin")).unwrap(),
        expected_download
    );
    transfers.clear(&[JobState::Done]);
    drop(transfers);

    // A paused upload continues from the part already on the server.
    files
        .delete(std::slice::from_ref(&remote_large))
        .await
        .unwrap();
    let transfers = fixture.transfers(TransferSettings {
        upload_limit_kib: 4 * 1024,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, index, std::slice::from_ref(&large), &remote)
        .await;
    let id = wait_for_progress(&transfers, 3 * MEBIBYTE as u64).await;
    transfers.pause(&[id]);
    let (list, _) = wait_until_settled(&transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Paused);
    let kept = read_remote(&files, &remote_large).await;
    assert!(
        !kept.is_empty() && kept.len() < large_bytes.len(),
        "{} bytes on the server",
        kept.len()
    );
    assert_eq!(kept[..], large_bytes[..kept.len()]);
    transfers.configure(transfer_settings());
    transfers.resume(&[id]);
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(list.jobs[0].transferred, large_bytes.len() as u64);
    assert_eq!(read_remote(&files, &remote_large).await, large_bytes);
    transfers.clear(&[JobState::Done]);

    // A part on the server that no longer matches the file is not continued but replaced.
    files
        .delete(std::slice::from_ref(&remote_large))
        .await
        .unwrap();
    transfers.configure(TransferSettings {
        upload_limit_kib: 4 * 1024,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, index, std::slice::from_ref(&large), &remote)
        .await;
    let id = wait_for_progress(&transfers, 3 * MEBIBYTE as u64).await;
    transfers.pause(&[id]);
    let (list, _) = wait_until_settled(&transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Paused);
    let changed: Vec<u8> = read_remote(&files, &remote_large)
        .await
        .iter()
        .map(|byte| !byte)
        .collect();
    write_remote(&files, &remote_large, &changed).await;
    transfers.configure(transfer_settings());
    transfers.resume(&[id]);
    assert_all_done(&wait_until_settled(&transfers).await.0);
    assert_eq!(read_remote(&files, &remote_large).await, large_bytes);
    transfers.clear(&[JobState::Done]);

    // A paused download keeps its complete part in a temporary file and continues from there.
    std::fs::remove_file(downloads.join("large.bin")).unwrap();
    transfers.configure(TransferSettings {
        download_limit_kib: 4 * 1024,
        ..transfer_settings()
    });
    fixture
        .download(&transfers, index, &[&remote_large], &downloads)
        .await;
    let id = wait_for_progress(&transfers, 6 * MEBIBYTE as u64).await;
    transfers.pause(&[id]);
    let (list, _) = wait_until_settled(&transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Paused);
    assert!(!downloads.join("large.bin").exists());
    let kept = std::fs::read(downloads.join(".large.bin.poros-part")).unwrap();
    assert!(!kept.is_empty() && kept.len() < large_bytes.len());
    assert_eq!(kept[..], large_bytes[..kept.len()]);
    transfers.configure(transfer_settings());
    transfers.resume(&[id]);
    assert_all_done(&wait_until_settled(&transfers).await.0);
    assert_eq!(
        std::fs::read(downloads.join("large.bin")).unwrap(),
        large_bytes
    );
    assert!(!downloads.join(".large.bin.poros-part").exists());
    drop(transfers);
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfer_conflicts_follow_the_settings() {
    let Some(server) = plain_server() else { return };
    let mut fixture = Fixture::new();
    let index = fixture.open(&server, "conflicts").await;
    let remote = fixture.at(index).remote.clone();
    let files = fixture.at(index).files.clone();
    let small = fixture.local.path().join("a.txt");
    std::fs::write(&small, pattern(1024, 1)).unwrap();
    let transfers = fixture.transfers(transfer_settings());
    fixture
        .upload(&transfers, index, std::slice::from_ref(&small), &remote)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    transfers.clear(&[JobState::Done]);

    // Asking leaves the job waiting; renaming keeps both files.
    transfers.configure(TransferSettings {
        exists_action: ExistsAction::Ask,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, index, std::slice::from_ref(&small), &remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    let job = &list.jobs[0];
    assert_eq!(job.state, JobState::Conflict);
    assert_eq!(job.conflict.as_ref().unwrap().target_size, 1024);
    transfers.resolve(job.id, ExistsAction::Rename, false);
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(list.jobs[0].name, "a (1).txt");
    let renamed = remote_path::join(&remote, "a (1).txt");
    assert_eq!(
        read_remote(&files, &renamed).await,
        std::fs::read(&small).unwrap()
    );
    transfers.clear(&[JobState::Done]);

    transfers.configure(TransferSettings {
        exists_action: ExistsAction::Skip,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, index, std::slice::from_ref(&small), &remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Skipped);
    transfers.clear(&[JobState::Skipped]);

    // A folder in the way of a file fails the file.
    files.make_dir(&remote, "blocked.txt").await.unwrap();
    let blocked = fixture.local.path().join("blocked.txt");
    std::fs::write(&blocked, b"blocked").unwrap();
    transfers.configure(transfer_settings());
    fixture
        .upload(&transfers, index, std::slice::from_ref(&blocked), &remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Failed, "{:#?}", list.jobs[0]);
    drop(transfers);
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ftps_asks_about_the_certificate() {
    for server in tls_servers() {
        let (_config, manager) = new_manager();
        let error = manager
            .connect(profile(&server, &server.password), None, OWNER)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::HostKeyUnknown, "{}", error.message);
        let certificate = error.host_key.expect("the error describes the certificate");
        assert_eq!(certificate.algorithm, CERTIFICATE_ALGORITHM);
        assert_eq!(certificate.port, server.port);
        assert!(certificate.fingerprint.starts_with("SHA256:"));
        assert!(
            certificate
                .certificate_problem
                .as_deref()
                .is_some_and(|problem| !problem.is_empty()),
            "{certificate:?}"
        );

        let wrong = HostKeyApproval {
            fingerprint: "SHA256:not-it".into(),
            remember: true,
        };
        let error = manager
            .connect(profile(&server, &server.password), Some(wrong), OWNER)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::HostKeyUnknown);

        // Approved for this connection only.
        let once = HostKeyApproval {
            fingerprint: certificate.fingerprint.clone(),
            remember: false,
        };
        let info = manager
            .connect(profile(&server, &server.password), Some(once), OWNER)
            .await
            .unwrap();
        manager.disconnect(&info.id).await.unwrap();
        let error = manager
            .connect(profile(&server, &server.password), None, OWNER)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::HostKeyUnknown);

        // Another certificate on record for the server is reported as a change.
        manager
            .certificates
            .trust("127.0.0.1", server.port, "SHA256:someone-else")
            .unwrap();
        let error = manager
            .connect(profile(&server, &server.password), None, OWNER)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::HostKeyChanged, "{}", error.message);
        assert_eq!(error.host_key.unwrap().fingerprint, certificate.fingerprint);

        // Remembered from then on, and pinned for the workers' connections.
        let remembered = HostKeyApproval {
            fingerprint: certificate.fingerprint.clone(),
            remember: true,
        };
        let info = manager
            .connect(profile(&server, &server.password), Some(remembered), OWNER)
            .await
            .unwrap();
        manager.disconnect(&info.id).await.unwrap();
        let info = manager
            .connect(profile(&server, &server.password), None, OWNER)
            .await
            .unwrap();
        let files = files_of(&manager, &info).await;
        let scratch = format!("poros-certificate-{}", std::process::id());
        let existing = remote_path::join(&info.home, &scratch);
        if files.stat(&existing).await.unwrap().is_some() {
            files.delete(std::slice::from_ref(&existing)).await.unwrap();
        }
        let remote = files.make_dir("~", &scratch).await.unwrap();
        let local = tempfile::tempdir().unwrap();
        let source = local.path().join("encrypted.bin");
        std::fs::write(&source, pattern(2 * MEBIBYTE + 3, 6)).unwrap();
        let transfers =
            TransferManager::new(manager.clone(), Events::default(), transfer_settings());
        transfers
            .enqueue(EnqueueRequest {
                session_id: info.id.clone(),
                source_session_id: None,
                direction: Direction::Upload,
                target_directory: remote.clone(),
                items: vec![TransferItem {
                    path: source.to_string_lossy().into_owned(),
                    name: "encrypted.bin".into(),
                    is_dir: false,
                    size: 2 * MEBIBYTE as u64 + 3,
                }],
            })
            .await
            .unwrap();
        assert_all_done(&wait_until_settled(&transfers).await.0);
        drop(transfers);
        assert_eq!(
            read_remote(&files, &remote_path::join(&remote, "encrypted.bin")).await,
            std::fs::read(&source).unwrap()
        );
        files.delete(std::slice::from_ref(&remote)).await.unwrap();
        manager.disconnect(&info.id).await.unwrap();
    }
}

/// A download limit that holds a copy through this computer for several seconds; a direct
/// copy between the servers ignores it.
const RELAY_LIMIT_KIB: u32 = 1024;
const RELAY_FILE_SIZE: usize = 4 * MEBIBYTE;

/// Puts a small tree with one large file on the server at `index`, and returns its path and
/// what it holds.
async fn relay_source(fixture: &Fixture, index: usize) -> (String, Tree) {
    let local = fixture.local.path().join("relay-source");
    std::fs::create_dir_all(local.join("nested/deeper")).unwrap();
    std::fs::write(local.join("large.bin"), pattern(RELAY_FILE_SIZE, 8)).unwrap();
    std::fs::write(local.join("empty.bin"), b"").unwrap();
    std::fs::write(local.join("nested/b.txt"), pattern(70_000, 9)).unwrap();
    std::fs::write(local.join("nested/deeper/c.bin"), pattern(5000, 10)).unwrap();
    let remote = fixture.at(index).remote.clone();
    let transfers = fixture.transfers(transfer_settings());
    fixture
        .upload(&transfers, index, std::slice::from_ref(&local), &remote)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    drop(transfers);
    (
        remote_path::join(&remote, "relay-source"),
        read_tree(&local),
    )
}

/// Copies the tree at `source_path` from one server to another and checks what arrived.
/// Returns how long the copy took.
async fn relay_and_check(
    fixture: &Fixture,
    settings: TransferSettings,
    source: usize,
    source_path: &str,
    target: usize,
    expected: &Tree,
) -> Duration {
    let transfers = fixture.transfers(settings);
    let target_directory = fixture.at(target).remote.clone();
    let started = Instant::now();
    fixture
        .relay(
            &transfers,
            source,
            &[source_path],
            target,
            &target_directory,
        )
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    let elapsed = started.elapsed();
    assert_all_done(&list);
    assert!(list
        .jobs
        .iter()
        .all(|job| job.direction == Direction::Relay));
    drop(transfers);
    let copied = remote_path::join(&target_directory, remote_path::file_name(source_path));
    assert_eq!(
        &read_remote_tree(&fixture.at(target).files, &copied).await,
        expected
    );
    let large = fixture
        .at(target)
        .files
        .stat(&remote_path::join(&copied, "large.bin"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(large.size, RELAY_FILE_SIZE as u64);
    elapsed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fxp_sends_files_straight_between_servers() {
    let (Some(first), Some(second)) = (plain_server(), second_server()) else {
        return;
    };
    let mut fixture = Fixture::new();
    let source = fixture.open(&first, "fxp-source").await;
    let target = fixture.open(&second, "fxp-target").await;
    let (source_path, expected) = relay_source(&fixture, source).await;

    let elapsed = relay_and_check(
        &fixture,
        TransferSettings {
            fxp: true,
            download_limit_kib: RELAY_LIMIT_KIB,
            ..transfer_settings()
        },
        source,
        &source_path,
        target,
        &expected,
    )
    .await;
    assert!(
        elapsed < Duration::from_secs(3),
        "the copy took {elapsed:?}, as if it went through this computer"
    );

    // Copying over existing files replaces them.
    relay_and_check(
        &fixture,
        TransferSettings {
            fxp: true,
            ..transfer_settings()
        },
        source,
        &source_path,
        target,
        &expected,
    )
    .await;
    // One server can send to itself, over two of its connections.
    copy_within(&fixture, source, &source_path, true, &expected).await;
    resume_relay(&fixture, source, &source_path, target, true).await;

    // A target that refuses the file fails it promptly, with the server's reason.
    let transfers = fixture.transfers(TransferSettings {
        fxp: true,
        ..transfer_settings()
    });
    let large = remote_path::join(&source_path, "large.bin");
    let started = Instant::now();
    fixture
        .relay(&transfers, source, &[&large], target, "/")
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    let job = &list.jobs[0];
    assert_eq!(job.state, JobState::Failed, "{job:#?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the refusal took {:?}: {:?}",
        started.elapsed(),
        job.error
    );
    drop(transfers);
    fixture.close().await;
}

/// Puts the first part of the large file, with a marked start, where a relay of it goes, and
/// checks that a resumed relay keeps that part and adds the rest.
async fn resume_relay(
    fixture: &Fixture,
    source: usize,
    source_path: &str,
    target: usize,
    fxp: bool,
) {
    let large = remote_path::join(source_path, "large.bin");
    let source_bytes = read_remote(&fixture.at(source).files, &large).await;
    let target_files = &fixture.at(target).files;
    let target_directory = target_files
        .make_dir(&fixture.at(target).remote, "resumed")
        .await
        .unwrap();
    let target_path = remote_path::join(&target_directory, "large.bin");
    let mut expected = source_bytes[..RELAY_FILE_SIZE / 2 + 7].to_vec();
    expected[..100].fill(0);
    write_remote(target_files, &target_path, &expected).await;
    expected.extend_from_slice(&source_bytes[expected.len()..]);

    let transfers = fixture.transfers(TransferSettings {
        fxp,
        exists_action: ExistsAction::Resume,
        ..transfer_settings()
    });
    fixture
        .relay(&transfers, source, &[&large], target, &target_directory)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    drop(transfers);
    assert_eq!(read_remote(target_files, &target_path).await, expected);
    target_files
        .delete(std::slice::from_ref(&target_directory))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relays_stream_through_this_computer_without_fxp() {
    let (Some(first), Some(second)) = (plain_server(), second_server()) else {
        return;
    };
    let mut fixture = Fixture::new();
    let source = fixture.open(&first, "relay-source").await;
    let target = fixture.open(&second, "relay-target").await;
    let (source_path, expected) = relay_source(&fixture, source).await;

    let elapsed = relay_and_check(
        &fixture,
        TransferSettings {
            fxp: false,
            download_limit_kib: RELAY_LIMIT_KIB,
            ..transfer_settings()
        },
        source,
        &source_path,
        target,
        &expected,
    )
    .await;
    assert!(
        elapsed > Duration::from_secs(2),
        "the copy took {elapsed:?}, too fast for the download limit"
    );
    copy_within(&fixture, source, &source_path, false, &expected).await;
    resume_relay(&fixture, source, &source_path, target, false).await;
    fixture.close().await;
}

/// Copies a folder into a subfolder on the same server, which reads over a second connection.
async fn copy_within(
    fixture: &Fixture,
    index: usize,
    source_path: &str,
    fxp: bool,
    expected: &Tree,
) {
    let connected = fixture.at(index);
    let inside = connected
        .files
        .make_dir(&connected.remote, if fxp { "inside-fxp" } else { "inside" })
        .await
        .unwrap();
    let transfers = fixture.transfers(TransferSettings {
        fxp,
        ..transfer_settings()
    });
    fixture
        .relay(&transfers, index, &[source_path], index, &inside)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    drop(transfers);
    let copied = remote_path::join(&inside, remote_path::file_name(source_path));
    assert_eq!(&read_remote_tree(&connected.files, &copied).await, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relays_between_encrypted_servers() {
    let (Some(explicit), Some(implicit)) = (explicit_server(), implicit_server()) else {
        return;
    };
    let mut fixture = Fixture::new();
    let source = fixture.open(&explicit, "tls-source").await;
    let target = fixture.open(&implicit, "tls-target").await;
    let (source_path, expected) = relay_source(&fixture, source).await;
    // Neither server can take the TLS client role, so FXP is not tried.
    relay_and_check(
        &fixture,
        TransferSettings {
            fxp: true,
            ..transfer_settings()
        },
        source,
        &source_path,
        target,
        &expected,
    )
    .await;
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relays_between_ftp_and_sftp() {
    let (Some(ftp), Some(ssh)) = (plain_server(), ssh_server()) else {
        return;
    };
    let mut fixture = Fixture::new();
    let source = fixture.open(&ftp, "to-sftp").await;
    let target = fixture.open(&ssh, "from-ftp").await;
    let (source_path, expected) = relay_source(&fixture, source).await;
    relay_and_check(
        &fixture,
        TransferSettings {
            fxp: true,
            ..transfer_settings()
        },
        source,
        &source_path,
        target,
        &expected,
    )
    .await;
    resume_relay(&fixture, source, &source_path, target, true).await;

    // And back, from the SFTP server into a folder on the FTP server.
    let back = fixture
        .at(source)
        .files
        .make_dir(&fixture.at(source).remote, "back")
        .await
        .unwrap();
    let copied = remote_path::join(&fixture.at(target).remote, "relay-source");
    let transfers = fixture.transfers(transfer_settings());
    fixture
        .relay(&transfers, target, &[&copied], source, &back)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    drop(transfers);
    assert_eq!(
        read_remote_tree(
            &fixture.at(source).files,
            &remote_path::join(&back, "relay-source")
        )
        .await,
        expected
    );
    fixture.close().await;
}

/// Two servers that each lend their one browsing connection, copying to each other at once:
/// neither copy may hold one server while waiting for the other.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opposite_relays_share_browsing_connections() {
    let (Some(first), Some(second)) = (plain_server(), second_server()) else {
        return;
    };
    let mut fixture = Fixture::new();
    let first_index = fixture.open(&first, "opposite-first").await;
    let second_index = fixture.open(&second, "opposite-second").await;
    let (first_path, expected) = relay_source(&fixture, first_index).await;
    let (second_path, _) = relay_source(&fixture, second_index).await;

    for fxp in [true, false] {
        let into_second = fixture
            .at(second_index)
            .files
            .make_dir(
                &fixture.at(second_index).remote,
                &format!("from-first-{fxp}"),
            )
            .await
            .unwrap();
        let into_first = fixture
            .at(first_index)
            .files
            .make_dir(
                &fixture.at(first_index).remote,
                &format!("from-second-{fxp}"),
            )
            .await
            .unwrap();
        let transfers = fixture.transfers(TransferSettings {
            fxp,
            separate_connections: false,
            ..transfer_settings()
        });
        fixture
            .relay(
                &transfers,
                first_index,
                &[&first_path],
                second_index,
                &into_second,
            )
            .await;
        fixture
            .relay(
                &transfers,
                second_index,
                &[&second_path],
                first_index,
                &into_first,
            )
            .await;
        assert_all_done(&wait_until_settled(&transfers).await.0);
        drop(transfers);
        for (index, folder) in [(second_index, &into_second), (first_index, &into_first)] {
            let copied = remote_path::join(folder, "relay-source");
            assert_eq!(
                read_remote_tree(&fixture.at(index).files, &copied).await,
                expected
            );
        }
    }
    fixture.close().await;
}
