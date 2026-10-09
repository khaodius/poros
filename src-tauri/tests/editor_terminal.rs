//! End-to-end tests of the editor and the terminal against real servers, set up as
//! `sftp_server.rs` and `ftp_server.rs` describe. The SSH tests are skipped unless
//! `POROS_TEST_SSH_PORT` is set, the FTP test unless `POROS_TEST_FTP_PORT` is.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::editor::{EditorManager, FileLocation, SaveOutcome, SaveRequest};
use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::session::SessionManager;
use poros_lib::sftp::RemoteFs;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::terminal::{Sink, TerminalEvent, TerminalManager};
use poros_lib::text::{LineEnding, TextEncoding};

const OWNER: &str = "main";
const WAIT: Duration = Duration::from_secs(15);

fn profile() -> Option<ConnectProfile> {
    server_profile(Protocol::Sftp, "POROS_TEST_SSH_PORT", "POROS_TEST_SSH")
}

fn ftp_profile() -> Option<ConnectProfile> {
    server_profile(Protocol::Ftp, "POROS_TEST_FTP_PORT", "POROS_TEST_FTP")
}

/// The test server named by `port_variable`, logging in as `{prefix}_USER`.
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
        route: Route::default(),
    })
}

/// A session on the test server, trusting its host key for this manager.
async fn connect(profile: &ConnectProfile) -> (tempfile::TempDir, Arc<SessionManager>, String) {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(SessionManager::new(
        temp_dir.path().join("known_hosts"),
        Events::default(),
    ));
    let info = match manager.connect(profile.clone(), None, OWNER).await {
        Ok(info) => info,
        Err(error) => {
            let host_key = error
                .host_key
                .expect("the server should only ask about its key");
            let approval = HostKeyApproval {
                fingerprint: host_key.fingerprint,
                remember: true,
            };
            manager
                .connect(profile.clone(), Some(approval), OWNER)
                .await
                .unwrap()
        }
    };
    (temp_dir, manager, info.id)
}

async fn write_file(fs: &RemoteFs, path: &str, contents: &[u8]) {
    let handle = fs.open_for_write(path, true, None).await.unwrap();
    fs.write_chunk(&handle, 0, contents.to_vec()).await.unwrap();
    fs.close_handle(handle).await.unwrap();
}

async fn read_file(fs: &RemoteFs, path: &str) -> Vec<u8> {
    let handle = fs.open_for_read(path).await.unwrap();
    let mut contents = Vec::new();
    while let poros_lib::sftp::ReadChunk::Data(data) = fs
        .read_chunk(&handle, contents.len() as u64, 64 * 1024)
        .await
        .unwrap()
    {
        contents.extend(data);
    }
    fs.close_handle(handle).await.unwrap();
    contents
}

fn save_request(
    document_id: &str,
    text: &str,
    expected: Option<poros_lib::editor::FileStamp>,
) -> SaveRequest {
    SaveRequest {
        document_id: document_id.to_string(),
        text: text.to_string(),
        encoding: TextEncoding::Utf8,
        line_ending: LineEnding::Crlf,
        expected,
    }
}

#[tokio::test]
async fn server_files_edit_save_and_survive_a_closed_session() {
    let Some(profile) = profile() else { return };
    let (_temp_dir, sessions, session_id) = connect(&profile).await;
    let session = sessions.get(&session_id).await.unwrap();
    let fs = session.sftp().unwrap().clone();
    let path = format!("{}/editor-{}.conf", fs.home, uuid::Uuid::new_v4());
    write_file(&fs, &path, b"listen 80;\r\nroot /srv;\r\n").await;

    let editor = EditorManager::new(sessions.clone(), Events::default());
    let location = FileLocation::Remote {
        session_id: session_id.clone(),
        path: path.clone(),
    };
    let info = editor.open(location, OWNER).await.unwrap();
    assert!(info.name.starts_with("editor-"));
    let document = editor.load(&info.id, OWNER).await.unwrap();
    assert_eq!(document.text, "listen 80;\nroot /srv;\n");
    assert_eq!(document.line_ending, LineEnding::Crlf);

    let saved = editor
        .save(save_request(
            &info.id,
            "listen 443;\nroot /srv;\n",
            Some(document.stamp),
        ))
        .await
        .unwrap();
    let SaveOutcome::Saved { stamp } = saved else {
        panic!("the first save should go through");
    };
    assert_eq!(
        read_file(&fs, &path).await,
        b"listen 443;\r\nroot /srv;\r\n"
    );

    write_file(&fs, &path, b"edited on the server meanwhile\n").await;
    let outcome = editor
        .save(save_request(&info.id, "mine\n", Some(stamp)))
        .await
        .unwrap();
    assert!(matches!(outcome, SaveOutcome::Changed { current: Some(_) }));
    assert_eq!(
        read_file(&fs, &path).await,
        b"edited on the server meanwhile\n"
    );

    drop((session, fs));
    sessions.disconnect(&session_id).await.unwrap();
    let outcome = editor
        .save(save_request(&info.id, "saved after the tab closed\n", None))
        .await
        .unwrap();
    assert!(matches!(outcome, SaveOutcome::Saved { .. }));

    let (_check_dir, check_sessions, check_id) = connect(&profile).await;
    let check = check_sessions.get(&check_id).await.unwrap();
    let check_fs = check.sftp().unwrap();
    assert_eq!(
        read_file(check_fs, &path).await,
        b"saved after the tab closed\r\n"
    );
    check_fs.delete(std::slice::from_ref(&path)).await.unwrap();
    editor.close(&info.id).await;
    check_sessions.disconnect_all().await;
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

#[tokio::test]
async fn ftp_files_edit_and_save_through_their_session() {
    let Some(profile) = ftp_profile() else { return };
    let (_temp_dir, sessions, session_id) = connect(&profile).await;
    let files = sessions.get(&session_id).await.unwrap().files();
    let path = format!("{}/editor-{}.txt", files.home(), uuid::Uuid::new_v4());
    write_through(&files, &path, b"first\r\nsecond\r\n").await;

    let editor = EditorManager::new(sessions.clone(), Events::default());
    let location = FileLocation::Remote {
        session_id: session_id.clone(),
        path: path.clone(),
    };
    let info = editor.open(location, OWNER).await.unwrap();
    let document = editor.load(&info.id, OWNER).await.unwrap();
    assert_eq!(document.text, "first\nsecond\n");
    assert_eq!(document.line_ending, LineEnding::Crlf);

    let saved = editor
        .save(save_request(
            &info.id,
            "first\nthird\n",
            Some(document.stamp),
        ))
        .await
        .unwrap();
    let SaveOutcome::Saved { stamp } = saved else {
        panic!("the first save should go through");
    };
    assert_eq!(read_through(&files, &path).await, b"first\r\nthird\r\n");

    write_through(&files, &path, b"edited on the server meanwhile\n").await;
    let outcome = editor
        .save(save_request(&info.id, "mine\n", Some(stamp)))
        .await
        .unwrap();
    assert!(matches!(outcome, SaveOutcome::Changed { current: Some(_) }));
    assert_eq!(
        read_through(&files, &path).await,
        b"edited on the server meanwhile\n"
    );

    let terminals = TerminalManager::new(sessions.clone(), Events::default());
    let refused = terminals
        .open(&session_id, 80, 24, Screen::default().sink(), OWNER)
        .await
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::Unsupported);

    files.delete(std::slice::from_ref(&path)).await.unwrap();
    editor.close(&info.id).await;
    sessions.disconnect_all().await;
}

/// Collects a terminal's output and tells when text shows up in it.
#[derive(Clone, Default)]
struct Screen {
    output: Arc<Mutex<String>>,
    exits: Arc<Mutex<Vec<String>>>,
}

impl Screen {
    fn sink(&self) -> Sink {
        let screen = self.clone();
        Arc::new(move |event| match event {
            TerminalEvent::Output { data } => screen.output.lock().unwrap().push_str(&data),
            TerminalEvent::Exit { message } => screen.exits.lock().unwrap().push(message),
        })
    }

    async fn wait_for(&self, check: impl Fn(&Screen) -> bool, what: &str) {
        let deadline = Instant::now() + WAIT;
        while !check(self) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; output so far: {:?}",
                self.output.lock().unwrap()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn wait_for_text(&self, text: &str) {
        self.wait_for(|screen| screen.output.lock().unwrap().contains(text), text)
            .await;
    }
}

#[tokio::test]
async fn terminals_run_a_shell_that_outlives_the_session() {
    let Some(profile) = profile() else { return };
    let (_temp_dir, sessions, session_id) = connect(&profile).await;
    let terminals = TerminalManager::new(sessions.clone(), Events::default());
    let screen = Screen::default();
    let info = terminals
        .open(&session_id, 100, 30, screen.sink(), OWNER)
        .await
        .unwrap();

    terminals
        .write(&info.id, b"echo poros-$((6*7)) $TERM\n".to_vec())
        .unwrap();
    screen.wait_for_text("poros-42 xterm-256color").await;

    terminals.resize(&info.id, 132, 40).unwrap();
    terminals.write(&info.id, b"stty size\n".to_vec()).unwrap();
    screen.wait_for_text("40 132").await;

    sessions.disconnect(&session_id).await.unwrap();
    terminals
        .write(&info.id, b"echo still-$((40+2))\n".to_vec())
        .unwrap();
    screen.wait_for_text("still-42").await;

    let moved = Screen::default();
    terminals
        .attach(&info.id, moved.sink(), "workspace-test")
        .unwrap();
    moved.wait_for_text("poros-42").await;

    terminals.write(&info.id, b"exit 3\n".to_vec()).unwrap();
    moved
        .wait_for(
            |screen| !screen.exits.lock().unwrap().is_empty(),
            "the exit",
        )
        .await;
    assert_eq!(
        moved.exits.lock().unwrap()[0],
        "The shell exited with status 3"
    );
    assert!(terminals.write(&info.id, b"echo gone\n".to_vec()).is_err());

    terminals.restart(&info.id).await.unwrap();
    terminals
        .write(&info.id, b"echo restarted-$((1+1))\n".to_vec())
        .unwrap();
    moved.wait_for_text("restarted-2").await;

    terminals.close_owned_by("workspace-test").await;
    assert!(terminals
        .write(&info.id, b"echo closed\n".to_vec())
        .is_err());
}
