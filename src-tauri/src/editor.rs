//! Text files open in the editor. A document remembers where its file lives, and for a file on
//! an SFTP server how to reach the server, so it still saves after its session was closed or
//! dropped. Files on FTP servers and cloud drives go through their session.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use bytes::Bytes;
use futures::future;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};
use crate::format::format_size;
use crate::protocol::{Protocol, RemoteFileSystem, RemoteStat, WriteRequest};
use crate::remote_path;
use crate::session::{self, RemoteTarget, SessionManager};
use crate::sftp::{ReadChunk, RemoteFs};
use crate::ssh::{self, SshHandle};
use crate::text::{self, LineEnding, TextEncoding};

/// Larger files are better served by a transfer and a dedicated editor.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const CHUNK_BYTES: u32 = 256 * 1024;
const PARALLEL_REQUESTS: u64 = 8;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "side", rename_all = "camelCase")]
pub enum FileLocation {
    Local {
        path: String,
    },
    #[serde(rename_all = "camelCase")]
    Remote {
        session_id: String,
        path: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    /// "Local", or the label of the server the file is on.
    pub origin: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStamp {
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocument {
    pub text: String,
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
    pub stamp: FileStamp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequest {
    pub document_id: String,
    pub text: String,
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
    /// The file as it was when last read or saved. A file that no longer matches is left alone
    /// and reported; `None` overwrites whatever is there.
    pub expected: Option<FileStamp>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SaveOutcome {
    Saved {
        stamp: FileStamp,
    },
    /// Something else changed or removed the file since it was opened; `None` when removed.
    Changed {
        current: Option<FileStamp>,
    },
}

struct Document {
    name: String,
    path: String,
    origin: Origin,
    /// Label of the window showing the document; closing that window forgets it.
    owner: Mutex<String>,
}

enum Origin {
    Local,
    Remote(Box<RemoteOrigin>),
}

struct RemoteOrigin {
    target: RemoteTarget,
    /// Opened once the file's session is gone, and kept for the saves that follow.
    own_connection: tokio::sync::Mutex<Option<OwnConnection>>,
}

struct OwnConnection {
    handle: SshHandle,
    fs: RemoteFs,
}

impl OwnConnection {
    async fn close(self) {
        self.fs.close();
        ssh::disconnect(&self.handle).await;
    }
}

pub struct EditorManager {
    documents: Mutex<HashMap<String, Arc<Document>>>,
    sessions: Arc<SessionManager>,
    events: Events,
}

impl EditorManager {
    pub fn new(sessions: Arc<SessionManager>, events: Events) -> Self {
        Self {
            documents: Mutex::new(HashMap::new()),
            sessions,
            events,
        }
    }

    pub async fn open(&self, location: FileLocation, owner: &str) -> AppResult<DocumentInfo> {
        let (path, name, origin, origin_label) = match location {
            FileLocation::Local { path } => {
                let name = Path::new(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                (path, name, Origin::Local, "Local".to_string())
            }
            FileLocation::Remote { session_id, path } => {
                let target = self.sessions.get(&session_id).await?.target();
                let label = target.label.clone();
                let name = remote_path::file_name(&path).to_string();
                let origin = Origin::Remote(Box::new(RemoteOrigin {
                    target,
                    own_connection: tokio::sync::Mutex::new(None),
                }));
                (path, name, origin, label)
            }
        };
        let id = uuid::Uuid::new_v4().to_string();
        let info = DocumentInfo {
            id: id.clone(),
            name: name.clone(),
            path: path.clone(),
            origin: origin_label,
        };
        let document = Document {
            name,
            path,
            origin,
            owner: Mutex::new(owner.to_string()),
        };
        self.documents
            .lock()
            .unwrap()
            .insert(id, Arc::new(document));
        Ok(info)
    }

    /// Reads the file; the calling window becomes the document's owner.
    pub async fn load(&self, id: &str, owner: &str) -> AppResult<TextDocument> {
        let document = self.document(id)?;
        *document.owner.lock().unwrap() = owner.to_string();
        let (bytes, stamp) = match &document.origin {
            Origin::Local => {
                let path = document.path.clone();
                tokio::task::spawn_blocking(move || read_local(&path)).await??
            }
            Origin::Remote(remote) => {
                let reply = self.on_remote(&document, remote, Request::Read).await?;
                (
                    reply.contents,
                    reply.stamp.expect("a read reports the file's stamp"),
                )
            }
        };
        let decoded = text::decode(&bytes).map_err(|error| error.with_path(&document.path))?;
        Ok(TextDocument {
            text: decoded.text,
            encoding: decoded.encoding,
            line_ending: decoded.line_ending,
            stamp,
        })
    }

    /// Makes the calling window the owner of a document it took over from another window.
    pub fn adopt(&self, id: &str, owner: &str) -> AppResult<()> {
        *self.document(id)?.owner.lock().unwrap() = owner.to_string();
        Ok(())
    }

    pub async fn save(&self, request: SaveRequest) -> AppResult<SaveOutcome> {
        let document = self.document(&request.document_id)?;
        let contents = text::encode(&request.text, request.encoding, request.line_ending)?;
        let size = contents.len() as u64;
        let outcome = match &document.origin {
            Origin::Local => {
                let path = document.path.clone();
                let expected = request.expected;
                tokio::task::spawn_blocking(move || save_local(&path, &contents, expected))
                    .await??
            }
            Origin::Remote(remote) => {
                self.save_remote(&document, remote, contents, request.expected)
                    .await?
            }
        };
        if let SaveOutcome::Saved { .. } = outcome {
            let (session_id, place) = match &document.origin {
                Origin::Local => (None, String::new()),
                Origin::Remote(remote) => (
                    Some(remote.target.session_id.as_str()),
                    format!(" on {}", remote.target.label),
                ),
            };
            self.events.log(
                LogLevel::Info,
                session_id,
                format!("Saved {}{place} ({})", document.path, format_size(size)),
            );
        }
        Ok(outcome)
    }

    async fn save_remote(
        &self,
        document: &Document,
        remote: &RemoteOrigin,
        contents: Vec<u8>,
        expected: Option<FileStamp>,
    ) -> AppResult<SaveOutcome> {
        if let Some(expected) = expected {
            let current = self.on_remote(document, remote, Request::Stat).await?.stamp;
            if current != Some(expected) {
                return Ok(SaveOutcome::Changed { current });
            }
        }
        self.on_remote(document, remote, Request::Write(&contents))
            .await?;
        let stamp = self
            .on_remote(document, remote, Request::Stat)
            .await?
            .stamp
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::NotFound,
                    "The file disappeared right after saving",
                )
                .with_path(&document.path)
            })?;
        if stamp.size != contents.len() as u64 {
            return Err(AppError::new(
                ErrorKind::Sftp,
                format!(
                    "The server stored {} instead of {}",
                    format_size(stamp.size),
                    format_size(contents.len() as u64)
                ),
            )
            .with_path(&document.path));
        }
        Ok(SaveOutcome::Saved { stamp })
    }

    /// Runs a request on the file's session. For an SFTP file it falls back to a connection of
    /// the document's own once that session is closed or its connection has dropped; FTP
    /// sessions reconnect by themselves and cloud drives have no connection to lose.
    async fn on_remote(
        &self,
        document: &Document,
        remote: &RemoteOrigin,
        request: Request<'_>,
    ) -> AppResult<Reply> {
        let mut own = remote.own_connection.lock().await;
        if own.is_none() {
            match self.sessions.get(&remote.target.session_id).await {
                Ok(session) => match session.sftp() {
                    Ok(fs) => match perform(fs, &document.path, &request).await {
                        Err(error) if error.is_connection_lost() => {}
                        result => return result,
                    },
                    Err(_) => return perform_on(session.files(), &document.path, &request).await,
                },
                Err(error) if remote.target.protocol() != Protocol::Sftp => return Err(error),
                Err(_) => {}
            }
        }
        if let Some(connection) = own
            .as_ref()
            .filter(|connection| !connection.handle.is_closed())
        {
            match perform(&connection.fs, &document.path, &request).await {
                Err(error) if error.is_connection_lost() => {}
                result => return result,
            }
        }
        if let Some(stale) = own.take() {
            stale.close().await;
        }
        let connection = own.insert(self.connect(document, remote).await?);
        perform(&connection.fs, &document.path, &request).await
    }

    async fn connect(
        &self,
        document: &Document,
        remote: &RemoteOrigin,
    ) -> AppResult<OwnConnection> {
        let handle = remote
            .target
            .connect(&self.sessions.known_hosts, "editor")
            .await?;
        let fs = match session::open_sftp(&handle).await {
            Ok(fs) => fs,
            Err(error) => {
                ssh::disconnect(&handle).await;
                return Err(error);
            }
        };
        self.events.log(
            LogLevel::Info,
            Some(&remote.target.session_id),
            format!(
                "Reconnected to {} for {}",
                remote.target.label, document.name
            ),
        );
        Ok(OwnConnection { handle, fs })
    }

    pub async fn close(&self, id: &str) {
        let document = self.documents.lock().unwrap().remove(id);
        if let Some(document) = document {
            release(&document).await;
        }
    }

    pub async fn close_owned_by(&self, owner: &str) {
        let owned: Vec<Arc<Document>> = {
            let mut documents = self.documents.lock().unwrap();
            let ids: Vec<String> = documents
                .iter()
                .filter(|(_, document)| *document.owner.lock().unwrap() == owner)
                .map(|(id, _)| id.clone())
                .collect();
            ids.iter().filter_map(|id| documents.remove(id)).collect()
        };
        for document in owned {
            release(&document).await;
        }
    }

    fn document(&self, id: &str) -> AppResult<Arc<Document>> {
        self.documents
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "This file is no longer open"))
    }
}

enum Request<'a> {
    Stat,
    Read,
    Write(&'a [u8]),
}

/// What a request returns: a read fills both fields, a stat only the stamp, a write neither.
#[derive(Default)]
struct Reply {
    contents: Vec<u8>,
    /// `None` when nothing is at the path.
    stamp: Option<FileStamp>,
}

async fn perform(fs: &RemoteFs, path: &str, request: &Request<'_>) -> AppResult<Reply> {
    match request {
        Request::Stat => Ok(Reply {
            stamp: fs.stat(path).await?.map(remote_stamp),
            ..Reply::default()
        }),
        Request::Read => {
            let (contents, stamp) = read_remote(fs, path).await?;
            Ok(Reply {
                contents,
                stamp: Some(stamp),
            })
        }
        Request::Write(contents) => {
            write_remote(fs, path, contents).await?;
            Ok(Reply::default())
        }
    }
}

/// The same requests through the file system every protocol shares, for FTP and cloud drives.
async fn perform_on(
    files: Arc<dyn RemoteFileSystem>,
    path: &str,
    request: &Request<'_>,
) -> AppResult<Reply> {
    match request {
        Request::Stat => Ok(Reply {
            stamp: files.stat(path).await?.map(remote_stamp),
            ..Reply::default()
        }),
        Request::Read => {
            let stat = existing_file(files.stat(path).await?, path)?;
            let mut reader = files.open_read(path, 0..stat.size).await?;
            let mut contents = Vec::new();
            let read = async {
                while let Some(data) = reader.next_chunk().await? {
                    contents.extend_from_slice(&data);
                    if contents.len() as u64 > MAX_FILE_BYTES {
                        return Err(too_large(contents.len() as u64, path));
                    }
                }
                Ok(())
            }
            .await;
            let finished = reader.finish().await;
            read.and(finished).map_err(|error| error.with_path(path))?;
            Ok(Reply {
                contents,
                stamp: Some(remote_stamp(stat)),
            })
        }
        Request::Write(contents) => {
            let mut writer = files
                .open_write(WriteRequest {
                    path: path.to_string(),
                    offset: 0,
                    size: contents.len() as u64,
                    modified: None,
                    permissions: None,
                })
                .await?;
            let written = writer.write(Bytes::copy_from_slice(contents)).await;
            let finished = writer.finish().await;
            written
                .and(finished)
                .map_err(|error| error.with_path(path))?;
            Ok(Reply::default())
        }
    }
}

async fn release(document: &Document) {
    if let Origin::Remote(remote) = &document.origin {
        if let Some(connection) = remote.own_connection.lock().await.take() {
            connection.close().await;
        }
    }
}

fn too_large(size: u64, path: &str) -> AppError {
    AppError::invalid(format!(
        "This file is {}, more than the {} the editor opens",
        format_size(size),
        format_size(MAX_FILE_BYTES)
    ))
    .with_path(path)
}

fn not_a_file(path: &str) -> AppError {
    AppError::invalid("Only files can be opened in the editor").with_path(path)
}

fn local_stamp(metadata: &std::fs::Metadata) -> FileStamp {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok());
    FileStamp {
        size: metadata.len(),
        modified,
    }
}

/// A file the editor can open, from what a stat found at its path.
fn existing_file(stat: Option<RemoteStat>, path: &str) -> AppResult<RemoteStat> {
    let stat = stat.ok_or_else(|| {
        AppError::new(ErrorKind::NotFound, "The file no longer exists").with_path(path)
    })?;
    if stat.is_dir {
        return Err(not_a_file(path));
    }
    if stat.size > MAX_FILE_BYTES {
        return Err(too_large(stat.size, path));
    }
    Ok(stat)
}

fn remote_stamp(stat: RemoteStat) -> FileStamp {
    FileStamp {
        size: stat.size,
        modified: stat.modified.map(|seconds| seconds * 1000),
    }
}

fn read_local(path: &str) -> AppResult<(Vec<u8>, FileStamp)> {
    let with_path = |error: std::io::Error| AppError::from(error).with_path(path);
    let metadata = std::fs::metadata(path).map_err(with_path)?;
    if metadata.is_dir() {
        return Err(not_a_file(path));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(too_large(metadata.len(), path));
    }
    let bytes = std::fs::read(path).map_err(with_path)?;
    Ok((bytes, local_stamp(&metadata)))
}

/// Writes in place, so the file keeps its permissions, owner and links.
fn save_local(path: &str, contents: &[u8], expected: Option<FileStamp>) -> AppResult<SaveOutcome> {
    use std::io::Write;

    let with_path = |error: std::io::Error| AppError::from(error).with_path(path);
    if let Some(expected) = expected {
        let current = match std::fs::metadata(path) {
            Ok(metadata) => Some(local_stamp(&metadata)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(with_path(error)),
        };
        if current != Some(expected) {
            return Ok(SaveOutcome::Changed { current });
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(with_path)?;
    file.write_all(contents).map_err(with_path)?;
    file.sync_all().map_err(with_path)?;
    let stamp = local_stamp(&file.metadata().map_err(with_path)?);
    Ok(SaveOutcome::Saved { stamp })
}

async fn read_remote(fs: &RemoteFs, path: &str) -> AppResult<(Vec<u8>, FileStamp)> {
    let stat = existing_file(fs.stat(path).await?, path)?;
    let handle = fs.open_for_read(path).await?;
    let result = read_open_file(fs, &handle, path).await;
    let _ = fs.close_handle(handle).await;
    Ok((result?, remote_stamp(stat)))
}

/// Reads ahead several chunks at a time. A short reply means the server sent less than asked,
/// so reading carries on from where the data ends.
async fn read_open_file(fs: &RemoteFs, handle: &str, path: &str) -> AppResult<Vec<u8>> {
    let chunk = fs.read_size(CHUNK_BYTES);
    let mut contents = Vec::new();
    loop {
        let start = contents.len() as u64;
        let reads = (0..PARALLEL_REQUESTS)
            .map(|index| fs.read_chunk(handle, start + index * u64::from(chunk), chunk));
        for reply in future::join_all(reads).await {
            match reply? {
                ReadChunk::Eof => return Ok(contents),
                ReadChunk::Data(data) => {
                    let short = data.len() < chunk as usize;
                    contents.extend_from_slice(&data);
                    if contents.len() as u64 > MAX_FILE_BYTES {
                        return Err(too_large(contents.len() as u64, path));
                    }
                    if short {
                        break;
                    }
                }
            }
        }
    }
}

/// Writes in place, so the file keeps its permissions, owner and links.
async fn write_remote(fs: &RemoteFs, path: &str, contents: &[u8]) -> AppResult<()> {
    let handle = fs.open_for_write(path, true, None).await?;
    let chunk = fs.write_size(CHUNK_BYTES) as usize;
    let pieces: Vec<(u64, Vec<u8>)> = contents
        .chunks(chunk)
        .enumerate()
        .map(|(index, piece)| ((index * chunk) as u64, piece.to_vec()))
        .collect();
    let written: Vec<AppResult<()>> = stream::iter(pieces)
        .map(|(offset, piece)| fs.write_chunk(&handle, offset, piece))
        .buffer_unordered(PARALLEL_REQUESTS as usize)
        .collect()
        .await;
    let closed = fs.close_handle(handle).await;
    written
        .into_iter()
        .collect::<AppResult<()>>()
        .map_err(|error| error.with_path(path))?;
    closed.map_err(|error| error.with_path(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_files_save_in_place_and_detect_outside_changes() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("app.conf");
        let path = path.to_str().unwrap();
        std::fs::write(path, "port = 80\n").unwrap();

        let (bytes, stamp) = read_local(path).unwrap();
        assert_eq!(bytes, b"port = 80\n");

        let saved = save_local(path, b"port = 8080\n", Some(stamp)).unwrap();
        let SaveOutcome::Saved { stamp: after_save } = saved else {
            panic!("expected the save to go through");
        };
        assert_eq!(std::fs::read(path).unwrap(), b"port = 8080\n");
        assert_eq!(after_save.size, 12);

        std::fs::write(path, "changed elsewhere, longer\n").unwrap();
        let outcome = save_local(path, b"mine\n", Some(after_save)).unwrap();
        assert!(matches!(outcome, SaveOutcome::Changed { current: Some(_) }));
        assert_eq!(std::fs::read(path).unwrap(), b"changed elsewhere, longer\n");

        let forced = save_local(path, b"mine\n", None).unwrap();
        assert!(matches!(forced, SaveOutcome::Saved { .. }));
        assert_eq!(std::fs::read(path).unwrap(), b"mine\n");
    }

    #[test]
    fn a_removed_file_is_reported_and_can_be_recreated() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("gone.txt");
        let path = path.to_str().unwrap();
        std::fs::write(path, "x").unwrap();
        let (_, stamp) = read_local(path).unwrap();
        std::fs::remove_file(path).unwrap();
        let outcome = save_local(path, b"y", Some(stamp)).unwrap();
        assert!(matches!(outcome, SaveOutcome::Changed { current: None }));
        assert!(matches!(
            save_local(path, b"y", None).unwrap(),
            SaveOutcome::Saved { .. }
        ));
    }

    #[test]
    fn folders_and_oversized_files_are_refused() {
        let folder = tempfile::tempdir().unwrap();
        assert!(read_local(folder.path().to_str().unwrap()).is_err());
        let big = folder.path().join("big.log");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        let error = read_local(big.to_str().unwrap()).unwrap_err();
        assert!(error.message.contains("more than"));
    }
}
