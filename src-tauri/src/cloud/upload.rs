//! Uploads to the HTTP services. A small file goes up in one request when it is finished; a
//! larger one opens an upload session (a multipart or large-file upload) and is sent in
//! fixed-size chunks as they fill, so memory use stays flat whatever the file size.

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol::WriteStream;

/// Files up to this size are sent in a single request.
pub const SINGLE_REQUEST_LIMIT: usize = 4 * 1024 * 1024;
/// A multiple of both services' chunk granularity: 256 KiB for Google, 320 KiB for Microsoft.
pub const CHUNK_SIZE: usize = 10 * 1024 * 1024;

/// The chunk size for a file of `size` bytes on a service that takes at most `max_chunks`
/// chunks per file: `CHUNK_SIZE`, or as many whole mebibytes as it takes to stay under the cap.
pub fn chunk_size_within(size: u64, max_chunks: u64) -> usize {
    const MEBIBYTE: u64 = 1024 * 1024;
    let needed = size.div_ceil(max_chunks).div_ceil(MEBIBYTE) * MEBIBYTE;
    usize::try_from(needed)
        .unwrap_or(usize::MAX)
        .max(CHUNK_SIZE)
}

/// One service's side of an upload.
#[async_trait]
pub trait UploadTarget: Send {
    /// Files up to this size go in one request.
    fn single_request_limit(&self) -> usize {
        SINGLE_REQUEST_LIMIT
    }

    /// The size of every chunk but the last, read once the session is open.
    fn chunk_size(&self) -> usize {
        CHUNK_SIZE
    }

    /// Sends a whole file that fit within `single_request_limit`.
    async fn upload_whole(&mut self, data: Bytes) -> AppResult<()>;

    /// Opens an upload session for a larger file.
    async fn start_session(&mut self) -> AppResult<()>;

    /// Sends the bytes at `offset`; `last` completes the file with them.
    async fn send_chunk(&mut self, offset: u64, data: Bytes, last: bool) -> AppResult<()>;
}

pub struct ChunkedUpload<T: UploadTarget> {
    target: T,
    buffer: BytesMut,
    sent: u64,
    session_open: bool,
    failed: bool,
}

impl<T: UploadTarget> ChunkedUpload<T> {
    pub fn new(target: T, size: u64) -> Self {
        let capacity = usize::try_from(size)
            .unwrap_or(usize::MAX)
            .min(target.single_request_limit() + 1);
        Self {
            target,
            buffer: BytesMut::with_capacity(capacity),
            sent: 0,
            session_open: false,
            failed: false,
        }
    }

    async fn flush_full_chunks(&mut self) -> AppResult<()> {
        if !self.session_open {
            if self.buffer.len() <= self.target.single_request_limit() {
                return Ok(());
            }
            self.target.start_session().await?;
            self.session_open = true;
        }
        let chunk_size = self.target.chunk_size();
        // At least one byte stays behind, so the final chunk is never empty.
        while self.buffer.len() > chunk_size {
            let chunk = self.buffer.split_to(chunk_size).freeze();
            let length = chunk.len() as u64;
            self.target.send_chunk(self.sent, chunk, false).await?;
            self.sent += length;
        }
        Ok(())
    }
}

#[async_trait]
impl<T: UploadTarget> WriteStream for ChunkedUpload<T> {
    async fn write(&mut self, data: Bytes) -> AppResult<()> {
        if self.failed {
            return Err(AppError::new(ErrorKind::Cloud, "The upload already failed"));
        }
        self.buffer.extend_from_slice(&data);
        let result = self.flush_full_chunks().await;
        self.failed = result.is_err();
        result
    }

    async fn finish(mut self: Box<Self>) -> AppResult<()> {
        if self.failed {
            return Err(AppError::new(ErrorKind::Cloud, "The upload already failed"));
        }
        let remaining = std::mem::take(&mut self.buffer).freeze();
        if self.session_open {
            let offset = self.sent;
            self.target.send_chunk(offset, remaining, true).await
        } else {
            self.target.upload_whole(remaining).await
        }
    }
}

/// The value of a `Content-Range` header for `length` bytes at `offset`, with the total size
/// once it is known.
pub fn content_range(offset: u64, length: u64, total: Option<u64>) -> String {
    let total = total.map_or_else(|| "*".to_string(), |total| total.to_string());
    if length == 0 {
        format!("bytes */{total}")
    } else {
        format!("bytes {offset}-{}/{total}", offset + length - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Whole(usize),
        Session,
        Chunk(u64, usize, bool),
    }

    struct Recorder(Arc<Mutex<Vec<Call>>>);

    #[async_trait]
    impl UploadTarget for Recorder {
        async fn upload_whole(&mut self, data: Bytes) -> AppResult<()> {
            self.0.lock().unwrap().push(Call::Whole(data.len()));
            Ok(())
        }
        async fn start_session(&mut self) -> AppResult<()> {
            self.0.lock().unwrap().push(Call::Session);
            Ok(())
        }
        async fn send_chunk(&mut self, offset: u64, data: Bytes, last: bool) -> AppResult<()> {
            self.0
                .lock()
                .unwrap()
                .push(Call::Chunk(offset, data.len(), last));
            Ok(())
        }
    }

    async fn upload(size: usize, piece: usize) -> Vec<Call> {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut upload: Box<dyn WriteStream> =
            Box::new(ChunkedUpload::new(Recorder(calls.clone()), size as u64));
        let mut written = 0;
        while written < size {
            let length = piece.min(size - written);
            upload.write(Bytes::from(vec![0; length])).await.unwrap();
            written += length;
        }
        upload.finish().await.unwrap();
        let recorded = std::mem::take(&mut *calls.lock().unwrap());
        recorded
    }

    #[tokio::test]
    async fn small_files_go_in_one_request() {
        assert_eq!(upload(0, 1).await, vec![Call::Whole(0)]);
        assert_eq!(
            upload(SINGLE_REQUEST_LIMIT, 1 << 20).await,
            vec![Call::Whole(SINGLE_REQUEST_LIMIT)]
        );
    }

    #[tokio::test]
    async fn large_files_go_in_chunks_and_end_with_data() {
        let size = 2 * CHUNK_SIZE;
        assert_eq!(
            upload(size, 1 << 20).await,
            vec![
                Call::Session,
                Call::Chunk(0, CHUNK_SIZE, false),
                Call::Chunk(CHUNK_SIZE as u64, CHUNK_SIZE, true),
            ]
        );
        let size = CHUNK_SIZE + 5;
        assert_eq!(
            upload(size, 3 << 20).await,
            vec![
                Call::Session,
                Call::Chunk(0, CHUNK_SIZE, false),
                Call::Chunk(CHUNK_SIZE as u64, 5, true),
            ]
        );
    }

    #[test]
    fn chunks_grow_only_to_stay_under_the_cap() {
        assert_eq!(chunk_size_within(0, 10_000), CHUNK_SIZE);
        assert_eq!(chunk_size_within(50 << 30, 10_000), CHUNK_SIZE);
        let huge = 5u64 << 40;
        let chunk = chunk_size_within(huge, 10_000) as u64;
        assert!(huge.div_ceil(chunk) <= 10_000);
        assert_eq!(chunk % (1024 * 1024), 0);
    }

    #[test]
    fn formats_content_ranges() {
        assert_eq!(content_range(0, 10, Some(10)), "bytes 0-9/10");
        assert_eq!(content_range(10, 5, None), "bytes 10-14/*");
        assert_eq!(content_range(0, 0, Some(0)), "bytes */0");
    }
}
