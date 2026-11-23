//! Renaming many items in one call, end to end: on this computer, on real servers of every kind,
//! and inside a Cryptomator vault on each of them. Each server is skipped unless configured,
//! with the variables the other end-to-end suites use:
//!
//! - `POROS_TEST_SSH_PORT`: SFTP.
//! - `POROS_TEST_FTP_PORT` and `POROS_TEST_FTPS_PORT`: plain FTP and explicit FTPS.
//! - `POROS_TEST_WEBDAV_PORT`: WebDAV over plain HTTP.
//! - `POROS_TEST_S3_PORT`: an S3-compatible service over plain HTTP that takes any keys.
//!
//! The requests are the ones the page plans (see `planRenames` in `src/lib/bulkRename.ts`):
//! names that trade places go through a temporary name, chains run from their free end.

use std::sync::Arc;

use bytes::Bytes;
use poros_lib::cloud::s3::{Addressing, S3Options};
use poros_lib::events::Events;
use poros_lib::file_ops::{
    FileOperations, Location, Rename, RenameGroup, RenameRequest, RenameSummary,
};
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};

const OWNER: &str = "main";
const PASSWORD: &str = "a vault's password";
const TEMPORARY: &str = "poros-rename-0a1b2c3d-1.tmp";
/// Option-like, with what a shell would expand, quotes and spaces: a name is never a command.
const AWKWARD: &str = "--help $HOME 'q' \u{2713}.txt";
const ACCENTED: &str = "\u{e1}lbum d'\u{e9}t\u{e9}";

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

fn every_server() -> Vec<Server> {
    [
        sftp_server(),
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

/// A folder to rename in, on a server or in a vault on one, removed again by `close`.
struct Place {
    label: String,
    _config: tempfile::TempDir,
    manager: Arc<SessionManager>,
    /// The session the folder is listed through: the server's, or the vault's.
    session_id: String,
    files: Arc<dyn RemoteFileSystem>,
    folder: String,
    /// The scratch folder on the server (a bucket on S3), which holds everything.
    scratch: String,
    server_files: Arc<dyn RemoteFileSystem>,
}

impl Place {
    /// An empty scratch folder in the account's home folder, and with `in_vault` a new vault
    /// in it, whose top folder is then the place.
    async fn open(server: &Server, name: &str, in_vault: bool) -> Self {
        let config = tempfile::tempdir().unwrap();
        let manager = Arc::new(SessionManager::new(
            config.path().join("known_hosts"),
            Events::default(),
        ));
        let session = connect(&manager, server).await;
        let server_files = manager.get(&session.id).await.unwrap().files();
        let scratch_name = format!("poros-bulk-rename-{name}-{}", std::process::id());
        let existing = remote_path::join(&session.home, &scratch_name);
        if server_files.stat(&existing).await.unwrap().is_some() {
            server_files
                .delete(std::slice::from_ref(&existing))
                .await
                .unwrap();
        }
        let scratch = server_files.make_dir("~", &scratch_name).await.unwrap();
        let protocol = server.protocol.display_name();
        if !in_vault {
            return Self {
                label: protocol.to_string(),
                _config: config,
                manager,
                session_id: session.id,
                files: server_files.clone(),
                folder: scratch.clone(),
                scratch,
                server_files,
            };
        }
        let vault = manager
            .create_vault(&session.id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
            .await
            .unwrap_or_else(|error| panic!("{protocol}: {}", error.message));
        let files = manager.get(&vault.id).await.unwrap().files();
        Self {
            label: format!("a vault over {protocol}"),
            _config: config,
            manager,
            session_id: vault.id,
            files,
            folder: remote_path::join(&scratch, "Vault"),
            scratch,
            server_files,
        }
    }

    fn operations(&self) -> FileOperations {
        FileOperations::new(self.manager.clone(), Events::default())
    }

    fn location(&self) -> Location {
        Location::Remote {
            session_id: self.session_id.clone(),
        }
    }

    fn path(&self, name: &str) -> String {
        remote_path::join(&self.folder, name)
    }

    async fn write(&self, name: &str, contents: &str) {
        let mut stream = self
            .files
            .clone()
            .open_write(WriteRequest {
                path: self.path(name),
                offset: 0,
                size: contents.len() as u64,
                modified: None,
                permissions: None,
            })
            .await
            .unwrap_or_else(|error| panic!("{}: {name}: {}", self.label, error.message));
        stream
            .write(Bytes::copy_from_slice(contents.as_bytes()))
            .await
            .unwrap();
        stream.finish().await.unwrap();
    }

    async fn read(&self, path: &str) -> String {
        let mut stream = self
            .files
            .clone()
            .open_read(path, 0..u64::MAX)
            .await
            .unwrap_or_else(|error| panic!("{}: {path}: {}", self.label, error.message));
        let mut contents = Vec::new();
        while let Some(chunk) = stream.next_chunk().await.unwrap() {
            contents.extend_from_slice(&chunk);
        }
        stream.finish().await.unwrap();
        String::from_utf8(contents).unwrap()
    }

    /// Every name in the folder with what the file holds, or `None` for a folder, in order.
    async fn contents(&self) -> Vec<(String, Option<String>)> {
        let mut entries = self.files.list_dir(&self.folder).await.unwrap().entries;
        entries.sort_by(|first, second| first.name.cmp(&second.name));
        let mut contents = Vec::new();
        for entry in entries {
            let held = match entry.kind {
                poros_lib::model::EntryKind::Dir => None,
                _ => Some(self.read(&entry.path).await),
            };
            contents.push((entry.name, held));
        }
        contents
    }

    async fn rename(&self, groups: Vec<RenameGroup>) -> RenameSummary {
        self.operations()
            .rename(RenameRequest {
                operation_id: format!("rename-{}", self.label),
                location: self.location(),
                directory: self.folder.clone(),
                groups,
            })
            .await
            .unwrap_or_else(|error| panic!("{}: {}", self.label, error.message))
    }

    /// Removes everything the test made on the server.
    async fn close(self) {
        self.server_files
            .delete(std::slice::from_ref(&self.scratch))
            .await
            .unwrap_or_else(|error| panic!("{}: {}", self.label, error.message));
        self.manager.disconnect_all().await;
    }
}

fn rename(from: &str, to: &str) -> Rename {
    Rename {
        from: from.to_string(),
        to: to.to_string(),
    }
}

fn single(from: &str, to: &str) -> RenameGroup {
    RenameGroup {
        temporary: None,
        renames: vec![rename(from, to)],
    }
}

fn ring(temporary: &str, renames: &[(&str, &str)]) -> RenameGroup {
    RenameGroup {
        temporary: Some(temporary.to_string()),
        renames: renames.iter().map(|(from, to)| rename(from, to)).collect(),
    }
}

fn held(names: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
    let mut held: Vec<_> = names
        .iter()
        .map(|(name, contents)| (name.to_string(), contents.map(str::to_string)))
        .collect();
    held.sort();
    held
}

fn renamed_names(summary: &RenameSummary) -> Vec<(&str, &str)> {
    summary
        .renamed
        .iter()
        .map(|renamed| (renamed.from.as_str(), renamed.to.as_str()))
        .collect()
}

/// A swap, a ring of three, a chain, a change of case alone, a folder, and names a shell or a
/// command line would trip over, all in one call.
async fn renames_a_set_with_a_swap(place: &Place) {
    let label = &place.label;
    for (name, contents) in [
        ("a.txt", "was a"),
        ("b.txt", "was b"),
        ("one.jpg", "was one"),
        ("two.jpg", "was two"),
        ("three.jpg", "was three"),
        ("IMG_1.jpg", "first"),
        ("IMG_2.jpg", "second"),
        ("IMG_3.jpg", "third"),
        ("readme.md", "read me"),
        ("plain.txt", "plain"),
        ("keep.txt", "kept"),
    ] {
        place.write(name, contents).await;
    }
    place.files.make_dir(&place.folder, "album").await.unwrap();
    place.write("album/inside.txt", "inside the album").await;

    let summary = place
        .rename(vec![
            ring(TEMPORARY, &[("a.txt", "b.txt"), ("b.txt", "a.txt")]),
            ring(
                "poros-rename-0a1b2c3d-2.tmp",
                &[
                    ("one.jpg", "two.jpg"),
                    ("three.jpg", "one.jpg"),
                    ("two.jpg", "three.jpg"),
                ],
            ),
            // Each name is free once the rename before it has run.
            single("IMG_3.jpg", "IMG_4.jpg"),
            single("IMG_2.jpg", "IMG_3.jpg"),
            single("IMG_1.jpg", "IMG_2.jpg"),
            single("readme.md", "README.md"),
            single("plain.txt", AWKWARD),
            single("album", ACCENTED),
        ])
        .await;

    assert_eq!(summary.failure, None, "{label}");
    assert_eq!(summary.stranded, None, "{label}");
    assert!(!summary.cancelled, "{label}");
    assert_eq!(
        renamed_names(&summary),
        [
            ("b.txt", "a.txt"),
            ("a.txt", "b.txt"),
            ("three.jpg", "one.jpg"),
            ("two.jpg", "three.jpg"),
            ("one.jpg", "two.jpg"),
            ("IMG_3.jpg", "IMG_4.jpg"),
            ("IMG_2.jpg", "IMG_3.jpg"),
            ("IMG_1.jpg", "IMG_2.jpg"),
            ("readme.md", "README.md"),
            ("plain.txt", AWKWARD),
            ("album", ACCENTED),
        ],
        "{label}"
    );
    for renamed in &summary.renamed {
        assert_eq!(renamed.path, place.path(&renamed.to), "{label}");
    }
    assert_eq!(
        place.contents().await,
        held(&[
            ("a.txt", Some("was b")),
            ("b.txt", Some("was a")),
            ("one.jpg", Some("was three")),
            ("two.jpg", Some("was one")),
            ("three.jpg", Some("was two")),
            ("IMG_2.jpg", Some("first")),
            ("IMG_3.jpg", Some("second")),
            ("IMG_4.jpg", Some("third")),
            ("README.md", Some("read me")),
            (AWKWARD, Some("plain")),
            ("keep.txt", Some("kept")),
            (ACCENTED, None),
        ]),
        "{label}: nothing is lost, overwritten or left under a temporary name"
    );
    let inside = remote_path::join(&place.path(ACCENTED), "inside.txt");
    assert_eq!(place.read(&inside).await, "inside the album", "{label}");
}

/// A swap whose last step finds its name taken is undone, the renames before it stay, and the
/// ones after it never run. Nothing is overwritten.
async fn stops_and_puts_names_back(place: &Place) {
    let label = &place.label;
    for (name, contents) in [
        ("first.txt", "first"),
        ("left.txt", "left"),
        ("right.txt", "right"),
        ("taken.txt", "not to be replaced"),
        ("last.txt", "last"),
    ] {
        place.write(name, contents).await;
    }

    let summary = place
        .rename(vec![
            single("first.txt", "1.txt"),
            // left.txt waits under the temporary name, right.txt takes its name, and then the
            // name left.txt is to get turns out to be in use.
            ring(
                TEMPORARY,
                &[("left.txt", "taken.txt"), ("right.txt", "left.txt")],
            ),
            single("last.txt", "9.txt"),
        ])
        .await;

    assert_eq!(renamed_names(&summary), [("first.txt", "1.txt")], "{label}");
    let failure = summary
        .failure
        .unwrap_or_else(|| panic!("{label}: the swap cannot finish"));
    assert_eq!(
        (failure.from.as_str(), failure.to.as_str()),
        ("left.txt", "taken.txt"),
        "{label}"
    );
    assert!(
        failure.message.contains("already exists"),
        "{label}: {}",
        failure.message
    );
    assert_eq!(summary.stranded, None, "{label}");
    assert!(!summary.cancelled, "{label}");
    assert_eq!(
        place.contents().await,
        held(&[
            ("1.txt", Some("first")),
            ("left.txt", Some("left")),
            ("right.txt", Some("right")),
            ("taken.txt", Some("not to be replaced")),
            ("last.txt", Some("last")),
        ]),
        "{label}"
    );

    // A plain rename onto a name in use is refused the same way.
    let summary = place.rename(vec![single("left.txt", "right.txt")]).await;
    assert!(summary.renamed.is_empty(), "{label}");
    assert!(summary.failure.is_some(), "{label}");
    assert_eq!(
        place.read(&place.path("right.txt")).await,
        "right",
        "{label}"
    );
}

#[tokio::test]
async fn renames_a_set_with_a_swap_on_the_sftp_server() {
    let Some(server) = sftp_server() else {
        eprintln!("POROS_TEST_SSH_PORT is not set; skipping");
        return;
    };
    let place = Place::open(&server, "sftp", false).await;
    renames_a_set_with_a_swap(&place).await;
    place.close().await;
}

#[tokio::test]
async fn renames_a_set_with_a_swap_on_every_other_server() {
    for server in every_server() {
        if server.protocol == Protocol::Sftp {
            continue;
        }
        let place = Place::open(&server, "set", false).await;
        renames_a_set_with_a_swap(&place).await;
        place.close().await;
    }
}

#[tokio::test]
async fn renames_a_set_with_a_swap_inside_a_vault_on_every_server() {
    for server in every_server() {
        let place = Place::open(&server, "vault", true).await;
        renames_a_set_with_a_swap(&place).await;
        place.close().await;
    }
}

#[tokio::test]
async fn stops_and_puts_names_back_on_every_server() {
    for server in every_server() {
        for in_vault in [false, true] {
            let place = Place::open(&server, "stop", in_vault).await;
            stops_and_puts_names_back(&place).await;
            place.close().await;
        }
    }
}

#[tokio::test]
async fn refuses_a_request_with_a_path_for_a_name_before_renaming_anything() {
    for server in every_server() {
        let place = Place::open(&server, "refuse", false).await;
        place.write("a.txt", "a").await;
        place.write("b.txt", "b").await;
        for (from, to) in [
            ("b.txt", "../escaped.txt"),
            ("b.txt", "sub/b.txt"),
            ("../b.txt", "c.txt"),
            ("b.txt", ".."),
            ("b.txt", ""),
        ] {
            let error = place
                .operations()
                .rename(RenameRequest {
                    operation_id: "refused".to_string(),
                    location: place.location(),
                    directory: place.folder.clone(),
                    groups: vec![single("a.txt", "renamed.txt"), single(from, to)],
                })
                .await
                .expect_err("a name that is a path is refused");
            assert_eq!(
                error.kind,
                poros_lib::error::ErrorKind::InvalidInput,
                "{}: {to:?}",
                place.label
            );
        }
        assert_eq!(
            place.contents().await,
            held(&[("a.txt", Some("a")), ("b.txt", Some("b"))]),
            "{}: the valid rename before it did not run either",
            place.label
        );
        place.close().await;
    }
}

#[tokio::test]
async fn says_which_names_are_taken_on_every_server() {
    for server in every_server() {
        for in_vault in [false, true] {
            let place = Place::open(&server, "taken", in_vault).await;
            place.write("Notes.txt", "notes").await;
            place.files.make_dir(&place.folder, "album").await.unwrap();
            let asked = ["Notes.txt", "NOTES.TXT", "album", "missing.txt"].map(str::to_string);
            let taken = place
                .operations()
                .conflicts(place.location(), asked.to_vec(), place.folder.clone())
                .await
                .unwrap_or_else(|error| panic!("{}: {}", place.label, error.message));
            // The servers of this test setup keep names that differ in case apart, which is
            // what the page learns by asking for a name in the other case.
            assert_eq!(taken, ["Notes.txt", "album"], "{}", place.label);
            place.close().await;
        }
    }
}

#[tokio::test]
async fn cancelling_leaves_every_item_under_its_old_or_its_new_name() {
    const ITEMS: usize = 240;
    let Some(server) = sftp_server() else {
        eprintln!("POROS_TEST_SSH_PORT is not set; skipping");
        return;
    };
    let place = Place::open(&server, "cancel", false).await;
    for number in 0..ITEMS {
        place
            .write(&format!("old-{number:03}.txt"), &number.to_string())
            .await;
    }
    let groups = (0..ITEMS)
        .map(|number| {
            single(
                &format!("old-{number:03}.txt"),
                &format!("new-{number:03}.txt"),
            )
        })
        .collect();

    let operations = Arc::new(place.operations());
    let running = {
        let operations = operations.clone();
        let request = RenameRequest {
            operation_id: "cancelled".to_string(),
            location: place.location(),
            directory: place.folder.clone(),
            groups,
        };
        tokio::spawn(async move { operations.rename(request).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    operations.cancel("cancelled");
    let summary = running.await.unwrap().unwrap();

    assert_eq!(summary.failure, None);
    assert_eq!(summary.stranded, None);
    // The renames are done in order, so those that ran are the first ones.
    let done = summary.renamed.len();
    assert_eq!(summary.cancelled, done < ITEMS);
    let expected: Vec<(String, Option<String>)> = {
        let mut names: Vec<_> = (0..ITEMS)
            .map(|number| {
                let prefix = if number < done { "new" } else { "old" };
                (
                    format!("{prefix}-{number:03}.txt"),
                    Some(number.to_string()),
                )
            })
            .collect();
        names.sort();
        names
    };
    assert_eq!(place.contents().await, expected);
    place.close().await;
}

#[tokio::test]
async fn renames_on_this_computer() {
    let folder = tempfile::tempdir().unwrap();
    let path = |name: &str| folder.path().join(name);
    for (name, contents) in [
        ("a.txt", "was a"),
        ("b.txt", "was b"),
        ("c.txt", "was c"),
        ("d.txt", "was d"),
        ("readme.md", "read me"),
        ("taken.txt", "not to be replaced"),
    ] {
        std::fs::write(path(name), contents).unwrap();
    }
    std::fs::create_dir(path("album")).unwrap();
    std::fs::write(path("album").join("inside.txt"), "inside").unwrap();

    let (_config, manager) = {
        let config = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(config.path().join("known_hosts"), Events::default());
        (config, Arc::new(manager))
    };
    let operations = FileOperations::new(manager, Events::default());
    let request = |groups| RenameRequest {
        operation_id: "local".to_string(),
        location: Location::Local,
        directory: folder.path().to_string_lossy().into_owned(),
        groups,
    };
    let summary = operations
        .rename(request(vec![
            ring(TEMPORARY, &[("a.txt", "b.txt"), ("b.txt", "a.txt")]),
            // Through a temporary name, as the page plans a change of case alone where case
            // does not matter: it works on every file system.
            ring("poros-rename-0a1b2c3d-2.tmp", &[("readme.md", "README.md")]),
            single("album", ACCENTED),
            single("c.txt", "taken.txt"),
            single("d.txt", "never.txt"),
        ]))
        .await
        .unwrap();

    assert_eq!(
        renamed_names(&summary),
        [
            ("b.txt", "a.txt"),
            ("a.txt", "b.txt"),
            ("readme.md", "README.md"),
            ("album", ACCENTED),
        ]
    );
    let failure = summary.failure.unwrap();
    assert_eq!(
        (failure.from.as_str(), failure.to.as_str()),
        ("c.txt", "taken.txt")
    );
    for renamed in &summary.renamed {
        assert_eq!(
            std::path::Path::new(&renamed.path),
            path(&renamed.to),
            "the path the pane selects afterwards"
        );
    }
    let read = |name: &str| std::fs::read_to_string(path(name)).unwrap();
    assert_eq!(read("a.txt"), "was b");
    assert_eq!(read("b.txt"), "was a");
    assert_eq!(read("README.md"), "read me");
    assert_eq!(read("c.txt"), "was c");
    assert_eq!(
        read("d.txt"),
        "was d",
        "the rename after the failure did not run"
    );
    assert_eq!(read("taken.txt"), "not to be replaced");
    assert_eq!(
        std::fs::read_to_string(path(ACCENTED).join("inside.txt")).unwrap(),
        "inside"
    );
    let mut names: Vec<String> = std::fs::read_dir(folder.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut expected = [
        "README.md",
        "a.txt",
        "b.txt",
        "c.txt",
        "d.txt",
        "taken.txt",
        ACCENTED,
    ]
    .map(str::to_string);
    expected.sort();
    assert_eq!(names, expected);

    let error = operations
        .rename(request(vec![single("a.txt", "../escaped.txt")]))
        .await
        .expect_err("a name that is a path is refused");
    assert_eq!(error.kind, poros_lib::error::ErrorKind::InvalidInput);
}
