//! Delta transfers with the rsync protocol. Poros starts `rsync --server` over its own SSH
//! connection and speaks protocol 29 with it, so only the server needs rsync. When the
//! receiving side already has a version of a file, only the parts that changed are sent.

mod checksum;
mod delta;
mod process;

use std::future::Future;
use std::io::{self, BufWriter};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use russh::client::Msg;
use russh::Channel;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::remote_path;
use checksum::{block_length, STRONG_SUM_BYTES};
use delta::{find_matches, BlockSum, Reconstruction, Signature, Token};
use process::RemoteProcess;
pub use process::{run_command, shell_quote, CommandOutput};

/// The newest protocol without negotiated checksums and incremental file lists.
const PROTOCOL_VERSION: i32 = 29;
/// Ends a phase of the transfer, and the transfer itself.
const DONE: i32 = -1;
const LITERAL_CHUNK: usize = 32 * 1024;
const MAX_LITERAL: usize = 1024 * 1024;
const TOKEN_BATCH_BYTES: usize = 256 * 1024;
const MAX_NAME_BYTES: usize = 4096;
const MAX_BLOCK_LENGTH: i32 = 1 << 29;
const MAX_BLOCK_COUNT: i32 = 1 << 22;
const MIN_STRONG_LENGTH: i32 = 2;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const PROBE_OUTPUT_LIMIT: usize = 64 * 1024;
const FILE_THREAD_BUFFER: usize = 256 * 1024;

const ITEM_BASIS_TYPE_FOLLOWS: u16 = 1 << 11;
const ITEM_NAME_FOLLOWS: u16 = 1 << 12;
const ITEM_IS_NEW: u16 = 1 << 13;
const ITEM_TRANSFER: u16 = 1 << 15;

const SAME_MODE: u32 = 1 << 1;
const EXTENDED_FLAGS: u32 = 1 << 2;
const SAME_OWNER: u32 = 1 << 3;
const SAME_GROUP: u32 = 1 << 4;
const SAME_NAME: u32 = 1 << 5;
const LONG_NAME: u32 = 1 << 6;
const SAME_TIME: u32 = 1 << 7;

const FILE_TYPE_MASK: u32 = 0o170_000;
const REGULAR_FILE: u32 = 0o100_000;

/// How a delta transfer reports progress and keeps to the speed limit.
pub trait Pace: Sync {
    /// `file_bytes` more of the file are in place; `literal_bytes` of them crossed the network.
    fn advance(&self, file_bytes: u64, literal_bytes: u64);
    /// Waits until `literal_bytes` may cross the network; fails once the transfer is cancelled.
    fn throttle(&self, literal_bytes: u64) -> impl Future<Output = AppResult<()>> + Send;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeltaReport {
    /// File data that crossed the network.
    pub literal_bytes: u64,
    /// File data copied from the old version.
    pub matched_bytes: u64,
}

/// The file as the server described it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteFile {
    pub size: u64,
    pub modified: i64,
    pub permissions: u32,
}

pub struct Upload<'a> {
    pub rsync_path: &'a str,
    pub source: &'a Path,
    pub target: &'a str,
    pub size: u64,
    pub modified: i64,
    pub permissions: u32,
    pub preserve_timestamps: bool,
    pub preserve_permissions: bool,
}

pub struct Download<'a> {
    pub rsync_path: &'a str,
    pub source: &'a str,
    /// The local file the new version is built from.
    pub basis: &'a Path,
    /// Where the new version is written; the caller moves it into place.
    pub output: &'a Path,
}

/// The protocol version of rsync on the server, or `None` when it does not run.
pub async fn probe(channel: Channel<Msg>, rsync_path: &str) -> AppResult<Option<i32>> {
    let command = format!("{rsync_path} --version");
    // Input is closed at once, so a server that forces another program for every command
    // does not keep this waiting.
    let probed = tokio::time::timeout(
        PROBE_TIMEOUT,
        run_command(channel, &command, PROBE_OUTPUT_LIMIT),
    )
    .await;
    Ok(match probed {
        Ok(Ok(CommandOutput {
            exit_status: Some(0),
            output,
            ..
        })) => protocol_version(&String::from_utf8_lossy(&output)),
        Ok(Err(error)) => return Err(error),
        _ => None,
    })
}

fn protocol_version(version_output: &str) -> Option<i32> {
    let after = version_output.split("protocol version").nth(1)?;
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Replaces `request.target` on the server, sending only what differs from it.
pub async fn upload(
    channel: Channel<Msg>,
    request: &Upload<'_>,
    pace: &impl Pace,
) -> AppResult<DeltaReport> {
    let mut options = String::from("-I");
    if request.preserve_timestamps {
        options.push('t');
    }
    if request.preserve_permissions {
        options.push('p');
    }
    let command = format!(
        "{} --server {options} . {}",
        request.rsync_path,
        shell_quote(&server_path(request.target))
    );
    let mut process = RemoteProcess::start(channel, &command).await?;
    let report = send_file(&mut process, request, pace).await?;
    process.finish().await?;
    Ok(report)
}

/// Writes the server's `request.source` to `request.output`, reusing `request.basis`.
pub async fn download(
    channel: Channel<Msg>,
    request: &Download<'_>,
    pace: &impl Pace,
) -> AppResult<(DeltaReport, RemoteFile)> {
    let command = format!(
        "{} --server --sender -tL . {}",
        request.rsync_path,
        shell_quote(&server_path(request.source))
    );
    let mut process = RemoteProcess::start(channel, &command).await?;
    let received = receive_file(&mut process, request, pace).await?;
    process.finish().await?;
    Ok(received)
}

/// Keeps a path that starts with a dash from reading as an option.
fn server_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("./{path}")
    }
}

async fn handshake(process: &mut RemoteProcess) -> AppResult<i32> {
    process.write_int(PROTOCOL_VERSION);
    process.flush().await?;
    let remote_version = process.read_int().await?;
    if !(PROTOCOL_VERSION..1000).contains(&remote_version) {
        let message = if (1..PROTOCOL_VERSION).contains(&remote_version) {
            format!("rsync on the server is too old (protocol {remote_version})")
        } else {
            "rsync on the server answered with something unexpected; the login shell may print text for commands".to_string()
        };
        return Err(AppError::new(ErrorKind::Rsync, message));
    }
    let seed = process.read_int().await?;
    process.start_multiplexed_input();
    Ok(seed)
}

async fn send_file(
    process: &mut RemoteProcess,
    request: &Upload<'_>,
    pace: &impl Pace,
) -> AppResult<DeltaReport> {
    let seed = handshake(process).await?;
    let name = remote_path::file_name(request.target).as_bytes();
    write_file_entry(
        process,
        name,
        request.size,
        request.modified,
        REGULAR_FILE | (request.permissions & 0o7777),
    );
    process.write_byte(0);
    // No read errors while building the list.
    process.write_int(0);
    process.flush().await?;

    let mut report = None;
    let mut phase = 0;
    loop {
        let index = process.read_int().await?;
        if index == DONE {
            phase += 1;
            if phase > 2 {
                break;
            }
            process.write_int(DONE);
            process.flush().await?;
            continue;
        }
        let item = read_item(process).await?;
        // Older servers send this as a keep-alive.
        if index == 1 && item.flags == ITEM_IS_NEW {
            continue;
        }
        if index != 0 {
            return Err(process.error("rsync on the server asked for a file it was not sent"));
        }
        if item.flags & ITEM_TRANSFER == 0 {
            write_item(process, index, &item);
            process.flush().await?;
            continue;
        }
        let signature = read_signature(process).await?;
        write_item(process, index, &item);
        write_signature_head(process, &signature);
        report = Some(send_tokens(process, request.source, signature, seed, pace).await?);
        process.flush().await?;
    }
    process.write_int(DONE);
    process.flush().await?;
    if process.read_int().await? != DONE {
        return Err(process.error("rsync on the server ended the transfer unexpectedly"));
    }
    report.ok_or_else(|| process.error("rsync on the server did not update the file"))
}

async fn send_tokens(
    process: &mut RemoteProcess,
    source: &Path,
    signature: Signature,
    seed: i32,
    pace: &impl Pace,
) -> AppResult<DeltaReport> {
    let (sender, mut batches) = mpsc::channel::<TokenBatch>(4);
    let source_path = source.to_path_buf();
    let scan = tokio::task::spawn_blocking(move || -> io::Result<[u8; STRONG_SUM_BYTES]> {
        let file = std::fs::File::open(&source_path)?;
        let mut batch = TokenBatch::default();
        let stopped = || io::Error::from(io::ErrorKind::Interrupted);
        let sum = find_matches(&signature, seed, file, |token| {
            match token {
                Token::Literal(data) => {
                    for chunk in data.chunks(LITERAL_CHUNK) {
                        batch
                            .bytes
                            .extend_from_slice(&(chunk.len() as i32).to_le_bytes());
                        batch.bytes.extend_from_slice(chunk);
                    }
                    batch.file_bytes += data.len() as u64;
                    batch.literal_bytes += data.len() as u64;
                }
                Token::Block(index) => {
                    batch
                        .bytes
                        .extend_from_slice(&(-(index as i32) - 1).to_le_bytes());
                    batch.file_bytes += signature.block_size(index as usize) as u64;
                }
            }
            if batch.bytes.len() >= TOKEN_BATCH_BYTES {
                sender
                    .blocking_send(std::mem::take(&mut batch))
                    .map_err(|_| stopped())?;
            }
            Ok(())
        })?;
        if !batch.bytes.is_empty() {
            sender.blocking_send(batch).map_err(|_| stopped())?;
        }
        Ok(sum)
    });

    let mut report = DeltaReport::default();
    while let Some(batch) = batches.recv().await {
        pace.throttle(batch.literal_bytes).await?;
        process.write_bytes(&batch.bytes);
        process.flush().await?;
        pace.advance(batch.file_bytes, batch.literal_bytes);
        report.literal_bytes += batch.literal_bytes;
        report.matched_bytes += batch.file_bytes - batch.literal_bytes;
    }
    let sum = scan
        .await?
        .map_err(|error| AppError::from(error).with_path(source.to_string_lossy()))?;
    process.write_int(0);
    process.write_bytes(&sum);
    Ok(report)
}

#[derive(Default)]
struct TokenBatch {
    bytes: Vec<u8>,
    file_bytes: u64,
    literal_bytes: u64,
}

async fn receive_file(
    process: &mut RemoteProcess,
    request: &Download<'_>,
    pace: &impl Pace,
) -> AppResult<(DeltaReport, RemoteFile)> {
    let seed = handshake(process).await?;
    // No filter rules.
    process.write_int(0);
    process.flush().await?;
    let file = read_file_list(process).await?;

    let stop = StopOnDrop::default();
    let stopped = stop.0.clone();
    let basis = request.basis.to_path_buf();
    let signature = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&basis)?;
        let length = file.metadata()?.len();
        Signature::generate(file, block_length(length), seed, || {
            !stopped.load(Ordering::Relaxed)
        })
    })
    .await?
    .map_err(|error| AppError::from(error).with_path(request.basis.to_string_lossy()))?;

    process.write_int(0);
    process.write_short(ITEM_TRANSFER);
    write_signature_head(process, &signature);
    for block in &signature.blocks {
        process.write_int(block.weak as i32);
        process.write_bytes(&block.strong[..signature.strong_length as usize]);
        process.flush_if_full().await?;
    }
    // Ends the file, redo and final phases at once; this side never asks for a redo.
    for _ in 0..3 {
        process.write_int(DONE);
    }
    process.flush().await?;

    if process.read_int().await? != 0 {
        return Err(process.error("rsync on the server could not send the file"));
    }
    read_item(process).await?;
    read_signature_head(process).await?;

    let layout = signature.clone();
    let (sender, pieces) = mpsc::channel::<Piece>(16);
    let basis = request.basis.to_path_buf();
    let output = request.output.to_path_buf();
    let rebuild =
        tokio::task::spawn_blocking(move || rebuild_file(&basis, &output, pieces, signature, seed));
    let mut report = DeltaReport::default();
    let streamed = stream_tokens(process, &sender, &layout, pace, &mut report).await;
    drop(sender);
    match (streamed, rebuild.await?) {
        (Ok(()), Some(Ok(_))) => {}
        (_, Some(Err(error))) => {
            return Err(AppError::from(error).with_path(request.output.to_string_lossy()))
        }
        (Err(error), _) => return Err(error),
        (Ok(()), None) => return Err(process.error("The rsync transfer ended early")),
    }

    // The ends of the three phases, then the sender's transfer statistics.
    for _ in 0..3 {
        if process.read_int().await? != DONE {
            return Err(process.error("rsync on the server sent more than one file"));
        }
    }
    for _ in 0..5 {
        process.read_long().await?;
    }
    process.write_int(DONE);
    Ok((report, file))
}

enum Piece {
    Literal(Vec<u8>),
    Block(u32),
    End([u8; STRONG_SUM_BYTES]),
}

async fn stream_tokens(
    process: &mut RemoteProcess,
    sender: &mpsc::Sender<Piece>,
    layout: &Signature,
    pace: &impl Pace,
    report: &mut DeltaReport,
) -> AppResult<()> {
    let ended = || AppError::new(ErrorKind::Rsync, "Could not write the received file");
    loop {
        let token = process.read_int().await?;
        let piece = if token > 0 {
            let length = token as usize;
            if length > MAX_LITERAL {
                return Err(process.error("rsync on the server sent an oversized piece"));
            }
            pace.throttle(length as u64).await?;
            let data = process.read_bytes(length).await?;
            pace.advance(length as u64, length as u64);
            report.literal_bytes += length as u64;
            Piece::Literal(data)
        } else if token < 0 {
            let index = (-(i64::from(token) + 1)) as usize;
            if index >= layout.blocks.len() {
                return Err(process.error("rsync on the server referred to a missing block"));
            }
            let length = layout.block_size(index) as u64;
            pace.advance(length, 0);
            report.matched_bytes += length;
            Piece::Block(index as u32)
        } else {
            let mut sum = [0; STRONG_SUM_BYTES];
            process.read_exact(&mut sum).await?;
            sender.send(Piece::End(sum)).await.map_err(|_| ended())?;
            return Ok(());
        };
        sender.send(piece).await.map_err(|_| ended())?;
    }
}

/// `None` when the tokens stopped before the end of the file.
fn rebuild_file(
    basis: &Path,
    output: &Path,
    mut pieces: mpsc::Receiver<Piece>,
    signature: Signature,
    seed: i32,
) -> Option<io::Result<u64>> {
    let opened = std::fs::File::open(basis).and_then(|basis| {
        let output = std::fs::File::create(output)?;
        Ok((basis, BufWriter::with_capacity(FILE_THREAD_BUFFER, output)))
    });
    let mut rebuilt = match opened {
        Ok((basis, output)) => Reconstruction::new(basis, output, signature, seed),
        Err(error) => return Some(Err(error)),
    };
    while let Some(piece) = pieces.blocking_recv() {
        let applied = match piece {
            Piece::Literal(data) => rebuilt.literal(&data),
            Piece::Block(index) => rebuilt.block(index).map(|_| ()),
            Piece::End(sum) => return Some(rebuilt.finish(&sum)),
        };
        if let Err(error) = applied {
            return Some(Err(error));
        }
    }
    None
}

/// Tells a worker thread to stop once the transfer that started it is gone.
#[derive(Default)]
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn write_file_entry(process: &mut RemoteProcess, name: &[u8], size: u64, modified: i64, mode: u32) {
    let mut flags = SAME_OWNER | SAME_GROUP;
    if name.len() > 255 {
        flags |= LONG_NAME;
    }
    process.write_byte(flags as u8);
    if flags & LONG_NAME != 0 {
        process.write_int(name.len() as i32);
    } else {
        process.write_byte(name.len() as u8);
    }
    process.write_bytes(name);
    process.write_long(size as i64);
    // Protocol 29 carries 32-bit times.
    process.write_int(modified.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
    process.write_int(mode as i32);
}

/// Reads a list that should hold one regular file.
async fn read_file_list(process: &mut RemoteProcess) -> AppResult<RemoteFile> {
    let mut files = Vec::new();
    let mut previous_name: Vec<u8> = Vec::new();
    let mut modified = 0;
    let mut mode = 0;
    loop {
        let mut flags = u32::from(process.read_byte().await?);
        if flags == 0 {
            break;
        }
        if flags & EXTENDED_FLAGS != 0 {
            flags |= u32::from(process.read_byte().await?) << 8;
        }
        let shared = if flags & SAME_NAME != 0 {
            usize::from(process.read_byte().await?)
        } else {
            0
        };
        let suffix = if flags & LONG_NAME != 0 {
            usize::try_from(process.read_int().await?).unwrap_or(usize::MAX)
        } else {
            usize::from(process.read_byte().await?)
        };
        if shared > previous_name.len() || suffix > MAX_NAME_BYTES {
            return Err(process.error("rsync on the server sent a malformed file list"));
        }
        let mut name = previous_name[..shared].to_vec();
        name.extend(process.read_bytes(suffix).await?);
        let size = process.read_long().await?;
        if flags & SAME_TIME == 0 {
            modified = i64::from(process.read_int().await?);
        }
        if flags & SAME_MODE == 0 {
            mode = process.read_int().await? as u32;
        }
        files.push((size, modified, mode));
        previous_name = name;
    }
    // Errors the server hit while listing.
    process.read_int().await?;
    match files.as_slice() {
        [(size, modified, mode)] if mode & FILE_TYPE_MASK == REGULAR_FILE => Ok(RemoteFile {
            size: u64::try_from(*size).unwrap_or(0),
            modified: *modified,
            permissions: mode & 0o7777,
        }),
        [] => Err(process.error("rsync on the server could not read the file")),
        _ => Err(process.error("The server's file is not a regular file")),
    }
}

struct Item {
    flags: u16,
    basis_type: Option<u8>,
    alternate_name: Option<Vec<u8>>,
}

async fn read_item(process: &mut RemoteProcess) -> AppResult<Item> {
    let flags = process.read_short().await?;
    let basis_type = if flags & ITEM_BASIS_TYPE_FOLLOWS != 0 {
        Some(process.read_byte().await?)
    } else {
        None
    };
    let alternate_name = if flags & ITEM_NAME_FOLLOWS != 0 {
        Some(process.read_short_string().await?)
    } else {
        None
    };
    Ok(Item {
        flags,
        basis_type,
        alternate_name,
    })
}

fn write_item(process: &mut RemoteProcess, index: i32, item: &Item) {
    process.write_int(index);
    process.write_short(item.flags);
    if let Some(basis_type) = item.basis_type {
        process.write_byte(basis_type);
    }
    if let Some(name) = &item.alternate_name {
        process.write_short_string(name);
    }
}

fn write_signature_head(process: &mut RemoteProcess, signature: &Signature) {
    process.write_int(signature.blocks.len() as i32);
    process.write_int(signature.block_length as i32);
    process.write_int(signature.strong_length as i32);
    process.write_int(signature.remainder as i32);
}

/// Block count, block length, strong sum length and last block length.
async fn read_signature_head(process: &mut RemoteProcess) -> AppResult<[i32; 4]> {
    let mut head = [0; 4];
    for value in &mut head {
        *value = process.read_int().await?;
    }
    let [count, length, strong_length, remainder] = head;
    let valid = (0..=MAX_BLOCK_COUNT).contains(&count)
        && (0..=MAX_BLOCK_LENGTH).contains(&length)
        && (0..=STRONG_SUM_BYTES as i32).contains(&strong_length)
        && (0..=length).contains(&remainder)
        && (count == 0 || (length > 0 && strong_length >= MIN_STRONG_LENGTH));
    if valid {
        Ok(head)
    } else {
        Err(process.error("rsync on the server sent malformed block sums"))
    }
}

async fn read_signature(process: &mut RemoteProcess) -> AppResult<Signature> {
    let [count, length, strong_length, remainder] = read_signature_head(process).await?;
    let entry = 4 + strong_length as usize;
    let mut blocks = Vec::with_capacity((count as usize).min(65_536));
    let mut left = count as usize;
    while left > 0 {
        let batch = left.min(4096);
        let data = process.read_bytes(batch * entry).await?;
        for sum in data.chunks_exact(entry) {
            let mut strong = [0; STRONG_SUM_BYTES];
            strong[..entry - 4].copy_from_slice(&sum[4..]);
            blocks.push(BlockSum {
                weak: u32::from_le_bytes([sum[0], sum[1], sum[2], sum[3]]),
                strong,
            });
        }
        left -= batch;
    }
    Ok(Signature {
        block_length: length as u32,
        remainder: remainder as u32,
        strong_length: strong_length as u32,
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_protocol_version() {
        assert_eq!(
            protocol_version("rsync  version 3.2.7  protocol version 31\nCopyright"),
            Some(31)
        );
        assert_eq!(protocol_version("bash: rsync: command not found"), None);
    }

    #[test]
    fn relative_paths_cannot_read_as_options() {
        assert_eq!(server_path("/srv/-x"), "/srv/-x");
        assert_eq!(server_path("-x"), "./-x");
    }
}
