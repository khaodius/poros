//! Server files open in the user's own programs. Poros downloads the file to a working copy
//! in a private folder, opens it, and uploads it again each time the program saves it, after
//! checking that nobody changed the server's copy meanwhile. Downloads and uploads go through
//! the transfer queue, so its limits, checks, retries and waiting for a dropped connection
//! apply. Files on this computer are simply opened where they are.
//!
//! The pieces: `names` makes a server's file name safe to use here, `command_line` and
//! `launch` start programs without a shell, `executable` keeps files that would run from being
//! opened by the system, `watch` notices saves, `conflict` decides what an upload may write
//! over, `journal` keeps the working copies and what a later run needs to recover them, and
//! `follow` is the task that takes one edit through all of it.

pub mod command_line;
pub mod conflict;
mod edit;
pub mod executable;
mod follow;
mod journal;
mod launch;
pub mod names;
pub mod watch;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub use conflict::Resolution;
pub use edit::{ConflictView, EditState, EditView};
pub use watch::Timing;

use conflict::ServerState;
use edit::{Core, Edit, Launch, Phase, Requests};
use journal::{Leftover, ServerIdentity};
use watch::Fingerprint;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::Events;
use crate::protocol::Protocol;
use crate::remote_path;
use crate::settings::EditingSettings;
use crate::transfer::{Direction, ExistsAction, FileQueue, FileTransfer, JobOptions};

/// A program picked for one file is a path of at most this many characters.
const MAX_COMMAND_CHARS: usize = 1024;
/// A working copy bigger than this is not read again as the app closes: one whose size is
/// the server's is taken for changed, and sorted out by the next run.
const MAX_EXIT_CHECK_BYTES: u64 = 64 * 1024 * 1024;
/// How often the page's questions about all edits look whether they can be answered.
const SETTLE_POLL: Duration = Duration::from_millis(50);

/// How often things are looked at, which tests shorten.
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    pub watch: Timing,
    /// How often a download or upload in the queue is asked how far it is.
    pub job_poll: Duration,
    /// How long before a server that could not be reached is asked again.
    pub server_retry: Duration,
    /// A program that ends sooner than this handed the file to a window already open.
    pub program_hand_over: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            watch: Timing::default(),
            job_poll: Duration::from_millis(150),
            server_retry: Duration::from_secs(5),
            program_hand_over: Duration::from_secs(5),
        }
    }
}

/// What to open a file with. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenWith {
    /// The program the system has for the file's type.
    Default,
    /// A program from the settings.
    Program { id: String },
    /// A program picked for this file.
    Custom { command: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    pub session_id: String,
    pub path: String,
    pub with: OpenWith,
    /// The user agreed to open a file of a type that runs, in the program named.
    #[serde(default)]
    pub confirmed_executable: bool,
}

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenOutcome {
    /// The download has been queued; the program opens when it is in.
    Opening { edit: EditView },
    /// The file was open already, and its working copy is opened again.
    AlreadyOpen { edit: EditView },
    /// The file is of a type that runs. Nothing was downloaded: the user has to agree first,
    /// and name an editor to open it in.
    NeedsConfirmation { name: String },
}

/// Something the user is told about, once, by the window in use. Mirrored in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Notice {
    Uploaded {
        edit_id: String,
        name: String,
        /// The name it was uploaded under when kept beside the server's copy.
        renamed_to: Option<String>,
    },
    UploadFailed {
        edit_id: String,
        name: String,
        message: String,
    },
    /// The server's copy changed; the user is asked what to do.
    Conflict { edit_id: String, name: String },
    /// Saved while uploads wait to be asked for.
    Saved { edit_id: String, name: String },
    /// Downloaded, and found to be a program by its content: not opened without the user's
    /// word.
    Blocked { edit_id: String, name: String },
    /// The file could not be downloaded, or its program could not be started.
    OpenFailed { name: String, message: String },
    /// Editing ended with something to say, as when a vault was locked under it.
    Ended { name: String, message: String },
}

/// Every file open in another program, sent whenever one of them changes. Mirrored in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditsUpdate {
    pub edits: Vec<EditView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<Notice>,
}

/// A working copy an earlier run left with changes the server never got. Mirrored in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverView {
    pub id: String,
    pub name: String,
    pub remote_path: String,
    /// The server's label.
    pub server: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_root: Option<String>,
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
}

impl LeftoverView {
    fn of(leftover: &Leftover) -> Self {
        let record = &leftover.record;
        Self {
            id: record.id.clone(),
            name: record.name.clone(),
            remote_path: record.remote_path.clone(),
            server: record.server.label.clone(),
            protocol: record.server.protocol,
            host: record.server.host.clone(),
            port: record.server.port,
            username: record.server.username.clone(),
            vault_root: record.server.vault_root.clone(),
            size: leftover.size,
            modified: leftover.modified,
        }
    }
}

struct Inner {
    /// The folder working copies are kept under.
    root: PathBuf,
    files: FileQueue,
    events: Events,
    settings: Mutex<EditingSettings>,
    edits: Mutex<Vec<Arc<Edit>>>,
    /// What an earlier run left behind; `None` until the folder has been gone through. Held
    /// while a working copy's folder is made, so the first look never takes a new one for old.
    leftovers: Mutex<Option<Vec<Leftover>>>,
    pace: Pace,
}

impl Inner {
    fn views(&self) -> Vec<EditView> {
        let edits = self.edits.lock().unwrap().clone();
        edits.iter().map(|edit| edit.view()).collect()
    }

    fn emit(&self, notice: Option<Notice>) {
        self.events.external_edits(&EditsUpdate {
            edits: self.views(),
            notice,
        });
    }

    /// Saves what a later run needs to know about a working copy. Without it the copy is
    /// only cleared away by that run, so a failure is logged and nothing more.
    fn keep_record(&self, record: &journal::Record) {
        if let Err(error) = journal::write_record(&self.root, record) {
            log::warn!(
                "Could not save the record of the working copy of {}: {}",
                record.name,
                error.message
            );
        }
    }

    fn find(&self, id: &str) -> AppResult<Arc<Edit>> {
        self.edits
            .lock()
            .unwrap()
            .iter()
            .find(|edit| edit.id == id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::NotFound,
                    "That file is no longer open in another program",
                )
            })
    }

    /// Goes through what an earlier run left, once.
    fn leftovers(&self) -> std::sync::MutexGuard<'_, Option<Vec<Leftover>>> {
        let mut leftovers = self.leftovers.lock().unwrap();
        if leftovers.is_none() {
            let open: Vec<String> = self
                .edits
                .lock()
                .unwrap()
                .iter()
                .map(|edit| edit.id.clone())
                .collect();
            *leftovers = Some(journal::collect_leftovers(&self.root, &open));
        }
        leftovers
    }

    /// What `with` comes to with the programs in the settings.
    fn launch_for(&self, with: &OpenWith) -> AppResult<Launch> {
        let launch = match with {
            OpenWith::Default => Launch::Default,
            OpenWith::Program { id } => {
                let settings = self.settings.lock().unwrap();
                let program = settings
                    .programs
                    .iter()
                    .find(|program| program.id == *id)
                    .ok_or_else(|| {
                        AppError::new(
                            ErrorKind::NotFound,
                            "That program is no longer in the settings",
                        )
                    })?;
                Launch::Program {
                    name: if program.name.is_empty() {
                        program_name(&program.command)
                    } else {
                        program.name.clone()
                    },
                    command: program.command.clone(),
                    arguments: program.arguments.clone(),
                }
            }
            OpenWith::Custom { command } => {
                let command = command.trim();
                if command.is_empty()
                    || command.chars().count() > MAX_COMMAND_CHARS
                    || command.chars().any(char::is_control)
                {
                    return Err(AppError::invalid("That is not a program's path"));
                }
                Launch::Program {
                    name: program_name(command),
                    command: command.to_string(),
                    arguments: String::new(),
                }
            }
        };
        if let Launch::Program { arguments, .. } = &launch {
            command_line::split_arguments(arguments)?;
        }
        Ok(launch)
    }
}

/// A program's name for showing: its file name without the extension.
fn program_name(command: &str) -> String {
    let file = command.rsplit(['/', '\\']).next().unwrap_or(command).trim();
    let name = match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => file,
    };
    name.to_string()
}

/// What every download and upload of a working copy asks of the queue.
fn job_options() -> Arc<JobOptions> {
    Arc::new(JobOptions {
        quiet: true,
        // The job is read once it has ended, to know how it ended.
        keep_completed: Some(true),
        // A working copy is readable by this user alone, which is not for the server's file to
        // take over, and what a server calls executable is not to become so here.
        preserve_permissions: Some(false),
        ..JobOptions::default()
    })
}

#[derive(Clone)]
pub struct ExternalEditManager {
    inner: Arc<Inner>,
}

impl ExternalEditManager {
    /// `root` is the folder working copies are kept under. What an earlier run left there is
    /// gone through in the background.
    pub fn new(root: PathBuf, files: FileQueue, events: Events, settings: EditingSettings) -> Self {
        let manager = Self::with_pace(root, files, events, settings, Pace::default());
        let inner = manager.inner.clone();
        crate::runtime::spawn_blocking(move || drop(inner.leftovers()));
        manager
    }

    pub fn with_pace(
        root: PathBuf,
        files: FileQueue,
        events: Events,
        settings: EditingSettings,
        pace: Pace,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                root,
                files,
                events,
                settings: Mutex::new(settings),
                edits: Mutex::new(Vec::new()),
                leftovers: Mutex::new(None),
                pace,
            }),
        }
    }

    /// Takes over changed settings. Files already open keep the program they were opened with.
    pub fn configure(&self, settings: EditingSettings) {
        *self.inner.settings.lock().unwrap() = settings;
    }

    pub fn list(&self) -> Vec<EditView> {
        self.inner.views()
    }

    /// Opens a server file in another program: downloads it to a working copy and follows it
    /// from there. A file that is open already gets its working copy opened again.
    pub async fn open(&self, request: OpenRequest) -> AppResult<OpenOutcome> {
        let launch = self.inner.launch_for(&request.with)?;
        if request.path.is_empty() || request.path.contains('\0') {
            return Err(AppError::invalid("That is not a file's path"));
        }
        let name = remote_path::file_name(&request.path).to_string();
        let local_name = names::working_copy_name(&name);
        let runs =
            executable::is_executable_name(&name) || executable::is_executable_name(&local_name);
        // A file that would run is never given to the system to open, and to an editor only
        // once the user has said so.
        if runs && (!request.confirmed_executable || launch == Launch::Default) {
            return Ok(OpenOutcome::NeedsConfirmation { name });
        }
        let route = self.inner.files.route_to(&request.session_id).await?;
        let server = ServerIdentity::of(&route);
        if let Some(edit) = self.reopen_same(&server, &request.path, &launch, &request) {
            return Ok(OpenOutcome::AlreadyOpen { edit });
        }
        let found = self
            .inner
            .files
            .stat(&route, &request.path)
            .await?
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::NotFound,
                    format!("{name} is no longer on the server"),
                )
                .with_path(request.path.clone())
            })?;
        if found.is_dir {
            return Err(AppError::invalid(format!("{name} is a folder")));
        }

        let inner = self.inner.clone();
        let id = uuid::Uuid::new_v4().simple().to_string();
        let prepared_id = id.clone();
        let folder = tokio::task::spawn_blocking(move || {
            // Held so the first look at what an earlier run left cannot take this folder for
            // one of those.
            let _leftovers = inner.leftovers();
            journal::prepare_root(&inner.root)?;
            journal::create_edit_folder(&inner.root, &prepared_id)
        })
        .await??;
        let local_path = folder.join(&local_name);
        let in_vault = server.in_vault();
        let core = Core {
            route: route.clone(),
            server,
            remote_path: request.path.clone(),
            name: name.clone(),
            launch: Some(launch.clone()),
            // Replaced by the download's job below, before anything reads it.
            phase: Phase::Watching,
            baseline: ServerState::of(&found),
            synced: None,
            transferred: 0,
            size: found.size,
            stalled: None,
            uploads: 0,
            last_upload: None,
            requests: Requests::default(),
            confirmed_executable: request.confirmed_executable,
            blocked: false,
            looked: None,
            program_closed: None,
            ends_when_clean: false,
            abandoned: false,
        };
        self.inner.keep_record(&core.record(&id, &local_name));
        let edit = Arc::new(Edit {
            id,
            folder,
            local_path,
            local_name,
            in_vault,
            core: Mutex::new(core),
            wake: Notify::new(),
            cancel: CancellationToken::new(),
        });
        {
            // Asked for twice at once, as by a double click: the first one stands.
            let mut edits = self.inner.edits.lock().unwrap();
            let same = edits.iter().find(|other| {
                let other = other.core.lock().unwrap();
                let core = edit.core.lock().unwrap();
                other.server.is_same_place(&core.server) && other.remote_path == core.remote_path
            });
            if let Some(same) = same {
                let view = same.view();
                drop(edits);
                journal::discard(&self.inner.root, &edit.id, in_vault);
                return Ok(OpenOutcome::AlreadyOpen { edit: view });
            }
            let transfer = FileTransfer {
                direction: Direction::Download,
                name,
                source: request.path.clone(),
                target: edit.local_path.to_string_lossy().into_owned(),
                target_directory: edit.folder.to_string_lossy().into_owned(),
                size: found.size,
                resolution: ExistsAction::Overwrite,
                options: Some(job_options()),
            };
            let job = self.inner.files.enqueue(&route, transfer);
            edit.core.lock().unwrap().phase = Phase::Downloading { job };
            edits.push(edit.clone());
        }
        crate::runtime::spawn(follow::follow(self.inner.clone(), edit.clone()));
        self.inner.emit(None);
        Ok(OpenOutcome::Opening { edit: edit.view() })
    }

    /// The edit of the same file on the same server, told to open its working copy again.
    fn reopen_same(
        &self,
        server: &ServerIdentity,
        path: &str,
        launch: &Launch,
        request: &OpenRequest,
    ) -> Option<EditView> {
        let edits = self.inner.edits.lock().unwrap();
        let edit = edits.iter().find(|edit| {
            let core = edit.core.lock().unwrap();
            core.server.is_same_place(server) && core.remote_path == path
        })?;
        {
            let mut core = edit.core.lock().unwrap();
            // A copy held back as a program stays held back until the user agrees.
            if !core.blocked || request.confirmed_executable {
                core.confirmed_executable |= request.confirmed_executable;
                core.requests.open = Some(launch.clone());
            }
        }
        edit.wake.notify_one();
        Some(edit.view())
    }

    /// Opens a file on this computer where it is. Nothing is copied or watched.
    pub async fn open_local(&self, path: &str, with: &OpenWith) -> AppResult<()> {
        let file = Path::new(path);
        if !file.is_absolute() {
            return Err(AppError::invalid("That is not a file's full path"));
        }
        let launch = self.inner.launch_for(with)?;
        let metadata = tokio::fs::metadata(file)
            .await
            .map_err(|error| AppError::from(error).with_path(path))?;
        if !metadata.is_file() {
            return Err(AppError::invalid("Only files open in another program"));
        }
        match launch {
            Launch::Default => launch::open_with_default(file).await,
            Launch::Program {
                command, arguments, ..
            } => {
                let invocation = command_line::invocation(&command, &arguments, path)?;
                // The program runs on by itself; nothing waits for it.
                launch::start(&invocation).map(drop)
            }
        }
    }

    /// Uploads a working copy now: one held back, one whose upload failed, or one whose
    /// server could not be reached.
    pub fn upload(&self, id: &str) -> AppResult<()> {
        let edit = self.inner.find(id)?;
        edit.core.lock().unwrap().requests.upload = true;
        edit.wake.notify_one();
        Ok(())
    }

    /// Answers the question asked when the server's copy changed.
    pub fn resolve(&self, id: &str, resolution: Resolution) -> AppResult<()> {
        let edit = self.inner.find(id)?;
        edit.core.lock().unwrap().requests.answer = Some(resolution);
        edit.wake.notify_one();
        Ok(())
    }

    /// Opens the working copy again: with `with`, or with what it was opened with before.
    /// Naming a program is the user's word that a file held back as a program may be opened
    /// in it.
    pub fn reopen(&self, id: &str, with: Option<&OpenWith>) -> AppResult<()> {
        let edit = self.inner.find(id)?;
        let chosen = with.map(|with| self.inner.launch_for(with)).transpose()?;
        {
            let mut core = edit.core.lock().unwrap();
            let runs = core.blocked || executable::is_executable_name(&core.name);
            let launch = match chosen {
                Some(launch) => {
                    if runs && launch != Launch::Default {
                        core.confirmed_executable = true;
                    }
                    launch
                }
                None => core.launch.clone().unwrap_or(Launch::Default),
            };
            if runs && (launch == Launch::Default || !core.confirmed_executable) {
                return Err(AppError::invalid(format!(
                    "{} is a program or a script. Pick an editor to open it with.",
                    core.name
                )));
            }
            core.requests.open = Some(launch);
        }
        edit.wake.notify_one();
        Ok(())
    }

    /// Shows the working copy's folder in the system's file manager.
    pub fn reveal(&self, id: &str) -> AppResult<()> {
        let edit = self.inner.find(id)?;
        crate::themes::open_folder(&edit.folder)
    }

    /// Stops following a file and deletes its working copy, uploaded or not.
    pub fn stop(&self, id: &str) -> AppResult<()> {
        self.inner.find(id)?.cancel.cancel();
        Ok(())
    }

    /// The edits whose working copy holds something the server does not, after giving a save
    /// made a moment ago the time to be noticed.
    pub async fn unsaved(&self) -> Vec<EditView> {
        let asked = Instant::now();
        let edits = self.inner.edits.lock().unwrap().clone();
        for edit in &edits {
            edit.wake.notify_one();
        }
        // Long enough for a file that is still being written to come to rest.
        let patience = (self.inner.pace.watch.settle + self.inner.pace.watch.poll) * 3;
        while asked.elapsed() < patience {
            let all_looked = edits.iter().all(|edit| {
                let core = edit.core.lock().unwrap();
                core.has_unsaved_changes()
                    || matches!(core.phase, Phase::Downloading { .. })
                    || core.looked.is_some_and(|looked| looked >= asked)
            });
            if all_looked {
                break;
            }
            tokio::time::sleep(SETTLE_POLL).await;
        }
        self.inner
            .views()
            .into_iter()
            .filter(|edit| !matches!(edit.state, EditState::Watching | EditState::Downloading))
            .collect()
    }

    /// Uploads what the edits `ids` hold and waits for it. Comes back with the ones that
    /// still hold something the server does not: a conflict to answer, a failed upload, a
    /// server out of reach.
    pub async fn flush(&self, ids: &[String]) -> Vec<EditView> {
        let asked = Instant::now();
        let edits: Vec<Arc<Edit>> = self
            .inner
            .edits
            .lock()
            .unwrap()
            .iter()
            .filter(|edit| ids.contains(&edit.id))
            .cloned()
            .collect();
        for edit in &edits {
            edit.core.lock().unwrap().requests.upload = true;
            edit.wake.notify_one();
        }
        loop {
            let open = self.inner.edits.lock().unwrap().clone();
            let busy = edits.iter().any(|edit| {
                if !open.iter().any(|other| Arc::ptr_eq(other, edit)) {
                    return false;
                }
                let core = edit.core.lock().unwrap();
                match &core.phase {
                    Phase::Downloading { .. } => false,
                    Phase::Watching => core.looked.is_none_or(|looked| looked < asked),
                    Phase::Uploading { .. } => core.stalled.is_none(),
                    Phase::Conflict { .. } => core.requests.answer.is_some(),
                    Phase::Changed { .. }
                    | Phase::Failed { .. }
                    | Phase::WaitingForServer { .. } => core.requests.upload,
                }
            });
            if !busy {
                break;
            }
            tokio::time::sleep(SETTLE_POLL).await;
        }
        self.inner
            .views()
            .into_iter()
            .filter(|edit| ids.contains(&edit.id))
            .filter(|edit| !matches!(edit.state, EditState::Watching | EditState::Downloading))
            .collect()
    }

    /// The working copies an earlier run left with changes that were never uploaded.
    pub fn leftovers(&self) -> Vec<LeftoverView> {
        self.inner
            .leftovers()
            .iter()
            .flatten()
            .map(LeftoverView::of)
            .collect()
    }

    /// Takes up a working copy from an earlier run and uploads it through the session
    /// `session_id`, which has to lead to the server the file came from. The upload asks
    /// first if the server's copy changed since, like any other.
    pub async fn recover(&self, id: &str, session_id: &str) -> AppResult<EditView> {
        let route = self.inner.files.route_to(session_id).await?;
        let server = ServerIdentity::of(&route);
        let leftover = {
            let mut leftovers = self.inner.leftovers();
            let kept = leftovers.get_or_insert_with(Vec::new);
            let position = kept
                .iter()
                .position(|leftover| leftover.record.id == id)
                .ok_or_else(|| AppError::new(ErrorKind::NotFound, "That file is no longer kept"))?;
            if !kept[position].record.server.is_same_place(&server) {
                return Err(AppError::invalid(format!(
                    "{} came from {}, which is not the server of this tab",
                    kept[position].record.name, kept[position].record.server.label
                )));
            }
            kept.remove(position)
        };
        let record = leftover.record;
        let edit = Arc::new(Edit {
            id: record.id.clone(),
            folder: journal::edit_folder(&self.inner.root, &record.id),
            local_path: journal::working_copy(&self.inner.root, &record),
            local_name: record.local_name.clone(),
            in_vault: server.in_vault(),
            core: Mutex::new(Core {
                route,
                server,
                remote_path: record.remote_path,
                name: record.name,
                launch: None,
                phase: Phase::Watching,
                baseline: record.baseline,
                synced: record.synced,
                transferred: 0,
                size: leftover.size,
                stalled: None,
                uploads: 0,
                last_upload: None,
                requests: Requests {
                    upload: true,
                    ..Requests::default()
                },
                confirmed_executable: false,
                blocked: false,
                looked: None,
                program_closed: None,
                ends_when_clean: true,
                abandoned: false,
            }),
            wake: Notify::new(),
            cancel: CancellationToken::new(),
        });
        self.inner.edits.lock().unwrap().push(edit.clone());
        crate::runtime::spawn(follow::follow(self.inner.clone(), edit.clone()));
        self.inner.emit(None);
        Ok(edit.view())
    }

    /// Shows the folder of a working copy an earlier run left.
    pub fn reveal_leftover(&self, id: &str) -> AppResult<()> {
        let known = self
            .inner
            .leftovers()
            .iter()
            .flatten()
            .any(|leftover| leftover.record.id == id);
        if !known {
            return Err(AppError::new(
                ErrorKind::NotFound,
                "That file is no longer kept",
            ));
        }
        crate::themes::open_folder(&journal::edit_folder(&self.inner.root, id))
    }

    /// Deletes a working copy an earlier run left, with the changes in it.
    pub fn discard_leftover(&self, id: &str) -> AppResult<()> {
        let mut leftovers = self.inner.leftovers();
        let kept = leftovers.get_or_insert_with(Vec::new);
        let position = kept
            .iter()
            .position(|leftover| leftover.record.id == id)
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "That file is no longer kept"))?;
        let leftover = kept.remove(position);
        journal::discard(&self.inner.root, id, leftover.record.server.in_vault());
        Ok(())
    }

    /// The app is closing. Working copies the server has are deleted. The ones holding
    /// changes that were never uploaded stay for the next run to offer back, except a
    /// vault's, whose decrypted files never outlast the app.
    pub fn shutdown(&self) {
        let edits = std::mem::take(&mut *self.inner.edits.lock().unwrap());
        for edit in edits {
            let (kept, name) = {
                let mut core = edit.core.lock().unwrap();
                core.abandoned = true;
                let changed = !matches!(core.phase, Phase::Downloading { .. })
                    && core
                        .synced
                        .as_ref()
                        .is_some_and(|synced| differs_from(&edit.local_path, synced));
                (changed && !edit.in_vault, core.name.clone())
            };
            edit.cancel.cancel();
            if kept {
                log::info!(
                    "Keeping the working copy of {name} for the next start: it was not uploaded"
                );
            } else {
                journal::discard(&self.inner.root, &edit.id, edit.in_vault);
            }
        }
    }
}

/// Whether the file holds other content than `synced`. A file that is there but cannot be
/// read is taken for changed, so it is kept rather than deleted unseen.
fn differs_from(file: &Path, synced: &Fingerprint) -> bool {
    let Ok(metadata) = std::fs::metadata(file) else {
        return false;
    };
    if metadata.len() != synced.size || metadata.len() > MAX_EXIT_CHECK_BYTES {
        return true;
    }
    Fingerprint::of(file).map_or(true, |content| content != *synced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_are_named_after_their_file() {
        assert_eq!(program_name("/usr/bin/gimp"), "gimp");
        assert_eq!(
            program_name(r"C:\Program Files\Notepad++\notepad++.exe"),
            "notepad++"
        );
        assert_eq!(program_name("code"), "code");
        assert_eq!(program_name(".hidden"), ".hidden");
    }

    #[test]
    fn working_copies_do_not_take_the_server_s_permissions() {
        let options = job_options();
        assert!(options.quiet);
        assert_eq!(options.preserve_permissions, Some(false));
        assert_eq!(options.keep_completed, Some(true));
        assert!(!options.executability);
    }

    #[test]
    fn a_working_copy_is_kept_at_exit_only_when_it_differs() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("report.txt");
        std::fs::write(&file, "first").unwrap();
        let synced = Fingerprint::of(&file).unwrap();
        assert!(!differs_from(&file, &synced));
        std::fs::write(&file, "FIRST").unwrap();
        assert!(differs_from(&file, &synced));
        std::fs::write(&file, "longer than before").unwrap();
        assert!(differs_from(&file, &synced));
        // Nothing is left to keep of a file that is gone.
        assert!(!differs_from(&folder.path().join("missing"), &synced));
    }
}
