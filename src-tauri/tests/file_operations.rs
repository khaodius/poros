//! Moving, copying, permissions and comparing on a real SSH server. Skipped unless
//! `POROS_TEST_SSH_PORT` is set; the server is the one `sftp_server.rs` describes.

use std::sync::Arc;

use poros_lib::events::Events;
use poros_lib::file_ops::archive::ExtractInto;
use poros_lib::file_ops::{
    CompareRequest, Comparison, Conflict, DeleteRequest, DeleteSummary, ExtractRequest,
    FileOperations, FileRef, Location, Method, Mode, ModeChange, MoveCopyRequest, OperationSummary,
    PermissionRequest, ServerFeatures,
};
use poros_lib::model::EntryKind;
use poros_lib::protocol::Protocol;
use poros_lib::session::{Session, SessionInfo, SessionManager};
use poros_lib::sftp::RemoteFs;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use serde_json::Value;

const OWNER: &str = "main";

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

fn profile(server: &Server) -> ConnectProfile {
    ConnectProfile {
        protocol: Protocol::Sftp,
        host: "127.0.0.1".into(),
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
        s3: Default::default(),
        route: Route::default(),
    }
}

struct Fixture {
    _known_hosts: tempfile::TempDir,
    local: tempfile::TempDir,
    manager: Arc<SessionManager>,
    session: SessionInfo,
    /// Absolute remote scratch folder.
    remote: String,
}

impl Fixture {
    async fn new(server: &Server, name: &str) -> Self {
        let known_hosts = tempfile::tempdir().unwrap();
        let manager = Arc::new(SessionManager::new(
            known_hosts.path().join("known_hosts"),
            Events::default(),
        ));
        let session = match manager.connect(profile(server), None, OWNER).await {
            Ok(session) => session,
            Err(error) => {
                let host_key = error.host_key.expect("only the host key may be unknown");
                let approval = HostKeyApproval {
                    fingerprint: host_key.fingerprint,
                    remember: true,
                };
                manager
                    .connect(profile(server), Some(approval), OWNER)
                    .await
                    .unwrap()
            }
        };
        let fixture = Self {
            _known_hosts: known_hosts,
            local: tempfile::tempdir().unwrap(),
            manager,
            remote: format!(
                "{}/poros-fileops-{name}-{}",
                session.home.trim_end_matches('/'),
                std::process::id()
            ),
            session,
        };
        let session = fixture.session().await;
        let _ = session
            .sftp()
            .unwrap()
            .delete(std::slice::from_ref(&fixture.remote))
            .await;
        session
            .sftp()
            .unwrap()
            .make_dir_at(&fixture.remote, None)
            .await
            .unwrap();
        fixture
    }

    async fn session(&self) -> Arc<Session> {
        self.manager.get(&self.session.id).await.unwrap()
    }

    fn path(&self, relative: &str) -> String {
        format!("{}/{relative}", self.remote)
    }

    fn operations(&self, features: ServerFeatures) -> FileOperations {
        FileOperations::with_features(self.manager.clone(), Events::default(), features)
    }

    fn location(&self) -> Location {
        Location::Remote {
            session_id: self.session.id.clone(),
        }
    }

    async fn write(&self, relative: &str, contents: &[u8]) {
        let session = self.session().await;
        write_file(session.sftp().unwrap(), &self.path(relative), contents).await;
    }

    async fn read(&self, relative: &str) -> Vec<u8> {
        let session = self.session().await;
        session
            .sftp()
            .unwrap()
            .read_to_end(&self.path(relative), u64::MAX)
            .await
            .unwrap()
    }

    async fn exists(&self, relative: &str) -> bool {
        let session = self.session().await;
        session
            .sftp()
            .unwrap()
            .lstat_entry(&self.path(relative))
            .await
            .unwrap()
            .is_some()
    }

    async fn mkdir(&self, relative: &str) {
        let session = self.session().await;
        session
            .sftp()
            .unwrap()
            .make_dir_at(&self.path(relative), None)
            .await
            .unwrap();
    }

    async fn run(
        &self,
        operations: &FileOperations,
        mode: Mode,
        sources: &[&str],
        target: &str,
        conflict: Conflict,
    ) -> OperationSummary {
        operations
            .move_or_copy(MoveCopyRequest {
                operation_id: uuid::Uuid::new_v4().to_string(),
                location: self.location(),
                mode,
                sources: sources.iter().map(|source| self.path(source)).collect(),
                target_directory: self.path(target),
                conflict,
                target_name: None,
            })
            .await
            .unwrap()
    }

    async fn delete(&self, operations: &FileOperations, paths: &[&str]) -> DeleteSummary {
        operations
            .delete(DeleteRequest {
                operation_id: uuid::Uuid::new_v4().to_string(),
                session_id: self.session.id.clone(),
                paths: paths.iter().map(|path| self.path(path)).collect(),
            })
            .await
            .unwrap()
    }

    async fn names(&self, relative: &str) -> Vec<String> {
        let session = self.session().await;
        let mut names: Vec<String> = session
            .sftp()
            .unwrap()
            .entries(&self.path(relative))
            .await
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        names.sort();
        names
    }

    async fn close(self) {
        let session = self.session().await;
        session
            .sftp()
            .unwrap()
            .delete(std::slice::from_ref(&self.remote))
            .await
            .unwrap();
        self.manager.disconnect(&self.session.id).await.unwrap();
    }
}

async fn write_file(fs: &RemoteFs, path: &str, contents: &[u8]) {
    let handle = fs.open_for_write(path, true, Some(0o644)).await.unwrap();
    let chunk = fs.write_size(32 * 1024) as usize;
    for (index, piece) in contents.chunks(chunk).enumerate() {
        fs.write_chunk(&handle, (index * chunk) as u64, piece.to_vec())
            .await
            .unwrap();
    }
    fs.close_handle(handle).await.unwrap();
}

/// Deterministic bytes that do not compress, so a misplaced piece shows up.
fn pattern(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..length)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u8
        })
        .collect()
}

const EVERY_WAY: [ServerFeatures; 4] = [
    ServerFeatures {
        copy_data: true,
        commands: true,
    },
    ServerFeatures {
        copy_data: false,
        commands: true,
    },
    ServerFeatures {
        copy_data: true,
        commands: false,
    },
    ServerFeatures {
        copy_data: false,
        commands: false,
    },
];

#[tokio::test]
async fn moves_within_the_server() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "move").await;
    let operations = fixture.operations(ServerFeatures::default());
    fixture.mkdir("from").await;
    fixture.mkdir("from/site").await;
    fixture.mkdir("to").await;
    fixture.write("from/a.txt", b"first").await;
    fixture.write("from/site/index.html", b"<p>hi</p>").await;

    let summary = fixture
        .run(
            &operations,
            Mode::Move,
            &["from/a.txt", "from/site"],
            "to",
            Conflict::KeepBoth,
        )
        .await;
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_eq!(
        summary.placed,
        [fixture.path("to/a.txt"), fixture.path("to/site")]
    );
    assert_eq!(summary.methods, [Method::Rename]);
    assert!(!fixture.exists("from/a.txt").await);
    assert_eq!(fixture.read("to/site/index.html").await, b"<p>hi</p>");

    // Name clashes: skip, keep both, then replace.
    fixture.write("from/a.txt", b"second").await;
    let skipped = fixture
        .run(
            &operations,
            Mode::Move,
            &["from/a.txt"],
            "to",
            Conflict::Skip,
        )
        .await;
    assert_eq!(skipped.skipped, 1);
    assert_eq!(fixture.read("to/a.txt").await, b"first");
    let kept = fixture
        .run(
            &operations,
            Mode::Move,
            &["from/a.txt"],
            "to",
            Conflict::KeepBoth,
        )
        .await;
    assert_eq!(kept.placed, [fixture.path("to/a (2).txt")]);
    fixture.write("from/a.txt", b"third").await;
    fixture
        .run(
            &operations,
            Mode::Move,
            &["from/a.txt"],
            "to",
            Conflict::Replace,
        )
        .await;
    assert_eq!(fixture.read("to/a.txt").await, b"third");

    // A folder merges into one of the same name.
    fixture.mkdir("from/site").await;
    fixture.write("from/site/index.html", b"<p>new</p>").await;
    fixture.write("from/site/about.html", b"about").await;
    let merged = fixture
        .run(
            &operations,
            Mode::Move,
            &["from/site"],
            "to",
            Conflict::Replace,
        )
        .await;
    assert!(merged.failures.is_empty(), "{:?}", merged.failures);
    assert_eq!(fixture.read("to/site/index.html").await, b"<p>new</p>");
    assert_eq!(fixture.read("to/site/about.html").await, b"about");
    assert!(!fixture.exists("from/site").await);

    let into_itself = fixture
        .run(
            &operations,
            Mode::Move,
            &["to/site"],
            "to/site",
            Conflict::KeepBoth,
        )
        .await;
    assert_eq!(into_itself.failures.len(), 1);
    assert!(fixture.exists("to/site/index.html").await);

    fixture.close().await;
}

#[tokio::test]
async fn copies_within_the_server_every_way() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "copy").await;
    let big = pattern(3 * 1024 * 1024 + 17, 7);
    fixture.mkdir("source").await;
    fixture.mkdir("source/nested").await;
    fixture.mkdir("source/nested/deeper").await;
    fixture.write("source/big.bin", &big).await;
    fixture.write("source/empty", b"").await;
    fixture
        .write("source/nested/deeper/note.txt", b"note")
        .await;
    {
        let session = fixture.session().await;
        session
            .sftp()
            .unwrap()
            .make_symlink(&fixture.path("source/link"), "nested/deeper/note.txt")
            .await
            .unwrap();
        session
            .sftp()
            .unwrap()
            .set_attributes(
                &fixture.path("source/big.bin"),
                Some(1_600_000_000),
                Some(0o640),
            )
            .await
            .unwrap();
    }
    let copy_data = fixture.session().await.sftp().unwrap().supports_copy_data();

    for (index, features) in EVERY_WAY.into_iter().enumerate() {
        let operations = fixture.operations(features);
        let target = format!("target-{index}");
        fixture.mkdir(&target).await;

        let summary = fixture
            .run(
                &operations,
                Mode::Copy,
                &["source", "source/big.bin"],
                &target,
                Conflict::KeepBoth,
            )
            .await;
        assert!(
            summary.failures.is_empty(),
            "{features:?}: {:?}",
            summary.failures
        );
        let file_method = if features.copy_data && copy_data {
            Method::CopyData
        } else if features.commands {
            Method::Command
        } else {
            Method::Stream
        };
        assert!(
            summary.methods.contains(&file_method),
            "{features:?}: {:?}",
            summary.methods
        );

        for copied in [
            format!("{target}/source/big.bin"),
            format!("{target}/big.bin"),
        ] {
            assert_eq!(fixture.read(&copied).await, big, "{features:?} {copied}");
            let session = fixture.session().await;
            let stat = session
                .sftp()
                .unwrap()
                .lstat_entry(&fixture.path(&copied))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(stat.modified, Some(1_600_000_000), "{features:?} {copied}");
            assert_eq!(stat.permissions, Some(0o640), "{features:?} {copied}");
        }
        assert_eq!(fixture.read(&format!("{target}/source/empty")).await, b"");
        assert_eq!(
            fixture
                .read(&format!("{target}/source/nested/deeper/note.txt"))
                .await,
            b"note"
        );
        let session = fixture.session().await;
        let link = fixture.path(&format!("{target}/source/link"));
        assert_eq!(
            session
                .sftp()
                .unwrap()
                .lstat_entry(&link)
                .await
                .unwrap()
                .unwrap()
                .kind,
            EntryKind::Symlink
        );
        assert_eq!(
            session.sftp().unwrap().read_link(&link).await.unwrap(),
            "nested/deeper/note.txt"
        );

        // Copying into the same folder makes a numbered copy; replacing merges and overwrites.
        let in_place = fixture
            .run(
                &operations,
                Mode::Copy,
                &[&format!("{target}/big.bin")],
                &target,
                Conflict::Replace,
            )
            .await;
        assert_eq!(
            in_place.placed,
            [fixture.path(&format!("{target}/big (2).bin"))]
        );
        fixture
            .write(&format!("{target}/source/empty"), b"stale")
            .await;
        fixture
            .write(&format!("{target}/source/extra"), b"kept")
            .await;
        let replaced = fixture
            .run(
                &operations,
                Mode::Copy,
                &["source"],
                &target,
                Conflict::Replace,
            )
            .await;
        assert!(
            replaced.failures.is_empty(),
            "{features:?}: {:?}",
            replaced.failures
        );
        assert_eq!(fixture.read(&format!("{target}/source/empty")).await, b"");
        assert_eq!(
            fixture.read(&format!("{target}/source/extra")).await,
            b"kept"
        );
    }

    fixture.close().await;
}

/// Items are placed four at a time. Each source here finds the same name free before any of them
/// has created it, so without a claim on the name they would all be written to it.
#[tokio::test]
async fn copies_made_together_get_names_of_their_own() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "together-copy").await;
    let files = ["a.txt", "a (2).txt", "a (3).txt", "a (4).txt"];
    let folders = ["site", "site (2)"];

    for (index, features) in EVERY_WAY.into_iter().enumerate() {
        let operations = fixture.operations(features);
        let folder = format!("copies-{index}");
        fixture.mkdir(&folder).await;
        for name in files {
            fixture
                .write(&format!("{folder}/{name}"), name.as_bytes())
                .await;
        }
        for name in folders {
            fixture.mkdir(&format!("{folder}/{name}")).await;
            fixture
                .write(&format!("{folder}/{name}/index.html"), name.as_bytes())
                .await;
        }
        let inside = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .map(|name| format!("{folder}/{name}"))
                .collect()
        };

        let sources = inside(&files);
        let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
        let copied = fixture
            .run(
                &operations,
                Mode::Copy,
                &sources,
                &folder,
                Conflict::KeepBoth,
            )
            .await;
        assert!(
            copied.failures.is_empty(),
            "{features:?}: {:?}",
            copied.failures
        );
        let mut placed = copied.placed.clone();
        placed.sort();
        assert_eq!(
            placed,
            inside(&["a (5).txt", "a (6).txt", "a (7).txt", "a (8).txt"])
                .iter()
                .map(|path| fixture.path(path))
                .collect::<Vec<_>>(),
            "{features:?}"
        );
        let session = fixture.session().await;
        for (name, copy) in files.iter().zip(&copied.placed) {
            let contents = session.sftp().unwrap().read_to_end(copy, 64).await.unwrap();
            assert_eq!(contents, name.as_bytes(), "{features:?} {copy}");
        }

        let sources = inside(&folders);
        let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
        let copied = fixture
            .run(
                &operations,
                Mode::Copy,
                &sources,
                &folder,
                Conflict::KeepBoth,
            )
            .await;
        assert!(
            copied.failures.is_empty(),
            "{features:?}: {:?}",
            copied.failures
        );
        let mut placed = copied.placed.clone();
        placed.sort();
        assert_eq!(
            placed,
            [
                fixture.path(&format!("{folder}/site (3)")),
                fixture.path(&format!("{folder}/site (4)"))
            ],
            "{features:?}"
        );
        for (name, copy) in folders.iter().zip(&copied.placed) {
            let entries = session.sftp().unwrap().entries(copy).await.unwrap();
            assert_eq!(entries.len(), 1, "{features:?} {copy}: {entries:?}");
            let contents = session
                .sftp()
                .unwrap()
                .read_to_end(&format!("{copy}/index.html"), 64)
                .await
                .unwrap();
            assert_eq!(contents, name.as_bytes(), "{features:?} {copy}");
        }
    }

    fixture.close().await;
}

/// Two items moved into a folder that has both their names: "keep both" numbers each one, and
/// neither is moved over the other.
#[tokio::test]
async fn items_moved_together_are_all_kept() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "together-move").await;

    for (index, features) in [EVERY_WAY[0], EVERY_WAY[3]].into_iter().enumerate() {
        let operations = fixture.operations(features);
        let (from, to) = (format!("from-{index}"), format!("to-{index}"));
        fixture.mkdir(&from).await;
        fixture.mkdir(&to).await;
        for name in ["a.txt", "a (2).txt"] {
            fixture
                .write(&format!("{to}/{name}"), format!("kept {name}").as_bytes())
                .await;
            fixture
                .write(
                    &format!("{from}/{name}"),
                    format!("moved {name}").as_bytes(),
                )
                .await;
        }

        let moved = fixture
            .run(
                &operations,
                Mode::Move,
                &[&format!("{from}/a.txt"), &format!("{from}/a (2).txt")],
                &to,
                Conflict::KeepBoth,
            )
            .await;
        assert!(
            moved.failures.is_empty(),
            "{features:?}: {:?}",
            moved.failures
        );
        assert_eq!(
            fixture.names(&to).await,
            ["a (2).txt", "a (3).txt", "a (4).txt", "a.txt"],
            "{features:?}"
        );
        assert!(fixture.names(&from).await.is_empty(), "{features:?}");
        for name in ["a.txt", "a (2).txt"] {
            assert_eq!(
                fixture.read(&format!("{to}/{name}")).await,
                format!("kept {name}").as_bytes(),
                "{features:?}"
            );
        }
        let session = fixture.session().await;
        for (name, placed) in ["a.txt", "a (2).txt"].iter().zip(&moved.placed) {
            let contents = session
                .sftp()
                .unwrap()
                .read_to_end(placed, 64)
                .await
                .unwrap();
            assert_eq!(
                contents,
                format!("moved {name}").as_bytes(),
                "{features:?} {placed}"
            );
        }
    }

    fixture.close().await;
}

/// Two items with one name, from different folders, never end up as one: the second is
/// numbered, skipped or refused as the choice for name clashes says, and never replaces the
/// first.
#[tokio::test]
async fn items_with_one_name_do_not_replace_each_other() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "one-name").await;
    let operations = fixture.operations(ServerFeatures::default());

    for (mode, conflict) in [
        (Mode::Move, Conflict::KeepBoth),
        (Mode::Move, Conflict::Replace),
        (Mode::Move, Conflict::Skip),
        (Mode::Copy, Conflict::Replace),
    ] {
        let case = format!("{mode:?}-{conflict:?}").to_lowercase();
        for folder in ["", "/first", "/second", "/archive"] {
            fixture.mkdir(&format!("{case}{folder}")).await;
        }
        fixture
            .write(&format!("{case}/first/report.csv"), b"first")
            .await;
        fixture
            .write(&format!("{case}/second/report.csv"), b"second")
            .await;

        let summary = fixture
            .run(
                &operations,
                mode,
                &[
                    &format!("{case}/first/report.csv"),
                    &format!("{case}/second/report.csv"),
                ],
                &format!("{case}/archive"),
                conflict,
            )
            .await;

        let mut contents = Vec::new();
        for folder in ["first", "second", "archive"] {
            for name in fixture.names(&format!("{case}/{folder}")).await {
                contents.push(fixture.read(&format!("{case}/{folder}/{name}")).await);
            }
        }
        contents.sort();
        contents.dedup();
        assert_eq!(
            contents,
            [b"first".to_vec(), b"second".to_vec()],
            "{case}: one item's contents are gone"
        );
        assert_eq!(
            fixture.read(&format!("{case}/archive/report.csv")).await,
            b"first",
            "{case}"
        );
        let archived = fixture.names(&format!("{case}/archive")).await;
        match conflict {
            Conflict::KeepBoth => {
                assert_eq!(archived, ["report (2).csv", "report.csv"], "{case}");
                assert!(
                    summary.failures.is_empty(),
                    "{case}: {:?}",
                    summary.failures
                );
            }
            Conflict::Skip => {
                assert_eq!(archived, ["report.csv"], "{case}");
                assert_eq!(summary.skipped, 1, "{case}");
            }
            _ => {
                assert_eq!(archived, ["report.csv"], "{case}");
                assert_eq!(summary.failures.len(), 1, "{case}: {:?}", summary.failures);
                assert_eq!(
                    summary.failures[0].path,
                    fixture.path(&format!("{case}/second/report.csv")),
                    "{case}"
                );
            }
        }
    }

    fixture.close().await;
}

/// `album` is a link to `photos`, so `album/inside` is a folder inside `photos` however it is
/// spelled, and `photos` cannot go into it.
#[tokio::test]
async fn a_folder_does_not_go_into_itself_through_a_link() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "into-itself").await;

    for (index, features) in [EVERY_WAY[0], EVERY_WAY[3]].into_iter().enumerate() {
        let operations = fixture.operations(features);
        let base = format!("case-{index}");
        for folder in ["", "/photos", "/photos/inside"] {
            fixture.mkdir(&format!("{base}{folder}")).await;
        }
        fixture.write(&format!("{base}/photos/a.jpg"), b"a").await;
        fixture
            .session()
            .await
            .sftp()
            .unwrap()
            .make_symlink(&fixture.path(&format!("{base}/album")), "photos")
            .await
            .unwrap();

        for (mode, verb) in [(Mode::Copy, "copy"), (Mode::Move, "move")] {
            let summary = fixture
                .run(
                    &operations,
                    mode,
                    &[&format!("{base}/photos")],
                    &format!("{base}/album/inside"),
                    Conflict::KeepBoth,
                )
                .await;
            assert!(summary.placed.is_empty(), "{features:?} {verb}");
            let messages: Vec<&str> = summary
                .failures
                .iter()
                .map(|failure| failure.message.as_str())
                .collect();
            assert_eq!(
                messages,
                [format!("Cannot {verb} the folder photos into itself")],
                "{features:?}"
            );
            assert_eq!(
                fixture.names(&format!("{base}/photos")).await,
                ["a.jpg", "inside"],
                "{features:?} {verb}"
            );
            assert!(
                fixture
                    .names(&format!("{base}/photos/inside"))
                    .await
                    .is_empty(),
                "{features:?} {verb}"
            );
        }
    }

    fixture.close().await;
}

/// /dev/shm is a separate filesystem on Linux, so a rename there fails and the move falls back
/// to `mv`, or without commands to copying and deleting.
#[tokio::test]
async fn moves_across_disks() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "disks").await;
    let other_disk = format!("/dev/shm/poros-fileops-{}", std::process::id());
    let session = fixture.session().await;
    let _ = session
        .sftp()
        .unwrap()
        .delete(std::slice::from_ref(&other_disk))
        .await;
    if session
        .sftp()
        .unwrap()
        .make_dir_at(&other_disk, None)
        .await
        .is_err()
    {
        fixture.close().await;
        return;
    }
    for (index, features) in [EVERY_WAY[0], EVERY_WAY[3]].into_iter().enumerate() {
        let folder = format!("folder-{index}");
        fixture.mkdir(&folder).await;
        fixture.write(&format!("{folder}/a.txt"), b"a").await;
        let summary = fixture
            .operations(features)
            .move_or_copy(MoveCopyRequest {
                operation_id: "disks".into(),
                location: fixture.location(),
                mode: Mode::Move,
                sources: vec![fixture.path(&folder)],
                target_directory: other_disk.clone(),
                conflict: Conflict::KeepBoth,
                target_name: None,
            })
            .await
            .unwrap();
        assert!(
            summary.failures.is_empty(),
            "{features:?}: {:?}",
            summary.failures
        );
        let expected = if features.commands {
            Method::Command
        } else {
            Method::Stream
        };
        assert!(
            summary.methods.contains(&expected),
            "{features:?}: {:?}",
            summary.methods
        );
        assert!(!fixture.exists(&folder).await);
        let moved = session
            .sftp()
            .unwrap()
            .read_to_end(&format!("{other_disk}/{folder}/a.txt"), 16)
            .await
            .unwrap();
        assert_eq!(moved, b"a");
    }
    session
        .sftp()
        .unwrap()
        .delete(std::slice::from_ref(&other_disk))
        .await
        .unwrap();
    drop(session);
    fixture.close().await;
}

/// Without commands, a folder moves to another disk one entry at a time, so a file that cannot
/// be read stays where it was instead of being deleted with the folder.
#[tokio::test]
async fn moving_across_disks_keeps_what_could_not_be_copied() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "disks-unreadable").await;
    let other_disk = format!("/dev/shm/poros-fileops-unreadable-{}", std::process::id());
    let session = fixture.session().await;
    let fs = session.sftp().unwrap();
    let _ = fs.delete(std::slice::from_ref(&other_disk)).await;
    if fs.make_dir_at(&other_disk, None).await.is_err() {
        drop(session);
        fixture.close().await;
        return;
    }
    fixture.mkdir("photos").await;
    fixture.mkdir("photos/2024").await;
    fixture.write("photos/a.jpg", b"a").await;
    fixture.write("photos/2024/b.jpg", b"b").await;
    fixture.write("photos/2024/locked.jpg", b"locked").await;
    fs.set_attributes(&fixture.path("photos/2024/locked.jpg"), None, Some(0))
        .await
        .unwrap();

    let summary = fixture
        .operations(EVERY_WAY[3])
        .move_or_copy(MoveCopyRequest {
            operation_id: "disks-unreadable".into(),
            location: fixture.location(),
            mode: Mode::Move,
            sources: vec![fixture.path("photos")],
            target_directory: other_disk.clone(),
            conflict: Conflict::KeepBoth,
            target_name: None,
        })
        .await
        .unwrap();
    assert_eq!(summary.failures.len(), 1, "{:?}", summary.failures);
    assert!(summary.failures[0].path.ends_with("locked.jpg"));
    for (path, contents) in [("a.jpg", b"a"), ("2024/b.jpg", b"b")] {
        let moved = fs
            .read_to_end(&format!("{other_disk}/photos/{path}"), 16)
            .await
            .unwrap();
        assert_eq!(moved, contents);
        assert!(!fixture.exists(&format!("photos/{path}")).await);
    }
    assert!(fixture.exists("photos/2024/locked.jpg").await);

    fs.set_attributes(&fixture.path("photos/2024/locked.jpg"), None, Some(0o600))
        .await
        .unwrap();
    fs.delete(std::slice::from_ref(&other_disk)).await.unwrap();
    drop(session);
    fixture.close().await;
}

#[tokio::test]
async fn deletes_trees_with_commands_and_over_sftp() {
    let Some(server) = server() else { return };
    for (way, commands) in [true, false].into_iter().enumerate() {
        let fixture = Fixture::new(&server, &format!("delete-{way}")).await;
        let operations = fixture.operations(ServerFeatures {
            copy_data: true,
            commands,
        });
        fixture.mkdir("keep").await;
        fixture.write("keep/precious.txt", b"keep me").await;
        fixture.mkdir("trash").await;
        let mut entries = 1;
        for folder in 0..3 {
            fixture.mkdir(&format!("trash/sub{folder}")).await;
            fixture.mkdir(&format!("trash/sub{folder}/deeper")).await;
            entries += 2;
            for file in 0..40 {
                fixture
                    .write(&format!("trash/sub{folder}/file {file}.txt"), b"x")
                    .await;
            }
            for file in 0..10 {
                fixture
                    .write(&format!("trash/sub{folder}/deeper/{file}"), b"y")
                    .await;
            }
            entries += 50;
        }
        let session = fixture.session().await;
        let fs = session.sftp().unwrap();
        // Links go, what they point at stays.
        fs.make_symlink(&fixture.path("trash/link"), &fixture.path("keep"))
            .await
            .unwrap();
        entries += 1;
        for name in ["loose.txt", "it's here.txt", "-delete"] {
            fixture.write(name, b"z").await;
            entries += 1;
        }

        let summary = fixture
            .delete(
                &operations,
                &[
                    "trash",
                    "trash/sub0",
                    "loose.txt",
                    "it's here.txt",
                    "-delete",
                ],
            )
            .await;
        assert_eq!(summary.failed, 0, "{:?}", summary.failures);
        assert_eq!(summary.deleted, entries);
        let expected_methods: &[Method] = if commands { &[Method::Command] } else { &[] };
        assert_eq!(summary.methods, expected_methods);
        // Nothing else was touched, and nothing was left behind.
        assert_eq!(fixture.names("").await, ["keep"]);
        assert_eq!(fixture.read("keep/precious.txt").await, b"keep me");
        drop(session);
        fixture.close().await;
    }
}

#[tokio::test]
async fn deleting_reports_what_could_not_go() {
    let Some(server) = server() else { return };
    for (way, commands) in [true, false].into_iter().enumerate() {
        let fixture = Fixture::new(&server, &format!("delete-locked-{way}")).await;
        let operations = fixture.operations(ServerFeatures {
            copy_data: true,
            commands,
        });
        fixture.mkdir("site").await;
        fixture.mkdir("site/locked").await;
        fixture.write("site/index.html", b"a").await;
        fixture.write("site/locked/held.txt", b"b").await;
        fixture.write("other.txt", b"c").await;
        let session = fixture.session().await;
        let fs = session.sftp().unwrap();
        let locked = fixture.path("site/locked");
        fs.set_permissions(&locked, 0o555).await.unwrap();

        let summary = fixture.delete(&operations, &["site", "other.txt"]).await;
        assert_eq!(summary.failed, 1, "{:?}", summary.failures);
        assert_eq!(
            summary.failures[0].path,
            fixture.path("site/locked/held.txt")
        );
        assert!(
            summary.failures[0].message.contains("ermission denied"),
            "{:?}",
            summary.failures
        );
        assert_eq!(summary.deleted, 2);
        assert!(!fixture.exists("other.txt").await);
        assert!(!fixture.exists("site/index.html").await);
        assert!(fixture.exists("site/locked/held.txt").await);
        assert_eq!(fixture.names("").await, ["site"]);

        fs.set_permissions(&locked, 0o755).await.unwrap();
        drop(session);
        fixture.close().await;
    }
}

#[tokio::test]
async fn changes_permissions_and_owners() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "chmod").await;
    fixture.mkdir("site").await;
    fixture.mkdir("site/assets").await;
    fixture.write("site/index.html", b"index").await;
    fixture.write("site/assets/app.js", b"app").await;
    let session = fixture.session().await;
    let mode = |relative: &str| {
        let fs = session.sftp().unwrap();
        let path = fixture.path(relative);
        async move {
            fs.lstat_entry(&path)
                .await
                .unwrap()
                .unwrap()
                .permissions
                .unwrap()
        }
    };
    session
        .sftp()
        .unwrap()
        .set_permissions(&fixture.path("site/index.html"), 0o600)
        .await
        .unwrap();

    for features in [ServerFeatures::default(), EVERY_WAY[3]] {
        let operations = fixture.operations(features);
        let exact = |bits: u32| ModeChange {
            set: bits,
            clear: 0o7777 & !bits,
        };
        let summary = operations
            .set_permissions(PermissionRequest {
                operation_id: "chmod".into(),
                session_id: fixture.session.id.clone(),
                paths: vec![fixture.path("site")],
                files: exact(0o644),
                folders: exact(0o750),
                recursive: true,
                owner: Some(if features.commands {
                    server.user.clone()
                } else {
                    session
                        .sftp()
                        .unwrap()
                        .lstat_entry(&fixture.path("site"))
                        .await
                        .unwrap()
                        .unwrap()
                        .uid
                        .unwrap()
                        .to_string()
                }),
                group: None,
            })
            .await
            .unwrap();
        assert!(
            summary.failures.is_empty(),
            "{features:?}: {:?}",
            summary.failures
        );
        assert_eq!(mode("site").await, 0o750);
        assert_eq!(mode("site/assets").await, 0o750);
        assert_eq!(mode("site/index.html").await, 0o644);
        assert_eq!(mode("site/assets/app.js").await, 0o644);

        // Only the named bit changes; the others stay as each entry has them.
        operations
            .set_permissions(PermissionRequest {
                operation_id: "chmod".into(),
                session_id: fixture.session.id.clone(),
                paths: vec![fixture.path("site/index.html"), fixture.path("site/assets")],
                files: ModeChange {
                    set: 0o020,
                    clear: 0,
                },
                folders: ModeChange {
                    set: 0,
                    clear: 0o050,
                },
                recursive: false,
                owner: None,
                group: None,
            })
            .await
            .unwrap();
        assert_eq!(mode("site/index.html").await, 0o664);
        assert_eq!(mode("site/assets").await, 0o700);
        assert_eq!(mode("site/assets/app.js").await, 0o644);
    }

    let operations = fixture.operations(ServerFeatures::default());
    let usage = operations
        .measure("measure", &fixture.session.id, &[fixture.path("site")])
        .await
        .unwrap();
    assert_eq!((usage.files, usage.folders, usage.bytes), (2, 2, 8));
    let details = operations
        .details(&fixture.session.id, &fixture.path("site/index.html"))
        .await
        .unwrap();
    assert!(details.uid.is_some());

    drop(session);
    fixture.close().await;
}

#[tokio::test]
async fn compares_server_and_local_files() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "compare").await;
    let operations = fixture.operations(ServerFeatures::default());
    fixture
        .write(
            "site.conf",
            b"server {\n  listen 80;\n  root /srv/www;\n}\n",
        )
        .await;
    let local = fixture.local.path().join("site.conf");
    std::fs::write(
        &local,
        b"server {\r\n  listen 443;\r\n  root /srv/www;\r\n}\r\n",
    )
    .unwrap();

    let compare = |left: FileRef, right: FileRef| {
        operations.compare(CompareRequest {
            operation_id: "compare".into(),
            left,
            right,
            ignore_whitespace: false,
            ignore_case: false,
        })
    };
    let remote_file = |relative: &str| FileRef {
        location: fixture.location(),
        path: fixture.path(relative),
    };
    let local_file = |path: &std::path::Path| FileRef {
        location: Location::Local,
        path: path.to_string_lossy().into_owned(),
    };

    let text = json(
        compare(remote_file("site.conf"), local_file(&local))
            .await
            .unwrap(),
    );
    assert_eq!(text["identical"], false);
    assert_eq!(text["content"]["kind"], "text");
    assert_eq!(text["content"]["changed"], 1);
    assert_eq!(text["content"]["leftLineEnding"], "lf");
    assert_eq!(text["content"]["rightLineEnding"], "crlf");
    assert_eq!(
        text["content"]["rows"][1]["leftChanges"][0],
        serde_json::json!([9, 11])
    );

    let same = compare(remote_file("site.conf"), remote_file("site.conf"))
        .await
        .unwrap();
    assert!(same.identical);

    // Past the text limit only the bytes are compared, stopping at the first difference.
    let large = pattern(9 * 1024 * 1024, 3);
    fixture.write("large.bin", &large).await;
    let large_local = fixture.local.path().join("large.bin");
    std::fs::write(&large_local, &large).unwrap();
    let large_same = json(
        compare(remote_file("large.bin"), local_file(&large_local))
            .await
            .unwrap(),
    );
    assert_eq!(large_same["identical"], true);
    assert_eq!(large_same["content"]["tooLarge"], true);
    let mut changed = large.clone();
    *changed.last_mut().unwrap() ^= 1;
    std::fs::write(&large_local, &changed).unwrap();
    let large_differs = compare(remote_file("large.bin"), local_file(&large_local))
        .await
        .unwrap();
    assert!(!large_differs.identical);

    fixture.close().await;
}

fn json(comparison: Comparison) -> Value {
    serde_json::to_value(comparison).unwrap()
}

fn extract_request(fixture: &Fixture, relative: &str, into: ExtractInto) -> ExtractRequest {
    ExtractRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        location: fixture.location(),
        path: fixture.path(relative),
        into,
        extractor: Default::default(),
    }
}

#[tokio::test]
async fn extracts_archives_with_the_servers_own_programs() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "extract").await;
    let operations = fixture.operations(ServerFeatures::default());

    // A tar.gz made here, with a folder inside, uploaded beside a folder of its name.
    let source = fixture.local.path().join("site");
    std::fs::create_dir_all(source.join("assets")).unwrap();
    std::fs::write(source.join("index.html"), b"<h1>hi</h1>").unwrap();
    std::fs::write(source.join("assets/app.js"), pattern(70_000, 3)).unwrap();
    let archive = fixture.local.path().join("site.tar.gz");
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&source)
        .arg(".")
        .status()
        .unwrap();
    assert!(status.success());
    fixture
        .write("site.tar.gz", &std::fs::read(&archive).unwrap())
        .await;
    fixture.mkdir("site").await;

    let summary = operations
        .extract(extract_request(
            &fixture,
            "site.tar.gz",
            ExtractInto::Folder,
        ))
        .await
        .unwrap();
    assert_eq!(summary.destination, fixture.path("site (2)"));
    assert_eq!(summary.folder, fixture.remote);
    assert_eq!(fixture.read("site (2)/index.html").await, b"<h1>hi</h1>");
    assert_eq!(
        fixture.read("site (2)/assets/app.js").await,
        pattern(70_000, 3)
    );

    // Extracting here keeps what is already there, and puts what came out beside it under
    // free names, as Extract to a folder does with the folder.
    fixture.write("index.html", b"mine").await;
    let summary = operations
        .extract(extract_request(&fixture, "site.tar.gz", ExtractInto::Here))
        .await
        .unwrap();
    assert_eq!(
        summary.placed,
        [fixture.path("assets"), fixture.path("index (2).html")]
    );
    assert_eq!(summary.destination, fixture.remote);
    assert_eq!(fixture.read("index.html").await, b"mine");
    assert_eq!(fixture.read("index (2).html").await, b"<h1>hi</h1>");
    assert_eq!(fixture.read("assets/app.js").await, pattern(70_000, 3));

    // A single compressed file unpacks beside itself, under a free name.
    let dump = fixture.local.path().join("dump.sql");
    std::fs::write(&dump, pattern(9_000, 4)).unwrap();
    let status = std::process::Command::new("gzip")
        .arg("-k")
        .arg(&dump)
        .status()
        .unwrap();
    assert!(status.success());
    let compressed = std::fs::read(fixture.local.path().join("dump.sql.gz")).unwrap();
    fixture.write("dump.sql.gz", &compressed).await;
    let summary = operations
        .extract(extract_request(&fixture, "dump.sql.gz", ExtractInto::Here))
        .await
        .unwrap();
    assert_eq!(summary.destination, fixture.path("dump.sql"));
    assert_eq!(summary.placed, [fixture.path("dump.sql")]);
    assert_eq!(fixture.read("dump.sql").await, pattern(9_000, 4));

    // A damaged archive leaves no empty folder behind, and says what the program said.
    fixture
        .write("broken.tar.gz", b"\x1f\x8b\x08\x00not gzip at all")
        .await;
    let error = operations
        .extract(extract_request(
            &fixture,
            "broken.tar.gz",
            ExtractInto::Folder,
        ))
        .await
        .unwrap_err();
    assert!(error.message.contains("tar"), "{}", error.message);
    assert!(!fixture.exists("broken").await);
    operations
        .extract(extract_request(
            &fixture,
            "broken.tar.gz",
            ExtractInto::Here,
        ))
        .await
        .unwrap_err();
    // No working folder stays behind, after a failure or a success.
    let names = fixture.names("").await;
    assert!(
        !names.iter().any(|name| name.contains(".poros-")),
        "{names:?}"
    );

    // Text is not an archive.
    fixture.write("notes.txt", b"plain").await;
    let error = operations
        .extract(extract_request(&fixture, "notes.txt", ExtractInto::Here))
        .await
        .unwrap_err();
    assert_eq!(
        error.message,
        "notes.txt is not an archive Poros can extract"
    );

    fixture.close().await;
}
