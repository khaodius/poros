//! Sends a spooled file to the server the way the transfer queue sends an upload: through
//! `RemoteFileSystem::open_write`, then checked as a finished transfer is. The server must
//! hold as many bytes as were sent. Where the transfer settings ask for uploads to be
//! verified, an SFTP server's own `sha256sum` must agree with a SHA-256 of what was sent, and
//! where they ask for reading back (FTP and WebDAV servers, which cannot hash a file
//! themselves, vaults, and SFTP servers that run no commands), the file is read back and
//! compared with it.
//!
//! On SFTP and FTP servers the file is written under a name of its own beside the original
//! and moved into its place once checked, so an upload cut short leaves the original whole.
//! Object stores and WebDAV servers replace a file only once all of it arrived anyway.

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
/// How much of the original name the name an upload is written under keeps, in bytes, so it
/// stays within what servers allow.
const STAGED_NAME_KEPT: usize = 100;

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
    /// The server hashes its copy, or, if it cannot and `read_back` says so, it is read back.
    ServerHash { read_back: bool },
}

impl UploadCheck {
    fn hashes(self) -> bool {
        self != UploadCheck::Size
    }
}

/// Says how an upload to a given server is checked.
pub type ChooseCheck = dyn Fn(&dyn RemoteFileSystem) -> UploadCheck + Send + Sync;

/// Chooses the check for a server, given whether the transfer settings ask for uploads to be
/// verified and read back. Mirrors what the transfer queue reads back.
pub fn check_for(files: &dyn RemoteFileSystem, verify: bool, read_back: bool) -> UploadCheck {
    let transport = files.transport();
    let in_vault = files.protocol() == Protocol::Cryptomator;
    if verify && !in_vault && transport == Protocol::Sftp {
        UploadCheck::ServerHash { read_back }
    } else if verify
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

/// Where an upload to `target` is written before it takes the file's place, on servers that
/// can put one file in another's place in a single step. `None` where it is written in place:
/// object stores and WebDAV servers replace a file only once it is whole anyway, and a vault
/// keeps its files its own way.
fn staged_path(files: &dyn RemoteFileSystem, target: &str) -> Option<String> {
    let transport = files.transport();
    let plain = files.protocol() == transport;
    if !plain || !(transport == Protocol::Sftp || transport.is_ftp()) {
        return None;
    }
    let folder = remote_path::parent(target)?;
    let name = remote_path::file_name(target);
    let mut kept = String::new();
    for character in name.chars() {
        if kept.len() + character.len_utf8() > STAGED_NAME_KEPT {
            break;
        }
        kept.push(character);
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    Some(remote_path::join(
        &folder,
        &format!(".{kept}.poros-{}", &token[..8]),
    ))
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
    let target = &request.remote_path;
    let Some(staged) = staged_path(files.as_ref(), target) else {
        return send_and_check(
            backend,
            &files,
            request,
            target,
            request.permissions,
            sent,
            choose_check,
        )
        .await;
    };
    let existing = step(files.stat(target)).await?;
    // The new file takes the place of the old one, which permissions on the old file would
    // not stop, so a file nobody may write stays as it is.
    if existing
        .and_then(|found| found.permissions)
        .is_some_and(|mode| mode & 0o222 == 0)
    {
        return Err(AppError::new(
            ErrorKind::PermissionDenied,
            format!(
                "{} is read-only on the server",
                remote_path::file_name(target)
            ),
        )
        .with_path(target.clone()));
    }
    // A new file gets the server's usual permissions; the old file's go on.
    let permissions = request
        .permissions
        .or(existing.and_then(|found| found.permissions));
    let sent_staged = send_and_check(
        backend,
        &files,
        request,
        &staged,
        permissions,
        sent,
        choose_check,
    )
    .await;
    let placed = match sent_staged {
        Ok(()) => put_in_place(&files, &staged, target).await,
        Err(error) if error.kind == ErrorKind::PermissionDenied => {
            // A folder that takes no new files may still let its files be written.
            let _ = step(files.delete(std::slice::from_ref(&staged))).await;
            sent.store(0, Ordering::Relaxed);
            return send_and_check(
                backend,
                &files,
                request,
                target,
                request.permissions,
                sent,
                choose_check,
            )
            .await;
        }
        Err(error) => Err(error),
    };
    if let Err(error) = placed {
        if !error.is_connection_lost() {
            let _ = step(files.delete(std::slice::from_ref(&staged))).await;
        }
        return Err(error.with_path(target.clone()));
    }
    set_time(&files, request, target).await;
    Ok(())
}

/// Moves `from` to `to` in the same folder, by renaming where the server cannot move.
async fn relocate(files: &Arc<dyn RemoteFileSystem>, from: &str, to: &str) -> AppResult<()> {
    match step(files.move_path(from, to)).await {
        Err(error) if error.kind == ErrorKind::Unsupported => {
            step(files.rename(from, remote_path::file_name(to)))
                .await
                .map(|_| ())
        }
        moved => moved,
    }
}

/// Puts the checked upload at `staged` in the place of `target`.
async fn put_in_place(
    files: &Arc<dyn RemoteFileSystem>,
    staged: &str,
    target: &str,
) -> AppResult<()> {
    let refused = match relocate(files, staged, target).await {
        Ok(()) => return Ok(()),
        Err(error) if error.is_connection_lost() || error.kind == ErrorKind::Timeout => {
            return Err(error)
        }
        Err(error) => error,
    };
    // Some servers move nothing onto a file that exists: the old one steps aside first, and
    // back again if the new one still cannot take its place.
    let aside = format!("{staged}.old");
    if relocate(files, target, &aside).await.is_err() {
        return Err(refused);
    }
    match relocate(files, staged, target).await {
        Ok(()) => {
            let _ = step(files.delete(&[aside])).await;
            Ok(())
        }
        Err(error) => {
            let _ = relocate(files, &aside, target).await;
            Err(error)
        }
    }
}

/// Writes the spool to `path` on the server and checks what arrived there.
async fn send_and_check(
    backend: &dyn Backend,
    files: &Arc<dyn RemoteFileSystem>,
    request: &UploadRequest,
    path: &str,
    permissions: Option<u32>,
    sent: &AtomicU64,
    choose_check: &ChooseCheck,
) -> AppResult<()> {
    let check = choose_check(files.as_ref());
    let name = remote_path::file_name(&request.remote_path).to_string();
    let with_path = |error: AppError| error.with_path(request.remote_path.clone());

    let mut file = tokio::fs::File::open(&request.local)
        .await
        .map_err(AppError::from)?;
    let mut writer = step(files.clone().open_write(WriteRequest {
        path: path.to_string(),
        offset: 0,
        size: request.size,
        modified: request.modified,
        permissions,
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
        if check.hashes() {
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
    if let Some(stored) = step(files.stat(path)).await? {
        if stored.is_dir || stored.size != request.size {
            return Err(AppError::integrity(format!(
                "The server holds {} of the {} sent for {name}",
                format_size(stored.size),
                format_size(request.size)
            ))
            .with_path(request.remote_path.clone()));
        }
    }
    let compared = match check {
        UploadCheck::Size => None,
        UploadCheck::ReadBack => Some(("read back", None)),
        UploadCheck::ServerHash { read_back } => match backend.server_sha256(path).await {
            Some(digest) => Some(("hashed by the server", Some(digest))),
            None if read_back => Some(("read back", None)),
            None => None,
        },
    };
    if let Some((how, digest)) = compared {
        let stored = match digest {
            Some(digest) => digest,
            None => read_back_digest(files, path, request.size)
                .await
                .map_err(with_path)?,
        };
        if stored != hasher.finalize().as_slice() {
            return Err(AppError::integrity(format!(
                "The copy of {name} on the server, {how}, does not match what was written"
            ))
            .with_path(request.remote_path.clone()));
        }
    }
    if path == request.remote_path {
        set_time(files, request, path).await;
    }
    Ok(())
}

/// Gives the uploaded file the time a program set, where the upload did not carry it.
async fn set_time(files: &Arc<dyn RemoteFileSystem>, request: &UploadRequest, path: &str) {
    if request.modified.is_none() || files.transport().uploads_carry_time() {
        return;
    }
    // Some servers refuse attribute changes; the file itself is complete.
    if let Err(error) = files.set_attributes(path, request.modified, None).await {
        log::debug!("Could not set the time of {path}: {}", error.message);
    }
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
