//! Pipelined SFTP reads and writes. Each worker keeps many requests in flight on its own
//! connection, so throughput is not bound by round-trip time, and claims pieces from the job
//! as it goes, so several workers can share one large file.

use std::collections::{BTreeSet, HashMap};
use std::io::SeekFrom;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::stream::{FuturesOrdered, FuturesUnordered, StreamExt};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use super::limiter::RateLimiter;
use super::queue::JobRun;
use crate::error::{AppError, AppResult};
use crate::sftp::{ReadChunk, RemoteFs};

pub struct CopyContext<'a> {
    pub fs: &'a RemoteFs,
    pub run: &'a JobRun,
    pub limiter: &'a RateLimiter,
    /// Bytes moved in this direction by every worker, for the overall speed.
    pub total: &'a AtomicU64,
    pub request_size: u32,
    pub requests_in_flight: usize,
}

impl CopyContext<'_> {
    fn record(&self, bytes: u64) {
        self.run.add_progress(bytes);
        self.total.fetch_add(bytes, Ordering::Relaxed);
    }

    async fn throttle(&self, bytes: u64) -> AppResult<()> {
        tokio::select! {
            biased;
            _ = self.run.cancel.cancelled() => Err(AppError::cancelled()),
            _ = self.limiter.acquire(bytes) => Ok(()),
        }
    }
}

/// The piece a worker is working through.
struct Cursor {
    piece_start: u64,
    next: u64,
    end: u64,
}

impl Cursor {
    fn advance(cursor: &mut Option<Cursor>, run: &JobRun) -> bool {
        if cursor
            .as_ref()
            .is_some_and(|current| current.next < current.end)
        {
            return true;
        }
        *cursor = run.claim_piece().map(|piece| Cursor {
            piece_start: piece.start,
            next: piece.start,
            end: piece.end,
        });
        cursor.is_some()
    }

    fn take(&mut self, chunk: u32) -> (u64, u32, bool) {
        let len = (u64::from(chunk)).min(self.end - self.next) as u32;
        let offset = self.next;
        self.next += u64::from(len);
        (offset, len, self.next == self.end)
    }
}

pub async fn download(context: &CopyContext<'_>, handle: &str, file: &mut File) -> AppResult<()> {
    let chunk = context.fs.read_size(context.request_size).max(1);
    let mut cursor = None;
    let mut in_flight = FuturesOrdered::new();
    let mut file_position = None;
    let mut claiming = true;

    loop {
        while claiming && in_flight.len() < context.requests_in_flight {
            if !Cursor::advance(&mut cursor, context.run) {
                claiming = false;
                break;
            }
            let current = cursor.as_mut().expect("advance returned a cursor");
            let piece_start = current.piece_start;
            let (offset, len, ends_piece) = current.take(chunk);
            context.throttle(u64::from(len)).await?;
            let fs = context.fs;
            in_flight.push_back(async move {
                let result = fs.read_chunk(handle, offset, len).await;
                (offset, len, piece_start, ends_piece, result)
            });
        }

        let next = tokio::select! {
            biased;
            _ = context.run.cancel.cancelled() => return Err(AppError::cancelled()),
            next = in_flight.next() => next,
        };
        let Some((offset, len, piece_start, ends_piece, result)) = next else {
            break;
        };
        let (data, reached_end) = match result? {
            ReadChunk::Eof => (Vec::new(), true),
            ReadChunk::Data(data) => fill_short_read(context, handle, offset, len, data).await?,
        };

        if !data.is_empty() {
            if file_position != Some(offset) {
                file.seek(SeekFrom::Start(offset)).await?;
            }
            file.write_all(&data).await?;
            file_position = Some(offset + data.len() as u64);
            context.record(data.len() as u64);
            // Reads complete in order, so the piece is whole up to here.
            context
                .run
                .reach_piece(piece_start, offset + data.len() as u64);
        }
        if reached_end {
            // The file is shorter than listed. This worker wrote its piece up to the end, and
            // anything it requested beyond the end no longer exists.
            context.run.end_at(offset + data.len() as u64);
            context.run.complete_piece(piece_start);
            break;
        }
        if ends_piece {
            context.run.complete_piece(piece_start);
        }
    }
    file.flush().await?;
    Ok(())
}

/// Servers may answer a read with fewer bytes than asked; fetch the rest before writing on.
async fn fill_short_read(
    context: &CopyContext<'_>,
    handle: &str,
    offset: u64,
    len: u32,
    mut data: Vec<u8>,
) -> AppResult<(Vec<u8>, bool)> {
    while data.len() < len as usize {
        let missing = len - data.len() as u32;
        match context
            .fs
            .read_chunk(handle, offset + data.len() as u64, missing)
            .await?
        {
            ReadChunk::Data(more) => data.extend_from_slice(&more),
            ReadChunk::Eof => return Ok((data, true)),
        }
    }
    Ok((data, false))
}

/// A piece this worker is writing; writes may be acknowledged out of order.
#[derive(Default)]
struct OpenPiece {
    unacknowledged: BTreeSet<u64>,
    issued_to: u64,
    fully_issued: bool,
}

impl OpenPiece {
    /// Everything before this offset is on the server.
    fn acknowledged_to(&self) -> u64 {
        self.unacknowledged
            .first()
            .copied()
            .unwrap_or(self.issued_to)
    }
}

pub async fn upload(context: &CopyContext<'_>, file: &mut File, handle: &str) -> AppResult<()> {
    let chunk = context.fs.write_size(context.request_size).max(1);
    let mut cursor = None;
    let mut in_flight = FuturesUnordered::new();
    let mut file_position = None;
    let mut claiming = true;
    let mut open_pieces: HashMap<u64, OpenPiece> = HashMap::new();

    loop {
        while claiming && in_flight.len() < context.requests_in_flight {
            if !Cursor::advance(&mut cursor, context.run) {
                claiming = false;
                break;
            }
            let current = cursor.as_mut().expect("advance returned a cursor");
            let piece_start = current.piece_start;
            let (offset, len, ends_piece) = current.take(chunk);
            context.throttle(u64::from(len)).await?;

            if file_position != Some(offset) {
                file.seek(SeekFrom::Start(offset)).await?;
            }
            let data = read_up_to(file, len as usize).await?;
            file_position = Some(offset + data.len() as u64);
            let source_ended = data.len() < len as usize;
            if source_ended {
                context.run.end_at(offset + data.len() as u64);
                claiming = false;
            }

            let piece = open_pieces.entry(piece_start).or_insert_with(|| OpenPiece {
                issued_to: piece_start,
                ..OpenPiece::default()
            });
            piece.fully_issued = ends_piece || source_ended;
            if data.is_empty() {
                if piece.unacknowledged.is_empty() && piece.fully_issued {
                    open_pieces.remove(&piece_start);
                    context.run.complete_piece(piece_start);
                }
                break;
            }
            piece.unacknowledged.insert(offset);
            piece.issued_to = offset + data.len() as u64;
            let fs = context.fs;
            let written = data.len() as u64;
            in_flight.push(async move {
                let result = fs.write_chunk(handle, offset, data).await;
                (piece_start, offset, written, result)
            });
            if source_ended {
                break;
            }
        }

        let next = tokio::select! {
            biased;
            _ = context.run.cancel.cancelled() => return Err(AppError::cancelled()),
            next = in_flight.next() => next,
        };
        let Some((piece_start, offset, written, result)) = next else {
            break;
        };
        result?;
        context.record(written);
        if let Some(piece) = open_pieces.get_mut(&piece_start) {
            piece.unacknowledged.remove(&offset);
            context
                .run
                .reach_piece(piece_start, piece.acknowledged_to());
            if piece.unacknowledged.is_empty() && piece.fully_issued {
                open_pieces.remove(&piece_start);
                context.run.complete_piece(piece_start);
            }
        }
    }
    Ok(())
}

pub(super) async fn read_up_to(file: &mut File, len: usize) -> AppResult<Vec<u8>> {
    let mut buffer = vec![0; len];
    let mut filled = 0;
    while filled < len {
        let read = file.read(&mut buffer[filled..]).await?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    buffer.truncate(filled);
    Ok(buffer)
}
