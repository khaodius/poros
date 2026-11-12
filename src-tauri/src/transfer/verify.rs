//! The contents check after a file is complete, and the repair of the parts that differ. The
//! server hashes its copy with its own `sha256sum`, in parts where GNU `split` lets it (OpenSSH
//! servers with a shell); SFTPGo offers the whole-file command. On a server with neither,
//! Poros reads its copy back and hashes it here. Only SHA-256 and SHA-512 count: SHA-1 and MD5
//! catch accidents but can be forged.

use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::{FuturesOrdered, FuturesUnordered, StreamExt};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use super::copy::read_up_to;
use super::hash_shell;
use super::queue::Direction;
use super::worker::{JobContext, JobRef};
use crate::checksum::{hash_file, hash_file_parts, parse_output, Algorithm, PartHasher, Parts};
use crate::error::{AppError, AppResult};
use crate::events::LogLevel;
use crate::protocol::RemoteFileSystem;
use crate::rsync::{run_command, shell_quote, CommandOutput};
use crate::sftp::{ReadChunk, RemoteFs};

/// Enough for 64 parts' lines and any error the command prints.
const OUTPUT_LIMIT: usize = 128 * 1024;

/// What a server can hash, found once per server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerHashing {
    /// Its strongest whole-file command, `sha256sum` or `sha512sum`.
    whole: Option<Algorithm>,
    /// `head -c` and GNU `split --filter` beside `sha256sum`: digests of each part of a file,
    /// and of its first bytes.
    parts: bool,
}

pub enum Checked {
    Same,
    /// The parts that differ, by index; with `Parts::WHOLE` the one part is the whole file.
    Different {
        parts: Parts,
        damaged: Vec<usize>,
    },
    /// The server could not hash the file and reading it back is turned off.
    Unavailable,
}

/// The two copies of a file: the one on the server and the one on this computer.
pub(super) struct Copies<'a> {
    pub remote: &'a str,
    pub local: &'a str,
    /// Bytes compared from the start of each.
    pub length: u64,
}

enum Method {
    Command { algorithm: Algorithm, parts: Parts },
    ReadBack { parts: Parts },
}

impl Method {
    fn parts(&self) -> Parts {
        match self {
            Self::Command { parts, .. } | Self::ReadBack { parts } => *parts,
        }
    }
}

pub(super) async fn compare(context: &JobContext<'_>, copies: &Copies<'_>) -> AppResult<Checked> {
    let hashing = server_hashing(context).await;
    let method = match hashing.whole {
        Some(Algorithm::Sha256) if hashing.parts => Method::Command {
            algorithm: Algorithm::Sha256,
            parts: Parts::for_length(copies.length),
        },
        Some(algorithm) => Method::Command {
            algorithm,
            parts: Parts::WHOLE,
        },
        None if context.settings.read_back => Method::ReadBack {
            parts: Parts::for_length(copies.length),
        },
        None => return Ok(Checked::Unavailable),
    };
    let parts = method.parts();
    let local = {
        let path = PathBuf::from(copies.local);
        let length = copies.length;
        let algorithm = match &method {
            Method::Command { algorithm, .. } => *algorithm,
            Method::ReadBack { .. } => Algorithm::Sha256,
        };
        tokio::task::spawn_blocking(move || match algorithm {
            Algorithm::Sha256 => hash_file_parts(&path, length, parts),
            other => hash_file(&path, other).map(|digest| vec![digest]),
        })
    };
    let remote = async {
        match &method {
            Method::Command { algorithm, parts } => {
                command_digests(context, copies, *algorithm, *parts).await
            }
            Method::ReadBack { parts } => read_back(context, copies, *parts).await.map(Some),
        }
    };
    let (local, remote) = tokio::select! {
        biased;
        _ = context.run.cancel.cancelled() => return Err(AppError::cancelled()),
        both = async { tokio::join!(local, remote) } => both,
    };
    let local = local?.map_err(|error| AppError::from(error).with_path(copies.local))?;
    let Some(remote) = remote? else {
        return Ok(Checked::Unavailable);
    };
    let damaged = damaged_parts(&local, &remote);
    Ok(if damaged.is_empty() {
        Checked::Same
    } else {
        Checked::Different { parts, damaged }
    })
}

fn damaged_parts(local: &[Vec<u8>], remote: &[Vec<u8>]) -> Vec<usize> {
    (0..local.len().max(remote.len()))
        .filter(|index| local.get(*index) != remote.get(*index))
        .collect()
}

/// The server's digests of the file, or `None` when its command failed on this one.
async fn command_digests(
    context: &JobContext<'_>,
    copies: &Copies<'_>,
    algorithm: Algorithm,
    parts: Parts,
) -> AppResult<Option<Vec<Vec<u8>>>> {
    let count = parts.count(copies.length);
    let quoted = shell_quote(copies.remote);
    // One file per command: SFTPGo hashes only the last argument.
    let command = if count == 1 {
        format!("{} -- {quoted}", algorithm.command())
    } else {
        format!(
            "split -b {} --filter={} -- {quoted}",
            parts.size,
            algorithm.command()
        )
    };
    let output = run_hash_command(context, &command).await?;
    let text = String::from_utf8_lossy(&output.output);
    let digests: Vec<Vec<u8>> = parse_output(&text, algorithm)
        .into_iter()
        .map(|(_, digest)| digest)
        .collect();
    if output.exit_status == Some(0) && digests.len() == count {
        return Ok(Some(digests));
    }
    let reason = match output.error_output.trim() {
        "" => text.trim().to_string(),
        error => error.to_string(),
    };
    context.shared.events.log(
        LogLevel::Warn,
        Some(&context.spec.session_id),
        format!(
            "The server could not compute a checksum of {}: {reason}",
            copies.remote
        ),
    );
    Ok(None)
}

/// Runs a checksum command through the worker's shell on the server where it keeps one, or
/// else on a channel of its own.
async fn run_hash_command(context: &JobContext<'_>, command: &str) -> AppResult<CommandOutput> {
    if let Some(output) = hash_shell::run(context, command, OUTPUT_LIMIT).await {
        return Ok(output);
    }
    let channel = context.command_channel().await?;
    run_command(channel, command, OUTPUT_LIMIT).await
}

/// Reads the server's copy and hashes it here, for servers with no checksum command.
async fn read_back(
    context: &JobContext<'_>,
    copies: &Copies<'_>,
    parts: Parts,
) -> AppResult<Vec<Vec<u8>>> {
    let fs = context.fs;
    let handle = fs.open_for_read(copies.remote).await?;
    let digests = read_hashing(context, &handle, copies.length, parts).await;
    let _ = fs.close_handle(handle).await;
    digests
}

async fn read_hashing(
    context: &JobContext<'_>,
    handle: &str,
    length: u64,
    parts: Parts,
) -> AppResult<Vec<Vec<u8>>> {
    let fs = context.fs;
    let chunk = fs.read_size(context.settings.request_size()).max(1);
    let limiter = context.shared.limiter_for(Direction::Download);
    let mut hasher = PartHasher::new(parts);
    let mut next = 0;
    let mut in_flight = FuturesOrdered::new();
    loop {
        while next < length && in_flight.len() < context.settings.requests_in_flight as usize {
            let len = u64::from(chunk).min(length - next) as u32;
            let offset = next;
            next += u64::from(len);
            cancellable(context, limiter.acquire(u64::from(len))).await?;
            in_flight
                .push_back(async move { (offset, len, fs.read_chunk(handle, offset, len).await) });
        }
        let Some((offset, len, result)) = cancellable(context, in_flight.next()).await? else {
            break;
        };
        match result? {
            ReadChunk::Data(data) => {
                let (data, ended) = read_rest(fs, handle, offset, len, data).await?;
                hasher.update(&data);
                if ended {
                    break;
                }
            }
            // Shorter than it should be: the digests then differ.
            ReadChunk::Eof => break,
        }
    }
    Ok(hasher.finish())
}

/// Servers may answer a read with fewer bytes than asked; the rest comes before what follows.
async fn read_rest(
    fs: &RemoteFs,
    handle: &str,
    offset: u64,
    len: u32,
    mut data: Vec<u8>,
) -> AppResult<(Vec<u8>, bool)> {
    while data.len() < len as usize {
        let missing = len - data.len() as u32;
        match fs
            .read_chunk(handle, offset + data.len() as u64, missing)
            .await?
        {
            ReadChunk::Data(more) => data.extend_from_slice(&more),
            ReadChunk::Eof => return Ok((data, true)),
        }
    }
    Ok((data, false))
}

async fn cancellable<T>(
    context: &JobContext<'_>,
    work: impl std::future::Future<Output = T>,
) -> AppResult<T> {
    tokio::select! {
        biased;
        _ = context.run.cancel.cancelled() => Err(AppError::cancelled()),
        value = work => Ok(value),
    }
}

/// For servers reached through `RemoteFileSystem` with no checksum command: whether the first
/// `length` bytes of the server's copy, read back, have the SHA-256 of the local one.
pub(super) async fn same_as_read_back(
    job: &JobRef<'_>,
    files: &Arc<dyn RemoteFileSystem>,
    remote: &str,
    local: &str,
    length: u64,
) -> AppResult<bool> {
    note_read_back(job).await;
    let local_digest = {
        let path = PathBuf::from(local);
        tokio::task::spawn_blocking(move || hash_file_parts(&path, length, Parts::WHOLE))
    };
    let remote_digest = async {
        let limiter = job.shared.limiter_for(Direction::Download);
        let mut stream = Arc::clone(files).open_read(remote, 0..length).await?;
        let mut hasher = PartHasher::new(Parts::WHOLE);
        let mut read = 0;
        while let Some(data) = stream.next_chunk().await? {
            let wanted = (length - read).min(data.len() as u64) as usize;
            limiter.acquire(wanted as u64).await;
            hasher.update(&data[..wanted]);
            read += wanted as u64;
            if read == length {
                break;
            }
        }
        stream.finish().await?;
        Ok::<_, AppError>(hasher.finish())
    };
    let (local_digest, remote_digest) = tokio::select! {
        biased;
        _ = job.run.cancel.cancelled() => return Err(AppError::cancelled()),
        both = async { tokio::join!(local_digest, remote_digest) } => both,
    };
    let local_digest = local_digest?.map_err(|error| AppError::from(error).with_path(local))?;
    Ok(local_digest == remote_digest?)
}

/// Says once per server that its files are read back.
async fn note_read_back(job: &JobRef<'_>) {
    let Ok(target) = job.shared.target(&job.spec.session_id) else {
        return;
    };
    let mut known = target.hashing.lock().await;
    if known.is_none() {
        *known = Some(ServerHashing::default());
        job.shared.events.log(
            LogLevel::Info,
            Some(&job.spec.session_id),
            format!(
                "{} has no checksum command, so finished files are read back from it and checked with SHA-256 here",
                target.label
            ),
        );
    }
}

/// Whether the damaged parts are few enough to send again on their own: a file in parts, at
/// most half of them damaged. More than that, and the whole file goes again.
pub(super) fn repairable(parts: Parts, length: u64, damaged: &[usize]) -> bool {
    let count = parts.count(length);
    count > 1 && !damaged.is_empty() && damaged.len() * 2 <= count
}

/// Writes the damaged parts of the copy again from the source. Returns the bytes sent.
pub(super) async fn repair(
    context: &JobContext<'_>,
    copies: &Copies<'_>,
    parts: Parts,
    damaged: &[usize],
) -> AppResult<u64> {
    let ranges: Vec<(u64, u64)> = damaged
        .iter()
        .map(|index| parts.range(*index, copies.length))
        .map(|range| (range.start, range.end))
        .collect();
    match context.spec.direction {
        Direction::Download => repair_local(context, copies, &ranges).await,
        Direction::Upload | Direction::Relay => repair_remote(context, copies, &ranges).await,
    }
}

/// Request-sized pieces of the ranges, in order.
fn pieces(ranges: &[(u64, u64)], chunk: u32) -> impl Iterator<Item = (u64, u32)> + '_ {
    ranges.iter().flat_map(move |(start, end)| {
        (*start..*end)
            .step_by(chunk as usize)
            .map(move |offset| (offset, u64::from(chunk).min(end - offset) as u32))
    })
}

async fn repair_remote(
    context: &JobContext<'_>,
    copies: &Copies<'_>,
    ranges: &[(u64, u64)],
) -> AppResult<u64> {
    let fs = context.fs;
    let mut file = File::open(copies.local)
        .await
        .map_err(|error| AppError::from(error).with_path(copies.local))?;
    let handle = fs.open_for_write(copies.remote, false, None).await?;
    let written = async {
        let chunk = fs.write_size(context.settings.request_size()).max(1);
        let limiter = context.shared.limiter_for(Direction::Upload);
        let mut written = 0;
        let mut in_flight = FuturesUnordered::new();
        for (offset, len) in pieces(ranges, chunk) {
            if in_flight.len() >= context.settings.requests_in_flight as usize {
                if let Some(result) = cancellable(context, in_flight.next()).await? {
                    result?;
                }
            }
            cancellable(context, limiter.acquire(u64::from(len))).await?;
            file.seek(std::io::SeekFrom::Start(offset)).await?;
            let data = read_up_to(&mut file, len as usize).await?;
            written += data.len() as u64;
            let handle = handle.as_str();
            in_flight.push(async move { fs.write_chunk(handle, offset, data).await });
        }
        while let Some(result) = cancellable(context, in_flight.next()).await? {
            result?;
        }
        Ok::<_, AppError>(written)
    }
    .await;
    let (_, closed) = fs
        .close_and_stat(handle, context.settings.flush_to_disk)
        .await;
    let written = written?;
    closed?;
    Ok(written)
}

async fn repair_local(
    context: &JobContext<'_>,
    copies: &Copies<'_>,
    ranges: &[(u64, u64)],
) -> AppResult<u64> {
    let fs = context.fs;
    let mut file = OpenOptions::new()
        .write(true)
        .open(copies.local)
        .await
        .map_err(|error| AppError::from(error).with_path(copies.local))?;
    let handle = fs.open_for_read(copies.remote).await?;
    let written = async {
        let chunk = fs.read_size(context.settings.request_size()).max(1);
        let limiter = context.shared.limiter_for(Direction::Download);
        let mut written = 0;
        let mut requests = pieces(ranges, chunk).peekable();
        let mut in_flight = FuturesOrdered::new();
        loop {
            while in_flight.len() < context.settings.requests_in_flight as usize {
                let Some((offset, len)) = requests.next() else {
                    break;
                };
                cancellable(context, limiter.acquire(u64::from(len))).await?;
                let handle = handle.as_str();
                in_flight.push_back(async move {
                    (offset, len, fs.read_chunk(handle, offset, len).await)
                });
            }
            let Some((offset, len, result)) = cancellable(context, in_flight.next()).await? else {
                break;
            };
            let data = match result? {
                ReadChunk::Data(data) => read_rest(fs, &handle, offset, len, data).await?.0,
                ReadChunk::Eof => Vec::new(),
            };
            file.seek(std::io::SeekFrom::Start(offset)).await?;
            file.write_all(&data).await?;
            written += data.len() as u64;
        }
        file.flush().await?;
        if context.settings.flush_to_disk {
            file.sync_all().await?;
        }
        Ok::<_, AppError>(written)
    }
    .await;
    let _ = fs.close_handle(handle).await;
    written
}

/// Where an interrupted file continues from once the part already transferred is compared on
/// both sides, part by part: `copies.length`, or the start of the first part that differs.
/// `None` when the server cannot hash part of a file; the caller then compares the bytes just
/// before the resume point instead.
pub(super) async fn resume_from(
    context: &JobContext<'_>,
    copies: &Copies<'_>,
    file_length: u64,
) -> AppResult<Option<u64>> {
    if !server_hashing(context).await.parts {
        return Ok(None);
    }
    let parts = Parts::for_length(file_length);
    let start = copies.length;
    let count = parts.count(start);
    let quoted = shell_quote(copies.remote);
    let command = if count == 1 {
        format!("head -c {start} -- {quoted} | sha256sum")
    } else {
        format!(
            "head -c {start} -- {quoted} | split -b {} --filter=sha256sum",
            parts.size
        )
    };
    let local = {
        let path = PathBuf::from(copies.local);
        tokio::task::spawn_blocking(move || hash_file_parts(&path, start, parts))
    };
    let remote = async { run_hash_command(context, &command).await };
    let (local, remote) = tokio::select! {
        biased;
        _ = context.run.cancel.cancelled() => return Err(AppError::cancelled()),
        both = async { tokio::join!(local, remote) } => both,
    };
    let local = local?.map_err(|error| AppError::from(error).with_path(copies.local))?;
    let output = remote?;
    if output.exit_status != Some(0) {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.output);
    let remote: Vec<Vec<u8>> = parse_output(&text, Algorithm::Sha256)
        .into_iter()
        .map(|(_, digest)| digest)
        .collect();
    let first_damaged = (0..count).find(|index| local.get(*index) != remote.get(*index));
    Ok(Some(match first_damaged {
        Some(index) => parts.range(index, start).start,
        None => start,
    }))
}

/// The server's hashing commands, looked for once per server.
async fn server_hashing(context: &JobContext<'_>) -> ServerHashing {
    let Ok(target) = context.shared.target(&context.spec.session_id) else {
        return ServerHashing::default();
    };
    let mut known = target.hashing.lock().await;
    if let Some(hashing) = *known {
        return hashing;
    }
    let mut hashing = ServerHashing::default();
    for algorithm in Algorithm::STRONG {
        // With no file named, the commands hash their empty input, so the answer is known.
        let expected = [algorithm.hasher().finish()];
        match probe(context, algorithm.command(), algorithm, &expected).await {
            Some(true) => {
                hashing.whole = Some(algorithm);
                break;
            }
            Some(false) => {}
            None => return ServerHashing::default(),
        }
    }
    if hashing.whole == Some(Algorithm::Sha256) {
        let expected = [b"ab".as_slice(), b"c"].map(|data| {
            let mut hasher = Algorithm::Sha256.hasher();
            hasher.update(data);
            hasher.finish()
        });
        let command = "printf abc | head -c 3 | split -b 2 --filter=sha256sum";
        match probe(context, command, Algorithm::Sha256, &expected).await {
            Some(works) => hashing.parts = works,
            None => return ServerHashing::default(),
        }
    }
    let message = match hashing.whole {
        Some(algorithm) if hashing.parts => format!(
            "Transfers to and from {} are checked with {} ({}), part by part, so a damaged part is sent again on its own",
            target.label,
            algorithm.label(),
            algorithm.command()
        ),
        Some(algorithm) => format!(
            "Transfers to and from {} are checked with {} ({})",
            target.label,
            algorithm.label(),
            algorithm.command()
        ),
        None if context.settings.read_back => format!(
            "{} has no sha256sum or sha512sum command, so finished files are read back from it and checked with SHA-256 here",
            target.label
        ),
        None => format!(
            "{} has no sha256sum or sha512sum command, so its transfers are checked by size only",
            target.label
        ),
    };
    context
        .shared
        .events
        .log(LogLevel::Info, Some(&context.spec.session_id), message);
    *known = Some(hashing);
    hashing
}

/// Whether a command printed the expected digests; `None` when the transfer was cancelled.
async fn probe(
    context: &JobContext<'_>,
    command: &str,
    algorithm: Algorithm,
    expected: &[Vec<u8>],
) -> Option<bool> {
    let probed = async {
        let channel = context.command_channel().await?;
        run_command(channel, command, OUTPUT_LIMIT).await
    };
    let output = tokio::select! {
        biased;
        _ = context.run.cancel.cancelled() => return None,
        output = probed => output,
    };
    let Ok(output) = output else {
        return Some(false);
    };
    let text = String::from_utf8_lossy(&output.output);
    let digests: Vec<Vec<u8>> = parse_output(&text, algorithm)
        .into_iter()
        .map(|(_, digest)| digest)
        .collect();
    Some(output.exit_status == Some(0) && digests == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_damaged_parts_and_what_can_be_repaired() {
        let digest = |byte: u8| vec![byte; 32];
        let local = vec![digest(1), digest(2), digest(3)];
        assert!(damaged_parts(&local, &local).is_empty());
        let remote = vec![digest(1), digest(9), digest(3)];
        assert_eq!(damaged_parts(&local, &remote), vec![1]);
        assert_eq!(damaged_parts(&local, &local[..2]), vec![2]);

        let parts = Parts { size: 10 };
        assert!(repairable(parts, 40, &[1]));
        assert!(repairable(parts, 40, &[0, 3]));
        assert!(!repairable(parts, 40, &[0, 1, 2]));
        assert!(!repairable(parts, 10, &[0]));
        assert!(!repairable(Parts::WHOLE, 40, &[0]));

        let ranges = [(0, 25), (30, 31)];
        let split: Vec<(u64, u32)> = pieces(&ranges, 10).collect();
        assert_eq!(split, vec![(0, 10), (10, 10), (20, 5), (30, 1)]);
    }
}
