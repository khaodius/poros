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
        run.run(&["rm", "-rf", &run.remote]);
        let made = run.run(&["mkdir", "-p", &run.remote]);
        assert_eq!(status(&made), 0, "{made:?}");
        Some(run)
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
