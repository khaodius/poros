//! End-to-end tests of the editor and the terminal against real servers, set up as
//! `sftp_server.rs` and `ftp_server.rs` describe. The SSH tests are skipped unless
//! `POROS_TEST_SSH_PORT` is set, the FTP tests unless `POROS_TEST_FTP_PORT` is, and WebDAV is
//! left out of the tests that take it unless `POROS_TEST_WEBDAV_PORT` is.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use poros_lib::automation::remote_command;
use poros_lib::editor::{EditorManager, FileLocation, SaveOutcome, SaveRequest};
use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use poros_lib::remote_path;
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

fn webdav_profile() -> Option<ConnectProfile> {
    server_profile(
        Protocol::WebDav,
        "POROS_TEST_WEBDAV_PORT",
        "POROS_TEST_WEBDAV",
    )
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
        rsync_path: None,
        s3: Default::default(),
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
            let approval = HostKeyApproval::of(&host_key, true);
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

/// The session a reconnect leaves in place of `session_id`, which is closed as the window does
/// once its tabs moved over.
async fn reconnect(sessions: &SessionManager, session_id: &str) -> String {
    let reconnected = sessions.reconnect(session_id, None, OWNER).await.unwrap();
    sessions.disconnect(session_id).await.unwrap();
    assert_ne!(reconnected.id, session_id);
    reconnected.id
}

#[tokio::test]
async fn files_save_through_the_session_that_took_the_place_of_theirs() {
    for profile in [ftp_profile(), webdav_profile()].into_iter().flatten() {
        let label = profile.protocol.display_name();
        let (_temp_dir, sessions, session_id) = connect(&profile).await;
        let files = sessions.get(&session_id).await.unwrap().files();
        let name = format!("editor-{}.txt", uuid::Uuid::new_v4());
        let path = remote_path::join(files.home(), &name);
        write_through(&files, &path, b"first\r\n").await;

        let editor = EditorManager::new(sessions.clone(), Events::default());
        let location = FileLocation::Remote {
            session_id: session_id.clone(),
            path: path.clone(),
        };
        let info = editor.open(location, OWNER).await.unwrap();
        let document = editor.load(&info.id, OWNER).await.unwrap();
        assert_eq!(document.text, "first\n", "{label}");
        drop(files);

        let reconnected = reconnect(&sessions, &session_id).await;
        let saved = editor
            .save(save_request(
                &info.id,
                "after a reconnect\n",
                Some(document.stamp),
            ))
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert!(matches!(saved, SaveOutcome::Saved { .. }), "{label}");
        let files = sessions.get(&reconnected).await.unwrap().files();
        assert_eq!(
            read_through(&files, &path).await,
            b"after a reconnect\r\n",
            "{label}"
        );
        drop(files);

        // With the server's tab closed nothing reaches the file, and the error says what does.
        sessions.disconnect(&reconnected).await.unwrap();
        let closed = editor
            .save(save_request(&info.id, "with no session\n", None))
            .await
            .unwrap_err();
        assert_eq!(closed.kind, ErrorKind::SessionNotFound, "{label}");
        assert!(
            closed.message.contains("Connect to it again"),
            "{label}: {}",
            closed.message
        );

        let reopened = sessions
            .connect(profile.clone(), None, OWNER)
            .await
            .unwrap();
        let saved = editor
            .save(save_request(&info.id, "after connecting again\n", None))
            .await
            .unwrap_or_else(|error| panic!("{label}: {}", error.message));
        assert!(matches!(saved, SaveOutcome::Saved { .. }), "{label}");
        let files = sessions.get(&reopened.id).await.unwrap().files();
        assert_eq!(
            read_through(&files, &path).await,
            b"after connecting again\r\n",
            "{label}"
        );

        files.delete(std::slice::from_ref(&path)).await.unwrap();
        editor.close(&info.id).await;
        sessions.disconnect_all().await;
    }
}

#[tokio::test]
async fn a_file_in_a_vault_saves_through_its_vault_or_not_at_all() {
    const PASSWORD: &str = "a vault's password";
    let Some(profile) = ftp_profile() else { return };
    let (_temp_dir, sessions, session_id) = connect(&profile).await;
    let server_files = sessions.get(&session_id).await.unwrap().files();
    let scratch = server_files
        .make_dir("~", &format!("editor-vault-{}", uuid::Uuid::new_v4()))
        .await
        .unwrap();
    let vault = sessions
        .create_vault(&session_id, &scratch, "Vault", PASSWORD.to_string(), OWNER)
        .await
        .unwrap();
    let root = remote_path::join(&scratch, "Vault");
    let path = remote_path::join(&root, "notes.txt");
    let vault_files = sessions.get(&vault.id).await.unwrap().files();
    write_through(&vault_files, &path, b"secret\n").await;
    drop(vault_files);

    let editor = EditorManager::new(sessions.clone(), Events::default());
    let location = FileLocation::Remote {
        session_id: vault.id.clone(),
        path: path.clone(),
    };
    let info = editor.open(location, OWNER).await.unwrap();
    let document = editor.load(&info.id, OWNER).await.unwrap();
    assert_eq!(document.text, "secret\n");

    // Locked, the vault's files are out of reach: the server's own session, which is still
    // open, must not be written to in the vault's place.
    sessions.disconnect(&vault.id).await.unwrap();
    let locked = editor
        .save(save_request(&info.id, "written past the vault\n", None))
        .await
        .unwrap_err();
    assert_eq!(locked.kind, ErrorKind::SessionNotFound);
    assert!(
        locked.message.contains("Unlock the vault again"),
        "{}",
        locked.message
    );
    assert_eq!(server_files.stat(&path).await.unwrap(), None);

    let unlocked = sessions
        .unlock_vault(&session_id, &root, PASSWORD.to_string(), OWNER)
        .await
        .unwrap();
    assert_ne!(unlocked.id, vault.id);
    let saved = editor
        .save(save_request(&info.id, "secret, edited\n", None))
        .await
        .unwrap();
    assert!(matches!(saved, SaveOutcome::Saved { .. }));
    let vault_files = sessions.get(&unlocked.id).await.unwrap().files();
    assert_eq!(
        read_through(&vault_files, &path).await,
        b"secret, edited\r\n"
    );
    assert_eq!(server_files.stat(&path).await.unwrap(), None);

    editor.close(&info.id).await;
    server_files
        .delete(std::slice::from_ref(&scratch))
        .await
        .unwrap();
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

/// A port that relays to the server until `closed` is set, then turns every new connection
/// away, as a server that allows one connection per client does.
async fn relay_until(server_port: u16, closed: Arc<AtomicBool>) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            if closed.load(Ordering::SeqCst) {
                continue;
            }
            tokio::spawn(async move {
                let address = ("127.0.0.1", server_port);
                if let Ok(mut server) = tokio::net::TcpStream::connect(address).await {
                    let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
                }
            });
        }
    });
    port
}

/// Whether a process with exactly this command line runs on the server.
async fn runs_on_server(sessions: &SessionManager, session_id: &str, command_line: &str) -> bool {
    let session = sessions.get(session_id).await.unwrap();
    let check = format!("pgrep -f -x '{command_line}'");
    let result = remote_command::run_on(
        &session,
        &check,
        None,
        &[],
        &tokio_util::sync::CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    result.exit_status == Some(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_terminal_on_the_browsing_connection_ends_its_shell_when_closed() {
    let Some(profile) = profile() else { return };
    let closed = Arc::new(AtomicBool::new(false));
    let relayed = ConnectProfile {
        port: relay_until(profile.port, closed.clone()).await,
        ..profile
    };
    let (_temp_dir, sessions, session_id) = connect(&relayed).await;
    closed.store(true, Ordering::SeqCst);
    let events = Events::keeping_startup_log();
    let terminals = TerminalManager::new(sessions.clone(), events.clone());
    let screen = Screen::default();
    let info = terminals
        .open(&session_id, 80, 24, screen.sink(), OWNER)
        .await
        .unwrap();

    // A process name no other test run shares, which ends by itself should a run fail.
    let sleeper = format!("sleep 300.{:06}", std::process::id() % 1_000_000);
    terminals
        .write(
            &info.id,
            format!("echo sleeping-$((6*7)); {sleeper}\n").into_bytes(),
        )
        .unwrap();
    screen.wait_for_text("sleeping-42").await;
    assert!(runs_on_server(&sessions, &session_id, &sleeper).await);

    terminals.close(&info.id).await;
    let deadline = Instant::now() + WAIT;
    while runs_on_server(&sessions, &session_id, &sleeper).await {
        if Instant::now() > deadline {
            let session = sessions.get(&session_id).await.unwrap();
            let _ = remote_command::run_on(
                &session,
                &format!("pkill -f -x '{sleeper}'"),
                None,
                &[],
                &tokio_util::sync::CancellationToken::new(),
                |_, _| {},
            )
            .await;
            panic!("the shell and {sleeper} still run on the server after the tab closed");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // The browsing connection is still there for everything else.
    assert!(!runs_on_server(&sessions, &session_id, &sleeper).await);
    // The shell's output is no longer read, although the browsing connection stays.
    let log: Vec<String> = events
        .take_startup_log()
        .into_iter()
        .map(|record| record.message)
        .collect();
    assert!(
        log.iter()
            .any(|message| message.contains("shares the browsing connection")),
        "the terminal got a connection of its own: {log:?}"
    );
    let closing_line = format!("Terminal on {} closed", info.label);
    assert!(
        log.contains(&closing_line),
        "the shell's output is still read: {log:?}"
    );
    sessions.disconnect_all().await;
}
