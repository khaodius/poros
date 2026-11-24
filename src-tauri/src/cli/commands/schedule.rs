//! `schedule`: the scheduled tasks saved in Poros, listed, run now, run when they are due, or
//! run by a daemon as they fall due. They are made and edited in the app. The command line
//! shares their file with it, runs them with the same code, and writes their results back.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use chrono::Local;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::Future;
use serde::Serialize;
use serde_json::Value;

use super::browse::table;
use crate::automation::board::{Claim, Claimed, TaskBoard};
use crate::automation::runner::{self, SignIn, TaskContext};
use crate::automation::schedule::date_and_time;
use crate::automation::task::{self, MissedRun, Outcome, ScheduledTask, TaskAction, TaskView};
use crate::cli::args::{ScheduleArgs, ScheduleCommand, ScheduleRunArgs};
use crate::cli::context::{config_dir, Context};
use crate::cli::exit::Exit;
use crate::cli::output::{time_iso, time_string, Reply};
use crate::cli::progress::Reporter;
use crate::cli::target;
use crate::cli::transfers;
use crate::error::{AppError, AppResult};
use crate::settings::TransferSettings;
use crate::ssh::proxy::ProxyKind;
use crate::sync::SyncManager;
use crate::transfer::{JobState, TransferManager};

const POLL: Duration = Duration::from_millis(100);
/// After an interrupt, the queue gets this long to stop its workers and remove partial files.
const WIND_DOWN: Duration = Duration::from_millis(1500);
const NO_TASKS: &str = "No scheduled tasks; make them in Poros, under Scheduler";

pub async fn schedule(context: &Context, arguments: &ScheduleArgs) -> AppResult<Reply> {
    let file = config_dir(&context.options)?.join("schedules.json");
    match &arguments.command {
        ScheduleCommand::List => list(&file, context),
        ScheduleCommand::Run(run) => run_now(&file, context, run).await,
        ScheduleCommand::Daemon => daemon(file, context).await,
        ScheduleCommand::Enable(named) => set_enabled(&file, context, &named.tasks, true),
        ScheduleCommand::Disable(named) => set_enabled(&file, context, &named.tasks, false),
    }
}

/// The app's tasks, or `None` when it has none. Looking makes no file of its own then.
fn board(file: &Path, context: &Context) -> AppResult<Option<TaskBoard>> {
    if !file.exists() {
        return Ok(None);
    }
    TaskBoard::borrow(file.to_path_buf(), context.events.clone()).map(Some)
}

/// A task as `list` and `enable` show it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    id: String,
    name: String,
    enabled: bool,
    running: bool,
    kind: &'static str,
    /// What it does, in a few words.
    description: String,
    /// When it runs, in words.
    schedule: String,
    next_run: Option<String>,
    last_run: Option<ListedRun>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListedRun {
    started: Option<String>,
    finished: Option<String>,
    succeeded: bool,
    message: String,
    /// Written as the run began and never replaced: the run is still going,
    /// or its process ended before it could say how the run went.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    under_way: bool,
}

impl Listed {
    fn of(view: &TaskView) -> Self {
        let task = &view.task;
        Self {
            id: task.id.clone(),
            name: task.name.clone(),
            enabled: task.enabled,
            running: view.running,
            kind: task.action.kind(),
            description: task.action.describe(),
            schedule: task.trigger.describe(&Local),
            next_run: time_iso(task.next_run.filter(|_| task.enabled)),
            last_run: task.last_run.as_ref().map(|last_run| ListedRun {
                started: time_iso(Some(last_run.started)),
                finished: time_iso(Some(last_run.finished)),
                succeeded: last_run.succeeded,
                message: last_run.message.clone(),
                under_way: last_run.under_way,
            }),
        }
    }

    fn row(view: &TaskView) -> [String; 6] {
        let task = &view.task;
        let next_run = match task.next_run {
            _ if !task.enabled => "off".to_string(),
            Some(next_run) => time_string(Some(next_run)),
            None => "-".to_string(),
        };
        let (last_run, result) = match &task.last_run {
            _ if view.running => (String::new(), "running now".to_string()),
            Some(last_run) => (
                time_string(Some(last_run.started)),
                format!(
                    "{}: {}",
                    if last_run.succeeded { "ok" } else { "failed" },
                    last_run.message
                ),
            ),
            None => ("-".to_string(), String::new()),
        };
        [
            task.name.clone(),
            task.action.kind().to_string(),
            task.trigger.describe(&Local),
            next_run,
            last_run,
            result,
        ]
    }
}

fn list(file: &Path, context: &Context) -> AppResult<Reply> {
    let views = match board(file, context)? {
        Some(board) => board.list(),
        None => Vec::new(),
    };
    if views.is_empty() {
        return Ok(Reply::new(Vec::<Listed>::new(), NO_TASKS));
    }
    let header = [["NAME", "KIND", "WHEN", "NEXT RUN", "LAST RUN", "RESULT"].map(String::from)];
    let rows: Vec<[String; 6]> = views.iter().map(Listed::row).collect();
    let listed: Vec<Listed> = views.iter().map(Listed::of).collect();
    Ok(Reply::new(listed, table(&header, &rows)))
}

fn set_enabled(
    file: &Path,
    context: &Context,
    wanted: &[String],
    enabled: bool,
) -> AppResult<Reply> {
    let Some(board) = board(file, context)? else {
        return Err(task::find(&[], &wanted[0]).unwrap_err());
    };
    let mut changed = Vec::with_capacity(wanted.len());
    let mut lines = Vec::with_capacity(wanted.len());
    for name in wanted {
        let view = board.set_enabled(name, enabled, &Local::now())?;
        lines.push(match view.task.next_run {
            Some(next_run) if enabled => format!(
                "{} is on; it runs next at {}",
                view.task.name,
                time_string(Some(next_run))
            ),
            _ if enabled => format!("{} is on", view.task.name),
            _ => format!("{} is off", view.task.name),
        });
        changed.push(Listed::of(&view));
    }
    Ok(Reply::new(changed, lines.join("\n")))
}

/// How one task's run went, as `run` and `daemon` report it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    id: String,
    name: String,
    /// `succeeded`, `failed` or `alreadyRunning`.
    state: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished: Option<String>,
    exit_code: u8,
    #[serde(skip)]
    exit: Exit,
}

impl RunReport {
    fn of(task: &ScheduledTask, result: &AppResult<Outcome>) -> Self {
        let (exit, message) = match result {
            Ok(outcome) if outcome.succeeded => (Exit::Success, outcome.message.clone()),
            // A command that failed fails the run; anything else did part of its work.
            Ok(outcome) => (
                match task.action {
                    TaskAction::Command { .. } => Exit::Failure,
                    _ => Exit::Partial,
                },
                outcome.message.clone(),
            ),
            Err(error) => (Exit::for_error(error.kind), error.message.clone()),
        };
        Self {
            id: task.id.clone(),
            name: task.name.clone(),
            state: if exit == Exit::Success {
                "succeeded"
            } else {
                "failed"
            },
            message,
            finished: time_iso(Some(Local::now().timestamp())),
            exit_code: exit.code(),
            exit,
        }
    }

    fn busy(task: &ScheduledTask) -> Self {
        Self {
            id: task.id.clone(),
            name: task.name.clone(),
            state: "alreadyRunning",
            message: format!(
                "{} is already running, in Poros or another poros-cli, so it was not started again",
                task.name
            ),
            finished: None,
            exit_code: Exit::Busy.code(),
            exit: Exit::Busy,
        }
    }

    fn line(&self) -> String {
        match self.exit {
            Exit::Busy => self.message.clone(),
            _ => format!("{}: {}", self.name, self.message),
        }
    }
}

/// What a run here needs beside the task: the app's settings and connections, and the sign-in
/// options given on the command line.
struct Runner {
    sync: SyncManager,
    transfers: TransferManager,
    sign_in: SignIn,
    keep_completed: bool,
}

impl Runner {
    /// `keep_completed` keeps finished files in the queue, for progress; a daemon lets them go,
    /// and a run's failures too once it has counted them.
    fn new(context: &Context, keep_completed: bool) -> AppResult<Self> {
        let given_for = match context.options.connection.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(wanted) => Some(
                target::find_saved(&context.store, wanted)?
                    .ok_or_else(|| {
                        AppError::invalid(format!(
                            "There is no saved connection {wanted}; with schedule, --connection names the saved connection a password is given for"
                        ))
                    })?
                    .id,
            ),
        };
        let (password, passphrase) = match given_for {
            Some(_) => target::given_secrets(&context.options)?,
            None => (None, None),
        };
        let options = &context.options;
        let settings = TransferSettings {
            auto_start: true,
            keep_queue: false,
            keep_completed,
            ..context.settings.transfers.clone()
        };
        Ok(Self {
            sync: SyncManager::new(context.sessions.clone(), context.events.clone()),
            transfers: TransferManager::new(
                context.sessions.clone(),
                context.events.clone(),
                settings,
            ),
            sign_in: SignIn {
                given_for,
                password,
                passphrase,
                host_key: options.host_key.clone(),
                trust_new_host: options.trust_new_host,
                save_host_key: options.save_host_key,
                command_line: true,
                timeout_secs: options.timeout,
            },
            keep_completed,
        })
    }

    fn task_context<'a>(&'a self, context: &'a Context) -> TaskContext<'a> {
        let mut connection_settings = context.settings.connection.clone();
        if context.options.no_proxy {
            connection_settings.proxy.kind = ProxyKind::None;
        }
        TaskContext {
            sessions: &context.sessions,
            connections: context.store.clone(),
            connection_settings,
            cloud_settings: context.settings.cloud.clone(),
            sync: &self.sync,
            transfers: &self.transfers,
            events: &context.events,
            sign_in: self.sign_in.clone(),
            keep_ended_jobs: self.keep_completed,
        }
    }

    /// Takes every job off the queue and gives the workers time to stop and clean up.
    async fn wind_down(&self) {
        let ids: Vec<_> = self
            .transfers
            .list()
            .jobs
            .iter()
            .filter(|job| !matches!(job.state, JobState::Done | JobState::Skipped))
            .map(|job| job.id)
            .collect();
        if !ids.is_empty() {
            self.transfers.remove(&ids);
            tokio::time::sleep(WIND_DOWN).await;
        }
    }
}

type Running<'a> = Pin<Box<dyn Future<Output = RunReport> + 'a>>;

fn start<'a>(board: &'a TaskBoard, claim: Claim, context: &'a TaskContext<'a>) -> Running<'a> {
    Box::pin(async move {
        let task = claim.task.clone();
        let result = runner::run_claimed(board, claim, context).await;
        RunReport::of(&task, &result)
    })
}

async fn run_now(file: &Path, context: &Context, arguments: &ScheduleRunArgs) -> AppResult<Reply> {
    let Some(board) = board(file, context)? else {
        if arguments.due {
            return Ok(Reply::new(Vec::<RunReport>::new(), NO_TASKS));
        }
        return Err(task::find(&[], &arguments.tasks[0]).unwrap_err());
    };
    let runner = Runner::new(context, true)?;
    let mut claims = Vec::new();
    let mut busy = Vec::new();
    if arguments.due {
        let now = Local::now();
        log_missed(board.settle_missed(&now, i64::from(arguments.missed_after) * 60));
        claims = board.take_due(&now);
        if claims.is_empty() {
            return Ok(Reply::new(Vec::<RunReport>::new(), "No task is due"));
        }
    } else {
        // Every name is found before any task is taken, so a mistyped one starts nothing and
        // leaves no task marked as stopped.
        let tasks: Vec<_> = board.list().into_iter().map(|view| view.task).collect();
        for wanted in &arguments.tasks {
            task::find(&tasks, wanted)?;
        }
        for wanted in &arguments.tasks {
            match board.claim_named(wanted)? {
                Claimed::Run(claim) => claims.push(claim),
                Claimed::Busy(task) if claims.iter().any(|claim| claim.task.id == task.id) => {}
                Claimed::Busy(task) => busy.push(RunReport::busy(&task)),
            }
        }
    }

    let task_context = runner.task_context(context);
    let mut running: FuturesUnordered<Running<'_>> = claims
        .into_iter()
        .map(|claim| start(&board, claim, &task_context))
        .collect();
    let mut reports = Vec::new();
    let mut reporter = Reporter::new(context.progress());
    let mut told_of_schedule = false;
    while !running.is_empty() {
        tokio::select! {
            Some(report) = running.next() => reports.push(report),
            _ = tokio::time::sleep(POLL) => {
                let list = runner.transfers.list();
                reporter.update(transfers::snapshot(&list.jobs));
                // A window of the speed limit schedule that pauses the run's files would look
                // like a run that hangs.
                if !told_of_schedule {
                    if let Some(notice) = transfers::scheduled_run_notice(&list, &Local::now()) {
                        log::warn!("{notice}");
                        told_of_schedule = true;
                    }
                }
            }
            _ = context.interrupt.cancelled() => {
                reporter.finish();
                // Each run that is dropped is written down as stopped.
                drop(running);
                runner.wind_down().await;
                return Err(AppError::cancelled());
            }
        }
    }
    reporter.finish();
    drop(running);

    // In the order the tasks were named, or are listed.
    let order: Vec<String> = match arguments.due {
        true => board.list().into_iter().map(|view| view.task.id).collect(),
        false => arguments.tasks.clone(),
    };
    let position = |report: &RunReport| {
        order
            .iter()
            .position(|wanted| {
                *wanted == report.id || wanted.trim().eq_ignore_ascii_case(&report.name)
            })
            .unwrap_or(usize::MAX)
    };
    reports.extend(busy);
    reports.sort_by_key(position);
    let exit = reports
        .iter()
        .map(|report| report.exit)
        .find(|exit| *exit != Exit::Success)
        .unwrap_or(Exit::Success);
    let text = reports
        .iter()
        .map(RunReport::line)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Reply::new(&reports, text).with_exit(exit))
}

/// Prints each result as its run ends: a line of text, or a JSON object per line.
fn announce(report: &RunReport, json: bool) {
    if json {
        crate::cli::output::print_json(&serde_json::to_value(report).unwrap_or(Value::Null));
    } else {
        crate::cli::output::print_text(&format!(
            "{} {}",
            date_and_time(&Local::now()),
            report.line()
        ));
    }
}

fn log_missed(missed: Vec<MissedRun>) {
    for missed in missed {
        if missed.made_up {
            log::info!(
                "Running scheduled task \"{}\", missed while nothing was running tasks",
                missed.name
            );
        } else {
            log::info!(
                "Skipped a run of scheduled task \"{}\" missed while nothing was running tasks",
                missed.name
            );
        }
    }
}

async fn daemon(file: PathBuf, context: &Context) -> AppResult<Reply> {
    let board = TaskBoard::borrow(file, context.events.clone())?;
    log_missed(board.settle_missed(&Local::now(), task::MISSED_AFTER_SECS));
    let runner = Runner::new(context, false)?;
    let task_context = runner.task_context(context);
    let json = context.options.json;
    log::info!("Running scheduled tasks as they fall due; Ctrl+C or SIGTERM stops");
    let mut running: FuturesUnordered<Running<'_>> = FuturesUnordered::new();
    loop {
        for claim in board.take_due(&Local::now()) {
            running.push(start(&board, claim, &task_context));
        }
        let wait = board.next_wake(Local::now().timestamp());
        tokio::select! {
            Some(report) = running.next(), if !running.is_empty() => announce(&report, json),
            _ = tokio::time::sleep(wait) => {}
            _ = context.interrupt.cancelled() => break,
        }
    }
    // Each run that is dropped is written down as stopped.
    let stopped = running.len();
    drop(running);
    runner.wind_down().await;
    let text = match stopped {
        0 => String::new(),
        1 => "Stopped 1 run that was under way".to_string(),
        count => format!("Stopped {count} runs that were under way"),
    };
    Ok(Reply::new(Value::Null, text))
}
