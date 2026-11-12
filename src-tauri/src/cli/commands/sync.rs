//! `sync`: the app's rsync-style synchronization, with every file through the transfer queue.
//! The plan is made, conflicts are settled as asked, and the rest is carried out unattended,
//! as scheduled tasks do.

use serde_json::json;

use super::absolute_local;
use crate::cli::args::{Direction, SyncArgs, SyncConflict};
use crate::cli::context::Context;
use crate::cli::exit::Exit;
use crate::cli::output::{plural, Reply};
use crate::cli::transfers::{self, TransferSummary};
use crate::error::{AppError, AppResult};
use crate::format::format_size;
use crate::protocol::Protocol;
use crate::rsync::shell_quote;
use crate::sync::{
    SyncAction, SyncChoice, SyncDirection, SyncManager, SyncOptions, SyncRequest, SyncRunRequest,
};
use crate::transfer::TransferManager;

/// The rsync options the flags stand for, after the connection's own and before `--rsync` and
/// what follows `--`, so later ones win as they do in rsync.
fn rsync_arguments(saved: Option<&str>, arguments: &SyncArgs) -> String {
    let mut words: Vec<String> = saved
        .map(str::trim)
        .filter(|saved| !saved.is_empty())
        .map(str::to_string)
        .into_iter()
        .collect();
    let flags = [
        (arguments.delete, "--delete"),
        (arguments.checksum, "--checksum"),
        (arguments.size_only, "--size-only"),
        (arguments.dry_run, "--dry-run"),
    ];
    words.extend(
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, flag)| flag.to_string()),
    );
    // Includes go first, so they keep what a later exclude would leave out.
    words.extend(
        arguments
            .include
            .iter()
            .map(|pattern| shell_quote(&format!("--include={pattern}"))),
    );
    words.extend(
        arguments
            .exclude
            .iter()
            .map(|pattern| shell_quote(&format!("--exclude={pattern}"))),
    );
    if let Some(more) = &arguments.rsync {
        words.push(more.clone());
    }
    words.extend(
        arguments
            .rsync_arguments
            .iter()
            .map(|word| shell_quote(word)),
    );
    words.join(" ")
}

pub async fn synchronize(context: &Context, arguments: &SyncArgs) -> AppResult<Reply> {
    let local = absolute_local(&arguments.local)?
        .to_string_lossy()
        .into_owned();
    let connected = context.connected().await?;
    if connected.info.protocol != Protocol::Sftp {
        return Err(AppError::unsupported(format!(
            "Synchronizing needs SFTP; {} can use get and put",
            connected.info.protocol.display_name()
        )));
    }
    let remote = connected.resolve(&arguments.remote);
    let direction = match arguments.direction {
        Direction::Up => SyncDirection::Upload,
        Direction::Down => SyncDirection::Download,
        Direction::Both => SyncDirection::Both,
    };
    let manager = SyncManager::new(context.sessions.clone(), context.events.clone());
    let request_id = uuid::Uuid::new_v4().to_string();
    let compared = manager.compare(SyncRequest {
        request_id: request_id.clone(),
        session_id: connected.info.id.clone(),
        local_path: local.clone(),
        remote_path: remote.clone(),
        direction,
        options: SyncOptions::default(),
        arguments: rsync_arguments(connected.sync_arguments.as_deref(), arguments),
    });
    let plan = tokio::select! {
        plan = compared => plan?,
        _ = context.interrupt.cancelled() => {
            manager.cancel(&request_id);
            return Err(AppError::cancelled());
        }
    };

    let held_back = (!arguments.allow_empty_source)
        .then(|| manager.empty_mirrored_folder(&plan.plan_id))
        .flatten();
    let mut conflicts = 0u64;
    let mut choices = Vec::new();
    for item in &plan.items {
        let action = match (item.action, arguments.conflict) {
            (SyncAction::Conflict, SyncConflict::Skip) => {
                conflicts += 1;
                continue;
            }
            (SyncAction::Conflict, SyncConflict::Local) => SyncAction::Upload,
            (SyncAction::Conflict, SyncConflict::Remote) => SyncAction::Download,
            (action, _) if action.deletes() && held_back.is_some() => continue,
            (action, _) => action,
        };
        choices.push(SyncChoice {
            id: item.id,
            action,
        });
    }
    let planned = json!({
        "localRoot": plan.local_root,
        "remoteRoot": plan.remote_root,
        "arguments": plan.arguments,
        "dryRun": plan.dry_run,
        "createsTarget": plan.creates_target,
        "counts": plan.counts,
        "items": plan.items,
        "notes": plan.notes,
        "passedOver": plan.passed_over,
    });
    let mut notes: Vec<String> = plan.notes.clone();
    if conflicts > 0 {
        notes.push(format!(
            "{} left as they are; settle them with --conflict local or --conflict remote",
            plural(conflicts, "conflict")
        ));
    }
    if let Some(folder) = &held_back {
        notes.push(format!(
            "Nothing deleted because {folder} is empty; add --allow-empty-source if that is meant"
        ));
    }
    let unsettled = conflicts > 0 || held_back.is_some();
    let mut text = String::new();
    if arguments.itemize || plan.dry_run {
        for item in &plan.items {
            let itemized = match item.action {
                SyncAction::Conflict => "*conflict",
                _ => item.itemized.as_str(),
            };
            text.push_str(&format!("{itemized} {}\n", item.path));
        }
    }

    if choices.is_empty() {
        manager.discard(&plan.plan_id);
        text.push_str("Nothing to change");
        if plan.counts.unchanged > 0 {
            text.push_str(&format!(
                "; {} unchanged",
                plural(plan.counts.unchanged, "file")
            ));
        }
        for note in &notes {
            text.push_str(&format!("\n  note: {note}"));
        }
        let reply = Reply::new(
            json!({ "plan": planned, "conflicts": conflicts, "heldBack": held_back, "notes": notes }),
            text,
        );
        return Ok(if unsettled {
            reply.with_exit(Exit::Partial)
        } else {
            reply
        });
    }

    let settings = transfers::settings(&context.settings.transfers, &arguments.options);
    let (workers, verified) = (settings.workers, settings.verify_checksums);
    let queue = TransferManager::new(context.sessions.clone(), context.events.clone(), settings);
    let summary = manager
        .run(
            &queue,
            SyncRunRequest {
                plan_id: plan.plan_id.clone(),
                choices,
            },
        )
        .await?;
    if summary.dry_run {
        text.push_str(&format!(
            "Dry run: {} would be copied ({}), {} deleted",
            plural(summary.queued_files, "file"),
            format_size(summary.queued_bytes),
            plural(summary.deleted, "item")
        ));
        for note in &notes {
            text.push_str(&format!("\n  note: {note}"));
        }
        return Ok(Reply::new(
            json!({ "plan": planned, "run": summary, "conflicts": conflicts, "heldBack": held_back, "notes": notes }),
            text,
        ));
    }
    let copied: TransferSummary = transfers::wait(context, &queue, workers, verified).await?;
    let report = manager.finished(&summary.run_id).await.unwrap_or_default();
    let failures: Vec<String> = summary
        .failures
        .iter()
        .chain(&report.failures)
        .cloned()
        .collect();
    let deleted = summary.deleted.max(report.deleted);
    let mut parts = Vec::new();
    if report.copied > 0 || (deleted == 0 && summary.updated == 0) {
        parts.push(format!(
            "{} copied ({})",
            plural(report.copied, "file"),
            format_size(report.bytes)
        ));
    }
    if deleted > 0 {
        parts.push(format!("{} deleted", plural(deleted, "item")));
    }
    if summary.updated > 0 {
        parts.push(format!("{} updated", plural(summary.updated, "item")));
    }
    if report.failed > 0 {
        parts.push(format!("{} failed", plural(report.failed, "file")));
    }
    text.push_str(&parts.join(", "));
    for failure in &failures {
        text.push_str(&format!("\n  failed: {failure}"));
    }
    for note in &notes {
        text.push_str(&format!("\n  note: {note}"));
    }
    let failed = report.failed > 0 || !failures.is_empty();
    let reply = Reply::new(
        json!({
            "plan": planned,
            "run": summary,
            "copied": report.copied,
            "bytes": report.bytes,
            "deleted": deleted,
            "failed": report.failed,
            "failures": failures,
            "transfers": copied,
            "conflicts": conflicts,
            "heldBack": held_back,
            "notes": notes,
        }),
        text,
    );
    Ok(if failed || unsettled {
        reply.with_exit(Exit::Partial)
    } else {
        reply
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::{Cli, Command};
    use clap::Parser;

    fn sync_args(line: &[&str]) -> SyncArgs {
        let mut words = vec!["poros-cli", "sync"];
        words.extend_from_slice(line);
        match Cli::try_parse_from(words).unwrap().command {
            Command::Sync(arguments) => arguments,
            _ => unreachable!(),
        }
    }

    #[test]
    fn flags_become_rsync_options_in_order() {
        let arguments = sync_args(&[
            "-d",
            "up",
            "--delete",
            "--exclude",
            "*.tmp",
            "--include",
            "keep me.tmp",
            "--rsync",
            "--max-delete=5",
            "a",
            "b",
            "--",
            "--exclude=cache dir/",
        ]);
        assert_eq!(
            rsync_arguments(Some(" -a "), &arguments),
            "-a --delete '--include=keep me.tmp' '--exclude=*.tmp' --max-delete=5 '--exclude=cache dir/'"
        );
        let plain = sync_args(&["-d", "down", "a", "b"]);
        assert_eq!(rsync_arguments(None, &plain), "");
    }

    #[test]
    fn the_options_are_ones_rsync_takes() {
        let arguments = sync_args(&[
            "-d",
            "both",
            "--checksum",
            "--dry-run",
            "--exclude",
            "*.log",
            "a",
            "b",
        ]);
        let checked = crate::sync::check_options(
            SyncOptions::default(),
            &[&rsync_arguments(None, &arguments)],
            SyncDirection::Both,
        );
        assert!(checked.errors.is_empty(), "{:?}", checked.errors);
        assert!(checked.options.dry_run);
        assert_eq!(checked.options.excludes, ["*.log"]);
    }
}
