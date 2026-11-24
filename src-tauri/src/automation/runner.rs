//! Carries out scheduled tasks, for the app and for the command line alike: signs in to a
//! task's saved connections with what is stored for them, does what the task says, and writes
//! down how it went.

use std::sync::Arc;

use chrono::Local;
use tokio_util::sync::CancellationToken;

use super::board::{Claim, TaskBoard};
use super::remote_command;
use super::task::{LastRun, Outcome, ScheduledTask, TaskAction, CUT_SHORT};
use super::transfer_task;
use crate::connections::ConnectionStore;
use crate::error::{AppError, AppResult, ErrorKind, HostKeyInfo};
use crate::events::{Events, LogLevel};
use crate::format::format_size;
use crate::route::{self, Unattended, MAX_JUMP_HOSTS};
use crate::session::{SessionInfo, SessionManager};
use crate::settings::{CloudSettings, ConnectionSettings};
use crate::ssh::{ConnectProfile, HostKeyApproval};
use crate::sync::{SyncAction, SyncChoice, SyncManager, SyncRequest, SyncRunRequest};
use crate::transfer::{JobBatch, TransferManager};

/// Sessions opened for tasks belong to no window, so closing one never disconnects them.
const TASK_OWNER: &str = "scheduler";
/// The last output line of a command task is kept as its result, cut to this length.
const MAX_RESULT_LINE_CHARS: usize = 200;
const QUEUE_PAUSED: &str = "The transfer queue is paused, so this run was skipped";

/// A run that would copy files while the user has paused the queue ends at once, before it
/// changes anything: waiting would keep it, and every run after it, going for as long as the
/// queue stays paused.
pub(super) fn skipped_while_paused() -> Outcome {
    Outcome {
        succeeded: false,
        message: QUEUE_PAUSED.into(),
    }
}

/// What the command line adds to signing in. The app adds nothing.
#[derive(Clone, Default)]
pub struct SignIn {
    /// The saved connection the password and passphrase below were given for. They are sent
    /// to no other server.
    pub given_for: Option<String>,
    pub password: Option<String>,
    pub passphrase: Option<String>,
    /// The fingerprint of the only host key the server of `given_for` is accepted with. Other
    /// servers are not held to it.
    pub host_key: Option<String>,
    /// Accepts the key of a server seen for the first time, other than the one pinned above.
    pub trust_new_host: bool,
    pub save_host_key: bool,
    /// Messages name command line options rather than the app's windows.
    pub command_line: bool,
    /// Seconds to wait for a server, in place of the connection settings'.
    pub timeout_secs: Option<u64>,
}

/// What a task needs to run.
pub struct TaskContext<'a> {
    pub sessions: &'a SessionManager,
    pub connections: Arc<ConnectionStore>,
    pub connection_settings: ConnectionSettings,
    pub cloud_settings: CloudSettings,
    pub sync: &'a SyncManager,
    pub transfers: &'a TransferManager,
    pub events: &'a Events,
    pub sign_in: SignIn,
    /// A run's files stay in the transfer list once the run has counted them, for the user to
    /// see. False for a queue no one sees, as the daemon's, which would otherwise keep every
    /// failure for as long as it lives.
    pub keep_ended_jobs: bool,
}

impl TaskContext<'_> {
    /// Takes a run's files off the queue once it has counted them, when no one will look.
    pub(super) fn done_with(&self, batch: &Arc<JobBatch>) {
        if !self.keep_ended_jobs {
            self.transfers.clear_batch(batch);
        }
    }
}

/// A run under way. Dropped before it ends, as when its process is interrupted, it is written
/// down as cut short. A process that dies without dropping it leaves the same result, written
/// when the task was claimed.
struct Run<'a> {
    board: &'a TaskBoard,
    claim: Option<Claim>,
    started: i64,
}

impl Run<'_> {
    fn end(&mut self, succeeded: bool, message: String) {
        if let Some(claim) = self.claim.take() {
            self.board.finish(
                claim,
                LastRun {
                    started: self.started,
                    finished: Local::now().timestamp(),
                    succeeded,
                    message,
                    under_way: false,
                },
            );
        }
    }
}

impl Drop for Run<'_> {
    fn drop(&mut self) {
        self.end(false, CUT_SHORT.into());
    }
}

/// Runs a task taken from the board, logs how it went and writes that to the task.
pub async fn run_claimed(
    board: &TaskBoard,
    claim: Claim,
    context: &TaskContext<'_>,
) -> AppResult<Outcome> {
    let task = claim.task.clone();
    context.events.log(
        LogLevel::Info,
        None,
        format!("Scheduled task \"{}\" started", task.name),
    );
    let mut run = Run {
        board,
        claim: Some(claim),
        started: Local::now().timestamp(),
    };
    let result = perform(&task, context).await;
    let (succeeded, message) = match &result {
        Ok(outcome) => (outcome.succeeded, outcome.message.clone()),
        Err(error) => (false, error.message.clone()),
    };
    context.events.log(
        if succeeded {
            LogLevel::Info
        } else {
            LogLevel::Error
        },
        None,
        format!("Scheduled task \"{}\": {message}", task.name),
    );
    run.end(succeeded, message);
    result
}

/// Connects with the task's saved connections, carries out its action and disconnects.
pub async fn perform(task: &ScheduledTask, context: &TaskContext<'_>) -> AppResult<Outcome> {
    let action = &task.action;
    if let TaskAction::Transfer { .. } = action {
        return transfer_task::run(task, context).await;
    }
    if let TaskAction::Sync { .. } = action {
        if context.transfers.is_paused() {
            return Ok(skipped_while_paused());
        }
    }
    let session = connect(context, action.connection_id(), Some("run scheduled tasks")).await?;
    let result = match action {
        TaskAction::Command {
            command, directory, ..
        } => run_command(command, directory.as_deref(), &session.id, context).await,
        _ => synchronize(task, &session.id, context).await,
    };
    context.sessions.disconnect(&session.id).await?;
    result
}

/// Opens a session with a saved connection, with no one there to answer. `needs_sftp_to` says
/// what only an SFTP connection can do, when the task needs one.
pub(super) async fn connect(
    context: &TaskContext<'_>,
    connection_id: &str,
    needs_sftp_to: Option<&'static str>,
) -> AppResult<SessionInfo> {
    let sign_in = &context.sign_in;
    let profile = {
        let store = context.connections.clone();
        let settings = context.connection_settings.clone();
        let cloud = context.cloud_settings.clone();
        let sign_in = sign_in.clone();
        let id = connection_id.to_string();
        tokio::task::spawn_blocking(move || {
            let given = sign_in.given_for.as_deref() == Some(id.as_str());
            route::saved_profile_with(
                &store,
                &settings,
                &id,
                &Unattended {
                    needs_sftp_to,
                    password: sign_in.password.filter(|_| given),
                    passphrase: sign_in.passphrase.filter(|_| given),
                    cloud: Some(&cloud),
                    command_line: sign_in.command_line,
                    timeout_secs: sign_in.timeout_secs,
                },
            )
        })
        .await??
    };
    // `--host-key` names the only key the server of the connection given with --connection is
    // accepted with. Jump hosts on the way and the run's other connections are not covered by
    // it: `--trust-new-host` is there for those.
    let pin = sign_in
        .host_key
        .as_deref()
        .map(str::trim)
        .filter(|pin| !pin.is_empty() && sign_in.given_for.as_deref() == Some(connection_id));
    let pinned = pin.map(|fingerprint| {
        let (host, port) = profile.server_address();
        HostKeyApproval {
            host,
            port,
            fingerprint: fingerprint.to_string(),
            remember: sign_in.save_host_key,
        }
    });
    let mut approval = pinned.clone();
    // Each jump host can stop the attempt once to have its key trusted.
    let mut tries = MAX_JUMP_HOSTS + 2;
    loop {
        tries -= 1;
        let error = match context
            .sessions
            .connect(profile.clone(), approval.clone(), TASK_OWNER)
            .await
        {
            Ok(session) => return held_to_pin(context, session, pin, &profile).await,
            Err(error) => error,
        };
        let offered = error.host_key.as_ref().filter(|_| {
            tries > 0
                && matches!(
                    error.kind,
                    ErrorKind::HostKeyUnknown | ErrorKind::HostKeyChanged
                )
        });
        let Some(offered) = offered else {
            return Err(with_host_key_advice(error, sign_in.command_line));
        };
        if approval
            .as_ref()
            .is_some_and(|approved| approved.fingerprint == offered.fingerprint)
        {
            return Err(with_host_key_advice(error, sign_in.command_line));
        }
        let for_server = asks_about_server(&profile, offered);
        let next = match &pinned {
            // The pinned key alone gets the server in. A jump host's approval took its place
            // for the attempt that stopped here.
            Some(pin) if for_server => {
                (pin.fingerprint == offered.fingerprint).then(|| pin.clone())
            }
            _ if sign_in.trust_new_host && error.kind == ErrorKind::HostKeyUnknown => {
                log::info!(
                    "Trusting the new key of {}:{} ({} {})",
                    offered.host,
                    offered.port,
                    offered.algorithm,
                    offered.fingerprint
                );
                Some(HostKeyApproval::of(offered, sign_in.save_host_key))
            }
            _ => None,
        };
        match next {
            Some(next) => approval = Some(next),
            None if pinned.is_some() && for_server => return Err(not_the_pinned_key(error)),
            None => return Err(with_host_key_advice(error, sign_in.command_line)),
        }
    }
}

/// Whether a question about a host key is about the server itself rather than a jump host on
/// the way to it. One that names neither, as a WebDAV or S3 address can, is the server's.
fn asks_about_server(profile: &ConnectProfile, offered: &HostKeyInfo) -> bool {
    let names = |hop: &ConnectProfile| {
        hop.port == offered.port && hop.host.trim().eq_ignore_ascii_case(offered.host.trim())
    };
    names(profile) || !profile.route.jump_hosts.iter().any(names)
}

/// A key on record is accepted before an approval is looked at, so the key the server proved it
/// holds is held to the pin once it is in.
async fn held_to_pin(
    context: &TaskContext<'_>,
    session: SessionInfo,
    pin: Option<&str>,
    profile: &ConnectProfile,
) -> AppResult<SessionInfo> {
    let Some(pin) = pin else {
        return Ok(session);
    };
    let shown = context
        .sessions
        .get(&session.id)
        .await
        .map(|opened| opened.host_key_fingerprint.clone());
    if matches!(&shown, Ok(shown) if shown == pin) {
        return Ok(session);
    }
    if let Err(error) = context.sessions.disconnect(&session.id).await {
        log::warn!("Could not disconnect: {}", error.message);
    }
    Err(match shown {
        Err(error) => error,
        Ok(shown) if shown.is_empty() => AppError::invalid(format!(
            "--host-key cannot be checked for {}",
            profile.protocol.display_name()
        )),
        Ok(shown) => AppError::new(
            ErrorKind::HostKeyChanged,
            format!(
                "{}:{} showed the key {shown}, the one on record for it, not the one given with --host-key",
                profile.host.trim(),
                profile.port
            ),
        ),
    })
}

/// The server showed a key, but not the pinned one: no option settles that but the right pin.
fn not_the_pinned_key(mut error: AppError) -> AppError {
    error.message = format!(
        "{} The server showed another key than the one given with --host-key.",
        error.message
    );
    error
}

/// A task cannot ask whether to trust a server, so the error says how to settle it once.
fn with_host_key_advice(mut error: AppError, command_line: bool) -> AppError {
    if error.host_key.is_some() {
        let advice = if command_line {
            "Connect to the server once from Poros and trust its key, or run this with --trust-new-host --save-host-key, then the task can run on its own."
        } else {
            "Connect to the server once from Poros and trust its key, then the task can run on its own."
        };
        error.message = format!("{} {advice}", error.message);
    }
    error
}

async fn synchronize(
    task: &ScheduledTask,
    session_id: &str,
    context: &TaskContext<'_>,
) -> AppResult<Outcome> {
    let TaskAction::Sync {
        local_path,
        remote_path,
        direction,
        options,
        arguments,
        ..
    } = &task.action
    else {
        return Err(AppError::invalid("Not a synchronization"));
    };
    let plan = context
        .sync
        .compare(SyncRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            local_path: local_path.trim().to_string(),
            remote_path: remote_path.trim().to_string(),
            direction: *direction,
            options: options.as_ref().clone(),
            arguments: arguments.clone(),
        })
        .await?;
    let empty_folder = context.sync.empty_mirrored_folder(&plan.plan_id);
    let held_back = |action: SyncAction| {
        empty_folder.is_some()
            && matches!(action, SyncAction::DeleteLocal | SyncAction::DeleteRemote)
    };
    let conflicts = plan
        .items
        .iter()
        .filter(|item| item.action == SyncAction::Conflict)
        .count();
    let choices: Vec<SyncChoice> = plan
        .items
        .iter()
        .filter(|item| item.action != SyncAction::Conflict && !held_back(item.action))
        .map(|item| SyncChoice {
            id: item.id,
            action: item.action,
        })
        .collect();
    let mut notes = match conflicts {
        0 => String::new(),
        1 => ", 1 conflict left for you to settle".into(),
        count => format!(", {count} conflicts left for you to settle"),
    };
    if let Some(folder) = &empty_folder {
        notes.push_str(&format!(
            ", nothing deleted because {folder} was empty (synchronize from Poros once if it is meant to be)"
        ));
    }
    let settled = conflicts == 0 && empty_folder.is_none();
    if choices.is_empty() {
        context.sync.discard(&plan.plan_id);
        let message = if empty_folder.is_some() {
            format!("Nothing copied{notes}")
        } else {
            format!("The folders already match{notes}")
        };
        return Ok(Outcome {
            succeeded: settled,
            message,
        });
    }

    // Its files carry the task's name in the transfer list and start whatever "start
    // transfers automatically" says, since no one is there to press Start.
    let batch = JobBatch::new(task.name.clone(), None);
    let summary = context
        .sync
        .run_in_batch(
            context.transfers,
            SyncRunRequest {
                plan_id: plan.plan_id,
                choices,
            },
            Some(batch.clone()),
        )
        .await?;
    if summary.dry_run {
        return Ok(Outcome {
            succeeded: settled,
            message: format!(
                "Dry run: {} would be copied ({}), {} deleted{notes}",
                plural(summary.queued_files, "file"),
                format_size(summary.queued_bytes),
                plural(summary.deleted, "item")
            ),
        });
    }
    let report = context
        .sync
        .finished(&summary.run_id)
        .await
        .unwrap_or_default();
    context.done_with(&batch);
    let failures = summary.failures.len().max(report.failures.len()) as u64;
    let mut message = format!(
        "{} copied ({})",
        plural(report.copied, "file"),
        format_size(report.bytes)
    );
    if report.deleted > 0 {
        message.push_str(&format!(", {} deleted", plural(report.deleted, "item")));
    }
    if report.failed > 0 {
        message.push_str(&format!(", {} failed", plural(report.failed, "file")));
    }
    if failures > 0 {
        message.push_str(&format!(
            ", {} could not be changed",
            plural(failures, "item")
        ));
    }
    message.push_str(&notes);
    Ok(Outcome {
        succeeded: report.failed == 0 && failures == 0 && settled,
        message,
    })
}

async fn run_command(
    command: &str,
    directory: Option<&str>,
    session_id: &str,
    context: &TaskContext<'_>,
) -> AppResult<Outcome> {
    let session = context.sessions.get(session_id).await?;
    let mut last_line = String::new();
    let mut pending = String::new();
    let result = remote_command::run_on(
        &session,
        command,
        directory,
        &CancellationToken::new(),
        |_, text| {
            pending.push_str(text);
            while let Some(end) = pending.find('\n') {
                let line = pending[..end].trim_end_matches('\r').to_string();
                pending.drain(..=end);
                if !line.trim().is_empty() {
                    context
                        .events
                        .log(LogLevel::Server, Some(session_id), line.as_str());
                    last_line = line;
                }
            }
        },
    )
    .await?;
    if !pending.trim().is_empty() {
        context
            .events
            .log(LogLevel::Server, Some(session_id), pending.as_str());
        last_line = pending;
    }
    let mut message = format!("Command {}", result.describe());
    if !last_line.trim().is_empty() {
        let shown: String = last_line
            .trim()
            .chars()
            .take(MAX_RESULT_LINE_CHARS)
            .collect();
        message.push_str(&format!(": {shown}"));
    }
    if result.exit_status.is_none() && !result.stopped && result.signal.is_none() {
        return Err(AppError::new(
            ErrorKind::Ssh,
            "The server closed the command without saying how it ended",
        ));
    }
    Ok(Outcome {
        succeeded: result.succeeded(),
        message,
    })
}

pub(super) fn plural(count: u64, noun: &str) -> String {
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
    fn a_key_question_is_the_servers_unless_it_names_a_jump_host() {
        let hop = |host: &str, port: u16| -> ConnectProfile {
            serde_json::from_value(serde_json::json!({
                "protocol": "sftp",
                "host": host,
                "port": port,
                "username": "root",
                "auth": { "type": "agent" },
            }))
            .unwrap()
        };
        let asked = |host: &str, port: u16| HostKeyInfo {
            host: host.into(),
            port,
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:abc".into(),
            certificate_problem: None,
            differs_from_pin: false,
        };
        let mut profile = hop(" files.lan", 22);
        assert!(asks_about_server(&profile, &asked("FILES.lan", 22)));
        // A WebDAV or S3 server is asked about under the host of its address.
        assert!(asks_about_server(&profile, &asked("dav.files.lan", 443)));
        profile.route.jump_hosts.push(hop("bastion.lan", 2222));
        assert!(!asks_about_server(&profile, &asked("Bastion.lan", 2222)));
        assert!(asks_about_server(&profile, &asked("bastion.lan", 22)));
        assert!(asks_about_server(&profile, &asked("files.lan", 22)));
    }

    #[test]
    fn an_untrusted_server_is_answered_with_what_settles_it() {
        let unknown = || {
            AppError::host_key(
                ErrorKind::HostKeyUnknown,
                HostKeyInfo {
                    host: "files.lan".into(),
                    port: 22,
                    algorithm: "ssh-ed25519".into(),
                    fingerprint: "SHA256:abc".into(),
                    certificate_problem: None,
                    differs_from_pin: false,
                },
            )
        };
        let in_the_app = with_host_key_advice(unknown(), false);
        assert!(in_the_app
            .message
            .ends_with("trust its key, then the task can run on its own."));
        assert!(with_host_key_advice(unknown(), true)
            .message
            .contains("--trust-new-host"));
        // Other failures are left as they are.
        let refused = AppError::new(ErrorKind::AuthFailed, "Wrong password");
        assert_eq!(
            with_host_key_advice(refused, true).message,
            "Wrong password"
        );
    }
}
