//! End-to-end tests of `poros-cli`, run as a program the way scripts and agents run it. The
//! tests that need a server are skipped unless `POROS_TEST_SSH_PORT` is set; the server is the
//! one `sftp_server.rs` describes. Share links are tested against the S3 server of
//! `http_storage.rs`, with `POROS_TEST_S3_PORT`.

use std::io::{Read, Write};
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
    /// Connection options every command of the run takes.
    options: Vec<&'static str>,
}

impl Run {
    fn new(connection: String, password: String, remote: String) -> Self {
        Self {
            config: tempfile::tempdir().unwrap(),
            local: tempfile::tempdir().unwrap(),
            connection,
            password,
            remote,
            options: Vec::new(),
        }
    }

    /// A run against the S3 server, with a bucket of its own as the scratch folder.
    fn object_storage(name: &str) -> Option<Self> {
        let port: u16 = std::env::var("POROS_TEST_S3_PORT").ok()?.parse().ok()?;
        let mut run = Self::new(
            format!("s3+http://poros-access-key@127.0.0.1:{port}"),
            "poros-secret-key".into(),
            format!("/poros-cli-{name}-{}", std::process::id()),
        );
        run.options = vec!["--s3-path-style"];
        run.run(&["rm", "-rf", &run.remote]);
        let made = run.run(&["mkdir", &run.remote]);
        assert_eq!(status(&made), 0, "{made:?}");
        Some(run)
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
            .args(&self.options)
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

/// `--host-key` pins the server asked for. `--trust-new-host`, there for new jump hosts, never
/// stands in for the pin, and a key on record does not outweigh it.
#[test]
fn a_host_key_pin_is_never_replaced() {
    let Some(run) = Run::online("pinned") else {
        return;
    };
    let (code, value) = run.json(&["info"]);
    assert_eq!(code, 0, "{value}");
    let fingerprint = value["hostKeyFingerprint"].as_str().unwrap().to_string();

    let fresh = Run::new(run.connection.clone(), run.password.clone(), String::new());
    let (code, value) = fresh.json(&[
        "--host-key",
        "SHA256:not-it",
        "--trust-new-host",
        "--save-host-key",
        "info",
    ]);
    assert_eq!(code, 5, "{value}");
    assert_eq!(
        value["error"]["hostKey"]["fingerprint"],
        fingerprint.as_str()
    );
    assert_eq!(
        value["error"]["hint"],
        "The server showed another key than the one given with --host-key"
    );
    assert_eq!(fresh.json(&["info"]).0, 5, "nothing was remembered");

    // On record, but not the key asked for.
    let (code, value) = run.json(&["--host-key", "SHA256:not-it", "info"]);
    assert_eq!(code, 6, "{value}");
    assert_eq!(value["error"]["kind"], "hostKeyChanged");
    assert_eq!(run.json(&["--host-key", &fingerprint, "info"]).0, 0);

    // Through a jump host that is new, which --trust-new-host accepts, to the pinned server.
    let jumping = Run::new(run.connection.clone(), run.password.clone(), String::new());
    let port: u16 = std::env::var("POROS_TEST_SSH_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let bastion = serde_json::json!([{
        "id": "bastion",
        "name": "bastion",
        "host": "localhost",
        "port": port,
        "username": std::env::var("POROS_TEST_SSH_USER").unwrap_or_else(|_| "poros".into()),
        "authType": "publicKey",
        "keyPath": fixture("openssh-ed25519"),
    }]);
    std::fs::write(
        jumping.config.path().join("connections.json"),
        bastion.to_string(),
    )
    .unwrap();
    let through = |pin: &str| {
        jumping.json(&[
            "-J",
            "bastion",
            "--host-key",
            pin,
            "--trust-new-host",
            "info",
        ])
    };
    let (code, value) = through("SHA256:not-it");
    assert_eq!(code, 5, "{value}");
    assert_eq!(value["error"]["hostKey"]["host"], "127.0.0.1");
    let (code, value) = through(&fingerprint);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["hostKeyFingerprint"], fingerprint.as_str());

    // A pin that names nothing, as a script exporting an empty POROS_HOST_KEY gives, is none.
    let output = run
        .command(&["--json", "info"])
        .env("POROS_HOST_KEY", " ")
        .output()
        .unwrap();
    assert_eq!(status(&output), 0, "{output:?}");
}

/// A server that shows no key of its own cannot be held to one: `--host-key` is refused before
/// anything is sent, rather than ignored.
#[test]
fn a_host_key_is_refused_where_no_key_can_be_checked() {
    let Some(port) = std::env::var("POROS_TEST_FTP_PORT").ok() else {
        return;
    };
    let user = std::env::var("POROS_TEST_FTP_USER").unwrap_or_else(|_| "poros".into());
    let password = std::env::var("POROS_TEST_FTP_PASSWORD").unwrap_or_else(|_| "poros-pass".into());
    let run = Run::new(
        format!("ftp://{user}@127.0.0.1:{port}"),
        password.clone(),
        String::new(),
    );
    let (code, value) = run.json(&["info"]);
    assert_eq!(code, 0, "{value}");
    let (code, value) = run.json(&["--host-key", "SHA256:not-it", "info"]);
    assert_eq!(code, 2, "{value}");
    assert_eq!(value["error"]["kind"], "invalidInput");
    assert_eq!(
        value["error"]["message"],
        "--host-key cannot be checked for FTP"
    );
    // Refused before connecting: a port nothing listens on gives the same answer.
    let closed = Run::new(format!("ftp://{user}@127.0.0.1:9"), password, String::new());
    let (code, value) = closed.json(&["--host-key", "SHA256:not-it", "info"]);
    assert_eq!(code, 2, "{value}");
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

/// A bare HTTP/1.1 `GET` over a socket: nothing but the address and the `Host` header go
/// out, so what comes back is what anyone holding the address gets. Returns the status, the
/// headers in lower case, and the body.
fn plain_get(address: &str) -> (u16, String, Vec<u8>) {
    let rest = address
        .strip_prefix("http://")
        .expect("a plain HTTP address");
    let (authority, target) = rest.split_once('/').unwrap();
    let mut stream = std::net::TcpStream::connect(authority).unwrap();
    write!(
        stream,
        "GET /{target} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let head_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("an HTTP response");
    let head = String::from_utf8_lossy(&response[..head_end]).to_ascii_lowercase();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head, response[head_end + 4..].to_vec())
}

#[test]
fn share_prints_links_that_open_without_credentials() {
    let Some(run) = Run::object_storage("share") else {
        return;
    };
    let contents = b"quarterly numbers\n".repeat(2000);
    let report = run.remote("reports/q3 report+final \u{e9}.txt");
    let notes = run.remote("notes.txt");
    let written = run.with_input(&["write", "-p", &report], &contents);
    assert_eq!(status(&written), 0, "{written:?}");
    let written = run.with_input(&["write", &notes], b"notes\n");
    assert_eq!(status(&written), 0, "{written:?}");

    // Plain output is the link and nothing else, ready for a pipe. Nothing on standard
    // error holds it, however much is logged.
    let output = run.run(&["-vv", "share", &report]);
    assert_eq!(status(&output), 0, "{output:?}");
    let link = String::from_utf8(output.stdout).unwrap();
    let link = link.strip_suffix('\n').expect("one line");
    assert!(!link.contains('\n') && link.starts_with("http://127.0.0.1:"));
    assert!(link.contains("/reports/q3%20report%2Bfinal%20%C3%A9.txt?X-Amz-Algorithm="));
    assert!(link.contains("&X-Amz-Expires=3600&"), "an hour by default");
    let logged = String::from_utf8_lossy(&output.stderr);
    assert!(logged.contains("Made a view link to"), "{logged}");
    assert!(!logged.contains("X-Amz-Signature") && !logged.contains("X-Amz-Credential"));
    let (code, _, body) = plain_get(link);
    assert_eq!(code, 200);
    assert_eq!(digest(&body), digest(&contents));

    let (code, value) = run.json(&["share", &report, "--expires", "2h", "--kind", "download"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["path"], report.as_str());
    assert_eq!(value["kind"], "download");
    assert_eq!(value["audience"], "anyone");
    assert_eq!(value["revocable"], false);
    assert_eq!(value["hasPassword"], false);
    assert_eq!(value["notes"], serde_json::json!([]));
    let expires_at = value["expiresAt"].as_str().unwrap();
    assert!(
        expires_at.ends_with('Z') && expires_at.len() == 20,
        "{expires_at}"
    );
    let address = value["url"].as_str().unwrap();
    assert!(address.contains("&X-Amz-Expires=7200&"));
    let (code, head, body) = plain_get(address);
    assert_eq!(code, 200);
    assert_eq!(digest(&body), digest(&contents));
    assert!(
        head.contains("content-disposition: attachment; filename="),
        "{head}"
    );

    // Several paths give one link each, and a failure does not hide the others.
    let missing = run.remote("missing.txt");
    let (code, value) = run.json(&["share", &notes, &missing, &report]);
    assert_eq!(code, 10, "{value}");
    let answers = value.as_array().unwrap();
    assert_eq!(answers.len(), 3);
    assert_eq!(plain_get(answers[0]["url"].as_str().unwrap()).2, b"notes\n");
    assert_eq!(answers[1]["error"]["kind"], "notFound");
    assert_eq!(answers[2]["path"], report.as_str());
    let output = run.run(&["share", &run.remote("*.txt")]);
    assert_eq!(status(&output), 0, "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 1);

    // A lifetime past the service's limit is cut to it, with a warning.
    let output = run.run(&["share", &notes, "--expires", "30d"]);
    assert_eq!(status(&output), 0, "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("&X-Amz-Expires=604800&"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("at most 7 days"));

    // What the service cannot do says so, with the status scripts check.
    let (code, value) = run.json(&["share", &notes, "--kind", "edit"]);
    assert_eq!(code, 2, "{value}");
    assert_eq!(
        value["error"]["message"],
        "S3 cannot make edit links; choose view or download"
    );
    let (code, value) = run.json(&["share", &run.remote("reports")]);
    assert_eq!(code, 12, "{value}");
    assert_eq!(value["error"]["kind"], "unsupported");
    let (code, value) = run.json(&["share", &notes, "--revoke"]);
    assert_eq!(code, 12, "{value}");
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("cannot be removed"));
    let (code, value) = run.json(&["share", &notes, "--expires", "soon"]);
    assert_eq!(code, 2, "{value}");
}

#[test]
fn share_is_refused_where_the_protocol_has_no_links() {
    let Some(run) = Run::online("share") else {
        return;
    };
    let note = run.remote("note.txt");
    let written = run.with_input(&["write", &note], b"hi\n");
    assert_eq!(status(&written), 0, "{written:?}");
    let (code, value) = run.json(&["share", &note]);
    assert_eq!(code, 12, "{value}");
    assert_eq!(value["error"]["kind"], "unsupported");
    assert_eq!(value["error"]["message"], "SFTP has no share links");
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

#[test]
fn a_source_named_twice_is_placed_once() {
    let Some(run) = Run::online("twice") else {
        return;
    };
    for folder in ["copied", "moved", "sent"] {
        let (code, _) = run.json(&["mkdir", &run.remote(folder)]);
        assert_eq!(code, 0);
    }
    run.write("p1.txt", "one\n");
    run.write("p2.txt", "two\n");
    let (code, value) = run.json(&[
        "cp",
        &run.remote("p*.txt"),
        &run.remote("p1.txt"),
        &run.remote("copied"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["placed"].as_array().unwrap().len(), 2, "{value}");
    assert_eq!(run.names("copied"), ["p1.txt", "p2.txt"]);
    let (code, value) = run.json(&[
        "mv",
        &run.remote("p1.txt"),
        &run.remote("p1.txt"),
        &run.remote("moved"),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.names("moved"), ["p1.txt"]);
    assert_eq!(run.read("moved/p1.txt").as_deref(), Some("one\n"));

    write_tree(run.local.path(), &[("local.txt", b"local\n")]);
    std::fs::create_dir(run.local("downloaded")).unwrap();
    let (code, value) = run.json(&["put", "local.txt", "./local.txt", &run.remote("sent")]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(run.names("sent"), ["local.txt"]);
    let two = run.remote("p2.txt");
    let (code, value) = run.json(&["get", &two, &two, "downloaded"]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(
        std::fs::read_to_string(run.local("downloaded/p2.txt")).unwrap(),
        "two\n"
    );
}

/// Root reads every folder, so where the tests run as root this is left to the unit test of
/// the report, which is given the plan such a folder makes.
#[cfg(unix)]
#[test]
fn verify_still_lists_files_only_on_the_server_when_a_local_folder_cannot_be_read() {
    use std::os::unix::fs::PermissionsExt;

    let Some(run) = Run::online("unreadlocal") else {
        return;
    };
    write_tree(
        run.local.path(),
        &[
            ("site/index.html", b"<h1>poros</h1>\n"),
            ("site/private/key", b"secret"),
        ],
    );
    let (code, value) = run.json(&["put", "site", &run.remote("")]);
    assert_eq!(code, 0, "{value}");
    run.write("site/stale.txt", "stale\n");
    let private = run.local("site/private");
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::read_dir(&private).is_ok();
    let compared = (!readable).then(|| run.json(&["verify", "site", &run.remote("site")]));
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o755)).unwrap();
    let Some((code, value)) = compared else {
        return;
    };
    assert_eq!(code, 11, "{value}");
    assert_eq!(value["onlyRemote"][0]["path"], "stale.txt", "{value}");
    assert_eq!(value["notCompared"], serde_json::json!(["private"]));
}

#[test]
fn rm_and_mv_never_take_the_home_or_start_folder_or_everything_at_the_top() {
    let Some(run) = Run::online("own") else {
        return;
    };
    let (_, info) = run.json(&["info"]);
    let start = format!(
        "{}/{}/start",
        info["home"].as_str().unwrap().trim_end_matches('/'),
        run.remote
    );
    let (code, _) = run.json(&["mkdir", "-p", &run.remote("start/keep")]);
    assert_eq!(code, 0);
    run.write("start/keep/file.txt", "kept\n");
    // Every command starts in `start`, as with a saved connection's own folder.
    let connection = format!("{}{start}", run.connection);
    let inside = |arguments: &[&str]| {
        let output = run
            .command(&[&["--json"], arguments].concat())
            .env("POROS_CONNECTION", &connection)
            .output()
            .unwrap();
        (status(&output), parsed(&output))
    };
    let start_slash = format!("{start}/");
    // Patterns that match nothing, so a regression could never delete anything there.
    let refused: [(&[&str], &str); 8] = [
        (&["rm", "-r", start.as_str()], "starts in"),
        (&["rm", "-rf", start_slash.as_str()], "starts in"),
        (&["mv", start.as_str(), "keep"], "starts in"),
        (&["mv", "~", "keep"], "home folder"),
        (&["mv", "~/", "keep"], "home folder"),
        (
            &["rm", "-rf", "/poros-nothing-is-named-this-*"],
            "top of the server",
        ),
        (&["rm", "-f", "/*.poros-never-a-name"], "top of the server"),
        (&["rm", "-rf", "//poros-nothing-??"], "top of the server"),
    ];
    for (arguments, says) in refused {
        let (code, value) = inside(arguments);
        assert_eq!(code, 2, "{arguments:?}: {value}");
        assert_eq!(value["error"]["kind"], "invalidInput", "{arguments:?}");
        let message = value["error"]["message"].as_str().unwrap();
        assert!(message.contains(says), "{arguments:?}: {message}");
        assert_eq!(run.names(""), ["start"], "{arguments:?}");
        assert_eq!(run.names("start"), ["keep"], "{arguments:?}");
        assert_eq!(run.names("start/keep"), ["file.txt"], "{arguments:?}");
    }
    // What is inside them is still named as usual.
    let (code, value) = inside(&["rm", "-r", &format!("{start}/keep")]);
    assert_eq!(code, 0, "{value}");
    assert!(run.names("start").is_empty());
}
