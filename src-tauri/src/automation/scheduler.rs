//! Scheduled tasks in the app: kept in `schedules.json` in the app config folder and run while
//! Poros is open. A run missed while Poros was closed happens once at the next start, unless
//! the task says to skip it. The tasks themselves, the file they share with `poros-cli` and
//! how one is carried out are in `task`, `board` and `runner`.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

use super::board::{Claim, Claimed, TaskBoard};
use super::runner::{self, SignIn, TaskContext};
use super::task::MISSED_AFTER_SECS;
pub use super::task::{ScheduledTask, TaskView};
use crate::connections::ConnectionStore;
use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel};
use crate::session::SessionManager;
use crate::settings::SettingsStore;
use crate::sync::SyncManager;
use crate::transfer::TransferManager;

struct Inner {
    board: TaskBoard,
    wake: Notify,
    events: Events,
}

pub struct Scheduler {
    inner: Arc<Inner>,
}

impl Scheduler {
    /// Reads the tasks and settles runs missed while Poros was closed. A missing file yields no
    /// tasks. One that cannot be read or understood is kept aside and reported.
    pub fn load(file: PathBuf, events: Events) -> Self {
        let board = TaskBoard::open(file, events.clone());
        for missed in board.settle_missed(&Local::now(), MISSED_AFTER_SECS) {
            let message = if missed.made_up {
                format!(
                    "Running scheduled task \"{}\", missed while Poros was closed",
                    missed.name
                )
            } else {
                format!(
                    "Skipped a run of scheduled task \"{}\" missed while Poros was closed",
                    missed.name
                )
            };
            events.log(LogLevel::Info, None, message);
        }
        Self {
            inner: Arc::new(Inner {
                board,
                wake: Notify::new(),
                events,
            }),
        }
    }

    /// Runs due tasks on Tauri's runtime for as long as the app runs, and takes in what
    /// `poros-cli` did to the tasks in the meantime.
    pub fn start(&self, app: AppHandle) {
        let inner = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                for claim in inner.board.take_due(&Local::now()) {
                    tauri::async_runtime::spawn(execute(inner.clone(), app.clone(), claim));
                }
                let wait = inner.board.next_wake(Local::now().timestamp());
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = inner.wake.notified() => {}
                }
            }
        });
    }

    pub fn list(&self) -> Vec<TaskView> {
        self.inner.board.refresh();
        self.inner.board.list()
    }

    /// Adds or replaces a task. Its run history stays as it was.
    pub fn save(&self, task: ScheduledTask) -> AppResult<TaskView> {
        let saved = self.inner.board.save(task, &Local::now())?;
        self.inner.wake.notify_one();
        Ok(saved)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.inner.board.delete(id)
    }

    /// Runs a task now, outside its schedule.
    pub fn run_now(&self, id: &str, app: AppHandle) -> AppResult<()> {
        match self.inner.board.claim_now(id)? {
            Claimed::Run(claim) => {
                tauri::async_runtime::spawn(execute(self.inner.clone(), app, claim));
                Ok(())
            }
            Claimed::Busy(_) => Err(AppError::invalid("This task is already running")),
        }
    }
}

async fn execute(inner: Arc<Inner>, app: AppHandle, claim: Claim) {
    let sessions = app.state::<Arc<SessionManager>>();
    let connections = app.state::<Arc<ConnectionStore>>();
    let sync = app.state::<SyncManager>();
    let transfers = app.state::<TransferManager>();
    let settings = app.state::<SettingsStore>().get();
    let context = TaskContext {
        sessions: sessions.inner(),
        connections: connections.inner().clone(),
        connection_settings: settings.connection,
        cloud_settings: settings.cloud,
        sync: sync.inner(),
        transfers: transfers.inner(),
        events: &inner.events,
        sign_in: SignIn::default(),
        // Shown in the transfer list.
        keep_ended_jobs: true,
    };
    // How it went is logged and written to the task there.
    let _ = runner::run_claimed(&inner.board, claim, &context).await;
}
