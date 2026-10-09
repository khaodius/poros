//! Moving, copying and changing files where they already are, on this computer or within one
//! server, without sending them through the transfer queue.

mod compare;
mod local_ops;
mod names;
mod permissions;
mod server;

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult};
use crate::events::Events;
use crate::session::{Session, SessionManager};
use crate::sftp::RemoteFs;
pub use compare::{CompareRequest, Comparison, FileRef};
pub use permissions::{Details, ModeChange, PermissionRequest, PermissionSummary, Usage};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Location {
    Local,
    Remote { session_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Move,
    Copy,
}

/// What to do when the target folder already has an item of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Conflict {
    /// Files are replaced and folders merged.
    Replace,
    Skip,
    /// The item gets a free name such as `name (2).txt`.
    #[default]
    KeepBoth,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveCopyRequest {
    /// Names this operation for progress events and cancelling.
    pub operation_id: String,
    pub location: Location,
    pub mode: Mode,
    pub sources: Vec<String>,
    pub target_directory: String,
    #[serde(default)]
    pub conflict: Conflict,
}

/// How an operation did its work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    /// Renamed in place.
    Rename,
    /// The server's `copy-data` extension.
    CopyData,
    /// `cp` or `mv` run on the server.
    Command,
    /// Read and written back over SFTP.
    Stream,
    /// On this computer's disks.
    Local,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationSummary {
    /// Where each item that was moved or copied is now.
    pub placed: Vec<String>,
    pub skipped: u64,
    pub failures: Vec<Failure>,
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationProgress {
    pub operation_id: String,
    pub files: u64,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

/// An SFTP channel of the operation's own, so long server-side work never holds up browsing,
/// or the browsing one when the server allows no more.
pub(crate) enum Channel<'a> {
    Own(RemoteFs),
    Shared(&'a RemoteFs),
}

impl<'a> Channel<'a> {
    async fn open(session: &'a Session) -> AppResult<Channel<'a>> {
        let browsing = session.sftp()?;
        Ok(match session.open_channel().await {
            Ok(fs) => Self::Own(fs),
            Err(_) => Self::Shared(browsing),
        })
    }
}

impl Deref for Channel<'_> {
    type Target = RemoteFs;

    fn deref(&self) -> &RemoteFs {
        match self {
            Self::Own(fs) => fs,
            Self::Shared(fs) => fs,
        }
    }
}

impl Drop for Channel<'_> {
    fn drop(&mut self) {
        if let Self::Own(fs) = self {
            fs.close();
        }
    }
}

/// What an operation has done so far, shared by everything working on it.
#[derive(Default)]
pub(crate) struct Progress {
    files: AtomicU64,
    bytes: AtomicU64,
    current: Mutex<Option<String>>,
    failures: Mutex<Vec<Failure>>,
    methods: Mutex<Vec<Method>>,
}

impl Progress {
    fn file_done(&self) {
        self.files.fetch_add(1, Ordering::Relaxed);
    }

    fn add_bytes(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    fn working_on(&self, path: &str) {
        *self.current.lock().unwrap() = Some(path.to_string());
    }

    fn used(&self, method: Method) {
        let mut methods = self.methods.lock().unwrap();
        if !methods.contains(&method) {
            methods.push(method);
        }
    }

    /// Records a problem with one entry so the rest can go on; cancelling is not a problem.
    fn fail(&self, path: &str, error: AppError) {
        if error.kind != crate::error::ErrorKind::Cancelled {
            self.failures.lock().unwrap().push(Failure {
                path: path.to_string(),
                message: error.message,
            });
        }
    }

    fn files_done(&self) -> u64 {
        self.files.load(Ordering::Relaxed)
    }

    fn snapshot(&self, operation_id: &str) -> OperationProgress {
        OperationProgress {
            operation_id: operation_id.to_string(),
            files: self.files.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            current: self.current.lock().unwrap().clone(),
        }
    }

    fn take_failures(&self) -> Vec<Failure> {
        std::mem::take(&mut self.failures.lock().unwrap())
    }

    fn methods(&self) -> Vec<Method> {
        self.methods.lock().unwrap().clone()
    }
}

/// Items done by a run over several top-level entries: where each went, or `None` if skipped.
fn summarize(
    progress: &Progress,
    results: Vec<(String, AppResult<Option<String>>)>,
    cancel: &CancellationToken,
) -> AppResult<OperationSummary> {
    if cancel.is_cancelled() {
        return Err(AppError::cancelled());
    }
    let mut summary = OperationSummary::default();
    for (source, result) in results {
        match result {
            Ok(Some(placed)) => summary.placed.push(placed),
            Ok(None) => summary.skipped += 1,
            Err(error) => progress.fail(&source, error),
        }
    }
    summary.failures = progress.take_failures();
    summary.methods = progress.methods();
    Ok(summary)
}

/// Server features operations may use when a server offers them. The app uses every one;
/// turning one off makes operations take the way they would on a server without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerFeatures {
    pub copy_data: bool,
    pub commands: bool,
}

impl Default for ServerFeatures {
    fn default() -> Self {
        Self {
            copy_data: true,
            commands: true,
        }
    }
}

pub struct FileOperations {
    sessions: Arc<SessionManager>,
    events: Events,
    features: ServerFeatures,
    running: Mutex<HashMap<String, CancellationToken>>,
}

/// An operation in progress: reports progress until dropped.
struct Running<'a> {
    operations: &'a FileOperations,
    id: String,
    cancel: CancellationToken,
    progress: Arc<Progress>,
    reporter: tokio::task::JoinHandle<()>,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.reporter.abort();
        self.operations.running.lock().unwrap().remove(&self.id);
    }
}

impl FileOperations {
    pub fn new(sessions: Arc<SessionManager>, events: Events) -> Self {
        Self::with_features(sessions, events, ServerFeatures::default())
    }

    pub fn with_features(
        sessions: Arc<SessionManager>,
        events: Events,
        features: ServerFeatures,
    ) -> Self {
        Self {
            sessions,
            events,
            features,
            running: Mutex::new(HashMap::new()),
        }
    }

    fn begin(&self, operation_id: &str) -> Running<'_> {
        let cancel = CancellationToken::new();
        self.running
            .lock()
            .unwrap()
            .insert(operation_id.to_string(), cancel.clone());
        let progress = Arc::new(Progress::default());
        let reporter = {
            let progress = progress.clone();
            let events = self.events.clone();
            let id = operation_id.to_string();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(PROGRESS_INTERVAL);
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    events.file_operation(&progress.snapshot(&id));
                }
            })
        };
        Running {
            operations: self,
            id: operation_id.to_string(),
            cancel,
            progress,
            reporter,
        }
    }

    pub fn cancel(&self, operation_id: &str) {
        if let Some(cancel) = self.running.lock().unwrap().get(operation_id) {
            cancel.cancel();
        }
    }

    /// The names among `names` that are already taken in `directory`.
    pub async fn conflicts(
        &self,
        location: Location,
        names: Vec<String>,
        directory: String,
    ) -> AppResult<Vec<String>> {
        match location {
            Location::Local => {
                tokio::task::spawn_blocking(move || local_ops::existing_names(&directory, &names))
                    .await?
            }
            Location::Remote { session_id } => {
                let session = self.sessions.get(&session_id).await?;
                server::existing_names(session.sftp()?, &directory, &names).await
            }
        }
    }

    pub async fn move_or_copy(&self, request: MoveCopyRequest) -> AppResult<OperationSummary> {
        let running = self.begin(&request.operation_id);
        match &request.location {
            Location::Local => {
                let cancel = running.cancel.clone();
                let progress = running.progress.clone();
                tokio::task::spawn_blocking(move || {
                    local_ops::move_or_copy(&request, &cancel, &progress)
                })
                .await?
            }
            Location::Remote { session_id } => {
                let session = self.sessions.get(session_id).await?;
                let work =
                    server::Work::new(&session, self.features, &running.cancel, &running.progress)
                        .await?;
                work.move_or_copy(&request).await
            }
        }
    }

    pub async fn details(&self, session_id: &str, path: &str) -> AppResult<Details> {
        let session = self.sessions.get(session_id).await?;
        permissions::details(session.sftp()?, path).await
    }

    pub async fn measure(
        &self,
        operation_id: &str,
        session_id: &str,
        paths: &[String],
    ) -> AppResult<Usage> {
        let running = self.begin(operation_id);
        let session = self.sessions.get(session_id).await?;
        permissions::measure(&session, paths, &running.cancel, &running.progress).await
    }

    pub async fn set_permissions(
        &self,
        request: PermissionRequest,
    ) -> AppResult<PermissionSummary> {
        let running = self.begin(&request.operation_id);
        let session = self.sessions.get(&request.session_id).await?;
        permissions::apply(
            &session,
            self.features,
            &request,
            &running.cancel,
            &running.progress,
        )
        .await
    }

    pub async fn compare(&self, request: CompareRequest) -> AppResult<Comparison> {
        let running = self.begin(&request.operation_id);
        compare::compare(&self.sessions, &request, &running.cancel).await
    }
}
