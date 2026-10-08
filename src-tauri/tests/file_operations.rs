//! Moving, copying, permissions and comparing on a real SSH server. Skipped unless
//! `POROS_TEST_SSH_PORT` is set; the server is the one `sftp_server.rs` describes.

use std::sync::Arc;

use poros_lib::events::Events;
use poros_lib::file_ops::{
    CompareRequest, Comparison, Conflict, FileOperations, FileRef, Location, Method, Mode,
    ModeChange, MoveCopyRequest, OperationSummary, PermissionRequest, ServerFeatures,
};
use poros_lib::model::EntryKind;
use poros_lib::session::{Session, SessionInfo, SessionManager};
use poros_lib::sftp::RemoteFs;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval};
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
            .fs
            .delete(std::slice::from_ref(&fixture.remote))
            .await;
        session.fs.make_dir_at(&fixture.remote, None).await.unwrap();
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
        write_file(&session.fs, &self.path(relative), contents).await;
    }

    async fn read(&self, relative: &str) -> Vec<u8> {
        let session = self.session().await;
        session
            .fs
            .read_to_end(&self.path(relative), u64::MAX)
            .await
            .unwrap()
    }

    async fn exists(&self, relative: &str) -> bool {
        let session = self.session().await;
        session
            .fs
            .lstat_entry(&self.path(relative))
            .await
            .unwrap()
            .is_some()
    }

    async fn mkdir(&self, relative: &str) {
        let session = self.session().await;
        session
            .fs
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
            })
            .await
            .unwrap()
    }

    async fn close(self) {
        let session = self.session().await;
        session
            .fs
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
            .fs
            .make_symlink(&fixture.path("source/link"), "nested/deeper/note.txt")
            .await
            .unwrap();
        session
            .fs
            .set_attributes(
                &fixture.path("source/big.bin"),
                Some(1_600_000_000),
                Some(0o640),
            )
            .await
            .unwrap();
    }
    let copy_data = fixture.session().await.fs.supports_copy_data();

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
                .fs
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
            session.fs.lstat_entry(&link).await.unwrap().unwrap().kind,
            EntryKind::Symlink
        );
        assert_eq!(
            session.fs.read_link(&link).await.unwrap(),
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

/// /dev/shm is a separate filesystem on Linux, so a rename there fails and the move falls back
/// to `mv`, or without commands to copying and deleting.
#[tokio::test]
async fn moves_across_disks() {
    let Some(server) = server() else { return };
    let fixture = Fixture::new(&server, "disks").await;
    let other_disk = format!("/dev/shm/poros-fileops-{}", std::process::id());
    let session = fixture.session().await;
    let _ = session.fs.delete(std::slice::from_ref(&other_disk)).await;
    if session.fs.make_dir_at(&other_disk, None).await.is_err() {
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
            .fs
            .read_to_end(&format!("{other_disk}/{folder}/a.txt"), 16)
            .await
            .unwrap();
        assert_eq!(moved, b"a");
    }
    session
        .fs
        .delete(std::slice::from_ref(&other_disk))
        .await
        .unwrap();
    drop(session);
    fixture.close().await;
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
        let fs = &session.fs;
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
        .fs
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
                        .fs
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
