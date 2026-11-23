//! End-to-end tests of `poros-cli`, run as a program the way scripts and agents run it. The
//! tests that need a server are skipped unless `POROS_TEST_SSH_PORT` is set; the server is the
//! one `sftp_server.rs` describes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The passphrase of the encrypted key fixtures.
const FIXTURE_PASSPHRASE: &str = "poros-test";

/// Variables a developer's shell may set that would change what a run does.
const AMBIENT: [&str; 12] = [
    "POROS_CONNECTION",
    "POROS_USER",
    "POROS_PASSWORD",
    "POROS_PASSPHRASE",
    "POROS_IDENTITY",
    "POROS_HOST_KEY",
    "POROS_TRUST_NEW_HOST",
    "POROS_JSON",
    "POROS_CONFIG_DIR",
    "POROS_NO_KEYCHAIN",
    "SSH_AUTH_SOCK",
    "RUST_LOG",
];

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/keys")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn status(output: &Output) -> i32 {
    output.status.code().expect("poros-cli ended by a signal")
}

fn parsed(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "not one JSON document ({error}):\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A config folder of its own, a local working folder, and a scratch folder on the server.
struct Run {
    config: tempfile::TempDir,
    local: tempfile::TempDir,
    connection: String,
    password: String,
    remote: String,
}

impl Run {
    fn new(connection: String, password: String, remote: String) -> Self {
        Self {
            config: tempfile::tempdir().unwrap(),
            local: tempfile::tempdir().unwrap(),
            connection,
            password,
            remote,
        }
    }

    fn offline() -> Self {
        Self::new(String::new(), String::new(), String::new())
    }

    /// Trusts the server's host key in this run's config folder and starts with an empty
    /// scratch folder.
    fn online(name: &str) -> Option<Self> {
        let port: u16 = std::env::var("POROS_TEST_SSH_PORT").ok()?.parse().ok()?;
        let user = std::env::var("POROS_TEST_SSH_USER").unwrap_or_else(|_| "poros".into());
        let run = Self::new(
            format!("sftp://{user}@127.0.0.1:{port}"),
            std::env::var("POROS_TEST_SSH_PASSWORD").unwrap_or_else(|_| "poros-pass".into()),
            format!("poros-cli-{name}-{}", std::process::id()),
        );
        let trusted = run.run(&["--trust-new-host", "--save-host-key", "info"]);
        assert_eq!(status(&trusted), 0, "{trusted:?}");
        run.empty_scratch_folder();
        Some(run)
    }

    /// The same on the plain FTP server `ftp_server.rs` describes, for what a server does
    /// without SFTP. Skipped unless `POROS_TEST_FTP_PORT` is set.
    fn online_over_ftp(name: &str) -> Option<Self> {
        let port: u16 = std::env::var("POROS_TEST_FTP_PORT").ok()?.parse().ok()?;
        let user = std::env::var("POROS_TEST_FTP_USER").unwrap_or_else(|_| "poros".into());
        let run = Self::new(
            format!("ftp://{user}@127.0.0.1:{port}"),
            std::env::var("POROS_TEST_FTP_PASSWORD").unwrap_or_else(|_| "poros-pass".into()),
            format!("poros-cli-{name}-{}", std::process::id()),
        );
        run.empty_scratch_folder();
        Some(run)
    }

    fn empty_scratch_folder(&self) {
        self.run(&["rm", "-rf", &self.remote]);
        let made = self.run(&["mkdir", "-p", &self.remote]);
        assert_eq!(status(&made), 0, "{made:?}");
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_poros-cli"));
        for name in AMBIENT {
            command.env_remove(name);
        }
        command
            .args(arguments)
            .current_dir(self.local.path())
            .env("POROS_CONFIG_DIR", self.config.path())
            .env("POROS_NO_KEYCHAIN", "1")
            .stdin(Stdio::null());
        if !self.connection.is_empty() {
            command
                .env("POROS_CONNECTION", &self.connection)
                .env("POROS_PASSWORD", &self.password);
        }
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }

    fn json(&self, arguments: &[&str]) -> (i32, Value) {
        let output = self.run(&[&["--json"], arguments].concat());
        (status(&output), parsed(&output))
    }

    fn with_input(&self, arguments: &[&str], input: &[u8]) -> Output {
        let mut child = self
            .command(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }

    fn remote(&self, path: &str) -> String {
        format!("{}/{path}", self.remote)
    }

    /// Writes a file in the scratch folder on the server.
    fn write(&self, path: &str, contents: &str) {
        let written = self.with_input(&["write", &self.remote(path)], contents.as_bytes());
        assert_eq!(status(&written), 0, "{written:?}");
    }

    /// What a file in the scratch folder on the server holds; `None` when it is not there.
    fn read(&self, path: &str) -> Option<String> {
        let shown = self.run(&["cat", &self.remote(path)]);
        (status(&shown) == 0).then(|| String::from_utf8(shown.stdout).unwrap())
    }

    /// The names in a folder of the scratch folder on the server, hidden ones included.
    fn names(&self, path: &str) -> Vec<String> {
        let (code, listed) = self.json(&["ls", "-a", &self.remote(path)]);
        assert_eq!(code, 0, "{listed}");
        listed["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_string())
            .collect()
    }

    fn local(&self, path: &str) -> PathBuf {
        self.local.path().join(path)
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if !self.remote.is_empty() {
            self.run(&["rm", "-rf", &self.remote]);
        }
    }
}

fn write_tree(root: &Path, files: &[(&str, &[u8])]) {
    for (path, contents) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
}

#[test]
fn usage_errors_are_json_when_json_is_asked_for() {
    let run = Run::offline();
    let (code, value) = run.json(&["frobnicate"]);
    assert_eq!(code, 2);
    assert_eq!(value["error"]["kind"], "invalidInput");
    assert_eq!(value["error"]["exitCode"], 2);

    let (code, value) = run.json(&["ls"]);
    assert_eq!(code, 2);
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--connection"));

    let help = run.run(&["--help"]);
    assert_eq!(status(&help), 0);
    assert!(String::from_utf8_lossy(&help.stdout).contains("Exit status"));
}

#[test]
fn host_keys_are_refused_until_trusted() {
    let Some(run) = Run::online("hostkeys") else {
        return;
    };
    // No scratch folder, so nothing to clean up when it is dropped.
    let fresh = Run::new(run.connection.clone(), run.password.clone(), String::new());
    let (code, value) = fresh.json(&["info"]);
    assert_eq!(code, 5, "{value}");
    assert_eq!(value["error"]["kind"], "hostKeyUnknown");
    let fingerprint = value["error"]["hostKey"]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(fingerprint.starts_with("SHA256:"));
    assert!(value["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("--host-key"));

    let (code, _) = fresh.json(&["--host-key", "SHA256:not-it", "info"]);
    assert_eq!(code, 5);
    let (code, value) = fresh.json(&["--host-key", &fingerprint, "info"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["hostKeyFingerprint"], fingerprint.as_str());
    // Accepting a key once does not remember it.
    assert_eq!(fresh.json(&["info"]).0, 5);

    let (code, _) = fresh.json(&["--trust-new-host", "--save-host-key", "info"]);
    assert_eq!(code, 0);
    assert_eq!(fresh.json(&["info"]).0, 0);
}

#[test]
fn sign_in_failures_exit_with_the_auth_status() {
    let Some(run) = Run::online("auth") else {
        return;
    };
    let output = run
        .command(&["--json", "info"])
        .env("POROS_PASSWORD", "not-the-password")
        .output()
        .unwrap();
    assert_eq!(status(&output), 4, "{output:?}");
    assert_eq!(parsed(&output)["error"]["kind"], "authFailed");

    let key = fixture("openssh-ed25519");
    let (code, value) = run.json(&["-i", &key, "info"]);
    assert_eq!(code, 0, "{value}");

    let encrypted = fixture("openssh-ed25519-enc");
    let (code, value) = run.json(&["-i", &encrypted, "info"]);
    assert_eq!(code, 4, "{value}");
    assert!(value["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("POROS_PASSPHRASE"));
    let output = run
        .command(&["--json", "-i", &encrypted, "info"])
        .env("POROS_PASSPHRASE", FIXTURE_PASSPHRASE)
        .output()
        .unwrap();
    assert_eq!(status(&output), 0, "{output:?}");
}

#[test]
fn files_go_up_and_down_and_verify() {
    let Some(run) = Run::online("files") else {
        return;
    };
    let note = b"hello from poros-cli\n".to_vec();
    let written = run.with_input(
        &["write", "--parents", &run.remote("notes/hello.txt")],
        &note,
    );
    assert_eq!(status(&written), 0, "{written:?}");
    let shown = run.run(&["cat", &run.remote("notes/hello.txt")]);
    assert_eq!(shown.stdout, note);
    let (code, value) = run.json(&["stat", &run.remote("notes/hello.txt")]);
    assert_eq!(code, 0);
    assert_eq!(value["size"], note.len());
    assert_eq!(value["kind"], "file");

    let (code, value) = run.json(&["checksum", &run.remote("notes/hello.txt")]);
    assert_eq!(code, 0);
    assert_eq!(value["digest"], digest(&note).as_str());

    let large: Vec<u8> = (0..3_000_000u32).map(|index| (index % 251) as u8).collect();
    let small_files: Vec<(String, Vec<u8>)> = (0..20)
        .map(|index| (format!("site/img/p{index}.bin"), vec![index as u8; 4096]))
        .collect();
    let mut tree: Vec<(&str, &[u8])> = small_files
        .iter()
        .map(|(path, contents)| (path.as_str(), contents.as_slice()))
        .collect();
    tree.push(("site/big.bin", &large));
    tree.push(("site/index.html", b"<h1>poros</h1>\n"));
    write_tree(run.local.path(), &tree);

    let (code, value) = run.json(&["put", "-w", "4", "site", &run.remote("")]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["done"], 22);
    let (code, value) = run.json(&["verify", "site", &run.remote("site")]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["same"], true);

    let (code, value) = run.json(&["get", "-w", "4", &run.remote("site"), "back"]);
    assert_eq!(code, 0, "{value}");
    for (path, contents) in &tree {
        let copied = run.local(&path.replacen("site", "back", 1));
        assert_eq!(std::fs::read(copied).unwrap(), *contents, "{path}");
    }

    std::fs::write(run.local("site/index.html"), b"<h1>changed</h1>\n").unwrap();
    let (code, value) = run.json(&["verify", "site", &run.remote("site")]);
    assert_eq!(code, 11, "{value}");
    assert_eq!(value["different"][0]["path"], "index.html");

    let (code, value) = run.json(&["ls", "-R", &run.remote("site")]);
    assert_eq!(code, 0);
    assert_eq!(value["entries"].as_array().unwrap().len(), 23);
}

#[test]
fn server_side_changes_report_each_path() {
    let Some(run) = Run::online("manage") else {
        return;
    };
    for name in ["a.txt", "b.txt", "keep.log"] {
        let written = run.with_input(&["write", &run.remote(name)], name.as_bytes());
        assert_eq!(status(&written), 0, "{written:?}");
    }
    let (code, _) = run.json(&["mkdir", "-p", &run.remote("folder/inner")]);
    assert_eq!(code, 0);

    let (code, value) = run.json(&["mv", &run.remote("a.txt"), &run.remote("renamed.txt")]);
    assert_eq!(code, 0, "{value}");
    let (code, _) = run.json(&["stat", &run.remote("a.txt")]);
    assert_eq!(code, 7);

    let (code, value) = run.json(&["cp", &run.remote("b.txt"), &run.remote("renamed.txt")]);
    assert_eq!(code, 9, "{value}");
    let (code, value) = run.json(&[
        "cp",
        "--conflict",
        "keep-both",
        &run.remote("b.txt"),
        &run.remote("folder"),
    ]);
    assert_eq!(code, 0, "{value}");

    let (code, value) = run.json(&["chmod", "600", &run.remote("b.txt")]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.json(&["stat", &run.remote("b.txt")]).1["mode"], "0600");
    let (code, _) = run.json(&["chmod", "u+x,go+r", &run.remote("b.txt")]);
    assert_eq!(code, 0);
    assert_eq!(run.json(&["stat", &run.remote("b.txt")]).1["mode"], "0744");

    let (code, value) = run.json(&["rm", &run.remote("folder")]);
    assert_eq!(code, 2, "{value}");
    let (code, value) = run.json(&["rm", &run.remote("missing"), &run.remote("keep.log")]);
    assert_eq!(code, 10, "{value}");
    let (code, value) = run.json(&["rm", "-rf", &run.remote("*.txt"), &run.remote("folder")]);
    assert_eq!(code, 0, "{value}");
    let (_, listed) = run.json(&["ls", &run.remote("")]);
    assert_eq!(listed["entries"].as_array().unwrap().len(), 0, "{listed}");
}

#[test]
fn sync_mirrors_and_holds_back_deletes_from_an_empty_source() {
    let Some(run) = Run::online("sync") else {
        return;
    };
    write_tree(
        run.local.path(),
        &[("source/a.txt", b"one\n"), ("source/sub/b.txt", b"two\n")],
    );
    let target = run.remote("mirror");
    let (code, value) = run.json(&["sync", "-d", "up", "source", &target]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["copied"], 2);
    let (code, value) = run.json(&["sync", "-d", "up", "source", &target]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["plan"]["counts"]["unchanged"], 2);

    std::fs::remove_file(run.local("source/sub/b.txt")).unwrap();
    let (code, value) = run.json(&["sync", "-d", "up", "--delete", "source", &target]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["deleted"], 1);
    assert_eq!(run.json(&["stat", &run.remote("mirror/sub/b.txt")]).0, 7);

    std::fs::create_dir(run.local("empty")).unwrap();
    let (code, value) = run.json(&["sync", "-d", "up", "--delete", "empty", &target]);
    assert_eq!(code, 10, "{value}");
    assert!(value["heldBack"].is_string());
    assert_eq!(run.json(&["stat", &run.remote("mirror/a.txt")]).0, 0);

    let (code, value) = run.json(&["sync", "-d", "down", "copy", &target]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(std::fs::read(run.local("copy/a.txt")).unwrap(), b"one\n");
}

#[test]
fn exec_exits_with_the_remote_status() {
    let Some(run) = Run::online("exec") else {
        return;
    };
    let output = run.run(&["exec", "--", "echo out; echo err >&2; exit 3"]);
    assert_eq!(status(&output), 3);
    assert_eq!(output.stdout, b"out\n");
    assert_eq!(output.stderr, b"err\n");

    let (code, value) = run.json(&["exec", "--cwd", &run.remote(""), "pwd"]);
    assert_eq!(code, 0);
    assert!(value["stdout"]
        .as_str()
        .unwrap()
        .trim_end()
        .ends_with(&run.remote));
}

#[test]
fn batch_answers_every_line_in_order() {
    let Some(run) = Run::online("batch") else {
        return;
    };
    let lines = [
        serde_json::json!(["mkdir", "-p", run.remote("b")]),
        serde_json::json!({ "id": "note", "args": ["write", run.remote("b/n.txt")], "input": "hi\n" }),
        serde_json::json!({ "args": ["write", run.remote("b/raw")], "inputBase64": "/wA=" }),
        serde_json::json!(["cat", run.remote("b/n.txt")]),
        serde_json::json!(["stat", run.remote("b/missing")]),
        serde_json::json!(["batch"]),
    ];
    let mut input = String::from("# comments and blank lines are passed over\n\n");
    for line in &lines {
        input.push_str(&line.to_string());
        input.push('\n');
    }
    input.push_str("not json\n");
    let output = run.with_input(&["batch"], input.as_bytes());
    assert_eq!(status(&output), 10, "{output:?}");
    let answers: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(answers.len(), 7);
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(answers[1]["id"], "note");
    assert_eq!(answers[2]["result"]["bytes"], 2);
    assert_eq!(answers[3]["result"]["content"], "hi\n");
    assert_eq!(answers[4]["ok"], false);
    assert_eq!(answers[4]["exitCode"], 7);
    assert_eq!(answers[5]["exitCode"], 2);
    assert_eq!(answers[6]["error"]["kind"], "invalidInput");
}

#[test]
fn replacing_keeps_the_old_file_until_the_new_one_is_in_place() {
    let Some(run) = Run::online("replace") else {
        return;
    };
    // One file under two spellings is not replaced with itself.
    run.write("notes.txt", "notes\n");
    for command in ["mv", "cp"] {
        let (code, value) = run.json(&[
            command,
            "--conflict",
            "replace",
            &run.remote("notes.txt"),
            &run.remote("./notes.txt"),
        ]);
        assert_eq!(code, 2, "{command}: {value}");
        assert_eq!(
            run.read("notes.txt").as_deref(),
            Some("notes\n"),
            "{command}"
        );
    }

    // A copy that cannot be made leaves the file it was to replace.
    run.write("unreadable.txt", "unreadable\n");
    run.write("report.txt", "old\n");
    let (code, _) = run.json(&["chmod", "000", &run.remote("unreadable.txt")]);
    assert_eq!(code, 0);
    let (code, value) = run.json(&[
        "cp",
        "--conflict",
        "replace",
        &run.remote("unreadable.txt"),
        &run.remote("report.txt"),
    ]);
    assert_ne!(code, 0, "{value}");
    assert_eq!(run.read("report.txt").as_deref(), Some("old\n"));
    // Nor is the copy it was writing left behind.
    assert_eq!(run.names(""), ["notes.txt", "report.txt", "unreadable.txt"]);

    // So does a move that cannot be made: nothing leaves a folder that cannot be written.
    let (code, _) = run.json(&["mkdir", &run.remote("locked")]);
    assert_eq!(code, 0);
    run.write("locked/new.txt", "new\n");
    let (code, _) = run.json(&["chmod", "555", &run.remote("locked")]);
    assert_eq!(code, 0);
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "replace",
        &run.remote("locked/new.txt"),
        &run.remote("report.txt"),
    ]);
    run.json(&["chmod", "755", &run.remote("locked")]);
    assert_ne!(code, 0, "{value}");
    assert_eq!(run.read("report.txt").as_deref(), Some("old\n"));
    assert_eq!(run.read("locked/new.txt").as_deref(), Some("new\n"));

    // Replacing puts the new contents under the old name, in the same folder or another.
    run.write("draft.txt", "draft\n");
    let (code, value) = run.json(&[
        "cp",
        "--conflict",
        "replace",
        &run.remote("draft.txt"),
        &run.remote("report.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.read("report.txt").as_deref(), Some("draft\n"));
    assert_eq!(run.read("draft.txt").as_deref(), Some("draft\n"));
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "replace",
        &run.remote("draft.txt"),
        &run.remote("notes.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert!(value["placed"][0].as_str().unwrap().ends_with("/notes.txt"));
    assert_eq!(run.read("notes.txt").as_deref(), Some("draft\n"));
    assert_eq!(run.read("draft.txt"), None);
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "replace",
        &run.remote("notes.txt"),
        &run.remote("locked/new.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.read("locked/new.txt").as_deref(), Some("draft\n"));
    assert_eq!(run.read("notes.txt"), None);

    // A folder does not replace a file.
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "replace",
        &run.remote("locked"),
        &run.remote("report.txt"),
    ]);
    assert_eq!(code, 9, "{value}");
    assert_eq!(run.read("report.txt").as_deref(), Some("draft\n"));
    assert_eq!(run.names("locked"), ["new.txt"]);
}

#[test]
fn keeping_both_numbers_the_name_asked_for() {
    let Some(run) = Run::online("keepboth") else {
        return;
    };
    run.write("a.txt", "a\n");
    run.write("b.txt", "b\n");
    let (code, value) = run.json(&[
        "cp",
        "--conflict",
        "keep-both",
        &run.remote("a.txt"),
        &run.remote("b.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert!(value["placed"][0].as_str().unwrap().ends_with("/b (2).txt"));
    assert_eq!(run.read("b (2).txt").as_deref(), Some("a\n"));

    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "keep-both",
        &run.remote("a.txt"),
        &run.remote("b.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert!(value["placed"][0].as_str().unwrap().ends_with("/b (3).txt"));
    assert_eq!(run.read("b (3).txt").as_deref(), Some("a\n"));
    assert_eq!(run.read("b.txt").as_deref(), Some("b\n"));
    assert_eq!(run.names(""), ["b (2).txt", "b (3).txt", "b.txt"]);
}

#[test]
fn renaming_without_sftp_replaces_safely() {
    let Some(run) = Run::online_over_ftp("ftp-replace") else {
        return;
    };
    run.write("notes.txt", "notes\n");
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "replace",
        &run.remote("notes.txt"),
        &run.remote("./notes.txt"),
    ]);
    assert_eq!(code, 2, "{value}");
    assert_eq!(run.read("notes.txt").as_deref(), Some("notes\n"));

    run.write("draft.txt", "draft\n");
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "replace",
        &run.remote("draft.txt"),
        &run.remote("notes.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.read("notes.txt").as_deref(), Some("draft\n"));
    // Neither the source nor the file moved aside is left behind.
    assert_eq!(run.names(""), ["notes.txt"]);

    run.write("more.txt", "more\n");
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "keep-both",
        &run.remote("more.txt"),
        &run.remote("notes.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.read("notes (2).txt").as_deref(), Some("more\n"));
    assert_eq!(run.names(""), ["notes (2).txt", "notes.txt"]);

    // A folder does not replace a file.
    let (code, _) = run.json(&["mkdir", &run.remote("folder")]);
    assert_eq!(code, 0);
    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "replace",
        &run.remote("folder"),
        &run.remote("notes.txt"),
    ]);
    assert_eq!(code, 9, "{value}");
    assert_eq!(run.read("notes.txt").as_deref(), Some("draft\n"));
}

#[test]
fn copying_without_sftp_is_refused_and_renames_nothing() {
    let Some(run) = Run::online_over_ftp("ftp-copy") else {
        return;
    };
    run.write("one.txt", "one\n");
    let (code, value) = run.json(&["cp", &run.remote("one.txt"), &run.remote("two.txt")]);
    assert_eq!(code, 12, "{value}");
    assert_eq!(run.names(""), ["one.txt"]);
    let (code, value) = run.json(&["mv", &run.remote("one.txt"), &run.remote("two.txt")]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.names(""), ["two.txt"]);
}

#[test]
fn sources_with_one_name_never_replace_each_other() {
    let Some(run) = Run::online("samename") else {
        return;
    };
    for folder in ["in/a", "in/b", "archive", "kept", "skipped"] {
        let (code, _) = run.json(&["mkdir", "-p", &run.remote(folder)]);
        assert_eq!(code, 0);
    }
    run.write("in/a/report.csv", "first\n");
    run.write("in/b/report.csv", "second\n");
    let first = run.remote("in/a/report.csv");
    let second = run.remote("in/b/report.csv");

    // Nothing is placed when one would have to give way to the other.
    for options in [
        &["mv"][..],
        &["cp"],
        &["mv", "--conflict", "replace"],
        &["cp", "--conflict", "replace"],
    ] {
        let archive = run.remote("archive");
        let (code, value) = run.json(&[options, &[&first, &second, &archive]].concat());
        assert_eq!(code, 9, "{options:?}: {value}");
        assert!(run.names("archive").is_empty(), "{options:?}");
        assert_eq!(run.read("in/a/report.csv").as_deref(), Some("first\n"));
        assert_eq!(run.read("in/b/report.csv").as_deref(), Some("second\n"));
    }

    // Transfers are refused the same way, whatever --exists says.
    write_tree(
        run.local.path(),
        &[("a/report.csv", b"first\n"), ("b/report.csv", b"second\n")],
    );
    std::fs::create_dir(run.local("downloaded")).unwrap();
    for exists in ["overwrite", "rename"] {
        let archive = run.remote("archive");
        let (code, value) = run.json(&[
            "put",
            "--exists",
            exists,
            "a/report.csv",
            "b/report.csv",
            &archive,
        ]);
        assert_eq!(code, 9, "put --exists {exists}: {value}");
        assert!(run.names("archive").is_empty(), "{exists}");
        let (code, value) = run.json(&["get", "--exists", exists, &first, &second, "downloaded"]);
        assert_eq!(code, 9, "get --exists {exists}: {value}");
        assert_eq!(
            std::fs::read_dir(run.local("downloaded")).unwrap().count(),
            0
        );
    }

    let (code, value) = run.json(&[
        "cp",
        "--conflict",
        "keep-both",
        &first,
        &second,
        &run.remote("kept"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.names("kept"), ["report (2).csv", "report.csv"]);
    assert_eq!(run.read("kept/report.csv").as_deref(), Some("first\n"));
    assert_eq!(run.read("kept/report (2).csv").as_deref(), Some("second\n"));

    let (code, value) = run.json(&[
        "mv",
        "--conflict",
        "skip",
        &first,
        &second,
        &run.remote("skipped"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["skipped"], 1);
    assert_eq!(run.read("skipped/report.csv").as_deref(), Some("first\n"));
    assert_eq!(run.read("in/b/report.csv").as_deref(), Some("second\n"));
    assert_eq!(run.read("in/a/report.csv"), None);
}

#[test]
fn an_empty_path_is_never_the_folder_the_connection_starts_in() {
    let Some(run) = Run::online("empty") else {
        return;
    };
    let (_, info) = run.json(&["info"]);
    let user = info["username"].as_str().unwrap().to_string();
    let start = format!(
        "{}/{}/start",
        info["home"].as_str().unwrap().trim_end_matches('/'),
        run.remote
    );
    let (code, _) = run.json(&["mkdir", "-p", &run.remote("start/keep")]);
    assert_eq!(code, 0);
    run.write("start/keep/file.txt", "kept\n");
    write_tree(
        run.local.path(),
        &[
            ("local.txt", b"local\n"),
            ("folder/inside.txt", b"inside\n"),
        ],
    );
    // Every command from here on starts in `start`, as with a saved connection's own folder.
    let connection = format!("{}{start}", run.connection);
    let inside = |arguments: &[&str]| {
        let output = run
            .command(&[&["--json"], arguments].concat())
            .env("POROS_CONNECTION", &connection)
            .output()
            .unwrap();
        (status(&output), parsed(&output))
    };

    // What a script leaves when a variable is unset, and the dots `rm` and `mv` refuse.
    let refused: [&[&str]; 20] = [
        &["rm", "-rf", ""],
        &["rm", "-r", " "],
        &["rm", "-rf", "keep", ""],
        &["rm", "-r", "."],
        &["rm", "-rf", "./"],
        &["rm", "-r", ".."],
        &["rm", "-r", "keep/.."],
        &["chmod", "-R", "700", ""],
        &["chown", "-R", user.as_str(), ""],
        &["mv", "", "elsewhere"],
        &["mv", ".", "elsewhere"],
        &["mv", "keep", ""],
        &["cp", "", "elsewhere"],
        &["cp", "keep/file.txt", ""],
        &["mkdir", "-p", ""],
        &["write", ""],
        &["get", "", "downloaded"],
        &["put", "local.txt", ""],
        &["sync", "-d", "up", "--delete", "folder", ""],
        &["exec", "--cwd", "", "--", "rm", "-rf", "keep"],
    ];
    // New folders take the server's umask, so the mode to keep is whatever it is now.
    let (_, before) = run.json(&["stat", &run.remote("start")]);
    assert!(before["mode"].is_string(), "{before}");
    for arguments in refused {
        let (code, value) = inside(arguments);
        assert_eq!(code, 2, "{arguments:?}: {value}");
        assert_eq!(value["error"]["kind"], "invalidInput", "{arguments:?}");
        assert_eq!(run.names(""), ["start"], "{arguments:?}");
        assert_eq!(run.names("start"), ["keep"], "{arguments:?}");
        assert_eq!(run.names("start/keep"), ["file.txt"], "{arguments:?}");
    }
    assert!(!run.local("downloaded").exists());
    let (_, kept) = run.json(&["stat", &run.remote("start")]);
    assert_eq!(kept["mode"], before["mode"], "{kept}");

    let mut batch = run
        .command(&["batch"])
        .env("POROS_CONNECTION", &connection)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    batch
        .stdin
        .take()
        .unwrap()
        .write_all(b"[\"rm\", \"-rf\", \"\"]\n")
        .unwrap();
    let answer: Value = serde_json::from_slice(&batch.wait_with_output().unwrap().stdout).unwrap();
    assert_eq!(answer["exitCode"], 2, "{answer}");
    assert_eq!(run.names("start"), ["keep"]);

    // The folder itself is listed with no path at all, and named with a dot.
    for arguments in [
        &["ls"][..],
        &["ls", "."],
        &["ls", ""],
        &["stat", "."],
        &["du", "."],
        &["chmod", "750", "."],
        &["cp", "keep/file.txt", "."],
        &["put", "local.txt", "."],
    ] {
        let (code, value) = inside(arguments);
        assert_eq!(code, 0, "{arguments:?}: {value}");
    }
    assert_eq!(run.names("start"), ["file.txt", "keep", "local.txt"]);
    let (_, changed) = run.json(&["stat", &run.remote("start")]);
    assert_eq!(changed["mode"], "0750", "{changed}");
}

#[test]
fn rm_force_passes_over_only_what_is_not_there() {
    let Some(run) = Run::online("force") else {
        return;
    };
    run.write("app.lock", "lock\n");
    run.write("keep.txt", "keep\n");
    let (code, value) = run.json(&[
        "rm",
        "-f",
        &run.remote("none*.bak"),
        &run.remote("app.lock"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["deleted"].as_array().unwrap().len(), 1, "{value}");
    assert_eq!(run.names(""), ["keep.txt"]);

    let (code, value) = run.json(&[
        "rm",
        "-f",
        &run.remote("none*.bak"),
        &run.remote("missing.txt"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert!(value["deleted"].as_array().unwrap().is_empty(), "{value}");

    // Without -f, a pattern that matches nothing stops the command before it deletes.
    let (code, value) = run.json(&["rm", &run.remote("none*.bak"), &run.remote("keep.txt")]);
    assert_eq!(code, 7, "{value}");
    assert_eq!(run.names(""), ["keep.txt"]);
}

#[test]
fn verify_names_what_it_could_not_compare() {
    let Some(run) = Run::online("unread") else {
        return;
    };
    write_tree(
        run.local.path(),
        &[
            ("site/index.html", b"<h1>poros</h1>\n"),
            ("site/photos/a.jpg", b"a"),
            ("site/photos/b.jpg", b"b"),
        ],
    );
    let (code, value) = run.json(&["put", "site", &run.remote("")]);
    assert_eq!(code, 0, "{value}");
    let (code, value) = run.json(&["verify", "site", &run.remote("site")]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["same"], true);

    // The server's photos folder cannot be listed, so nothing in it is compared.
    let (code, _) = run.json(&["chmod", "000", &run.remote("site/photos")]);
    assert_eq!(code, 0);
    let unread = run.json(&["verify", "site", &run.remote("site")]);
    let shown = run.run(&["verify", "site", &run.remote("site")]);
    std::fs::write(run.local("site/index.html"), b"<h1>changed</h1>\n").unwrap();
    let differing = run.json(&["verify", "site", &run.remote("site")]);
    run.json(&["chmod", "755", &run.remote("site/photos")]);

    let (code, value) = unread;
    assert_eq!(code, 10, "{value}");
    assert_eq!(value["same"], false, "{value}");
    assert_eq!(value["notCompared"], serde_json::json!(["photos"]));
    assert_eq!(status(&shown), 10);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("not compared: photos"), "{text}");
    assert!(!text.contains("Same contents"), "{text}");

    // A difference that was found is still what the status reports.
    let (code, value) = differing;
    assert_eq!(code, 11, "{value}");
    assert_eq!(value["different"][0]["path"], "index.html");
    assert_eq!(value["notCompared"], serde_json::json!(["photos"]));
}

#[test]
fn one_item_that_cannot_be_placed_exits_with_its_own_status() {
    let Some(run) = Run::online("single") else {
        return;
    };
    for folder in ["locked", "open"] {
        let (code, _) = run.json(&["mkdir", &run.remote(folder)]);
        assert_eq!(code, 0);
    }
    run.write("locked/a.txt", "a\n");
    run.write("open/b.txt", "b\n");
    let (code, _) = run.json(&["chmod", "555", &run.remote("locked")]);
    assert_eq!(code, 0);
    let renamed = run.json(&[
        "mv",
        &run.remote("locked/a.txt"),
        &run.remote("locked/c.txt"),
    ]);
    let moved = run.json(&["mv", &run.remote("locked/a.txt"), &run.remote("open")]);
    let copied = run.json(&["cp", &run.remote("open/b.txt"), &run.remote("locked")]);
    run.json(&["chmod", "755", &run.remote("locked")]);
    for (what, (code, value)) in [("rename", renamed), ("move", moved), ("copy", copied)] {
        assert_eq!(code, 8, "{what}: {value}");
        assert_eq!(
            value["error"]["kind"], "permissionDenied",
            "{what}: {value}"
        );
    }
    assert_eq!(run.names("locked"), ["a.txt"]);
    assert_eq!(run.names("open"), ["b.txt"]);
}

#[test]
fn a_link_that_leads_nowhere_still_takes_its_name() {
    let Some(run) = Run::online("dangling") else {
        return;
    };
    run.write("one.txt", "one\n");
    let (code, _) = run.json(&["mkdir", &run.remote("sub")]);
    assert_eq!(code, 0);
    for link in ["dangling.txt", "sub/one.txt"] {
        let (code, value) = run.json(&[
            "exec",
            "--cwd",
            &run.remote(""),
            "--",
            "ln",
            "-s",
            "nowhere",
            link,
        ]);
        assert_eq!(code, 0, "{value}");
    }
    // Without --conflict, nothing that is there is replaced or renamed around.
    for command in ["mv", "cp"] {
        for destination in ["dangling.txt", "sub", "sub/one.txt"] {
            let (code, value) =
                run.json(&[command, &run.remote("one.txt"), &run.remote(destination)]);
            assert_eq!(code, 9, "{command} {destination}: {value}");
            assert_eq!(value["error"]["kind"], "alreadyExists", "{command}");
            assert_eq!(run.names(""), ["dangling.txt", "one.txt", "sub"]);
            assert_eq!(run.names("sub"), ["one.txt"]);
        }
    }
    assert_eq!(run.read("one.txt").as_deref(), Some("one\n"));
}

#[test]
fn a_file_is_never_replaced_with_itself_through_a_link() {
    let Some(run) = Run::online("itself") else {
        return;
    };
    let (code, _) = run.json(&["mkdir", &run.remote("real")]);
    assert_eq!(code, 0);
    run.write("target.txt", "target\n");
    run.write("real/notes.txt", "notes\n");
    for (link, leads_to) in [("link.txt", "target.txt"), ("alias", "real")] {
        let (code, value) = run.json(&[
            "exec",
            "--cwd",
            &run.remote(""),
            "--",
            "ln",
            "-s",
            leads_to,
            link,
        ]);
        assert_eq!(code, 0, "{value}");
    }
    // A link onto the file it leads to, and one file through a linked folder, given as the
    // file or as the folder it is in.
    for command in ["mv", "cp"] {
        for (source, destination) in [
            ("link.txt", "target.txt"),
            ("alias/notes.txt", "real/notes.txt"),
            ("alias/notes.txt", "real"),
        ] {
            let (code, value) = run.json(&[
                command,
                "--conflict",
                "replace",
                &run.remote(source),
                &run.remote(destination),
            ]);
            assert_eq!(code, 2, "{command} {source} {destination}: {value}");
            assert_eq!(value["error"]["kind"], "invalidInput", "{value}");
            assert_eq!(run.read("target.txt").as_deref(), Some("target\n"));
            assert_eq!(run.read("real/notes.txt").as_deref(), Some("notes\n"));
            assert_eq!(run.names(""), ["alias", "link.txt", "real", "target.txt"]);
            assert_eq!(run.names("real"), ["notes.txt"]);
        }
    }
}
