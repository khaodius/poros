//! End-to-end tests of searching below a folder, against live servers. Each test is skipped
//! unless the servers it needs are configured, as in `vault_servers.rs`:
//!
//! - `POROS_TEST_SSH_PORT`: SFTP with a shell, where the server's `find` does the search.
//! - `POROS_TEST_SFTP_ONLY_USER`: an account on that server limited to SFTP, where folders
//!   are listed instead.
//! - `POROS_TEST_FTP_PORT`, `POROS_TEST_WEBDAV_PORT`, `POROS_TEST_S3_PORT`: FTP, WebDAV and
//!   an S3-compatible service.
//!
//! Every test makes a small tree in a folder of its own and removes it again.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::cloud::s3::{Addressing, S3Options};
use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::file_ops::{Failure, Location};
use poros_lib::model::{EntryKind, FileEntry, LinkTarget};
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::search::{
    Engine, Engines, KindFilter, Outcome, PatternKind, SearchManager, SearchReport, SearchRequest,
    SearchSink, SearchSummary,
};
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};

const OWNER: &str = "main";
const VAULT_PASSWORD: &str = "a vault's p\u{e4}ssword";
/// A time long before the tests run, for the one file that is old.
const LONG_AGO: i64 = 1_000_000_000;

#[derive(Clone)]
struct Server {
    protocol: Protocol,
    port: u16,
    user: String,
    password: String,
    /// What the account is, where two share a server.
    label: &'static str,
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
        label: protocol.display_name(),
    })
}

fn shell_server() -> Option<Server> {
    server("POROS_TEST_SSH_PORT", Protocol::Sftp)
}

/// The account that may only use SFTP, so nothing can run `find` for it.
fn sftp_only_server() -> Option<Server> {
    let user = std::env::var("POROS_TEST_SFTP_ONLY_USER").ok()?;
    Some(Server {
        user,
        label: "SFTP without a shell",
        ..shell_server()?
    })
}

fn every_server() -> Vec<Server> {
    [
        shell_server(),
        sftp_only_server(),
        server("POROS_TEST_FTP_PORT", Protocol::Ftp),
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

async fn write_remote(files: &Arc<dyn RemoteFileSystem>, path: &str, size: usize) {
    let contents = vec![b'x'; size];
    let mut stream = files
        .clone()
        .open_write(WriteRequest {
            path: path.to_string(),
            offset: 0,
            size: size as u64,
            modified: None,
            permissions: None,
        })
        .await
        .unwrap();
    stream.write(Bytes::from(contents)).await.unwrap();
    stream.finish().await.unwrap();
}

/// A session with a scratch folder (a bucket on S3) that is removed when the test is done.
struct Scratch {
    manager: Arc<SessionManager>,
    _config: tempfile::TempDir,
    session: SessionInfo,
    files: Arc<dyn RemoteFileSystem>,
    /// The scratch folder, as the server names it.
    root: String,
    label: &'static str,
}

impl Scratch {
    async fn open(server: &Server, name: &str) -> Self {
        let (config, manager) = new_manager();
        let session = connect(&manager, server).await;
        let files = manager.get(&session.id).await.unwrap().files();
        let folder = format!("poros-search-{name}-{}", std::process::id());
        let existing = remote_path::join(&session.home, &folder);
        if files.stat(&existing).await.unwrap().is_some() {
            files.delete(std::slice::from_ref(&existing)).await.unwrap();
        }
        let made = files.make_dir("~", &folder).await.unwrap();
        let root = files.list_dir(&made).await.unwrap().path;
        Self {
            manager,
            _config: config,
            session,
            files,
            root,
            label: server.label,
        }
    }

    fn path(&self, below: &str) -> String {
        format!("{}/{below}", self.root)
    }

    /// Makes the folders on the way to `below` and the file itself.
    async fn write(&self, below: &str, size: usize) {
        if let Some((folder, _)) = below.rsplit_once('/') {
            self.folder(folder).await;
        }
        write_remote(&self.files, &self.path(below), size).await;
    }

    async fn folder(&self, below: &str) {
        let mut path = self.root.clone();
        for name in below.split('/') {
            path = remote_path::join(&path, name);
            self.files.ensure_dir(&path).await.unwrap();
        }
    }

    /// The tree every test searches:
    ///
    /// ```text
    /// report.log              10 bytes, changed long ago where the server keeps times
    /// Notes.TXT            2,000 bytes
    /// logs/app.log           300 bytes
    /// logs/.hidden.log         8 bytes
    /// logs/archive/app.log.1  5,000 bytes
    /// logs/archive/deep/trace.LOG  1 byte
    /// .cache/hidden.log        7 bytes
    /// empty/
    /// ```
    async fn with_tree(self) -> Self {
        self.write("report.log", 10).await;
        self.write("Notes.TXT", 2_000).await;
        self.write("logs/app.log", 300).await;
        self.write("logs/.hidden.log", 8).await;
        self.write("logs/archive/app.log.1", 5_000).await;
        self.write("logs/archive/deep/trace.LOG", 1).await;
        self.write(".cache/hidden.log", 7).await;
        self.folder("empty").await;
        // Servers that keep no times of their own choosing say so or ignore this.
        let _ = self
            .files
            .set_attributes(&self.path("report.log"), Some(LONG_AGO), None)
            .await;
        self
    }

    fn searches(&self, engines: Engines) -> SearchManager {
        SearchManager::with_engines(self.manager.clone(), engines)
    }

    fn request(&self, pattern: &str) -> SearchRequest {
        SearchRequest {
            search_id: format!("search-{}", std::process::id()),
            location: Location::Remote {
                session_id: self.session.id.clone(),
            },
            folder: self.root.clone(),
            pattern: pattern.to_string(),
            pattern_kind: PatternKind::Text,
            case_sensitive: false,
            kind: KindFilter::Any,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            max_depth: None,
            hidden: false,
            follow_links: false,
            limit: None,
        }
    }

    async fn search(&self, searches: &SearchManager, request: SearchRequest) -> Found {
        let reports: Arc<Mutex<Vec<SearchReport>>> = Arc::default();
        let sink_reports = reports.clone();
        let sink: SearchSink = Arc::new(move |report| sink_reports.lock().unwrap().push(report));
        let summary = searches
            .run(request, sink)
            .await
            .unwrap_or_else(|error| panic!("{}: {}", self.label, error.message));
        let reports = std::mem::take(&mut *reports.lock().unwrap());
        let entries: Vec<FileEntry> = reports
            .iter()
            .flat_map(|report| report.entries.clone())
            .collect();
        let prefix = format!("{}/", self.root);
        let paths: Vec<String> = entries
            .iter()
            .map(|entry| {
                entry
                    .path
                    .strip_prefix(&prefix)
                    .unwrap_or_else(|| {
                        panic!("{}: {} is outside the search", self.label, entry.path)
                    })
                    .to_string()
            })
            .collect();
        let found: BTreeSet<String> = paths.iter().cloned().collect();
        assert_eq!(
            found.len(),
            paths.len(),
            "{}: found twice: {paths:?}",
            self.label
        );
        assert_eq!(summary.found as usize, paths.len(), "{}", self.label);
        let start = reports.iter().find_map(|report| report.start.clone());
        Found {
            start,
            paths: found,
            entries,
            unreadable: reports
                .into_iter()
                .flat_map(|report| report.unreadable)
                .collect(),
            summary,
        }
    }

    /// When the server says the old file was changed. Servers that keep the time a file is
    /// given say long ago, some of them to the minute; the others say just now.
    async fn time_of_old_file(&self) -> i64 {
        let listing = self.files.list_dir(&self.root).await.unwrap();
        listing
            .entries
            .iter()
            .find(|entry| entry.name == "report.log")
            .and_then(|entry| entry.modified)
            .unwrap_or_else(|| panic!("{}: report.log has no time", self.label))
    }

    async fn remove(self) {
        self.files
            .delete(std::slice::from_ref(&self.root))
            .await
            .unwrap_or_else(|error| panic!("{}: {}", self.label, error.message));
    }
}

struct Found {
    /// Paths below the scratch folder.
    paths: BTreeSet<String>,
    entries: Vec<FileEntry>,
    unreadable: Vec<Failure>,
    summary: SearchSummary,
    /// The start folder as its listing named it, when not as asked for.
    start: Option<String>,
}

impl Found {
    fn entry(&self, name: &str) -> &FileEntry {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("{name} was not found"))
    }
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

/// The engines to check on a server, with the one a search is expected to say it used.
fn engines_for(server: &Server) -> Vec<(Engines, Engine)> {
    let has_shell = server.protocol == Protocol::Sftp && server.label != "SFTP without a shell";
    if has_shell {
        vec![
            (Engines::Automatic, Engine::Find),
            (Engines::WalkerOnly, Engine::Walker),
        ]
    } else {
        vec![(Engines::Automatic, Engine::Walker)]
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_kinds_sizes_and_depth_narrow_a_search_on_every_server() {
    for server in every_server() {
        let scratch = Scratch::open(&server, "filters").await.with_tree().await;
        for (engines, engine) in engines_for(&server) {
            let label = format!("{} ({engine:?})", server.label);
            let searches = scratch.searches(engines);

            let logs = scratch.search(&searches, scratch.request("*.log")).await;
            assert_eq!(logs.summary.engine, engine, "{label}");
            assert_eq!(logs.summary.outcome, Outcome::Finished, "{label}");
            assert_eq!(
                logs.paths,
                set(&[
                    "report.log",
                    "logs/app.log",
                    "logs/.hidden.log",
                    "logs/archive/deep/trace.LOG"
                ]),
                "{label}: hidden folders are skipped, hidden files in other folders are not"
            );
            assert_eq!(
                logs.summary.folders, 5,
                "{label}: the start folder, logs, archive, deep and empty; not .cache"
            );
            let app_log = logs.entry("app.log");
            assert_eq!(app_log.kind, EntryKind::File, "{label}");
            assert_eq!(app_log.size, 300, "{label}");
            assert_eq!(app_log.path, scratch.path("logs/app.log"), "{label}");
            assert!(logs.unreadable.is_empty(), "{label}");

            let exact_case = scratch
                .search(
                    &searches,
                    SearchRequest {
                        case_sensitive: true,
                        hidden: true,
                        ..scratch.request("*.log")
                    },
                )
                .await;
            assert_eq!(
                exact_case.paths,
                set(&[
                    "report.log",
                    "logs/app.log",
                    "logs/.hidden.log",
                    ".cache/hidden.log"
                ]),
                "{label}"
            );

            let anywhere = scratch.search(&searches, scratch.request("APP")).await;
            assert_eq!(
                anywhere.paths,
                set(&["logs/app.log", "logs/archive/app.log.1"]),
                "{label}: plain text is found anywhere in a name"
            );

            let expression = scratch
                .search(
                    &searches,
                    SearchRequest {
                        pattern_kind: PatternKind::Regex,
                        case_sensitive: true,
                        ..scratch.request(r"^(app|trace)\.(log|LOG)(\.\d+)?$")
                    },
                )
                .await;
            assert_eq!(
                expression.paths,
                set(&[
                    "logs/app.log",
                    "logs/archive/app.log.1",
                    "logs/archive/deep/trace.LOG"
                ]),
                "{label}"
            );

            let folders = scratch
                .search(
                    &searches,
                    SearchRequest {
                        kind: KindFilter::Folders,
                        ..scratch.request("")
                    },
                )
                .await;
            assert_eq!(
                folders.paths,
                set(&[
                    ".cache",
                    "empty",
                    "logs",
                    "logs/archive",
                    "logs/archive/deep"
                ]),
                "{label}"
            );
            assert!(
                folders.entries.iter().all(FileEntry::is_dir_like),
                "{label}"
            );

            let files = scratch
                .search(
                    &searches,
                    SearchRequest {
                        kind: KindFilter::Files,
                        ..scratch.request("log")
                    },
                )
                .await;
            assert_eq!(
                files.paths,
                set(&[
                    "report.log",
                    "logs/app.log",
                    "logs/.hidden.log",
                    "logs/archive/app.log.1",
                    "logs/archive/deep/trace.LOG"
                ]),
                "{label}: the folder named logs is not a file"
            );

            let sized = scratch
                .search(
                    &searches,
                    SearchRequest {
                        min_size: Some(300),
                        max_size: Some(2_000),
                        ..scratch.request("")
                    },
                )
                .await;
            assert_eq!(sized.paths, set(&["logs/app.log", "Notes.TXT"]), "{label}");

            let large = scratch
                .search(
                    &searches,
                    SearchRequest {
                        min_size: Some(2_001),
                        ..scratch.request("")
                    },
                )
                .await;
            assert_eq!(large.paths, set(&["logs/archive/app.log.1"]), "{label}");

            let top = scratch
                .search(
                    &searches,
                    SearchRequest {
                        max_depth: Some(1),
                        ..scratch.request("log")
                    },
                )
                .await;
            assert_eq!(top.paths, set(&["report.log", "logs"]), "{label}");
            assert_eq!(top.summary.folders, 1, "{label}: the start folder alone");

            let two_levels = scratch
                .search(
                    &searches,
                    SearchRequest {
                        max_depth: Some(2),
                        ..scratch.request("log")
                    },
                )
                .await;
            assert_eq!(
                two_levels.paths,
                set(&["report.log", "logs", "logs/app.log", "logs/.hidden.log"]),
                "{label}"
            );
            assert_eq!(
                two_levels.summary.folders, 3,
                "{label}: the start folder, logs and empty"
            );

            let below = scratch
                .search(
                    &searches,
                    SearchRequest {
                        folder: scratch.path("logs/archive"),
                        ..scratch.request("*")
                    },
                )
                .await;
            assert_eq!(
                below.paths,
                set(&[
                    "logs/archive/app.log.1",
                    "logs/archive/deep",
                    "logs/archive/deep/trace.LOG"
                ]),
                "{label}: a search starts where it is told to"
            );

            let capped = scratch
                .search(
                    &searches,
                    SearchRequest {
                        limit: Some(3),
                        hidden: true,
                        ..scratch.request("")
                    },
                )
                .await;
            assert_eq!(capped.paths.len(), 3, "{label}");
            assert_eq!(capped.summary.outcome, Outcome::Capped, "{label}");
        }
        scratch.remove().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dates_narrow_a_search_on_every_server() {
    for server in every_server() {
        let scratch = Scratch::open(&server, "dates").await.with_tree().await;
        let old_time = scratch.time_of_old_file().await;
        let an_hour_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 3_600;
        for (engines, engine) in engines_for(&server) {
            let label = format!("{} ({engine:?})", server.label);
            let searches = scratch.searches(engines);
            let files_changed = |after: Option<i64>, before: Option<i64>| SearchRequest {
                kind: KindFilter::Files,
                modified_after: after,
                modified_before: before,
                ..scratch.request("*.log")
            };

            let recent = scratch
                .search(&searches, files_changed(Some(an_hour_ago), None))
                .await;
            let old = scratch
                .search(&searches, files_changed(None, Some(an_hour_ago)))
                .await;
            let fresh = set(&[
                "logs/app.log",
                "logs/.hidden.log",
                "logs/archive/deep/trace.LOG",
            ]);
            if old_time < an_hour_ago {
                assert_eq!(recent.paths, fresh, "{label}");
                assert_eq!(old.paths, set(&["report.log"]), "{label}");
                let exactly = scratch
                    .search(&searches, files_changed(Some(old_time), Some(old_time)))
                    .await;
                assert_eq!(
                    exactly.paths,
                    set(&["report.log"]),
                    "{label}: both ends count"
                );
                let just_after = scratch
                    .search(
                        &searches,
                        files_changed(Some(old_time + 1), Some(an_hour_ago)),
                    )
                    .await;
                assert!(
                    just_after.paths.is_empty(),
                    "{label}: {:?}",
                    just_after.paths
                );
                assert_eq!(old.entry("report.log").modified, Some(old_time), "{label}");
            } else {
                let mut everything = fresh.clone();
                everything.insert("report.log".to_string());
                assert_eq!(recent.paths, everything, "{label}");
                assert!(old.paths.is_empty(), "{label}: {:?}", old.paths);
            }
        }
        scratch.remove().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_search_stops_at_once_when_cancelled_on_every_server() {
    for server in every_server() {
        let scratch = Scratch::open(&server, "cancel").await.with_tree().await;
        for index in 0..12 {
            scratch
                .write(&format!("more/folder-{index}/file.log"), 1)
                .await;
        }
        for (engines, engine) in engines_for(&server) {
            let label = format!("{} ({engine:?})", server.label);
            let searches = Arc::new(scratch.searches(engines));
            let request = SearchRequest {
                hidden: true,
                ..scratch.request("")
            };
            let search_id = request.search_id.clone();
            // The search is stopped as soon as it is known under its name.
            let stopper = {
                let searches = searches.clone();
                tokio::spawn(async move {
                    loop {
                        searches.cancel(&search_id);
                        tokio::time::sleep(Duration::from_micros(200)).await;
                    }
                })
            };
            let started = Instant::now();
            let stopped = scratch.search(&searches, request).await;
            stopper.abort();
            assert_eq!(stopped.summary.outcome, Outcome::Stopped, "{label}");
            assert!(stopped.paths.len() < 37, "{label}: {}", stopped.paths.len());
            assert!(started.elapsed() < Duration::from_secs(5), "{label}");

            // Nothing is left behind: the same name searches again, to the end.
            let again = scratch
                .search(
                    &searches,
                    SearchRequest {
                        hidden: true,
                        ..scratch.request("")
                    },
                )
                .await;
            assert_eq!(again.summary.outcome, Outcome::Finished, "{label}");
            assert_eq!(again.paths.len(), 37, "{label}: {:?}", again.paths);
        }
        scratch.remove().await;
    }
}

/// Links, names that would break a careless command line, and a folder that may not be read:
/// all things only an SFTP server can be given. Both accounts and both engines agree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn links_hostile_names_and_unreadable_folders_over_sftp() {
    for server in [shell_server(), sftp_only_server()].into_iter().flatten() {
        let scratch = Scratch::open(&server, "sftp").await.with_tree().await;
        let session = scratch.manager.get(&scratch.session.id).await.unwrap();
        let fs = session.sftp().unwrap();

        let hostile = [
            "odd/two\nlines.log",
            "odd/it's a \"quoted\".log",
            "odd/-rf.log",
            "odd/--help.log",
            "odd/$(touch pwned).log",
            "odd/`touch pwned`.log",
            "odd/semi;colon & more.log",
            "odd/ spaces .log",
            "odd/star*.log",
            "odd/back\\slash.log",
            "odd/caf\u{e9}.LOG",
        ];
        for path in hostile {
            scratch.write(path, 3).await;
        }

        // A link back to the start, a link to a file, and one to nothing.
        fs.make_symlink(&scratch.path("logs/archive/back-to-start"), &scratch.root)
            .await
            .unwrap();
        fs.make_symlink(&scratch.path("latest.log"), "logs/app.log")
            .await
            .unwrap();
        fs.make_symlink(&scratch.path("dangling.log"), "nowhere")
            .await
            .unwrap();

        scratch.write("locked/inside.log", 4).await;
        fs.set_attributes(&scratch.path("locked"), None, Some(0o000))
            .await
            .unwrap();

        for (engines, engine) in engines_for(&server) {
            let label = format!("{} ({engine:?})", server.label);
            let searches = scratch.searches(engines);

            let logs = scratch.search(&searches, scratch.request("*.log")).await;
            assert_eq!(logs.summary.engine, engine, "{label}");
            let mut expected = set(&[
                "report.log",
                "logs/app.log",
                "logs/.hidden.log",
                "logs/archive/deep/trace.LOG",
                "latest.log",
                "dangling.log",
            ]);
            expected.extend(hostile.iter().map(|path| path.to_string()));
            assert_eq!(logs.paths, expected, "{label}");
            assert_eq!(logs.entry("two\nlines.log").size, 3, "{label}");

            assert_eq!(logs.summary.unreadable, 1, "{label}");
            assert_eq!(logs.unreadable.len(), 1, "{label}");
            assert!(
                logs.unreadable[0].path.ends_with("/locked"),
                "{label}: {:?}",
                logs.unreadable[0]
            );

            let link = logs.entry("latest.log");
            assert_eq!(link.kind, EntryKind::Symlink, "{label}");
            assert_eq!(link.link_target, Some(LinkTarget::File), "{label}");
            assert_eq!(link.size, 300, "{label}: a link shows the size of its file");
            assert_eq!(
                logs.entry("dangling.log").link_target,
                Some(LinkTarget::Broken),
                "{label}"
            );

            let by_size = scratch
                .search(
                    &searches,
                    SearchRequest {
                        min_size: Some(300),
                        max_size: Some(300),
                        ..scratch.request("")
                    },
                )
                .await;
            assert_eq!(
                by_size.paths,
                set(&["logs/app.log", "latest.log"]),
                "{label}: a link counts with the size of its file"
            );

            // One name of each kind of trouble, looked for by name.
            for (pattern, path) in [
                ("it's", "odd/it's a \"quoted\".log"),
                ("-rf", "odd/-rf.log"),
                ("$(touch", "odd/$(touch pwned).log"),
                ("`touch", "odd/`touch pwned`.log"),
                ("two\nlines", "odd/two\nlines.log"),
                ("back\\slash", "odd/back\\slash.log"),
                ("caf\u{c9}", "odd/caf\u{e9}.LOG"),
                ("; rm -rf ~", ""),
            ] {
                let found = scratch.search(&searches, scratch.request(pattern)).await;
                let expected = match path {
                    "" => BTreeSet::new(),
                    path => set(&[path]),
                };
                assert_eq!(found.paths, expected, "{label}: {pattern:?}");
            }
            let starred = scratch
                .search(&searches, scratch.request("star?.log"))
                .await;
            assert_eq!(starred.paths, set(&["odd/star*.log"]), "{label}");

            let folder_links = scratch
                .search(
                    &searches,
                    SearchRequest {
                        kind: KindFilter::Folders,
                        ..scratch.request("back")
                    },
                )
                .await;
            assert_eq!(
                folder_links.paths,
                set(&["logs/archive/back-to-start"]),
                "{label}: a link to a folder is a folder"
            );

            // Following links enters the link back to the start, and comes out again.
            let following = scratch
                .search(
                    &searches,
                    SearchRequest {
                        follow_links: true,
                        ..scratch.request("*.TXT")
                    },
                )
                .await;
            assert_eq!(following.paths, set(&["Notes.TXT"]), "{label}");
            assert_eq!(following.summary.outcome, Outcome::Finished, "{label}");
        }

        // Searched through a link, the results are below the folder's real path, which the
        // search names so that the page can tell where each result is.
        fs.make_symlink(&scratch.path("to-logs"), &scratch.path("logs"))
            .await
            .unwrap();
        for (engines, engine) in engines_for(&server) {
            let label = format!("{} ({engine:?})", server.label);
            let searches = scratch.searches(engines);
            let through = scratch
                .search(
                    &searches,
                    SearchRequest {
                        folder: scratch.path("to-logs"),
                        ..scratch.request("app")
                    },
                )
                .await;
            assert_eq!(through.summary.engine, engine, "{label}");
            assert_eq!(
                through.start.as_deref(),
                Some(scratch.path("logs").as_str()),
                "{label}"
            );
            assert_eq!(
                through.paths,
                set(&["logs/app.log", "logs/archive/app.log.1"]),
                "{label}"
            );
        }

        assert!(
            fs.lstat_entry(&scratch.path("pwned"))
                .await
                .unwrap()
                .is_none()
                && fs
                    .stat(&remote_path::join(&scratch.session.home, "pwned"))
                    .await
                    .unwrap()
                    .is_none(),
            "{}: a name ran as a command",
            server.label
        );
        fs.set_attributes(&scratch.path("locked"), None, Some(0o755))
            .await
            .unwrap();
        scratch.remove().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_folder_that_is_not_there_is_an_error_on_every_server() {
    for server in every_server() {
        let scratch = Scratch::open(&server, "missing").await;
        for (engines, engine) in engines_for(&server) {
            let label = format!("{} ({engine:?})", server.label);
            let searches = scratch.searches(engines);
            let sink: SearchSink = Arc::new(|_| {});
            let result = searches
                .run(
                    SearchRequest {
                        folder: scratch.path("nowhere"),
                        ..scratch.request("")
                    },
                    sink,
                )
                .await;
            match (server.protocol, result) {
                // S3 stores names, not folders: a folder nothing is in is an empty one.
                (Protocol::S3, Ok(summary)) => assert_eq!(summary.found, 0, "{label}"),
                (_, Ok(_)) => panic!("{label}: searched a folder that is not there"),
                (_, Err(error)) => {
                    assert_eq!(
                        error.kind,
                        ErrorKind::NotFound,
                        "{label}: {}",
                        error.message
                    );
                }
            }
        }
        scratch.remove().await;
    }
}

/// S3 lists everything below a folder at once, by the folder's tidied name: a start folder
/// written another way finds the same, and is reported by that name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_folder_written_another_way_finds_the_same_on_s3() {
    let Some(server) = server("POROS_TEST_S3_PORT", Protocol::S3) else {
        return;
    };
    let scratch = Scratch::open(&server, "written").await.with_tree().await;
    let searches = scratch.searches(Engines::Automatic);
    let plain = scratch.search(&searches, scratch.request("log")).await;
    assert_eq!(
        plain.paths,
        set(&[
            "logs",
            "logs/.hidden.log",
            "logs/app.log",
            "logs/archive/app.log.1",
            "logs/archive/deep/trace.LOG",
            "report.log",
        ])
    );
    assert_eq!(plain.start, None);
    let roundabout = scratch
        .search(
            &searches,
            SearchRequest {
                folder: format!("{}/", scratch.root),
                ..scratch.request("log")
            },
        )
        .await;
    assert_eq!(roundabout.paths, plain.paths);
    assert_eq!(roundabout.start.as_deref(), Some(scratch.root.as_str()));
    scratch.remove().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn results_are_looked_at_again_on_every_server() {
    for server in every_server() {
        let scratch = Scratch::open(&server, "recheck").await.with_tree().await;
        let searches = scratch.searches(Engines::Automatic);
        let location = Location::Remote {
            session_id: scratch.session.id.clone(),
        };
        let found = scratch.search(&searches, scratch.request("app")).await;
        let paths: Vec<String> = found
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert_eq!(paths.len(), 2, "{}", server.label);

        let unchanged = searches.recheck(&location, paths.clone()).await.unwrap();
        assert_eq!(unchanged.entries.len(), 2, "{}", server.label);
        assert!(unchanged.missing.is_empty(), "{}", server.label);

        scratch
            .files
            .delete(&[scratch.path("logs/archive")])
            .await
            .unwrap();
        scratch.write("logs/app.log", 512).await;
        let changed = searches.recheck(&location, paths).await.unwrap();
        assert_eq!(
            changed.missing,
            [scratch.path("logs/archive/app.log.1")],
            "{}",
            server.label
        );
        assert_eq!(changed.entries.len(), 1, "{}", server.label);
        assert_eq!(changed.entries[0].path, scratch.path("logs/app.log"));
        assert_eq!(changed.entries[0].size, 512, "{}", server.label);
        scratch.remove().await;
    }
}

/// Names in a vault are encrypted on the server, so nothing there can search them: folders
/// are listed through the vault, whatever the server is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vault_is_searched_by_its_real_names_on_every_server() {
    for server in every_server() {
        let scratch = Scratch::open(&server, "vault").await;
        let vault = scratch
            .manager
            .create_vault(
                &scratch.session.id,
                &scratch.root,
                "Vault",
                VAULT_PASSWORD.to_string(),
                OWNER,
            )
            .await
            .unwrap_or_else(|error| panic!("{}: {}", server.label, error.message));
        let files = scratch.manager.get(&vault.id).await.unwrap().files();
        let start = files.list_dir(&vault.home).await.unwrap().path;
        let inside = |below: &str| remote_path::join(&start, below);
        for folder in ["letters", "letters/2024", ".private"] {
            files.ensure_dir(&inside(folder)).await.unwrap();
        }
        write_remote(&files, &inside("letters/2024/tax return.pdf"), 1_500).await;
        write_remote(&files, &inside("letters/note.txt"), 20).await;
        write_remote(&files, &inside(".private/tax secrets.pdf"), 30).await;

        let searches = scratch.searches(Engines::Automatic);
        let request = |pattern: &str| SearchRequest {
            location: Location::Remote {
                session_id: vault.id.clone(),
            },
            folder: start.clone(),
            ..scratch.request(pattern)
        };
        let paths_of = |reports: &Mutex<Vec<SearchReport>>| -> BTreeSet<String> {
            reports
                .lock()
                .unwrap()
                .iter()
                .flat_map(|report| report.entries.iter().map(|entry| entry.path.clone()))
                .collect()
        };
        let run = |request: SearchRequest| {
            let searches = &searches;
            async move {
                let reports: Arc<Mutex<Vec<SearchReport>>> = Arc::default();
                let sink_reports = reports.clone();
                let sink: SearchSink =
                    Arc::new(move |report| sink_reports.lock().unwrap().push(report));
                let summary = searches
                    .run(request, sink)
                    .await
                    .unwrap_or_else(|error| panic!("{}: {}", server.label, error.message));
                (summary, paths_of(&reports))
            }
        };

        let (summary, found) = run(request("tax*.pdf")).await;
        assert_eq!(summary.engine, Engine::Walker, "{}", server.label);
        assert_eq!(
            found,
            BTreeSet::from([inside("letters/2024/tax return.pdf")]),
            "{}",
            server.label
        );

        let (_, found) = run(SearchRequest {
            hidden: true,
            min_size: Some(25),
            ..request("")
        })
        .await;
        assert_eq!(
            found,
            BTreeSet::from([
                inside("letters/2024/tax return.pdf"),
                inside(".private/tax secrets.pdf")
            ]),
            "{}",
            server.label
        );

        let (_, found) = run(SearchRequest {
            kind: KindFilter::Folders,
            max_depth: Some(1),
            ..request("")
        })
        .await;
        assert_eq!(
            found,
            BTreeSet::from([inside("letters"), inside(".private")]),
            "{}",
            server.label
        );
        scratch.remove().await;
    }
}
