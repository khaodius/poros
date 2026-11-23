//! The scheduled tasks of one `schedules.json` as every Poros process sees them. The app, a
//! `poros-cli schedule daemon` and any number of `poros-cli schedule run` started by cron may
//! work on the file at once, so nothing here trusts what it has in memory:
//!
//! - Every change is made to a fresh copy of the file while `schedules.lock` is held, and the
//!   whole file is written back before the lock is let go. An edit in the app and a result
//!   written by the command line at the same moment both end up in the file.
//! - A task is taken for a run under a lock of its own (see `lock`), so it never runs twice
//!   at once, whoever starts it.
//! - What other processes wrote is noticed by looking at the file again: `refresh`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, TimeZone};
use sha2::{Digest, Sha256};

use super::lock::{Locks, RunLock, StoreLock};
use super::task::{self, LastRun, MissedRun, ScheduledTask, TaskView};
use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel, Store};
use crate::storage;

/// The file and the clock are looked at at least this often, which notices what another
/// process wrote, a wake from sleep and a clock that was set.
pub const TICK: Duration = Duration::from_secs(3);

/// Tells the tasks as one process left them from how the next did: what `schedules.json`
/// holds, and the mark in `schedules.lock`, which is new after every change. The mark alone
/// tells that a run began or ended, which leaves the tasks as they were. The time of the file
/// would not do for either, since two processes may write within one tick of the file
/// system's clock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Stamp {
    digest: Option<[u8; 32]>,
    mark: Option<String>,
}

fn digest(file: &Path) -> Option<[u8; 32]> {
    let contents = std::fs::read(file).ok()?;
    Some(Sha256::digest(&contents).into())
}

#[derive(Default)]
struct State {
    /// The tasks as the file had them when it was last read or written here.
    tasks: Vec<ScheduledTask>,
    /// The tasks other processes were running at that time.
    elsewhere: HashSet<String>,
    /// `None` until the file was looked at.
    stamp: Option<Stamp>,
}

pub struct TaskBoard {
    file: PathBuf,
    locks: Locks,
    events: Events,
    /// The app sets a file it cannot read aside and goes on; the command line only borrows
    /// the app's file, so it leaves it where it is and stops.
    owns_file: bool,
    state: Mutex<State>,
    /// The tasks this process is running.
    here: Arc<Mutex<HashSet<String>>>,
}

/// A task taken for one run. No other run of it starts, in any process, until this is handed
/// to `TaskBoard::finish` or dropped.
#[derive(Debug)]
pub struct Claim {
    pub task: ScheduledTask,
    lock: Option<RunLock>,
    here: Arc<Mutex<HashSet<String>>>,
}

impl Drop for Claim {
    fn drop(&mut self) {
        // The lock goes first: a task that is still counted here is never taken for one that
        // runs elsewhere.
        self.lock.take();
        self.here.lock().unwrap().remove(&self.task.id);
    }
}

/// What asking for a task to run now came to.
#[derive(Debug)]
pub enum Claimed {
    Run(Claim),
    /// A run of it is under way, in this process or another.
    Busy(ScheduledTask),
}

/// What a change did, which decides what is written for other processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Changed {
    Nothing,
    /// The tasks: `schedules.json` is written.
    Tasks,
    /// Only which tasks run: the mark tells.
    Runs,
}

impl Changed {
    fn tasks_if(changed: bool) -> Self {
        if changed {
            Self::Tasks
        } else {
            Self::Nothing
        }
    }
}

impl TaskBoard {
    /// The app's tasks. A missing file yields none. One that cannot be read or understood is
    /// kept aside and reported.
    pub fn open(file: PathBuf, events: Events) -> Self {
        let board = Self::new(file, events, true);
        if let Err(error) = board.change(|_, _| Ok(((), Changed::Nothing))) {
            board.events.log(LogLevel::Error, None, error.message);
        }
        board
    }

    /// The app's tasks for the command line, which stops at a file it cannot read or
    /// understand and leaves it as it is.
    pub fn borrow(file: PathBuf, events: Events) -> AppResult<Self> {
        let board = Self::new(file, events, false);
        board.change(|_, _| Ok(((), Changed::Nothing)))?;
        Ok(board)
    }

    fn new(file: PathBuf, events: Events, owns_file: bool) -> Self {
        Self {
            locks: Locks::beside(&file),
            file,
            events,
            owns_file,
            state: Mutex::new(State::default()),
            here: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// The tasks in the file right now.
    fn fresh(&self, _held: &StoreLock) -> AppResult<Vec<ScheduledTask>> {
        if !self.owns_file {
            let name = self.file.display();
            return match std::fs::read(&self.file) {
                Ok(contents) => serde_json::from_slice(&contents)
                    .map_err(|error| AppError::invalid(format!("{name} is not valid ({error})"))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(error) => Err(AppError::from(error).with_path(name.to_string())),
            };
        }
        match storage::read_json::<Vec<ScheduledTask>>(&self.file) {
            Ok(tasks) => Ok(tasks.unwrap_or_default()),
            Err(problem) => {
                let (tasks, started) = {
                    let state = self.state.lock().unwrap();
                    (state.tasks.clone(), state.stamp.is_none())
                };
                if tasks.is_empty() && started {
                    self.events.log(
                        LogLevel::Warn,
                        None,
                        format!("{problem} Poros started with no scheduled tasks."),
                    );
                    return Ok(tasks);
                }
                // The file went bad while Poros was open, as after an edit by hand. The tasks
                // it had are written back, so they are not lost with the next start.
                self.events.log(
                    LogLevel::Warn,
                    None,
                    format!("{problem} Poros goes on with the scheduled tasks it had."),
                );
                if let Err(error) = storage::write_json(&self.file, &tasks) {
                    self.events.log(LogLevel::Error, None, error.message);
                }
                Ok(tasks)
            }
        }
    }

    fn views(&self) -> Vec<(ScheduledTask, bool)> {
        self.list()
            .into_iter()
            .map(|view| (view.task, view.running))
            .collect()
    }

    /// Changes the tasks: `change` works on a fresh copy of the file with the store lock held
    /// and says what it did, which is then written. What other processes wrote in the meantime
    /// is taken in along the way.
    fn change<T>(
        &self,
        change: impl FnOnce(&mut Vec<ScheduledTask>, &StoreLock) -> AppResult<(T, Changed)>,
    ) -> AppResult<T> {
        let held = self.locks.store()?;
        let before = self.views();
        let mut tasks = self.fresh(&held)?;
        let (result, changed) = change(&mut tasks, &held)?;
        if changed == Changed::Tasks {
            storage::write_json(&self.file, &tasks)?;
        }
        if changed != Changed::Nothing {
            held.leave_mark()?;
        }
        let results_from_elsewhere = self.remember(tasks, &held);
        drop(held);
        for (level, message) in results_from_elsewhere {
            self.events.log(level, None, message);
        }
        if self.views() != before {
            self.events.store_changed(Store::Schedules);
        }
        Ok(result)
    }

    /// Keeps the tasks as the file has them now, and which of them other processes are
    /// running. Returns what to log about runs that ended in another process.
    fn remember(&self, tasks: Vec<ScheduledTask>, held: &StoreLock) -> Vec<(LogLevel, String)> {
        let here = self.here.lock().unwrap().clone();
        let elsewhere: HashSet<String> = tasks
            .iter()
            .filter(|task| !here.contains(&task.id) && self.locks.is_running(held, &task.id))
            .map(|task| task.id.clone())
            .collect();
        let mut state = self.state.lock().unwrap();
        let results = tasks
            .iter()
            .filter_map(|task| {
                let result = task.last_run.as_ref()?;
                let known = state.tasks.iter().find(|known| known.id == task.id)?;
                let new = known.last_run.as_ref() != Some(result);
                if !result.under_way {
                    return new.then(|| {
                        (
                            if result.succeeded {
                                LogLevel::Info
                            } else {
                                LogLevel::Error
                            },
                            format!(
                                "Scheduled task \"{}\" was run by another Poros process: {}",
                                task.name, result.message
                            ),
                        )
                    });
                }
                // A run another process began has no result until it ends. One that ended
                // with its process, killed or lost, keeps what was written as it began.
                let running = here.contains(&task.id) || elsewhere.contains(&task.id);
                let ended = !running && (new || state.elsewhere.contains(&task.id));
                ended.then(|| {
                    (
                        LogLevel::Error,
                        format!(
                            "Scheduled task \"{}\" stopped before it finished: the Poros process running it ended",
                            task.name
                        ),
                    )
                })
            })
            .collect();
        state.tasks = tasks;
        state.elsewhere = elsewhere;
        state.stamp = Some(Stamp {
            digest: digest(&self.file),
            mark: held.mark(),
        });
        results
    }

    /// Takes in what other processes did since the last look: tasks they changed, results
    /// they wrote and runs they began or ended. Cheap when nothing happened. Returns whether
    /// anything changed.
    pub fn refresh(&self) -> bool {
        let unchanged = {
            let state = self.state.lock().unwrap();
            let now = Stamp {
                digest: digest(&self.file),
                mark: self.locks.mark(),
            };
            // A run that ends with its process leaves no mark, so runs elsewhere are looked
            // at for as long as there are any.
            state.stamp.as_ref() == Some(&now) && state.elsewhere.is_empty()
        };
        if unchanged {
            return false;
        }
        let before = self.views();
        if let Err(error) = self.change(|_, _| Ok(((), Changed::Nothing))) {
            self.events.log(LogLevel::Error, None, error.message);
        }
        self.views() != before
    }

    pub fn list(&self) -> Vec<TaskView> {
        let here = self.here.lock().unwrap().clone();
        let state = self.state.lock().unwrap();
        state
            .tasks
            .iter()
            .map(|task| TaskView {
                task: task.clone(),
                running: here.contains(&task.id) || state.elsewhere.contains(&task.id),
            })
            .collect()
    }

    fn view(&self, task: ScheduledTask) -> TaskView {
        let running = self.here.lock().unwrap().contains(&task.id)
            || self.state.lock().unwrap().elsewhere.contains(&task.id);
        TaskView { task, running }
    }

    /// Adds or replaces a task. Its run history stays as it was.
    pub fn save<Tz: TimeZone>(
        &self,
        task: ScheduledTask,
        now: &DateTime<Tz>,
    ) -> AppResult<TaskView> {
        let task = task::prepared(task, now)?;
        let saved = self.change(|tasks, _| Ok((task::put(tasks, task), Changed::Tasks)))?;
        Ok(self.view(saved))
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.change(|tasks, held| {
            tasks.retain(|task| task.id != id);
            self.locks.forget(held, id);
            Ok(((), Changed::Tasks))
        })
    }

    /// Turns a task, named by its id or its name, on or off.
    pub fn set_enabled<Tz: TimeZone>(
        &self,
        wanted: &str,
        enabled: bool,
        now: &DateTime<Tz>,
    ) -> AppResult<TaskView> {
        let changed = self.change(|tasks, _| {
            let id = task::find(tasks, wanted)?.id.clone();
            let task = tasks
                .iter_mut()
                .find(|task| task.id == id)
                .expect("the task was just found");
            let next_run = task.trigger.next_after(now).map(|next| next.timestamp());
            if enabled && next_run.is_none() {
                return Err(AppError::invalid(format!(
                    "{} was set to run once, at a time that has passed; give it a new time in Poros",
                    task.name
                )));
            }
            let changed = task.enabled != enabled;
            if changed {
                task.enabled = enabled;
                task.next_run = next_run;
            }
            Ok((task.clone(), Changed::tasks_if(changed)))
        })?;
        Ok(self.view(changed))
    }

    /// Settles the runs that are more than `missed_after_secs` late because nothing was
    /// running tasks at their time: see `task::settle_missed`.
    pub fn settle_missed<Tz: TimeZone>(
        &self,
        now: &DateTime<Tz>,
        missed_after_secs: i64,
    ) -> Vec<MissedRun> {
        let settled = self.change(|tasks, _| {
            let missed = task::settle_missed(tasks, now, missed_after_secs);
            let changed = Changed::tasks_if(!missed.is_empty());
            Ok((missed, changed))
        });
        settled.unwrap_or_else(|error| {
            self.events.log(LogLevel::Error, None, error.message);
            Vec::new()
        })
    }

    /// Takes `task` for a run, if no run of it is under way anywhere. The task is given a
    /// failed result to keep until the run ends, written with the claim: a process that is
    /// killed, or a computer that loses power, then leaves that rather than the result of the
    /// run before.
    fn claim(
        &self,
        task: &mut ScheduledTask,
        held: &StoreLock,
        now: i64,
    ) -> AppResult<Option<Claim>> {
        let Some(lock) = self.locks.try_run(held, &task.id)? else {
            return Ok(None);
        };
        task.last_run = Some(LastRun::under_way(now));
        self.here.lock().unwrap().insert(task.id.clone());
        Ok(Some(Claim {
            task: task.clone(),
            lock: Some(lock),
            here: self.here.clone(),
        }))
    }

    /// Takes the tasks due at `now` for a run, moving each on to its next run. A task another
    /// process took at the same moment is not due here any more, and one that is still
    /// running misses this run.
    pub fn take_due<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> Vec<Claim> {
        self.refresh();
        let any_due = {
            let state = self.state.lock().unwrap();
            state
                .tasks
                .iter()
                .any(|task| task::is_due(task, now.timestamp()))
        };
        if !any_due {
            return Vec::new();
        }
        let taken = self.change(|tasks, held| {
            let due = task::take_due(tasks, now);
            let mut claims = Vec::new();
            for index in &due {
                let task = &mut tasks[*index];
                match self.claim(task, held, now.timestamp()) {
                    Ok(Some(claim)) => claims.push(claim),
                    Ok(None) => self.events.log(
                        LogLevel::Warn,
                        None,
                        format!(
                            "Scheduled task \"{}\" is still running, so this run is skipped",
                            task.name
                        ),
                    ),
                    Err(error) => self.events.log(
                        LogLevel::Error,
                        None,
                        format!(
                            "Scheduled task \"{}\" could not start: {}",
                            task.name, error.message
                        ),
                    ),
                }
            }
            Ok((claims, Changed::tasks_if(!due.is_empty())))
        });
        // A run whose new time could not be written does not start: it would be due again,
        // and start again, with every look.
        taken.unwrap_or_else(|error| {
            self.events.log(LogLevel::Error, None, error.message);
            Vec::new()
        })
    }

    /// Takes the task with this id for a run now, outside its schedule.
    pub fn claim_now(&self, id: &str) -> AppResult<Claimed> {
        self.claim_found(|tasks| {
            tasks
                .iter()
                .find(|task| task.id == id)
                .ok_or_else(|| AppError::invalid("This task no longer exists"))
        })
    }

    /// Takes the task with this id, or of this name, for a run now.
    pub fn claim_named(&self, wanted: &str) -> AppResult<Claimed> {
        self.claim_found(|tasks| task::find(tasks, wanted))
    }

    fn claim_found(
        &self,
        find: impl FnOnce(&[ScheduledTask]) -> AppResult<&ScheduledTask>,
    ) -> AppResult<Claimed> {
        self.change(|tasks, held| {
            let id = find(tasks)?.id.clone();
            let task = tasks
                .iter_mut()
                .find(|task| task.id == id)
                .expect("the task was just found");
            Ok(match self.claim(task, held, chrono::Utc::now().timestamp())? {
                Some(claim) => (Claimed::Run(claim), Changed::Tasks),
                None => (Claimed::Busy(task.clone()), Changed::Nothing),
            })
        })
    }

    /// Ends a run: its result is written to the task, which may then run again.
    pub fn finish(&self, claim: Claim, last_run: LastRun) {
        let id = claim.task.id.clone();
        let finished = self.change(|tasks, held| {
            let task = tasks.iter_mut().find(|task| task.id == id);
            let exists = task.is_some();
            if let Some(task) = task {
                task.last_run = Some(last_run.clone());
            }
            // Known here before the file is looked at again, so the result is not taken for
            // one another process wrote.
            if let Some(known) = self
                .state
                .lock()
                .unwrap()
                .tasks
                .iter_mut()
                .find(|task| task.id == id)
            {
                known.last_run = Some(last_run);
            }
            drop(claim);
            if !exists {
                // Deleted while it ran, when its lock file had to stay.
                self.locks.forget(held, &id);
            }
            Ok((
                (),
                if exists {
                    Changed::Tasks
                } else {
                    Changed::Runs
                },
            ))
        });
        if let Err(error) = finished {
            self.events.log(LogLevel::Error, None, error.message);
        }
    }

    /// How long until the next look: until the soonest task is due, and never longer than
    /// `TICK`.
    pub fn next_wake(&self, now: i64) -> Duration {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::schedule::Trigger;
    use crate::automation::task::tests::{at, task};

    fn folder() -> (tempfile::TempDir, PathBuf) {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("schedules.json");
        (folder, file)
    }

    fn board() -> (tempfile::TempDir, TaskBoard) {
        let (folder, file) = folder();
        (folder, TaskBoard::open(file, Events::default()))
    }

    fn every(minutes: u32) -> ScheduledTask {
        task(Trigger::Every {
            minutes,
            start: at(9, 0).timestamp(),
        })
    }

    fn result(message: &str) -> LastRun {
        LastRun {
            under_way: false,
            started: 1,
            finished: 2,
            succeeded: true,
            message: message.into(),
        }
    }

    fn run_now(board: &TaskBoard, id: &str) -> Claimed {
        board.claim_now(id).unwrap()
    }

    #[test]
    fn saving_works_out_the_next_run_and_keeps_the_history() {
        let (_folder, board) = board();
        let saved = board.save(every(60), &at(9, 30)).unwrap();
        assert!(!saved.task.id.is_empty());
        assert_eq!(saved.task.name, "Nightly backup");
        assert_eq!(saved.task.next_run, Some(at(10, 0).timestamp()));

        let Claimed::Run(claim) = run_now(&board, &saved.task.id) else {
            panic!("the task is not running");
        };
        board.finish(claim, result("ok"));
        let edited = ScheduledTask {
            name: "Hourly".into(),
            last_run: None,
            ..saved.task
        };
        let resaved = board.save(edited, &at(9, 30)).unwrap();
        assert_eq!(resaved.task.last_run.unwrap().message, "ok");
        assert_eq!(board.list().len(), 1);

        let reloaded = TaskBoard::open(board.file.clone(), Events::default());
        assert_eq!(reloaded.list()[0].task.name, "Hourly");
    }

    #[test]
    fn a_corrupt_file_is_kept_and_reported_instead_of_overwritten() {
        let (folder, file) = folder();
        std::fs::write(&file, "[{\"id\": \"nightly\"").unwrap();
        let events = Events::keeping_startup_log();

        let board = TaskBoard::open(file.clone(), events.clone());
        assert!(board.list().is_empty());
        let notices = events.take_startup_log();
        assert_eq!(notices.len(), 1);
        assert!(matches!(notices[0].level, LogLevel::Warn));
        assert!(notices[0].message.contains("schedules.json is not valid"));
        assert!(notices[0].message.contains("no scheduled tasks"));

        board.save(every(60), &at(9, 30)).unwrap();
        let kept = crate::storage::tests::kept_copies(folder.path(), "schedules.json.corrupt-");
        assert_eq!(kept.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&kept[0]).unwrap(),
            "[{\"id\": \"nightly\""
        );
    }

    #[test]
    fn a_file_that_goes_bad_while_open_does_not_cost_the_tasks() {
        let (folder, file) = folder();
        let events = Events::keeping_startup_log();
        let board = TaskBoard::open(file.clone(), events.clone());
        board.save(every(60), &at(9, 30)).unwrap();
        std::fs::write(&file, "not json at all").unwrap();

        assert!(!board.refresh());
        assert_eq!(board.list().len(), 1);
        let notices = events.take_startup_log();
        assert!(notices[0]
            .message
            .contains("goes on with the scheduled tasks"));
        assert_eq!(
            crate::storage::tests::kept_copies(folder.path(), "schedules.json.corrupt-").len(),
            1
        );
        let reloaded = TaskBoard::open(file, Events::default());
        assert_eq!(reloaded.list()[0].task.name, "Nightly backup");
    }

    #[test]
    fn the_command_line_leaves_a_file_it_cannot_read_as_it_is() {
        let (folder, file) = folder();
        std::fs::write(&file, "[{\"id\": \"nightly\"").unwrap();
        let error = TaskBoard::borrow(file.clone(), Events::default())
            .err()
            .unwrap();
        assert!(error.message.contains("is not valid"), "{}", error.message);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[{\"id\": \"nightly\""
        );
        assert!(
            crate::storage::tests::kept_copies(folder.path(), "schedules.json.corrupt-").is_empty()
        );

        // No file is no tasks, and looking makes none.
        let (_folder, file) = self::folder();
        assert!(TaskBoard::borrow(file.clone(), Events::default())
            .unwrap()
            .list()
            .is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn invalid_tasks_are_refused_and_nothing_is_written() {
        let (_folder, board) = board();
        let unnamed = ScheduledTask {
            name: "  ".into(),
            ..every(5)
        };
        assert!(board.save(unnamed, &at(9, 0)).is_err());
        assert!(board.list().is_empty());
        assert!(!board.file.exists());
    }

    #[test]
    fn due_tasks_move_on_and_do_not_run_twice_at_once() {
        let (_folder, board) = board();
        board.save(every(10), &at(8, 0)).unwrap();
        assert!(board.take_due(&at(8, 59)).is_empty());
        let mut due = board.take_due(&at(9, 0));
        assert_eq!(due.len(), 1);
        assert!(board.list()[0].running);
        assert_eq!(board.list()[0].task.next_run, Some(at(9, 10).timestamp()));
        // Still running at the next slot: skipped, and the schedule moves on.
        assert!(board.take_due(&at(9, 10)).is_empty());
        assert_eq!(board.list()[0].task.next_run, Some(at(9, 20).timestamp()));

        board.finish(due.remove(0), result(""));
        assert!(!board.list()[0].running);
        assert_eq!(board.take_due(&at(9, 20)).len(), 1);
    }

    #[test]
    fn runs_missed_while_closed_happen_once_or_are_skipped() {
        let (_folder, board) = board();
        let daily = Trigger::Daily {
            minute_of_day: 6 * 60,
            weekdays: (0..7).collect(),
        };
        let catch_up = board.save(task(daily.clone()), &at(5, 0)).unwrap();
        let skip = board
            .save(
                ScheduledTask {
                    run_missed: false,
                    ..task(daily)
                },
                &at(5, 0),
            )
            .unwrap();

        let missed = board.settle_missed(&at(12, 0), task::MISSED_AFTER_SECS);
        assert_eq!(missed.len(), 2);
        let next_run = |id: &str| {
            board
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
        assert_eq!(board.take_due(&at(12, 0)).len(), 1);
    }

    #[test]
    fn a_task_asked_to_run_now_runs_unless_it_already_does() {
        let (_folder, board) = board();
        let saved = board.save(every(10), &at(8, 0)).unwrap();
        let Claimed::Run(claim) = run_now(&board, &saved.task.id) else {
            panic!("the task is not running");
        };
        assert!(matches!(run_now(&board, &saved.task.id), Claimed::Busy(_)));
        // By name, as the command line asks.
        assert!(matches!(
            board.claim_named("nightly backup").unwrap(),
            Claimed::Busy(_)
        ));
        assert_eq!(
            board.claim_named("weekly").unwrap_err().kind,
            crate::error::ErrorKind::NotFound
        );
        assert!(board.claim_now("gone").is_err());

        // A run that is given up, as when its process is interrupted, frees the task.
        drop(claim);
        assert!(!board.list()[0].running);
        assert!(matches!(run_now(&board, &saved.task.id), Claimed::Run(_)));
    }

    #[test]
    fn two_processes_never_take_the_same_due_task() {
        let (_folder, file) = folder();
        let app = TaskBoard::open(file.clone(), Events::default());
        app.save(every(10), &at(8, 0)).unwrap();
        let daemon = TaskBoard::borrow(file, Events::default()).unwrap();

        // Both see the task fall due; the first to look takes it.
        let mut taken = daemon.take_due(&at(9, 0));
        assert_eq!(taken.len(), 1);
        assert!(app.take_due(&at(9, 0)).is_empty());
        let view = &app.list()[0];
        assert!(view.running);
        assert_eq!(view.task.next_run, Some(at(9, 10).timestamp()));
        // Asked to run it now, the app is told it is busy, and at its next slot it is skipped.
        let id = view.task.id.clone();
        assert!(matches!(run_now(&app, &id), Claimed::Busy(_)));
        assert!(app.take_due(&at(9, 10)).is_empty());
        assert_eq!(daemon.list()[0].task.next_run, Some(at(9, 10).timestamp()));
        assert!(daemon.refresh());
        assert_eq!(daemon.list()[0].task.next_run, Some(at(9, 20).timestamp()));

        daemon.finish(taken.remove(0), result("done by the daemon"));
        assert!(app.refresh());
        let view = &app.list()[0];
        assert!(!view.running);
        assert_eq!(
            view.task.last_run.as_ref().unwrap().message,
            "done by the daemon"
        );
        assert_eq!(app.take_due(&at(9, 20)).len(), 1);
    }

    #[test]
    fn of_many_boards_racing_for_a_due_task_exactly_one_runs_it() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Barrier;

        let (_folder, file) = folder();
        TaskBoard::open(file.clone(), Events::default())
            .save(every(10), &at(8, 0))
            .unwrap();
        let runners = 6;
        for round in 0..10 {
            let now = at(9, round * 5);
            let start = Arc::new(Barrier::new(runners));
            let runs = Arc::new(AtomicUsize::new(0));
            let threads: Vec<_> = (0..runners)
                .map(|_| {
                    let (file, start, runs) = (file.clone(), start.clone(), runs.clone());
                    std::thread::spawn(move || {
                        // A board of its own, as each process has.
                        let board = TaskBoard::borrow(file, Events::default()).unwrap();
                        start.wait();
                        for claim in board.take_due(&now) {
                            runs.fetch_add(1, Ordering::SeqCst);
                            board.finish(claim, result("ran"));
                        }
                    })
                })
                .collect();
            for thread in threads {
                thread.join().unwrap();
            }
            // Due at 9:00, 9:10, 9:20 and so on; in between no one runs it.
            let expected = usize::from(round % 2 == 0);
            assert_eq!(runs.load(Ordering::SeqCst), expected, "round {round}");
        }
    }

    #[test]
    fn an_edit_in_the_app_and_a_result_from_the_command_line_both_stay() {
        let (_folder, file) = folder();
        let events = Events::keeping_startup_log();
        let app = TaskBoard::open(file.clone(), events.clone());
        let saved = app.save(every(60), &at(9, 30)).unwrap();
        let cli = TaskBoard::borrow(file, Events::default()).unwrap();

        // The command line runs the task while the app has it open in its editor.
        let Claimed::Run(claim) = run_now(&cli, &saved.task.id) else {
            panic!("the task is not running");
        };
        assert!(app.refresh());
        assert!(app.list()[0].running);
        cli.finish(claim, result("7 files uploaded"));

        // The app saves its edit without having looked at the file again.
        let edited = ScheduledTask {
            name: "Renamed in the app".into(),
            ..saved.task.clone()
        };
        let resaved = app.save(edited, &at(9, 30)).unwrap();
        assert_eq!(resaved.task.last_run.unwrap().message, "7 files uploaded");
        assert!(!resaved.running);
        let notices = events.take_startup_log();
        assert!(
            notices.iter().any(|notice| notice.message
                == "Scheduled task \"Renamed in the app\" was run by another Poros process: 7 files uploaded"),
            "{notices:?}"
        );

        // And the command line, which still has the old name in memory, loses neither.
        let Claimed::Run(claim) = run_now(&cli, &saved.task.id) else {
            panic!("the task is not running");
        };
        assert_eq!(claim.task.name, "Renamed in the app");
        cli.finish(claim, result("nothing new"));
        assert!(app.refresh());
        let view = &app.list()[0];
        assert_eq!(view.task.name, "Renamed in the app");
        assert_eq!(view.task.last_run.as_ref().unwrap().message, "nothing new");
        // Nothing changed since: looking again is free and says so.
        assert!(!app.refresh());
    }

    #[test]
    fn a_run_that_dies_with_its_process_stops_counting_as_running_and_shows_a_failure() {
        let (_folder, file) = folder();
        let events = Events::keeping_startup_log();
        let app = TaskBoard::open(file.clone(), events.clone());
        let mut task = app.save(every(60), &at(9, 30)).unwrap().task;
        task.last_run = Some(result("the night before"));
        let saved = app.change(|tasks, _| Ok((task::put(tasks, task), Changed::Tasks)));
        let id = saved.unwrap().id;
        let cli = TaskBoard::borrow(file.clone(), Events::default()).unwrap();
        let Claimed::Run(claim) = run_now(&cli, &id) else {
            panic!("the task is not running");
        };
        assert!(app.refresh());
        assert!(app.list()[0].running);
        // What the run leaves should it never end is written as it begins, and is not taken
        // for a result while it runs.
        let under_way = app.list()[0].task.last_run.clone().unwrap();
        assert!(under_way.under_way && !under_way.succeeded);
        assert_eq!(under_way.message, task::CUT_SHORT);

        // The process is killed: its lock goes, and nothing more is written.
        drop(claim);
        assert!(app.refresh());
        let view = app.list()[0].clone();
        assert!(!view.running);
        assert_eq!(view.task.last_run.as_ref(), Some(&under_way));
        // A process that comes later sees the failure, not the result of the night before.
        let later = TaskBoard::borrow(file, Events::default()).unwrap();
        assert_eq!(later.list()[0].task.last_run.as_ref(), Some(&under_way));
        assert!(!app.refresh());

        // Said once, when the run ended, and not while it ran.
        let notices = events.take_startup_log();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert_eq!(
            notices[0].message,
            format!(
                "Scheduled task \"{}\" stopped before it finished: the Poros process running it ended",
                view.task.name
            )
        );
    }

    #[test]
    fn a_run_that_ends_replaces_what_was_written_as_it_began() {
        let (_folder, board) = board();
        let saved = board.save(every(60), &at(9, 30)).unwrap();
        let claims = board.take_due(&at(10, 0));
        assert_eq!(claims.len(), 1);
        let begun = board.list()[0].task.last_run.clone().unwrap();
        assert!(begun.under_way);
        assert_eq!(begun.started, at(10, 0).timestamp());
        let claim = claims.into_iter().next().unwrap();
        board.finish(claim, result("12 files uploaded"));
        let other = TaskBoard::borrow(board.file.clone(), Events::default()).unwrap();
        let ended = other.list()[0].task.last_run.clone().unwrap();
        assert_eq!(ended, result("12 files uploaded"));
        assert!(!ended.under_way);
        assert_eq!(other.list()[0].task.id, saved.task.id);
    }

    #[test]
    fn a_task_deleted_while_it_runs_ends_without_coming_back() {
        let (folder, board) = board();
        let saved = board.save(every(60), &at(9, 30)).unwrap();
        let Claimed::Run(claim) = run_now(&board, &saved.task.id) else {
            panic!("the task is not running");
        };
        board.delete(&saved.task.id).unwrap();
        let lock_files = || {
            std::fs::read_dir(folder.path().join("schedules.runs"))
                .unwrap()
                .count()
        };
        assert_eq!(lock_files(), 1);
        board.finish(claim, result("too late"));
        assert!(board.list().is_empty());
        assert_eq!(lock_files(), 0);
    }

    #[test]
    fn tasks_are_turned_on_and_off_by_name() {
        let (_folder, board) = board();
        board.save(every(60), &at(9, 30)).unwrap();
        let off = board
            .set_enabled("nightly backup", false, &at(9, 30))
            .unwrap();
        assert!(!off.task.enabled);
        assert!(board.take_due(&at(10, 0)).is_empty());
        let on = board
            .set_enabled("Nightly backup", true, &at(11, 30))
            .unwrap();
        assert_eq!(on.task.next_run, Some(at(12, 0).timestamp()));
        assert!(board.set_enabled("Weekly", true, &at(11, 30)).is_err());

        let once = board
            .save(
                ScheduledTask {
                    name: "Once".into(),
                    enabled: false,
                    ..task(Trigger::Once {
                        at: at(8, 0).timestamp(),
                    })
                },
                &at(9, 0),
            )
            .unwrap();
        let error = board
            .set_enabled(&once.task.id, true, &at(9, 0))
            .unwrap_err();
        assert!(error.message.contains("has passed"), "{}", error.message);
    }

    #[test]
    fn the_next_look_is_when_a_task_is_due_and_never_far_off() {
        let (_folder, board) = board();
        assert_eq!(board.next_wake(at(9, 0).timestamp()), TICK);
        board.save(every(60), &at(9, 30)).unwrap();
        assert_eq!(board.next_wake(at(9, 31).timestamp()), TICK);
        assert_eq!(
            board.next_wake(at(10, 0).timestamp() - 2),
            Duration::from_secs(2)
        );
        assert_eq!(
            board.next_wake(at(10, 0).timestamp() + 5),
            Duration::from_millis(200)
        );
    }
}
