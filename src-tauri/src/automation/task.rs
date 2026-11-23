//! Scheduled tasks as `schedules.json` keeps them, and what is worked out from them without
//! touching a file or a server: whether one can be saved, which are due, which were missed.
//! The clock is always handed in, so all of it is tested with fixed times. Mirrored in
//! `src/lib/types.ts`.

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, TimeZone};
use serde::{Deserialize, Serialize};

use super::schedule::{expand_folder_pattern, Trigger};
use crate::error::{AppError, AppResult};
use crate::protocol;
use crate::remote_path;
use crate::sync::{SyncDirection, SyncOptions};
use crate::transfer::{Direction, ExistsAction};

/// A run this late when Poros starts counts as missed rather than just due.
pub const MISSED_AFTER_SECS: i64 = 60;
const MAX_NAME_CHARS: usize = 200;
/// More than anyone lists by hand; a folder stands for everything in it.
pub const MAX_TRANSFER_ITEMS: usize = 2000;

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

fn overwrite() -> ExistsAction {
    ExistsAction::Overwrite
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TaskAction {
    /// Folder synchronization over SSH. Conflicts are left for the user.
    #[serde(rename_all = "camelCase")]
    Sync {
        connection_id: String,
        local_path: String,
        remote_path: String,
        direction: SyncDirection,
        /// The rsync options, saved with the task so later changes to the defaults leave it
        /// as it was set up.
        #[serde(flatten)]
        options: Box<SyncOptions>,
        /// rsync arguments applied over `options`.
        #[serde(default)]
        arguments: String,
    },
    /// A command run through the server's shell.
    #[serde(rename_all = "camelCase")]
    Command {
        connection_id: String,
        command: String,
        #[serde(default)]
        directory: Option<String>,
    },
    /// Files and folders copied to or from a saved connection of any protocol, or from one
    /// saved connection to another, through the transfer queue.
    #[serde(rename_all = "camelCase")]
    Transfer {
        /// The server written to, or read from by a download.
        connection_id: String,
        /// The server a copy between two servers reads from.
        #[serde(default)]
        source_connection_id: Option<String>,
        direction: Direction,
        items: Vec<TaskItem>,
        /// The folder the items go into: on the server, or on this computer for a download.
        destination: String,
        /// What happens to a file already there. Never `ask`: no one is there to answer.
        #[serde(default = "overwrite")]
        exists: ExistsAction,
        /// A folder inside the destination named from the time of each run, such as
        /// `backup-{date}`. Empty for none.
        #[serde(default)]
        subfolder: String,
    },
}

/// A file or folder a transfer task copies. A folder goes with everything in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub path: String,
    /// As it was when it was added, for the icon in the list. A run looks again.
    #[serde(default)]
    pub is_dir: bool,
}

impl TaskItem {
    /// The name the item lands under: the last part of its path.
    pub fn name(&self, direction: Direction) -> String {
        match direction {
            Direction::Upload => Path::new(self.path.trim())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Direction::Download | Direction::Relay => {
                let path = protocol::normalize(self.path.trim());
                match path.as_str() {
                    "/" => String::new(),
                    path => remote_path::file_name(path).to_string(),
                }
            }
        }
    }
}

fn has_control_characters(text: &str) -> bool {
    text.chars().any(char::is_control)
}

impl TaskAction {
    /// The saved connection the task works on; for a copy between servers, the one written to.
    pub fn connection_id(&self) -> &str {
        match self {
            Self::Sync { connection_id, .. }
            | Self::Command { connection_id, .. }
            | Self::Transfer { connection_id, .. } => connection_id,
        }
    }

    /// Every saved connection the task signs in to.
    pub fn connection_ids(&self) -> Vec<&str> {
        let mut ids = vec![self.connection_id()];
        if let Self::Transfer {
            direction: Direction::Relay,
            source_connection_id: Some(source),
            ..
        } = self
        {
            if !ids.contains(&source.as_str()) {
                ids.push(source);
            }
        }
        ids
    }

    /// The tag the action is saved under.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Sync { .. } => "sync",
            Self::Command { .. } => "command",
            Self::Transfer { .. } => "transfer",
        }
    }

    /// What the task does, in a few words, for lists.
    pub fn describe(&self) -> String {
        match self {
            Self::Sync {
                local_path,
                remote_path,
                direction,
                ..
            } => match direction {
                SyncDirection::Upload => format!("Synchronize {local_path} to {remote_path}"),
                SyncDirection::Download => format!("Synchronize {remote_path} to {local_path}"),
                SyncDirection::Both => format!("Synchronize {local_path} with {remote_path}"),
            },
            Self::Command { command, .. } => format!("Run {command}"),
            Self::Transfer {
                direction,
                items,
                destination,
                subfolder,
                ..
            } => {
                let verb = match direction {
                    Direction::Upload => "Upload",
                    Direction::Download => "Download",
                    Direction::Relay => "Copy",
                };
                let what = match items.as_slice() {
                    [single] => single.path.clone(),
                    several => format!("{} items", several.len()),
                };
                let folder = match subfolder.trim().trim_matches('/') {
                    "" => destination.clone(),
                    pattern => format!("{}/{pattern}", destination.trim_end_matches(['/', '\\'])),
                };
                format!("{verb} {what} to {folder}")
            }
        }
    }

    /// Trims what the user typed and drops what the action has no use for.
    fn tidy(&mut self) {
        if let Self::Transfer {
            source_connection_id,
            direction,
            items,
            destination,
            subfolder,
            ..
        } = self
        {
            if *direction != Direction::Relay
                || source_connection_id.as_deref().is_some_and(str::is_empty)
            {
                *source_connection_id = None;
            }
            for item in items.iter_mut() {
                item.path = item.path.trim().to_string();
            }
            let mut seen = HashSet::new();
            items.retain(|item| seen.insert(item.path.clone()));
            *destination = destination.trim().to_string();
            *subfolder = subfolder.trim().trim_matches('/').to_string();
        }
    }

    /// Checks what a task needs before it can run; a file edited by hand is checked again as
    /// the run begins.
    pub(super) fn validate<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> AppResult<()> {
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
            Self::Transfer {
                source_connection_id,
                direction,
                items,
                destination,
                exists,
                subfolder,
                ..
            } => {
                if *direction == Direction::Relay && source_connection_id.is_none() {
                    return Err(AppError::invalid(
                        "Choose the saved connection to copy from",
                    ));
                }
                validate_items(items, *direction)?;
                if destination.is_empty() {
                    return Err(AppError::invalid(match direction {
                        Direction::Download => "Enter the folder on this computer to download into",
                        Direction::Upload | Direction::Relay => {
                            "Enter the folder on the server to copy into"
                        }
                    }));
                }
                if has_control_characters(destination) {
                    return Err(AppError::invalid(
                        "The destination folder has a character that cannot be in a path",
                    ));
                }
                if *direction == Direction::Download && !Path::new(destination).is_absolute() {
                    return Err(AppError::invalid(format!(
                        "{destination} is not a full path; name the folder on this computer from its drive or root"
                    )));
                }
                if *exists == ExistsAction::Ask {
                    return Err(AppError::invalid(
                        "Choose what happens to files that already exist; a task cannot ask",
                    ));
                }
                expand_folder_pattern(subfolder, now)?;
            }
        }
        Ok(())
    }
}

fn validate_items(items: &[TaskItem], direction: Direction) -> AppResult<()> {
    if items.is_empty() {
        return Err(AppError::invalid("Add at least one file or folder"));
    }
    if items.len() > MAX_TRANSFER_ITEMS {
        return Err(AppError::invalid(format!(
            "A task copies at most {MAX_TRANSFER_ITEMS} listed items; list their folder instead"
        )));
    }
    let mut names = HashSet::new();
    for item in items {
        let path = &item.path;
        if path.is_empty() || has_control_characters(path) {
            return Err(AppError::invalid(
                "An item has an empty path, or a character that cannot be in a path",
            ));
        }
        let full = match direction {
            Direction::Upload => Path::new(path).is_absolute(),
            Direction::Download | Direction::Relay => path.starts_with('/'),
        };
        if !full {
            return Err(AppError::invalid(format!(
                "{path} is not a full path; list items from the top of their drive or server"
            )));
        }
        let name = item.name(direction);
        if name.is_empty() {
            return Err(AppError::invalid(format!(
                "{path} is the top of a drive or server; list the folders inside it instead"
            )));
        }
        // Local file systems may ignore case, so names that differ only in case would land on
        // one file as well.
        if !names.insert(name.to_lowercase()) {
            return Err(AppError::invalid(format!(
                "Two items are named {name} and would land on the same name; put one in a task of its own"
            )));
        }
    }
    Ok(())
}

/// The result of a run whose process was stopped, killed or lost under it.
pub const CUT_SHORT: &str = "Stopped before it finished";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRun {
    /// Seconds since the Unix epoch.
    pub started: i64,
    pub finished: i64,
    pub succeeded: bool,
    pub message: String,
    /// Written as a run begins and replaced as it ends, so a run whose process is killed, or
    /// whose computer loses power, leaves a failure rather than the result of the run before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub under_way: bool,
}

impl LastRun {
    /// What a run that began at `started` leaves until it ends.
    pub fn under_way(started: i64) -> Self {
        Self {
            started,
            finished: started,
            succeeded: false,
            message: CUT_SHORT.into(),
            under_way: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    #[serde(flatten)]
    pub task: ScheduledTask,
    /// A run is under way, in this process or another.
    pub running: bool,
}

/// What a finished task reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub succeeded: bool,
    pub message: String,
}

/// A task as it is saved: tidied, checked, and with its next run worked out from `now`.
pub fn prepared<Tz: TimeZone>(
    mut task: ScheduledTask,
    now: &DateTime<Tz>,
) -> AppResult<ScheduledTask> {
    task.name = task.name.trim().chars().take(MAX_NAME_CHARS).collect();
    if task.name.is_empty() {
        return Err(AppError::invalid("Give the task a name"));
    }
    task.trigger.validate()?;
    task.action.tidy();
    task.action.validate(now)?;
    task.next_run = task.trigger.next_after(now).map(|next| next.timestamp());
    if task.enabled && task.next_run.is_none() {
        return Err(AppError::invalid("Pick a time in the future"));
    }
    Ok(task)
}

/// Adds a prepared task or replaces the one with its id, whose run history stays as it was.
pub fn put(tasks: &mut Vec<ScheduledTask>, mut task: ScheduledTask) -> ScheduledTask {
    if task.id.is_empty() {
        task.id = uuid::Uuid::new_v4().to_string();
    }
    match tasks.iter_mut().find(|existing| existing.id == task.id) {
        Some(existing) => {
            task.last_run = existing.last_run.clone();
            *existing = task.clone();
        }
        None => {
            task.last_run = None;
            tasks.push(task.clone());
        }
    }
    task
}

/// Takes the tasks due at `now`, moving each on to its next run. Returns where they are in
/// `tasks`.
pub fn take_due<Tz: TimeZone>(tasks: &mut [ScheduledTask], now: &DateTime<Tz>) -> Vec<usize> {
    let mut due = Vec::new();
    for (index, task) in tasks.iter_mut().enumerate() {
        if is_due(task, now.timestamp()) {
            task.next_run = task.trigger.next_after(now).map(|next| next.timestamp());
            due.push(index);
        }
    }
    due
}

pub fn is_due(task: &ScheduledTask, now: i64) -> bool {
    task.enabled && task.next_run.is_some_and(|next_run| next_run <= now)
}

/// A run that nothing was there to start at its time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissedRun {
    pub name: String,
    /// It runs now, late; otherwise the task waits for its next time.
    pub made_up: bool,
}

/// Settles the runs that are more than `missed_after_secs` late because nothing was running
/// tasks at their time: a task that makes up for missed runs becomes due now, once however
/// many it missed, and any other moves on to its next time.
pub fn settle_missed<Tz: TimeZone>(
    tasks: &mut [ScheduledTask],
    now: &DateTime<Tz>,
    missed_after_secs: i64,
) -> Vec<MissedRun> {
    let mut missed = Vec::new();
    for task in tasks.iter_mut().filter(|task| task.enabled) {
        let late = task
            .next_run
            .is_some_and(|next_run| next_run < now.timestamp() - missed_after_secs);
        if !late {
            continue;
        }
        task.next_run = if task.run_missed {
            Some(now.timestamp())
        } else {
            task.trigger.next_after(now).map(|next| next.timestamp())
        };
        missed.push(MissedRun {
            name: task.name.clone(),
            made_up: task.run_missed,
        });
    }
    missed
}

/// The task with this id, or the one of this name ignoring case.
pub fn find<'a>(tasks: &'a [ScheduledTask], wanted: &str) -> AppResult<&'a ScheduledTask> {
    if let Some(task) = tasks.iter().find(|task| task.id == wanted) {
        return Ok(task);
    }
    let named: Vec<&ScheduledTask> = tasks
        .iter()
        .filter(|task| task.name.eq_ignore_ascii_case(wanted.trim()))
        .collect();
    match named.as_slice() {
        [] => Err(AppError::new(
            crate::error::ErrorKind::NotFound,
            format!("There is no scheduled task {wanted}"),
        )),
        [task] => Ok(task),
        several => Err(AppError::invalid(format!(
            "{} scheduled tasks are named {wanted}; name one by its id: {}",
            several.len(),
            several
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::Utc;

    pub(crate) fn task(trigger: Trigger) -> ScheduledTask {
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

    pub(crate) fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, hour, minute, 0).unwrap()
    }

    fn every(minutes: u32) -> Trigger {
        Trigger::Every {
            minutes,
            start: at(9, 0).timestamp(),
        }
    }

    /// A full path on the system the tests run on.
    fn local(path: &str) -> String {
        if cfg!(windows) {
            format!("C:\\{}", path.replace('/', "\\"))
        } else {
            format!("/{path}")
        }
    }

    fn upload(items: &[&str]) -> TaskAction {
        TaskAction::Transfer {
            connection_id: "server".into(),
            source_connection_id: None,
            direction: Direction::Upload,
            items: items
                .iter()
                .map(|path| TaskItem {
                    path: local(path),
                    is_dir: false,
                })
                .collect(),
            destination: "/var/backups".into(),
            exists: ExistsAction::Overwrite,
            subfolder: "backup-{date}".into(),
        }
    }

    fn transfer_task(action: TaskAction) -> ScheduledTask {
        ScheduledTask {
            action,
            ..task(every(60))
        }
    }

    fn refusal(action: TaskAction) -> String {
        prepared(transfer_task(action), &at(8, 0))
            .unwrap_err()
            .message
    }

    #[test]
    fn preparing_trims_the_name_and_works_out_the_next_run() {
        let prepared_task = prepared(task(every(60)), &at(9, 30)).unwrap();
        assert_eq!(prepared_task.name, "Nightly backup");
        assert_eq!(prepared_task.next_run, Some(at(10, 0).timestamp()));

        let unnamed = ScheduledTask {
            name: "  ".into(),
            ..task(every(5))
        };
        assert!(prepared(unnamed, &at(9, 0)).is_err());
    }

    #[test]
    fn a_time_that_has_passed_is_refused_unless_the_task_is_off() {
        let past = task(Trigger::Once {
            at: at(8, 0).timestamp(),
        });
        let error = prepared(past.clone(), &at(9, 0)).unwrap_err();
        assert!(error.message.contains("future"));
        // A disabled one-off may keep a time that has passed.
        let disabled = ScheduledTask {
            enabled: false,
            ..past
        };
        assert!(prepared(disabled, &at(9, 0)).is_ok());
    }

    #[test]
    fn putting_a_task_keeps_its_history_and_gives_new_ones_an_id() {
        let mut tasks = Vec::new();
        let saved = put(&mut tasks, task(every(60)));
        assert!(!saved.id.is_empty());
        tasks[0].last_run = Some(LastRun {
            started: 1,
            finished: 2,
            succeeded: true,
            message: "ok".into(),
            under_way: false,
        });
        let edited = ScheduledTask {
            name: "Hourly".into(),
            last_run: None,
            ..saved
        };
        let resaved = put(&mut tasks, edited);
        assert_eq!(resaved.last_run.unwrap().message, "ok");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "Hourly");
    }

    #[test]
    fn due_tasks_are_taken_once_and_move_on() {
        let mut tasks = vec![
            prepared(task(every(10)), &at(8, 0)).unwrap(),
            ScheduledTask {
                enabled: false,
                next_run: Some(at(8, 0).timestamp()),
                ..task(every(10))
            },
            prepared(
                task(Trigger::Once {
                    at: at(9, 5).timestamp(),
                }),
                &at(8, 0),
            )
            .unwrap(),
        ];
        assert!(take_due(&mut tasks, &at(8, 59)).is_empty());
        assert_eq!(take_due(&mut tasks, &at(9, 0)), [0]);
        assert_eq!(tasks[0].next_run, Some(at(9, 10).timestamp()));
        assert!(take_due(&mut tasks, &at(9, 0)).is_empty());
        // Late by more than one interval: one run, then the next slot on the grid.
        assert_eq!(take_due(&mut tasks, &at(9, 47)), [0, 2]);
        assert_eq!(tasks[0].next_run, Some(at(9, 50).timestamp()));
        // A one-off has no next run, so it is never due again.
        assert_eq!(tasks[2].next_run, None);
        assert_eq!(take_due(&mut tasks, &at(23, 0)), [0]);
    }

    #[test]
    fn missed_runs_are_made_up_once_or_passed_over() {
        let daily = Trigger::Daily {
            minute_of_day: 6 * 60,
            weekdays: (0..7).collect(),
        };
        let mut tasks = vec![
            prepared(task(daily.clone()), &at(5, 0)).unwrap(),
            prepared(
                ScheduledTask {
                    name: "No catching up".into(),
                    run_missed: false,
                    ..task(daily.clone())
                },
                &at(5, 0),
            )
            .unwrap(),
            ScheduledTask {
                enabled: false,
                next_run: Some(at(6, 0).timestamp()),
                ..task(daily)
            },
        ];
        // Half a minute late is due, not missed.
        let on_time = at(6, 0) + chrono::Duration::seconds(30);
        assert!(settle_missed(&mut tasks, &on_time, MISSED_AFTER_SECS).is_empty());

        let missed = settle_missed(&mut tasks, &at(12, 0), MISSED_AFTER_SECS);
        assert_eq!(
            missed,
            [
                MissedRun {
                    name: "Nightly backup".into(),
                    made_up: true
                },
                MissedRun {
                    name: "No catching up".into(),
                    made_up: false
                }
            ]
        );
        assert_eq!(tasks[0].next_run, Some(at(12, 0).timestamp()));
        assert_eq!(
            tasks[1].next_run,
            Some((at(6, 0) + chrono::Duration::days(1)).timestamp())
        );
        assert_eq!(take_due(&mut tasks, &at(12, 0)), [0]);
        // A wider allowance, as a timer that fires every hour needs, keeps a late run due.
        let mut hourly_timer = vec![ScheduledTask {
            run_missed: false,
            next_run: Some(at(6, 0).timestamp()),
            ..task(every(600))
        }];
        assert!(settle_missed(&mut hourly_timer, &at(6, 45), 3600).is_empty());
        assert_eq!(take_due(&mut hourly_timer, &at(6, 45)), [0]);
    }

    #[test]
    fn tasks_are_found_by_id_or_by_name() {
        let named = |id: &str, name: &str| ScheduledTask {
            id: id.into(),
            name: name.into(),
            ..task(every(60))
        };
        let tasks = [
            named("1", "Nightly backup"),
            named("2", "Twin"),
            named("3", "twin"),
        ];
        assert_eq!(find(&tasks, "nightly BACKUP").unwrap().id, "1");
        assert_eq!(find(&tasks, "3").unwrap().name, "twin");
        let error = find(&tasks, "Twin").unwrap_err();
        assert!(
            error.message.contains("2 scheduled tasks"),
            "{}",
            error.message
        );
        assert!(error.message.contains("2, 3"));
        let error = find(&tasks, "weekly").unwrap_err();
        assert_eq!(error.kind, crate::error::ErrorKind::NotFound);
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
        let TaskAction::Sync {
            options, arguments, ..
        } = &parsed.action
        else {
            panic!("not a sync task");
        };
        assert_eq!(options.time_tolerance_secs, 2);
        assert!(options.recursive && options.times);
        assert!(arguments.is_empty());

        // Options are saved alongside the older fields.
        let saved = serde_json::to_value(&parsed.action).unwrap();
        assert_eq!(saved["compare"], "sizeAndTime");
        assert_eq!(saved["deleteTiming"], "during");
        let read_back: TaskAction = serde_json::from_value(saved).unwrap();
        assert_eq!(read_back, parsed.action);
    }

    #[test]
    fn transfer_tasks_round_trip_with_their_defaults() {
        let saved = serde_json::to_value(upload(&["home/me/site", "home/me/notes.txt"])).unwrap();
        assert_eq!(saved["type"], "transfer");
        assert_eq!(saved["direction"], "upload");
        assert_eq!(saved["exists"], "overwrite");
        assert_eq!(saved["items"][1]["isDir"], false);
        assert_eq!(saved["subfolder"], "backup-{date}");
        assert_eq!(saved["sourceConnectionId"], serde_json::Value::Null);
        let read_back: TaskAction = serde_json::from_value(saved).unwrap();
        assert_eq!(read_back, upload(&["home/me/site", "home/me/notes.txt"]));

        // The least a task file may say about a transfer.
        let minimal: TaskAction = serde_json::from_value(serde_json::json!({
            "type": "transfer",
            "connectionId": "c",
            "direction": "download",
            "items": [{ "path": "/srv/logs" }],
            "destination": "/home/me/logs"
        }))
        .unwrap();
        let TaskAction::Transfer {
            exists,
            subfolder,
            source_connection_id,
            items,
            ..
        } = &minimal
        else {
            panic!("not a transfer task");
        };
        assert_eq!(*exists, ExistsAction::Overwrite);
        assert!(subfolder.is_empty() && source_connection_id.is_none());
        assert!(!items[0].is_dir);
        assert_eq!(minimal.kind(), "transfer");
    }

    #[test]
    fn a_tasks_file_written_by_0_7_0_loads_unchanged() {
        let fixture = include_str!("../../tests/fixtures/schedules-0.7.0.json");
        let tasks: Vec<ScheduledTask> = serde_json::from_str(fixture).unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].name, "Nightly site upload");
        assert_eq!(tasks[0].action.kind(), "sync");
        assert_eq!(tasks[0].next_run, Some(1_795_485_600));
        let last_run = tasks[0].last_run.as_ref().unwrap();
        assert!(last_run.succeeded);
        assert_eq!(last_run.message, "12 files copied (3.4 MB)");
        assert_eq!(tasks[1].action.kind(), "command");
        assert!(!tasks[1].enabled && !tasks[1].run_missed);
        assert_eq!(tasks[2].trigger, Trigger::Once { at: 1_795_500_000 });
        assert_eq!(tasks[2].last_run, None);

        // Written back, every value 0.7.0 wrote is still there, as it was.
        let original: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let rewritten = serde_json::to_value(&tasks).unwrap();
        assert_eq!(rewritten, original);
    }

    #[test]
    fn transfer_tasks_are_tidied_before_they_are_saved() {
        let TaskAction::Transfer {
            items,
            destination,
            subfolder,
            source_connection_id,
            ..
        } = prepared(
            transfer_task(TaskAction::Transfer {
                connection_id: "server".into(),
                source_connection_id: Some("other".into()),
                direction: Direction::Upload,
                items: vec![
                    TaskItem {
                        path: format!("  {} ", local("home/me/site")),
                        is_dir: true,
                    },
                    TaskItem {
                        path: local("home/me/site"),
                        is_dir: true,
                    },
                ],
                destination: " /var/backups ".into(),
                exists: ExistsAction::OverwriteIfNewer,
                subfolder: " /{year}/site-{date}/ ".into(),
            }),
            &at(8, 0),
        )
        .unwrap()
        .action
        else {
            panic!("not a transfer task");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, local("home/me/site"));
        assert_eq!(destination, "/var/backups");
        assert_eq!(subfolder, "{year}/site-{date}");
        // Only a copy between servers reads from a second connection.
        assert_eq!(source_connection_id, None);
    }

    #[test]
    fn transfer_tasks_that_cannot_run_are_refused() {
        assert!(prepared(transfer_task(upload(&["home/me/site"])), &at(8, 0)).is_ok());
        assert!(refusal(upload(&[])).contains("at least one"));

        let with = |change: &dyn Fn(&mut TaskAction)| {
            let mut action = upload(&["home/me/site"]);
            change(&mut action);
            refusal(action)
        };
        assert!(with(&|action| {
            if let TaskAction::Transfer { connection_id, .. } = action {
                connection_id.clear();
            }
        })
        .contains("saved connection"));
        assert!(with(&|action| {
            if let TaskAction::Transfer { exists, .. } = action {
                *exists = ExistsAction::Ask;
            }
        })
        .contains("cannot ask"));
        assert!(with(&|action| {
            if let TaskAction::Transfer { destination, .. } = action {
                destination.clear();
            }
        })
        .contains("folder on the server"));
        assert!(with(&|action| {
            if let TaskAction::Transfer { subfolder, .. } = action {
                *subfolder = "../{date}".into();
            }
        })
        .contains("cannot be a folder name"));
        assert!(with(&|action| {
            if let TaskAction::Transfer { direction, .. } = action {
                *direction = Direction::Relay;
            }
        })
        .contains("copy from"));
        assert!(with(&|action| {
            if let TaskAction::Transfer { items, .. } = action {
                items[0].path = "site".into();
            }
        })
        .contains("not a full path"));
        assert!(with(&|action| {
            if let TaskAction::Transfer { items, .. } = action {
                items[0].path = "line\nbreak".into();
            }
        })
        .contains("cannot be in a path"));

        let same_name = refusal(upload(&["home/me/site", "srv/Site"]));
        assert!(same_name.contains("same name"), "{same_name}");
    }

    #[test]
    fn downloads_and_copies_take_server_paths_and_a_local_or_server_folder() {
        let download = |items: &[&str], destination: &str| TaskAction::Transfer {
            connection_id: "server".into(),
            source_connection_id: None,
            direction: Direction::Download,
            items: items
                .iter()
                .map(|path| TaskItem {
                    path: (*path).into(),
                    is_dir: true,
                })
                .collect(),
            destination: destination.into(),
            exists: ExistsAction::Skip,
            subfolder: String::new(),
        };
        let folder = local("home/me/logs");
        assert!(prepared(transfer_task(download(&["/srv/logs"], &folder)), &at(8, 0)).is_ok());
        assert!(refusal(download(&["/"], &folder)).contains("top of a drive or server"));
        assert!(refusal(download(&["/srv/.."], &folder)).contains("top of a drive or server"));
        assert!(refusal(download(&["srv/logs"], &folder)).contains("not a full path"));
        assert!(refusal(download(&["/srv/logs"], "logs")).contains("not a full path"));

        let copy = TaskAction::Transfer {
            connection_id: "archive".into(),
            source_connection_id: Some("server".into()),
            direction: Direction::Relay,
            items: vec![TaskItem {
                path: "/srv/logs/".into(),
                is_dir: true,
            }],
            destination: "logs".into(),
            exists: ExistsAction::Rename,
            subfolder: "{weekday}".into(),
        };
        assert_eq!(copy.connection_ids(), ["archive", "server"]);
        assert_eq!(copy.describe(), "Copy /srv/logs/ to logs/{weekday}");
        let TaskAction::Transfer { items, .. } = &copy else {
            unreachable!()
        };
        assert_eq!(items[0].name(Direction::Relay), "logs");
        assert!(prepared(transfer_task(copy), &at(8, 0)).is_ok());
    }
}
