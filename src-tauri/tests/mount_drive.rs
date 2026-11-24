//! Drives mounted for real over the live test servers and used by ordinary programs. Each
//! server is skipped unless configured, with the variables the other end-to-end suites use
//! (`POROS_TEST_SSH_PORT`, `POROS_TEST_FTP_PORT`, `POROS_TEST_WEBDAV_PORT`,
//! `POROS_TEST_S3_PORT`), and everything is skipped where this computer cannot mount: no
//! `/dev/fuse`, no `fusermount3`, or a kernel that refuses the mount.

#![cfg(target_os = "linux")]

use std::io::Write;
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::cloud::s3::{Addressing, S3Options};
use poros_lib::events::Events;
use poros_lib::model::EntryKind;
use poros_lib::mount::manager::{DriveView, MountManager, MountRequest};
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use sha2::{Digest, Sha256};

const OWNER: &str = "main";
const MEBIBYTE: usize = 1024 * 1024;
const PASSWORD: &str = "a drive's vault p\u{e4}ssword";
const SCRATCH_PREFIX: &str = "poros-mount-drive-";

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
        server("POROS_TEST_WEBDAV_PORT", Protocol::WebDav),
        server("POROS_TEST_S3_PORT", Protocol::S3),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn sftp_server() -> Option<Server> {
    server("POROS_TEST_SSH_PORT", Protocol::Sftp)
}

fn ftp_server() -> Option<Server> {
    server("POROS_TEST_FTP_PORT", Protocol::Ftp)
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

/// Connects, approving the server's host key if the manager asks.
async fn connect(sessions: &SessionManager, server: &Server) -> SessionInfo {
    let profile = profile(server);
    let label = profile.label();
    match sessions.connect(profile.clone(), None, OWNER).await {
        Ok(info) => info,
        Err(error) => {
            let host_key = error
                .host_key
                .unwrap_or_else(|| panic!("{label}: {}", error.message));
            let approval = HostKeyApproval::of(&host_key, true);
            sessions
                .connect(profile, Some(approval), OWNER)
                .await
                .unwrap_or_else(|error| panic!("{label}: {}", error.message))
        }
    }
}

async fn files_of(sessions: &SessionManager, session_id: &str) -> Arc<dyn RemoteFileSystem> {
    sessions.get(session_id).await.unwrap().files()
}

/// Whether drives can be mounted here at all, saying why not.
fn can_mount() -> bool {
    match MountManager::support().missing {
        Some(missing) => {
            eprintln!("Skipped: {}", missing.problem);
            false
        }
        None => true,
    }
}

/// A session, a drive manager and an empty scratch folder (a bucket on S3) on one server.
struct Setup {
    label: &'static str,
    sessions: Arc<SessionManager>,
    mounts: Arc<MountManager>,
    session: SessionInfo,
    files: Arc<dyn RemoteFileSystem>,
    scratch: String,
    /// Holds the known hosts, the mount points and the spool folders.
    local: tempfile::TempDir,
}

impl Setup {
    async fn new(server: &Server, name: &str) -> Self {
        let local = tempfile::tempdir().unwrap();
        let sessions = Arc::new(SessionManager::new(
            local.path().join("known_hosts"),
            Events::default(),
        ));
        let mounts = MountManager::new(
            sessions.clone(),
            Events::default(),
            local.path().join("spool"),
            Arc::new(|| (true, true)),
        );
        let session = connect(&sessions, server).await;
        let files = files_of(&sessions, &session.id).await;
        let prefix = format!("{SCRATCH_PREFIX}{name}-");
        let left: Vec<String> = files
            .list_dir(&session.home)
            .await
            .unwrap()
            .entries
            .into_iter()
            .filter(|entry| entry.name.starts_with(&prefix))
            .map(|entry| entry.path)
            .collect();
        if !left.is_empty() {
            files.delete(&left).await.unwrap();
        }
        let scratch = files
            .make_dir("~", &format!("{prefix}{}", std::process::id()))
            .await
            .unwrap();
        Self {
            label: server.protocol.display_name(),
            sessions,
            mounts,
            session,
            files,
            scratch,
            local,
        }
    }

    fn mount_point(&self, name: &str) -> PathBuf {
        self.local.path().join(name)
    }

    /// Mounts `remote_root` of `session_id`, or `None` where the kernel refuses to mount.
    async fn mount(
        &self,
        session_id: &str,
        remote_root: &str,
        at: &Path,
        read_only: bool,
    ) -> Option<DriveView> {
        let mounted = self
            .mounts
            .mount(MountRequest {
                session_id: session_id.to_string(),
                mount_point: at.to_string_lossy().into_owned(),
                remote_root: remote_root.to_string(),
                read_only,
            })
            .await;
        match mounted {
            Ok(drive) => {
                assert!(
                    is_mounted(at),
                    "{}: {} is mounted",
                    self.label,
                    at.display()
                );
                Some(drive)
            }
            Err(error) if error.message.starts_with("Could not mount at") => {
                eprintln!("Skipped: {}", error.message);
                None
            }
            Err(error) => panic!("{}: {}", self.label, error.message),
        }
    }

    async fn finish(self) {
        self.mounts.unmount_all().await;
        self.files
            .delete(std::slice::from_ref(&self.scratch))
            .await
            .unwrap_or_else(|error| panic!("{}: {}", self.label, error.message));
        self.sessions.disconnect(&self.session.id).await.unwrap();
    }
}

/// Whether the kernel lists a mount at `path`.
fn is_mounted(path: &Path) -> bool {
    let listed = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    // The kernel writes a space, tab, newline or backslash in a path as an octal escape.
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\134")
        .replace(' ', "\\040")
        .replace('\t', "\\011")
        .replace('\n', "\\012");
    listed
        .lines()
        .any(|line| line.split(' ').nth(4) == Some(escaped.as_str()))
}

/// Runs file operations the way another program would: off the threads that serve the drive.
async fn program<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work).await.unwrap()
}

/// Runs a program on this computer, with each argument passed as it is.
fn run(program: &str, arguments: &[&Path]) -> String {
    let output = Command::new(program).args(arguments).output().unwrap();
    assert!(
        output.status.success(),
        "{program}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Deterministic bytes that do not compress, so a misplaced piece shows up.
fn pattern(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

fn sha256_hex(contents: &[u8]) -> String {
    Sha256::digest(contents)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn errno(error: &std::io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
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
    stream
        .write(Bytes::copy_from_slice(contents))
        .await
        .unwrap();
    stream.finish().await.unwrap();
}

async fn until(what: &str, mut holds: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !holds() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ordinary_programs_work_on_a_drive_of_every_server() {
    if !can_mount() {
        return;
    }
    for server in every_server() {
        let setup = Setup::new(&server, "programs").await;
        let label = setup.label;
        let scratch = setup.scratch.clone();
        write_remote(
            &setup.files,
            &remote_path::join(&scratch, "from the server.txt"),
            b"written on the server",
        )
        .await;
        let at = setup.mount_point("drive");
        let Some(drive) = setup.mount(&setup.session.id, &scratch, &at, false).await else {
            setup.finish().await;
            return;
        };
        assert_eq!(drive.remote_root, scratch, "{label}");

        let large = pattern(9 * MEBIBYTE + 4321, 1);
        let mut patched = large.clone();
        patched[1000..1005].copy_from_slice(b"PATCH");
        let local = setup.local.path().join("local");
        std::fs::create_dir(&local).unwrap();
        let outside_copy = local.join("copied out.bin");
        let expected_large = patched.clone();
        program(move || {
            assert_eq!(
                std::fs::read_to_string(at.join("from the server.txt")).unwrap(),
                "written on the server",
                "{label}"
            );

            std::fs::write(local.join("large.bin"), &large).unwrap();
            run("cp", &[&local.join("large.bin"), &at.join("large.bin")]);
            let summed = run("sha256sum", &[&at.join("large.bin")]);
            assert!(summed.starts_with(&sha256_hex(&large)), "{label}: {summed}");
            let mut piece = vec![0; 1000];
            let file = std::fs::File::open(at.join("large.bin")).unwrap();
            file.read_exact_at(&mut piece, 5_000_000).unwrap();
            assert_eq!(piece, large[5_000_000..5_001_000], "{label}");
            drop(file);

            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(at.join("large.bin"))
                .unwrap();
            file.write_all_at(b"PATCH", 1000).unwrap();
            file.sync_all().unwrap();
            drop(file);
            run("cp", &[&at.join("large.bin"), &outside_copy]);
            assert!(
                std::fs::read(&outside_copy).unwrap() == expected_large,
                "{label}"
            );

            // A text editor's save: the new text under another name, then renamed over.
            std::fs::write(at.join("notes.txt"), "first draft").unwrap();
            std::fs::write(at.join(".notes.txt.swp"), "second draft").unwrap();
            std::fs::rename(at.join(".notes.txt.swp"), at.join("notes.txt")).unwrap();
            assert_eq!(
                std::fs::read_to_string(at.join("notes.txt")).unwrap(),
                "second draft",
                "{label}"
            );

            std::fs::create_dir_all(at.join("tree/inner")).unwrap();
            std::fs::write(at.join("tree/inner/deep.txt"), "deep").unwrap();
            std::fs::write(at.join("tree/moving.txt"), "moving").unwrap();
            run("mv", &[&at.join("tree/moving.txt"), &at.join("moved.txt")]);

            let writers: Vec<_> = [("one.bin", 2), ("two.bin", 3)]
                .into_iter()
                .map(|(name, seed)| {
                    let path = at.join(name);
                    std::thread::spawn(move || {
                        let contents = pattern(2 * MEBIBYTE + seed as usize, seed);
                        let mut file = std::fs::File::create(&path).unwrap();
                        for piece in contents.chunks(64 * 1024) {
                            file.write_all(piece).unwrap();
                        }
                        file.sync_all().unwrap();
                        drop(file);
                        assert!(std::fs::read(&path).unwrap() == contents);
                    })
                })
                .collect();
            for writer in writers {
                writer.join().unwrap();
            }

            let listed = run("ls", &[&at]);
            let mut names: Vec<&str> = listed.lines().collect();
            names.sort();
            assert_eq!(
                names,
                [
                    "from the server.txt",
                    "large.bin",
                    "moved.txt",
                    "notes.txt",
                    "one.bin",
                    "tree",
                    "two.bin"
                ],
                "{label}"
            );
            run("rm", &[Path::new("-r"), &at.join("tree")]);
            assert!(!at.join("tree").exists(), "{label}");
        })
        .await;

        assert_eq!(
            names_in(&setup.files, &scratch).await,
            [
                "from the server.txt",
                "large.bin",
                "moved.txt",
                "notes.txt",
                "one.bin",
                "two.bin"
            ],
            "{label}"
        );
        let on_server = read_remote(&setup.files, &remote_path::join(&scratch, "large.bin")).await;
        assert!(
            on_server == patched,
            "{label}: the server holds the patched file"
        );
        assert_eq!(
            read_remote(&setup.files, &remote_path::join(&scratch, "notes.txt")).await,
            b"second draft",
            "{label}"
        );
        assert_eq!(
            read_remote(&setup.files, &remote_path::join(&scratch, "two.bin")).await,
            pattern(2 * MEBIBYTE + 3, 3),
            "{label}"
        );

        let at = setup.mount_point("drive");
        let shown = setup.mounts.list();
        assert_eq!(shown.len(), 1, "{label}");
        assert!(shown[0].bytes_written >= 9 * MEBIBYTE as u64, "{label}");
        setup.mounts.unmount(&drive.id, false).await.unwrap();
        assert!(!is_mounted(&at), "{label}: the drive is gone");
        assert!(!at.exists(), "{label}: the folder made for it is gone");
        assert!(setup.mounts.unsent().is_empty(), "{label}");
        setup.finish().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_vault_on_a_drive_stores_only_encrypted_names_and_goes_when_it_locks() {
    let Some(server) = sftp_server().or_else(|| every_server().into_iter().next()) else {
        return;
    };
    if !can_mount() {
        return;
    }
    let setup = Setup::new(&server, "vault").await;
    let label = setup.label;
    let vault = setup
        .sessions
        .create_vault(
            &setup.session.id,
            &setup.scratch,
            "Vault",
            PASSWORD.to_string(),
            OWNER,
        )
        .await
        .unwrap_or_else(|error| panic!("{label}: {}", error.message));
    let root = remote_path::join(&setup.scratch, "Vault");
    let at = setup.mount_point("vault");
    let Some(drive) = setup.mount(&vault.id, &root, &at, false).await else {
        setup.finish().await;
        return;
    };
    assert_eq!(drive.protocol, Protocol::Cryptomator, "{label}");

    let secret = pattern(3 * MEBIBYTE + 17, 9);
    let written = secret.clone();
    let folder = at.clone();
    program(move || {
        std::fs::create_dir(folder.join("private notes")).unwrap();
        std::fs::write(folder.join("private notes/plan of the year.txt"), &written).unwrap();
        assert!(
            std::fs::read(folder.join("private notes/plan of the year.txt")).unwrap() == written
        );
    })
    .await;

    let vault_files = files_of(&setup.sessions, &vault.id).await;
    let in_vault = remote_path::join(&root, "private notes/plan of the year.txt");
    assert!(
        read_remote(&vault_files, &in_vault).await == secret,
        "{label}"
    );
    for name in stored_names(&setup.files, &remote_path::join(&root, "d")).await {
        let encrypted =
            name.len() == 2 || name.len() == 30 || name.ends_with(".c9r") || name.ends_with(".c9s");
        assert!(encrypted, "{label}: {name} is stored in the vault");
    }

    // Locking the vault ends its drive, as does closing the session of its server.
    setup.mounts.end_for_session(&vault.id).await;
    assert!(setup.mounts.list().is_empty(), "{label}");
    assert!(!is_mounted(&at), "{label}");
    let again = setup.mount(&vault.id, &root, &at, true).await.unwrap();
    assert!(again.read_only, "{label}");
    setup.mounts.end_for_session(&setup.session.id).await;
    assert!(setup.mounts.list().is_empty(), "{label}");
    assert!(!is_mounted(&at), "{label}");
    setup.sessions.disconnect(&vault.id).await.unwrap();
    setup.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unmounting_waits_for_open_files_unless_forced_and_keeps_what_they_wrote() {
    let Some(server) = sftp_server() else {
        return;
    };
    if !can_mount() {
        return;
    }
    let setup = Setup::new(&server, "open").await;
    let label = setup.label;
    let at = setup.mount_point("drive");
    let Some(drive) = setup
        .mount(&setup.session.id, &setup.scratch, &at, false)
        .await
    else {
        setup.finish().await;
        return;
    };
    // Another program holds the file, as an editor would. Not this process: every close of
    // a descriptor reaches the drive as a flush, and the programs this process starts close
    // their copies of its descriptors, which would send the file.
    let mut holder = Command::new("dd")
        .arg(format!("of={}", at.join("draft.md").display()))
        // With a block size, each piece read is written at once rather than gathered.
        .args(["bs=4096", "status=none"])
        .stdin(Stdio::piped())
        // It fails to close the file once the drive is gone, and says so.
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut words = holder.stdin.take().unwrap();
    words.write_all(b"words not saved yet").unwrap();
    until("the program wrote its words", || {
        setup.mounts.list()[0]
            .uploads
            .iter()
            .any(|upload| upload.path == "draft.md" && upload.size == 19)
    })
    .await;

    let shown = setup.mounts.list();
    assert_eq!(shown[0].open_files, ["draft.md"], "{label}");
    let refused = setup.mounts.unmount(&drive.id, false).await.unwrap_err();
    assert!(refused.message.contains("draft.md"), "{}", refused.message);
    assert!(is_mounted(&at), "{label}: still mounted");

    setup.mounts.unmount(&drive.id, true).await.unwrap();
    assert!(!is_mounted(&at), "{label}");
    // The program finds the drive gone when it closes the file.
    drop(words);
    program(move || holder.wait().unwrap()).await;
    let kept = setup.mounts.unsent();
    assert_eq!(kept.len(), 1, "{label}");
    assert_eq!(
        kept[0].note.remote_path,
        remote_path::join(&setup.scratch, "draft.md")
    );
    assert_eq!(
        std::fs::read(&kept[0].path).unwrap(),
        b"words not saved yet"
    );
    let saved = setup.local.path().join("saved draft.md");
    setup
        .mounts
        .save_unsent(&kept[0].path, &saved.to_string_lossy())
        .unwrap();
    assert_eq!(std::fs::read(&saved).unwrap(), b"words not saved yet");
    setup.mounts.discard_unsent(&kept[0].path).unwrap();
    assert!(setup.mounts.unsent().is_empty(), "{label}");
    setup.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_upload_the_server_refuses_reaches_the_program_and_is_sent_when_tried_again() {
    let Some(server) = sftp_server() else {
        return;
    };
    if !can_mount() {
        return;
    }
    let setup = Setup::new(&server, "refused").await;
    let label = setup.label;
    let remote = remote_path::join(&setup.scratch, "locked.txt");
    write_remote(&setup.files, &remote, b"the original").await;
    setup
        .files
        .set_attributes(&remote, None, Some(0o444))
        .await
        .unwrap();
    let at = setup.mount_point("drive");
    let Some(drive) = setup
        .mount(&setup.session.id, &setup.scratch, &at, false)
        .await
    else {
        setup.finish().await;
        return;
    };
    let folder = at.clone();
    program(move || {
        let mode = std::fs::metadata(folder.join("locked.txt"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o444);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(folder.join("locked.txt"))
            .unwrap();
        file.write_all(b"the replacement").unwrap();
        let synced = file.sync_all().unwrap_err();
        assert_eq!(
            errno(&synced),
            libc::EIO,
            "the program hears of the failure"
        );
    })
    .await;

    until("the drive shows the failed upload", || {
        setup.mounts.list()[0]
            .uploads
            .iter()
            .any(|upload| upload.path == "locked.txt" && upload.error.is_some())
    })
    .await;
    assert_eq!(read_remote(&setup.files, &remote).await, b"the original");
    // The close and the release each try once more. Both are over before the file is let
    // write again, so neither can be the attempt that gets through.
    until("the program let go of the file and no attempt runs", || {
        let shown = &setup.mounts.list()[0];
        shown.open_files.is_empty() && shown.uploads.iter().all(|upload| !upload.uploading)
    })
    .await;
    assert_eq!(read_remote(&setup.files, &remote).await, b"the original");
    let refused = setup.mounts.unmount(&drive.id, false).await.unwrap_err();
    assert!(
        refused
            .message
            .starts_with("Not on the server yet: locked.txt"),
        "{}",
        refused.message
    );

    setup
        .files
        .set_attributes(&remote, None, Some(0o644))
        .await
        .unwrap();
    setup.mounts.retry(&drive.id).unwrap();
    until("the upload is sent", || {
        setup.mounts.list()[0].uploads.is_empty()
    })
    .await;
    assert_eq!(read_remote(&setup.files, &remote).await, b"the replacement");
    setup.mounts.unmount(&drive.id, false).await.unwrap();
    assert!(setup.mounts.unsent().is_empty(), "{label}");
    setup.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_the_ftp_server_will_not_delete_stays_where_it_is() {
    let Some(server) = ftp_server() else {
        return;
    };
    if !can_mount() {
        return;
    }
    let setup = Setup::new(&server, "undeletable").await;
    let folder = setup.files.make_dir(&setup.scratch, "ro").await.unwrap();
    let keep = remote_path::join(&folder, "keep.txt");
    write_remote(&setup.files, &keep, b"keep").await;
    let at = setup.mount_point("drive");
    let Some(_drive) = setup
        .mount(&setup.session.id, &setup.scratch, &at, false)
        .await
    else {
        setup.finish().await;
        return;
    };
    // The server answers the refused delete with a bare "550 Delete operation failed."
    setup
        .files
        .set_attributes(&folder, None, Some(0o555))
        .await
        .unwrap();
    let shown = at.join("ro/keep.txt");
    let (removed, still_shown) = program(move || {
        let removed = std::fs::remove_file(&shown).map_err(|error| errno(&error));
        (removed, shown.exists())
    })
    .await;
    // Writable again before anything is asserted, so the folder can always be cleaned up.
    setup
        .files
        .set_attributes(&folder, None, Some(0o755))
        .await
        .unwrap();
    assert_eq!(removed, Err(libc::EACCES));
    assert!(still_shown, "the drive still shows it");
    assert_eq!(read_remote(&setup.files, &keep).await, b"keep");
    setup.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn removing_a_folder_with_links_in_it_removes_only_the_links() {
    let Some(server) = sftp_server() else {
        return;
    };
    if !can_mount() {
        return;
    }
    let setup = Setup::new(&server, "links").await;
    let label = setup.label;
    let outside = remote_path::join(&setup.scratch, "outside");
    let tree = remote_path::join(&setup.scratch, "tree");
    // Parents first: folders are made one at a time.
    for folder in [
        outside.clone(),
        tree.clone(),
        remote_path::join(&tree, "dir"),
        remote_path::join(&tree, "docs"),
    ] {
        setup.files.ensure_dir(&folder).await.unwrap();
    }
    write_remote(
        &setup.files,
        &remote_path::join(&outside, "keep.txt"),
        b"keep",
    )
    .await;
    write_remote(
        &setup.files,
        &remote_path::join(&outside, "two.txt"),
        b"two",
    )
    .await;
    let note = remote_path::join(&tree, "docs/a.txt");
    write_remote(&setup.files, &note, b"alpha").await;
    {
        let session = setup.sessions.get(&setup.session.id).await.unwrap();
        let sftp = session.sftp().unwrap();
        let dir = remote_path::join(&tree, "dir");
        sftp.make_symlink(&remote_path::join(&dir, "link"), "../../outside")
            .await
            .unwrap();
        sftp.make_symlink(&remote_path::join(&dir, "loop"), ".")
            .await
            .unwrap();
        sftp.make_symlink(&remote_path::join(&dir, "note"), &note)
            .await
            .unwrap();
    }
    // The FTP server shows the same account's files, and says in its listing where the
    // links lead.
    if let Some(ftp) = ftp_server() {
        let ftp_session = connect(&setup.sessions, &ftp).await;
        let at = setup.mount_point("ftp");
        let Some(drive) = setup.mount(&ftp_session.id, &tree, &at, false).await else {
            setup.finish().await;
            return;
        };
        program(move || {
            let dir = at.join("dir");
            assert!(
                std::fs::symlink_metadata(dir.join("loop"))
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                "FTP"
            );
            assert_eq!(
                std::fs::read_link(dir.join("note")).unwrap(),
                Path::new("../docs/a.txt"),
                "FTP"
            );
            assert_eq!(
                std::fs::read_to_string(dir.join("note")).unwrap(),
                "alpha",
                "FTP"
            );
            let refused = std::fs::read_link(dir.join("link")).unwrap_err();
            assert_eq!(errno(&refused), libc::EACCES, "FTP: {refused}");
            let found = run("find", &[&dir]);
            assert_eq!(found.lines().count(), 4, "FTP: {found}");
        })
        .await;
        setup.mounts.unmount(&drive.id, false).await.unwrap();
        setup.sessions.disconnect(&ftp_session.id).await.unwrap();
    }

    let at = setup.mount_point("drive");
    let Some(_drive) = setup.mount(&setup.session.id, &tree, &at, false).await else {
        setup.finish().await;
        return;
    };
    program(move || {
        let dir = at.join("dir");
        assert!(
            std::fs::symlink_metadata(dir.join("link"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "{label}"
        );
        assert_eq!(
            std::fs::read_link(dir.join("note")).unwrap(),
            Path::new("../docs/a.txt"),
            "{label}: a full path on the server leads to the same file on the drive"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("note")).unwrap(),
            "alpha",
            "{label}"
        );
        let refused = std::fs::read_link(dir.join("link")).unwrap_err();
        assert_eq!(errno(&refused), libc::EACCES, "{label}: {refused}");
        // A walk lists the links and goes neither round the loop nor out of the drive.
        let found = run("find", &[&dir]);
        assert_eq!(found.lines().count(), 4, "{label}: {found}");
        run("rm", &[Path::new("-r"), &dir]);
        assert!(!dir.exists(), "{label}");
    })
    .await;
    assert_eq!(
        names_in(&setup.files, &outside).await,
        ["keep.txt", "two.txt"],
        "{label}: what the link led to stays"
    );
    assert_eq!(names_in(&setup.files, &tree).await, ["docs"], "{label}");
    assert_eq!(read_remote(&setup.files, &note).await, b"alpha", "{label}");
    setup.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_drive_follows_its_session_through_a_reconnect() {
    let Some(server) = sftp_server() else {
        return;
    };
    if !can_mount() {
        return;
    }
    let setup = Setup::new(&server, "reconnect").await;
    let at = setup.mount_point("drive");
    let Some(drive) = setup
        .mount(&setup.session.id, &setup.scratch, &at, false)
        .await
    else {
        setup.finish().await;
        return;
    };
    let folder = at.clone();
    program(move || std::fs::write(folder.join("before.txt"), "before").unwrap()).await;

    let old = setup.session.id.clone();
    let renewed = setup.sessions.reconnect(&old, None, OWNER).await.unwrap();
    setup.mounts.follow(&old, &renewed.id);
    // What the app does when it closes the session that was replaced.
    setup.mounts.end_for_session(&old).await;
    setup.sessions.disconnect(&old).await.unwrap();
    let shown = setup.mounts.list();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].session_id, renewed.id);

    let folder = at.clone();
    program(move || {
        assert_eq!(
            std::fs::read_to_string(folder.join("before.txt")).unwrap(),
            "before"
        );
        std::fs::write(folder.join("after.txt"), "after").unwrap();
    })
    .await;
    let renewed_files = files_of(&setup.sessions, &renewed.id).await;
    assert_eq!(
        read_remote(
            &renewed_files,
            &remote_path::join(&setup.scratch, "after.txt")
        )
        .await,
        b"after"
    );
    setup.mounts.unmount(&drive.id, false).await.unwrap();
    renewed_files
        .delete(std::slice::from_ref(&setup.scratch))
        .await
        .unwrap();
    setup.sessions.disconnect(&renewed.id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_only_drive_refuses_every_change() {
    let Some(server) = sftp_server() else {
        return;
    };
    if !can_mount() {
        return;
    }
    let setup = Setup::new(&server, "read-only").await;
    write_remote(
        &setup.files,
        &remote_path::join(&setup.scratch, "kept.txt"),
        b"kept as it is",
    )
    .await;
    let at = setup.mount_point("drive");
    let Some(drive) = setup
        .mount(&setup.session.id, &setup.scratch, &at, true)
        .await
    else {
        setup.finish().await;
        return;
    };
    assert!(drive.read_only);
    let folder = at.clone();
    program(move || {
        assert_eq!(
            std::fs::read(folder.join("kept.txt")).unwrap(),
            b"kept as it is"
        );
        let refusals = [
            std::fs::write(folder.join("new.txt"), "new").unwrap_err(),
            std::fs::OpenOptions::new()
                .append(true)
                .open(folder.join("kept.txt"))
                .unwrap_err(),
            std::fs::remove_file(folder.join("kept.txt")).unwrap_err(),
            std::fs::create_dir(folder.join("folder")).unwrap_err(),
            std::fs::rename(folder.join("kept.txt"), folder.join("renamed.txt")).unwrap_err(),
        ];
        for refusal in refusals {
            assert_eq!(errno(&refusal), libc::EROFS, "{refusal}");
        }
    })
    .await;
    assert_eq!(names_in(&setup.files, &setup.scratch).await, ["kept.txt"]);
    setup.finish().await;
}
