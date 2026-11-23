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
fn find_searches_below_a_folder_by_name_size_and_kind() {
    let Some(run) = Run::online("find") else {
        return;
    };
    for (path, size) in [
        ("site/index.html", 120),
        ("site/logs/access.log", 3_000),
        ("site/logs/old/access.log.1", 9_000),
        ("site/logs/old/Error.LOG", 10),
        ("site/.git/packed.log", 5),
        ("site/it's -rf $(reboot).log", 1),
    ] {
        let (folder, _) = path.rsplit_once('/').unwrap();
        let (code, _) = run.json(&["mkdir", "-p", &run.remote(folder)]);
        assert_eq!(code, 0);
        let written = run.with_input(&["write", &run.remote(path)], &vec![b'x'; size]);
        assert_eq!(status(&written), 0, "{written:?}");
    }
    let site = run.remote("site");
    let found = |value: &Value| -> Vec<String> {
        let prefix = format!("/{site}/");
        value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                let path = entry["path"].as_str().unwrap();
                path.split_once(&prefix).unwrap().1.to_string()
            })
            .collect()
    };

    let (code, value) = run.json(&["find", "*.log", &site]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["engine"], "find", "the server's find did the search");
    assert_eq!(value["capped"], false);
    assert_eq!(
        found(&value),
        [
            "it's -rf $(reboot).log",
            "logs/access.log",
            "logs/old/Error.LOG"
        ]
    );
    assert_eq!(value["entries"][1]["size"], 3_000);
    assert_eq!(value["entries"][1]["kind"], "file");

    let (_, value) = run.json(&["find", "--case-sensitive", "--hidden", "*.log", &site]);
    assert_eq!(
        found(&value),
        [
            ".git/packed.log",
            "it's -rf $(reboot).log",
            "logs/access.log"
        ]
    );

    let (_, value) = run.json(&["find", "access", &site, "--min-size", "4K"]);
    assert_eq!(found(&value), ["logs/old/access.log.1"]);
    let (_, value) = run.json(&["find", "", &site, "--max-size", "100", "--type", "file"]);
    assert_eq!(
        found(&value),
        ["it's -rf $(reboot).log", "logs/old/Error.LOG"]
    );

    let (_, value) = run.json(&["find", "-E", r"^access\.log(\.\d+)?$", &site]);
    assert_eq!(found(&value), ["logs/access.log", "logs/old/access.log.1"]);

    let (_, value) = run.json(&["find", "", &site, "--type", "folder", "--max-depth", "2"]);
    assert_eq!(found(&value), [".git", "logs", "logs/old"]);

    let (_, value) = run.json(&["find", "", &site, "--newer", "1h", "--type", "f"]);
    assert_eq!(found(&value).len(), 5, "everything was just written");
    let (_, value) = run.json(&["find", "", &site, "--older", "2001-09-09"]);
    assert_eq!(found(&value).len(), 0);

    let (code, value) = run.json(&["find", "", &site, "--limit", "2", "--hidden"]);
    assert_eq!(code, 0);
    assert_eq!(value["capped"], true);
    assert_eq!(value["entries"].as_array().unwrap().len(), 2);

    let text = run.run(&["find", "*.log", &site, "--max-depth", "2"]);
    assert_eq!(status(&text), 0, "{text:?}");
    let listed = String::from_utf8(text.stdout).unwrap();
    let lines: Vec<&str> = listed.lines().collect();
    assert_eq!(lines.len(), 2, "{listed}");
    assert!(
        lines[0].ends_with("/site/it's -rf $(reboot).log"),
        "{listed}"
    );
    assert!(lines[1].ends_with("/site/logs/access.log"), "{listed}");

    let long = run.run(&["find", "-l", "index", &site]);
    let listed = String::from_utf8(long.stdout).unwrap();
    assert!(listed.starts_with("-rw"), "{listed}");
    assert!(listed.contains(" 120 "), "{listed}");

    let (code, value) = run.json(&["find", "-E", "(", &site]);
    assert_eq!(code, 2, "{value}");
    assert_eq!(value["error"]["kind"], "invalidInput");
    let (code, value) = run.json(&["find", "x", &site, "--min-size", "huge"]);
    assert_eq!(code, 2, "{value}");
    let (code, value) = run.json(&["find", "x", &run.remote("missing")]);
    assert_eq!(code, 7, "{value}");

    let batch = run.with_input(
        &["batch"],
        format!("[\"find\", \"*.html\", \"{site}\"]\n").as_bytes(),
    );
    assert_eq!(status(&batch), 0, "{batch:?}");
    let answer: Value = serde_json::from_slice(&batch.stdout).unwrap();
    assert_eq!(
        answer.to_string().matches("index.html").count(),
        2,
        "{answer}"
    );
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

impl Run {
    /// `schedule` as cron would run it: no connection or password in the environment.
    fn schedule(&self, arguments: &[&str]) -> Command {
        let mut command = self.command(&[&["schedule"], arguments].concat());
        command
            .env_remove("POROS_CONNECTION")
            .env_remove("POROS_PASSWORD");
        command
    }

    fn schedule_json(&self, arguments: &[&str]) -> (i32, Value) {
        let output = self
            .schedule(&[&["--json"], arguments].concat())
            .output()
            .unwrap();
        (status(&output), parsed(&output))
    }

    fn write_config(&self, name: &str, value: &Value) {
        std::fs::write(
            self.config.path().join(name),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }

    fn tasks(&self) -> Vec<Value> {
        let (code, value) = self.schedule_json(&["list"]);
        assert_eq!(code, 0, "{value}");
        value.as_array().unwrap().clone()
    }

    fn task(&self, name: &str) -> Value {
        self.tasks()
            .into_iter()
            .find(|task| task["name"] == name)
            .unwrap_or_else(|| panic!("no task {name}"))
    }

    fn wait_until_running(&self, name: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while self.task(name)["running"] != true {
            assert!(std::time::Instant::now() < deadline, "{name} never started");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn scheduled_tasks_from_0_7_0_are_listed() {
    let run = Run::offline();
    let output = run.schedule(&["list"]).output().unwrap();
    assert_eq!(status(&output), 0, "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("No scheduled tasks"));
    // Looking leaves no trace in a config folder without tasks.
    assert_eq!(std::fs::read_dir(run.config.path()).unwrap().count(), 0);

    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schedules-0.7.0.json"),
        run.config.path().join("schedules.json"),
    )
    .unwrap();
    let output = run.schedule(&["list"]).output().unwrap();
    assert_eq!(status(&output), 0, "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("NAME") && lines[0].contains("NEXT RUN"));
    assert!(lines[1].starts_with("Nightly site upload  sync"));
    assert!(lines[1].contains("Every day at 02:00") && lines[1].contains("ok: 12 files copied"));
    assert!(lines[2].contains("Every 6 hours") && lines[2].contains(" off "));
    assert!(lines[2].contains("failed: Command exited with status 1"));

    let tasks = run.tasks();
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0]["kind"], "sync");
    assert_eq!(tasks[0]["nextRun"], "2026-11-24T02:00:00Z");
    assert_eq!(tasks[0]["lastRun"]["succeeded"], true);
    assert_eq!(tasks[1]["enabled"], false);
    assert_eq!(tasks[1]["nextRun"], Value::Null);
    assert_eq!(
        tasks[2]["description"],
        "Run wall 'Moving to the new server tonight'"
    );
    assert_eq!(tasks[2]["lastRun"], Value::Null);

    let (code, value) = run.schedule_json(&["run", "weekly"]);
    assert_eq!(code, 7, "{value}");
    assert_eq!(value["error"]["kind"], "notFound");
    let (code, value) = run.schedule_json(&["enable", "rotate LOGS"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value[0]["enabled"], true);
    assert!(value[0]["nextRun"].is_string());
    let help = run.run(&["schedule", "--help"]);
    let help = String::from_utf8_lossy(&help.stdout);
    for part in [
        "*/5 * * * * poros-cli schedule run --due",
        "OnCalendar=",
        "13 for one already running",
    ] {
        assert!(help.contains(part), "{help}");
    }
}

#[test]
fn scheduled_tasks_run_now_when_due_and_never_twice_at_once() {
    let Some(run) = Run::online("schedule") else {
        return;
    };
    let (code, info) = run.json(&["info"]);
    assert_eq!(code, 0, "{info}");
    let home = info["home"]
        .as_str()
        .unwrap()
        .trim_end_matches('/')
        .to_string();
    let remote = format!("{home}/{}", run.remote);
    let port: u16 = std::env::var("POROS_TEST_SSH_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let user = std::env::var("POROS_TEST_SSH_USER").unwrap_or_else(|_| "poros".into());
    let saved = |id: &str, name: &str, auth: &str| {
        serde_json::json!({
            "id": id,
            "protocol": "sftp",
            "name": name,
            "host": "127.0.0.1",
            "port": port,
            "username": user,
            "authType": auth,
            "keyPath": fixture("openssh-ed25519"),
            "saveSecret": auth == "password",
        })
    };
    run.write_config(
        "connections.json",
        &serde_json::json!([
            saved("by-key", "test server", "publicKey"),
            saved("by-password", "with password", "password"),
        ]),
    );
    write_tree(
        run.local.path(),
        &[
            ("site/index.html", b"<h1>scheduled</h1>\n"),
            ("site/css/site.css", &[7u8; 40_000]),
            ("notes.txt", b"remember\n"),
        ],
    );
    let site = run.local("site").to_string_lossy().into_owned();
    let notes = run.local("notes.txt").to_string_lossy().into_owned();
    let restored = run.local("restored").to_string_lossy().into_owned();
    let every_hour = serde_json::json!({ "type": "every", "minutes": 60, "start": 0 });
    let task = |id: &str, name: &str, next_run: i64, action: Value| {
        serde_json::json!({
            "id": id,
            "name": name,
            "enabled": true,
            "trigger": every_hour,
            "action": action,
            "runMissed": true,
            "nextRun": next_run,
        })
    };
    run.write_config(
        "schedules.json",
        &serde_json::json!([
            task(
                "upload",
                "Backup",
                now() - 30,
                serde_json::json!({
                    "type": "transfer",
                    "connectionId": "by-key",
                    "direction": "upload",
                    "items": [{ "path": site, "isDir": true }, { "path": notes }],
                    "destination": remote,
                    "exists": "overwrite",
                    "subfolder": "backup-{date}",
                })
            ),
            task(
                "download",
                "Restore",
                now() + 3600,
                serde_json::json!({
                    "type": "transfer",
                    "connectionId": "by-key",
                    "direction": "download",
                    "items": [{ "path": format!("{remote}/latest"), "isDir": true }],
                    "destination": restored,
                })
            ),
            task(
                "slow",
                "Slow",
                now() + 3600,
                serde_json::json!({
                    "type": "command",
                    "connectionId": "by-key",
                    "command": "sleep 3; echo slept",
                })
            ),
            task(
                "long",
                "Long",
                now() + 3600,
                serde_json::json!({
                    "type": "command",
                    "connectionId": "by-key",
                    "command": "sleep 20; echo slept",
                })
            ),
            task(
                "password",
                "Needs a password",
                now() + 3600,
                serde_json::json!({
                    "type": "command",
                    "connectionId": "by-password",
                    "command": "echo signed in",
                })
            ),
        ]),
    );

    // Only the task whose time has come runs, once.
    let (code, value) = run.schedule_json(&["run", "--due"]);
    assert_eq!(code, 0, "{value}");
    let reports = value.as_array().unwrap();
    assert_eq!(reports.len(), 1, "{value}");
    assert_eq!(reports[0]["name"], "Backup");
    assert_eq!(reports[0]["state"], "succeeded");
    let message = reports[0]["message"].as_str().unwrap();
    assert!(message.starts_with("3 files uploaded ("), "{message}");
    let dated = message.rsplit_once(" to ").unwrap().1.to_string();
    assert!(dated.starts_with(&format!("{remote}/backup-20")), "{dated}");
    let shown = run.run(&["cat", &format!("{dated}/site/index.html")]);
    assert_eq!(shown.stdout, b"<h1>scheduled</h1>\n");
    let backup = run.task("Backup");
    assert_eq!(backup["lastRun"]["message"], message);
    assert!(backup["nextRun"].as_str().unwrap() > backup["lastRun"]["started"].as_str().unwrap());
    let output = run.schedule(&["run", "--due"]).output().unwrap();
    assert_eq!(status(&output), 0, "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "No task is due"
    );

    // A task asked for by name runs whenever; this one downloads what the backup uploaded.
    let (code, value) = run.json(&["mv", &dated, &format!("{remote}/latest")]);
    assert_eq!(code, 0, "{value}");
    let output = run.schedule(&["run", "restore"]).output().unwrap();
    assert_eq!(status(&output), 0, "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("Restore: 3 files downloaded"));
    assert_eq!(
        std::fs::read(run.local("restored/latest/site/css/site.css")).unwrap(),
        [7u8; 40_000]
    );
    assert_eq!(
        std::fs::read(run.local("restored/latest/notes.txt")).unwrap(),
        b"remember\n"
    );

    // While one run of a task goes on, another is refused with a status of its own.
    let first = run
        .schedule(&["run", "Slow"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    run.wait_until_running("Slow");
    let (code, value) = run.schedule_json(&["run", "slow"]);
    assert_eq!(code, 13, "{value}");
    assert_eq!(value[0]["state"], "alreadyRunning");
    let first = first.wait_with_output().unwrap();
    assert_eq!(status(&first), 0, "{first:?}");
    assert_eq!(
        String::from_utf8_lossy(&first.stdout).trim(),
        "Slow: Command finished: slept"
    );
    assert_eq!(run.task("Slow")["running"], false);

    // SIGTERM, as systemctl stop sends, stops a run the way Ctrl+C does, and it is written down.
    #[cfg(unix)]
    {
        let stopped = run
            .schedule(&["run", "Long"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        run.wait_until_running("Long");
        let killed = Command::new("kill")
            .args(["-TERM", &stopped.id().to_string()])
            .status()
            .unwrap();
        assert!(killed.success());
        let stopped = stopped.wait_with_output().unwrap();
        assert_eq!(status(&stopped), 130, "{stopped:?}");
        let long = run.task("Long");
        assert_eq!(long["running"], false);
        assert_eq!(long["lastRun"]["succeeded"], false);
        assert_eq!(long["lastRun"]["message"], "Stopped before it finished");
        assert!(long["lastRun"].get("underWay").is_none(), "{long}");
    }

    // A run whose process is killed outright leaves the result written as it began: a failure,
    // not the result of the run before.
    let mut killed = run
        .schedule(&["run", "Long"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    run.wait_until_running("Long");
    killed.kill().unwrap();
    killed.wait().unwrap();
    let long = run.task("Long");
    assert_eq!(long["running"], false);
    assert_eq!(long["lastRun"]["succeeded"], false);
    assert_eq!(long["lastRun"]["message"], "Stopped before it finished");
    assert_eq!(long["lastRun"]["underWay"], true, "{long}");

    // Without a keychain, a connection that needs its password says how to give one.
    let (code, value) = run.schedule_json(&["run", "Needs a password"]);
    assert_eq!(code, 4, "{value}");
    let message = value[0]["message"].as_str().unwrap();
    assert!(
        message.contains("--connection \"with password\"") && message.contains("POROS_PASSWORD"),
        "{message}"
    );
    let output = run
        .schedule(&["--json", "-c", "with password", "run", "Needs a password"])
        .env("POROS_PASSWORD", &run.password)
        .output()
        .unwrap();
    assert_eq!(status(&output), 0, "{output:?}");
    assert_eq!(parsed(&output)[0]["message"], "Command finished: signed in");
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&run.password));

    let (code, value) = run.schedule_json(&["disable", "Backup"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.task("Backup")["enabled"], false);
}
