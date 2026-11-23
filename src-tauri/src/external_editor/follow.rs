//! The task that follows one edit: waits for the download, opens the program, then looks at
//! the working copy on a timer and uploads it each time it has been saved, until the edit ends.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::conflict::{server_change, upload_decision, Resolution, ServerState, UploadDecision};
use super::edit::{Edit, Launch, Phase};
use super::watch::{ChangeDetector, Fingerprint, Observation};
use super::{command_line, executable, job_options, journal, launch, Inner, Notice};
use crate::error::{AppError, AppResult};
use crate::events::LogLevel;
use crate::remote_path;
use crate::transfer::{Direction, ExistsAction, FileTransfer, JobId, JobProgress, JobState};

/// Progress of a transfer goes to the page this often at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);

/// A program started on the working copy.
struct Program {
    child: tokio::process::Child,
    started: Instant,
}

enum EndReason {
    /// The user stopped the edit, or took its download off the queue.
    Stopped,
    /// The program closed and the server has everything.
    Finished,
    VaultLocked,
    /// The file could not be downloaded or opened.
    OpenFailed(String),
}

struct Follower {
    inner: Arc<Inner>,
    edit: Arc<Edit>,
    /// Away only while a look at the file runs on another thread.
    detector: Option<ChangeDetector>,
    program: Option<Program>,
    progress_sent: Instant,
}

pub(super) async fn follow(inner: Arc<Inner>, edit: Arc<Edit>) {
    let mut follower = Follower {
        inner,
        edit,
        detector: None,
        program: None,
        progress_sent: Instant::now(),
    };
    let reason = follower.run().await;
    follower.end(reason);
}

impl Follower {
    async fn run(&mut self) -> EndReason {
        let download = match self.edit.core.lock().unwrap().phase {
            Phase::Downloading { job } => Some(job),
            _ => None,
        };
        match download {
            Some(job) => {
                if let Err(reason) = self.download(job).await {
                    return reason;
                }
            }
            None => self.detector = Some(self.new_detector()),
        }
        loop {
            let wait = match self.edit.core.lock().unwrap().phase {
                Phase::Uploading { .. } => self.inner.pace.job_poll,
                _ => self.inner.pace.watch.poll,
            };
            tokio::select! {
                _ = self.edit.cancel.cancelled() => return EndReason::Stopped,
                _ = self.edit.wake.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
            if let Some(reason) = self.step().await {
                return reason;
            }
        }
    }

    fn new_detector(&self) -> ChangeDetector {
        let synced = self.edit.core.lock().unwrap().synced.clone();
        ChangeDetector::new(
            self.edit.local_path.clone(),
            self.inner.pace.watch.settle,
            synced,
        )
    }

    /// Waits for the working copy to arrive, then opens it.
    async fn download(&mut self, job: JobId) -> Result<(), EndReason> {
        loop {
            tokio::select! {
                _ = self.edit.cancel.cancelled() => return Err(EndReason::Stopped),
                _ = tokio::time::sleep(self.inner.pace.job_poll) => {}
            }
            let Some(progress) = self.inner.files.job(job) else {
                return Err(EndReason::Stopped);
            };
            match progress.state {
                JobState::Done => break,
                JobState::Failed | JobState::Skipped => {
                    return Err(EndReason::OpenFailed(
                        progress
                            .error
                            .unwrap_or_else(|| "The download failed".to_string()),
                    ));
                }
                _ => self.show_progress(&progress),
            }
        }
        if !self.inner.files.keeps_completed() {
            self.inner.files.remove(&[job]);
        }

        let path = self.edit.local_path.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            journal::make_private_file(&path)?;
            let content = Fingerprint::of(&path)?;
            Ok::<_, AppError>((content, executable::has_executable_content(&path)))
        })
        .await
        .map_err(AppError::from)
        .and_then(|prepared| prepared);
        let (content, is_program) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return Err(EndReason::OpenFailed(error.message)),
        };
        let (blocked, name, record) = {
            let mut core = self.edit.core.lock().unwrap();
            core.synced = Some(content.clone());
            core.phase = Phase::Watching;
            core.transferred = 0;
            core.size = content.size;
            core.stalled = None;
            core.lifetime.changed(Instant::now());
            core.runs |= is_program;
            // Held back until the user agrees, and names an editor: the system would run it.
            let by_default = matches!(core.launch, None | Some(Launch::Default));
            core.blocked = core.runs && (!core.confirmed_executable || by_default);
            (
                core.blocked,
                core.name.clone(),
                core.record(&self.edit.id, &self.edit.local_name),
            )
        };
        self.inner.keep_record(&record);
        self.detector = Some(self.new_detector());
        if blocked {
            self.inner.emit(Some(Notice::Blocked {
                edit_id: self.edit.id.clone(),
                name,
            }));
            return Ok(());
        }
        match self.open_program().await {
            Ok(()) => {
                self.inner.emit(None);
                Ok(())
            }
            Err(error) => Err(EndReason::OpenFailed(error.message)),
        }
    }

    /// Opens the working copy with what the edit was last opened with.
    async fn open_program(&mut self) -> AppResult<()> {
        let (launch, runs, name) = {
            let core = self.edit.core.lock().unwrap();
            let launch = core.launch.clone().unwrap_or(Launch::Default);
            (launch, core.runs, core.name.clone())
        };
        if runs && launch == Launch::Default {
            return Err(AppError::invalid(format!(
                "{name} is a program or a script. Pick an editor to open it with."
            )));
        }
        // On Windows, SmartScreen and Office's Protected View then treat it as from the
        // internet. A program that saves by replacing the file drops the mark, so it is
        // written again each time the copy is opened.
        if let Err(error) = journal::mark_from_elsewhere(&self.edit.local_path) {
            log::warn!(
                "Could not mark the working copy of {name} as from another computer: {error}"
            );
        }
        match &launch {
            Launch::Default => {
                launch::open_with_default(&self.edit.local_path).await?;
                self.program = None;
            }
            Launch::Program {
                command, arguments, ..
            } => {
                let file = self.edit.local_path.to_string_lossy();
                let invocation = command_line::invocation(command, arguments, &file)?;
                self.program = Some(Program {
                    child: launch::start(&invocation)?,
                    started: Instant::now(),
                });
            }
        }
        let mut core = self.edit.core.lock().unwrap();
        core.launch = Some(launch);
        core.lifetime.program_started();
        Ok(())
    }

    /// One round: what the user asked for, then the program, then the file or its upload.
    async fn step(&mut self) -> Option<EndReason> {
        let (vault_locked, reopen) = {
            let mut core = self.edit.core.lock().unwrap();
            (
                self.edit.in_vault && core.route.vault_locked(),
                core.requests.open.take(),
            )
        };
        if vault_locked {
            return Some(EndReason::VaultLocked);
        }
        if let Some(launch) = reopen {
            self.reopen(launch).await;
        }
        self.notice_program_end();
        let phase = self.edit.core.lock().unwrap().phase.clone();
        match phase {
            Phase::Downloading { .. } => None,
            Phase::Uploading { job, content } => {
                self.follow_upload(job, content).await;
                None
            }
            phase => self.watch(phase).await,
        }
    }

    async fn reopen(&mut self, launch: Launch) {
        let before = {
            let mut core = self.edit.core.lock().unwrap();
            let before = (core.launch.replace(launch), core.blocked);
            core.blocked = false;
            before
        };
        if let Err(error) = self.open_program().await {
            // Nothing was opened, so the edit stays as it was, held back if it was.
            {
                let mut core = self.edit.core.lock().unwrap();
                (core.launch, core.blocked) = before;
            }
            let (name, session_id) = self.name_and_session();
            self.inner.events.log(
                LogLevel::Error,
                Some(&session_id),
                format!("Could not open {name}: {}", error.message),
            );
            self.inner.emit(Some(Notice::OpenFailed {
                name,
                message: error.message,
            }));
        } else {
            self.inner.emit(None);
        }
    }

    /// A program that ran for a while and ended has closed the file. One that ended at once
    /// handed the file to a window already open, which nothing tells the end of.
    fn notice_program_end(&mut self) {
        let Some(program) = &mut self.program else {
            return;
        };
        match program.child.try_wait() {
            Ok(None) => {}
            Ok(Some(_)) => {
                if program.started.elapsed() >= self.inner.pace.program_hand_over {
                    let mut core = self.edit.core.lock().unwrap();
                    core.lifetime.program_closed(Instant::now());
                }
                self.program = None;
            }
            Err(_) => self.program = None,
        }
    }

    async fn look(&mut self) -> Observation {
        let Some(mut detector) = self.detector.take() else {
            return Observation::Settling;
        };
        let looked = tokio::task::spawn_blocking(move || {
            let observation = detector.look(Instant::now());
            (detector, observation)
        })
        .await;
        match looked {
            Ok((detector, observation)) => {
                self.detector = Some(detector);
                observation
            }
            Err(_) => {
                self.detector = Some(self.new_detector());
                Observation::Settling
            }
        }
    }

    async fn watch(&mut self, phase: Phase) -> Option<EndReason> {
        let content = match self.look().await {
            Observation::Settling => return None,
            Observation::Changed(content) => Some(content),
            Observation::Unchanged => {
                self.back_in_step();
                None
            }
            Observation::Missing => None,
        };
        {
            let mut core = self.edit.core.lock().unwrap();
            let now = Instant::now();
            core.looked = Some(now);
            // A change is reported at every look until the server has it: only content not
            // seen before is a save.
            if content
                .as_ref()
                .is_some_and(|content| phase.content() != Some(content))
            {
                core.lifetime.changed(now);
            }
        }
        match content {
            Some(content) => {
                self.saved(phase, content).await;
                None
            }
            None => self.ended_by_now(),
        }
    }

    /// The working copy holds what the server has, so nothing waits to be uploaded any more.
    fn back_in_step(&self) {
        let was_waiting = {
            let mut core = self.edit.core.lock().unwrap();
            core.requests.upload = false;
            core.requests.answer = None;
            let was_waiting = !matches!(core.phase, Phase::Watching);
            core.phase = Phase::Watching;
            was_waiting
        };
        if was_waiting {
            self.inner.emit(None);
        }
    }

    /// Whether the edit is over: the program closed long enough ago, or the file was only
    /// here to be uploaded, and the server has everything.
    fn ended_by_now(&self) -> Option<EndReason> {
        let keep = if self.edit.in_vault {
            Duration::ZERO
        } else {
            let minutes = self.inner.settings.lock().unwrap().keep_minutes;
            Duration::from_secs(u64::from(minutes) * 60)
        };
        let core = self.edit.core.lock().unwrap();
        if !matches!(core.phase, Phase::Watching) || core.blocked {
            return None;
        }
        let over = core.ends_when_clean || core.lifetime.is_over(keep, Instant::now());
        over.then_some(EndReason::Finished)
    }

    /// The working copy has come to rest with content the server does not have.
    async fn saved(&mut self, phase: Phase, content: Fingerprint) {
        match phase {
            Phase::Watching => self.new_save(content, None).await,
            Phase::Changed { content: held } => {
                if held != content {
                    self.new_save(content, None).await;
                } else if self.upload_requested() {
                    self.begin_upload(content, None).await;
                }
            }
            Phase::Conflict {
                content: asked,
                server,
                ..
            } => {
                if let Some(answer) = self.take_answer() {
                    self.begin_upload(content, Some(answer)).await;
                } else if asked != content {
                    let saved_at = saved_at(&self.edit.local_path);
                    self.edit.core.lock().unwrap().phase = Phase::Conflict {
                        content,
                        server,
                        saved_at,
                    };
                    self.inner.emit(None);
                }
            }
            Phase::WaitingForServer {
                content: waiting,
                retry_at,
                ..
            } => {
                if waiting != content {
                    // Not the content that was let go: it is asked about like any new save.
                    let answer = self.take_answer();
                    self.new_save(content, answer).await;
                } else if Instant::now() >= retry_at || self.upload_requested() {
                    let answer = self.take_answer();
                    self.begin_upload(content, answer).await;
                }
            }
            Phase::Failed {
                job,
                content: failed,
                ..
            } => self.after_failure(job, failed, content).await,
            Phase::Downloading { .. } | Phase::Uploading { .. } => {}
        }
    }

    /// Content not seen before: held until the user asks for it to go, unless every save goes.
    /// `answer` is what the user said about the server's copy, should the upload go now.
    async fn new_save(&mut self, content: Fingerprint, answer: Option<Resolution>) {
        let asks = !self.inner.settings.lock().unwrap().upload_on_save;
        let held = {
            let mut core = self.edit.core.lock().unwrap();
            let held = asks && !core.requests.upload && !core.ends_when_clean;
            if held {
                core.phase = Phase::Changed {
                    content: content.clone(),
                };
            }
            held
        };
        if held {
            let (name, _) = self.name_and_session();
            self.inner.emit(Some(Notice::Saved {
                edit_id: self.edit.id.clone(),
                name,
            }));
        } else {
            self.begin_upload(content, answer).await;
        }
    }

    /// A failed upload goes again when the user says so, or when its job is tried again from the
    /// transfer queue. A save after it is a new save, asked about when saves wait to be asked for.
    async fn after_failure(
        &mut self,
        job: Option<JobId>,
        failed: Fingerprint,
        content: Fingerprint,
    ) {
        let retried = job.and_then(|job| Some((job, self.inner.files.job(job)?)));
        if let Some((job, progress)) = retried {
            match progress.state {
                JobState::Done => {
                    self.finish_upload(job, failed, progress.target).await;
                    return;
                }
                JobState::Failed | JobState::Skipped => {}
                _ => {
                    self.edit.core.lock().unwrap().phase = Phase::Uploading {
                        job,
                        content: failed,
                    };
                    self.inner.emit(None);
                    return;
                }
            }
        }
        let requested = self.upload_requested();
        if failed != content || requested {
            if let Some(job) = job {
                self.inner.files.remove(&[job]);
            }
            if requested {
                self.begin_upload(content, None).await;
            } else {
                self.new_save(content, None).await;
            }
        }
    }

    fn upload_requested(&self) -> bool {
        self.edit.core.lock().unwrap().requests.upload
    }

    fn take_answer(&self) -> Option<Resolution> {
        self.edit.core.lock().unwrap().requests.answer.take()
    }

    fn name_and_session(&self) -> (String, String) {
        let core = self.edit.core.lock().unwrap();
        (core.name.clone(), core.route.session_id().to_string())
    }

    /// Checks the server's copy against the one Poros last saw, then queues the upload, asks
    /// the user, or waits for the server.
    async fn begin_upload(&mut self, content: Fingerprint, answer: Option<Resolution>) {
        let (route, remote_path, baseline) = {
            let mut core = self.edit.core.lock().unwrap();
            core.requests.upload = false;
            (core.route.clone(), core.remote_path.clone(), core.baseline)
        };
        let route = self.inner.files.current_route(&route).await;
        let found = match self.inner.files.stat(&route, &remote_path).await {
            Ok(found) => found,
            Err(error) => {
                self.could_not_ask(content, answer, error);
                return;
            }
        };
        let server = match found {
            Some(found) if found.is_dir => {
                let error = AppError::invalid("A folder has taken its place on the server");
                self.could_not_ask(content, None, error);
                return;
            }
            found => found.as_ref().map(ServerState::of),
        };
        let exists = match upload_decision(server_change(&baseline, server.as_ref()), answer) {
            UploadDecision::Replace => ExistsAction::Overwrite,
            UploadDecision::BesideServerCopy => ExistsAction::Rename,
            UploadDecision::Hold => {
                self.edit.core.lock().unwrap().phase = Phase::Changed { content };
                self.inner.emit(None);
                return;
            }
            UploadDecision::Ask => {
                let saved_at = saved_at(&self.edit.local_path);
                let name = {
                    let mut core = self.edit.core.lock().unwrap();
                    core.phase = Phase::Conflict {
                        content,
                        server,
                        saved_at,
                    };
                    core.name.clone()
                };
                self.inner.events.log(
                    LogLevel::Warn,
                    Some(route.session_id()),
                    format!(
                        "{name} changed on the server while it was open in another program; not uploaded yet"
                    ),
                );
                self.inner.emit(Some(Notice::Conflict {
                    edit_id: self.edit.id.clone(),
                    name,
                }));
                return;
            }
        };
        let transfer = FileTransfer {
            direction: Direction::Upload,
            name: remote_path::file_name(&remote_path).to_string(),
            source: self.edit.local_path.to_string_lossy().into_owned(),
            target: remote_path.clone(),
            target_directory: remote_path::parent(&remote_path).unwrap_or_else(|| "/".to_string()),
            size: content.size,
            resolution: exists,
            options: Some(job_options()),
        };
        let job = self.inner.files.enqueue(&route, transfer);
        {
            let mut core = self.edit.core.lock().unwrap();
            core.route = route;
            core.transferred = 0;
            core.size = content.size;
            core.stalled = None;
            core.phase = Phase::Uploading { job, content };
        }
        self.inner.emit(None);
    }

    /// The server could not be asked how its copy stands. A lost connection is waited out, as
    /// uploads in the queue are; anything else is a failure the user can try again.
    fn could_not_ask(&self, content: Fingerprint, answer: Option<Resolution>, error: AppError) {
        let (name, session_id) = self.name_and_session();
        if error.is_connection_lost() {
            let was_waiting = {
                let mut core = self.edit.core.lock().unwrap();
                let was_waiting = matches!(core.phase, Phase::WaitingForServer { .. });
                core.requests.answer = answer;
                core.phase = Phase::WaitingForServer {
                    content,
                    message: format!("Waiting for the server: {}", error.message),
                    retry_at: Instant::now() + self.inner.pace.server_retry,
                };
                was_waiting
            };
            if !was_waiting {
                self.inner.events.log(
                    LogLevel::Warn,
                    Some(&session_id),
                    format!(
                        "{name} was saved but the server cannot be reached; it is uploaded once it can ({})",
                        error.message
                    ),
                );
                self.inner.emit(None);
            }
            return;
        }
        self.failed(None, content, error.message);
    }

    fn failed(&self, job: Option<JobId>, content: Fingerprint, message: String) {
        let (name, session_id) = self.name_and_session();
        self.edit.core.lock().unwrap().phase = Phase::Failed {
            job,
            content,
            message: message.clone(),
        };
        self.inner.events.log(
            LogLevel::Error,
            Some(&session_id),
            format!("Could not upload {name}: {message}. The saved file is kept."),
        );
        self.inner.emit(Some(Notice::UploadFailed {
            edit_id: self.edit.id.clone(),
            name,
            message,
        }));
    }

    async fn follow_upload(&mut self, job: JobId, content: Fingerprint) {
        let Some(progress) = self.inner.files.job(job) else {
            // Taken off the queue by the user: the save stays until it is asked for again.
            self.edit.core.lock().unwrap().phase = Phase::Changed { content };
            self.inner.emit(None);
            return;
        };
        match progress.state {
            JobState::Done => self.finish_upload(job, content, progress.target).await,
            JobState::Failed | JobState::Skipped => {
                let message = progress
                    .error
                    .unwrap_or_else(|| "The upload failed".to_string());
                self.failed(Some(job), content, message);
            }
            _ => self.show_progress(&progress),
        }
    }

    /// Shows how far a transfer is, and why it does not move when it cannot.
    fn show_progress(&mut self, progress: &JobProgress) {
        let stalled = if progress.reconnecting {
            Some("Waiting for the server to be reachable again".to_string())
        } else if progress.state == JobState::Paused || self.inner.files.is_paused() {
            Some("The transfer queue is paused".to_string())
        } else {
            None
        };
        let (moved, stall_changed) = {
            let mut core = self.edit.core.lock().unwrap();
            let moved = core.transferred != progress.transferred || core.size != progress.size;
            let stall_changed = core.stalled != stalled;
            core.transferred = progress.transferred;
            core.size = progress.size;
            core.stalled = stalled;
            (moved, stall_changed)
        };
        if stall_changed || (moved && self.progress_sent.elapsed() >= PROGRESS_EVERY) {
            self.progress_sent = Instant::now();
            self.inner.emit(None);
        }
    }

    /// The server has `content` now, at `target`: the path uploaded to, or the new name a file
    /// kept beside the server's copy was given.
    async fn finish_upload(&mut self, job: JobId, content: Fingerprint, target: String) {
        if !self.inner.files.keeps_completed() {
            self.inner.files.remove(&[job]);
        }
        let route = self.edit.core.lock().unwrap().route.clone();
        let baseline = match self.inner.files.stat(&route, &target).await {
            Ok(Some(found)) if !found.is_dir => ServerState::of(&found),
            // Without the server's word on the new file, it is compared by size from here on.
            _ => ServerState {
                size: content.size,
                modified: None,
            },
        };
        if let Some(detector) = &mut self.detector {
            detector.mark_synced(content.clone());
        }
        let (name, renamed_to, record) = {
            let mut core = self.edit.core.lock().unwrap();
            let name = core.name.clone();
            let renamed_to =
                (target != core.remote_path).then(|| remote_path::file_name(&target).to_string());
            if let Some(new_name) = &renamed_to {
                core.name = new_name.clone();
                core.remote_path = target;
            }
            core.baseline = baseline;
            core.synced = Some(content);
            core.uploads += 1;
            core.last_upload = Some(now_millis());
            core.lifetime.uploaded(Instant::now());
            core.transferred = 0;
            core.stalled = None;
            core.phase = Phase::Watching;
            (
                name,
                renamed_to,
                core.record(&self.edit.id, &self.edit.local_name),
            )
        };
        self.inner.keep_record(&record);
        let message = match &renamed_to {
            Some(new_name) => format!(
                "Uploaded {name} to {} as {new_name}, beside the copy that changed there",
                route.label()
            ),
            None => format!("Uploaded {name} to {}", route.label()),
        };
        self.inner
            .events
            .log(LogLevel::Info, Some(route.session_id()), message);
        self.inner.emit(Some(Notice::Uploaded {
            edit_id: self.edit.id.clone(),
            name,
            renamed_to,
        }));
    }

    /// Takes the edit off the list and clears its working copy away.
    fn end(&mut self, reason: EndReason) {
        let (abandoned, job, name, session_id, unsaved) = {
            let core = self.edit.core.lock().unwrap();
            let job = match core.phase {
                Phase::Downloading { job } | Phase::Uploading { job, .. } => Some(job),
                Phase::Failed { job, .. } => job,
                _ => None,
            };
            (
                core.abandoned,
                job,
                core.name.clone(),
                core.route.session_id().to_string(),
                core.has_unsaved_changes(),
            )
        };
        if abandoned {
            return;
        }
        if let Some(job) = job {
            self.inner.files.remove(&[job]);
        }
        // The list tells the page the edit is over, so the working copy goes first.
        journal::discard(&self.inner.root, &self.edit.id, self.edit.in_vault);
        self.inner
            .edits
            .lock()
            .unwrap()
            .retain(|edit| edit.id != self.edit.id);
        let notice = match reason {
            EndReason::OpenFailed(message) => {
                self.inner.events.log(
                    LogLevel::Error,
                    Some(&session_id),
                    format!("Could not open {name}: {message}"),
                );
                Some(Notice::OpenFailed { name, message })
            }
            EndReason::VaultLocked if unsaved => {
                let message = format!(
                    "The vault was locked, so the working copy of {name} was wiped with changes that were not uploaded"
                );
                self.inner
                    .events
                    .log(LogLevel::Warn, Some(&session_id), message.clone());
                Some(Notice::Ended { name, message })
            }
            EndReason::VaultLocked | EndReason::Stopped | EndReason::Finished => None,
        };
        self.inner.emit(notice);
    }
}

/// When the file was last written, in milliseconds since the Unix epoch.
fn saved_at(path: &std::path::Path) -> Option<u64> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as u64)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}
