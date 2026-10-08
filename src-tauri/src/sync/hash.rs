//! Content comparison for the checksum mode: MD5 of each file on both sides. The server
//! hashes its own files with `md5sum` when it has it; otherwise they are read over SFTP.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use futures::stream::{self, StreamExt};
use tokio_util::sync::CancellationToken;

use super::tree::Counter;
use crate::checksum::{hash_file, parse_output, Algorithm};
use crate::error::{AppError, AppResult};
use crate::remote_path;
use crate::rsync::{run_command, shell_quote};
use crate::session::Session;
use crate::sftp::{ReadChunk, RemoteFs};

type Hash = Vec<u8>;

const PARALLEL_LOCAL_FILES: usize = 4;
const PARALLEL_COMMANDS: usize = 2;
const MAX_COMMAND_BYTES: usize = 32 * 1024;
const MAX_FILES_PER_COMMAND: usize = 500;
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;
const READ_CHUNK: u32 = 256 * 1024;
/// The shell's status when a command does not exist.
const COMMAND_NOT_FOUND: u32 = 127;

/// For each path, whether both sides have the same contents.
pub async fn same_contents(
    session: &Session,
    fs: &RemoteFs,
    local_root: &str,
    remote_root: &str,
    paths: &[String],
    counter: &Counter,
    cancel: &CancellationToken,
) -> AppResult<HashMap<String, bool>> {
    let local = hash_local(local_root, paths, counter);
    let remote = hash_remote(session, fs, remote_root, paths, counter);
    let (local, remote) = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(AppError::cancelled()),
        hashed = async { tokio::join!(local, remote) } => hashed,
    };
    let (local, remote) = (local?, remote?);
    Ok(paths
        .iter()
        .map(|path| {
            let same = matches!(
                (local.get(path), remote.get(path)),
                (Some(left), Some(right)) if left == right
            );
            (path.clone(), same)
        })
        .collect())
}

async fn hash_local(
    root: &str,
    paths: &[String],
    counter: &Counter,
) -> AppResult<HashMap<String, Hash>> {
    let hashed: Vec<(String, Option<Hash>)> = stream::iter(paths.iter().cloned())
        .map(|path| {
            let file = local_path(root, &path);
            async move {
                let hash = tokio::task::spawn_blocking(move || hash_file(&file, Algorithm::Md5))
                    .await
                    .ok()
                    .and_then(Result::ok);
                counter.fetch_add(1, Ordering::Relaxed);
                (path, hash)
            }
        })
        .buffer_unordered(PARALLEL_LOCAL_FILES)
        .collect()
        .await;
    Ok(hashed
        .into_iter()
        .filter_map(|(path, hash)| hash.map(|hash| (path, hash)))
        .collect())
}

pub fn local_path(root: &str, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(PathBuf::from(root), |path, component| path.join(component))
}

async fn hash_remote(
    session: &Session,
    fs: &RemoteFs,
    root: &str,
    paths: &[String],
    counter: &Counter,
) -> AppResult<HashMap<String, Hash>> {
    if let Some(hashes) = hash_with_md5sum(session, root, paths, counter).await {
        return Ok(hashes);
    }
    let mut hashes = HashMap::new();
    for path in paths {
        if let Ok(hash) = hash_over_sftp(fs, &remote_path::join(root, path)).await {
            hashes.insert(path.clone(), hash);
        }
        counter.fetch_add(1, Ordering::Relaxed);
    }
    Ok(hashes)
}

/// `None` when the server cannot run `md5sum`.
async fn hash_with_md5sum(
    session: &Session,
    root: &str,
    paths: &[String],
    counter: &Counter,
) -> Option<HashMap<String, Hash>> {
    let batches = command_batches(root, paths);
    let results: Vec<Option<HashMap<String, Hash>>> = stream::iter(batches)
        .map(|(command, by_argument)| async move {
            let channel = session.open_command_channel().await.ok()?;
            let output = run_command(channel, &command, OUTPUT_LIMIT).await.ok()?;
            // Status 1 means some files could not be read; the others are still listed.
            if output.exit_status == Some(COMMAND_NOT_FOUND) || output.exit_status.is_none() {
                return None;
            }
            let text = String::from_utf8_lossy(&output.output);
            let hashes: HashMap<String, Hash> = parse_output(&text, Algorithm::Md5)
                .into_iter()
                .filter_map(|(name, hash)| by_argument.get(&name).map(|path| (path.clone(), hash)))
                .collect();
            // A server that runs one fixed program for every command answers with nothing.
            if hashes.is_empty() && output.exit_status == Some(0) {
                return None;
            }
            counter.fetch_add(by_argument.len() as u64, Ordering::Relaxed);
            Some(hashes)
        })
        .buffer_unordered(PARALLEL_COMMANDS)
        .collect()
        .await;
    let mut hashes = HashMap::new();
    for result in results {
        hashes.extend(result?);
    }
    Some(hashes)
}

/// Commands that each fit a modest command line, with each argument's relative path.
fn command_batches(root: &str, paths: &[String]) -> Vec<(String, HashMap<String, String>)> {
    let mut batches = Vec::new();
    let mut command = String::from("md5sum --");
    let mut by_argument = HashMap::new();
    for path in paths {
        let argument = remote_path::join(root, path);
        let quoted = shell_quote(&argument);
        let full = command.len() + quoted.len() + 1 > MAX_COMMAND_BYTES
            || by_argument.len() >= MAX_FILES_PER_COMMAND;
        if full && !by_argument.is_empty() {
            batches.push((
                std::mem::replace(&mut command, String::from("md5sum --")),
                std::mem::take(&mut by_argument),
            ));
        }
        command.push(' ');
        command.push_str(&quoted);
        by_argument.insert(argument, path.clone());
    }
    if !by_argument.is_empty() {
        batches.push((command, by_argument));
    }
    batches
}

async fn hash_over_sftp(fs: &RemoteFs, path: &str) -> AppResult<Hash> {
    let handle = fs.open_for_read(path).await?;
    let chunk = fs.read_size(READ_CHUNK).max(1);
    let mut digest = Algorithm::Md5.hasher();
    let mut offset = 0;
    let result = loop {
        match fs.read_chunk(&handle, offset, chunk).await {
            Ok(ReadChunk::Data(data)) => {
                offset += data.len() as u64;
                digest.update(&data);
            }
            Ok(ReadChunk::Eof) => break Ok(()),
            Err(error) => break Err(error),
        }
    };
    let _ = fs.close_handle(handle).await;
    result.map(|_| digest.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_long_argument_lists() {
        let paths: Vec<String> = (0..1200).map(|index| format!("dir/file-{index}")).collect();
        let batches = command_batches("/srv", &paths);
        assert_eq!(batches.len(), 3);
        assert!(batches
            .iter()
            .all(|(command, _)| command.len() <= MAX_COMMAND_BYTES));
        assert_eq!(batches[0].1["/srv/dir/file-0"], "dir/file-0");
        assert!(batches[0].0.starts_with("md5sum -- '/srv/dir/file-0'"));
    }
}
