//! Compare folders end to end: what two folders hold below them, compared between this computer
//! and real servers of every kind, between two servers, and inside a vault. The comparison of
//! two local folders always runs; each server is skipped unless configured, with the variables
//! the other end-to-end suites use:
//!
//! - `POROS_TEST_SSH_PORT`: SFTP.
//! - `POROS_TEST_FTP_PORT` and `POROS_TEST_FTPS_PORT`: plain FTP and explicit FTPS.
//! - `POROS_TEST_WEBDAV_PORT`: WebDAV over plain HTTP.
//! - `POROS_TEST_S3_PORT`: an S3-compatible service over plain HTTP that takes any keys.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use poros_lib::cloud::s3::{Addressing, S3Options};
use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::file_ops::Location;
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::sync::{
    FolderCompareRequest, FolderComparison, FolderRef, FolderSummary, SyncManager,
};

const OWNER: &str = "main";
const VAULT_PASSWORD: &str = "a vault's password";
/// A whole minute in the past, as every protocol can keep it.
const BASE_TIME: i64 = 1_700_000_040;

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

fn sftp_server() -> Option<Server> {
    server("POROS_TEST_SSH_PORT", Protocol::Sftp)
}

fn ftp_server() -> Option<Server> {
    server("POROS_TEST_FTP_PORT", Protocol::Ftp)
}

fn webdav_server() -> Option<Server> {
    server("POROS_TEST_WEBDAV_PORT", Protocol::WebDav)
}

fn s3_server() -> Option<Server> {
    server("POROS_TEST_S3_PORT", Protocol::S3)
}

fn every_server() -> Vec<Server> {
    [
        sftp_server(),
        ftp_server(),
        server("POROS_TEST_FTPS_PORT", Protocol::Ftps),
        webdav_server(),
        s3_server(),
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

/// A session with an empty scratch folder (a bucket on S3) in its home folder, named for this
/// suite so nothing else on the server is touched.
async fn open_scratch(
    manager: &SessionManager,
    server: &Server,
    name: &str,
) -> (SessionInfo, Arc<dyn RemoteFileSystem>, String) {
    let session = connect(manager, server).await;
    let files = files_of(manager, &session.id).await;
    let scratch = format!("poros-visual-compare-{name}-{}", std::process::id());
    let existing = remote_path::join(&session.home, &scratch);
    if files.stat(&existing).await.unwrap().is_some() {
        files.delete(std::slice::from_ref(&existing)).await.unwrap();
    }
    let remote = files.make_dir("~", &scratch).await.unwrap();
    (session, files, remote)
}

async fn remove_scratch(files: &Arc<dyn RemoteFileSystem>, scratch: &str) {
    files.delete(&[scratch.to_string()]).await.unwrap();
}

/// One side's files below its root: a path, and the contents of a file or `None` for a folder.
type Tree = [(&'static str, Option<&'static str>)];

/// A file's name, then its size and how many seconds after the base time it was changed, on the
/// left side and on the right.
type TimedFile = (&'static str, (usize, i64), (usize, i64));

/// The left side of every comparison here.
const LEFT: &Tree = &[
    ("same", None),
    ("same/a.txt", Some("aaa")),
    ("same/inner", None),
    ("same/inner/b.txt", Some("bbbb")),
    ("mixed", None),
    ("mixed/changed.txt", Some("a longer text")),
    ("mixed/kept.txt", Some("kept")),
    ("mixed/only-left.txt", Some("left")),
    ("mixed/only-left-folder", None),
    ("mixed/only-left-folder/one", Some("1")),
    ("mixed/only-left-folder/two", Some("2")),
    ("empty", None),
    (".cache", None),
    (".cache/left-only.tmp", Some("tmp")),
    ("top.txt", Some("top")),
];

/// The right side: the same folders, with one file changed, one missing and one more.
const RIGHT: &Tree = &[
    ("same", None),
    ("same/a.txt", Some("aaa")),
    ("same/inner", None),
    ("same/inner/b.txt", Some("bbbb")),
    ("mixed", None),
    ("mixed/changed.txt", Some("short")),
    ("mixed/kept.txt", Some("kept")),
    ("mixed/only-right.txt", Some("right")),
    ("empty", None),
    (".cache", None),
    ("top.txt", Some("top")),
];

fn set_local_time(path: &Path, seconds: i64) {
    let time = UNIX_EPOCH + Duration::from_secs(seconds as u64);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

fn write_local(root: &Path, tree: &Tree) {
    for (path, contents) in tree {
        let target = root.join(path);
        match contents {
            None => std::fs::create_dir_all(&target).unwrap(),
            Some(contents) => {
                std::fs::write(&target, contents).unwrap();
                set_local_time(&target, BASE_TIME);
            }
        }
    }
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
    stream
        .write(Bytes::copy_from_slice(contents))
        .await
        .unwrap();
    stream.finish().await.unwrap();
    // Some protocols take the time with the upload and others only afterwards, as transfers
    // set it; servers that keep no times refuse, and the comparisons by size do not need them.
    if modified.is_some() {
        let _ = files.set_attributes(path, modified, None).await;
    }
}

async fn write_server(files: &Arc<dyn RemoteFileSystem>, root: &str, tree: &Tree) {
    for (path, contents) in tree {
        let target = remote_path::join(root, path);
        match contents {
            None => files.ensure_dir(&target).await.unwrap(),
            Some(contents) => {
                write_remote(files, &target, contents.as_bytes(), Some(BASE_TIME)).await
            }
        }
    }
}

fn local_folder(path: &Path) -> FolderRef {
    FolderRef {
        location: Location::Local,
        path: path.to_string_lossy().into_owned(),
    }
}

fn server_folder(session: &SessionInfo, path: &str) -> FolderRef {
    FolderRef {
        location: Location::Remote {
            session_id: session.id.clone(),
        },
        path: path.to_string(),
    }
}

/// A comparison by size alone, which holds on every server whatever it does to times.
fn by_size(left: FolderRef, right: FolderRef, include_hidden: bool) -> FolderCompareRequest {
    FolderCompareRequest {
        request_id: format!("compare-{}", next_request()),
        left,
        right,
        size_only: true,
        time_tolerance_secs: 2,
        ignore_hour_offsets: false,
        include_hidden,
        ignore_case: false,
    }
}

fn next_request() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn summary<'a>(found: &'a FolderComparison, path: &str) -> &'a FolderSummary {
    found
        .folders
        .iter()
        .find(|folder| folder.path == path)
        .unwrap_or_else(|| panic!("no folder {path} among {:?}", paths(found)))
}

fn paths(found: &FolderComparison) -> Vec<&str> {
    found
        .folders
        .iter()
        .map(|folder| folder.path.as_str())
        .collect()
}

/// A file of another size is newer on one side, or different when the times are the same.
fn changed(folder: &FolderSummary) -> u64 {
    folder.newer_left + folder.newer_right + folder.different
}

/// Checks a comparison of `LEFT` with `RIGHT`, or of `RIGHT` with `LEFT` when `mirrored`.
fn assert_trees_compared(label: &str, found: &FolderComparison, hidden: bool, mirrored: bool) {
    let expected: &[&str] = if hidden {
        &[".cache", "empty", "mixed", "same", "same/inner"]
    } else {
        &["empty", "mixed", "same", "same/inner"]
    };
    assert_eq!(paths(found), expected, "{label}");
    assert!(!found.truncated, "{label}");
    let only = |folder: &FolderSummary| {
        if mirrored {
            (folder.only_right, folder.only_left)
        } else {
            (folder.only_left, folder.only_right)
        }
    };

    let mixed = summary(found, "mixed");
    assert_eq!(mixed.identical, 1, "{label}");
    // The file, and the two files in the folder only the left side has.
    assert_eq!(only(mixed), (3, 1), "{label}");
    assert_eq!(changed(mixed), 1, "{label}");
    assert_eq!(mixed.not_compared, 0, "{label}");

    let same = summary(found, "same");
    assert_eq!((same.identical, changed(same)), (2, 0), "{label}");
    assert_eq!(only(same), (0, 0), "{label}");
    assert_eq!(summary(found, "same/inner").identical, 1, "{label}");
    assert_eq!(
        *summary(found, "empty"),
        FolderSummary {
            path: "empty".into(),
            ..FolderSummary::default()
        },
        "{label}"
    );
    if hidden {
        let cache = summary(found, ".cache");
        assert_eq!((cache.identical, only(cache)), (0, (1, 0)), "{label}");
    }
}

#[tokio::test]
async fn two_local_folders_are_compared_without_any_server() {
    let (_config, manager) = new_manager();
    let sync = SyncManager::new(manager, Events::default());
    let temp = tempfile::tempdir().unwrap();
    let (left, right) = (temp.path().join("left"), temp.path().join("right"));
    write_local(&left, LEFT);
    write_local(&right, RIGHT);

    let visible = sync
        .compare_folders(by_size(local_folder(&left), local_folder(&right), false))
        .await
        .unwrap();
    assert_trees_compared("local", &visible, false, false);
    let all = sync
        .compare_folders(by_size(local_folder(&left), local_folder(&right), true))
        .await
        .unwrap();
    assert_trees_compared("local with hidden", &all, true, false);
    assert!(all.left_entries >= LEFT.len() as u64);
    assert!(all.right_entries >= RIGHT.len() as u64);

    let mirrored = sync
        .compare_folders(by_size(local_folder(&right), local_folder(&left), true))
        .await
        .unwrap();
    assert_trees_compared("local mirrored", &mirrored, true, true);
}

#[tokio::test]
async fn times_decide_by_the_tolerance_and_whole_hours_as_the_rows_do() {
    let (_config, manager) = new_manager();
    let sync = SyncManager::new(manager, Events::default());
    let temp = tempfile::tempdir().unwrap();
    let (left, right) = (temp.path().join("left"), temp.path().join("right"));
    let files: &[TimedFile] = &[
        ("same.txt", (4, 0), (4, 0)),
        ("close.txt", (4, 2), (4, 0)),
        ("newer-left.txt", (4, 100), (4, 0)),
        ("newer-right.txt", (4, 0), (6, 100)),
        ("tie.txt", (4, 1), (5, 0)),
        ("zone.txt", (4, 7201), (4, 0)),
        ("zone-behind.txt", (4, 0), (4, 3600)),
    ];
    for (root, side) in [(&left, 0), (&right, 1)] {
        std::fs::create_dir_all(root.join("times")).unwrap();
        for (name, on_left, on_right) in files {
            let (size, offset) = if side == 0 { *on_left } else { *on_right };
            let path = root.join("times").join(name);
            std::fs::write(&path, vec![b'x'; size]).unwrap();
            set_local_time(&path, BASE_TIME + offset);
        }
    }
    let request =
        |ignore_hour_offsets: bool, size_only: bool, tolerance: u32| FolderCompareRequest {
            size_only,
            ignore_hour_offsets,
            time_tolerance_secs: tolerance,
            ..by_size(local_folder(&left), local_folder(&right), true)
        };

    let plain = sync
        .compare_folders(request(false, false, 2))
        .await
        .unwrap();
    assert_eq!(
        *summary(&plain, "times"),
        FolderSummary {
            path: "times".into(),
            identical: 2,
            newer_left: 2,
            newer_right: 2,
            different: 1,
            ..FolderSummary::default()
        }
    );

    let zones = sync.compare_folders(request(true, false, 2)).await.unwrap();
    assert_eq!(
        *summary(&zones, "times"),
        FolderSummary {
            path: "times".into(),
            identical: 4,
            newer_left: 1,
            newer_right: 1,
            different: 1,
            ..FolderSummary::default()
        }
    );

    // Without a tolerance, two seconds apart is newer.
    let exact = sync
        .compare_folders(request(false, false, 0))
        .await
        .unwrap();
    assert_eq!(summary(&exact, "times").identical, 1);
    assert_eq!(summary(&exact, "times").newer_left, 4);

    // By size alone, only the two files of another size differ.
    let sizes = sync.compare_folders(request(false, true, 2)).await.unwrap();
    assert_eq!(summary(&sizes, "times").identical, 5);
    assert_eq!(changed(summary(&sizes, "times")), 2);
}

#[tokio::test]
async fn names_differing_in_case_match_when_asked() {
    let (_config, manager) = new_manager();
    let sync = SyncManager::new(manager, Events::default());
    let temp = tempfile::tempdir().unwrap();
    let (left, right) = (temp.path().join("left"), temp.path().join("right"));
    write_local(
        &left,
        &[("Docs", None), ("Docs/Read Me.TXT", Some("hello"))],
    );
    write_local(
        &right,
        &[("docs", None), ("docs/read me.txt", Some("hello"))],
    );

    let exact = sync
        .compare_folders(by_size(local_folder(&left), local_folder(&right), true))
        .await
        .unwrap();
    assert!(exact.folders.is_empty());
    let folded = sync
        .compare_folders(FolderCompareRequest {
            ignore_case: true,
            ..by_size(local_folder(&left), local_folder(&right), true)
        })
        .await
        .unwrap();
    assert_eq!(paths(&folded), ["docs"]);
    assert_eq!(summary(&folded, "docs").identical, 1);
}

#[tokio::test]
async fn a_folder_that_is_not_there_fails_plainly() {
    let (_config, manager) = new_manager();
    let sync = SyncManager::new(manager, Events::default());
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let error = sync
        .compare_folders(by_size(
            local_folder(temp.path()),
            local_folder(&missing),
            true,
        ))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::NotFound, "{}", error.message);

    let empty = FolderRef {
        path: "  ".into(),
        ..local_folder(temp.path())
    };
    let error = sync
        .compare_folders(by_size(local_folder(temp.path()), empty, true))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidInput);

    let unknown = FolderRef {
        location: Location::Remote {
            session_id: "no-such-session".into(),
        },
        path: "/".into(),
    };
    assert!(sync
        .compare_folders(by_size(local_folder(temp.path()), unknown, true))
        .await
        .is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_comparison_stops_when_cancelled() {
    let (_config, manager) = new_manager();
    let sync = Arc::new(SyncManager::new(manager, Events::default()));
    let temp = tempfile::tempdir().unwrap();
    // Enough folders that listing them outlasts the moment it takes to cancel.
    for outer in 0..60 {
        for inner in 0..40 {
            let folder = temp.path().join(format!("left/{outer}/{inner}"));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("file"), b"x").unwrap();
        }
    }
    std::fs::create_dir_all(temp.path().join("right")).unwrap();
    let request = by_size(
        local_folder(&temp.path().join("left")),
        local_folder(&temp.path().join("right")),
        true,
    );
    let request_id = request.request_id.clone();
    let comparing = tokio::spawn({
        let sync = sync.clone();
        async move { sync.compare_folders(request).await }
    });
    while !comparing.is_finished() {
        sync.cancel(&request_id);
        tokio::task::yield_now().await;
    }
    let error = comparing.await.unwrap().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
}

#[tokio::test]
async fn a_local_folder_compares_with_a_folder_on_every_kind_of_server() {
    for server in every_server() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let sync = SyncManager::new(manager.clone(), Events::default());
        let (session, files, scratch) = open_scratch(&manager, &server, "local").await;
        if !matches!(server.protocol, Protocol::Ftp | Protocol::Ftps) {
            assert!(!session.coarse_times, "{label}");
        }
        let temp = tempfile::tempdir().unwrap();
        write_local(temp.path(), LEFT);
        write_server(&files, &scratch, RIGHT).await;

        for hidden in [false, true] {
            let found = sync
                .compare_folders(by_size(
                    local_folder(temp.path()),
                    server_folder(&session, &scratch),
                    hidden,
                ))
                .await
                .unwrap_or_else(|error| panic!("{label}: {}", error.message));
            assert_trees_compared(label, &found, hidden, false);
        }
        let mirrored = sync
            .compare_folders(by_size(
                server_folder(&session, &scratch),
                local_folder(temp.path()),
                true,
            ))
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_trees_compared(label, &mirrored, true, true);

        // A folder below the one compared is a comparison's root as well as any other.
        let below = sync
            .compare_folders(by_size(
                local_folder(&temp.path().join("same")),
                server_folder(&session, &remote_path::join(&scratch, "same")),
                true,
            ))
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_eq!(paths(&below), ["inner"], "{label}");

        remove_scratch(&files, &scratch).await;
        manager.disconnect(&session.id).await.unwrap();
    }
}

#[tokio::test]
async fn times_kept_by_an_sftp_server_compare_with_local_ones() {
    let Some(server) = sftp_server() else { return };
    let (_config, manager) = new_manager();
    let sync = SyncManager::new(manager.clone(), Events::default());
    let (session, files, scratch) = open_scratch(&manager, &server, "times").await;
    let temp = tempfile::tempdir().unwrap();
    let folder = remote_path::join(&scratch, "times");
    files.ensure_dir(&folder).await.unwrap();
    std::fs::create_dir_all(temp.path().join("times")).unwrap();
    // On a whole minute on both sides, as the one entry at the top of the comparison.
    let top = temp.path().join("top.txt");
    std::fs::write(&top, b"top").unwrap();
    set_local_time(&top, BASE_TIME);
    let top_there = remote_path::join(&scratch, "top.txt");
    write_remote(&files, &top_there, b"top", Some(BASE_TIME)).await;
    // Here on the left, on the server on the right.
    let cases: &[TimedFile] = &[
        ("same.txt", (4, 0), (4, 0)),
        ("close.txt", (4, 1), (4, 0)),
        ("newer-here.txt", (9, 500), (4, 0)),
        ("newer-there.txt", (4, 0), (4, 500)),
        ("tie.txt", (4, 0), (7, 0)),
        ("zone.txt", (4, 0), (4, 2 * 3600)),
    ];
    for (name, here, there) in cases {
        let path = temp.path().join("times").join(name);
        std::fs::write(&path, vec![b'x'; here.0]).unwrap();
        set_local_time(&path, BASE_TIME + here.1);
        write_remote(
            &files,
            &remote_path::join(&folder, name),
            &vec![b'y'; there.0],
            Some(BASE_TIME + there.1),
        )
        .await;
    }
    let request = |ignore_hour_offsets: bool| FolderCompareRequest {
        size_only: false,
        ignore_hour_offsets,
        ..by_size(
            local_folder(temp.path()),
            server_folder(&session, &scratch),
            true,
        )
    };

    let plain = sync.compare_folders(request(false)).await.unwrap();
    assert_eq!(
        *summary(&plain, "times"),
        FolderSummary {
            path: "times".into(),
            identical: 2,
            newer_left: 1,
            newer_right: 2,
            different: 1,
            ..FolderSummary::default()
        }
    );
    let zones = sync.compare_folders(request(true)).await.unwrap();
    assert_eq!(summary(&zones, "times").identical, 3);
    assert_eq!(summary(&zones, "times").newer_right, 1);

    // The server keeps seconds, so times on whole minutes, as every one here is, are taken as
    // they are: seconds apart below them still make a file newer.
    assert!(!session.coarse_times);
    let late = temp.path().join("times/close.txt");
    set_local_time(&late, BASE_TIME + 45);
    let exact = sync.compare_folders(request(false)).await.unwrap();
    assert_eq!(summary(&exact, "times").newer_left, 2);

    remove_scratch(&files, &scratch).await;
    manager.disconnect(&session.id).await.unwrap();
}

#[tokio::test]
async fn times_an_ftp_server_lists_to_the_minute_are_allowed_for() {
    let Some(server) = ftp_server() else { return };
    let (_config, manager) = new_manager();
    let sync = SyncManager::new(manager.clone(), Events::default());
    let (session, files, scratch) = open_scratch(&manager, &server, "ftp-times").await;
    // The server of this suite has no `MLSD`, so its listings come from `LIST`: to the minute,
    // and to the day for older files. The session says so, and the comparison needs no telling.
    assert!(session.coarse_times);
    let temp = tempfile::tempdir().unwrap();
    let folder = remote_path::join(&scratch, "times");
    files.ensure_dir(&folder).await.unwrap();
    std::fs::create_dir_all(temp.path().join("times")).unwrap();
    for name in ["close.txt", "later.txt"] {
        let path = remote_path::join(&folder, name);
        write_remote(&files, &path, b"four", Some(BASE_TIME)).await;
    }
    // Times here follow the ones listed there, whether the server kept the time it was given.
    for entry in files.list_dir(&folder).await.unwrap().entries {
        let listed = entry.modified.expect("a listed time");
        let offset = if entry.name == "close.txt" {
            45
        } else {
            3 * 86_400
        };
        let path = temp.path().join("times").join(&entry.name);
        std::fs::write(&path, b"four").unwrap();
        set_local_time(&path, listed + offset);
    }
    let found = sync
        .compare_folders(FolderCompareRequest {
            size_only: false,
            ..by_size(
                local_folder(temp.path()),
                server_folder(&session, &scratch),
                true,
            )
        })
        .await
        .unwrap();
    assert_eq!(
        *summary(&found, "times"),
        FolderSummary {
            path: "times".into(),
            identical: 1,
            newer_left: 1,
            ..FolderSummary::default()
        }
    );

    remove_scratch(&files, &scratch).await;
    manager.disconnect(&session.id).await.unwrap();
}

#[tokio::test]
async fn folders_on_two_servers_compare_with_each_other() {
    let pairs = [
        (ftp_server(), sftp_server()),
        (webdav_server(), s3_server()),
        (sftp_server(), webdav_server()),
    ];
    for (left, right) in pairs {
        let (Some(left), Some(right)) = (left, right) else {
            continue;
        };
        let label = format!(
            "{} and {}",
            left.protocol.display_name(),
            right.protocol.display_name()
        );
        let (_config, manager) = new_manager();
        let sync = SyncManager::new(manager.clone(), Events::default());
        let (left_session, left_files, left_scratch) =
            open_scratch(&manager, &left, "pair-left").await;
        let (right_session, right_files, right_scratch) =
            open_scratch(&manager, &right, "pair-right").await;
        write_server(&left_files, &left_scratch, LEFT).await;
        write_server(&right_files, &right_scratch, RIGHT).await;

        let found = sync
            .compare_folders(by_size(
                server_folder(&left_session, &left_scratch),
                server_folder(&right_session, &right_scratch),
                true,
            ))
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_trees_compared(&label, &found, true, false);

        remove_scratch(&left_files, &left_scratch).await;
        remove_scratch(&right_files, &right_scratch).await;
        manager.disconnect(&left_session.id).await.unwrap();
        manager.disconnect(&right_session.id).await.unwrap();
    }
}

#[tokio::test]
async fn a_vault_is_compared_by_the_files_it_decrypts() {
    for server in [sftp_server(), webdav_server()].into_iter().flatten() {
        let label = server.protocol.display_name();
        let (_config, manager) = new_manager();
        let sync = SyncManager::new(manager.clone(), Events::default());
        let (session, files, scratch) = open_scratch(&manager, &server, "vault").await;
        let vault = manager
            .create_vault(
                &session.id,
                &scratch,
                "Vault",
                VAULT_PASSWORD.to_string(),
                OWNER,
            )
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        let root = remote_path::join(&scratch, "Vault");
        let vault_files = files_of(&manager, &vault.id).await;
        write_server(&vault_files, &root, RIGHT).await;
        let temp = tempfile::tempdir().unwrap();
        write_local(temp.path(), LEFT);

        let found = sync
            .compare_folders(by_size(
                local_folder(temp.path()),
                server_folder(&vault, &root),
                true,
            ))
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert_trees_compared(label, &found, true, false);

        // The server's own listing of the vault shows none of those names.
        let encrypted = sync
            .compare_folders(by_size(
                local_folder(temp.path()),
                server_folder(&session, &root),
                true,
            ))
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert!(encrypted.folders.is_empty(), "{label}");

        remove_scratch(&files, &scratch).await;
        manager.disconnect(&session.id).await.unwrap();
    }
}

#[tokio::test]
async fn folders_nested_too_deep_on_a_server_are_not_compared() {
    let Some(server) = [webdav_server(), ftp_server(), s3_server()]
        .into_iter()
        .flatten()
        .next()
    else {
        return;
    };
    // One past where a server's listing stops, and two more below it.
    const LISTED_DEPTH: usize = 63;
    let (_config, manager) = new_manager();
    let sync = SyncManager::new(manager.clone(), Events::default());
    let (session, files, scratch) = open_scratch(&manager, &server, "deep").await;
    let temp = tempfile::tempdir().unwrap();
    let nested = vec!["n"; LISTED_DEPTH + 3].join("/");
    std::fs::create_dir_all(temp.path().join(&nested)).unwrap();
    let mut folder = scratch.clone();
    for _ in 0..LISTED_DEPTH + 3 {
        folder = remote_path::join(&folder, "n");
        files.ensure_dir(&folder).await.unwrap();
    }

    let found = sync
        .compare_folders(by_size(
            local_folder(temp.path()),
            server_folder(&session, &scratch),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(found.folders.len(), LISTED_DEPTH);
    assert_eq!(found.folders[0].path, "n");
    // The folder past the last one listed is not compared, and every folder above it says so.
    assert!(found.folders.iter().all(|folder| folder.not_compared == 1));

    remove_scratch(&files, &scratch).await;
    manager.disconnect(&session.id).await.unwrap();
}

#[test]
fn the_base_time_is_in_the_past() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!((BASE_TIME as u64) < now);
    assert_eq!(BASE_TIME % 60, 0);
}
