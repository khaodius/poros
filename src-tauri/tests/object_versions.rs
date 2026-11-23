//! End-to-end tests of object versions against an S3-compatible service that implements
//! versioning. Skipped unless `POROS_TEST_S3_PORT` names one on 127.0.0.1 that takes any keys,
//! as the moto server of the CI workflow does. Each test works in a bucket of its own and
//! deletes it at the end.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::stream::{self, StreamExt};
use poros_lib::cloud::s3::{Addressing, S3Options};
use poros_lib::editor::{EditorManager, SaveRequest};
use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::file_ops::{
    DeletedAction, DeletedItem, DeletedRequest, FileOperations, VersionCompareRequest,
};
use poros_lib::model::EntryKind;
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::ssh::{AuthMethod, ConnectProfile, Route};
use poros_lib::transfer::{ExistsAction, JobState, TransferManager, VersionDownload};
use poros_lib::versions::{self, ObjectVersion, ObjectVersions, VersionKind, Versioning};

const OWNER: &str = "main";
const MEBIBYTE: usize = 1024 * 1024;

fn port() -> Option<u16> {
    std::env::var("POROS_TEST_S3_PORT").ok()?.parse().ok()
}

fn profile(port: u16) -> ConnectProfile {
    ConnectProfile {
        protocol: Protocol::S3,
        host: format!("http://127.0.0.1:{port}"),
        port,
        username: "poros-access-key".into(),
        auth: AuthMethod::Password {
            password: "poros-secret-key".into(),
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

/// Sets a bucket's versioning, which nothing in Poros changes. The test server checks no
/// signatures, so the request goes out as it is.
fn set_versioning(port: u16, bucket: &str, status: &str) {
    let body = format!(
        r#"<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Status>{status}</Status></VersioningConfiguration>"#
    );
    let request = format!(
        "PUT /{bucket}?versioning HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
}

/// A session and an empty bucket of the test's own.
struct Scratch {
    _config: tempfile::TempDir,
    manager: Arc<SessionManager>,
    session: SessionInfo,
    files: Arc<dyn RemoteFileSystem>,
    port: u16,
    bucket: String,
}

impl Scratch {
    async fn open(name: &str) -> Option<Self> {
        let port = port()?;
        let config = tempfile::tempdir().unwrap();
        let manager = Arc::new(SessionManager::new(
            config.path().join("known_hosts"),
            Events::default(),
        ));
        let session = manager.connect(profile(port), None, OWNER).await.unwrap();
        let files = manager.get(&session.id).await.unwrap().files();
        let bucket = format!("poros-object-versions-{name}-{}", std::process::id());
        let root = format!("/{bucket}");
        if files.stat(&root).await.unwrap().is_some() {
            files.delete(std::slice::from_ref(&root)).await.unwrap();
        }
        files.make_dir("/", &bucket).await.unwrap();
        Some(Self {
            _config: config,
            manager,
            session,
            files,
            port,
            bucket,
        })
    }

    fn path(&self, key: &str) -> String {
        format!("/{}/{key}", self.bucket)
    }

    fn versioned(&self) -> &dyn ObjectVersions {
        versions::of(self.files.as_ref()).unwrap()
    }

    fn enable_versioning(&self) {
        set_versioning(self.port, &self.bucket, "Enabled");
    }

    async fn write(&self, key: &str, contents: &[u8]) {
        let mut stream = self
            .files
            .clone()
            .open_write(WriteRequest {
                path: self.path(key),
                offset: 0,
                size: contents.len() as u64,
                modified: None,
                permissions: None,
            })
            .await
            .unwrap();
        for chunk in contents.chunks(256 * 1024) {
            stream.write(Bytes::copy_from_slice(chunk)).await.unwrap();
        }
        stream.finish().await.unwrap();
    }

    async fn read(&self, key: &str) -> Vec<u8> {
        let stream = self
            .files
            .clone()
            .open_read(&self.path(key), 0..u64::MAX)
            .await
            .unwrap();
        collect(stream).await
    }

    async fn read_version(&self, key: &str, version_id: &str) -> Vec<u8> {
        let stream = self
            .versioned()
            .open_version(&self.path(key), version_id, 0..u64::MAX)
            .await
            .unwrap();
        collect(stream).await
    }

    async fn history(&self, key: &str) -> Vec<ObjectVersion> {
        self.versioned()
            .list_versions(&self.path(key))
            .await
            .unwrap()
    }

    async fn delete(&self, key: &str) {
        self.files.delete(&[self.path(key)]).await.unwrap();
    }

    /// The names a folder lists, and the names of what was deleted from it. The test server
    /// still lists a folder once every file in it carries a delete marker, which Amazon does
    /// not, so such a folder, empty when opened, is taken for what Amazon would list.
    async fn listed(&self, folder: &str) -> (Vec<String>, Vec<String>) {
        let path = match folder {
            "" => format!("/{}", self.bucket),
            folder => self.path(folder),
        };
        let listing = versions::list_with_deleted(self.files.as_ref(), &path)
            .await
            .unwrap();
        let mut held = Vec::new();
        for entry in listing.listing.entries {
            if entry.kind != EntryKind::Dir || self.holds_files(&entry.path).await {
                held.push(entry);
            }
        }
        let deleted = self.versioned().list_deleted(&path, &held).await.unwrap();
        let mut live: Vec<String> = held.into_iter().map(|entry| entry.name).collect();
        let mut deleted: Vec<String> = deleted
            .into_iter()
            .map(|entry| match entry.is_dir {
                true => format!("{}/", entry.name),
                false => entry.name,
            })
            .collect();
        live.sort();
        deleted.sort();
        (live, deleted)
    }

    /// Whether a file is listed anywhere under a folder.
    async fn holds_files(&self, folder: &str) -> bool {
        let mut folders = vec![folder.to_string()];
        while let Some(folder) = folders.pop() {
            for entry in self.files.list_dir(&folder).await.unwrap().entries {
                match entry.kind {
                    EntryKind::Dir => folders.push(entry.path),
                    _ => return true,
                }
            }
        }
        false
    }

    /// Deletes the bucket with every version in it and closes the session.
    async fn close(self) {
        let root = format!("/{}", self.bucket);
        self.files
            .delete(std::slice::from_ref(&root))
            .await
            .unwrap();
        assert!(self.files.stat(&root).await.unwrap().is_none());
        self.manager.disconnect(&self.session.id).await.unwrap();
    }
}

async fn collect(mut stream: Box<dyn poros_lib::protocol::ReadStream>) -> Vec<u8> {
    let mut contents = Vec::new();
    while let Some(chunk) = stream.next_chunk().await.unwrap() {
        contents.extend_from_slice(&chunk);
    }
    stream.finish().await.unwrap();
    contents
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

fn transfers(manager: &Arc<SessionManager>) -> TransferManager {
    TransferManager::new(
        manager.clone(),
        Events::default(),
        TransferSettings {
            workers: 4,
            segment_threshold_mib: 1,
            exists_action: ExistsAction::Overwrite,
            retry_attempts: 0,
            ..TransferSettings::default()
        },
    )
}

async fn wait_until_done(transfers: &TransferManager) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let list = transfers.list();
        let counts = &list.stats.counts;
        if counts.queued == 0 && counts.running == 0 {
            let unfinished: Vec<_> = list
                .jobs
                .iter()
                .filter(|job| job.state != JobState::Done)
                .collect();
            assert!(unfinished.is_empty(), "{unfinished:#?}");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "transfers did not finish: {counts:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn ids(history: &[ObjectVersion]) -> Vec<&str> {
    history
        .iter()
        .map(|version| version.version_id.as_str())
        .collect()
}

#[tokio::test]
async fn lists_restores_undeletes_and_removes_versions() {
    let Some(scratch) = Scratch::open("history").await else {
        return;
    };
    scratch.enable_versioning();
    let versioned = scratch.versioned();
    let key = "notes/report.txt";
    let path = scratch.path(key);
    assert_eq!(
        versioned.versioning(&path).await.unwrap(),
        Versioning::Enabled
    );

    let contents: [&[u8]; 3] = [b"first", b"second version", b"third and newest"];
    for content in contents {
        scratch.write(key, content).await;
    }
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 3);
    assert!(history
        .iter()
        .all(|version| version.kind == VersionKind::Version));
    assert_eq!(
        history
            .iter()
            .map(|version| version.latest)
            .collect::<Vec<_>>(),
        [true, false, false]
    );
    assert_eq!(
        history
            .iter()
            .map(|version| version.size)
            .collect::<Vec<_>>(),
        [16, 14, 5],
        "newest first"
    );
    assert!(history
        .iter()
        .all(|version| version.modified.is_some() && version.etag.is_some()));
    assert_eq!(ids(&history).into_iter().collect::<HashSet<_>>().len(), 3);
    let (newest, second, oldest) = (
        history[0].version_id.clone(),
        history[1].version_id.clone(),
        history[2].version_id.clone(),
    );
    assert_eq!(scratch.read_version(key, &oldest).await, b"first");
    assert_eq!(scratch.read_version(key, &second).await, b"second version");
    assert_eq!(
        versioned.stat_version(&path, &second).await.unwrap().size,
        14
    );
    // A range of an old version, as a download split between workers asks for.
    let part = versioned.open_version(&path, &second, 7..14).await.unwrap();
    assert_eq!(collect(part).await, b"version");

    // Restoring the oldest version copies it on top; nothing is lost.
    versioned.restore_version(&path, &oldest).await.unwrap();
    assert_eq!(scratch.read(key).await, b"first");
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 4, "history grew");
    assert!(history[0].latest && history[0].size == 5);
    assert_eq!(
        ids(&history)[1..],
        [newest.as_str(), second.as_str(), oldest.as_str()]
    );
    assert_eq!(
        scratch.read_version(key, &newest).await,
        b"third and newest"
    );

    // A delete only puts a marker over the versions.
    scratch.delete(key).await;
    assert!(scratch.files.stat(&path).await.unwrap().is_none());
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 5);
    assert!(history[0].latest && history[0].is_marker());
    assert!(history[1..].iter().all(|version| !version.is_marker()));
    let marker = history[0].version_id.clone();
    let refused = versioned.stat_version(&path, &marker).await.unwrap_err();
    assert_eq!(refused.kind, ErrorKind::InvalidInput, "{}", refused.message);

    let listing = versions::list_with_deleted(scratch.files.as_ref(), &scratch.path("notes"))
        .await
        .unwrap();
    assert!(listing.listing.entries.is_empty());
    assert_eq!(listing.deleted.len(), 1);
    let deleted = &listing.deleted[0];
    assert_eq!(deleted.name, "report.txt");
    assert_eq!(deleted.path, path);
    assert!(!deleted.is_dir);
    assert_eq!(
        deleted.size, 5,
        "the size of the version a restore brings back"
    );
    assert_eq!(deleted.deleted, history[0].modified);
    // One level up, the folder that now holds nothing shows as deleted.
    assert_eq!(
        scratch.listed("").await,
        (Vec::new(), vec!["notes/".to_string()])
    );

    versioned.undelete(&path).await.unwrap();
    assert_eq!(scratch.read(key).await, b"first");
    assert_eq!(scratch.history(key).await.len(), 4, "the marker is gone");
    assert_eq!(
        scratch.listed("notes").await,
        (vec!["report.txt".to_string()], Vec::new())
    );
    let not_deleted = versioned.undelete(&path).await.unwrap_err();
    assert_eq!(not_deleted.kind, ErrorKind::InvalidInput);

    // One version removed for good leaves the others alone.
    versioned.delete_version(&path, &second).await.unwrap();
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 3);
    assert!(!ids(&history).contains(&second.as_str()));
    let gone = versioned.stat_version(&path, &second).await.unwrap_err();
    assert_eq!(gone.kind, ErrorKind::NotFound);
    assert_eq!(scratch.read(key).await, b"first");
    assert_eq!(scratch.read_version(key, &oldest).await, b"first");

    // What the page sends is checked before it reaches a request.
    let invalid = versioned.restore_version(&path, "").await.unwrap_err();
    assert_eq!(invalid.kind, ErrorKind::InvalidInput);
    let invalid = versioned
        .delete_version(&path, "line\nbreak")
        .await
        .unwrap_err();
    assert_eq!(invalid.kind, ErrorKind::InvalidInput);
    let unknown = versioned
        .restore_version(&path, "no-such-version")
        .await
        .unwrap_err();
    assert_eq!(unknown.kind, ErrorKind::NotFound, "{}", unknown.message);

    scratch.close().await;
}

#[tokio::test]
async fn downloads_one_version_through_the_queue() {
    let Some(scratch) = Scratch::open("download").await else {
        return;
    };
    scratch.enable_versioning();
    let key = "backups/disk image (1).bin";
    let old = pattern(3 * MEBIBYTE + 4321, 7);
    let current = pattern(2 * MEBIBYTE, 11);
    scratch.write(key, &old).await;
    scratch.write(key, &current).await;
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 2);
    let old_version = &history[1];
    assert_eq!(old_version.size, old.len() as u64);

    let local = tempfile::tempdir().unwrap();
    let transfers = transfers(&scratch.manager);
    transfers
        .enqueue_version(VersionDownload {
            session_id: scratch.session.id.clone(),
            path: scratch.path(key),
            version_id: old_version.version_id.clone(),
            target_directory: local.path().to_string_lossy().into_owned(),
            name: None,
            size: old_version.size,
        })
        .await
        .unwrap();
    wait_until_done(&transfers).await;
    let downloaded = std::fs::read(local.path().join("disk image (1).bin")).unwrap();
    assert!(
        downloaded == old,
        "the old version, split between workers, arrived whole"
    );

    // Under another name, and the current version stays what a plain download gets.
    transfers
        .enqueue_version(VersionDownload {
            session_id: scratch.session.id.clone(),
            path: scratch.path(key),
            version_id: history[0].version_id.clone(),
            target_directory: local.path().to_string_lossy().into_owned(),
            name: Some("current.bin".into()),
            size: 0,
        })
        .await
        .unwrap();
    wait_until_done(&transfers).await;
    assert!(std::fs::read(local.path().join("current.bin")).unwrap() == current);

    // Names from the page cannot climb out of the folder, and a version must be one.
    let escape = transfers
        .enqueue_version(VersionDownload {
            session_id: scratch.session.id.clone(),
            path: scratch.path(key),
            version_id: old_version.version_id.clone(),
            target_directory: local.path().to_string_lossy().into_owned(),
            name: Some("../escaped.bin".into()),
            size: 0,
        })
        .await
        .unwrap_err();
    assert_eq!(escape.kind, ErrorKind::InvalidInput);
    assert!(!local.path().parent().unwrap().join("escaped.bin").exists());

    scratch.close().await;
}

#[tokio::test]
async fn restores_and_purges_deleted_folders_as_one_operation() {
    let Some(scratch) = Scratch::open("folders").await else {
        return;
    };
    scratch.enable_versioning();
    let kept = ["site/index.html", "site/live/now.txt"];
    let deleted = [
        "site/archive/2023/a.txt",
        "site/archive/2023/b b+c \u{e9}.txt",
        "site/archive/2024/deep/c.txt",
        "site/archive/top.txt",
    ];
    for key in kept.iter().chain(&deleted) {
        scratch.write(key, key.as_bytes()).await;
    }
    scratch.write("site/gone.txt", b"gone").await;
    scratch.delete("site/archive").await;
    scratch.delete("site/gone.txt").await;

    assert_eq!(
        scratch.listed("site").await,
        (
            vec!["index.html".to_string(), "live".to_string()],
            vec!["archive/".to_string(), "gone.txt".to_string()]
        )
    );
    // Inside a deleted folder, its files and folders are deleted in turn.
    assert_eq!(
        scratch.listed("site/archive").await,
        (
            Vec::new(),
            vec![
                "2023/".to_string(),
                "2024/".to_string(),
                "top.txt".to_string()
            ]
        )
    );
    let hidden = scratch
        .versioned()
        .deleted_under(&scratch.path("site/archive"))
        .await
        .unwrap();
    let with_content: Vec<&str> = hidden
        .iter()
        .filter(|file| file.newest_version().is_some())
        .map(|file| file.path.as_str())
        .collect();
    let expected: Vec<String> = deleted.iter().map(|key| scratch.path(key)).collect();
    assert_eq!(with_content, expected);

    let operations = FileOperations::new(scratch.manager.clone(), Events::default());
    let restored = operations
        .restore_or_purge_deleted(DeletedRequest {
            operation_id: "restore".into(),
            session_id: scratch.session.id.clone(),
            action: DeletedAction::Restore,
            items: vec![
                DeletedItem {
                    path: scratch.path("site/archive"),
                    is_dir: true,
                },
                DeletedItem {
                    path: scratch.path("site/gone.txt"),
                    is_dir: false,
                },
            ],
        })
        .await
        .unwrap();
    assert_eq!(restored.files, 5);
    assert_eq!(restored.failed, 0, "{:?}", restored.failures);
    for key in deleted {
        assert_eq!(scratch.read(key).await, key.as_bytes(), "{key}");
    }
    assert_eq!(scratch.read("site/gone.txt").await, b"gone");
    let (live, none_deleted) = scratch.listed("site").await;
    assert_eq!(live, ["archive", "gone.txt", "index.html", "live"]);
    assert!(none_deleted.is_empty());

    // A file that is not deleted is reported, and the rest still goes through.
    scratch.delete("site/gone.txt").await;
    let partly = operations
        .restore_or_purge_deleted(DeletedRequest {
            operation_id: "restore-again".into(),
            session_id: scratch.session.id.clone(),
            action: DeletedAction::Restore,
            items: vec![
                DeletedItem {
                    path: scratch.path("site/index.html"),
                    is_dir: false,
                },
                DeletedItem {
                    path: scratch.path("site/gone.txt"),
                    is_dir: false,
                },
            ],
        })
        .await
        .unwrap();
    assert_eq!((partly.files, partly.failed), (1, 1));
    assert_eq!(partly.failures[0].path, scratch.path("site/index.html"));
    assert_eq!(partly.failures[0].message, "index.html is not deleted");

    // Deleting for good leaves nothing of the folder behind, and nothing of the files kept.
    scratch.delete("site/archive").await;
    scratch.delete("site/gone.txt").await;
    let purged = operations
        .restore_or_purge_deleted(DeletedRequest {
            operation_id: "purge".into(),
            session_id: scratch.session.id.clone(),
            action: DeletedAction::Purge,
            items: vec![
                DeletedItem {
                    path: scratch.path("site/archive"),
                    is_dir: true,
                },
                DeletedItem {
                    path: scratch.path("site/gone.txt"),
                    is_dir: false,
                },
            ],
        })
        .await
        .unwrap();
    assert_eq!(purged.failed, 0, "{:?}", purged.failures);
    assert!(purged.files >= 5);
    assert_eq!(
        scratch.listed("site").await,
        (
            vec!["index.html".to_string(), "live".to_string()],
            Vec::new()
        )
    );
    assert!(scratch.history("site/gone.txt").await.is_empty());
    assert!(scratch
        .versioned()
        .deleted_under(&scratch.path("site"))
        .await
        .unwrap()
        .iter()
        .all(|file| file.newest_version().is_none()));
    for key in kept {
        assert_eq!(scratch.read(key).await, key.as_bytes());
    }

    scratch.close().await;
}

#[tokio::test]
async fn reads_an_old_version_in_the_editor_and_compares_it_with_the_current_file() {
    let Some(scratch) = Scratch::open("editor").await else {
        return;
    };
    scratch.enable_versioning();
    let key = "conf/site.conf";
    let path = scratch.path(key);
    scratch.write(key, b"listen 80;\nroot /srv;\n").await;
    scratch
        .write(key, b"listen 8080;\nroot /srv;\nindex on;\n")
        .await;
    let history = scratch.history(key).await;
    let old = history[1].version_id.clone();

    let editor = EditorManager::new(scratch.manager.clone(), Events::default());
    let info = editor
        .open_version(&scratch.session.id, &path, &old, OWNER)
        .await
        .unwrap();
    assert!(info.read_only);
    assert_eq!(info.name, "site.conf");
    let document = editor.load(&info.id, OWNER).await.unwrap();
    assert_eq!(document.text, "listen 80;\nroot /srv;\n");
    let refused = editor
        .save(SaveRequest {
            document_id: info.id.clone(),
            text: "changed".into(),
            encoding: document.encoding,
            line_ending: document.line_ending,
            expected: None,
        })
        .await
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::Unsupported);
    assert_eq!(
        scratch.read(key).await,
        b"listen 8080;\nroot /srv;\nindex on;\n"
    );
    editor.close(&info.id).await;

    let operations = FileOperations::new(scratch.manager.clone(), Events::default());
    let compare = |version_id: String| {
        operations.compare_version(VersionCompareRequest {
            operation_id: format!("compare-{version_id}"),
            session_id: scratch.session.id.clone(),
            path: path.clone(),
            version_id,
            ignore_whitespace: false,
            ignore_case: false,
        })
    };
    let comparison = serde_json::to_value(compare(old.clone()).await.unwrap()).unwrap();
    assert_eq!(comparison["identical"], false);
    assert_eq!(comparison["leftSize"], 22);
    assert_eq!(comparison["rightSize"], 34);
    assert_eq!(comparison["content"]["kind"], "text");
    assert_eq!(comparison["content"]["changed"], 1);
    assert_eq!(comparison["content"]["added"], 1);
    assert_eq!(comparison["content"]["leftLines"][0], "listen 80;");
    assert_eq!(comparison["content"]["rightLines"][0], "listen 8080;");
    let same = compare(history[0].version_id.clone()).await.unwrap();
    assert!(same.identical);

    // Past the text limit only the bytes are compared, read as streams.
    let large_key = "conf/large.bin";
    let large = pattern(9 * MEBIBYTE, 5);
    let mut changed = large.clone();
    changed[8 * MEBIBYTE] ^= 1;
    scratch.write(large_key, &large).await;
    scratch.write(large_key, &changed).await;
    let large_history = scratch.history(large_key).await;
    let large_compare = |version_id: String| {
        operations.compare_version(VersionCompareRequest {
            operation_id: "compare-large".into(),
            session_id: scratch.session.id.clone(),
            path: scratch.path(large_key),
            version_id,
            ignore_whitespace: false,
            ignore_case: false,
        })
    };
    let differs = large_compare(large_history[1].version_id.clone())
        .await
        .unwrap();
    assert!(!differs.identical);
    assert_eq!(differs.left_size, differs.right_size);
    let matches = large_compare(large_history[0].version_id.clone())
        .await
        .unwrap();
    assert!(matches.identical);

    scratch.close().await;
}

#[tokio::test]
async fn keeps_the_null_version_and_follows_suspended_versioning() {
    let Some(scratch) = Scratch::open("null").await else {
        return;
    };
    let versioned = scratch.versioned();
    let key = "plain.txt";
    let path = scratch.path(key);
    // Written before versioning was turned on.
    scratch.write(key, b"before versioning").await;
    scratch.enable_versioning();
    scratch.write(key, b"after").await;
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].version_id, "null");
    assert_eq!(
        scratch.read_version(key, "null").await,
        b"before versioning"
    );
    assert_eq!(
        versioned.stat_version(&path, "null").await.unwrap().size,
        17
    );

    versioned.restore_version(&path, "null").await.unwrap();
    assert_eq!(scratch.read(key).await, b"before versioning");
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 3);
    assert_eq!(history[2].version_id, "null");

    // The null version goes like any other, and the copy restored from it stays.
    versioned.delete_version(&path, "null").await.unwrap();
    let history = scratch.history(key).await;
    assert_eq!(history.len(), 2);
    assert!(!ids(&history).contains(&"null"));
    assert_eq!(scratch.read(key).await, b"before versioning");

    set_versioning(scratch.port, &scratch.bucket, "Suspended");
    assert_eq!(
        versioned.versioning(&path).await.unwrap(),
        Versioning::Suspended
    );
    // What was kept stays listed and readable while versioning is suspended.
    let suspended = scratch.history(key).await;
    assert_eq!(ids(&suspended), ids(&history));
    assert_eq!(
        scratch.read_version(key, &history[1].version_id).await,
        b"after"
    );

    scratch.close().await;
}

#[tokio::test]
async fn a_bucket_that_never_had_versioning_keeps_no_versions() {
    let Some(scratch) = Scratch::open("plain").await else {
        return;
    };
    let versioned = scratch.versioned();
    scratch.write("docs/a.txt", b"only copy").await;
    scratch.write("docs/a.txt", b"replaced").await;
    scratch.write("docs/b.txt", b"other").await;
    let folder = scratch.path("docs");
    assert_eq!(
        versioned.versioning(&folder).await.unwrap(),
        Versioning::Off
    );
    assert!(!Versioning::Off.keeps_versions());
    // The top of the connection lists buckets, which keep nothing themselves.
    assert_eq!(versioned.versioning("/").await.unwrap(), Versioning::Off);

    // Without versions a delete removes the file: nothing is hidden, nothing comes back.
    scratch.delete("docs/b.txt").await;
    assert_eq!(
        scratch.listed("docs").await,
        (vec!["a.txt".to_string()], Vec::new())
    );
    assert_eq!(
        scratch.listed("").await,
        (vec!["docs".to_string()], Vec::new())
    );
    assert!(versioned.deleted_under(&folder).await.unwrap().is_empty());
    assert!(scratch.history("docs/b.txt").await.is_empty());
    // The file itself is all there is of it.
    let history = scratch.history("docs/a.txt").await;
    assert_eq!(history.len(), 1);
    assert!(history[0].latest && history[0].size == 8);
    assert_eq!(history[0].version_id, "null");

    scratch.close().await;
}

#[tokio::test]
async fn pages_through_long_histories_and_folders() {
    let Some(scratch) = Scratch::open("pages").await else {
        return;
    };
    scratch.enable_versioning();
    // More versions of one file than a page holds, so the listing continues by version id.
    let versions_of_one: usize = 1_003;
    let key = "busy/counter.txt";
    // One after another: the test server loses versions of a key written at the same time.
    for number in 0..versions_of_one {
        scratch
            .write(key, format!("write {number}").as_bytes())
            .await;
    }
    // A neighbour that starts with the same name must not be taken for it.
    scratch.write("busy/counter.txt.bak", b"backup").await;
    let history = scratch.history(key).await;
    assert_eq!(history.len(), versions_of_one);
    assert_eq!(
        ids(&history).into_iter().collect::<HashSet<_>>().len(),
        versions_of_one
    );
    assert_eq!(history.iter().filter(|version| version.latest).count(), 1);
    assert!(history[0].latest);
    assert_eq!(scratch.history("busy/counter.txt.bak").await.len(), 1);

    // More deleted files in one folder than a page holds, so the listing continues by key.
    let deleted_files: usize = 1_010;
    stream::iter(0..deleted_files)
        .for_each_concurrent(8, |number| {
            let scratch = &scratch;
            async move {
                scratch
                    .write(&format!("many/file-{number:04}.txt"), b"x")
                    .await;
            }
        })
        .await;
    scratch.write("many/kept.txt", b"kept").await;
    scratch.delete("many").await;
    scratch.write("many/kept.txt", b"kept again").await;
    let (live, deleted) = scratch.listed("many").await;
    assert_eq!(live, ["kept.txt"]);
    assert_eq!(deleted.len(), deleted_files);
    assert_eq!(deleted[0], "file-0000.txt");
    assert_eq!(deleted[deleted_files - 1], "file-1009.txt");
    let hidden = scratch
        .versioned()
        .deleted_under(&scratch.path("many"))
        .await
        .unwrap();
    assert_eq!(
        hidden
            .iter()
            .filter(|file| file.newest_version().is_some())
            .count(),
        deleted_files
    );

    scratch.close().await;
}
