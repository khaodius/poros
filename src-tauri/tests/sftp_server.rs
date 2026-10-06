//! End-to-end tests against a real SSH server. Skipped unless `POROS_TEST_SSH_PORT` is set.
//!
//! The server must listen on 127.0.0.1 and accept, for `POROS_TEST_SSH_USER`:
//! the password in `POROS_TEST_SSH_PASSWORD`, and every `tests/fixtures/keys/*.pub` key.
//! The README describes a throwaway `sshd` setup.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::model::{EntryKind, LinkTarget};
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobSnapshot, JobState, TransferItem, TransferList,
    TransferManager,
};

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
    let fs = &session.fs;

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
        let fs = &manager.get(&session.id).await.unwrap().fs;
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
        let fs = &self.manager.get(&self.session.id).await.unwrap().fs;
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
        let fs = &self.manager.get(&self.session.id).await.unwrap().fs;
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
    let kept = std::fs::read(downloads.join("large.bin")).unwrap();
    assert!(!kept.is_empty() && kept.len() < large_bytes.len());
    assert_eq!(kept[..], large_bytes[..kept.len()]);

    transfers.configure(transfer_settings());
    transfers.resume(&[id]);
    assert_all_done(&wait_until_settled(&transfers).await.0);
    assert_eq!(
        std::fs::read(downloads.join("large.bin")).unwrap(),
        large_bytes
    );
    drop(transfers);
    fixture.close().await;
}
