//! What a command hands back: a JSON value for `--json`, the text a person reads otherwise, and
//! how it went.

use std::io::{IsTerminal, Write};

use serde::Serialize;
use serde_json::{json, Value};

use super::exit::Exit;
use super::target::PASSPHRASE_VARIABLE;
use crate::error::{AppError, ErrorKind};

pub struct Reply {
    pub value: Value,
    /// Printed to standard output without `--json`; commands that stream their output, such as
    /// `cat`, leave it empty.
    pub text: String,
    pub exit: Exit,
}

impl Reply {
    pub fn new(value: impl Serialize, text: impl Into<String>) -> Self {
        Self {
            value: serde_json::to_value(value).unwrap_or(Value::Null),
            text: text.into(),
            exit: Exit::Success,
        }
    }

    pub fn with_exit(mut self, exit: Exit) -> Self {
        self.exit = exit;
        self
    }

    /// Partial when some of `total` items failed, the failure's own status when all did.
    pub fn with_failures(self, failed: usize, total: usize, all_failed: Exit) -> Self {
        match failed {
            0 => self,
            failed if failed == total && total > 0 => self.with_exit(all_failed),
            _ => self.with_exit(Exit::Partial),
        }
    }
}

/// What to change on the command line to get past an error, when the message alone, written
/// for the app, does not say.
pub fn hint(error: &AppError) -> Option<String> {
    match error.kind {
        ErrorKind::HostKeyUnknown | ErrorKind::HostKeyChanged => {
            let fingerprint = &error.host_key.as_ref()?.fingerprint;
            Some(match error.kind {
                ErrorKind::HostKeyUnknown => format!(
                    "To accept it, run again with --host-key {fingerprint}, or --trust-new-host; add --save-host-key to remember it"
                ),
                _ => format!(
                    "Only if the change is expected, run again with --host-key {fingerprint}"
                ),
            })
        }
        ErrorKind::PassphraseRequired => Some(format!(
            "Give the passphrase with {PASSPHRASE_VARIABLE}, --passphrase-env or --passphrase-file"
        )),
        _ => None,
    }
}

/// An error as `--json` prints it, with the exit status it leads to.
pub fn error_value(error: &AppError) -> Value {
    let mut value = serde_json::to_value(error).unwrap_or(Value::Null);
    if let Some(fields) = value.as_object_mut() {
        fields.insert("exitCode".into(), json!(Exit::for_error(error.kind).code()));
        if let Some(hint) = hint(error) {
            fields.insert("hint".into(), json!(hint));
        }
    }
    json!({ "error": value })
}

pub fn print_json(value: &Value) {
    let mut stdout = std::io::stdout().lock();
    let text = if stdout.is_terminal() {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .unwrap_or_default();
    let _ = writeln!(stdout, "{text}");
    let _ = stdout.flush();
}

pub fn print_text(text: &str) {
    if text.is_empty() {
        return;
    }
    let mut stdout = std::io::stdout().lock();
    let _ = if text.ends_with('\n') {
        write!(stdout, "{text}")
    } else {
        writeln!(stdout, "{text}")
    };
    let _ = stdout.flush();
}

pub fn print_error(error: &AppError) {
    let mut message = error.message.clone();
    if let Some(path) = &error.path {
        if !message.contains(path.as_str()) {
            message = format!("{path}: {message}");
        }
    }
    eprintln!("poros-cli: {message}");
    if let Some(hint) = hint(error) {
        eprintln!("poros-cli: {hint}");
    }
}

/// `rwxr-xr-x` with the type in front, as `ls -l` shows it.
pub fn mode_string(kind: char, permissions: Option<u32>) -> String {
    let Some(mode) = permissions else {
        return format!("{kind}?????????");
    };
    let mut shown = String::with_capacity(10);
    shown.push(kind);
    let specials = [(0o4000, 's', 'S'), (0o2000, 's', 'S'), (0o1000, 't', 'T')];
    for (group, (special, set, unset)) in specials.iter().enumerate() {
        let shift = 6 - group * 3;
        let bits = (mode >> shift) & 0o7;
        shown.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        shown.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        let execute = bits & 0o1 != 0;
        shown.push(match (mode & special != 0, execute) {
            (true, true) => *set,
            (true, false) => *unset,
            (false, true) => 'x',
            (false, false) => '-',
        });
    }
    shown
}

/// Seconds since the epoch as `2026-11-12 16:15`, in local time.
pub fn time_string(seconds: Option<i64>) -> String {
    use chrono::TimeZone;
    seconds
        .and_then(|seconds| chrono::Local.timestamp_opt(seconds, 0).single())
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".repeat(16))
}

/// Seconds since the epoch as RFC 3339 in UTC, for JSON.
pub fn time_iso(seconds: Option<i64>) -> Option<String> {
    use chrono::TimeZone;
    seconds
        .and_then(|seconds| chrono::Utc.timestamp_opt(seconds, 0).single())
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// `0644`, the way `chmod` takes permissions.
pub fn octal(permissions: Option<u32>) -> Option<String> {
    permissions.map(|mode| format!("{:04o}", mode & 0o7777))
}

pub fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_read_as_ls_shows_them() {
        assert_eq!(mode_string('d', Some(0o755)), "drwxr-xr-x");
        assert_eq!(mode_string('-', Some(0o644)), "-rw-r--r--");
        assert_eq!(mode_string('-', Some(0o4755)), "-rwsr-xr-x");
        assert_eq!(mode_string('d', Some(0o1777)), "drwxrwxrwt");
        assert_eq!(mode_string('-', Some(0o2644)), "-rw-r-Sr--");
        assert_eq!(mode_string('l', None), "l?????????");
        assert_eq!(octal(Some(0o100644)).as_deref(), Some("0644"));
    }

    #[test]
    fn errors_carry_their_exit_status() {
        let error = AppError::new(ErrorKind::NotFound, "missing").with_path("/srv/a");
        let value = error_value(&error);
        assert_eq!(value["error"]["kind"], "notFound");
        assert_eq!(value["error"]["path"], "/srv/a");
        assert_eq!(value["error"]["exitCode"], 7);
    }

    #[test]
    fn some_failures_are_partial_and_all_take_the_failure_status() {
        let reply = || Reply::new(json!({}), "");
        assert_eq!(
            reply().with_failures(0, 3, Exit::NotFound).exit,
            Exit::Success
        );
        assert_eq!(
            reply().with_failures(1, 3, Exit::NotFound).exit,
            Exit::Partial
        );
        assert_eq!(
            reply().with_failures(3, 3, Exit::NotFound).exit,
            Exit::NotFound
        );
    }

    #[test]
    fn times_are_utc_in_json() {
        assert_eq!(
            time_iso(Some(1_700_000_000)).as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
        assert_eq!(time_iso(None), None);
    }
}
