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
use crate::sync::{
    SyncAction, SyncDirection, SyncManager, SyncOptions, SyncPlanView, SyncReason, SyncRequest,
};

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
        HashAlgorithm::Sha512 => Algorithm::Sha512,
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
    let manager = SyncManager::new(context.sessions.clone(), context.events.clone());
    let request_id = uuid::Uuid::new_v4().to_string();
    let compared = manager.compare(SyncRequest {
        request_id: request_id.clone(),
        session_id: connected.info.id.clone(),
        local_path: local.to_string(),
        remote_path: remote.to_string(),
        direction: SyncDirection::Upload,
        options: SyncOptions::default(),
        arguments: comparison_arguments(arguments),
    });
    let plan = tokio::select! {
        plan = compared => plan?,
        _ = context.interrupt.cancelled() => {
            manager.cancel(&request_id);
            return Err(AppError::cancelled());
        }
    };
    manager.discard(&plan.plan_id);
    Ok(folder_report(&plan, local, remote))
}

/// What `verify` says about two folders: the same, how they differ, and what it could not
/// compare.
fn folder_report(plan: &SyncPlanView, local: &str, remote: &str) -> Reply {
    let (mut only_local, mut only_remote, mut different) = (Vec::new(), Vec::new(), Vec::new());
    for item in &plan.items {
        let listed = json!({ "path": item.path, "isDir": item.is_dir, "files": item.files });
        match (item.action, item.reason) {
            (SyncAction::Upload, SyncReason::New) => only_local.push(listed),
            (SyncAction::DeleteRemote, _) => only_remote.push(listed),
            _ => different.push(listed),
        }
    }
    // Unreadable folders, special files and links that lead nowhere, on either side.
    let mut not_compared = plan.passed_over.clone();
    not_compared.sort();
    not_compared.dedup();
    let differences = only_local.len() + only_remote.len() + different.len();
    let same = differences == 0 && plan.counts.passed_over == 0;
    let unchanged = plural(plan.counts.unchanged, "file");
    let mut text = if same {
        format!("Same contents: {unchanged} in {local} and {remote}")
    } else if differences == 0 {
        format!("Not everything could be compared: {unchanged} the same in {local} and {remote}")
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
    for path in &not_compared {
        text.push_str(&format!("\n  not compared: {path}"));
    }
    for note in &plan.notes {
        text.push_str(&format!("\n  note: {note}"));
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
            "notCompared": not_compared,
            "notes": plan.notes,
        }),
        text,
    );
    match (differences, same) {
        (0, true) => reply,
        // What was compared matches, but the rest is unknown.
        (0, false) => reply.with_exit(Exit::Partial),
        _ => reply.with_exit(Exit::Integrity),
    }
}

/// A comparison as `rsync --checksum --delete --dry-run` makes it. Nothing is deleted, so files
/// only on the server are listed even when a local folder could not be read.
fn comparison_arguments(arguments: &VerifyArgs) -> String {
    let mut words = vec![
        "--checksum".to_string(),
        "--delete".to_string(),
        "--ignore-errors".to_string(),
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
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::{Cli, Command};
    use crate::sync::{Counts, SyncDirection, SyncItem};
    use clap::Parser;

    /// The plan of a comparison in which the walker could not read this computer's `private`
    /// folder. Errors are ignored, so the file only on the server is listed all the same.
    fn plan_without_private(items: Vec<SyncItem>) -> SyncPlanView {
        SyncPlanView {
            plan_id: "plan".into(),
            local_root: "/home/me/site".into(),
            remote_root: "/srv/site".into(),
            items,
            counts: Counts {
                unchanged: 2,
                passed_over: 1,
                ..Counts::default()
            },
            passed_over: vec!["private".into()],
            notes: Vec::new(),
            arguments: String::new(),
            dry_run: true,
            creates_target: false,
            removes_unchanged: 0,
        }
    }

    #[test]
    fn a_local_folder_that_could_not_be_read_is_named_and_the_rest_still_compared() {
        let stale = SyncItem {
            id: 1,
            path: "stale.txt".into(),
            is_dir: false,
            action: SyncAction::DeleteRemote,
            reason: SyncReason::Extraneous,
            files: 1,
            bytes: 6,
            local: None,
            remote: None,
            itemized: "*deleting".into(),
        };
        let report = folder_report(&plan_without_private(vec![stale]), "site", "/srv/site");
        assert_eq!(report.exit, Exit::Integrity);
        assert_eq!(report.value["same"], false);
        assert_eq!(report.value["onlyRemote"][0]["path"], "stale.txt");
        assert_eq!(report.value["notCompared"], serde_json::json!(["private"]));
        assert_eq!(
            report.text,
            "1 difference between site and /srv/site\n  only on the server: stale.txt\n  not \
             compared: private"
        );

        let report = folder_report(&plan_without_private(Vec::new()), "site", "/srv/site");
        assert_eq!(report.exit, Exit::Partial);
        assert_eq!(report.value["same"], false);
        assert_eq!(
            report.text,
            "Not everything could be compared: 2 files the same in site and /srv/site\n  not \
             compared: private"
        );
    }

    #[test]
    fn folders_are_compared_whatever_could_not_be_read() {
        let cli = Cli::try_parse_from([
            "poros-cli",
            "verify",
            "--exclude",
            "*.log",
            "site",
            "/srv/site",
        ])
        .unwrap();
        let Command::Verify(arguments) = cli.command else {
            panic!("not verify");
        };
        let checked = crate::sync::check_options(
            SyncOptions::default(),
            &[&comparison_arguments(&arguments)],
            SyncDirection::Upload,
        );
        assert!(checked.errors.is_empty(), "{:?}", checked.errors);
        assert!(checked.options.delete_extraneous && checked.options.ignore_errors);
        assert_eq!(checked.options.excludes, ["*.log"]);
    }
}
