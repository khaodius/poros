//! End-to-end tests of server files open in other programs, against real servers set up as
//! `sftp_server.rs`, `ftp_server.rs` and `http_storage.rs` describe. A small shell script
//! stands in for the program: it is started on the working copy and saves it after a moment.
//! Each test is skipped unless its server is configured:
//!
//! - `POROS_TEST_SSH_PORT`: SFTP, also for the vault.
//! - `POROS_TEST_FTP_PORT`: plain FTP.
//! - `POROS_TEST_WEBDAV_PORT`: WebDAV over plain HTTP.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::events::Events;
use poros_lib::external_editor::{
    EditState, EditView, ExternalEditManager, OpenOutcome, OpenRequest, OpenWith, Pace, Resolution,
    Timing,
};
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::{EditingSettings, ExternalProgram, TransferSettings};
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::transfer::TransferManager;

const OWNER: &str = "main";
const WAIT: Duration = Duration::from_secs(30);
const PASSWORD: &str = "a vault's password";

/// What the stand-in program does with the file it is given, a moment after it starts.
const EDITOR_SCRIPT: &str = r#"#!/bin/sh
file="$1"
sleep 0.4
case "$2" in
  rename)
    printf 'saved through a temporary file\n' > "$file.tmp"
    mv "$file.tmp" "$file"
    ;;
  in-place)
    printf 'rewritten in place\n' > "$file"
    ;;
  recreate)
    rm "$file"
    printf 'deleted and made anew\n' > "$file"
    ;;
  twice)
    printf 'first save\n' > "$file"
    sleep 0.1
    printf 'second save, the one that counts\n' > "$file"
    ;;
  nothing) ;;
esac
"#;

fn server_profile(protocol: Protocol, port_variable: &str, prefix: &str) -> Option<ConnectProfile> {
    let port = std::env::var(port_variable).ok()?.parse().ok()?;
    Some(ConnectProfile {
        protocol,
        host: "127.0.0.1".into(),
        port,
        username: std::env::var(format!("{prefix}_USER")).unwrap_or_else(|_| "poros".into()),
        auth: AuthMethod::Password {
            password: std::env::var(format!("{prefix}_PASSWORD"))
                .unwrap_or_else(|_| "poros-pass".into()),
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
    })
}

fn sftp_profile() -> Option<ConnectProfile> {
    server_profile(Protocol::Sftp, "POROS_TEST_SSH_PORT", "POROS_TEST_SSH")
}

fn ftp_profile() -> Option<ConnectProfile> {
    server_profile(Protocol::Ftp, "POROS_TEST_FTP_PORT", "POROS_TEST_FTP")
}

fn webdav_profile() -> Option<ConnectProfile> {
    server_profile(
        Protocol::WebDav,
        "POROS_TEST_WEBDAV_PORT",
        "POROS_TEST_WEBDAV",
    )
}

/// Connects, approving the server's host key if the manager asks.
async fn connect(sessions: &SessionManager, profile: &ConnectProfile) -> SessionInfo {
    match sessions.connect(profile.clone(), None, OWNER).await {
        Ok(info) => info,
        Err(error) => {
            let host_key = error
                .host_key
                .unwrap_or_else(|| panic!("could not connect: {}", error.message));
            let approval = HostKeyApproval {
                fingerprint: host_key.fingerprint,
                remember: true,
            };
            sessions
                .connect(profile.clone(), Some(approval), OWNER)
                .await
                .unwrap()
        }
    }
}

async fn write_through(files: &Arc<dyn RemoteFileSystem>, path: &str, contents: &[u8]) {
    let mut writer = files
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
    writer
        .write(Bytes::copy_from_slice(contents))
        .await
        .unwrap();
    writer.finish().await.unwrap();
}

async fn read_through(files: &Arc<dyn RemoteFileSystem>, path: &str) -> Vec<u8> {
    let size = files.stat(path).await.unwrap().unwrap().size;
    let mut reader = files.clone().open_read(path, 0..size).await.unwrap();
    let mut contents = Vec::new();
    while let Some(data) = reader.next_chunk().await.unwrap() {
        contents.extend_from_slice(&data);
    }
    reader.finish().await.unwrap();
    contents
}

/// The stand-in program, once for each way it saves, named after it.
fn stand_in_programs(folder: &Path) -> Vec<ExternalProgram> {
    let script = folder.join("editor.sh");
    std::fs::write(&script, EDITOR_SCRIPT).unwrap();
    ["rename", "in-place", "recreate", "twice", "nothing"]
        .into_iter()
        .map(|mode| ExternalProgram {
            id: mode.to_string(),
            name: format!("Stand-in ({mode})"),
            command: "/bin/sh".to_string(),
            arguments: format!("\"{}\" {{file}} {mode}", script.display()),
            patterns: Vec::new(),
        })
        .collect()
}

/// Short waits, so the tests take seconds.
fn quick_pace() -> Pace {
    Pace {
        watch: Timing {
            poll: Duration::from_millis(100),
            settle: Duration::from_millis(400),
        },
        job_poll: Duration::from_millis(40),
        server_retry: Duration::from_millis(500),
        program_hand_over: Duration::from_secs(60),
    }
}

/// A server with a scratch folder, and the app's side of editing its files.
struct Bench {
    local: tempfile::TempDir,
    sessions: Arc<SessionManager>,
    session: SessionInfo,
    files: Arc<dyn RemoteFileSystem>,
    /// The scratch folder on the server.
    folder: String,
    transfers: TransferManager,
    events: Events,
    external: ExternalEditManager,
}

impl Bench {
    async fn start(profile: &ConnectProfile, name: &str) -> Self {
        Self::start_with(
            profile,
            name,
            EditingSettings::default(),
            quick_pace(),
            true,
        )
        .await
    }

    async fn start_with(
        profile: &ConnectProfile,
        name: &str,
        mut editing: EditingSettings,
        pace: Pace,
        keep_completed: bool,
    ) -> Self {
        let local = tempfile::tempdir().unwrap();
        let events = Events::keeping_startup_log();
        let sessions = Arc::new(SessionManager::new(
            local.path().join("known_hosts"),
            Events::default(),
        ));
        let session = connect(&sessions, profile).await;
        let files = sessions.get(&session.id).await.unwrap().files();
        let scratch = format!("poros-external-editor-{name}-{}", std::process::id());
        let existing = remote_path::join(&session.home, &scratch);
        if files.stat(&existing).await.unwrap().is_some() {
            files.delete(std::slice::from_ref(&existing)).await.unwrap();
        }
        let folder = files.make_dir("~", &scratch).await.unwrap();

        editing.programs = stand_in_programs(local.path());
        let transfers = TransferManager::new(
            sessions.clone(),
            events.clone(),
            TransferSettings {
                keep_completed,
                retry_attempts: 0,
                ..TransferSettings::default()
            },
        );
        let external = ExternalEditManager::with_pace(
            local.path().join("editing"),
            transfers.files(),
            events.clone(),
            editing,
            pace,
        );
        Self {
            local,
            sessions,
            session,
            files,
            folder,
            transfers,
            events,
            external,
        }
    }

    /// Puts a file in the scratch folder and returns its path.
    async fn put(&self, name: &str, contents: &[u8]) -> String {
        let path = remote_path::join(&self.folder, name);
        write_through(&self.files, &path, contents).await;
        path
    }

    async fn read(&self, path: &str) -> Vec<u8> {
        read_through(&self.files, path).await
    }

    fn request(&self, path: &str, program: &str) -> OpenRequest {
        OpenRequest {
            session_id: self.session.id.clone(),
            path: path.to_string(),
            with: OpenWith::Program {
                id: program.to_string(),
            },
            confirmed_executable: false,
        }
    }

    /// Opens a server file in the stand-in program.
    async fn open(&self, path: &str, program: &str) -> EditView {
        match self
            .external
            .open(self.request(path, program))
            .await
            .unwrap()
        {
            OpenOutcome::Opening { edit } => edit,
            other => panic!("expected the file to open, got {other:?}"),
        }
    }

    fn edit(&self, id: &str) -> Option<EditView> {
        self.external.list().into_iter().find(|edit| edit.id == id)
    }

    /// Waits until the edit is as `done` says.
    async fn wait(&self, id: &str, what: &str, done: impl Fn(&EditView) -> bool) -> EditView {
        let deadline = Instant::now() + WAIT;
        loop {
            let edit = self.edit(id);
            if let Some(edit) = edit.as_ref().filter(|edit| done(edit)) {
                return edit.clone();
            }
            assert!(Instant::now() < deadline, "waited for {what}: {edit:#?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Waits until the working copy is in and nothing waits to be uploaded.
    async fn wait_watching(&self, id: &str, uploads: u32) -> EditView {
        self.wait(id, &format!("{uploads} uploads"), |edit| {
            edit.state == EditState::Watching && edit.uploads == uploads
        })
        .await
    }

    async fn wait_gone(&self, id: &str) {
        let deadline = Instant::now() + WAIT;
        while self.edit(id).is_some() {
            assert!(
                Instant::now() < deadline,
                "the edit did not end: {:#?}",
                self.edit(id)
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn logged(&self) -> Vec<String> {
        self.events
            .take_startup_log()
            .into_iter()
            .map(|record| record.message)
            .collect()
    }

    /// Removes what the test left on the server.
    async fn finish(self) {
        self.files
            .delete(std::slice::from_ref(&self.folder))
            .await
            .unwrap();
        self.sessions.disconnect_all().await;
    }
}

fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[tokio::test]
async fn a_file_saved_in_another_program_is_uploaded() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let bench = Bench::start(&profile, "saves").await;
    // A name a shell would run commands for, if one ever saw it.
    let name = "report $(touch injected) `id` 'q' ; x.txt";
    let path = bench.put(name, b"as it was on the server\n").await;

    let opened = bench.open(&path, "rename").await;
    assert_eq!(opened.name, name);
    assert_eq!(opened.state, EditState::Downloading);
    assert!(opened.outlives_session);
    assert!(!opened.in_vault);

    let edit = bench.wait_watching(&opened.id, 1).await;
    assert_eq!(bench.read(&path).await, b"saved through a temporary file\n");
    assert!(edit.last_upload.is_some());

    // The working copy has the file's own name, in a folder of its own that only this user
    // can read, and nothing else was made beside it.
    let local = PathBuf::from(&edit.local_path);
    let folder = local.parent().unwrap();
    assert_eq!(local.file_name().unwrap().to_str().unwrap(), name);
    assert_eq!(folder.parent().unwrap(), bench.local.path().join("editing"));
    assert_eq!(mode_of(folder), 0o700);
    assert_eq!(mode_of(&local) & 0o111, 0);
    let beside: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(beside.len(), 1, "{beside:?}");

    // Opened a second time, the same working copy is used.
    let again = bench
        .external
        .open(bench.request(&path, "nothing"))
        .await
        .unwrap();
    assert!(
        matches!(&again, OpenOutcome::AlreadyOpen { edit } if edit.id == opened.id),
        "{again:?}"
    );
    assert_eq!(bench.external.list().len(), 1);

    // The upload went through the queue, where it is listed like any other.
    let jobs = bench.transfers.list().jobs;
    assert!(
        jobs.iter()
            .any(|job| job.name == name && job.target == path),
        "{jobs:#?}"
    );

    bench.external.stop(&opened.id).unwrap();
    bench.wait_gone(&opened.id).await;
    assert!(!folder.exists());
    assert_eq!(bench.read(&path).await, b"saved through a temporary file\n");

    let logged = bench.logged();
    assert!(
        logged
            .iter()
            .any(|line| line.starts_with(&format!("Uploaded {name} to "))),
        "{logged:#?}"
    );
    bench.finish().await;
}

#[tokio::test]
async fn every_way_of_saving_is_noticed() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let bench = Bench::start(&profile, "styles").await;
    for (mode, saved) in [
        ("in-place", "rewritten in place\n"),
        ("recreate", "deleted and made anew\n"),
    ] {
        let path = bench
            .put(&format!("{mode}.txt"), b"as it was on the server\n")
            .await;
        let opened = bench.open(&path, mode).await;
        bench.wait_watching(&opened.id, 1).await;
        assert_eq!(bench.read(&path).await, saved.as_bytes(), "{mode}");
    }
    bench.finish().await;
}

#[tokio::test]
async fn two_saves_in_quick_succession_are_one_upload() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let bench = Bench::start(&profile, "twice").await;
    let path = bench.put("notes.txt", b"as it was on the server\n").await;

    let opened = bench.open(&path, "twice").await;
    bench.wait_watching(&opened.id, 1).await;
    // Long enough for a second upload to have started and finished, were there one.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let edit = bench.edit(&opened.id).unwrap();
    assert_eq!(edit.uploads, 1);
    assert_eq!(edit.state, EditState::Watching);
    assert_eq!(
        bench.read(&path).await,
        b"second save, the one that counts\n"
    );
    let uploads = bench
        .logged()
        .iter()
        .filter(|line| line.starts_with("Uploaded notes.txt"))
        .count();
    assert_eq!(uploads, 1);
    bench.finish().await;
}

/// Opens a file without saving it, changes the server's copy underneath, saves the working
/// copy, and goes through every answer to the question that follows.
async fn a_changed_server_copy_asks_first(
    profile: &ConnectProfile,
    name: &str,
    keep_completed: bool,
) {
    let bench = Bench::start_with(
        profile,
        name,
        EditingSettings::default(),
        quick_pace(),
        keep_completed,
    )
    .await;
    let path = bench.put("shared.txt", b"as it was on the server\n").await;
    let opened = bench.open(&path, "nothing").await;
    let edit = bench.wait_watching(&opened.id, 0).await;
    let local = PathBuf::from(&edit.local_path);
    assert_eq!(std::fs::read(&local).unwrap(), b"as it was on the server\n");
    // As downloaded, the working copy is for this user alone.
    assert_eq!(mode_of(&local), 0o600);

    // Someone else changes the file on the server, then the program saves.
    let theirs = b"changed by someone else meanwhile, and longer\n";
    bench.put("shared.txt", theirs).await;
    std::fs::write(&local, "my edit\n").unwrap();
    let asked = bench
        .wait(&opened.id, "the conflict", |edit| {
            edit.state == EditState::Conflict
        })
        .await;
    let conflict = asked.conflict.unwrap();
    assert_eq!(conflict.server_size, Some(theirs.len() as u64));
    assert_eq!(conflict.local_size, 8);
    assert!(!conflict.deleted);
    assert_eq!(asked.uploads, 0);
    assert_eq!(bench.read(&path).await, theirs);

    // Cancel: nothing is uploaded, and asking for the upload asks the question again.
    bench
        .external
        .resolve(&opened.id, Resolution::Cancel)
        .unwrap();
    bench
        .wait(&opened.id, "the held save", |edit| {
            edit.state == EditState::Changed
        })
        .await;
    assert_eq!(bench.read(&path).await, theirs);
    bench.external.upload(&opened.id).unwrap();
    bench
        .wait(&opened.id, "the second question", |edit| {
            edit.state == EditState::Conflict
        })
        .await;

    // Keep both: the working copy goes up under a new name, which the edit follows from
    // then on.
    bench
        .external
        .resolve(&opened.id, Resolution::KeepBoth)
        .unwrap();
    let kept = bench.wait_watching(&opened.id, 1).await;
    assert_ne!(kept.remote_path, path);
    assert_eq!(
        remote_path::parent(&kept.remote_path).unwrap(),
        bench.folder
    );
    assert_eq!(bench.read(&path).await, theirs);
    assert_eq!(bench.read(&kept.remote_path).await, b"my edit\n");
    std::fs::write(&local, "my second edit\n").unwrap();
    bench.wait_watching(&opened.id, 2).await;
    assert_eq!(bench.read(&kept.remote_path).await, b"my second edit\n");
    assert_eq!(bench.read(&path).await, theirs);

    // Overwrite: the server's newer copy is replaced, knowingly.
    write_through(
        &bench.files,
        &kept.remote_path,
        b"changed by someone else once more\n",
    )
    .await;
    std::fs::write(&local, "my third edit\n").unwrap();
    bench
        .wait(&opened.id, "the third question", |edit| {
            edit.state == EditState::Conflict
        })
        .await;
    assert_eq!(
        bench.read(&kept.remote_path).await,
        b"changed by someone else once more\n"
    );
    bench
        .external
        .resolve(&opened.id, Resolution::Overwrite)
        .unwrap();
    bench.wait_watching(&opened.id, 3).await;
    assert_eq!(bench.read(&kept.remote_path).await, b"my third edit\n");

    // Deleted on the server: the question says so, and the upload puts the file back.
    bench
        .files
        .delete(std::slice::from_ref(&kept.remote_path))
        .await
        .unwrap();
    std::fs::write(&local, "my fourth edit\n").unwrap();
    let asked = bench
        .wait(&opened.id, "the question about the deleted file", |edit| {
            edit.state == EditState::Conflict
        })
        .await;
    assert!(asked.conflict.unwrap().deleted);
    bench
        .external
        .resolve(&opened.id, Resolution::Overwrite)
        .unwrap();
    bench.wait_watching(&opened.id, 4).await;
    assert_eq!(bench.read(&kept.remote_path).await, b"my fourth edit\n");

    let logged = bench.logged();
    assert!(
        logged
            .iter()
            .any(|line| line.contains("changed on the server while it was open")),
        "{logged:#?}"
    );
    bench.finish().await;
}

#[tokio::test]
async fn a_changed_sftp_file_is_not_written_over_unasked() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    a_changed_server_copy_asks_first(&profile, "conflict-sftp", true).await;
}

#[tokio::test]
async fn a_changed_ftp_file_is_not_written_over_unasked() {
    let Some(profile) = ftp_profile() else {
        return;
    };
    a_changed_server_copy_asks_first(&profile, "conflict-ftp", false).await;
}

#[tokio::test]
async fn a_changed_webdav_file_is_not_written_over_unasked() {
    let Some(profile) = webdav_profile() else {
        return;
    };
    a_changed_server_copy_asks_first(&profile, "conflict-webdav", true).await;
}

/// The stand-in program saves a file on a server that is not SFTP, and the upload arrives.
async fn a_save_reaches(profile: &ConnectProfile, name: &str) {
    let bench = Bench::start(profile, name).await;
    let path = bench.put("page.html", b"as it was on the server\n").await;
    let opened = bench.open(&path, "rename").await;
    assert!(opened.outlives_session);
    bench.wait_watching(&opened.id, 1).await;
    assert_eq!(bench.read(&path).await, b"saved through a temporary file\n");
    bench.external.stop(&opened.id).unwrap();
    bench.wait_gone(&opened.id).await;
    bench.finish().await;
}

#[tokio::test]
async fn ftp_files_are_uploaded_when_saved() {
    let Some(profile) = ftp_profile() else {
        return;
    };
    a_save_reaches(&profile, "saves-ftp").await;
}

#[tokio::test]
async fn webdav_files_are_uploaded_when_saved() {
    let Some(profile) = webdav_profile() else {
        return;
    };
    a_save_reaches(&profile, "saves-webdav").await;
}

#[tokio::test]
async fn files_that_would_run_are_only_opened_in_an_editor_once_confirmed() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let bench = Bench::start(&profile, "programs").await;
    let script = bench.put("deploy.sh", b"echo deploying\n").await;

    // By name: nothing is downloaded until the user agrees, and never for the system's
    // default program.
    let mut request = bench.request(&script, "rename");
    assert_eq!(
        bench.external.open(request.clone()).await.unwrap(),
        OpenOutcome::NeedsConfirmation {
            name: "deploy.sh".to_string()
        }
    );
    let by_default = OpenRequest {
        with: OpenWith::Default,
        confirmed_executable: true,
        ..request.clone()
    };
    assert!(matches!(
        bench.external.open(by_default).await.unwrap(),
        OpenOutcome::NeedsConfirmation { .. }
    ));
    assert!(bench.external.list().is_empty());
    assert!(!bench.local.path().join("editing").exists());

    request.confirmed_executable = true;
    let OpenOutcome::Opening { edit: opened } = bench.external.open(request).await.unwrap() else {
        panic!("a confirmed script should open in the editor");
    };
    let edit = bench.wait_watching(&opened.id, 1).await;
    assert_eq!(
        bench.read(&script).await,
        b"saved through a temporary file\n"
    );
    // The working copy cannot be run, whatever the file's permissions on the server.
    assert_eq!(mode_of(Path::new(&edit.local_path)) & 0o111, 0);

    // By content: a script behind a harmless name is downloaded but not opened.
    let disguised = bench
        .put("notes.txt", b"#!/bin/sh\necho not notes at all\n")
        .await;
    let opened = bench.open(&disguised, "rename").await;
    let held = bench
        .wait(&opened.id, "the held script", |edit| edit.blocked)
        .await;
    assert_eq!(held.state, EditState::Watching);
    // The stand-in program would have saved the file by now, had it been started.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(bench.edit(&opened.id).unwrap().uploads, 0);
    // Nor does the page get the system to open it, even saying the user agreed: not for the
    // copy held back, and not for a new one.
    let by_default = |path: &str| OpenRequest {
        with: OpenWith::Default,
        confirmed_executable: true,
        ..bench.request(path, "rename")
    };
    assert!(matches!(
        bench.external.open(by_default(&disguised)).await.unwrap(),
        OpenOutcome::AlreadyOpen { .. }
    ));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let still_held = bench.edit(&opened.id).unwrap();
    assert!(still_held.blocked, "{still_held:#?}");
    assert_eq!(still_held.program, "Stand-in (rename)");
    let unnamed = bench.put("readme", b"#!/bin/sh\necho not a readme\n").await;
    let OpenOutcome::Opening { edit: unnamed } =
        bench.external.open(by_default(&unnamed)).await.unwrap()
    else {
        panic!("the file should be downloaded and held back");
    };
    bench
        .wait(&unnamed.id, "the held script", |edit| edit.blocked)
        .await;
    bench.external.stop(&unnamed.id).unwrap();
    bench.wait_gone(&unnamed.id).await;
    assert!(bench.external.reopen(&opened.id, None).is_err());
    assert!(bench
        .external
        .reopen(&opened.id, Some(&OpenWith::Default))
        .is_err());
    // Naming an editor is the user's word.
    bench
        .external
        .reopen(
            &opened.id,
            Some(&OpenWith::Program {
                id: "rename".to_string(),
            }),
        )
        .unwrap();
    let edit = bench.wait_watching(&opened.id, 1).await;
    assert!(!edit.blocked);
    assert_eq!(
        bench.read(&disguised).await,
        b"saved through a temporary file\n"
    );
    bench.finish().await;
}

#[tokio::test]
async fn saves_still_arrive_after_the_tab_is_closed() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    saves_arrive_after_closing(&profile, "closed-tab").await;
}

#[tokio::test]
async fn ftp_saves_still_arrive_after_the_tab_is_closed() {
    let Some(profile) = ftp_profile() else {
        return;
    };
    saves_arrive_after_closing(&profile, "closed-tab-ftp").await;
}

#[tokio::test]
async fn webdav_saves_still_arrive_after_the_tab_is_closed() {
    let Some(profile) = webdav_profile() else {
        return;
    };
    saves_arrive_after_closing(&profile, "closed-tab-webdav").await;
}

/// Closes the session a file was opened through, then saves it: the upload finds its way.
async fn saves_arrive_after_closing(profile: &ConnectProfile, name: &str) {
    let bench = Bench::start(profile, name).await;
    let path = bench.put("config.ini", b"as it was on the server\n").await;
    let opened = bench.open(&path, "nothing").await;
    assert!(opened.outlives_session);
    let edit = bench.wait_watching(&opened.id, 0).await;

    bench.sessions.disconnect(&bench.session.id).await.unwrap();
    std::fs::write(&edit.local_path, "saved after the tab closed\n").unwrap();
    bench.wait_watching(&opened.id, 1).await;

    let check_sessions = SessionManager::new(bench.local.path().join("check"), Events::default());
    let check = connect(&check_sessions, profile).await;
    let files = check_sessions.get(&check.id).await.unwrap().files();
    assert_eq!(
        read_through(&files, &path).await,
        b"saved after the tab closed\n"
    );
    files
        .delete(std::slice::from_ref(&bench.folder))
        .await
        .unwrap();
    check_sessions.disconnect_all().await;
}

#[tokio::test]
async fn a_working_copy_goes_once_its_program_has_closed() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let pace = Pace {
        // The stand-in program runs for longer than this, so its end counts as closing.
        program_hand_over: Duration::from_millis(200),
        ..quick_pace()
    };
    let editing = EditingSettings {
        keep_minutes: 0,
        ..EditingSettings::default()
    };
    let bench = Bench::start_with(&profile, "closing", editing, pace, true).await;
    let path = bench.put("letter.txt", b"as it was on the server\n").await;
    let opened = bench.open(&path, "rename").await;
    let local = PathBuf::from(&opened.local_path);

    // The save made as the program closed is uploaded before the copy goes.
    bench.wait_gone(&opened.id).await;
    assert!(!local.parent().unwrap().exists());
    assert_eq!(bench.read(&path).await, b"saved through a temporary file\n");
    bench.finish().await;
}

#[tokio::test]
async fn saves_wait_to_be_asked_for_and_survive_a_restart() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let editing = EditingSettings {
        upload_on_save: false,
        ..EditingSettings::default()
    };
    let bench = Bench::start_with(&profile, "recovery", editing.clone(), quick_pace(), true).await;
    let asked_for = bench.put("asked.txt", b"as it was on the server\n").await;
    let left = bench.put("left.txt", b"as it was on the server\n").await;
    let untouched = bench
        .put("untouched.txt", b"as it was on the server\n")
        .await;

    // With uploads waiting to be asked for, a save is held until the user says so.
    let opened = bench.open(&asked_for, "rename").await;
    bench
        .wait(&opened.id, "the held save", |edit| {
            edit.state == EditState::Changed
        })
        .await;
    assert_eq!(bench.read(&asked_for).await, b"as it was on the server\n");
    assert_eq!(bench.external.unsaved().await.len(), 1);
    bench.external.upload(&opened.id).unwrap();
    bench.wait_watching(&opened.id, 1).await;
    assert_eq!(
        bench.read(&asked_for).await,
        b"saved through a temporary file\n"
    );
    assert!(bench.external.unsaved().await.is_empty());

    // The app closes with one file saved but not uploaded, and one never touched.
    let changed = bench.open(&left, "rename").await;
    let clean = bench.open(&untouched, "nothing").await;
    bench
        .wait(&changed.id, "the save left behind", |edit| {
            edit.state == EditState::Changed
        })
        .await;
    bench.wait_watching(&clean.id, 0).await;
    let folder_of = |edit: &EditView| {
        PathBuf::from(&edit.local_path)
            .parent()
            .unwrap()
            .to_path_buf()
    };
    bench.external.shutdown();
    assert!(folder_of(&changed).exists());
    assert!(!folder_of(&clean).exists());
    assert!(!folder_of(&opened).exists());

    // The next run offers the file back, and uploads it when asked to.
    let next_run = ExternalEditManager::with_pace(
        bench.local.path().join("editing"),
        bench.transfers.files(),
        bench.events.clone(),
        editing,
        quick_pace(),
    );
    let leftovers = next_run.leftovers();
    assert_eq!(leftovers.len(), 1, "{leftovers:#?}");
    assert_eq!(leftovers[0].id, changed.id);
    assert_eq!(leftovers[0].name, "left.txt");
    assert_eq!(leftovers[0].remote_path, left);
    assert_eq!(leftovers[0].host, "127.0.0.1");
    assert_eq!(bench.read(&left).await, b"as it was on the server\n");

    next_run
        .recover(&changed.id, &bench.session.id)
        .await
        .unwrap();
    let deadline = Instant::now() + WAIT;
    while !next_run.list().is_empty() {
        assert!(Instant::now() < deadline, "{:#?}", next_run.list());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(bench.read(&left).await, b"saved through a temporary file\n");
    assert!(next_run.leftovers().is_empty());
    assert!(!folder_of(&changed).exists());
    bench.finish().await;
}

#[tokio::test]
async fn a_failed_upload_keeps_the_working_copy_and_can_be_tried_again() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let bench = Bench::start(&profile, "failure").await;
    let path = bench.put("locked.txt", b"as it was on the server\n").await;
    let opened = bench.open(&path, "nothing").await;
    let edit = bench.wait_watching(&opened.id, 0).await;

    // A folder takes the file's place, so nothing can be uploaded there.
    bench
        .files
        .delete(std::slice::from_ref(&path))
        .await
        .unwrap();
    bench
        .files
        .make_dir(&bench.folder, "locked.txt")
        .await
        .unwrap();
    std::fs::write(&edit.local_path, "saved while the server refused\n").unwrap();
    let failed = bench
        .wait(&opened.id, "the failure", |edit| {
            edit.state == EditState::Failed
        })
        .await;
    assert!(failed.detail.is_some());
    assert_eq!(
        std::fs::read(&edit.local_path).unwrap(),
        b"saved while the server refused\n"
    );
    assert_eq!(bench.external.unsaved().await.len(), 1);

    // Once the way is clear, trying again uploads what was saved. The file was gone from
    // the server in between, which is asked about like any other change there.
    bench
        .files
        .delete(std::slice::from_ref(&path))
        .await
        .unwrap();
    bench.external.upload(&opened.id).unwrap();
    bench
        .wait(&opened.id, "the question", |edit| {
            edit.state == EditState::Conflict
        })
        .await;
    bench
        .external
        .resolve(&opened.id, Resolution::Overwrite)
        .unwrap();
    bench.wait_watching(&opened.id, 1).await;
    assert_eq!(bench.read(&path).await, b"saved while the server refused\n");
    let stuck = bench.external.flush(std::slice::from_ref(&opened.id)).await;
    assert!(stuck.is_empty(), "{stuck:#?}");
    bench.finish().await;
}

#[tokio::test]
async fn a_save_after_a_failed_upload_is_asked_about_again() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let editing = EditingSettings {
        upload_on_save: false,
        ..EditingSettings::default()
    };
    let bench = Bench::start_with(&profile, "asked-again", editing, quick_pace(), true).await;
    let path = bench.put("draft.txt", b"as it was on the server\n").await;
    let opened = bench.open(&path, "nothing").await;
    let edit = bench.wait_watching(&opened.id, 0).await;

    // The user lets a save go, and it fails: a folder has taken the file's place.
    bench
        .files
        .delete(std::slice::from_ref(&path))
        .await
        .unwrap();
    bench
        .files
        .make_dir(&bench.folder, "draft.txt")
        .await
        .unwrap();
    std::fs::write(&edit.local_path, "the save the user let go\n").unwrap();
    bench
        .wait(&opened.id, "the held save", |edit| {
            edit.state == EditState::Changed
        })
        .await;
    bench.external.upload(&opened.id).unwrap();
    bench
        .wait(&opened.id, "the failure", |edit| {
            edit.state == EditState::Failed
        })
        .await;

    // With the way clear again, a later save is not what the user let go, so it waits for them.
    bench
        .files
        .delete(std::slice::from_ref(&path))
        .await
        .unwrap();
    write_through(&bench.files, &path, b"as it was on the server\n").await;
    std::fs::write(&edit.local_path, "a save nobody has seen yet\n").unwrap();
    bench
        .wait(&opened.id, "the later save held", |edit| {
            edit.state == EditState::Changed
        })
        .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(bench.edit(&opened.id).unwrap().state, EditState::Changed);
    assert_eq!(bench.read(&path).await, b"as it was on the server\n");

    bench.external.upload(&opened.id).unwrap();
    bench.wait_watching(&opened.id, 1).await;
    assert_eq!(bench.read(&path).await, b"a save nobody has seen yet\n");
    bench.finish().await;
}

#[tokio::test]
async fn vault_files_are_edited_as_plain_copies_and_wiped_when_the_vault_locks() {
    let Some(profile) = sftp_profile() else {
        return;
    };
    let bench = Bench::start(&profile, "vault").await;
    let vault = bench
        .sessions
        .create_vault(
            &bench.session.id,
            &bench.folder,
            "Vault",
            PASSWORD.to_string(),
            OWNER,
        )
        .await
        .unwrap();
    let vault_files = bench.sessions.get(&vault.id).await.unwrap().files();
    let path = remote_path::join(&vault.home, "secret plans.txt");
    write_through(&vault_files, &path, b"plain words inside the vault\n").await;

    let opened = match bench
        .external
        .open(OpenRequest {
            session_id: vault.id.clone(),
            ..bench.request(&path, "in-place")
        })
        .await
        .unwrap()
    {
        OpenOutcome::Opening { edit } => edit,
        other => panic!("expected the vault's file to open, got {other:?}"),
    };
    assert!(opened.in_vault);
    assert!(!opened.outlives_session);

    let edit = bench.wait_watching(&opened.id, 1).await;
    let local = PathBuf::from(&edit.local_path);
    // The program was given the decrypted file, in a folder nobody else can read.
    assert_eq!(std::fs::read(&local).unwrap(), b"rewritten in place\n");
    assert_eq!(mode_of(local.parent().unwrap()), 0o700);
    assert_eq!(mode_of(&local), 0o600);
    // The save went back into the vault encrypted: the vault reads it, and the server holds
    // neither the name nor the words.
    assert_eq!(
        read_through(&vault_files, &path).await,
        b"rewritten in place\n"
    );
    let mut pending = vec![vault.vault.as_ref().unwrap().root.clone()];
    while let Some(folder) = pending.pop() {
        for entry in bench.files.list_dir(&folder).await.unwrap().entries {
            assert!(!entry.name.contains("secret"), "{}", entry.name);
            let stored = remote_path::join(&folder, &entry.name);
            if bench.files.stat(&stored).await.unwrap().unwrap().is_dir {
                pending.push(stored);
            } else {
                let contents = bench.read(&stored).await;
                let plain = b"rewritten in place";
                assert!(!contents.windows(plain.len()).any(|window| window == plain));
            }
        }
    }

    // A save is pending as the vault is locked: the plain copy does not outlast the vault.
    std::fs::write(&local, "saved a moment before locking\n").unwrap();
    bench.sessions.disconnect(&vault.id).await.unwrap();
    bench.wait_gone(&opened.id).await;
    assert!(!local.parent().unwrap().exists());
    bench.finish().await;
}

#[tokio::test]
async fn local_files_open_where_they_are_and_programs_are_not_run() {
    let local = tempfile::tempdir().unwrap();
    let sessions = Arc::new(SessionManager::new(
        local.path().join("known_hosts"),
        Events::default(),
    ));
    let transfers = TransferManager::new(sessions, Events::default(), TransferSettings::default());
    let editing = EditingSettings {
        programs: stand_in_programs(local.path()),
        ..EditingSettings::default()
    };
    let external = ExternalEditManager::with_pace(
        local.path().join("editing"),
        transfers.files(),
        Events::default(),
        editing,
        quick_pace(),
    );
    let in_place = OpenWith::Program {
        id: "in-place".to_string(),
    };

    // The program is handed the file itself: nothing is copied or followed.
    let notes = local.path().join("notes $(touch injected).txt");
    std::fs::write(&notes, "as it was\n").unwrap();
    let path = notes.to_str().unwrap();
    assert_eq!(
        external.open_local(path, &in_place).await.unwrap(),
        OpenOutcome::Opened
    );
    let deadline = Instant::now() + WAIT;
    while std::fs::read(&notes).unwrap() != b"rewritten in place\n" {
        assert!(
            Instant::now() < deadline,
            "the program never saved the file"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(external.list().is_empty());
    assert!(!local.path().join("editing").exists());
    assert!(!local.path().join("injected").exists());

    // Neither a script by its name nor one by its content goes to the system to open.
    let named = local.path().join("deploy.sh");
    std::fs::write(&named, "echo deploying\n").unwrap();
    let disguised = local.path().join("readme.txt");
    std::fs::write(&disguised, "#!/bin/sh\necho not a readme\n").unwrap();
    for (file, name) in [(&named, "deploy.sh"), (&disguised, "readme.txt")] {
        assert_eq!(
            external
                .open_local(file.to_str().unwrap(), &OpenWith::Default)
                .await
                .unwrap(),
            OpenOutcome::NeedsConfirmation {
                name: name.to_string()
            }
        );
    }
    // Named by the user, an editor opens them.
    assert_eq!(
        external
            .open_local(disguised.to_str().unwrap(), &in_place)
            .await
            .unwrap(),
        OpenOutcome::Opened
    );

    assert!(external
        .open_local("notes.txt", &OpenWith::Default)
        .await
        .is_err());
    assert!(external
        .open_local(local.path().to_str().unwrap(), &in_place)
        .await
        .is_err());
}
