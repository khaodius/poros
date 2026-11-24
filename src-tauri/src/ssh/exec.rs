//! Commands Poros runs on a server over an SSH exec channel.
//!
//! The server hands the line of an exec request to the account's login shell, which need not be
//! a POSIX shell: fish takes `\'` and `\\` inside single quotes for escapes, csh ends a quoted
//! word at a newline and expands `!`, nushell, xonsh and PowerShell quote in ways of their own.
//! A name quoted for `sh` on that line could then leave its quotes and run as a command. So the
//! line never holds a name, path, pattern or anything else from a server or a selection. Scripts
//! go to `sh` on standard input behind the fixed line `sh -s`, so only `sh` reads their quoting,
//! which [`shell_quote`] writes for it. The only other lines are commands written into Poros,
//! commands the user wrote for their own login shell, and, for servers that run a few commands
//! without any shell, words every shell reads as they are.
//!
//! Every exec request goes through this module.

use russh::client::Msg;
use russh::Channel;

use crate::error::AppResult;
pub use crate::rsync::shell_quote;
use crate::rsync::{CommandOutput, RemoteProcess};

/// What the login shell runs for every script: `sh`, reading its commands from standard input.
const SCRIPT_LINE: &str = "sh -s";
/// What a login script may print before `sh` starts.
const LOGIN_OUTPUT_LIMIT: usize = 64 * 1024;

/// The line of an exec request, which the login shell reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLine(String);

impl CommandLine {
    /// A command written into Poros itself, holding nothing it was given.
    pub fn fixed(line: &'static str) -> Self {
        Self(line.to_string())
    }

    /// A command the user wrote for their own login shell, exactly as they wrote it.
    pub fn written_by_user(line: &str) -> Self {
        Self(line.to_string())
    }

    /// `words`, written into Poros, followed by `arguments`, each in single quotes, for servers
    /// with no `sh` that run a few commands themselves, as SFTPGo runs `sha256sum`. `None` unless
    /// every argument is a [`portable`] word.
    pub fn portable(words: &[&'static str], arguments: &[&str]) -> Option<Self> {
        if !arguments.iter().all(|argument| portable(argument)) {
            return None;
        }
        let mut line = words.join(" ");
        for argument in arguments {
            line.push(' ');
            line.push_str(&shell_quote(argument));
        }
        Some(Self(line))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether every login shell reads `word` in single quotes as itself: printable ASCII without
/// the characters that sh, fish, csh, nushell, xonsh or PowerShell treat specially inside
/// single quotes, nor those that cmd.exe, which takes the quotes for part of the word, acts on.
pub fn portable(word: &str) -> bool {
    !word.is_empty()
        && word
            .bytes()
            .all(|byte| (b' '..=b'~').contains(&byte) && !b"'\"\\`$!%&|<>^;".contains(&byte))
}

/// What the commands of a script read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptInput {
    /// Nothing: their input is empty, and the channel's input is closed after the script.
    Empty,
    /// The channel, which stays open after the script, as it does for a command of its own.
    Channel,
}

/// What `sh` is sent for `script`. The script is one group, which `sh` reads whole before it
/// runs any of it, so no command in it can read the rest of the script as its input.
pub(crate) fn script_input(script: &str, input: ScriptInput) -> String {
    match input {
        ScriptInput::Empty => format!("{{\n{script}\n}} </dev/null\n"),
        ScriptInput::Channel => format!("{{\n{script}\n}}\n"),
    }
}

/// What `sh` is sent to start `command` with the channel as its input. `sh` reads the whole
/// group before it prints the marker, and reads nothing more after it, so what Poros writes once
/// the marker is back reaches the command alone. The group ends with `exit`, so even after the
/// command, `sh` never reads that as a script.
fn hand_over_input(command: &str, marker: &str) -> String {
    format!("{{ printf '%s\\n' {marker}; {command}\nexit; }}\n")
}

/// Starts `script` under `sh` on `channel`. The caller reads what it prints from the channel.
pub async fn start_script(
    channel: &Channel<Msg>,
    script: &str,
    input: ScriptInput,
) -> AppResult<()> {
    channel.exec(true, SCRIPT_LINE).await?;
    channel.data(script_input(script, input).as_bytes()).await?;
    if input == ScriptInput::Empty {
        let _ = channel.eof().await;
    }
    Ok(())
}

/// Starts `line` on `channel` as its own command, with the channel's input left open.
pub async fn start_line(channel: &Channel<Msg>, line: &CommandLine) -> AppResult<()> {
    channel.exec(true, line.as_str()).await?;
    Ok(())
}

/// Runs `script` under `sh` with no input and collects up to `limit` bytes of its output.
pub async fn run_script(
    channel: Channel<Msg>,
    script: &str,
    limit: usize,
) -> AppResult<CommandOutput> {
    let process = start_process(channel, SCRIPT_LINE).await?;
    Ok(process
        .collect(script_input(script, ScriptInput::Empty).as_bytes(), limit)
        .await)
}

/// Runs `line` with no input and collects up to `limit` bytes of its output.
pub async fn run_line(
    channel: Channel<Msg>,
    line: &CommandLine,
    limit: usize,
) -> AppResult<CommandOutput> {
    let process = start_process(channel, line.as_str()).await?;
    Ok(process.collect(&[], limit).await)
}

/// Starts `command`, a line of script, under `sh`, and gives it the channel: what the caller
/// writes to the process from then on is the command's input. Anything a login script printed
/// first is passed over.
pub async fn hand_over(channel: Channel<Msg>, command: &str) -> AppResult<RemoteProcess> {
    let marker = format!("poros-started-{}", uuid::Uuid::new_v4().simple());
    let mut process = start_process(channel, SCRIPT_LINE).await?;
    process.write_bytes(hand_over_input(command, &marker).as_bytes());
    process.flush().await?;
    process
        .skip_past(format!("{marker}\n").as_bytes(), LOGIN_OUTPUT_LIMIT)
        .await?;
    Ok(process)
}

/// `sh`, reading commands from the channel as the caller writes them.
pub async fn open_shell(channel: Channel<Msg>) -> AppResult<RemoteProcess> {
    start_process(channel, SCRIPT_LINE).await
}

async fn start_process(channel: Channel<Msg>, line: &str) -> AppResult<RemoteProcess> {
    channel.exec(true, line).await?;
    Ok(RemoteProcess::attach(channel))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Names someone else on the server can give files, each of which some login shell reads
    /// otherwise than `sh` when it is quoted for `sh`.
    pub(crate) const HOSTILE_NAMES: &[&str] = &[
        "it's",
        r"x\';touch poros-pwned;'",
        // Quoted for `sh`, fish runs the `touch` in this one.
        r"x\';touch poros-pwned;echo \'",
        r"back\\slash",
        r"trailing\",
        "$(touch poros-pwned)",
        "`touch poros-pwned`",
        "!touch",
        "two\nlines",
        "a; touch poros-pwned",
        "-rf",
        "--checkpoint-action=exec=sh x.sh",
        "\"quoted\"",
        "tab\there",
        "*",
        "{a,b}",
        "~root",
        "%PATH%&calc",
        "ends with newline\n",
    ];

    #[test]
    fn scripts_and_shells_are_asked_for_with_one_fixed_line() {
        assert_eq!(SCRIPT_LINE, "sh -s");
        for name in HOSTILE_NAMES {
            let script = format!("printf '%s\\0' {}", shell_quote(name));
            // The name only ever goes to standard input.
            assert!(script_input(&script, ScriptInput::Empty).contains(&shell_quote(name)));
            assert!(hand_over_input(&script, "marker").contains(&shell_quote(name)));
        }
    }

    #[test]
    fn only_portable_words_go_on_a_line() {
        for word in [
            "/srv/data/report 2024 (final).pdf",
            "a-b_c.d",
            "-rf",
            "~user",
            "x=1,y*",
        ] {
            assert!(portable(word), "{word:?}");
        }
        let refused = [
            "it's",
            r"x\';touch poros-pwned;'",
            r"back\\slash",
            r"trailing\",
            "$(touch poros-pwned)",
            "`touch poros-pwned`",
            "!touch",
            "two\nlines",
            "a; touch poros-pwned",
            "\"quoted\"",
            "tab\there",
            "%PATH%&calc",
            "a|b",
            "a<b>c",
            "a^b",
            "ends with newline\n",
            "",
            "caf\u{e9}",
            "\u{2019}quoted\u{2019}",
        ];
        for word in refused {
            assert!(!portable(word), "{word:?}");
            assert_eq!(CommandLine::portable(&["sha256sum", "--"], &[word]), None);
        }
        assert_eq!(
            CommandLine::portable(&["sha256sum", "--"], &["/srv/a b.iso"])
                .unwrap()
                .as_str(),
            "sha256sum -- '/srv/a b.iso'"
        );
    }

    /// The arguments `sh` gives a program in `script`, as it runs on a server: from standard
    /// input, the way [`start_script`] sends it.
    #[cfg(unix)]
    pub(crate) fn run_locally(script: &str, input: ScriptInput) -> Vec<u8> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut shell = Command::new("sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = shell.stdin.take().unwrap();
        stdin
            .write_all(script_input(script, input).as_bytes())
            .unwrap();
        drop(stdin);
        shell.wait_with_output().unwrap().stdout
    }

    /// The words a script handed to `sh` gave `printf`, one per NUL.
    #[cfg(unix)]
    pub(crate) fn printed_words(output: &[u8]) -> Vec<String> {
        let text = String::from_utf8(output.to_vec()).unwrap();
        let mut words: Vec<String> = text.split('\0').map(str::to_string).collect();
        assert_eq!(words.pop().as_deref(), Some(""), "{text:?}");
        words
    }

    #[cfg(unix)]
    #[test]
    fn names_reach_sh_unchanged() {
        let folder = tempfile::tempdir().unwrap();
        let quoted: Vec<String> = HOSTILE_NAMES.iter().map(|name| shell_quote(name)).collect();
        let script = format!(
            "cd {} || exit 1\nprintf '%s\\0' {}",
            shell_quote(&folder.path().to_string_lossy()),
            quoted.join(" ")
        );
        let words = printed_words(&run_locally(&script, ScriptInput::Empty));
        assert_eq!(words, HOSTILE_NAMES);
        assert!(!folder.path().join("poros-pwned").exists());
    }

    #[cfg(unix)]
    #[test]
    fn commands_in_a_script_never_read_the_script() {
        let output = run_locally("cat\necho after", ScriptInput::Empty);
        assert_eq!(String::from_utf8(output).unwrap(), "after\n");
    }

    /// `sh` passes the input that follows the marker to the command, and reads none of it
    /// itself, even when the command reads only part of it or none at all.
    #[cfg(unix)]
    #[test]
    fn a_handed_over_command_gets_the_rest_of_the_input() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::process::{Command, Stdio};

        let folder = tempfile::tempdir().unwrap();
        let names: Vec<String> = HOSTILE_NAMES.iter().map(|name| shell_quote(name)).collect();
        let printing = format!("printf '%s\\0' {} && head -c 7", names.join(" "));
        for (command, expected) in [
            (printing.as_str(), Some("payload")),
            ("true", None),
            ("poros-no-such-program", None),
        ] {
            let mut shell = Command::new("sh")
                .arg("-s")
                .current_dir(folder.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut stdin = shell.stdin.take().unwrap();
            stdin
                .write_all(hand_over_input(command, "poros-marker").as_bytes())
                .unwrap();
            stdin.flush().unwrap();
            let mut stdout = BufReader::new(shell.stdout.take().unwrap());
            let mut line = String::new();
            stdout.read_line(&mut line).unwrap();
            assert_eq!(line, "poros-marker\n");
            // Were `sh` to read this as a script, it would make the file.
            let _ = stdin.write_all(b"payload; touch poros-pwned\n");
            drop(stdin);
            let mut rest = Vec::new();
            stdout.read_to_end(&mut rest).unwrap();
            shell.wait().unwrap();
            match expected {
                Some(printed) => {
                    let mut words = printed_words(&rest[..rest.len() - printed.len()]);
                    assert_eq!(words.len(), HOSTILE_NAMES.len());
                    words.truncate(HOSTILE_NAMES.len());
                    assert_eq!(words, HOSTILE_NAMES);
                    assert!(rest.ends_with(printed.as_bytes()));
                }
                None => assert!(rest.is_empty(), "{rest:?}"),
            }
            assert!(!folder.path().join("poros-pwned").exists());
        }
    }

    /// Each file below `folder`, by its path from `root`, that holds any of `needles` once white
    /// space is taken out, as rustfmt may break a call anywhere.
    fn sources_holding(
        root: &std::path::Path,
        folder: &std::path::Path,
        needles: &[&'static str],
        found: &mut Vec<(String, &'static str)>,
    ) {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources_holding(root, &path, needles, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text: String = std::fs::read_to_string(&path)
                    .unwrap()
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect();
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                for needle in needles {
                    if text.contains(needle) {
                        found.push((relative.clone(), *needle));
                    }
                }
            }
        }
    }

    /// No code starts a command or a shell on a server except through this module, so none can
    /// put a name on the line a login shell reads: the only others are the user's own commands,
    /// which alone may send text as it was written, and the terminal, which asks for a shell.
    #[test]
    fn only_this_module_sends_exec_requests() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        sources_holding(
            &source,
            &source,
            &[".exec(", "::exec(", "written_by_user", "request_shell"],
            &mut found,
        );
        found.retain(|(path, _)| path != "ssh/exec.rs");
        found.sort();
        assert_eq!(
            found,
            [
                (
                    "automation/remote_command.rs".to_string(),
                    "written_by_user"
                ),
                ("terminal.rs".to_string(), "request_shell"),
            ]
        );
    }
}
