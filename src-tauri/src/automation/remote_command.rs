//! Commands the user runs on a server over SSH. Output streams to the window that asked for
//! it and to the log.
//!
//! The command is the user's own text, for their own login shell. The names of selected items
//! never go into that text: the command refers to them as positional parameters (`"${1}"`), and
//! `sh` hands them over as arguments, so no login shell ever parses a name. A command that refers
//! to none runs in the login shell, whatever it is; one that does runs in the login shell when it
//! is a POSIX shell, and otherwise in `sh`, since fish, csh and the like take arguments otherwise.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::session::{Session, SessionManager};
use crate::ssh::exec::{self, shell_quote, CommandLine, ScriptInput};

/// Output lines copied to the log per run; the output window has the rest.
const MAX_LOGGED_LINES: usize = 200;
/// Login shells that take a command and its positional parameters the way `sh` does.
const POSIX_SHELLS: &str = "sh|bash|dash|ash|ksh|ksh93|mksh|lksh|pdksh|oksh|zsh|yash|posh";
/// More than this in the arguments together could pass what the server takes for one command.
const MAX_ARGUMENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    /// Names this run for its output events and for stopping it.
    pub run_id: String,
    pub session_id: String,
    pub command: String,
    /// The folder to run the command in; the login folder when absent.
    #[serde(default)]
    pub directory: Option<String>,
    /// What the command's `"${1}"`, `"${2}"` and so on stand for: the selected items.
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunk {
    pub run_id: String,
    pub stream: OutputStream,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    /// Absent when the server did not say, for example after a stop.
    pub exit_status: Option<u32>,
    /// The signal that ended the command, if one did.
    pub signal: Option<String>,
    pub stopped: bool,
    pub elapsed_millis: u64,
}

impl CommandResult {
    /// Only a command the server reported as exiting with status 0 succeeded: a channel that
    /// closes with no status, as when the connection drops, says nothing about the command.
    pub fn succeeded(&self) -> bool {
        !self.stopped && self.signal.is_none() && self.exit_status == Some(0)
    }

    pub fn describe(&self) -> String {
        if self.stopped {
            "stopped".into()
        } else if let Some(signal) = &self.signal {
            format!("ended by signal {signal}")
        } else {
            match self.exit_status {
                Some(0) => "finished".into(),
                Some(status) => format!("exited with status {status}"),
                None => "ended without an exit status".into(),
            }
        }
    }
}

pub struct CommandRunner {
    sessions: Arc<SessionManager>,
    events: Events,
    running: Mutex<HashMap<String, CancellationToken>>,
}

impl CommandRunner {
    pub fn new(sessions: Arc<SessionManager>, events: Events) -> Self {
        Self {
            sessions,
            events,
            running: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(&self, request: CommandRequest) -> AppResult<CommandResult> {
        let command = request.command.trim();
        if command.is_empty() {
            return Err(AppError::invalid("Enter a command to run"));
        }
        let session = self.sessions.get(&request.session_id).await?;
        let cancel = CancellationToken::new();
        self.running
            .lock()
            .unwrap()
            .insert(request.run_id.clone(), cancel.clone());

        let session_id = Some(request.session_id.as_str());
        let mut described = format!("Running on the server: {command}");
        if !request.arguments.is_empty() {
            let arguments: Vec<String> = request
                .arguments
                .iter()
                .map(|argument| shell_quote(argument))
                .collect();
            described.push_str(&format!(" with {}", arguments.join(" ")));
        }
        self.events.log(LogLevel::Info, session_id, described);
        let mut logged_lines = 0;
        let mut lines = [String::new(), String::new()];
        let mut log_line = |line: &str| {
            if logged_lines < MAX_LOGGED_LINES {
                self.events.log(LogLevel::Server, session_id, line);
            } else if logged_lines == MAX_LOGGED_LINES {
                self.events.log(
                    LogLevel::Info,
                    session_id,
                    "The rest of the output is in the output window",
                );
            }
            logged_lines += 1;
        };
        let result = run_on(
            &session,
            command,
            request.directory.as_deref(),
            &request.arguments,
            &cancel,
            |stream, text| {
                self.events.command_output(&OutputChunk {
                    run_id: request.run_id.clone(),
                    stream,
                    text: text.to_string(),
                });
                let pending = &mut lines[stream as usize];
                pending.push_str(text);
                while let Some(end) = pending.find('\n') {
                    log_line(pending[..end].trim_end_matches('\r'));
                    pending.drain(..=end);
                }
            },
        )
        .await;
        for rest in &lines {
            if !rest.is_empty() {
                log_line(rest);
            }
        }
        self.running.lock().unwrap().remove(&request.run_id);

        match &result {
            Ok(outcome) => self.events.log(
                if outcome.succeeded() {
                    LogLevel::Info
                } else {
                    LogLevel::Warn
                },
                session_id,
                format!("Command {}", outcome.describe()),
            ),
            Err(error) => self.events.log(
                LogLevel::Error,
                session_id,
                format!("Command failed: {}", error.message),
            ),
        }
        result
    }

    pub fn stop(&self, run_id: &str) {
        if let Some(cancel) = self.running.lock().unwrap().remove(run_id) {
            cancel.cancel();
        }
    }
}

/// Runs `command` through the server's shell on a channel of the session's connection, in
/// `directory`, with `arguments` as its positional parameters, handing output to `on_output` as
/// it arrives.
pub async fn run_on(
    session: &Session,
    command: &str,
    directory: Option<&str>,
    arguments: &[String],
    cancel: &CancellationToken,
    mut on_output: impl FnMut(OutputStream, &str),
) -> AppResult<CommandResult> {
    let started = Instant::now();
    let directory = directory.map(str::trim).filter(|folder| !folder.is_empty());
    check_command(command, directory, arguments)?;
    let mut channel = session.open_command_channel().await?;
    match (directory, arguments.is_empty()) {
        // Nothing was added to what the user wrote, so it goes to the login shell as it is.
        (None, true) => {
            exec::start_line(&channel, &CommandLine::written_by_user(command)).await?;
        }
        _ => {
            let script = command_script(command, directory, arguments);
            exec::start_script(&channel, &script, ScriptInput::Channel).await?;
        }
    }

    let mut decoders = [Utf8Decoder::default(), Utf8Decoder::default()];
    let mut result = CommandResult::default();
    loop {
        let message = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = channel.close().await;
                result.stopped = true;
                break;
            }
            message = channel.wait() => message,
        };
        let (stream, data) = match message {
            Some(ChannelMsg::Data { data }) => (OutputStream::Stdout, data),
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => (OutputStream::Stderr, data),
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                result.exit_status = Some(exit_status);
                continue;
            }
            Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                result.signal = Some(format!("{signal_name:?}"));
                continue;
            }
            Some(ChannelMsg::Failure) => {
                let _ = channel.close().await;
                return Err(AppError::new(
                    ErrorKind::Ssh,
                    "The server does not allow running commands",
                ));
            }
            Some(ChannelMsg::Close) | None => break,
            Some(_) => continue,
        };
        let text = decoders[stream as usize].decode(&data);
        if !text.is_empty() {
            on_output(stream, &text);
        }
    }
    for (stream, decoder) in [OutputStream::Stdout, OutputStream::Stderr]
        .into_iter()
        .zip(&mut decoders)
    {
        let rest = decoder.finish();
        if !rest.is_empty() {
            on_output(stream, &rest);
        }
    }
    result.elapsed_millis = started.elapsed().as_millis() as u64;
    Ok(result)
}

fn check_command(command: &str, directory: Option<&str>, arguments: &[String]) -> AppResult<()> {
    let parts = std::iter::once(command)
        .chain(directory)
        .chain(arguments.iter().map(String::as_str));
    if parts.clone().any(|part| part.contains('\0')) {
        return Err(AppError::invalid("A command cannot hold a NUL character"));
    }
    if arguments.iter().map(String::len).sum::<usize>() > MAX_ARGUMENT_BYTES {
        return Err(AppError::invalid(
            "The selection is too long for one command; select fewer items",
        ));
    }
    Ok(())
}

/// The script `sh` runs for `command`: it enters `directory`, then runs the command in the login
/// shell with `arguments` as its positional parameters. The login shell gets the command as the
/// user wrote it, and each argument as an argument, never as text to read. A login shell that
/// does not take positional parameters as `sh` does gets no command with arguments; `sh` runs
/// that itself.
///
/// The script is also Bourne shell, for servers whose `sh` is one (Solaris 10): no `$(...)`, no
/// `${name##pattern}`, and no `--` for `cd`.
fn command_script(command: &str, directory: Option<&str>, arguments: &[String]) -> String {
    let mut script = String::new();
    if let Some(folder) = directory {
        let entered = if folder.starts_with('-') {
            format!("./{folder}")
        } else {
            folder.to_string()
        };
        script.push_str(&format!("cd {} || exit 1\n", shell_quote(&entered)));
        // The folder as `"$PWD"`, exactly, for every login shell, also those that do not keep
        // `PWD` themselves.
        if folder.starts_with('/') {
            script.push_str(&format!("PWD={}; export PWD\n", shell_quote(folder)));
        }
    }
    let command = shell_quote(command);
    if arguments.is_empty() {
        script.push_str(&format!("exec \"${{SHELL:-sh}}\" -c {command}"));
        return script;
    }
    let arguments: String = arguments
        .iter()
        .map(|argument| format!(" {}", shell_quote(argument)))
        .collect();
    script.push_str(&format!(
        "login_shell=`expr \"//$SHELL\" : '.*/\\(.*\\)'`\n\
         case $login_shell in\n\
         {POSIX_SHELLS}) exec \"$SHELL\" -c {command} \"$login_shell\"{arguments} ;;\n\
         esac\n\
         exec sh -c {command} sh{arguments}"
    ));
    script
}

/// Turns output into text without splitting a character that spans two packets.
#[derive(Default)]
struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let complete = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            // An incomplete character at the end waits for the next packet.
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => self.pending.len(),
        };
        let text = String::from_utf8_lossy(&self.pending[..complete]).into_owned();
        self.pending.drain(..complete);
        text
    }

    fn finish(&mut self) -> String {
        let text = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `command_script` prints when `sh` runs it as a server does, for an account whose
    /// login shell is `login_shell`, and its exit status.
    #[cfg(unix)]
    fn run_as_server(
        command: &str,
        directory: Option<&str>,
        arguments: &[String],
        login_shell: Option<&std::path::Path>,
    ) -> (Vec<u8>, Option<i32>) {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut shell = Command::new("sh");
        shell
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        match login_shell {
            Some(path) => shell.env("SHELL", path),
            None => shell.env_remove("SHELL"),
        };
        let mut shell = shell.spawn().unwrap();
        let script = command_script(command, directory, arguments);
        let mut input = shell.stdin.take().unwrap();
        input
            .write_all(exec::script_input(&script, ScriptInput::Channel).as_bytes())
            .unwrap();
        drop(input);
        let output = shell.wait_with_output().unwrap();
        (output.stdout, output.status.code())
    }

    /// A stand-in for a login shell such as fish: it says it ran and with what, and runs nothing.
    #[cfg(unix)]
    fn fake_shell(folder: &std::path::Path, name: &str) -> std::path::PathBuf {
        fake_program(folder, name, &format!("printf '%s\\0' {name} \"$@\""))
    }

    #[cfg(unix)]
    fn fake_program(folder: &std::path::Path, name: &str, script: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = folder.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn selected_names_reach_the_command_as_arguments() {
        use crate::ssh::exec::tests::{printed_words, HOSTILE_NAMES};

        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("it's a $(folder)\n");
        std::fs::create_dir(&folder).unwrap();
        let folder_text = folder.to_string_lossy().into_owned();
        let arguments: Vec<String> = HOSTILE_NAMES.iter().map(|name| name.to_string()).collect();
        let command = r#"printf '%s\0' "$0" "$PWD" "$@""#;
        let expected = |shell: &str| {
            let mut words = vec![shell.to_string(), folder_text.clone()];
            words.extend(arguments.iter().cloned());
            words
        };

        // A POSIX login shell runs the command itself.
        let bash = std::path::Path::new("/bin/bash");
        if bash.exists() {
            let (output, status) =
                run_as_server(command, Some(&folder_text), &arguments, Some(bash));
            assert_eq!(status, Some(0));
            assert_eq!(printed_words(&output), expected("bash"));
        }
        // Any other login shell, or none known, has `sh` run it. busybox is no shell by that
        // name, only as `sh` or `ash`.
        let fish = fake_shell(root.path(), "fish");
        let busybox = fake_shell(root.path(), "busybox");
        for login_shell in [Some(fish.as_path()), Some(busybox.as_path()), None] {
            let (output, status) =
                run_as_server(command, Some(&folder_text), &arguments, login_shell);
            assert_eq!(status, Some(0));
            assert_eq!(printed_words(&output), expected("sh"));
        }
        assert!(!folder.join("poros-pwned").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_command_without_names_runs_in_the_login_shell_as_written() {
        use crate::ssh::exec::tests::printed_words;

        let root = tempfile::tempdir().unwrap();
        let fish = fake_shell(root.path(), "fish");
        let command = r"echo (pwd); set x 1; and echo \' $x";
        let folder = root.path().to_string_lossy().into_owned();
        let (output, status) = run_as_server(command, Some(&folder), &[], Some(&fish));
        assert_eq!(status, Some(0));
        assert_eq!(printed_words(&output), ["fish", "-c", command]);
        // A folder that cannot be entered fails as `cd` in the login shell did, and runs nothing.
        let missing = format!("{folder}/missing");
        let (output, status) = run_as_server(command, Some(&missing), &[], Some(&fish));
        assert_eq!(status, Some(1));
        assert!(output.is_empty());
    }

    /// A command in a folder, such as one whose only placeholder is the folder, finds it as
    /// `"$PWD"`, exactly, in whatever login shell runs it.
    #[cfg(unix)]
    #[test]
    fn the_folder_is_pwd_in_the_login_shell() {
        use crate::ssh::exec::tests::printed_words;

        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("it's a $(folder)\n");
        std::fs::create_dir(&folder).unwrap();
        let folder_text = folder.to_string_lossy().into_owned();
        let bash = std::path::Path::new("/bin/bash");
        if bash.exists() {
            let command = r#"printf '%s\0' "$PWD""#;
            let (output, status) = run_as_server(command, Some(&folder_text), &[], Some(bash));
            assert_eq!(status, Some(0));
            assert_eq!(printed_words(&output), std::slice::from_ref(&folder_text));
        }
        // Exported, for login shells that do not set it themselves.
        let environment = fake_program(root.path(), "csh", "exec env");
        let (output, status) = run_as_server("pwd", Some(&folder_text), &[], Some(&environment));
        assert_eq!(status, Some(0));
        let printed = String::from_utf8(output).unwrap();
        assert!(
            printed.contains(&format!("\nPWD={folder_text}\n")),
            "{printed}"
        );
    }

    /// Where `sh` is the Bourne shell (Solaris 10), the scripts still run.
    #[test]
    fn scripts_keep_to_the_bourne_shell() {
        let names = ["a".to_string()];
        for script in [
            command_script("ls", Some("/srv"), &names),
            command_script("ls", Some("/srv"), &[]),
            command_script("ls", None, &names),
        ] {
            for modern in ["$(", "##", "cd --"] {
                assert!(!script.contains(modern), "{modern} in {script}");
            }
        }
        assert!(command_script("ls", Some("-x"), &[]).starts_with("cd './-x' || exit 1\nexec "));
    }

    #[test]
    fn commands_refuse_what_cannot_reach_a_server() {
        assert!(check_command("ls", Some("/srv"), &["a".into()]).is_ok());
        assert!(check_command("ls\0", None, &[]).is_err());
        assert!(check_command("ls", Some("/srv\0"), &[]).is_err());
        assert!(check_command("ls", None, &["a\0b".into()]).is_err());
        let huge = vec!["x".repeat(1024); 1025];
        assert!(check_command("ls", None, &huge).is_err());
    }

    #[test]
    fn characters_split_across_packets_stay_whole() {
        let mut decoder = Utf8Decoder::default();
        let text = "größe ✓";
        let bytes = text.as_bytes();
        let split = text.find('ö').unwrap() + 1;
        let mut decoded = decoder.decode(&bytes[..split]);
        assert_eq!(decoded, "gr");
        decoded.push_str(&decoder.decode(&bytes[split..bytes.len() - 1]));
        decoded.push_str(&decoder.decode(&bytes[bytes.len() - 1..]));
        assert_eq!(decoded, text);
        assert_eq!(decoder.finish(), "");

        assert_eq!(decoder.decode(b"bad \xff byte"), "bad \u{fffd} byte");
        assert_eq!(decoder.decode(b"cut \xe2\x9c"), "cut ");
        assert_eq!(decoder.finish(), "\u{fffd}");
    }

    #[test]
    fn results_describe_how_the_command_ended() {
        let finished = CommandResult {
            exit_status: Some(0),
            ..Default::default()
        };
        assert!(finished.succeeded());
        assert_eq!(finished.describe(), "finished");
        let failed = CommandResult {
            exit_status: Some(2),
            ..Default::default()
        };
        assert!(!failed.succeeded());
        assert_eq!(failed.describe(), "exited with status 2");
        let stopped = CommandResult {
            stopped: true,
            ..Default::default()
        };
        assert_eq!(stopped.describe(), "stopped");
        // The connection went away before the server said how the command ended.
        let cut_off = CommandResult::default();
        assert!(!cut_off.succeeded());
        assert_eq!(cut_off.describe(), "ended without an exit status");
    }
}
