//! Every kind of command Poros runs on a server, for an account whose login shell is fish, on
//! names that fish and other login shells that are not POSIX read otherwise than `sh` does.
//! Skipped unless `POROS_TEST_SSH_PORT` and `POROS_TEST_FISH_USER` are set; the password is
//! `POROS_TEST_FISH_PASSWORD`, `poros-pass` when unset.
//!
//! The account's login shell must be a wrapper that appends each command line it is given
//! (`-c LINE`), followed by a NUL, to `~/.poros-exec-lines`, and then runs fish with the same
//! arguments. Each test checks that the right files changed, that nothing a name holds ran, and
//! that the login shell was only ever given the fixed `sh -s`, apart from commands the user
//! wrote. Tests take turns, as they share the account's record of lines.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use poros_lib::automation::remote_command::{self, OutputStream};
use poros_lib::events::Events;
use poros_lib::file_ops::archive::ExtractInto;
use poros_lib::file_ops::{
    DeleteRequest, ExtractRequest, FileOperations, Location, Method, Mode, ModeChange,
    MoveCopyRequest, PermissionRequest, ServerFeatures,
};
use poros_lib::model::EntryKind;
use poros_lib::protocol::Protocol;
use poros_lib::search::{
    Engine, Engines, KindFilter, PatternKind, SearchManager, SearchReport, SearchRequest,
    SearchSink,
};
use poros_lib::session::{Session, SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::sftp::RemoteFs;
use poros_lib::ssh::exec::{self, shell_quote, CommandLine};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::sync::{
    CompareMode, SyncAction, SyncDirection, SyncManager, SyncOptions, SyncReason, SyncRequest,
};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobSnapshot, JobState, TransferItem, TransferList,
    TransferManager,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const OWNER: &str = "main";
/// What a name that got out of its quotes makes, in the folder its command ran in.
const CANARY: &str = "poros-pwned";
/// Where the account's login shell writes the lines it is given.
const EXEC_LINES: &str = ".poros-exec-lines";
/// The one line Poros gives the login shell for its own commands.
const SCRIPT_LINE: &str = "sh -s";
const OUTPUT_LIMIT: usize = 64 * 1024;

/// Names someone else on the server can give files. Quoted for `sh`, fish runs the `touch` in
/// the second, misreads the third and cannot read the fourth; the others hold what csh,
/// cmd.exe or a careless `sh` line would expand or run, or read as options.
const HOSTILE_NAMES: [&str; 16] = [
    "it's",
    r"x\';touch poros-pwned;echo \'",
    r"back\\slash",
    r"trailing\",
    "$(touch poros-pwned)",
    "`touch poros-pwned`",
    "!touch poros-pwned",
    "two\nlines; touch poros-pwned",
    "a; touch poros-pwned",
    "-rf",
    "\"quoted\" & touch poros-pwned",
    "%PATH%&touch poros-pwned",
    "tab\there",
    "*",
    "{a,b}",
    "~root",
];

/// The folder name most tests work in.
const HOSTILE_FOLDER: &str = r"dir \';touch poros-pwned;echo \' $(touch poros-pwned)";

/// The account's record of lines is one file, so tests run one after another.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Account {
    port: u16,
    user: String,
    password: String,
}

fn account() -> Option<Account> {
    let port = std::env::var("POROS_TEST_SSH_PORT").ok()?.parse().ok()?;
    Some(Account {
        port,
        user: std::env::var("POROS_TEST_FISH_USER").ok()?,
        password: std::env::var("POROS_TEST_FISH_PASSWORD").unwrap_or_else(|_| "poros-pass".into()),
    })
}

fn profile(account: &Account) -> ConnectProfile {
    ConnectProfile {
        protocol: Protocol::Sftp,
        host: "127.0.0.1".into(),
        port: account.port,
        username: account.user.clone(),
        auth: AuthMethod::Password {
            password: account.password.clone(),
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
    events: Events,
    manager: Arc<SessionManager>,
    session: SessionInfo,
    /// Absolute scratch folder on the server.
    remote: String,
}

impl Fixture {
    async fn new(account: &Account, name: &str) -> Self {
        let known_hosts = tempfile::tempdir().unwrap();
        let events = Events::keeping_startup_log();
        let manager = Arc::new(SessionManager::new(
            known_hosts.path().join("known_hosts"),
            events.clone(),
        ));
        let session = match manager.connect(profile(account), None, OWNER).await {
            Ok(session) => session,
            Err(error) => {
                let host_key = error.host_key.expect("only the host key may be unknown");
                manager
                    .connect(
                        profile(account),
                        Some(HostKeyApproval::of(&host_key, true)),
                        OWNER,
                    )
                    .await
                    .unwrap()
            }
        };
        let fixture = Self {
            _known_hosts: known_hosts,
            local: tempfile::tempdir().unwrap(),
            events,
            manager,
            remote: format!(
                "{}/poros-shell-{name}-{}",
                session.home.trim_end_matches('/'),
                std::process::id()
            ),
            session,
        };
        let fs = fixture.fs().await;
        let _ = fs.delete(std::slice::from_ref(&fixture.remote)).await;
        fs.make_dir_at(&fixture.remote, None).await.unwrap();
        fixture.forget_exec_lines().await;
        fixture
    }

    async fn session(&self) -> Arc<Session> {
        self.manager.get(&self.session.id).await.unwrap()
    }

    async fn fs(&self) -> Arc<RemoteFs> {
        self.session().await.sftp().unwrap().clone()
    }

    fn path(&self, relative: &str) -> String {
        format!("{}/{relative}", self.remote)
    }

    fn location(&self) -> Location {
        Location::Remote {
            session_id: self.session.id.clone(),
        }
    }

    fn operations(&self, features: ServerFeatures) -> FileOperations {
        FileOperations::with_features(self.manager.clone(), Events::default(), features)
    }

    async fn mkdir(&self, relative: &str) {
        self.fs()
            .await
            .make_dir_at(&self.path(relative), None)
            .await
            .unwrap();
    }

    async fn write(&self, relative: &str, contents: &[u8]) {
        let fs = self.fs().await;
        let path = self.path(relative);
        let handle = fs.open_for_write(&path, true, Some(0o644)).await.unwrap();
        let chunk = fs.write_size(32 * 1024) as usize;
        for (index, piece) in contents.chunks(chunk).enumerate() {
            fs.write_chunk(&handle, (index * chunk) as u64, piece.to_vec())
                .await
                .unwrap();
        }
        fs.close_handle(handle).await.unwrap();
    }

    async fn read(&self, relative: &str) -> Vec<u8> {
        self.fs()
            .await
            .read_to_end(&self.path(relative), u64::MAX)
            .await
            .unwrap_or_else(|error| panic!("{relative:?}: {}", error.message))
    }

    async fn names(&self, relative: &str) -> BTreeSet<String> {
        self.fs()
            .await
            .entries(&self.path(relative))
            .await
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    async fn mode(&self, relative: &str) -> u32 {
        self.fs()
            .await
            .lstat_entry(&self.path(relative))
            .await
            .unwrap()
            .unwrap()
            .permissions
            .unwrap()
            & 0o7777
    }

    fn exec_lines_path(&self) -> String {
        format!("{}/{EXEC_LINES}", self.session.home.trim_end_matches('/'))
    }

    /// The lines the login shell was given since the last call to `forget_exec_lines`.
    async fn exec_lines(&self) -> Vec<String> {
        let Ok(record) = self
            .fs()
            .await
            .read_to_end(&self.exec_lines_path(), u64::MAX)
            .await
        else {
            return Vec::new();
        };
        let mut lines: Vec<String> = String::from_utf8(record)
            .unwrap()
            .split('\0')
            .map(str::to_string)
            .collect();
        assert_eq!(lines.pop().as_deref(), Some(""), "{lines:?}");
        lines
    }

    async fn forget_exec_lines(&self) {
        let _ = self
            .fs()
            .await
            .delete(std::slice::from_ref(&self.exec_lines_path()))
            .await;
    }

    /// That the login shell was given commands, and that each was `sh -s` or one of `written`.
    async fn assert_only_fixed_lines(&self, written: &[&str]) {
        let lines = self.exec_lines().await;
        assert!(!lines.is_empty(), "nothing ran through the login shell");
        let unexpected: Vec<&String> = lines
            .iter()
            .filter(|line| *line != SCRIPT_LINE && !written.contains(&line.as_str()))
            .collect();
        assert!(
            unexpected.is_empty(),
            "the login shell was given {unexpected:?}"
        );
    }

    /// Every file a name that ran as a command would have made: in the scratch folder or below
    /// it, and in the home folder, where commands start.
    async fn canaries(&self) -> Vec<String> {
        let fs = self.fs().await;
        let mut found = Vec::new();
        let mut folders = vec![self.remote.clone()];
        while let Some(folder) = folders.pop() {
            for (name, stat) in fs.entries(&folder).await.unwrap() {
                let path = format!("{folder}/{name}");
                if name == CANARY {
                    found.push(path.clone());
                }
                if stat.kind == EntryKind::Dir {
                    folders.push(path);
                }
            }
        }
        let home = self.session.home.trim_end_matches('/');
        if fs
            .lstat_entry(&format!("{home}/{CANARY}"))
            .await
            .unwrap()
            .is_some()
        {
            found.push(format!("{home}/{CANARY}"));
        }
        found.sort();
        found
    }

    async fn assert_nothing_ran(&self) {
        assert_eq!(self.canaries().await, Vec::<String>::new());
    }

    async fn close(self) {
        let fs = self.fs().await;
        fs.delete(std::slice::from_ref(&self.remote)).await.unwrap();
        let home = self.session.home.trim_end_matches('/');
        let _ = fs.delete(&[format!("{home}/{CANARY}")]).await;
        self.forget_exec_lines().await;
        self.manager.disconnect(&self.session.id).await.unwrap();
    }
}

/// Deterministic bytes that do not compress.
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

fn names_of(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// The names a glob in the command line would have matched, which must be left alone.
const BYSTANDERS: [&str; 4] = ["a", "b", "keep.txt", "root"];

#[tokio::test]
async fn the_account_runs_fish_and_a_quoted_name_escapes_it() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "check").await;
    let name = HOSTILE_NAMES[1];
    fixture.write(name, b"bait").await;
    let session = fixture.session().await;

    // The way names reached the server before: quoted for `sh`, on the line the login shell
    // reads. fish reads the quotes otherwise and runs what the name holds.
    let line = format!(
        "cd {} && ls -d -- {}",
        shell_quote(&fixture.remote),
        shell_quote(name)
    );
    let channel = session.open_command_channel().await.unwrap();
    exec::run_line(channel, &CommandLine::written_by_user(&line), OUTPUT_LIMIT)
        .await
        .unwrap();
    assert_eq!(fixture.exec_lines().await, [line]);
    assert_eq!(fixture.canaries().await, [fixture.path(CANARY)]);
    fixture
        .fs()
        .await
        .delete(&[fixture.path(CANARY)])
        .await
        .unwrap();
    fixture.forget_exec_lines().await;

    // The same command in a script for `sh` gets the name whole.
    let script = format!(
        "cd {} && ls -d -- {}",
        shell_quote(&fixture.remote),
        shell_quote(name)
    );
    let channel = session.open_command_channel().await.unwrap();
    let output = exec::run_script(channel, &script, OUTPUT_LIMIT)
        .await
        .unwrap();
    assert_eq!(output.exit_status, Some(0), "{}", output.error_output);
    assert_eq!(output.output, format!("{name}\n").as_bytes());
    assert_eq!(fixture.exec_lines().await, [SCRIPT_LINE]);
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

#[tokio::test]
async fn deletes_exactly_the_named_files_and_folders() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "delete").await;
    fixture.mkdir(HOSTILE_FOLDER).await;
    for name in HOSTILE_NAMES.iter().chain(&BYSTANDERS) {
        fixture
            .write(&format!("{HOSTILE_FOLDER}/{name}"), b"x")
            .await;
    }
    // A folder of a hostile name, with hostile names inside, goes as a whole.
    let inner = format!("{HOSTILE_FOLDER}/{}", HOSTILE_NAMES[4]);
    fixture
        .fs()
        .await
        .delete(&[fixture.path(&inner)])
        .await
        .unwrap();
    fixture.mkdir(&inner).await;
    for name in HOSTILE_NAMES {
        fixture.write(&format!("{inner}/{name}"), b"inside").await;
    }
    fixture.forget_exec_lines().await;

    let summary = fixture
        .operations(ServerFeatures::default())
        .delete(DeleteRequest {
            operation_id: uuid::Uuid::new_v4().to_string(),
            session_id: fixture.session.id.clone(),
            paths: HOSTILE_NAMES
                .iter()
                .map(|name| fixture.path(&format!("{HOSTILE_FOLDER}/{name}")))
                .collect(),
        })
        .await
        .unwrap();
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_eq!(summary.methods, [Method::Command]);
    assert_eq!(summary.deleted, HOSTILE_NAMES.len() as u64 * 2);
    assert_eq!(fixture.names(HOSTILE_FOLDER).await, names_of(&BYSTANDERS));
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

#[tokio::test]
async fn copies_and_changes_owners_and_modes_of_exactly_the_named_files() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "copy").await;
    fixture.mkdir(HOSTILE_FOLDER).await;
    fixture.mkdir("target").await;
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        fixture
            .write(
                &format!("{HOSTILE_FOLDER}/{name}"),
                &pattern(100, index as u64),
            )
            .await;
    }
    for name in BYSTANDERS {
        fixture
            .write(&format!("{HOSTILE_FOLDER}/{name}"), b"x")
            .await;
        fixture.write(&format!("target/{name}"), b"x").await;
    }
    fixture.forget_exec_lines().await;

    // Without the copy-data extension, files and folders are copied by `cp` on the server.
    let operations = fixture.operations(ServerFeatures {
        copy_data: false,
        commands: true,
    });
    let sources: Vec<String> = HOSTILE_NAMES
        .iter()
        .map(|name| fixture.path(&format!("{HOSTILE_FOLDER}/{name}")))
        .chain([fixture.path(HOSTILE_FOLDER)])
        .collect();
    let summary = operations
        .move_or_copy(MoveCopyRequest {
            operation_id: uuid::Uuid::new_v4().to_string(),
            location: fixture.location(),
            mode: Mode::Copy,
            sources,
            target_directory: fixture.path("target"),
            conflict: Default::default(),
            target_name: None,
        })
        .await
        .unwrap();
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_eq!(summary.methods, [Method::Command]);
    let mut expected = names_of(&HOSTILE_NAMES);
    expected.extend(names_of(&BYSTANDERS));
    expected.insert(HOSTILE_FOLDER.to_string());
    assert_eq!(fixture.names("target").await, expected);
    assert_eq!(
        fixture.names(&format!("target/{HOSTILE_FOLDER}")).await,
        expected
            .iter()
            .filter(|name| *name != HOSTILE_FOLDER)
            .cloned()
            .collect()
    );
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        let copied = fixture.read(&format!("target/{name}")).await;
        assert_eq!(copied, pattern(100, index as u64), "{name:?}");
        let copied = fixture
            .read(&format!("target/{HOSTILE_FOLDER}/{name}"))
            .await;
        assert_eq!(copied, pattern(100, index as u64), "{name:?}");
    }

    // Owners are changed by `chown` and `chgrp` on the server; modes over SFTP.
    let exact = |bits: u32| ModeChange {
        set: bits,
        clear: 0o7777 & !bits,
    };
    let summary = operations
        .set_permissions(PermissionRequest {
            operation_id: uuid::Uuid::new_v4().to_string(),
            session_id: fixture.session.id.clone(),
            paths: HOSTILE_NAMES
                .iter()
                .map(|name| fixture.path(&format!("{HOSTILE_FOLDER}/{name}")))
                .collect(),
            files: exact(0o600),
            folders: exact(0o700),
            recursive: false,
            owner: Some(account.user.clone()),
            group: Some(account.user.clone()),
        })
        .await
        .unwrap();
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_eq!(summary.changed, HOSTILE_NAMES.len() as u64);
    for name in HOSTILE_NAMES {
        assert_eq!(
            fixture.mode(&format!("{HOSTILE_FOLDER}/{name}")).await,
            0o600
        );
    }
    for name in BYSTANDERS {
        assert_eq!(
            fixture.mode(&format!("{HOSTILE_FOLDER}/{name}")).await,
            0o644
        );
    }
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

fn tar(source: &Path, archive: &Path) {
    let status = Command::new("tar")
        .arg("-czf")
        .arg(archive)
        .arg("-C")
        .arg(source)
        .arg(".")
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn extracts_archives_of_hostile_names_with_the_servers_programs() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "extract").await;
    let source = fixture.local.path().join("source");
    std::fs::create_dir_all(source.join(HOSTILE_FOLDER)).unwrap();
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        std::fs::write(source.join(name), pattern(500, index as u64)).unwrap();
        std::fs::write(source.join(HOSTILE_FOLDER).join(name), b"inside").unwrap();
    }
    let archive = fixture.local.path().join("archive.tar.gz");
    tar(&source, &archive);
    let archive_name = format!("{}.tar.gz", HOSTILE_NAMES[1]);
    fixture.mkdir(HOSTILE_FOLDER).await;
    let archive_path = format!("{HOSTILE_FOLDER}/{archive_name}");
    fixture
        .write(&archive_path, &std::fs::read(&archive).unwrap())
        .await;
    fixture.forget_exec_lines().await;

    let operations = fixture.operations(ServerFeatures::default());
    let summary = operations
        .extract(ExtractRequest {
            operation_id: uuid::Uuid::new_v4().to_string(),
            location: fixture.location(),
            path: fixture.path(&archive_path),
            into: ExtractInto::Folder,
            extractor: Default::default(),
        })
        .await
        .unwrap();
    let folder = format!("{HOSTILE_FOLDER}/{}", HOSTILE_NAMES[1]);
    assert_eq!(summary.destination, fixture.path(&folder));
    let mut expected = names_of(&HOSTILE_NAMES);
    expected.insert(HOSTILE_FOLDER.to_string());
    assert_eq!(fixture.names(&folder).await, expected);
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        let extracted = fixture.read(&format!("{folder}/{name}")).await;
        assert_eq!(extracted, pattern(500, index as u64), "{name:?}");
    }
    assert_eq!(
        fixture.names(&format!("{folder}/{HOSTILE_FOLDER}")).await,
        names_of(&HOSTILE_NAMES)
    );

    // A single compressed file of a hostile name unpacks beside itself.
    let single = fixture.local.path().join(HOSTILE_NAMES[5]);
    std::fs::write(&single, pattern(9_000, 4)).unwrap();
    let status = Command::new("gzip")
        .arg("-k")
        .arg(&single)
        .status()
        .unwrap();
    assert!(status.success());
    let compressed = std::fs::read(format!("{}.gz", single.display())).unwrap();
    let compressed_path = format!("{HOSTILE_FOLDER}/{}.gz", HOSTILE_NAMES[5]);
    fixture.write(&compressed_path, &compressed).await;
    let summary = operations
        .extract(ExtractRequest {
            operation_id: uuid::Uuid::new_v4().to_string(),
            location: fixture.location(),
            path: fixture.path(&compressed_path),
            into: ExtractInto::Here,
            extractor: Default::default(),
        })
        .await
        .unwrap();
    let unpacked = format!("{HOSTILE_FOLDER}/{}", HOSTILE_NAMES[5]);
    assert_eq!(summary.destination, fixture.path(&unpacked));
    assert_eq!(fixture.read(&unpacked).await, pattern(9_000, 4));
    expected = names_of(&[
        &archive_name,
        HOSTILE_NAMES[1],
        &format!("{}.gz", HOSTILE_NAMES[5]),
        HOSTILE_NAMES[5],
    ]);
    assert_eq!(fixture.names(HOSTILE_FOLDER).await, expected);
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

#[tokio::test]
async fn searches_below_a_hostile_folder_for_hostile_patterns() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "search").await;
    fixture.mkdir(HOSTILE_FOLDER).await;
    for name in HOSTILE_NAMES.iter().chain(&BYSTANDERS) {
        fixture
            .write(&format!("{HOSTILE_FOLDER}/{name}"), b"x")
            .await;
    }
    fixture.forget_exec_lines().await;
    let searches = SearchManager::with_engines(fixture.manager.clone(), Engines::Automatic);
    let search = |pattern: &str| {
        let request = SearchRequest {
            search_id: uuid::Uuid::new_v4().to_string(),
            location: fixture.location(),
            folder: fixture.path(HOSTILE_FOLDER),
            pattern: pattern.to_string(),
            pattern_kind: PatternKind::Text,
            case_sensitive: true,
            kind: KindFilter::Any,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            max_depth: None,
            hidden: true,
            follow_links: false,
            limit: None,
        };
        let reports: Arc<Mutex<Vec<SearchReport>>> = Arc::default();
        let sink_reports = reports.clone();
        let sink: SearchSink = Arc::new(move |report| sink_reports.lock().unwrap().push(report));
        let searches = &searches;
        async move {
            let summary = searches.run(request, sink).await.unwrap();
            assert_eq!(summary.engine, Engine::Find);
            let found: BTreeSet<String> = reports
                .lock()
                .unwrap()
                .iter()
                .flat_map(|report| report.entries.iter().map(|entry| entry.name.clone()))
                .collect();
            found
        }
    };

    let touching: Vec<&str> = HOSTILE_NAMES
        .into_iter()
        .filter(|name| name.contains("touch poros-pwned"))
        .collect();
    assert_eq!(search("touch poros-pwned").await, names_of(&touching));
    assert_eq!(
        search("$(touch poros-pwned)").await,
        names_of(&["$(touch poros-pwned)"])
    );
    assert_eq!(search("';touch poros").await, names_of(&[HOSTILE_NAMES[1]]));
    assert_eq!(search("it's").await, names_of(&["it's"]));
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

fn transfer_settings() -> TransferSettings {
    TransferSettings {
        workers: 2,
        segment_threshold_mib: 1,
        delta_threshold_kib: 64,
        exists_action: ExistsAction::Overwrite,
        retry_attempts: 0,
        verify_checksums: true,
        ..TransferSettings::default()
    }
}

async fn wait_until_settled(transfers: &TransferManager) -> TransferList {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let list = transfers.list();
        let counts = &list.stats.counts;
        if counts.queued == 0 && counts.running == 0 {
            return list;
        }
        assert!(
            Instant::now() < deadline,
            "transfers did not finish: {counts:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
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

/// A file of `HOSTILE_NAMES[index]`: one large enough to be checked part by part, the others
/// large enough for rsync.
fn transfer_contents(index: usize, edited: bool) -> Vec<u8> {
    let length = if index == 0 {
        1_500_000
    } else {
        100_000 + index
    };
    let mut contents = pattern(length, index as u64);
    if edited {
        contents[length / 2..length / 2 + 50].fill(7);
    }
    contents
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transfers_check_and_rsync_files_of_hostile_names() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "transfer").await;
    let local = fixture.local.path().join("local");
    std::fs::create_dir_all(&local).unwrap();
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        std::fs::write(local.join(name), transfer_contents(index, false)).unwrap();
    }
    fixture.mkdir(HOSTILE_FOLDER).await;
    let remote_folder = fixture.path(HOSTILE_FOLDER);
    let upload_all = || {
        let items = HOSTILE_NAMES
            .iter()
            .map(|name| {
                let path: PathBuf = local.join(name);
                TransferItem {
                    path: path.to_string_lossy().into_owned(),
                    name: name.to_string(),
                    is_dir: false,
                    size: path.metadata().unwrap().len(),
                }
            })
            .collect();
        EnqueueRequest {
            session_id: fixture.session.id.clone(),
            source_session_id: None,
            direction: Direction::Upload,
            target_directory: remote_folder.clone(),
            items,
        }
    };

    // New files are sent whole and checked with the server's sha256sum, part by part for the
    // large one.
    let transfers = TransferManager::new(
        fixture.manager.clone(),
        fixture.events.clone(),
        transfer_settings(),
    );
    transfers.enqueue(upload_all()).await.unwrap();
    let list = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        let sent = fixture.read(&format!("{HOSTILE_FOLDER}/{name}")).await;
        assert!(sent == transfer_contents(index, false), "{name:?}");
    }
    transfers.clear(&[JobState::Done]);

    // Changed files go through rsync on the server, both ways.
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        std::fs::write(local.join(name), transfer_contents(index, true)).unwrap();
    }
    transfers.enqueue(upload_all()).await.unwrap();
    let list = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    for job in &list.jobs {
        assert!(job.delta_bytes.is_some(), "{} was sent whole", job.name);
    }
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        let sent = fixture.read(&format!("{HOSTILE_FOLDER}/{name}")).await;
        assert!(sent == transfer_contents(index, true), "{name:?}");
    }
    transfers.clear(&[JobState::Done]);

    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        std::fs::write(local.join(name), transfer_contents(index, false)).unwrap();
    }
    let fs = fixture.fs().await;
    let items = fs
        .entries(&remote_folder)
        .await
        .unwrap()
        .into_iter()
        .map(|(name, stat)| TransferItem {
            path: format!("{remote_folder}/{name}"),
            name,
            is_dir: false,
            size: stat.size,
        })
        .collect();
    transfers
        .enqueue(EnqueueRequest {
            session_id: fixture.session.id.clone(),
            source_session_id: None,
            direction: Direction::Download,
            target_directory: local.to_string_lossy().into_owned(),
            items,
        })
        .await
        .unwrap();
    let list = wait_until_settled(&transfers).await;
    assert_all_done(&list);
    assert_eq!(list.jobs.len(), HOSTILE_NAMES.len());
    for job in &list.jobs {
        assert!(job.delta_bytes.is_some(), "{} was received whole", job.name);
    }
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        let received = std::fs::read(local.join(name)).unwrap();
        assert!(received == transfer_contents(index, true), "{name:?}");
    }
    let left: BTreeSet<String> = std::fs::read_dir(&local)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, names_of(&HOSTILE_NAMES));
    drop(transfers);
    let messages: Vec<String> = fixture
        .events
        .take_startup_log()
        .into_iter()
        .map(|record| record.message)
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("are checked with SHA-256 (sha256sum), part by part")),
        "{messages:#?}"
    );
    let unchecked: Vec<&String> = messages
        .iter()
        .filter(|message| message.contains("could not compute a checksum"))
        .collect();
    assert!(unchecked.is_empty(), "{unchecked:#?}");
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

#[tokio::test]
async fn synchronization_compares_hostile_names_by_checksum_on_the_server() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "sync").await;
    let local = fixture.local.path().join("local");
    std::fs::create_dir_all(&local).unwrap();
    fixture.mkdir(HOSTILE_FOLDER).await;
    for (index, name) in HOSTILE_NAMES.iter().enumerate() {
        let contents = pattern(1_000, index as u64);
        std::fs::write(local.join(name), &contents).unwrap();
        fixture
            .write(&format!("{HOSTILE_FOLDER}/{name}"), &contents)
            .await;
    }
    // The same size on both sides, so only the contents tell them apart.
    let changed = HOSTILE_NAMES[2];
    std::fs::write(local.join(changed), pattern(1_000, 99)).unwrap();
    fixture.forget_exec_lines().await;

    let sync = SyncManager::new(fixture.manager.clone(), Events::default());
    let plan = sync
        .compare(SyncRequest {
            request_id: "shell".into(),
            session_id: fixture.session.id.clone(),
            local_path: local.to_string_lossy().into_owned(),
            remote_path: fixture.path(HOSTILE_FOLDER),
            direction: SyncDirection::Upload,
            options: SyncOptions {
                compare: CompareMode::Checksum,
                ..SyncOptions::default()
            },
            arguments: String::new(),
        })
        .await
        .unwrap();
    let planned: Vec<(String, SyncAction, SyncReason)> = plan
        .items
        .iter()
        .map(|item| (item.path.clone(), item.action, item.reason))
        .collect();
    assert_eq!(
        planned,
        [(
            changed.to_string(),
            SyncAction::Upload,
            SyncReason::ContentDiffers
        )]
    );
    assert_eq!(plan.counts.unchanged, HOSTILE_NAMES.len() as u64 - 1);
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

#[tokio::test]
async fn commands_get_hostile_names_as_arguments_and_written_commands_run_in_fish() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "command").await;
    fixture.mkdir(HOSTILE_FOLDER).await;
    for name in HOSTILE_NAMES.iter().chain(&BYSTANDERS) {
        fixture
            .write(&format!("{HOSTILE_FOLDER}/{name}"), b"x")
            .await;
    }
    fixture.forget_exec_lines().await;
    let session = fixture.session().await;
    let run = |command: &'static str, directory: Option<String>, arguments: Vec<String>| {
        let session = session.clone();
        async move {
            let mut stdout = String::new();
            let mut stderr = String::new();
            let result = remote_command::run_on(
                &session,
                command,
                directory.as_deref(),
                &arguments,
                &CancellationToken::new(),
                |stream, text| match stream {
                    OutputStream::Stdout => stdout.push_str(text),
                    OutputStream::Stderr => stderr.push_str(text),
                },
            )
            .await
            .unwrap();
            (result.exit_status, stdout, stderr)
        }
    };

    // With names, in a folder: the names are the command's arguments, whole.
    let arguments: Vec<String> = HOSTILE_NAMES.iter().map(|name| name.to_string()).collect();
    let (status, stdout, stderr) = run(
        r#"printf '%s\0' "$@"; printf '%s' "$PWD" >&2"#,
        Some(fixture.path(HOSTILE_FOLDER)),
        arguments.clone(),
    )
    .await;
    assert_eq!(status, Some(0), "{stderr}");
    let printed: Vec<&str> = stdout.strip_suffix('\0').unwrap().split('\0').collect();
    assert_eq!(printed, HOSTILE_NAMES);
    assert_eq!(stderr, fixture.path(HOSTILE_FOLDER));
    let (status, _, stderr) = run(
        r#"rm -- "$@""#,
        Some(fixture.path(HOSTILE_FOLDER)),
        arguments,
    )
    .await;
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(fixture.names(HOSTILE_FOLDER).await, names_of(&BYSTANDERS));
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.forget_exec_lines().await;

    // Without names, what the user wrote runs in their own shell, in fish syntax.
    let fish_command = "set greeting (string join ' ' hello from fish); echo $greeting";
    let (status, stdout, stderr) = run(fish_command, None, Vec::new()).await;
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(stdout, "hello from fish\n");
    assert_eq!(fixture.exec_lines().await, [fish_command]);
    fixture.forget_exec_lines().await;

    // In a folder too: `sh` goes to the folder, then hands the command to the login shell.
    let in_folder = "echo (count *) in (basename $PWD)";
    let (status, stdout, stderr) =
        run(in_folder, Some(fixture.path(HOSTILE_FOLDER)), Vec::new()).await;
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(
        stdout,
        format!("{} in {HOSTILE_FOLDER}\n", BYSTANDERS.len())
    );
    assert_eq!(fixture.exec_lines().await, [SCRIPT_LINE, in_folder]);
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}

fn status(output: &Output) -> i32 {
    output.status.code().expect("poros-cli ended by a signal")
}

#[tokio::test]
async fn the_command_line_checksums_hostile_names_with_the_servers_command() {
    let Some(account) = account() else { return };
    let _turn = ONE_AT_A_TIME.lock().await;
    let fixture = Fixture::new(&account, "cli").await;
    // The command line expands wildcards in the paths it is given, so names that are
    // wildcards are left to the other tests.
    let names: Vec<&str> = HOSTILE_NAMES
        .into_iter()
        .filter(|name| !name.contains(['*', '?', '[', '{']))
        .collect();
    fixture.mkdir(HOSTILE_FOLDER).await;
    for (index, name) in names.iter().enumerate() {
        fixture
            .write(
                &format!("{HOSTILE_FOLDER}/{name}"),
                &pattern(700, index as u64),
            )
            .await;
    }
    fixture.forget_exec_lines().await;

    let config = tempfile::tempdir().unwrap();
    let cli = |arguments: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_poros-cli"))
            .args(arguments)
            .env_remove("POROS_USER")
            .env_remove("POROS_PASSPHRASE")
            .env_remove("POROS_IDENTITY")
            .env_remove("POROS_HOST_KEY")
            .env_remove("POROS_TRUST_NEW_HOST")
            .env_remove("POROS_JSON")
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("RUST_LOG")
            .env("POROS_CONFIG_DIR", config.path())
            .env("POROS_NO_KEYCHAIN", "1")
            .env(
                "POROS_CONNECTION",
                format!("sftp://{}@127.0.0.1:{}", account.user, account.port),
            )
            .env("POROS_PASSWORD", &account.password)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let trusted = cli(&["--trust-new-host", "--save-host-key", "info"]);
    assert_eq!(status(&trusted), 0, "{trusted:?}");
    for (index, name) in names.iter().enumerate() {
        let path = fixture.path(&format!("{HOSTILE_FOLDER}/{name}"));
        let output = cli(&["--json", "checksum", &path]);
        assert_eq!(status(&output), 0, "{output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        let expected: String = Sha256::digest(pattern(700, index as u64))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(value["digest"], expected.as_str(), "{name:?}");
        assert_eq!(value["method"], "serverCommand", "{name:?}");
    }
    fixture.assert_only_fixed_lines(&[]).await;
    fixture.assert_nothing_ran().await;
    fixture.close().await;
}
