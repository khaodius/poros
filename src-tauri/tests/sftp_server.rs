//! End-to-end tests against a real SSH server. Skipped unless `POROS_TEST_SSH_PORT` is set.
//!
//! The server must listen on 127.0.0.1 and accept, for `POROS_TEST_SSH_USER`:
//! the password in `POROS_TEST_SSH_PASSWORD`, and every `tests/fixtures/keys/*.pub` key.
//! The end-to-end step in `.github/workflows/ci.yml` starts a suitable throwaway `sshd`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use poros_lib::automation::remote_command::{self, OutputStream};
use poros_lib::automation::scheduler::{self, TaskAction, TaskContext};
use poros_lib::connections::{AuthType, ConnectionStore, SavedConnection, SecretStore};
use poros_lib::error::{AppResult, ErrorKind};
use poros_lib::events::Events;
use poros_lib::model::{EntryKind, LinkTarget};
use poros_lib::protocol::Protocol;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::{ConnectionSettings, TransferSettings};
use poros_lib::ssh::proxy::{Proxy, ProxyKind};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::sync::{
    CompareMode, SyncAction, SyncChoice, SyncDirection, SyncManager, SyncPlanView, SyncReason,
    SyncRequest, SyncRunRequest,
};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobSnapshot, JobState, TransferItem, TransferList,
    TransferManager,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

/// The window label sessions belong to.
const OWNER: &str = "main";
const MEBIBYTE: usize = 1024 * 1024;

struct Server {
    port: u16,
    user: String,
    password: String,
}

fn server() -> Option<Server> {
    let port = std::env::var("POROS_TEST_SSH_PORT").ok()?.parse().ok()?;
    Some(Server {
        port,
        user: std::env::var("POROS_TEST_SSH_USER").unwrap_or_else(|_| "poros".into()),
        password: std::env::var("POROS_TEST_SSH_PASSWORD").unwrap_or_else(|_| "poros-pass".into()),
    })
}

fn profile(server: &Server, auth: AuthMethod) -> ConnectProfile {
    ConnectProfile {
        protocol: Protocol::Sftp,
        host: "127.0.0.1".into(),
        port: server.port,
        username: server.user.clone(),
        auth,
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
        route: Route::default(),
    }
}

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/keys")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// A manager whose app known_hosts already trusts the test server.
async fn trusted_manager(server: &Server) -> (tempfile::TempDir, SessionManager) {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = SessionManager::new(temp_dir.path().join("known_hosts"), Events::default());
    let password = AuthMethod::Password {
        password: server.password.clone(),
    };
    let error = manager
        .connect(profile(server, password.clone()), None, OWNER)
        .await
        .err();
    if let Some(error) = error {
        let host_key = error
            .host_key
            .expect("first connect should ask about the host key");
        let info = manager
            .connect(
                profile(server, password),
                Some(HostKeyApproval {
                    fingerprint: host_key.fingerprint,
                    remember: true,
                }),
                OWNER,
            )
            .await
            .unwrap();
        manager.disconnect(&info.id).await.unwrap();
    }
    (temp_dir, manager)
}

#[tokio::test]
async fn host_key_prompt_then_remembered() {
    let Some(server) = server() else { return };
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = SessionManager::new(temp_dir.path().join("known_hosts"), Events::default());
    let auth = AuthMethod::Password {
        password: server.password.clone(),
    };

    let error = manager
        .connect(profile(&server, auth.clone()), None, OWNER)
        .await
        .unwrap_err();
    // The server may already be in ~/.ssh/known_hosts on a dev machine.
    if error.kind == ErrorKind::HostKeyUnknown {
        let host_key = error.host_key.unwrap();
        assert!(host_key.fingerprint.starts_with("SHA256:"));

        let wrong = HostKeyApproval {
            fingerprint: "SHA256:not-it".into(),
            remember: true,
        };
        let error = manager
            .connect(profile(&server, auth.clone()), Some(wrong), OWNER)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::HostKeyUnknown);

        let ok = HostKeyApproval {
            fingerprint: host_key.fingerprint,
            remember: true,
        };
        let info = manager
            .connect(profile(&server, auth.clone()), Some(ok), OWNER)
            .await
            .unwrap();
        manager.disconnect(&info.id).await.unwrap();
    }

    let info = manager
        .connect(profile(&server, auth), None, OWNER)
        .await
        .unwrap();
    manager.disconnect(&info.id).await.unwrap();
}

#[tokio::test]
async fn connects_with_fixed_socket_buffers() {
    let Some(server) = server() else { return };
    let (_dir, manager) = trusted_manager(&server).await;
    let password = AuthMethod::Password {
        password: server.password.clone(),
    };
    let info = manager
        .connect(
            ConnectProfile {
                receive_buffer_kib: Some(128),
                send_buffer_kib: Some(128),
                ..profile(&server, password)
            },
            None,
            OWNER,
        )
        .await
        .unwrap();
    manager.disconnect(&info.id).await.unwrap();
}

#[tokio::test]
async fn password_auth_and_wrong_password() {
    let Some(server) = server() else { return };
    let (_dir, manager) = trusted_manager(&server).await;
    let error = manager
        .connect(
            profile(
                &server,
                AuthMethod::Password {
                    password: "wrong".into(),
                },
            ),
            None,
            OWNER,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::AuthFailed, "{}", error.message);
}

#[tokio::test]
async fn every_key_format_authenticates() {
    let Some(server) = server() else { return };
    let (_dir, manager) = trusted_manager(&server).await;
    let mut files: Vec<(String, Option<String>)> = Vec::new();
    for name in ["rsa", "ed25519", "ecdsa256", "ecdsa384", "ecdsa521"] {
        for version in ["v2", "v3"] {
            files.push((format!("{name}-{version}-plain.ppk"), None));
            files.push((
                format!("{name}-{version}-enc.ppk"),
                Some("poros-test".into()),
            ));
        }
    }
    files.push((
        "ed25519-v3-argon2i-enc.ppk".into(),
        Some("poros-test".into()),
    ));
    files.push((
        "ed25519-v3-argon2d-enc.ppk".into(),
        Some("poros-test".into()),
    ));
    files.push(("nocomment-v3-plain.ppk".into(), None));
    files.push(("openssh-ed25519".into(), None));
    files.push(("openssh-ed25519-enc".into(), Some("poros-test".into())));
    files.push(("pem-rsa".into(), None));
    files.push(("pem-rsa-enc".into(), Some("poros-test".into())));
    files.push(("pkcs8-ecdsa".into(), None));

    for (file, passphrase) in files {
        let auth = AuthMethod::PublicKey {
            key_path: fixture(&file),
            passphrase,
        };
        let info = manager
            .connect(profile(&server, auth), None, OWNER)
            .await
            .unwrap_or_else(|error| panic!("{file}: {}", error.message));
        manager.disconnect(&info.id).await.unwrap();
    }
}

#[tokio::test]
async fn browse_and_manage_files() {
    let Some(server) = server() else { return };
    let (_dir, manager) = trusted_manager(&server).await;
    let info = manager
        .connect(
            profile(
                &server,
                AuthMethod::Password {
                    password: server.password.clone(),
                },
            ),
            None,
            OWNER,
        )
        .await
        .unwrap();
    let session = manager.get(&info.id).await.unwrap();
    let fs = session.sftp().unwrap();

    let scratch = format!("poros-test-{}", std::process::id());
    let root = fs.make_dir("~", &scratch).await.unwrap();
    let sub = fs.make_dir(&root, "sub").await.unwrap();
    fs.make_dir(&sub, "deeper").await.unwrap();
    let dup = fs.make_dir(&root, "sub").await.unwrap_err();
    assert_eq!(dup.kind, ErrorKind::AlreadyExists);

    let listing = fs.list_dir(&root).await.unwrap();
    assert_eq!(listing.path, root);
    assert_eq!(listing.parent.as_deref(), Some(info.home.as_str()));
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].kind, EntryKind::Dir);

    let renamed = fs.rename(&sub, "renamed").await.unwrap();
    assert!(renamed.ends_with("/renamed"));

    // Symlinks are resolved so the UI can navigate into linked folders.
    let home = fs.list_dir("~").await.unwrap();
    if let Some(link) = home.entries.iter().find(|entry| entry.name == "data-link") {
        assert_eq!(link.kind, EntryKind::Symlink);
        assert_eq!(link.link_target, Some(LinkTarget::Dir));
    }
    if let Some(link) = home
        .entries
        .iter()
        .find(|entry| entry.name == "broken-link")
    {
        assert_eq!(link.link_target, Some(LinkTarget::Broken));
    }

    fs.delete(std::slice::from_ref(&root)).await.unwrap();
    let error = fs.list_dir(&root).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::NotFound);

    manager.disconnect(&info.id).await.unwrap();
    let error = manager.get(&info.id).await.err().unwrap();
    assert_eq!(error.kind, ErrorKind::SessionNotFound);
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

fn write_tree(root: &Path) {
    std::fs::create_dir_all(root.join("nested/deeper")).unwrap();
    std::fs::create_dir_all(root.join("empty folder")).unwrap();
    std::fs::write(root.join("a.txt"), pattern(1024, 1)).unwrap();
    std::fs::write(root.join("empty.bin"), b"").unwrap();
    std::fs::write(root.join("large.bin"), pattern(20 * MEBIBYTE + 12345, 2)).unwrap();
    std::fs::write(root.join("nested/b.txt"), pattern(70_000, 3)).unwrap();
    std::fs::write(root.join("nested/deeper/c.bin"), pattern(3 * MEBIBYTE, 4)).unwrap();
}

/// Every file and folder under `root`, relative to it, with file contents.
fn read_tree(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
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

fn transfer_settings() -> TransferSettings {
    TransferSettings {
        workers: 4,
        segment_threshold_mib: 1,
        exists_action: ExistsAction::Overwrite,
        retry_attempts: 0,
        ..TransferSettings::default()
    }
}

struct TransferFixture {
    _known_hosts: tempfile::TempDir,
    local: tempfile::TempDir,
    manager: Arc<SessionManager>,
    session: SessionInfo,
    /// Absolute remote scratch folder, removed on drop.
    remote: String,
}

impl TransferFixture {
    async fn new(server: &Server, name: &str) -> Self {
        let (known_hosts, manager) = trusted_manager(server).await;
        let manager = Arc::new(manager);
        let session = manager
            .connect(
                profile(
                    server,
                    AuthMethod::Password {
                        password: server.password.clone(),
                    },
                ),
                None,
                OWNER,
            )
            .await
            .unwrap();
        let fs = manager
            .get(&session.id)
            .await
            .unwrap()
            .sftp()
            .unwrap()
            .clone();
        let scratch = format!("poros-{name}-{}", std::process::id());
        let existing = remote_path_join(&session.home, &scratch);
        let _ = fs.delete(std::slice::from_ref(&existing)).await;
        let remote = fs.make_dir("~", &scratch).await.unwrap();
        Self {
            _known_hosts: known_hosts,
            local: tempfile::tempdir().unwrap(),
            manager,
            session,
            remote,
        }
    }

    fn transfers(&self, settings: TransferSettings) -> TransferManager {
        TransferManager::new(self.manager.clone(), Events::default(), settings)
    }

    async fn upload(&self, transfers: &TransferManager, sources: &[PathBuf], target: &str) {
        let items = sources
            .iter()
            .map(|source| TransferItem {
                path: source.to_string_lossy().into_owned(),
                name: source.file_name().unwrap().to_string_lossy().into_owned(),
                is_dir: source.is_dir(),
                size: source.metadata().unwrap().len(),
            })
            .collect();
        self.enqueue(transfers, Direction::Upload, target, items)
            .await;
    }

    async fn download(&self, transfers: &TransferManager, sources: &[&str], target: &Path) {
        let fs = self
            .manager
            .get(&self.session.id)
            .await
            .unwrap()
            .sftp()
            .unwrap()
            .clone();
        let mut items = Vec::new();
        for source in sources {
            let parent = source.rsplit_once('/').unwrap().0;
            let listing = fs.list_dir(parent).await.unwrap();
            let entry = listing
                .entries
                .into_iter()
                .find(|entry| entry.path == *source)
                .unwrap_or_else(|| panic!("{source} is not on the server"));
            items.push(TransferItem {
                path: entry.path,
                name: entry.name,
                is_dir: entry.kind == EntryKind::Dir,
                size: entry.size,
            });
        }
        self.enqueue(
            transfers,
            Direction::Download,
            &target.to_string_lossy(),
            items,
        )
        .await;
    }

    async fn enqueue(
        &self,
        transfers: &TransferManager,
        direction: Direction,
        target: &str,
        items: Vec<TransferItem>,
    ) {
        transfers
            .enqueue(EnqueueRequest {
                session_id: self.session.id.clone(),
                source_session_id: None,
                direction,
                target_directory: target.to_string(),
                items,
            })
            .await
            .unwrap();
    }

    async fn remote_bytes(&self, path: &str) -> Vec<u8> {
        let target = self.local.path().join("fetched");
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target).unwrap();
        let transfers = self.transfers(transfer_settings());
        self.download(&transfers, &[path], &target).await;
        wait_until_settled(&transfers).await;
        let name = path.rsplit_once('/').unwrap().1;
        std::fs::read(target.join(name)).unwrap()
    }

    async fn close(self) {
        let fs = self
            .manager
            .get(&self.session.id)
            .await
            .unwrap()
            .sftp()
            .unwrap()
            .clone();
        fs.delete(std::slice::from_ref(&self.remote)).await.unwrap();
        self.manager.disconnect(&self.session.id).await.unwrap();
    }
}

fn remote_path_join(folder: &str, name: &str) -> String {
    format!("{}/{name}", folder.trim_end_matches('/'))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfer_tree_both_ways_with_parallel_workers() {
    let Some(server) = server() else { return };
    let fixture = TransferFixture::new(&server, "tree").await;
    let source = fixture.local.path().join("tree");
    write_tree(&source);

    // Slow enough that idle workers have time to join the large file.
    let transfers = fixture.transfers(TransferSettings {
        upload_limit_kib: 24 * 1024,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, std::slice::from_ref(&source), &fixture.remote)
        .await;
    let (list, most_connections) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert!(most_connections > 1, "the large upload was not split");
    drop(transfers);

    let transfers = fixture.transfers(transfer_settings());
    let downloaded = fixture.local.path().join("downloaded");
    std::fs::create_dir_all(&downloaded).unwrap();
    let remote_tree = remote_path_join(&fixture.remote, "tree");
    fixture
        .download(&transfers, &[&remote_tree], &downloaded)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(read_tree(&downloaded.join("tree")), read_tree(&source));

    // Without separate connections every worker uses a channel of the browsing connection.
    let shared = fixture.local.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    drop(transfers);
    let transfers = fixture.transfers(TransferSettings {
        separate_connections: false,
        ..transfer_settings()
    });
    let large = remote_path_join(&remote_tree, "large.bin");
    fixture.download(&transfers, &[&large], &shared).await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(
        std::fs::read(shared.join("large.bin")).unwrap(),
        std::fs::read(source.join("large.bin")).unwrap()
    );
    drop(transfers);
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfer_conflicts_and_resume() {
    let Some(server) = server() else { return };
    let fixture = TransferFixture::new(&server, "conflicts").await;
    let source = fixture.local.path().join("source");
    write_tree(&source);
    let small = source.join("a.txt");
    let large = source.join("large.bin");
    let large_bytes = std::fs::read(&large).unwrap();

    let transfers = fixture.transfers(transfer_settings());
    fixture
        .upload(&transfers, &[small.clone(), large.clone()], &fixture.remote)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    transfers.clear(&[JobState::Done]);

    // Asking leaves the job waiting; renaming keeps both files.
    transfers.configure(TransferSettings {
        exists_action: ExistsAction::Ask,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, std::slice::from_ref(&small), &fixture.remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    let job = &list.jobs[0];
    assert_eq!(job.state, JobState::Conflict);
    assert_eq!(job.conflict.as_ref().unwrap().target_size, 1024);
    transfers.resolve(job.id, ExistsAction::Rename, false);
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(list.jobs[0].name, "a (1).txt");
    let renamed = remote_path_join(&fixture.remote, "a (1).txt");
    assert_eq!(
        fixture.remote_bytes(&renamed).await,
        std::fs::read(&small).unwrap()
    );
    transfers.clear(&[JobState::Done]);

    transfers.configure(TransferSettings {
        exists_action: ExistsAction::Skip,
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, std::slice::from_ref(&small), &fixture.remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Skipped);
    transfers.clear(&[JobState::Skipped]);

    // Resuming keeps what is already there, so a marked prefix survives.
    let downloads = fixture.local.path().join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let mut partial = large_bytes[..7 * MEBIBYTE].to_vec();
    partial[..512].fill(0);
    std::fs::write(downloads.join("large.bin"), &partial).unwrap();
    transfers.configure(TransferSettings {
        exists_action: ExistsAction::Resume,
        ..transfer_settings()
    });
    let remote_large = remote_path_join(&fixture.remote, "large.bin");
    fixture
        .download(&transfers, &[&remote_large], &downloads)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    let mut expected = large_bytes.clone();
    expected[..512].fill(0);
    assert_eq!(
        std::fs::read(downloads.join("large.bin")).unwrap(),
        expected
    );
    transfers.clear(&[JobState::Done]);

    // A paused download keeps its complete part and continues from there.
    std::fs::remove_file(downloads.join("large.bin")).unwrap();
    transfers.configure(TransferSettings {
        download_limit_kib: 4 * 1024,
        ..transfer_settings()
    });
    fixture
        .download(&transfers, &[&remote_large], &downloads)
        .await;
    let deadline = Instant::now() + Duration::from_secs(30);
    let id = loop {
        let list = transfers.list();
        if let Some(job) = list.jobs.first() {
            if job.transferred > 6 * MEBIBYTE as u64 {
                break job.id;
            }
        }
        assert!(Instant::now() < deadline, "the download did not start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    transfers.pause(&[id]);
    let (list, _) = wait_until_settled(&transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Paused);
    // The unfinished file waits under a temporary name.
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

/// Waits for a running upload to pass `bytes`, then pauses it.
async fn pause_after(transfers: &TransferManager, bytes: u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let id = loop {
        let list = transfers.list();
        if let Some(job) = list.jobs.iter().find(|job| job.transferred > bytes) {
            break job.id;
        }
        assert!(Instant::now() < deadline, "the transfer did not start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    transfers.pause(&[id]);
    let (list, _) = wait_until_settled(transfers).await;
    assert_eq!(list.jobs[0].state, JobState::Paused);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replaced_files_stay_whole_and_the_queue_survives_a_restart() {
    let Some(server) = server() else { return };
    let fixture = TransferFixture::new(&server, "replace").await;
    let session = fixture.manager.get(&fixture.session.id).await.unwrap();
    let fs = session.sftp().unwrap();
    let source = fixture.local.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let file = source.join("data.bin");
    let old_bytes = pattern(3 * MEBIBYTE + 99, 4);
    let new_bytes = pattern(12 * MEBIBYTE + 4321, 5);
    let remote_file = remote_path_join(&fixture.remote, "data.bin");
    let remote_partial = remote_path_join(&fixture.remote, ".data.bin.poros-part");
    // Whole files over SFTP; rsync keeps its own temporary files.
    let checked = TransferSettings {
        verify_checksums: true,
        delta_transfers: false,
        ..transfer_settings()
    };

    // Uploads and downloads checked against the server's sha256sum.
    std::fs::write(&file, &old_bytes).unwrap();
    let transfers = fixture.transfers(checked.clone());
    fixture
        .upload(&transfers, std::slice::from_ref(&file), &fixture.remote)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    let downloads = fixture.local.path().join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    fixture
        .download(&transfers, &[&remote_file], &downloads)
        .await;
    assert_all_done(&wait_until_settled(&transfers).await.0);
    assert_eq!(
        std::fs::read(downloads.join("data.bin")).unwrap(),
        old_bytes
    );
    drop(transfers);

    // While a replacement is unfinished, the old file stays whole under its name.
    std::fs::write(&file, &new_bytes).unwrap();
    let queue_file = fixture.local.path().join("transfers.json");
    let transfers = fixture.transfers(TransferSettings {
        upload_limit_kib: 4 * 1024,
        ..checked.clone()
    });
    transfers.keep_queue_in(queue_file.clone());
    fixture
        .upload(&transfers, std::slice::from_ref(&file), &fixture.remote)
        .await;
    pause_after(&transfers, 4 * MEBIBYTE as u64).await;
    assert!(fs.stat(&remote_partial).await.unwrap().is_some());
    assert_eq!(fixture.remote_bytes(&remote_file).await, old_bytes);
    transfers.save_queue();
    drop(transfers);

    // The next start finds the paused upload and continues it.
    let transfers = fixture.transfers(checked.clone());
    transfers.keep_queue_in(queue_file.clone());
    let list = transfers.list();
    assert_eq!(list.jobs.len(), 1);
    assert_eq!(list.jobs[0].state, JobState::Paused);
    assert!(list.jobs[0].transferred > 0);
    transfers.resume(&[list.jobs[0].id]);
    assert_all_done(&wait_until_settled(&transfers).await.0);
    assert_eq!(fixture.remote_bytes(&remote_file).await, new_bytes);
    assert!(fs.stat(&remote_partial).await.unwrap().is_none());
    transfers.clear(&[JobState::Done]);
    transfers.save_queue();
    assert!(!queue_file.exists());

    // Removing an unfinished upload deletes its temporary file.
    transfers.configure(TransferSettings {
        upload_limit_kib: 4 * 1024,
        ..checked
    });
    fixture
        .upload(&transfers, std::slice::from_ref(&file), &fixture.remote)
        .await;
    pause_after(&transfers, MEBIBYTE as u64).await;
    assert!(fs.stat(&remote_partial).await.unwrap().is_some());
    transfers.remove(&[transfers.list().jobs[0].id]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while fs.stat(&remote_partial).await.unwrap().is_some() {
        assert!(
            Instant::now() < deadline,
            "the temporary file was left behind"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(fixture.remote_bytes(&remote_file).await, new_bytes);
    drop(transfers);
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delta_transfers_send_only_the_changes() {
    let Some(server) = server() else { return };
    let fixture = TransferFixture::new(&server, "delta").await;
    let local = fixture.local.path().join("local");
    std::fs::create_dir_all(&local).unwrap();
    let file = local.join("data.bin");
    let original = pattern(6 * MEBIBYTE + 777, 9);
    std::fs::write(&file, &original).unwrap();
    let transfers = fixture.transfers(transfer_settings());
    fixture
        .upload(&transfers, std::slice::from_ref(&file), &fixture.remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(list.jobs[0].delta_bytes, None);
    transfers.clear(&[JobState::Done]);

    // A few edits in the middle and a longer tail.
    let mut edited = original.clone();
    edited[MEBIBYTE..MEBIBYTE + 100].fill(7);
    edited.splice(3 * MEBIBYTE..3 * MEBIBYTE, pattern(5000, 10));
    edited.extend(pattern(20_000, 11));
    std::fs::write(&file, &edited).unwrap();
    fixture
        .upload(&transfers, std::slice::from_ref(&file), &fixture.remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    let sent = list.jobs[0]
        .delta_bytes
        .expect("the upload did not use rsync");
    assert!(sent < 200_000, "sent {sent} bytes");
    assert_eq!(list.jobs[0].transferred, edited.len() as u64);
    let remote_file = remote_path_join(&fixture.remote, "data.bin");
    let fs = fixture
        .manager
        .get(&fixture.session.id)
        .await
        .unwrap()
        .sftp()
        .unwrap()
        .clone();
    let local_modified = std::fs::metadata(&file)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert_eq!(
        fs.stat(&remote_file).await.unwrap().unwrap().modified,
        Some(local_modified)
    );
    transfers.clear(&[JobState::Done]);

    // Downloading over the old version rebuilds the new one from it.
    std::fs::write(&file, &original).unwrap();
    fixture.download(&transfers, &[&remote_file], &local).await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    let received = list.jobs[0]
        .delta_bytes
        .expect("the download did not use rsync");
    assert!(received < 200_000, "received {received} bytes");
    assert_eq!(std::fs::read(&file).unwrap(), edited);
    let leftovers: Vec<_> = std::fs::read_dir(&local)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, vec![std::ffi::OsString::from("data.bin")]);
    drop(transfers);

    // Workers sharing the browsing connection run rsync on it too.
    std::fs::write(&file, &original).unwrap();
    let transfers = fixture.transfers(TransferSettings {
        separate_connections: false,
        ..transfer_settings()
    });
    fixture.download(&transfers, &[&remote_file], &local).await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert!(list.jobs[0].delta_bytes.is_some());
    assert_eq!(std::fs::read(&file).unwrap(), edited);
    drop(transfers);

    // Without rsync on the server the whole file is copied.
    std::fs::write(&file, &original).unwrap();
    let transfers = fixture.transfers(TransferSettings {
        rsync_path: "/nonexistent/rsync".into(),
        ..transfer_settings()
    });
    fixture
        .upload(&transfers, std::slice::from_ref(&file), &fixture.remote)
        .await;
    let (list, _) = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(list.jobs[0].delta_bytes, None);
    assert_eq!(fixture.remote_bytes(&remote_file).await, original);
    drop(transfers);
    fixture.close().await;
}

fn sync_request(
    fixture: &TransferFixture,
    local: &Path,
    remote: &str,
    direction: SyncDirection,
    compare: CompareMode,
) -> SyncRequest {
    SyncRequest {
        request_id: "test".into(),
        session_id: fixture.session.id.clone(),
        local_path: local.to_string_lossy().into_owned(),
        remote_path: remote.to_string(),
        direction,
        compare,
        delete_extraneous: true,
        skip_newer_on_target: false,
        ignore_existing: false,
        time_tolerance_secs: 2,
        excludes: vec!["*.log".into()],
    }
}

fn planned(plan: &SyncPlanView) -> Vec<(String, SyncAction, SyncReason)> {
    plan.items
        .iter()
        .map(|item| (item.path.clone(), item.action, item.reason))
        .collect()
}

async fn run_everything(sync: &SyncManager, transfers: &TransferManager, plan: &SyncPlanView) {
    let choices = plan
        .items
        .iter()
        .map(|item| SyncChoice {
            id: item.id,
            action: item.action,
        })
        .collect();
    let summary = sync
        .run(
            transfers,
            SyncRunRequest {
                plan_id: plan.plan_id.clone(),
                choices,
            },
        )
        .await
        .unwrap();
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_all_done(&wait_until_settled(transfers).await.0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_mirrors_folders_both_ways() {
    let Some(server) = server() else { return };
    let fixture = TransferFixture::new(&server, "sync").await;
    let local = fixture.local.path().join("site");
    write_tree(&local);
    std::fs::write(local.join("debug.log"), b"log").unwrap();
    let sync = SyncManager::new(fixture.manager.clone(), Events::default());
    let transfers = fixture.transfers(transfer_settings());
    let upload = sync_request(
        &fixture,
        &local,
        &fixture.remote,
        SyncDirection::Upload,
        CompareMode::SizeAndTime,
    );

    let plan = sync.compare(upload.clone()).await.unwrap();
    use SyncAction::{DeleteRemote, Download, Upload};
    use SyncReason::{ContentDiffers, Extraneous, New};
    assert_eq!(
        planned(&plan),
        [
            ("a.txt".into(), Upload, New),
            ("empty folder".into(), Upload, New),
            ("empty.bin".into(), Upload, New),
            ("large.bin".into(), Upload, New),
            ("nested".into(), Upload, New),
        ]
    );
    assert_eq!(plan.counts.excluded, 1);
    assert_eq!(plan.items[4].files, 2);
    run_everything(&sync, &transfers, &plan).await;
    transfers.clear(&[JobState::Done]);
    let plan = sync.compare(upload.clone()).await.unwrap();
    assert!(plan.items.is_empty(), "{:?}", planned(&plan));
    assert_eq!(plan.counts.unchanged, 5);

    // A changed file, a folder only on the server, and an excluded file there that stays.
    std::fs::write(local.join("a.txt"), pattern(2000, 5)).unwrap();
    let fs = fixture
        .manager
        .get(&fixture.session.id)
        .await
        .unwrap()
        .sftp()
        .unwrap()
        .clone();
    fs.make_dir(&fixture.remote, "old").await.unwrap();
    let kept_log = remote_path_join(&fixture.remote, "keep.log");
    let handle = fs.open_for_write(&kept_log, true, None).await.unwrap();
    fs.write_chunk(&handle, 0, b"server log".to_vec())
        .await
        .unwrap();
    fs.close_handle(handle).await.unwrap();
    let plan = sync.compare(upload.clone()).await.unwrap();
    assert_eq!(
        planned(&plan),
        [
            ("a.txt".into(), Upload, SyncReason::Changed),
            ("old".into(), DeleteRemote, Extraneous),
        ]
    );
    run_everything(&sync, &transfers, &plan).await;
    transfers.clear(&[JobState::Done]);
    assert!(fs.stat(&kept_log).await.unwrap().is_some());

    // Mirroring the server into an empty folder brings everything but excluded files.
    let mirror = fixture.local.path().join("mirror");
    std::fs::create_dir_all(&mirror).unwrap();
    let download = sync_request(
        &fixture,
        &mirror,
        &fixture.remote,
        SyncDirection::Download,
        CompareMode::SizeAndTime,
    );
    let plan = sync.compare(download.clone()).await.unwrap();
    assert!(plan.items.iter().all(|item| item.action == Download));
    run_everything(&sync, &transfers, &plan).await;
    transfers.clear(&[JobState::Done]);
    std::fs::remove_file(local.join("debug.log")).unwrap();
    assert_eq!(read_tree(&mirror), read_tree(&local));
    assert!(sync.compare(download).await.unwrap().items.is_empty());

    // Same size and time, different bytes: only a content comparison notices.
    let changed = local.join("nested/b.txt");
    let modified = std::fs::metadata(&changed).unwrap().modified().unwrap();
    let mut bytes = std::fs::read(&changed).unwrap();
    bytes[100] ^= 0xff;
    std::fs::write(&changed, &bytes).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&changed)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert!(sync.compare(upload.clone()).await.unwrap().items.is_empty());
    let plan = sync
        .compare(SyncRequest {
            compare: CompareMode::Checksum,
            ..upload
        })
        .await
        .unwrap();
    assert_eq!(
        planned(&plan),
        [("nested/b.txt".into(), Upload, ContentDiffers)]
    );
    drop(transfers);
    fixture.close().await;
}

/// A SOCKS5 or HTTP CONNECT proxy on a local port that relays to wherever it is asked, and
/// counts the connections it carried.
async fn start_proxy(kind: ProxyKind) -> (Proxy, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let carried = Arc::new(AtomicUsize::new(0));
    let counter = carried.clone();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let counter = counter.clone();
            tokio::spawn(async move {
                if let Some(mut server) = accept_tunnel(kind, client).await {
                    counter.fetch_add(1, Ordering::SeqCst);
                    let (mut client, server) = (server.0, &mut server.1);
                    let _ = tokio::io::copy_bidirectional(&mut client, server).await;
                }
            });
        }
    });
    let proxy = Proxy {
        kind,
        host: "127.0.0.1".into(),
        port,
        username: String::new(),
        password: String::new(),
        remote_dns: true,
    };
    (proxy, carried)
}

async fn accept_tunnel(kind: ProxyKind, mut client: TcpStream) -> Option<(TcpStream, TcpStream)> {
    let target = match kind {
        ProxyKind::Socks5 => {
            let mut greeting = [0u8; 2];
            client.read_exact(&mut greeting).await.ok()?;
            let mut methods = vec![0u8; usize::from(greeting[1])];
            client.read_exact(&mut methods).await.ok()?;
            client.write_all(&[5, 0]).await.ok()?;
            let mut request = [0u8; 4];
            client.read_exact(&mut request).await.ok()?;
            let host = match request[3] {
                1 => {
                    let mut octets = [0u8; 4];
                    client.read_exact(&mut octets).await.ok()?;
                    std::net::Ipv4Addr::from(octets).to_string()
                }
                3 => {
                    let length = client.read_u8().await.ok()?;
                    let mut name = vec![0u8; usize::from(length)];
                    client.read_exact(&mut name).await.ok()?;
                    String::from_utf8(name).ok()?
                }
                _ => return None,
            };
            let port = client.read_u16().await.ok()?;
            let server = TcpStream::connect((host.as_str(), port)).await.ok()?;
            client
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .ok()?;
            server
        }
        ProxyKind::Http => {
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(client.read_u8().await.ok()?);
            }
            let head = String::from_utf8(head).ok()?;
            let authority = head.strip_prefix("CONNECT ")?.split_whitespace().next()?;
            let server = TcpStream::connect(authority).await.ok()?;
            client
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .ok()?;
            server
        }
        _ => return None,
    };
    Some((client, target))
}

fn routed(
    server: &Server,
    proxy: Option<Proxy>,
    jump_hosts: Vec<ConnectProfile>,
) -> ConnectProfile {
    ConnectProfile {
        route: Route { proxy, jump_hosts },
        ..profile(
            server,
            AuthMethod::Password {
                password: server.password.clone(),
            },
        )
    }
}

async fn list_home(manager: &SessionManager, info: &SessionInfo) {
    let session = manager.get(&info.id).await.unwrap();
    let listing = session.files().list_dir(&info.home).await.unwrap();
    assert_eq!(listing.path, info.home);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connects_through_proxies_and_jump_hosts() {
    let Some(server) = server() else { return };
    let (_known_hosts, manager) = trusted_manager(&server).await;
    let manager = Arc::new(manager);
    let password = || AuthMethod::Password {
        password: server.password.clone(),
    };

    for kind in [ProxyKind::Socks5, ProxyKind::Http] {
        let (proxy, carried) = start_proxy(kind).await;
        let info = manager
            .connect(routed(&server, Some(proxy), Vec::new()), None, OWNER)
            .await
            .unwrap_or_else(|error| panic!("{kind:?}: {error}"));
        list_home(&manager, &info).await;
        manager.disconnect(&info.id).await.unwrap();
        assert_eq!(carried.load(Ordering::SeqCst), 1, "{kind:?}");
    }

    // Through a jump host that is itself reached through the proxy. The jump host tunnels to
    // the same server, which is enough to exercise every hop.
    let (proxy, carried) = start_proxy(ProxyKind::Socks5).await;
    let jump_host = profile(&server, password());
    let info = manager
        .connect(routed(&server, Some(proxy), vec![jump_host]), None, OWNER)
        .await
        .unwrap();
    list_home(&manager, &info).await;
    assert_eq!(carried.load(Ordering::SeqCst), 1);

    // Transfer workers open their own connections along the same route.
    let local = tempfile::tempdir().unwrap();
    let file = local
        .path()
        .join(format!("routed-{}.bin", std::process::id()));
    std::fs::write(&file, pattern(300_000, 9)).unwrap();
    let transfers = TransferManager::new(manager.clone(), Events::default(), transfer_settings());
    transfers
        .enqueue(EnqueueRequest {
            session_id: info.id.clone(),
            direction: Direction::Upload,
            target_directory: info.home.clone(),
            items: vec![TransferItem {
                path: file.to_string_lossy().into_owned(),
                name: file.file_name().unwrap().to_string_lossy().into_owned(),
                is_dir: false,
                size: 300_000,
            }],
            source_session_id: None,
        })
        .await
        .unwrap();
    assert_all_done(&wait_until_settled(&transfers).await.0);
    assert!(carried.load(Ordering::SeqCst) > 1);
    let uploaded = remote_path_join(&info.home, &file.file_name().unwrap().to_string_lossy());
    let session = manager.get(&info.id).await.unwrap();
    assert_eq!(
        session.files().stat(&uploaded).await.unwrap().unwrap().size,
        300_000
    );
    session.files().delete(&[uploaded]).await.unwrap();
    drop(transfers);
    manager.disconnect(&info.id).await.unwrap();

    // A jump host the jump server cannot reach past says which hop failed.
    let unreachable = ConnectProfile {
        port: 1,
        ..routed(&server, None, vec![profile(&server, password())])
    };
    let error = manager.connect(unreachable, None, OWNER).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Connection);
    assert!(
        error.message.contains("could not reach"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn a_jump_host_trusted_once_stays_trusted_until_exit() {
    let Some(server) = server() else { return };
    let temp_dir = tempfile::tempdir().unwrap();
    // No system known_hosts, so the server is unknown even on a developer's machine.
    let known_hosts = poros_lib::ssh::known_hosts::KnownHosts::new(
        temp_dir.path().join("known_hosts"),
        Vec::new(),
    );
    let jumped = || {
        routed(
            &server,
            None,
            vec![profile(
                &server,
                AuthMethod::Password {
                    password: server.password.clone(),
                },
            )],
        )
    };
    let error = poros_lib::ssh::connect("test", &jumped(), &known_hosts, None, &Events::default())
        .await
        .err()
        .expect("an unknown jump host asks first");
    let host_key = error.host_key.expect("the question names the jump host");
    let once = HostKeyApproval {
        fingerprint: host_key.fingerprint,
        remember: false,
    };
    let connection = poros_lib::ssh::connect(
        "test",
        &jumped(),
        &known_hosts,
        Some(once),
        &Events::default(),
    )
    .await
    .unwrap();
    poros_lib::ssh::disconnect(&connection.handle).await;
    // A transfer connection pins only the server's key, yet gets through the jump host.
    let pinned = HostKeyApproval {
        fingerprint: connection.host_key_fingerprint,
        remember: false,
    };
    let worker = poros_lib::ssh::connect(
        "test",
        &jumped(),
        &known_hosts,
        Some(pinned),
        &Events::default(),
    )
    .await
    .unwrap();
    poros_lib::ssh::disconnect(&worker.handle).await;
    assert!(!temp_dir.path().join("known_hosts").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_commands_on_the_server() {
    let Some(server) = server() else { return };
    let fixture = TransferFixture::new(&server, "commands").await;
    let session = fixture.manager.get(&fixture.session.id).await.unwrap();
    let folder = session
        .files()
        .make_dir(&fixture.remote, "it's here")
        .await
        .unwrap();

    let mut stdout = String::new();
    let mut stderr = String::new();
    let result = remote_command::run_on(
        &session,
        "pwd; printf 'one\\ntwo'; printf oops >&2; exit 3",
        Some(&folder),
        &CancellationToken::new(),
        |stream, text| match stream {
            OutputStream::Stdout => stdout.push_str(text),
            OutputStream::Stderr => stderr.push_str(text),
        },
    )
    .await
    .unwrap();
    assert_eq!(stdout, format!("{folder}\none\ntwo"));
    assert_eq!(stderr, "oops");
    assert_eq!(result.exit_status, Some(3));
    assert!(!result.succeeded());

    let stop = CancellationToken::new();
    let stopper = stop.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        stopper.cancel();
    });
    let started = Instant::now();
    let stopped = remote_command::run_on(&session, "sleep 30", None, &stop, |_, _| {})
        .await
        .unwrap();
    assert!(stopped.stopped);
    assert!(started.elapsed() < Duration::from_secs(10));
    fixture.close().await;
}

/// Saved connections in these tests use keys, so no secret is ever stored.
struct NoSecrets;

impl SecretStore for NoSecrets {
    fn get(&self, _account: &str) -> AppResult<Option<String>> {
        Ok(None)
    }
    fn set(&self, _account: &str, _secret: &str) -> AppResult<()> {
        Ok(())
    }
    fn delete(&self, _account: &str) -> AppResult<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduled_tasks_connect_on_their_own() {
    let Some(server) = server() else { return };
    let workspace = TransferFixture::new(&server, "scheduled").await;
    let store_dir = tempfile::tempdir().unwrap();
    let connections = Arc::new(ConnectionStore::new(
        store_dir.path().join("connections.json"),
        Box::new(NoSecrets),
    ));
    let saved = connections
        .save(
            SavedConnection {
                id: String::new(),
                protocol: Protocol::Sftp,
                name: "test server".into(),
                host: "127.0.0.1".into(),
                port: server.port,
                username: server.user.clone(),
                auth_type: AuthType::PublicKey,
                key_path: Some(fixture("openssh-ed25519")),
                remote_path: None,
                save_secret: false,
                last_used: None,
                ftp_active: false,
                bypass_proxy: false,
                jump_connection_id: None,
            },
            None,
        )
        .unwrap();
    let sync = SyncManager::new(workspace.manager.clone(), Events::default());
    let transfers = workspace.transfers(transfer_settings());
    let events = Events::default();
    let context = TaskContext {
        sessions: &workspace.manager,
        connections,
        connection_settings: ConnectionSettings::default(),
        sync: &sync,
        transfers: &transfers,
        events: &events,
    };

    let local = workspace.local.path().join("backup");
    write_tree(&local);
    let upload = TaskAction::Sync {
        connection_id: saved.id.clone(),
        local_path: local.to_string_lossy().into_owned(),
        remote_path: workspace.remote.clone(),
        direction: SyncDirection::Upload,
        compare: CompareMode::SizeAndTime,
        delete_extraneous: false,
        skip_newer_on_target: false,
        ignore_existing: false,
        time_tolerance_secs: 2,
        excludes: Vec::new(),
    };
    let first = scheduler::perform(&upload, &context).await.unwrap();
    assert!(first.succeeded, "{}", first.message);
    assert!(
        first.message.starts_with("5 files copied"),
        "{}",
        first.message
    );
    let again = scheduler::perform(&upload, &context).await.unwrap();
    assert_eq!(again.message, "The folders already match");

    let command = TaskAction::Command {
        connection_id: saved.id.clone(),
        command: "ls | wc -l; echo done-$((1 + 1))".into(),
        directory: Some(workspace.remote.clone()),
    };
    let ran = scheduler::perform(&command, &context).await.unwrap();
    assert!(ran.succeeded);
    assert_eq!(ran.message, "Command finished: done-2");

    let failing = TaskAction::Command {
        connection_id: saved.id,
        command: "echo nope >&2; exit 1".into(),
        directory: None,
    };
    let failed = scheduler::perform(&failing, &context).await.unwrap();
    assert!(!failed.succeeded);
    assert_eq!(failed.message, "Command exited with status 1: nope");
    drop(transfers);
    workspace.close().await;
}
