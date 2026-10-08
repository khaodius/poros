//! What the computer does when the transfer queue finishes: lock, sleep, shut down, or run
//! the user's own command.

use std::collections::HashMap;
use std::process::Stdio;

use serde::Deserialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};

/// Output lines of a local command copied to the log.
const MAX_LOGGED_LINES: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PowerAction {
    Lock,
    Sleep,
    Hibernate,
    LogOff,
    ShutDown,
}

impl PowerAction {
    fn description(self) -> &'static str {
        match self {
            Self::Lock => "lock the computer",
            Self::Sleep => "put the computer to sleep",
            Self::Hibernate => "hibernate the computer",
            Self::LogOff => "log off",
            Self::ShutDown => "shut down the computer",
        }
    }
}

/// Programs that carry out an action on this system, tried in order until one succeeds.
fn candidates(action: PowerAction) -> Vec<Vec<String>> {
    let command = |parts: &[&str]| parts.iter().map(|part| part.to_string()).collect();
    if cfg!(windows) {
        match action {
            PowerAction::Lock => vec![command(&["rundll32.exe", "user32.dll,LockWorkStation"])],
            // SetSuspendState through rundll32 hibernates when hibernation is on; this asks
            // for sleep.
            PowerAction::Sleep => vec![command(&[
                "powershell.exe",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; [void][System.Windows.Forms.Application]::SetSuspendState('Suspend', $false, $false)",
            ])],
            PowerAction::Hibernate => vec![command(&["shutdown.exe", "/h"])],
            PowerAction::LogOff => vec![command(&["shutdown.exe", "/l"])],
            PowerAction::ShutDown => vec![command(&["shutdown.exe", "/s", "/t", "0"])],
        }
    } else if cfg!(target_os = "macos") {
        match action {
            PowerAction::Lock => vec![command(&["pmset", "displaysleepnow"])],
            PowerAction::Sleep | PowerAction::Hibernate => vec![command(&["pmset", "sleepnow"])],
            PowerAction::LogOff => vec![command(&[
                "osascript",
                "-e",
                "tell application \"System Events\" to log out",
            ])],
            PowerAction::ShutDown => vec![command(&[
                "osascript",
                "-e",
                "tell application \"System Events\" to shut down",
            ])],
        }
    } else {
        match action {
            PowerAction::Lock => vec![
                command(&["loginctl", "lock-session"]),
                command(&["xdg-screensaver", "lock"]),
            ],
            PowerAction::Sleep => vec![command(&["systemctl", "suspend"])],
            PowerAction::Hibernate => vec![command(&["systemctl", "hibernate"])],
            PowerAction::LogOff => {
                let mut commands = Vec::new();
                if let Some(session) = std::env::var("XDG_SESSION_ID")
                    .ok()
                    .filter(|session| !session.is_empty())
                {
                    commands.push(command(&["loginctl", "terminate-session", &session]));
                }
                commands.push(command(&["gnome-session-quit", "--logout", "--no-prompt"]));
                commands.push(command(&[
                    "qdbus",
                    "org.kde.ksmserver",
                    "/KSMServer",
                    "logout",
                    "0",
                    "0",
                    "0",
                ]));
                commands
            }
            PowerAction::ShutDown => vec![
                command(&["systemctl", "poweroff"]),
                command(&["shutdown", "-h", "now"]),
            ],
        }
    }
}

/// Blocks until the system has accepted the request.
pub fn perform(action: PowerAction) -> AppResult<()> {
    let mut failures = Vec::new();
    for command in candidates(action) {
        let mut process = std::process::Command::new(&command[0]);
        process
            .args(&command[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        hide_console(&mut process);
        match process.output() {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                let reason = String::from_utf8_lossy(&output.stderr).trim().to_string();
                failures.push(if reason.is_empty() {
                    format!("{} ({})", command[0], output.status)
                } else {
                    format!("{}: {reason}", command[0])
                });
            }
            Err(error) => failures.push(format!("{}: {error}", command[0])),
        }
    }
    Err(AppError::new(
        ErrorKind::Io,
        format!(
            "Could not {}. {}",
            action.description(),
            failures.join("; ")
        ),
    ))
}

#[cfg(windows)]
fn hide_console(process: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    process.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_process: &mut std::process::Command) {}

/// Runs the user's command through the system shell and logs what it printed.
pub async fn run_local_command(
    command: &str,
    environment: &HashMap<String, String>,
    events: &Events,
) -> AppResult<()> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(());
    }
    let mut process = if cfg!(windows) {
        let mut process = tokio::process::Command::new("cmd.exe");
        process.arg("/C").arg(command);
        process
    } else {
        let mut process = tokio::process::Command::new("sh");
        process.arg("-c").arg(command);
        process
    };
    process
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(windows)]
    process.creation_flags(0x0800_0000);

    events.log(LogLevel::Info, None, format!("Running: {command}"));
    let output = process.output().await.map_err(|error| {
        AppError::new(
            ErrorKind::Io,
            format!("Could not run \"{command}\": {error}"),
        )
    })?;
    let printed = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    for line in printed.lines().take(MAX_LOGGED_LINES) {
        events.log(LogLevel::Server, None, line);
    }
    if output.status.success() {
        events.log(LogLevel::Info, None, "The command finished");
        Ok(())
    } else {
        Err(AppError::new(
            ErrorKind::Io,
            format!("\"{command}\" ended with {}", output.status),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_program_to_try() {
        for action in [
            PowerAction::Lock,
            PowerAction::Sleep,
            PowerAction::Hibernate,
            PowerAction::LogOff,
            PowerAction::ShutDown,
        ] {
            let commands = candidates(action);
            assert!(!commands.is_empty());
            assert!(commands
                .iter()
                .all(|command| command.iter().all(|part| !part.is_empty())));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_commands_get_the_environment_and_report_failure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("out.txt");
        let environment = HashMap::from([("POROS_FILES_DONE".to_string(), "3".to_string())]);
        run_local_command(
            &format!("echo \"$POROS_FILES_DONE\" > '{}'", file.display()),
            &environment,
            &Events::default(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), "3");

        let error = run_local_command("exit 4", &environment, &Events::default())
            .await
            .unwrap_err();
        assert!(error.message.contains("exit 4"));
    }
}
