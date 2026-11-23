//! One server file open in another program: what is known of it while it is followed, and how
//! the page is shown it.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::conflict::{Resolution, ServerState};
use super::journal::{Record, ServerIdentity};
use super::watch::Fingerprint;
use crate::transfer::{JobId, TransferRoute};

/// What a working copy is opened with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Launch {
    /// The program the system has for the file's type.
    Default,
    Program {
        name: String,
        command: String,
        arguments: String,
    },
}

impl Launch {
    fn label(&self) -> &str {
        match self {
            Launch::Default => "Default program",
            Launch::Program { name, .. } => name,
        }
    }
}

/// Where an edit stands. Every phase after the download knows the content it is about, so a
/// later save is told from the one already dealt with.
#[derive(Debug, Clone)]
pub(super) enum Phase {
    Downloading {
        job: JobId,
    },
    /// The working copy holds what the server has.
    Watching,
    /// Saved, and held back: the user is asked before uploads, or said not now.
    Changed {
        content: Fingerprint,
    },
    /// The server could not be asked about its copy; asked again at `retry_at`.
    WaitingForServer {
        content: Fingerprint,
        message: String,
        retry_at: Instant,
    },
    /// The server's copy changed since Poros last saw it, and the user has not said what to do.
    Conflict {
        content: Fingerprint,
        /// The server's copy now; `None` when it is gone.
        server: Option<ServerState>,
        /// When the working copy was saved, in milliseconds since the Unix epoch.
        saved_at: Option<u64>,
    },
    Uploading {
        job: JobId,
        content: Fingerprint,
    },
    /// The upload failed. Its job stays in the queue, where it can be tried again as well.
    Failed {
        job: Option<JobId>,
        content: Fingerprint,
        message: String,
    },
}

/// What the user asked for, picked up by the task following the edit.
#[derive(Debug, Default)]
pub(super) struct Requests {
    /// Upload the working copy as it is, without waiting to be asked.
    pub upload: bool,
    /// The answer to a conflict.
    pub answer: Option<Resolution>,
    /// Open the working copy again, with this.
    pub open: Option<Launch>,
}

pub(super) struct Core {
    pub route: TransferRoute,
    pub server: ServerIdentity,
    pub remote_path: String,
    /// The file's name on the server.
    pub name: String,
    /// What the working copy was last opened with; `None` for one taken up from an earlier run.
    pub launch: Option<Launch>,
    pub phase: Phase,
    /// The server's copy as it was downloaded, or as the last upload left it.
    pub baseline: ServerState,
    /// The content the server has, once the download is in.
    pub synced: Option<Fingerprint>,
    /// Bytes moved and to move by the download or upload under way.
    pub transferred: u64,
    pub size: u64,
    /// The transfer under way cannot move: the queue is paused or the server is out of reach.
    pub stalled: Option<String>,
    pub uploads: u32,
    /// When the last upload finished, in milliseconds since the Unix epoch.
    pub last_upload: Option<u64>,
    pub requests: Requests,
    /// The user agreed to open a file of a type that runs, in an editor.
    pub confirmed_executable: bool,
    /// The file is a program or a script, by its name or its content, so the system's default
    /// program is never asked to open it.
    pub runs: bool,
    /// Downloaded but not opened: its content is a program's, and the user has not agreed.
    pub blocked: bool,
    /// When the working copy was last found at rest, changed or not.
    pub looked: Option<Instant>,
    /// When the program that had the file open was seen to close.
    pub program_closed: Option<Instant>,
    /// Ends once the server has the working copy's content, as for a file taken up from an
    /// earlier run, which no program has open.
    pub ends_when_clean: bool,
    /// The app is closing and has dealt with the working copy itself.
    pub abandoned: bool,
}

impl Core {
    pub fn record(&self, id: &str, local_name: &str) -> Record {
        Record {
            id: id.to_string(),
            server: self.server.clone(),
            remote_path: self.remote_path.clone(),
            name: self.name.clone(),
            local_name: local_name.to_string(),
            synced: self.synced.clone(),
            baseline: self.baseline,
        }
    }

    /// Whether the working copy holds something the server does not, as far as is known.
    pub fn has_unsaved_changes(&self) -> bool {
        !matches!(self.phase, Phase::Downloading { .. } | Phase::Watching)
    }
}

pub(super) struct Edit {
    pub id: String,
    /// The working copy's own folder.
    pub folder: PathBuf,
    pub local_path: PathBuf,
    pub local_name: String,
    /// The working copy is a decrypted file of a vault.
    pub in_vault: bool,
    pub core: Mutex<Core>,
    pub wake: Notify,
    pub cancel: CancellationToken,
}

impl Edit {
    pub fn view(&self) -> EditView {
        let core = self.core.lock().unwrap();
        let (state, detail, conflict) = match &core.phase {
            Phase::Downloading { .. } => (EditState::Downloading, core.stalled.clone(), None),
            Phase::Watching => (EditState::Watching, None, None),
            Phase::Changed { .. } => (EditState::Changed, None, None),
            Phase::WaitingForServer { message, .. } => {
                (EditState::Waiting, Some(message.clone()), None)
            }
            Phase::Conflict {
                content,
                server,
                saved_at,
            } => (
                EditState::Conflict,
                None,
                Some(ConflictView {
                    server_size: server.map(|server| server.size),
                    server_modified: server.and_then(|server| server.modified),
                    deleted: server.is_none(),
                    local_size: content.size,
                    local_modified: *saved_at,
                }),
            ),
            Phase::Uploading { .. } => match &core.stalled {
                Some(reason) => (EditState::Waiting, Some(reason.clone()), None),
                None => (EditState::Uploading, None, None),
            },
            Phase::Failed { message, .. } => (EditState::Failed, Some(message.clone()), None),
        };
        EditView {
            id: self.id.clone(),
            name: core.name.clone(),
            remote_path: core.remote_path.clone(),
            session_id: core.route.session_id().to_string(),
            server: core.server.label.clone(),
            in_vault: self.in_vault,
            // A vault locks as its tab closes; any other server is reached again by the queue.
            outlives_session: !self.in_vault,
            local_path: self.local_path.to_string_lossy().into_owned(),
            program: core
                .launch
                .as_ref()
                .map(|launch| launch.label().to_string())
                .unwrap_or_default(),
            state,
            transferred: core.transferred,
            size: core.size,
            detail,
            conflict,
            blocked: core.blocked,
            uploads: core.uploads,
            last_upload: core.last_upload,
        }
    }
}

/// Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EditState {
    Downloading,
    /// Open in its program, with nothing to upload.
    Watching,
    /// Saved, and waiting for the user to say it should be uploaded.
    Changed,
    /// An upload is waiting for the server to be reached or for the queue to run.
    Waiting,
    /// The server's copy changed; the user is asked what to do.
    Conflict,
    Uploading,
    Failed,
}

/// The two copies of a file whose server copy changed. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub server_size: Option<u64>,
    /// Seconds since the Unix epoch.
    pub server_modified: Option<i64>,
    /// The file is gone from the server.
    pub deleted: bool,
    pub local_size: u64,
    /// Milliseconds since the Unix epoch.
    pub local_modified: Option<u64>,
}

/// A file open in another program, as the page shows it. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditView {
    pub id: String,
    pub name: String,
    pub remote_path: String,
    /// The session uploads go through now, which changes when a tab reconnects.
    pub session_id: String,
    /// The server's label.
    pub server: String,
    pub in_vault: bool,
    /// Saves are still uploaded once the tab is closed; a vault's are not, as it locks.
    pub outlives_session: bool,
    pub local_path: String,
    /// What the file was opened with; empty for a file taken up from an earlier run.
    pub program: String,
    pub state: EditState,
    pub transferred: u64,
    pub size: u64,
    /// Why an upload failed or waits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<ConflictView>,
    /// Downloaded but not opened, because its content is a program's.
    pub blocked: bool,
    pub uploads: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_upload: Option<u64>,
}
