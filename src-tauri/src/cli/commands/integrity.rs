//! Checking contents: hashes of server files, and whether a local and a server copy match.

use std::path::Path;

use serde_json::json;

use super::browse::read_range;
use super::{absolute_local, each, expand, missing};
use crate::checksum::{hash_file, parse_output, Algorithm};
use crate::cli::args::{ChecksumArgs, HashAlgorithm, VerifyArgs};
use crate::cli::context::{Connected, Context};
use crate::cli::exit::Exit;
use crate::cli::output::{plural, Reply};
use crate::error::{AppError, AppResult};
use crate::protocol::Protocol;
use crate::rsync::{run_command, shell_quote};
use crate::sync::{SyncAction, SyncDirection, SyncManager, SyncOptions, SyncReason, SyncRequest};

const COMMAND_OUTPUT_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    /// The server's own `sha256sum`, `sha1sum` or `md5sum`.
    ServerCommand,
    /// Read over the connection and hashed here.
    Streamed,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Self::ServerCommand => "serverCommand",
            Self::Streamed => "streamed",
        }
    }
}

fn algorithm(choice: HashAlgorithm) -> Algorithm {
    match choice {
        HashAlgorithm::Sha256 => Algorithm::Sha256,
        HashAlgorithm::Sha1 => Algorithm::Sha1,
        HashAlgorithm::Md5 => Algorithm::Md5,
    }
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The digest of a server file, from the server's command when it has one.
async fn remote_digest(
    connected: &Connected,
    path: &str,
    algorithm: Algorithm,
    stream_only: bool,
) -> AppResult<(Vec<u8>, Method)> {
    let files = connected.files();
    let stat = files.stat(path).await?.ok_or_else(|| missing(path))?;
    if stat.is_dir {
        return Err(AppError::invalid(format!("{path} is a folder")).with_path(path));
    }
    if !stream_only && connected.info.protocol == Protocol::Sftp {
        match server_digest(connected, path, algorithm).await {
            Ok(Some(digest)) => return Ok((digest, Method::ServerCommand)),
            Ok(None) => log::info!(
                "The server could not run {} on {path}; reading it instead",
                algorithm.command()
            ),
            Err(error) => log::info!("{}; reading {path} instead", error.message),
        }
    }
    let mut hasher = algorithm.hasher();
    read_range(files, path, 0..stat.size, |chunk| {
        hasher.update(chunk);
        true
    })
    .await?;
    Ok((hasher.finish(), Method::Streamed))
}

async fn server_digest(
    connected: &Connected,
    path: &str,
    algorithm: Algorithm,
) -> AppResult<Option<Vec<u8>>> {
    let channel = connected.session.open_command_channel().await?;
    // One file per command: SFTPGo hashes only the last argument.
    let command = format!("{} -- {}", algorithm.command(), shell_quote(path));
    let output = run_command(channel, &command, COMMAND_OUTPUT_LIMIT).await?;
    if output.exit_status != Some(0) {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.output);
    Ok(parse_output(&text, algorithm)
        .into_iter()
        .next()
        .map(|(_, digest)| digest))
}

pub async fn checksum(context: &Context, arguments: &ChecksumArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let algorithm = algorithm(arguments.algorithm);
    let paths = expand(connected, &arguments.paths).await?;
    each(&paths, |path| async move {
        let (digest, method) = remote_digest(connected, path, algorithm, arguments.stream).await?;
        let digest = hex(&digest);
        let text = format!("{digest}  {path}");
        let value = json!({
            "path": path,
            "algorithm": algorithm.command().trim_end_matches("sum"),
            "digest": digest,
            "method": method.name(),
        });
        Ok((value, text))
    })
    .await
}

pub async fn verify(context: &Context, arguments: &VerifyArgs) -> AppResult<Reply> {
    let local = absolute_local(&arguments.local)?;
    let local_text = local.to_string_lossy().into_owned();
    let metadata = std::fs::metadata(&local).map_err(|_| missing(&local_text))?;
    let connected = context.connected().await?;
    let remote = connected.resolve(&arguments.remote);
    if metadata.is_dir() {
        verify_folder(context, connected, &local_text, &remote, arguments).await
    } else {
        verify_file(connected, &local, metadata.len(), &remote).await
    }
}

async fn verify_file(
    connected: &Connected,
    local: &Path,
    local_size: u64,
    remote: &str,
) -> AppResult<Reply> {
    let local_text = local.to_string_lossy().into_owned();
    let stat = connected
        .files()
        .stat(remote)
        .await?
        .ok_or_else(|| missing(remote))?;
    if stat.is_dir {
        return Err(AppError::invalid(format!(
            "{remote} is a folder and {local_text} a file"
        )));
    }
    let algorithm = Algorithm::Sha256;
    if stat.size != local_size {
        let text = format!(
            "Different: {local_text} has {local_size} bytes, {remote} has {}",
            stat.size
        );
        return Ok(Reply::new(
            json!({
                "local": local_text,
                "remote": remote,
                "same": false,
                "localSize": local_size,
                "remoteSize": stat.size,
            }),
            text,
        )
        .with_exit(Exit::Integrity));
    }
    let hashing = {
        let local = local.to_path_buf();
        tokio::task::spawn_blocking(move || hash_file(&local, algorithm))
    };
    let (local_digest, remote_digest) =
        tokio::join!(hashing, remote_digest(connected, remote, algorithm, false));
    let local_digest =
        local_digest?.map_err(|error| AppError::from(error).with_path(local_text.clone()))?;
    let (remote_digest, method) = remote_digest?;
    let same = local_digest == remote_digest;
    let text = if same {
        format!(
            "Same: {local_text} and {remote} (SHA-256 {})",
            hex(&local_digest)
        )
    } else {
        format!(
            "Different: {local_text} is SHA-256 {}, {remote} is {}",
            hex(&local_digest),
            hex(&remote_digest)
        )
    };
    let reply = Reply::new(
        json!({
            "local": local_text,
            "remote": remote,
            "same": same,
            "size": local_size,
            "algorithm": "sha256",
            "localDigest": hex(&local_digest),
            "remoteDigest": hex(&remote_digest),
            "method": method.name(),
        }),
        text,
    );
    Ok(if same {
        reply
    } else {
        reply.with_exit(Exit::Integrity)
    })
}

/// Folders are compared the way `rsync --checksum --delete --dry-run` would, with the
/// synchronization's own listing and hashing.
async fn verify_folder(
    context: &Context,
    connected: &Connected,
    local: &str,
    remote: &str,
    arguments: &VerifyArgs,
) -> AppResult<Reply> {
    if connected.info.protocol != Protocol::Sftp {
        return Err(AppError::unsupported(format!(
            "Comparing folders needs SFTP; {} can check single files",
            connected.info.protocol.display_name()
        )));
    }
    let mut words = vec![
        "--checksum".to_string(),
        "--delete".to_string(),
        "--no-times".to_string(),
    ];
    words.extend(
        arguments
            .exclude
            .iter()
            .map(|pattern| shell_quote(&format!("--exclude={pattern}"))),
    );
    if let Some(more) = &arguments.rsync {
        words.push(more.clone());
    }
    let manager = SyncManager::new(context.sessions.clone(), context.events.clone());
    let request_id = uuid::Uuid::new_v4().to_string();
    let compared = manager.compare(SyncRequest {
        request_id: request_id.clone(),
        session_id: connected.info.id.clone(),
        local_path: local.to_string(),
        remote_path: remote.to_string(),
        direction: SyncDirection::Upload,
        options: SyncOptions::default(),
        arguments: words.join(" "),
    });
    let plan = tokio::select! {
        plan = compared => plan?,
        _ = context.interrupt.cancelled() => {
            manager.cancel(&request_id);
            return Err(AppError::cancelled());
        }
    };
    manager.discard(&plan.plan_id);
    let (mut only_local, mut only_remote, mut different) = (Vec::new(), Vec::new(), Vec::new());
    for item in &plan.items {
        let listed = json!({ "path": item.path, "isDir": item.is_dir, "files": item.files });
        match (item.action, item.reason) {
            (SyncAction::Upload, SyncReason::New) => only_local.push(listed),
            (SyncAction::DeleteRemote, _) => only_remote.push(listed),
            _ => different.push(listed),
        }
    }
    let differences = only_local.len() + only_remote.len() + different.len();
    let same = differences == 0;
    let mut text = if same {
        format!(
            "Same contents: {} in {local} and {remote}",
            plural(plan.counts.unchanged, "file")
        )
    } else {
        format!(
            "{} between {local} and {remote}",
            plural(differences as u64, "difference")
        )
    };
    for (label, items) in [
        ("only local", &only_local),
        ("only on the server", &only_remote),
        ("differs", &different),
    ] {
        for item in items.iter() {
            text.push_str(&format!(
                "\n  {label}: {}",
                item["path"].as_str().unwrap_or_default()
            ));
        }
    }
    let reply = Reply::new(
        json!({
            "local": plan.local_root,
            "remote": plan.remote_root,
            "same": same,
            "unchanged": plan.counts.unchanged,
            "excluded": plan.counts.excluded,
            "onlyLocal": only_local,
            "onlyRemote": only_remote,
            "different": different,
            "notes": plan.notes,
        }),
        text,
    );
    Ok(if same {
        reply
    } else {
        reply.with_exit(Exit::Integrity)
    })
}
