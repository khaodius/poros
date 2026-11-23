//! Reads part of a file for the preview pane: a bounded range of a file on this computer or on
//! a server of any kind, vaults included, as raw bytes. A read never returns more than
//! `MAX_READ_BYTES` whatever it is asked for, reads only the range it needs from the server,
//! and stops when the page cancels it, leaving the connection ready for the next request. On
//! FTP, where one control connection serves every request, a preview never queues behind a
//! transfer, which keeps the connection for a whole file: it waits a few seconds at most for
//! anything else holding it, such as the listing of a folder, and answers busy past that.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::OwnedMutexGuard;
use tokio_util::sync::CancellationToken;

use crate::editor::{self, FileLocation, FileStamp};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::ftp;
use crate::model::DirListing;
use crate::protocol::RemoteFileSystem;
use crate::session::SessionManager;

/// The most one read returns, whatever the page asks for.
pub const MAX_READ_BYTES: u64 = 64 * 1024 * 1024;
/// Bytes before the contents in a reply framed for the page: the file's size, then its time.
pub const FRAME_HEADER_BYTES: usize = 16;
/// Stands for a file whose time is not known in a frame's header.
const UNKNOWN_TIME: i64 = i64::MIN;
/// A local file is read in pieces this large, so a cancelled read of a slow disk stops soon.
const LOCAL_PIECE_BYTES: u64 = 1024 * 1024;
/// How long a cancellation that arrived before its read is remembered.
const EARLY_CANCEL_LIFETIME: Duration = Duration::from_secs(60);
const MAX_REQUEST_ID_BYTES: usize = 64;
/// How long a preview's request waits for an FTP connection something other than transfers
/// holds, such as the listing of a folder, before it answers busy.
const CONNECTION_PATIENCE: Duration = Duration::from_secs(4);
/// How often a preview that waits for an FTP connection looks whether transfers took it.
const LENT_CHECK_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadRequest {
    /// Names this read for cancelling.
    pub request_id: String,
    pub location: FileLocation,
    /// Where in the file reading starts.
    pub offset: u64,
    /// How many bytes to read at most. Fewer come back where the file ends, or the bound does.
    pub length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadReply {
    /// The file as it was when read, which may not be how its folder listed it.
    pub stamp: FileStamp,
    pub contents: Vec<u8>,
}

impl ReadReply {
    /// The reply as one block of bytes for the page: the file's size and its modification time
    /// in milliseconds, both as 64-bit little-endian integers, then the contents.
    pub fn into_frame(self) -> Vec<u8> {
        let mut frame = Vec::with_capacity(FRAME_HEADER_BYTES + self.contents.len());
        frame.extend_from_slice(&self.stamp.size.to_le_bytes());
        frame.extend_from_slice(&self.stamp.modified.unwrap_or(UNKNOWN_TIME).to_le_bytes());
        frame.extend_from_slice(&self.contents);
        frame
    }
}

enum Slot {
    Running(CancellationToken),
    /// Cancelled before the read it names had started.
    CancelledEarly(Instant),
}

/// Names a read: the window that asked for it, and the name that window gave it. Windows make
/// up their names on their own, so one window never cancels another's read.
type ReadKey = (String, String);

/// The previews of one FTP connection wait here for each other, so that only something else
/// holding the connection counts against how long a preview waits for it.
type Turn = Arc<tokio::sync::Mutex<()>>;

pub struct FilePreviews {
    sessions: Arc<SessionManager>,
    reads: Mutex<HashMap<ReadKey, Slot>>,
    /// By connection identity.
    turns: Mutex<HashMap<usize, Turn>>,
    patience: Duration,
}

/// A read under way: forgotten once it ends.
struct Running<'a> {
    previews: &'a FilePreviews,
    key: ReadKey,
    cancel: CancellationToken,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.previews.reads.lock().unwrap().remove(&self.key);
    }
}

impl FilePreviews {
    pub fn new(sessions: Arc<SessionManager>) -> Self {
        Self {
            sessions,
            reads: Mutex::new(HashMap::new()),
            turns: Mutex::new(HashMap::new()),
            patience: CONNECTION_PATIENCE,
        }
    }

    /// Reads the range asked for by the window labelled `owner`. `lent` tells whether transfers
    /// are using the connection a server's file would be read over, in which case the read is
    /// refused at once instead of waiting for them or making them wait.
    pub async fn read(
        &self,
        owner: &str,
        request: ReadRequest,
        lent: impl Fn(&dyn RemoteFileSystem) -> bool,
    ) -> AppResult<ReadReply> {
        validate_request_id(&request.request_id)?;
        let running = self.begin(owner, &request.request_id);
        let length = request.length.min(MAX_READ_BYTES);
        match request.location {
            FileLocation::Local { path } => {
                validate_local_path(&path)?;
                let cancel = running.cancel.clone();
                tokio::task::spawn_blocking(move || {
                    read_local(&path, request.offset, length, &cancel)
                })
                .await?
            }
            FileLocation::Remote { session_id, path } => {
                validate_remote_path(&path)?;
                let files = self.sessions.get(&session_id).await?.files();
                let range = (request.offset, length);
                self.read_server(files, &path, range, &running.cancel, lent)
                    .await
            }
        }
    }

    /// Reads from a server, on the terms of `read`.
    async fn read_server(
        &self,
        files: Arc<dyn RemoteFileSystem>,
        path: &str,
        (offset, length): (u64, u64),
        cancel: &CancellationToken,
        lent: impl Fn(&dyn RemoteFileSystem) -> bool,
    ) -> AppResult<ReadReply> {
        if lent(files.as_ref()) {
            return Err(lent_to_transfers());
        }
        let _turn = self.turn(files.as_ref(), Some(cancel)).await?;
        let read = read_remote(files.clone(), path, offset, length, cancel);
        self.patiently(files.as_ref(), cancel, lent, read).await
    }

    /// Lists a folder on a server for the summary of what it holds, on the same terms as a
    /// read: not while transfers are using the connection.
    pub async fn list(
        &self,
        session_id: &str,
        path: &str,
        lent: impl Fn(&dyn RemoteFileSystem) -> bool,
    ) -> AppResult<DirListing> {
        validate_remote_path(path)?;
        let files = self.sessions.get(session_id).await?.files();
        if lent(files.as_ref()) {
            return Err(lent_to_transfers());
        }
        let _turn = self.turn(files.as_ref(), None).await?;
        let never = CancellationToken::new();
        self.patiently(files.as_ref(), &never, lent, files.list_dir(path))
            .await
    }

    /// Runs a preview's `requests` so that on FTP each waits for the connection only so long,
    /// and not at all once transfers are seen to take it.
    async fn patiently<T>(
        &self,
        files: &dyn RemoteFileSystem,
        cancel: &CancellationToken,
        lent: impl Fn(&dyn RemoteFileSystem) -> bool,
        requests: impl std::future::Future<Output = AppResult<T>>,
    ) -> AppResult<T> {
        if !files.transport().is_ftp() {
            return requests.await;
        }
        let patience = ftp::Patience {
            bound: self.patience,
            cancel: cancel.clone(),
            give_up: CancellationToken::new(),
        };
        let give_up = patience.give_up.clone();
        let requests = ftp::waiting_briefly(patience, requests);
        tokio::pin!(requests);
        let watch = async {
            while !lent(files) {
                tokio::time::sleep(LENT_CHECK_EVERY).await;
            }
            give_up.cancel();
        };
        // The requests always run to their end, so none stops half done on the connection.
        let result = tokio::select! {
            result = &mut requests => result,
            () = watch => requests.await,
        };
        busy_as_lent(result, files, &lent)
    }

    /// Stops the read named `request_id` by the window labelled `owner`, also when it has not
    /// started yet.
    pub fn cancel(&self, owner: &str, request_id: &str) {
        if validate_request_id(request_id).is_err() {
            return;
        }
        let key = (owner.to_string(), request_id.to_string());
        let mut reads = self.reads.lock().unwrap();
        forget_old_cancellations(&mut reads);
        match reads.get(&key) {
            Some(Slot::Running(cancel)) => cancel.cancel(),
            Some(Slot::CancelledEarly(_)) => {}
            None => {
                reads.insert(key, Slot::CancelledEarly(Instant::now()));
            }
        }
    }

    fn begin(&self, owner: &str, request_id: &str) -> Running<'_> {
        let key = (owner.to_string(), request_id.to_string());
        let cancel = CancellationToken::new();
        let mut reads = self.reads.lock().unwrap();
        forget_old_cancellations(&mut reads);
        if let Some(Slot::CancelledEarly(_)) = reads.get(&key) {
            cancel.cancel();
        }
        reads.insert(key.clone(), Slot::Running(cancel.clone()));
        Running {
            previews: self,
            key,
            cancel,
        }
    }

    /// Waits for the other previews of an FTP connection to finish with it, or for `cancel`.
    /// Other protocols serve requests side by side, so their previews go straight ahead.
    async fn turn(
        &self,
        files: &dyn RemoteFileSystem,
        cancel: Option<&CancellationToken>,
    ) -> AppResult<Option<OwnedMutexGuard<()>>> {
        if !files.transport().is_ftp() {
            return Ok(None);
        }
        let turn = {
            let mut turns = self.turns.lock().unwrap();
            // Those of connections nobody previews from any more.
            turns.retain(|_, turn| Arc::strong_count(turn) > 1);
            turns
                .entry(files.connection_identity())
                .or_default()
                .clone()
        };
        let Some(cancel) = cancel else {
            return Ok(Some(turn.lock_owned().await));
        };
        // Nothing is sent while waiting here, so stopping the wait leaves nothing half done.
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(AppError::cancelled()),
            taken = turn.lock_owned() => Ok(Some(taken)),
        }
    }
}

/// A preview that found the connection busy is told why when it is transfers.
fn busy_as_lent<T>(
    result: AppResult<T>,
    files: &dyn RemoteFileSystem,
    lent: impl Fn(&dyn RemoteFileSystem) -> bool,
) -> AppResult<T> {
    match result {
        Err(error) if error.kind == ErrorKind::Busy && lent(files) => Err(lent_to_transfers()),
        other => other,
    }
}

fn forget_old_cancellations(reads: &mut HashMap<ReadKey, Slot>) {
    reads.retain(|_, slot| match slot {
        Slot::Running(_) => true,
        Slot::CancelledEarly(since) => since.elapsed() < EARLY_CANCEL_LIFETIME,
    });
}

/// The page makes these up; a short one is all a read needs to be told from the others.
fn validate_request_id(request_id: &str) -> AppResult<()> {
    if request_id.is_empty() || request_id.len() > MAX_REQUEST_ID_BYTES {
        return Err(AppError::invalid("This is not the name of a preview read"));
    }
    Ok(())
}

fn lent_to_transfers() -> AppError {
    AppError::new(ErrorKind::Lent, "A transfer is using this connection")
}

fn not_a_file(path: &str) -> AppError {
    AppError::invalid("Only files can be previewed").with_path(path)
}

/// The page names files by the paths their folder listed, which are absolute.
fn validate_local_path(path: &str) -> AppResult<()> {
    if path.contains('\0') || !Path::new(path).is_absolute() {
        return Err(AppError::invalid("This is not the path of a file").with_path(path));
    }
    Ok(())
}

/// A server's listing never holds a relative path, nor one that climbs out of its folder.
fn validate_remote_path(path: &str) -> AppResult<()> {
    let climbs = path.split('/').any(|part| part == "." || part == "..");
    if !path.starts_with('/') || path.contains('\0') || climbs {
        return Err(AppError::invalid("This is not the path of a file").with_path(path));
    }
    Ok(())
}

/// The part of a file `length` bytes long from `offset` that the file has.
fn range_within(size: u64, offset: u64, length: u64) -> std::ops::Range<u64> {
    let end = size.min(offset.saturating_add(length.min(MAX_READ_BYTES)));
    offset.min(end)..end
}

fn read_local(
    path: &str,
    offset: u64,
    length: u64,
    cancel: &CancellationToken,
) -> AppResult<ReadReply> {
    let with_path = |error: std::io::Error| AppError::from(error).with_path(path);
    // Asked before opening: opening a pipe would wait for someone to write to it.
    if !std::fs::metadata(path).map_err(with_path)?.is_file() {
        return Err(not_a_file(path));
    }
    let mut file = std::fs::File::open(path).map_err(with_path)?;
    let metadata = file.metadata().map_err(with_path)?;
    if !metadata.is_file() {
        return Err(not_a_file(path));
    }
    let stamp = editor::local_stamp(&metadata);
    let range = range_within(stamp.size, offset, length);
    let wanted = range.end - range.start;
    let mut contents = Vec::with_capacity(wanted as usize);
    if wanted > 0 {
        file.seek(SeekFrom::Start(range.start)).map_err(with_path)?;
    }
    while (contents.len() as u64) < wanted {
        if cancel.is_cancelled() {
            return Err(AppError::cancelled());
        }
        let piece = LOCAL_PIECE_BYTES.min(wanted - contents.len() as u64);
        let read = (&mut file)
            .take(piece)
            .read_to_end(&mut contents)
            .map_err(with_path)?;
        // Shorter than when it was opened.
        if read == 0 {
            break;
        }
    }
    Ok(ReadReply { stamp, contents })
}

/// Reads over `files`, whatever the protocol behind it. Nothing waits half done on the
/// connection when the read is cancelled: the stream is always ended properly.
pub async fn read_remote(
    files: Arc<dyn RemoteFileSystem>,
    path: &str,
    offset: u64,
    length: u64,
    cancel: &CancellationToken,
) -> AppResult<ReadReply> {
    if cancel.is_cancelled() {
        return Err(AppError::cancelled());
    }
    let stat = files
        .stat(path)
        .await
        .map_err(|error| error.with_path(path))?
        .ok_or_else(|| {
            AppError::new(ErrorKind::NotFound, "The file no longer exists").with_path(path)
        })?;
    if stat.is_dir {
        return Err(not_a_file(path));
    }
    let stamp = editor::remote_stamp(stat);
    let range = range_within(stamp.size, offset, length);
    let wanted = (range.end - range.start) as usize;
    if cancel.is_cancelled() {
        return Err(AppError::cancelled());
    }
    if wanted == 0 {
        return Ok(ReadReply {
            stamp,
            contents: Vec::new(),
        });
    }
    let mut reader = files
        .open_read(path, range)
        .await
        .map_err(|error| error.with_path(path))?;
    let mut contents = Vec::with_capacity(wanted);
    let read = async {
        while contents.len() < wanted {
            let chunk = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(AppError::cancelled()),
                chunk = reader.next_chunk() => chunk?,
            };
            let Some(data) = chunk else { break };
            // A server may send more than the range asked for.
            let room = wanted - contents.len();
            contents.extend_from_slice(&data[..data.len().min(room)]);
        }
        Ok(())
    }
    .await;
    let finished = reader.finish().await;
    if cancel.is_cancelled() {
        return Err(AppError::cancelled());
    }
    read.and(finished).map_err(|error| error.with_path(path))?;
    Ok(ReadReply { stamp, contents })
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use async_trait::async_trait;
    use bytes::Bytes;

    use super::*;
    use crate::events::Events;
    use crate::protocol::{
        Capabilities, Protocol, ReadStream, RemoteStat, WriteRequest, WriteStream,
    };

    fn previews() -> (tempfile::TempDir, FilePreviews) {
        let folder = tempfile::tempdir().unwrap();
        let sessions = Arc::new(SessionManager::new(
            folder.path().join("known_hosts"),
            Events::default(),
        ));
        (folder, FilePreviews::new(sessions))
    }

    fn local_request(id: &str, path: &Path, offset: u64, length: u64) -> ReadRequest {
        ReadRequest {
            request_id: id.to_string(),
            location: FileLocation::Local {
                path: path.to_str().unwrap().to_string(),
            },
            offset,
            length,
        }
    }

    const WINDOW: &str = "main";

    fn never_lent(_files: &dyn RemoteFileSystem) -> bool {
        false
    }

    #[tokio::test]
    async fn a_local_read_returns_the_range_and_the_file_as_it_is() {
        let (folder, previews) = previews();
        let path = folder.path().join("notes.txt");
        std::fs::write(&path, b"0123456789").unwrap();

        let start = previews
            .read(WINDOW, local_request("a", &path, 0, 4), never_lent)
            .await
            .unwrap();
        assert_eq!(start.contents, b"0123");
        assert_eq!(start.stamp.size, 10);
        assert!(start.stamp.modified.is_some());

        let middle = previews
            .read(WINDOW, local_request("b", &path, 3, 4), never_lent)
            .await
            .unwrap();
        assert_eq!(middle.contents, b"3456");

        let past_the_end = previews
            .read(WINDOW, local_request("c", &path, 8, 100), never_lent)
            .await
            .unwrap();
        assert_eq!(past_the_end.contents, b"89");

        let beyond = previews
            .read(WINDOW, local_request("d", &path, 50, 10), never_lent)
            .await
            .unwrap();
        assert!(beyond.contents.is_empty());
        assert_eq!(beyond.stamp.size, 10);
        assert!(previews.reads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_empty_local_file_reads_as_nothing() {
        let (folder, previews) = previews();
        let path = folder.path().join("empty.log");
        std::fs::write(&path, b"").unwrap();
        let reply = previews
            .read(WINDOW, local_request("a", &path, 0, 1024), never_lent)
            .await
            .unwrap();
        assert!(reply.contents.is_empty());
        assert_eq!(reply.stamp.size, 0);
    }

    #[tokio::test]
    async fn folders_missing_files_and_relative_paths_are_refused() {
        let (folder, previews) = previews();
        let directory = previews
            .read(WINDOW, local_request("a", folder.path(), 0, 16), never_lent)
            .await
            .unwrap_err();
        assert_eq!(directory.kind, ErrorKind::InvalidInput);

        let missing = previews
            .read(
                WINDOW,
                local_request("b", &folder.path().join("gone.png"), 0, 16),
                never_lent,
            )
            .await
            .unwrap_err();
        assert_eq!(missing.kind, ErrorKind::NotFound);

        let relative = previews
            .read(
                WINDOW,
                local_request("c", Path::new("notes.txt"), 0, 16),
                never_lent,
            )
            .await
            .unwrap_err();
        assert_eq!(relative.kind, ErrorKind::InvalidInput);
    }

    #[test]
    fn no_read_is_longer_than_the_bound() {
        let huge = 10 * MAX_READ_BYTES;
        assert_eq!(range_within(huge, 0, u64::MAX), 0..MAX_READ_BYTES);
        assert_eq!(range_within(huge, 5, huge), 5..5 + MAX_READ_BYTES);
        assert_eq!(range_within(100, u64::MAX, u64::MAX), 100..100);
        assert_eq!(range_within(100, 40, 20), 40..60);
        assert_eq!(range_within(100, 90, 20), 90..100);
    }

    #[tokio::test]
    async fn a_local_read_stops_at_the_bound() {
        let (folder, previews) = previews();
        let path = folder.path().join("large.bin");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_READ_BYTES + 4096).unwrap();
        drop(file);
        let reply = previews
            .read(WINDOW, local_request("a", &path, 0, u64::MAX), never_lent)
            .await
            .unwrap();
        assert_eq!(reply.contents.len() as u64, MAX_READ_BYTES);
        assert_eq!(reply.stamp.size, MAX_READ_BYTES + 4096);
    }

    #[tokio::test]
    async fn a_read_cancelled_before_it_starts_reads_nothing() {
        let (folder, previews) = previews();
        let path = folder.path().join("photo.jpg");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        previews.cancel(WINDOW, "early");
        let error = previews
            .read(WINDOW, local_request("early", &path, 0, 4096), never_lent)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Cancelled);
        // The same name reads again afterwards.
        let reply = previews
            .read(WINDOW, local_request("early", &path, 0, 4096), never_lent)
            .await
            .unwrap();
        assert_eq!(reply.contents.len(), 4096);
    }

    #[tokio::test]
    async fn a_window_cancels_only_its_own_reads() {
        let (folder, previews) = previews();
        let path = folder.path().join("photo.jpg");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        // Another window gave a read of its own the same name.
        previews.cancel("tab-window", "7");
        let reply = previews
            .read(WINDOW, local_request("7", &path, 0, 4096), never_lent)
            .await
            .unwrap();
        assert_eq!(reply.contents.len(), 4096);
        let theirs = previews
            .read("tab-window", local_request("7", &path, 0, 4096), never_lent)
            .await
            .unwrap_err();
        assert_eq!(theirs.kind, ErrorKind::Cancelled);
    }

    #[test]
    fn cancellations_of_reads_that_never_start_are_forgotten() {
        let (_folder, previews) = previews();
        previews.cancel(WINDOW, "never-read");
        let key = (WINDOW.to_string(), "never-read".to_string());
        if let Some(Slot::CancelledEarly(since)) = previews.reads.lock().unwrap().get_mut(&key) {
            *since = Instant::now() - EARLY_CANCEL_LIFETIME - Duration::from_secs(1);
        }
        drop(previews.begin(WINDOW, "another"));
        assert!(previews.reads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reads_need_a_short_name() {
        let (folder, previews) = previews();
        let path = folder.path().join("notes.txt");
        std::fs::write(&path, b"text").unwrap();
        for refused in [String::new(), "x".repeat(MAX_REQUEST_ID_BYTES + 1)] {
            let error = previews
                .read(WINDOW, local_request(&refused, &path, 0, 4), never_lent)
                .await
                .unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidInput);
            previews.cancel(WINDOW, &refused);
        }
        assert!(previews.reads.lock().unwrap().is_empty());
    }

    #[test]
    fn frames_carry_the_stamp_before_the_contents() {
        let frame = ReadReply {
            stamp: FileStamp {
                size: 0x0102_0304_0506,
                modified: Some(1_700_000_000_123),
            },
            contents: b"PNG".to_vec(),
        }
        .into_frame();
        assert_eq!(frame.len(), FRAME_HEADER_BYTES + 3);
        assert_eq!(
            u64::from_le_bytes(frame[..8].try_into().unwrap()),
            0x0102_0304_0506
        );
        assert_eq!(
            i64::from_le_bytes(frame[8..16].try_into().unwrap()),
            1_700_000_000_123
        );
        assert_eq!(&frame[16..], b"PNG");

        let untimed = ReadReply {
            stamp: FileStamp {
                size: 1,
                modified: None,
            },
            contents: Vec::new(),
        }
        .into_frame();
        assert_eq!(
            i64::from_le_bytes(untimed[8..16].try_into().unwrap()),
            i64::MIN
        );
    }

    #[test]
    fn server_paths_must_be_absolute_and_plain() {
        assert!(validate_remote_path("/var/www/logo.svg").is_ok());
        assert!(validate_remote_path("/").is_ok());
        for refused in [
            "",
            "logo.svg",
            "~/logo.svg",
            "/var/../etc/passwd",
            "/var/./x",
            "/a\0b",
        ] {
            assert!(validate_remote_path(refused).is_err(), "{refused:?}");
        }
    }

    /// A server holding one file, which sends it in small pieces and counts what it is asked.
    struct OneFile {
        protocol: Protocol,
        contents: Vec<u8>,
        /// Sends this many bytes more than each read asks for.
        overshoot: usize,
        stats: AtomicUsize,
        /// Each `stat` waits for a permit, like a server slow to answer.
        stat_gate: Option<tokio::sync::Semaphore>,
        opened: AtomicUsize,
        finished: Arc<AtomicBool>,
        /// Cancels the read after its first piece.
        cancel_after_first: Option<CancellationToken>,
        /// Each `stat` takes this as a request over FTP takes the control connection.
        connection: Option<Arc<tokio::sync::Mutex<()>>>,
    }

    impl OneFile {
        fn holding(contents: &[u8]) -> Self {
            Self {
                protocol: Protocol::Sftp,
                contents: contents.to_vec(),
                overshoot: 0,
                stats: AtomicUsize::new(0),
                stat_gate: None,
                opened: AtomicUsize::new(0),
                finished: Arc::new(AtomicBool::new(false)),
                cancel_after_first: None,
                connection: None,
            }
        }

        /// Over FTP, with a control connection something else can hold.
        fn over_ftp(contents: &[u8]) -> (Arc<Self>, Arc<tokio::sync::Mutex<()>>) {
            let connection = Arc::new(tokio::sync::Mutex::new(()));
            let mut server = Self::holding(contents);
            server.protocol = Protocol::Ftp;
            server.connection = Some(connection.clone());
            (Arc::new(server), connection)
        }
    }

    struct Pieces {
        remaining: Vec<u8>,
        finished: Arc<AtomicBool>,
        cancel_after_first: Option<CancellationToken>,
    }

    #[async_trait]
    impl ReadStream for Pieces {
        async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
            if self.remaining.is_empty() {
                return Ok(None);
            }
            let piece: Vec<u8> = self
                .remaining
                .drain(..self.remaining.len().min(3))
                .collect();
            if let Some(cancel) = self.cancel_after_first.take() {
                cancel.cancel();
            }
            Ok(Some(Bytes::from(piece)))
        }

        async fn finish(self: Box<Self>) -> AppResult<()> {
            self.finished.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn not_in_this_test<T>() -> AppResult<T> {
        Err(AppError::unsupported("Not part of this test"))
    }

    #[async_trait]
    impl RemoteFileSystem for OneFile {
        fn protocol(&self) -> Protocol {
            self.protocol
        }

        fn home(&self) -> &str {
            "/"
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }

        async fn canonicalize(&self, _path: &str) -> AppResult<String> {
            not_in_this_test()
        }

        async fn list_dir(&self, _path: &str) -> AppResult<DirListing> {
            not_in_this_test()
        }

        async fn make_dir(&self, _parent: &str, _name: &str) -> AppResult<String> {
            not_in_this_test()
        }

        async fn rename(&self, _path: &str, _new_name: &str) -> AppResult<String> {
            not_in_this_test()
        }

        async fn delete(&self, _paths: &[String]) -> AppResult<()> {
            not_in_this_test()
        }

        async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
            let _taken = match &self.connection {
                Some(connection) => Some(ftp::take_connection(connection.clone()).await?),
                None => None,
            };
            self.stats.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.stat_gate {
                gate.acquire().await.unwrap().forget();
            }
            Ok(match path {
                "/file" => Some(RemoteStat {
                    is_dir: false,
                    size: self.contents.len() as u64,
                    modified: Some(1_700_000_000),
                    permissions: None,
                }),
                "/folder" => Some(RemoteStat {
                    is_dir: true,
                    size: 0,
                    modified: None,
                    permissions: None,
                }),
                _ => None,
            })
        }

        async fn ensure_dir(&self, _path: &str) -> AppResult<()> {
            not_in_this_test()
        }

        async fn set_attributes(
            &self,
            _path: &str,
            _modified: Option<i64>,
            _permissions: Option<u32>,
        ) -> AppResult<()> {
            not_in_this_test()
        }

        async fn open_read(
            self: Arc<Self>,
            _path: &str,
            range: Range<u64>,
        ) -> AppResult<Box<dyn ReadStream>> {
            self.opened.fetch_add(1, Ordering::SeqCst);
            let end = (range.end as usize + self.overshoot).min(self.contents.len());
            Ok(Box::new(Pieces {
                remaining: self.contents[range.start as usize..end].to_vec(),
                finished: self.finished.clone(),
                cancel_after_first: self.cancel_after_first.clone(),
            }))
        }

        async fn open_write(
            self: Arc<Self>,
            _request: WriteRequest,
        ) -> AppResult<Box<dyn WriteStream>> {
            not_in_this_test()
        }
    }

    #[tokio::test]
    async fn a_server_read_asks_only_for_the_range_the_file_has() {
        let server = Arc::new(OneFile::holding(b"The quick brown fox"));
        let cancel = CancellationToken::new();
        let reply = read_remote(server.clone(), "/file", 4, 5, &cancel)
            .await
            .unwrap();
        assert_eq!(reply.contents, b"quick");
        assert_eq!(reply.stamp.size, 19);
        assert_eq!(reply.stamp.modified, Some(1_700_000_000_000));
        assert!(server.finished.load(Ordering::SeqCst));

        let tail = read_remote(server.clone(), "/file", 16, 100, &cancel)
            .await
            .unwrap();
        assert_eq!(tail.contents, b"fox");

        // Nothing is opened for a range the file does not have.
        let opened = server.opened.load(Ordering::SeqCst);
        let beyond = read_remote(server.clone(), "/file", 40, 10, &cancel)
            .await
            .unwrap();
        assert!(beyond.contents.is_empty());
        assert_eq!(server.opened.load(Ordering::SeqCst), opened);
    }

    #[tokio::test]
    async fn a_server_that_sends_too_much_is_cut_at_what_was_asked() {
        let mut server = OneFile::holding(b"0123456789abcdef");
        server.overshoot = 6;
        let reply = read_remote(Arc::new(server), "/file", 0, 8, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(reply.contents, b"01234567");
    }

    #[tokio::test]
    async fn a_cancelled_server_read_still_ends_its_stream() {
        let cancel = CancellationToken::new();
        let mut server = OneFile::holding(&[1u8; 64]);
        server.cancel_after_first = Some(cancel.clone());
        let finished = server.finished.clone();
        let error = read_remote(Arc::new(server), "/file", 0, 64, &cancel)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Cancelled);
        assert!(finished.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn folders_and_missing_files_on_a_server_are_refused() {
        let server = Arc::new(OneFile::holding(b"x"));
        let cancel = CancellationToken::new();
        let folder = read_remote(server.clone(), "/folder", 0, 8, &cancel)
            .await
            .unwrap_err();
        assert_eq!(folder.kind, ErrorKind::InvalidInput);
        let missing = read_remote(server.clone(), "/gone", 0, 8, &cancel)
            .await
            .unwrap_err();
        assert_eq!(missing.kind, ErrorKind::NotFound);
        assert_eq!(missing.path.as_deref(), Some("/gone"));
        assert_eq!(server.opened.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn previews_of_one_ftp_connection_take_turns_and_stop_waiting_when_cancelled() {
        let (_folder, previews) = previews();
        let mut server = OneFile::holding(b"abc");
        server.protocol = Protocol::Ftp;
        server.stat_gate = Some(tokio::sync::Semaphore::new(0));
        let server = Arc::new(server);
        let (first_cancel, second_cancel) = (CancellationToken::new(), CancellationToken::new());
        let first =
            previews.read_server(server.clone(), "/file", (0, 3), &first_cancel, never_lent);
        let second =
            previews.read_server(server.clone(), "/file", (0, 3), &second_cancel, never_lent);
        tokio::pin!(first, second);

        // The first waits for the server's answer, the second for the first.
        assert!(futures::poll!(first.as_mut()).is_pending());
        assert!(futures::poll!(second.as_mut()).is_pending());
        assert_eq!(server.stats.load(Ordering::SeqCst), 1);

        second_cancel.cancel();
        assert_eq!(second.await.unwrap_err().kind, ErrorKind::Cancelled);
        server.stat_gate.as_ref().unwrap().add_permits(1);
        assert_eq!(first.await.unwrap().contents, b"abc");
        // The cancelled one never asked the server anything.
        assert_eq!(server.stats.load(Ordering::SeqCst), 1);
        assert!(previews
            .turns
            .lock()
            .unwrap()
            .values()
            .all(|turn| turn.try_lock().is_ok()));
    }

    #[tokio::test]
    async fn previews_on_other_protocols_do_not_wait_for_each_other() {
        let (_folder, previews) = previews();
        let mut server = OneFile::holding(b"abc");
        server.stat_gate = Some(tokio::sync::Semaphore::new(0));
        let server = Arc::new(server);
        let cancel = CancellationToken::new();
        let first = previews.read_server(server.clone(), "/file", (0, 3), &cancel, never_lent);
        let second = previews.read_server(server.clone(), "/file", (0, 3), &cancel, never_lent);
        tokio::pin!(first, second);
        assert!(futures::poll!(first.as_mut()).is_pending());
        assert!(futures::poll!(second.as_mut()).is_pending());
        assert_eq!(server.stats.load(Ordering::SeqCst), 2);
        server.stat_gate.as_ref().unwrap().add_permits(2);
        assert!(first.await.is_ok() && second.await.is_ok());
    }

    #[tokio::test]
    async fn a_preview_waits_for_an_ftp_connection_another_request_holds() {
        let (_folder, previews) = previews();
        let (server, connection) = OneFile::over_ftp(b"abc");
        let held = connection.clone().lock_owned().await;
        let cancel = CancellationToken::new();
        let read = previews.read_server(server.clone(), "/file", (0, 3), &cancel, never_lent);
        tokio::pin!(read);
        assert!(futures::poll!(read.as_mut()).is_pending());
        assert_eq!(server.stats.load(Ordering::SeqCst), 0);
        drop(held);
        assert_eq!(read.await.unwrap().contents, b"abc");
    }

    #[tokio::test]
    async fn a_preview_answers_busy_once_it_has_waited_long_enough() {
        let (_folder, mut previews) = previews();
        previews.patience = Duration::from_millis(30);
        let (server, connection) = OneFile::over_ftp(b"abc");
        let _held = connection.clone().lock_owned().await;
        let cancel = CancellationToken::new();
        let error = previews
            .read_server(server.clone(), "/file", (0, 3), &cancel, never_lent)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Busy);
        assert_eq!(error.message, "Another request is using this connection");
        assert_eq!(server.stats.load(Ordering::SeqCst), 0);
        let listed = previews
            .patiently(server.as_ref(), &cancel, never_lent, server.stat("/file"))
            .await;
        assert_eq!(listed.unwrap_err().kind, ErrorKind::Busy);
    }

    #[tokio::test]
    async fn a_waiting_preview_gives_up_at_once_when_transfers_take_the_connection() {
        let (_folder, previews) = previews();
        let (server, connection) = OneFile::over_ftp(b"abc");
        let _held = connection.clone().lock_owned().await;
        let transfers_started = AtomicBool::new(false);
        let lent = |_: &dyn RemoteFileSystem| transfers_started.load(Ordering::SeqCst);
        let cancel = CancellationToken::new();
        let read = previews.read_server(server.clone(), "/file", (0, 3), &cancel, lent);
        tokio::pin!(read);
        assert!(futures::poll!(read.as_mut()).is_pending());
        transfers_started.store(true, Ordering::SeqCst);
        let error = tokio::time::timeout(Duration::from_secs(2), read)
            .await
            .expect("the preview kept waiting")
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Lent);
        assert_eq!(error.message, lent_to_transfers().message);
        assert_eq!(server.stats.load(Ordering::SeqCst), 0);

        // Once transfers have it, a preview is refused before it waits at all.
        let refused = previews
            .read_server(server.clone(), "/file", (0, 3), &cancel, lent)
            .await
            .unwrap_err();
        assert_eq!(refused.kind, ErrorKind::Lent);
    }

    #[tokio::test]
    async fn a_preview_waiting_for_the_ftp_connection_stops_when_cancelled() {
        let (_folder, previews) = previews();
        let (server, connection) = OneFile::over_ftp(b"abc");
        let held = connection.clone().lock_owned().await;
        let cancel = CancellationToken::new();
        let read = previews.read_server(server.clone(), "/file", (0, 3), &cancel, never_lent);
        tokio::pin!(read);
        assert!(futures::poll!(read.as_mut()).is_pending());
        cancel.cancel();
        assert_eq!(read.await.unwrap_err().kind, ErrorKind::Cancelled);
        drop(held);
        assert!(connection.try_lock().is_ok());
    }

    #[test]
    fn a_connection_held_by_transfers_is_named_as_such() {
        let server = OneFile::holding(b"x");
        let held = || -> AppResult<()> {
            Err(AppError::new(
                ErrorKind::Busy,
                "Another request is using this connection",
            ))
        };
        let lent = busy_as_lent(held(), &server, |_| true).unwrap_err();
        assert_eq!(lent.kind, ErrorKind::Lent);
        assert_eq!(lent.message, lent_to_transfers().message);
        let other = busy_as_lent(held(), &server, never_lent).unwrap_err();
        assert_eq!(other.message, "Another request is using this connection");
        assert!(busy_as_lent(Ok(()), &server, |_| true).is_ok());
    }

    #[tokio::test]
    async fn a_session_that_is_gone_reads_nothing() {
        let (_folder, previews) = previews();
        let request = ReadRequest {
            request_id: "a".into(),
            location: FileLocation::Remote {
                session_id: "closed".into(),
                path: "/file".into(),
            },
            offset: 0,
            length: 8,
        };
        let error = previews
            .read(WINDOW, request, never_lent)
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::SessionNotFound);
    }
}
