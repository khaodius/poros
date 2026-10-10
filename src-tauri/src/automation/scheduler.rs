//! Scheduled tasks, kept in `schedules.json` in the app config folder and run while Poros is
//! open. A run missed while Poros was closed happens once at the next start, unless the task
//! says to skip it. Mirrored in `src/lib/types.ts`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::remote_command;
use super::schedule::Trigger;
use crate::connections::ConnectionStore;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel, Store};
use crate::format::format_size;
use crate::route;
use crate::session::SessionManager;
use crate::settings::{ConnectionSettings, SettingsStore};
use crate::storage;
use crate::sync::{
    CompareMode, SyncAction, SyncChoice, SyncDirection, SyncManager, SyncRequest, SyncRunRequest,
};
use crate::transfer::TransferManager;

/// Sessions opened for tasks belong to no window, so closing one never disconnects them.
const SCHEDULER_OWNER: &str = "scheduler";
/// Checks the clock at least this often, which also notices a wake from sleep.
const TICK: Duration = Duration::from_secs(30);
/// A run this late at startup counts as missed rather than just due.
const MISSED_AFTER_SECS: i64 = 60;
const TRANSFER_POLL: Duration = Duration::from_secs(1);
const MAX_NAME_CHARS: usize = 200;
/// The last output line of a command task is kept as its result, cut to this length.
const MAX_RESULT_LINE_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    /// Empty for a task not saved yet.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub trigger: Trigger,
    pub action: TaskAction,
    /// Runs once at the next start when Poros was closed at the time.
    #[serde(default = "default_true")]
    pub run_missed: bool,
    /// Seconds since the Unix epoch. Worked out here.
    #[serde(default)]
    pub next_run: Option<i64>,
    #[serde(default)]
    pub last_run: Option<LastRun>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TaskAction {
    /// Folder synchronization, which with one direction is also how files are uploaded or
    /// downloaded on a schedule. Conflicts are left for the user.
    #[serde(rename_all = "camelCase")]
    Sync {
        connection_id: String,
        local_path: String,
        remote_path: String,
        direction: SyncDirection,
        compare: CompareMode,
        #[serde(default)]
        delete_extraneous: bool,
        #[serde(default)]
        skip_newer_on_target: bool,
        #[serde(default)]
        ignore_existing: bool,
        #[serde(default = "default_time_tolerance")]
        time_tolerance_secs: u32,
        #[serde(default)]
        excludes: Vec<String>,
    },
    /// A command run through the server's shell.
    #[serde(rename_all = "camelCase")]
    Command {
        connection_id: String,
        command: String,
        #[serde(default)]
        directory: Option<String>,
    },
}

fn default_time_tolerance() -> u32 {
    2
}

impl TaskAction {
    pub fn connection_id(&self) -> &str {
        match self {
            Self::Sync { connection_id, .. } | Self::Command { connection_id, .. } => connection_id,
        }
    }

    fn validate(&self) -> AppResult<()> {
        if self.connection_id().is_empty() {
            return Err(AppError::invalid("Choose a saved connection"));
        }
        match self {
            Self::Sync {
                local_path,
                remote_path,
                ..
            } => {
                if local_path.trim().is_empty() || remote_path.trim().is_empty() {
                    return Err(AppError::invalid(
                        "Enter both the local folder and the server folder",
                    ));
                }
            }
            Self::Command { command, .. } => {
                if command.trim().is_empty() {
                    return Err(AppError::invalid("Enter the command to run"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRun {
    /// Seconds since the Unix epoch.
    pub started: i64,
    pub finished: i64,
    pub succeeded: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    #[serde(flatten)]
    pub task: ScheduledTask,
    pub running: bool,
}

/// What a finished task reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub succeeded: bool,
    pub message: String,
}

#[derive(Default)]
struct State {
    tasks: Vec<ScheduledTask>,
    running: HashSet<String>,
}

impl State {
    fn view(&self, task: &ScheduledTask) -> TaskView {
        TaskView {
            task: task.clone(),
            running: self.running.contains(&task.id),
        }
    }
}

struct Inner {
    file: PathBuf,
    state: Mutex<State>,
    wake: Notify,
    events: Events,
}

impl Inner {
    fn persist(&self, state: &State) -> AppResult<()> {
        storage::write_json(&self.file, &state.tasks)
    }

    fn changed(&self) {
        self.events.store_changed(Store::Schedules);
    }

    /// Takes the tasks due at `now`, moving each on to its next run.
    fn take_due<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> Vec<ScheduledTask> {
        let mut state = self.state.lock().unwrap();
        let State { tasks, running } = &mut *state;
        let mut due = Vec::new();
        for task in tasks.iter_mut() {
            let is_due = task.enabled
                && task
                    .next_run
                    .is_some_and(|next_run| next_run <= now.timestamp());
            if !is_due {
                continue;
            }
            task.next_run = task.trigger.next_after(now).map(|next| next.timestamp());
            if running.insert(task.id.clone()) {
                due.push(task.clone());
            } else {
                self.events.log(
                    LogLevel::Warn,
                    None,
                    format!(
                        "Scheduled task \"{}\" is still running, so this run is skipped",
                        task.name
                    ),
                );
            }
        }
        if !due.is_empty() {
            if let Err(error) = self.persist(&state) {
                self.events.log(LogLevel::Error, None, error.message);
            }
        }
        due
    }

    fn next_wake(&self, now: i64) -> Duration {
        let state = self.state.lock().unwrap();
        let soonest = state
            .tasks
            .iter()
            .filter(|task| task.enabled)
            .filter_map(|task| task.next_run)
            .min();
        match soonest {
            Some(next_run) => Duration::from_secs(next_run.saturating_sub(now).max(0) as u64)
                .clamp(Duration::from_millis(200), TICK),
            None => TICK,
        }
    }

    fn finish(&self, id: &str, last_run: LastRun) {
        let mut state = self.state.lock().unwrap();
        state.running.remove(id);
        if let Some(task) = state.tasks.iter_mut().find(|task| task.id == id) {
            task.last_run = Some(last_run);
        }
        if let Err(error) = self.persist(&state) {
            self.events.log(LogLevel::Error, None, error.message);
        }
    }
}

pub struct Scheduler {
    inner: Arc<Inner>,
}

impl Scheduler {
    /// Reads the tasks and settles runs missed while Poros was closed. A missing file yields no
    /// tasks. One that cannot be read or understood is kept aside and reported.
    pub fn load(file: PathBuf, events: Events) -> Self {
        let tasks = match storage::read_json::<Vec<ScheduledTask>>(&file) {
            Ok(tasks) => tasks.unwrap_or_default(),
            Err(problem) => {
                events.log(
                    LogLevel::Warn,
                    None,
                    format!("{problem} Poros started with no scheduled tasks."),
                );
                Vec::new()
            }
        };
        let scheduler = Self {
            inner: Arc::new(Inner {
                file,
                state: Mutex::new(State {
                    tasks,
                    running: HashSet::new(),
                }),
                wake: Notify::new(),
                events,
            }),
        };
        scheduler.catch_up(&Local::now());
        scheduler
    }

    fn catch_up<Tz: TimeZone>(&self, now: &DateTime<Tz>) {
        let inner = &self.inner;
        let mut state = inner.state.lock().unwrap();
        let mut changed = false;
        for task in state.tasks.iter_mut().filter(|task| task.enabled) {
            let next_run = match task.next_run {
                Some(next_run) if next_run < now.timestamp() - MISSED_AFTER_SECS => {
                    if task.run_missed {
                        inner.events.log(
                            LogLevel::Info,
                            None,
                            format!(
                                "Running scheduled task \"{}\", missed while Poros was closed",
                                task.name
                            ),
                        );
                        Some(now.timestamp())
                    } else {
                        inner.events.log(
                            LogLevel::Info,
                            None,
                            format!(
                                "Skipped a run of scheduled task \"{}\" missed while Poros was closed",
                                task.name
                            ),
                        );
                        task.trigger.next_after(now).map(|next| next.timestamp())
                    }
                }
                Some(next_run) => Some(next_run),
                None => continue,
            };
            changed |= next_run != task.next_run;
            task.next_run = next_run;
        }
        if changed {
            if let Err(error) = inner.persist(&state) {
                inner.events.log(LogLevel::Error, None, error.message);
            }
        }
    }

    /// Runs due tasks on Tauri's runtime for as long as the app runs.
    pub fn start(&self, app: AppHandle) {
        let inner = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let now = Local::now();
                for task in inner.take_due(&now) {
                    tauri::async_runtime::spawn(execute(inner.clone(), app.clone(), task));
                }
                let wait = inner.next_wake(Local::now().timestamp());
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = inner.wake.notified() => {}
                }
            }
        });
    }

    pub fn list(&self) -> Vec<TaskView> {
        let state = self.inner.state.lock().unwrap();
        state.tasks.iter().map(|task| state.view(task)).collect()
    }

    /// Adds or replaces a task. Its run history stays as it was.
    pub fn save(&self, task: ScheduledTask) -> AppResult<TaskView> {
        let saved = self.save_at(task, &Local::now())?;
        self.inner.wake.notify_one();
        self.inner.changed();
        Ok(saved)
    }

    fn save_at<Tz: TimeZone>(
        &self,
        mut task: ScheduledTask,
        now: &DateTime<Tz>,
    ) -> AppResult<TaskView> {
        task.name = task.name.trim().chars().take(MAX_NAME_CHARS).collect();
        if task.name.is_empty() {
            return Err(AppError::invalid("Give the task a name"));
        }
        task.trigger.validate()?;
        task.action.validate()?;
        task.next_run = task.trigger.next_after(now).map(|next| next.timestamp());
        if task.enabled && task.next_run.is_none() {
            return Err(AppError::invalid("Pick a time in the future"));
        }

        let mut state = self.inner.state.lock().unwrap();
        if task.id.is_empty() {
            task.id = uuid::Uuid::new_v4().to_string();
        }
        match state
            .tasks
            .iter_mut()
            .find(|existing| existing.id == task.id)
        {
            Some(existing) => {
                task.last_run = existing.last_run.clone();
                *existing = task.clone();
            }
            None => {
                task.last_run = None;
                state.tasks.push(task.clone());
            }
        }
        self.inner.persist(&state)?;
        Ok(state.view(&task))
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let mut state = self.inner.state.lock().unwrap();
        state.tasks.retain(|task| task.id != id);
        self.inner.persist(&state)?;
        drop(state);
        self.inner.changed();
        Ok(())
    }

    /// Runs a task now, outside its schedule.
    pub fn run_now(&self, id: &str, app: AppHandle) -> AppResult<()> {
        let task = {
            let mut state = self.inner.state.lock().unwrap();
            let task = state
                .tasks
                .iter()
                .find(|task| task.id == id)
                .cloned()
                .ok_or_else(|| AppError::invalid("This task no longer exists"))?;
            if !state.running.insert(task.id.clone()) {
                return Err(AppError::invalid("This task is already running"));
            }
            task
        };
        tauri::async_runtime::spawn(execute(self.inner.clone(), app, task));
        Ok(())
    }
}

async fn execute(inner: Arc<Inner>, app: AppHandle, task: ScheduledTask) {
    inner.changed();
    inner.events.log(
        LogLevel::Info,
        None,
        format!("Scheduled task \"{}\" started", task.name),
    );
    let started = Local::now().timestamp();
    let outcome = {
        let sessions = app.state::<Arc<SessionManager>>();
        let connections = app.state::<Arc<ConnectionStore>>();
        let sync = app.state::<SyncManager>();
        let transfers = app.state::<TransferManager>();
        let context = TaskContext {
            sessions: sessions.inner(),
            connections: connections.inner().clone(),
            connection_settings: app.state::<SettingsStore>().get().connection,
            sync: sync.inner(),
            transfers: transfers.inner(),
            events: &inner.events,
        };
        perform(&task.action, &context).await
    };
    let outcome = outcome.unwrap_or_else(|error| Outcome {
        succeeded: false,
        message: error.message,
    });
    inner.events.log(
        if outcome.succeeded {
            LogLevel::Info
        } else {
            LogLevel::Error
        },
        None,
        format!("Scheduled task \"{}\": {}", task.name, outcome.message),
    );
    inner.finish(
        &task.id,
        LastRun {
            started,
            finished: Local::now().timestamp(),
            succeeded: outcome.succeeded,
            message: outcome.message,
        },
    );
    inner.changed();
}

/// What a task needs to run.
pub struct TaskContext<'a> {
    pub sessions: &'a SessionManager,
    pub connections: Arc<ConnectionStore>,
    pub connection_settings: ConnectionSettings,
    pub sync: &'a SyncManager,
    pub transfers: &'a TransferManager,
    pub events: &'a Events,
}

/// Connects with the task's saved connection, carries out its action and disconnects.
pub async fn perform(action: &TaskAction, context: &TaskContext<'_>) -> AppResult<Outcome> {
    let profile = {
        let store = context.connections.clone();
        let settings = context.connection_settings.clone();
        let id = action.connection_id().to_string();
        tokio::task::spawn_blocking(move || route::saved_profile(&store, &settings, &id)).await??
    };
    let session = context
        .sessions
        .connect(profile, None, SCHEDULER_OWNER)
        .await
        .map_err(|mut error| {
            if error.host_key.is_some() {
                error.message = format!(
                    "{} Connect to the server once from Poros and trust its key, then the task can run on its own.",
                    error.message
                );
            }
            error
        })?;
    let result = match action {
        TaskAction::Sync { .. } => synchronize(action, &session.id, context).await,
        TaskAction::Command {
            command, directory, ..
        } => run_command(command, directory.as_deref(), &session.id, context).await,
    };
    context.sessions.disconnect(&session.id).await?;
    result
}

async fn synchronize(
    action: &TaskAction,
    session_id: &str,
    context: &TaskContext<'_>,
) -> AppResult<Outcome> {
    let TaskAction::Sync {
        local_path,
        remote_path,
        direction,
        compare,
        delete_extraneous,
        skip_newer_on_target,
        ignore_existing,
        time_tolerance_secs,
        excludes,
        ..
    } = action
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
            compare: *compare,
            delete_extraneous: *delete_extraneous,
            skip_newer_on_target: *skip_newer_on_target,
            ignore_existing: *ignore_existing,
            time_tolerance_secs: *time_tolerance_secs,
            excludes: excludes.clone(),
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

    let summary = context
        .sync
        .run(
            context.transfers,
            SyncRunRequest {
                plan_id: plan.plan_id,
                choices,
            },
        )
        .await?;
    while context.transfers.unfinished_session_jobs(session_id) > 0 {
        tokio::time::sleep(TRANSFER_POLL).await;
    }
    let failed = context.transfers.failed_session_jobs(session_id);
    let copied = summary.queued_files.saturating_sub(failed as u64);
    let mut message = format!(
        "{} copied ({})",
        plural(copied, "file"),
        format_size(summary.queued_bytes)
    );
    if summary.deleted > 0 {
        message.push_str(&format!(", {} deleted", plural(summary.deleted, "item")));
    }
    if failed > 0 {
        message.push_str(&format!(", {} failed", plural(failed as u64, "file")));
    }
    if !summary.failures.is_empty() {
        message.push_str(&format!(
            ", {} could not be changed",
            plural(summary.failures.len() as u64, "item")
        ));
    }
    message.push_str(&notes);
    Ok(Outcome {
        succeeded: failed == 0 && summary.failures.is_empty() && settled,
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

fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn scheduler() -> (tempfile::TempDir, Scheduler) {
        let temp_dir = tempfile::tempdir().unwrap();
        let scheduler = Scheduler::load(temp_dir.path().join("schedules.json"), Events::default());
        (temp_dir, scheduler)
    }

    fn task(trigger: Trigger) -> ScheduledTask {
        ScheduledTask {
            id: String::new(),
            name: " Nightly backup ".into(),
            enabled: true,
            trigger,
            action: TaskAction::Command {
                connection_id: "server".into(),
                command: "backup.sh".into(),
                directory: None,
            },
            run_missed: true,
            next_run: None,
            last_run: None,
        }
    }

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, hour, minute, 0).unwrap()
    }

    #[test]
    fn saving_works_out_the_next_run_and_keeps_the_history() {
        let (_dir, scheduler) = scheduler();
        let every_hour = Trigger::Every {
            minutes: 60,
            start: at(9, 0).timestamp(),
        };
        let saved = scheduler.save_at(task(every_hour), &at(9, 30)).unwrap();
        assert!(!saved.task.id.is_empty());
        assert_eq!(saved.task.name, "Nightly backup");
        assert_eq!(saved.task.next_run, Some(at(10, 0).timestamp()));

        scheduler.inner.finish(
            &saved.task.id,
            LastRun {
                started: 1,
                finished: 2,
                succeeded: true,
                message: "ok".into(),
            },
        );
        let edited = ScheduledTask {
            name: "Hourly".into(),
            last_run: None,
            ..saved.task
        };
        let resaved = scheduler.save_at(edited, &at(9, 30)).unwrap();
        assert_eq!(resaved.task.last_run.unwrap().message, "ok");
        assert_eq!(scheduler.list().len(), 1);

        let reloaded = Scheduler::load(scheduler.inner.file.clone(), Events::default());
        assert_eq!(reloaded.list()[0].task.name, "Hourly");
    }

    #[test]
    fn a_corrupt_file_is_kept_and_reported_instead_of_overwritten() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("schedules.json");
        std::fs::write(&file, "[{\"id\": \"nightly\"").unwrap();
        let events = Events::keeping_startup_log();

        let scheduler = Scheduler::load(file.clone(), events.clone());
        assert!(scheduler.list().is_empty());
        let notices = events.take_startup_log();
        assert_eq!(notices.len(), 1);
        assert!(matches!(notices[0].level, LogLevel::Warn));
        assert!(notices[0].message.contains("schedules.json is not valid"));
        assert!(notices[0].message.contains("no scheduled tasks"));

        let hourly = Trigger::Every {
            minutes: 60,
            start: at(9, 0).timestamp(),
        };
        scheduler.save_at(task(hourly), &at(9, 30)).unwrap();
        let kept = crate::storage::tests::kept_copies(temp_dir.path(), "schedules.json.corrupt-");
        assert_eq!(kept.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&kept[0]).unwrap(),
            "[{\"id\": \"nightly\""
        );
    }

    #[test]
    fn invalid_tasks_are_refused() {
        let (_dir, scheduler) = scheduler();
        let past = task(Trigger::Once {
            at: at(8, 0).timestamp(),
        });
        let error = scheduler.save_at(past.clone(), &at(9, 0)).unwrap_err();
        assert!(error.message.contains("future"));
        // A disabled one-off may keep a time that has passed.
        let disabled = ScheduledTask {
            enabled: false,
            ..past
        };
        assert!(scheduler.save_at(disabled, &at(9, 0)).is_ok());

        let unnamed = ScheduledTask {
            name: "  ".into(),
            ..task(Trigger::Every {
                minutes: 5,
                start: 0,
            })
        };
        assert!(scheduler.save_at(unnamed, &at(9, 0)).is_err());
    }

    #[test]
    fn due_tasks_move_on_and_do_not_run_twice_at_once() {
        let (_dir, scheduler) = scheduler();
        let saved = scheduler
            .save_at(
                task(Trigger::Every {
                    minutes: 10,
                    start: at(9, 0).timestamp(),
                }),
                &at(8, 0),
            )
            .unwrap();
        assert!(scheduler.inner.take_due(&at(8, 59)).is_empty());
        let due = scheduler.inner.take_due(&at(9, 0));
        assert_eq!(due.len(), 1);
        assert!(scheduler.list()[0].running);
        assert_eq!(
            scheduler.list()[0].task.next_run,
            Some(at(9, 10).timestamp())
        );
        // Still running at the next slot: skipped, and the schedule moves on.
        assert!(scheduler.inner.take_due(&at(9, 10)).is_empty());
        assert_eq!(
            scheduler.list()[0].task.next_run,
            Some(at(9, 20).timestamp())
        );

        scheduler.inner.finish(
            &saved.task.id,
            LastRun {
                started: 0,
                finished: 0,
                succeeded: true,
                message: String::new(),
            },
        );
        assert_eq!(scheduler.inner.take_due(&at(9, 20)).len(), 1);
    }

    #[test]
    fn runs_missed_while_closed_happen_once_or_are_skipped() {
        let (_dir, scheduler) = scheduler();
        let daily = Trigger::Daily {
            minute_of_day: 6 * 60,
            weekdays: (0..7).collect(),
        };
        let catch_up = scheduler.save_at(task(daily.clone()), &at(5, 0)).unwrap();
        let skip = scheduler
            .save_at(
                ScheduledTask {
                    run_missed: false,
                    ..task(daily)
                },
                &at(5, 0),
            )
            .unwrap();

        scheduler.catch_up(&at(12, 0));
        let next_run = |id: &str| {
            scheduler
                .list()
                .into_iter()
                .find(|view| view.task.id == id)
                .unwrap()
                .task
                .next_run
        };
        assert_eq!(next_run(&catch_up.task.id), Some(at(12, 0).timestamp()));
        assert_eq!(
            next_run(&skip.task.id),
            Some((at(6, 0) + chrono::Duration::days(1)).timestamp())
        );
        assert_eq!(scheduler.inner.take_due(&at(12, 0)).len(), 1);
    }

    #[test]
    fn tasks_round_trip_as_tagged_json() {
        let json = serde_json::to_value(task(Trigger::Once { at: 5 })).unwrap();
        assert_eq!(json["action"]["type"], "command");
        assert_eq!(json["action"]["connectionId"], "server");
        assert_eq!(json["trigger"]["type"], "once");
        let parsed: ScheduledTask = serde_json::from_value(serde_json::json!({
            "name": "Sync",
            "trigger": { "type": "every", "minutes": 15, "start": 0 },
            "action": {
                "type": "sync",
                "connectionId": "c",
                "localPath": "/home/me/site",
                "remotePath": "/var/www",
                "direction": "upload",
                "compare": "sizeAndTime"
            }
        }))
        .unwrap();
        assert!(parsed.enabled && parsed.run_missed);
        assert!(matches!(
            parsed.action,
            TaskAction::Sync {
                time_tolerance_secs: 2,
                ..
            }
        ));
    }
}
