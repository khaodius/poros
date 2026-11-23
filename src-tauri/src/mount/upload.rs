//! Sends a spooled file to the server the way the transfer queue sends an upload: through
//! `RemoteFileSystem::open_write`, then checked as a finished transfer is. The server must
//! hold as many bytes as were sent, and where the transfer settings ask for uploads to be
//! read back (FTP and WebDAV servers, which cannot hash a file themselves, and vaults), the
//! file is read back and compared with a SHA-256 of what was sent.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use super::backend::{Backend, Purpose};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::format::format_size;
use crate::protocol::{Protocol, RemoteFileSystem, WriteRequest};
use crate::remote_path;

const CHUNK: usize = 256 * 1024;
/// The longest one step of an upload may take before the server counts as stalled.
const STEP_LIMIT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub local: PathBuf,
    pub remote_path: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
}

/// How a finished upload is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadCheck {
    /// The server holds as many bytes as were sent.
    Size,
    /// The file is also read back and compared with what was sent.
    ReadBack,
}

/// Says how an upload to a given server is checked.
pub type ChooseCheck = dyn Fn(&dyn RemoteFileSystem) -> UploadCheck + Send + Sync;

/// Chooses the check for a server, given whether the transfer settings ask for uploads to be
/// verified and read back. Mirrors what the transfer queue reads back.
pub fn check_for(files: &dyn RemoteFileSystem, verify: bool, read_back: bool) -> UploadCheck {
    let transport = files.transport();
    let in_vault = files.protocol() == Protocol::Cryptomator;
    if verify
        && read_back
        && (transport.is_ftp()
            || transport.is_webdav()
            || (in_vault && transport == Protocol::Sftp))
    {
        UploadCheck::ReadBack
    } else {
        UploadCheck::Size
    }
}

async fn step<T>(work: impl std::future::Future<Output = AppResult<T>>) -> AppResult<T> {
    match tokio::time::timeout(STEP_LIMIT, work).await {
        Ok(result) => result,
        Err(_) => Err(AppError::new(
            ErrorKind::Timeout,
            "The server stopped answering during the upload",
        )),
    }
}

/// Uploads `request.local` to `request.remote_path`, adding the bytes sent to `sent`.
pub async fn upload(
    backend: &dyn Backend,
    request: &UploadRequest,
    sent: &AtomicU64,
    choose_check: &ChooseCheck,
) -> AppResult<()> {
    let lease = backend.lease(Purpose::Upload).await?;
    let files = lease.files.clone();
    let check = choose_check(files.as_ref());
    let name = remote_path::file_name(&request.remote_path).to_string();
    let with_path = |error: AppError| error.with_path(request.remote_path.clone());

    let mut file = tokio::fs::File::open(&request.local)
        .await
        .map_err(AppError::from)?;
    let mut writer = step(files.clone().open_write(WriteRequest {
        path: request.remote_path.clone(),
        offset: 0,
        size: request.size,
        modified: request.modified,
        permissions: request.permissions,
    }))
    .await
    .map_err(with_path)?;
    let mut hasher = Sha256::new();
    let mut remaining = request.size;
    while remaining > 0 {
        let wanted = remaining.min(CHUNK as u64) as usize;
        let mut buffer = vec![0; wanted];
        let mut filled = 0;
        while filled < wanted {
            let read = file.read(&mut buffer[filled..]).await?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        if filled < wanted {
            return Err(AppError::integrity(format!(
                "{name} changed while it was being uploaded"
            )));
        }
        if check == UploadCheck::ReadBack {
            hasher.update(&buffer);
        }
        step(writer.write(Bytes::from(buffer)))
            .await
            .map_err(with_path)?;
        sent.fetch_add(wanted as u64, Ordering::Relaxed);
        remaining -= wanted as u64;
    }
    step(writer.finish()).await.map_err(with_path)?;

    // A server that cannot find what it just stored gives nothing to compare.
    if let Some(stored) = step(files.stat(&request.remote_path)).await? {
        if stored.is_dir || stored.size != request.size {
            return Err(AppError::integrity(format!(
                "The server holds {} of the {} sent for {name}",
                format_size(stored.size),
                format_size(request.size)
            ))
            .with_path(request.remote_path.clone()));
        }
    }
    if check == UploadCheck::ReadBack {
        let stored = read_back_digest(&files, &request.remote_path, request.size).await?;
        if stored != hasher.finalize().as_slice() {
            return Err(AppError::integrity(format!(
                "The copy of {name} on the server, read back, does not match what was written"
            ))
            .with_path(request.remote_path.clone()));
        }
    }
    if request.modified.is_some() && !files.transport().uploads_carry_time() {
        // Some servers refuse attribute changes; the file itself is complete.
        if let Err(error) = files
            .set_attributes(&request.remote_path, request.modified, None)
            .await
        {
            log::debug!(
                "Could not set the time of {}: {}",
                request.remote_path,
                error.message
            );
        }
    }
    Ok(())
}

async fn read_back_digest(
    files: &Arc<dyn RemoteFileSystem>,
    path: &str,
    size: u64,
) -> AppResult<Vec<u8>> {
    let mut stream = step(files.clone().open_read(path, 0..size)).await?;
    let mut hasher = Sha256::new();
    let read = async {
        while let Some(chunk) = step(stream.next_chunk()).await? {
            hasher.update(&chunk);
        }
        Ok(())
    }
    .await;
    let finished = stream.finish().await;
    read.and(finished)
        .map_err(|error: AppError| error.with_path(path))?;
    Ok(hasher.finalize().to_vec())
}
