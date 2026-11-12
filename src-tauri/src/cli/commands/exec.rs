//! `exec`: a command through the server's shell, its output passed along as it arrives and its
//! status as the exit status, the way `ssh host command` behaves.

use std::io::Write;

use serde_json::json;

use crate::automation::remote_command::{self, OutputStream};
use crate::cli::args::ExecArgs;
use crate::cli::context::Context;
use crate::cli::exit::Exit;
use crate::cli::output::Reply;
use crate::error::{AppError, AppResult, ErrorKind};

/// What a shell reports for a command ended by a signal, 128 and the signal's number.
fn signal_status(signal: &str) -> u8 {
    let number = match signal
        .trim_start_matches("SIG")
        .to_ascii_uppercase()
        .as_str()
    {
        "HUP" => 1,
        "INT" => 2,
        "QUIT" => 3,
        "ILL" => 4,
        "ABRT" => 6,
        "FPE" => 8,
        "KILL" => 9,
        "USR1" => 10,
        "SEGV" => 11,
        "USR2" => 12,
        "PIPE" => 13,
        "ALRM" => 14,
        "TERM" => 15,
        _ => return 255,
    };
    128 + number
}

pub async fn execute(context: &Context, arguments: &ExecArgs, json: bool) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let command = arguments.command.join(" ");
    let (mut stdout, mut stderr) = (String::new(), String::new());
    let result = remote_command::run_on(
        &connected.session,
        &command,
        arguments.cwd.as_deref(),
        &context.interrupt,
        |stream, text| match (stream, json) {
            (OutputStream::Stdout, true) => stdout.push_str(text),
            (OutputStream::Stderr, true) => stderr.push_str(text),
            (OutputStream::Stdout, false) => {
                let mut out = std::io::stdout();
                let _ = out.write_all(text.as_bytes());
                let _ = out.flush();
            }
            (OutputStream::Stderr, false) => {
                let mut out = std::io::stderr();
                let _ = out.write_all(text.as_bytes());
                let _ = out.flush();
            }
        },
    )
    .await?;
    let exit = if result.stopped {
        Exit::Interrupted
    } else if let Some(signal) = &result.signal {
        Exit::Remote(signal_status(signal))
    } else {
        match result.exit_status {
            Some(0) => Exit::Success,
            Some(status) => Exit::Remote(status.min(255) as u8),
            None => {
                return Err(AppError::new(
                    ErrorKind::Ssh,
                    "The server closed the command without saying how it ended",
                ))
            }
        }
    };
    let value = if json {
        json!({
            "command": command,
            "exitStatus": result.exit_status,
            "signal": result.signal,
            "stdout": stdout,
            "stderr": stderr,
            "elapsedMillis": result.elapsed_millis,
        })
    } else {
        serde_json::Value::Null
    };
    Ok(Reply::new(value, "").with_exit(exit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals_end_with_the_status_a_shell_gives() {
        assert_eq!(signal_status("TERM"), 143);
        assert_eq!(signal_status("SIGKILL"), 137);
        assert_eq!(signal_status("Custom(\"XCPU\")"), 255);
    }
}
