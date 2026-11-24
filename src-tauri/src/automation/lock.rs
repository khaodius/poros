//! The locks every Poros process shares when it works with scheduled tasks: the app, a
//! `poros-cli schedule daemon` and any number of `poros-cli schedule run` started by cron may
//! be alive at once.
//!
//! Both kinds are operating system file locks (`flock` on Linux, `LockFileEx` on Windows) on
//! files of their own beside `schedules.json`, so they end with the process that holds them
//! however it ends, and a crash never leaves a task marked as running:
//!
//! - `schedules.lock` is held for the moment it takes to read `schedules.json`, change it and
//!   write it back, so no process writes over what another just wrote. The file itself holds
//!   a mark that is new after every change, which is how a process learns without taking the
//!   lock that another one changed a task or began or ended a run.
//! - `schedules.runs/<task>.lock` is held for as long as a task runs, so it never runs twice
//!   at once. It is only ever taken, tested and deleted while `schedules.lock` is held, so a
//!   test never makes a task look busy to a process about to run it.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs4::TryLockError;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult, ErrorKind};

/// How long to wait for another process to finish writing `schedules.json`, which takes it
/// milliseconds.
const STORE_WAIT: Duration = Duration::from_secs(10);
const STORE_POLL: Duration = Duration::from_millis(15);

/// Where the lock files of one `schedules.json` are.
#[derive(Debug, Clone)]
pub struct Locks {
    store_file: PathBuf,
    runs_folder: PathBuf,
}

/// `schedules.json` may be read and written while this is held.
#[derive(Debug)]
pub struct StoreLock {
    file: File,
}

/// A task runs while this is held.
#[derive(Debug)]
pub struct RunLock {
    file: File,
}

fn open(path: &Path) -> AppResult<File> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| AppError::from(error).with_path(folder.display().to_string()))?;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| AppError::from(error).with_path(path.display().to_string()))
}

/// Named in full: `File` has methods of the same names from Rust 1.89 on, which is newer than
/// the oldest compiler Poros builds with.
fn try_lock(file: &File) -> Result<bool, std::io::Error> {
    match fs4::FileExt::try_lock(file) {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn unlock(file: &File) {
    // Closing the file releases the lock as well, so a failure here changes nothing.
    let _ = fs4::FileExt::unlock(file);
}

impl StoreLock {
    /// The mark the last change left. Read through the lock's own handle: on Windows no other
    /// handle may read a locked file, not even one of this process.
    pub fn mark(&self) -> Option<String> {
        let mut file = &self.file;
        let mut mark = String::new();
        file.seek(SeekFrom::Start(0)).ok()?;
        file.read_to_string(&mut mark).ok()?;
        Some(mark)
    }

    /// Leaves a mark no change left before, for the processes that watch the file.
    pub fn leave_mark(&self) -> AppResult<()> {
        let mut file = &self.file;
        let mark = uuid::Uuid::new_v4().to_string();
        self.file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(mark.as_bytes())?;
        Ok(())
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

impl Locks {
    /// For the tasks kept in `file`: `schedules.json` gets `schedules.lock` and the folder
    /// `schedules.runs` beside it.
    pub fn beside(file: &Path) -> Self {
        Self {
            store_file: file.with_extension("lock"),
            runs_folder: file.with_extension("runs"),
        }
    }

    /// Waits for the lock on the tasks file, which other processes hold only while they write.
    pub fn store(&self) -> AppResult<StoreLock> {
        let file = open(&self.store_file)?;
        let started = Instant::now();
        loop {
            match try_lock(&file) {
                Ok(true) => return Ok(StoreLock { file }),
                Ok(false) if started.elapsed() < STORE_WAIT => std::thread::sleep(STORE_POLL),
                Ok(false) => {
                    return Err(AppError::new(
                        ErrorKind::Io,
                        format!(
                            "The scheduled tasks are in use by another Poros process, which holds {} and does not let go",
                            self.store_file.display()
                        ),
                    ))
                }
                Err(error) => {
                    return Err(
                        AppError::from(error).with_path(self.store_file.display().to_string())
                    )
                }
            }
        }
    }

    /// The mark the last change left, read without the lock. `None` when there is none yet, or
    /// while another process holds the lock where that keeps the file from being read.
    pub fn mark(&self) -> Option<String> {
        std::fs::read_to_string(&self.store_file).ok()
    }

    /// Task ids come from a file anyone can edit, so they never name a file themselves.
    fn run_file(&self, task_id: &str) -> PathBuf {
        let name: String = Sha256::digest(task_id.as_bytes())[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.runs_folder.join(format!("{name}.lock"))
    }

    /// Takes the lock a task runs under, or `None` when a run of it holds the lock, in this
    /// process or another.
    pub fn try_run(&self, _held: &StoreLock, task_id: &str) -> AppResult<Option<RunLock>> {
        let path = self.run_file(task_id);
        let file = open(&path)?;
        match try_lock(&file) {
            Ok(true) => Ok(Some(RunLock { file })),
            Ok(false) => Ok(None),
            Err(error) => Err(AppError::from(error).with_path(path.display().to_string())),
        }
    }

    /// Whether a run of the task holds its lock right now.
    pub fn is_running(&self, _held: &StoreLock, task_id: &str) -> bool {
        // A task that never ran has no lock file, and looking must not create one.
        let Ok(file) = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.run_file(task_id))
        else {
            return false;
        };
        match try_lock(&file) {
            Ok(true) => {
                unlock(&file);
                false
            }
            Ok(false) => true,
            Err(_) => false,
        }
    }

    /// Removes the lock file of a task that was deleted, unless a run of it still holds it.
    pub fn forget(&self, _held: &StoreLock, task_id: &str) {
        let path = self.run_file(task_id);
        let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) else {
            return;
        };
        if try_lock(&file).is_ok_and(|locked| locked) {
            let _ = std::fs::remove_file(&path);
            unlock(&file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    fn locks() -> (tempfile::TempDir, Locks) {
        let folder = tempfile::tempdir().unwrap();
        let locks = Locks::beside(&folder.path().join("schedules.json"));
        (folder, locks)
    }

    #[test]
    fn lock_files_sit_beside_the_tasks_and_never_take_a_task_id_as_a_path() {
        let (folder, locks) = locks();
        let store = locks.store().unwrap();
        let held = locks.try_run(&store, "../../etc/passwd").unwrap().unwrap();
        assert!(folder.path().join("schedules.lock").is_file());
        let run_files: Vec<_> = std::fs::read_dir(folder.path().join("schedules.runs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(run_files.len(), 1);
        assert!(run_files[0].ends_with(".lock") && run_files[0].len() == 32 + 5);
        drop(held);
    }

    #[test]
    fn a_task_runs_once_at_a_time_and_again_when_its_run_ends() {
        let (_folder, locks) = locks();
        let store = locks.store().unwrap();
        assert!(!locks.is_running(&store, "nightly"));
        let first = locks.try_run(&store, "nightly").unwrap().unwrap();
        assert!(locks.is_running(&store, "nightly"));
        assert!(locks.try_run(&store, "nightly").unwrap().is_none());
        // Another task is not held up.
        assert!(locks.try_run(&store, "hourly").unwrap().is_some());

        drop(first);
        assert!(!locks.is_running(&store, "nightly"));
        // Looking did not take the lock away from the next run.
        assert!(locks.try_run(&store, "nightly").unwrap().is_some());
    }

    #[test]
    fn of_many_runners_racing_for_one_task_exactly_one_runs() {
        let (_folder, locks) = locks();
        let runners = 8;
        let rounds = 25;
        for _ in 0..rounds {
            let start = Arc::new(Barrier::new(runners));
            let winners = Arc::new(AtomicUsize::new(0));
            let release = Arc::new(Barrier::new(runners));
            let threads: Vec<_> = (0..runners)
                .map(|_| {
                    // Each runner opens the lock files itself, as a separate process would.
                    let (locks, start, winners, release) = (
                        locks.clone(),
                        start.clone(),
                        winners.clone(),
                        release.clone(),
                    );
                    std::thread::spawn(move || {
                        start.wait();
                        let run = {
                            let store = locks.store().unwrap();
                            locks.try_run(&store, "nightly").unwrap()
                        };
                        if run.is_some() {
                            winners.fetch_add(1, Ordering::SeqCst);
                        }
                        // The winner keeps running until every runner has tried.
                        release.wait();
                        drop(run);
                    })
                })
                .collect();
            for thread in threads {
                thread.join().unwrap();
            }
            assert_eq!(winners.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn a_deleted_task_leaves_no_lock_file_unless_it_still_runs() {
        let (folder, locks) = locks();
        let runs = folder.path().join("schedules.runs");
        let count = || std::fs::read_dir(&runs).unwrap().count();
        let store = locks.store().unwrap();
        let running = locks.try_run(&store, "nightly").unwrap().unwrap();
        locks.forget(&store, "nightly");
        assert_eq!(count(), 1);
        drop(running);
        locks.forget(&store, "nightly");
        assert_eq!(count(), 0);
        locks.forget(&store, "never ran");
    }

    #[test]
    fn every_change_leaves_a_mark_of_its_own() {
        let (_folder, locks) = locks();
        assert_eq!(locks.mark(), None);
        let store = locks.store().unwrap();
        assert_eq!(store.mark().as_deref(), Some(""));
        store.leave_mark().unwrap();
        let first = store.mark().unwrap();
        assert_eq!(first.len(), 36);
        store.leave_mark().unwrap();
        let second = store.mark().unwrap();
        assert_ne!(first, second);
        drop(store);
        // Seen without the lock, and by the next process to take it.
        assert_eq!(locks.mark(), Some(second.clone()));
        assert_eq!(locks.store().unwrap().mark(), Some(second));
    }

    #[test]
    fn the_store_lock_is_handed_from_one_writer_to_the_next() {
        let (_folder, locks) = locks();
        let first = locks.store().unwrap();
        let waiting = {
            let locks = locks.clone();
            std::thread::spawn(move || locks.store().map(|_| ()))
        };
        std::thread::sleep(Duration::from_millis(60));
        assert!(!waiting.is_finished());
        drop(first);
        waiting.join().unwrap().unwrap();
    }
}
