//! `mount`: the server as a folder on this computer for as long as the command runs, which
//! makes drives usable on machines without the app's window.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::super::args::MountArgs;
use super::super::context::{Context, APP_IDENTIFIER};
use super::super::exit::Exit;
use super::super::output::Reply;
use crate::error::AppResult;
use crate::mount::manager::{MountManager, MountRequest, SPOOL_FOLDER};

/// How long Ctrl+C waits for uploads under way before the drive goes regardless, inside the
/// time the command line gives a command to stop.
const UPLOAD_GRACE: Duration = Duration::from_secs(2);
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

pub async fn mount(context: &Context, arguments: &MountArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let remote_root = match &arguments.path {
        Some(path) => connected.resolve(path),
        None => connected.info.initial_path.clone(),
    };
    let mount_point = if arguments.mount_point.is_absolute() {
        arguments.mount_point.clone()
    } else {
        std::env::current_dir()?.join(&arguments.mount_point)
    };
    let spool_root = dirs::cache_dir()
        .map(|folder| folder.join(APP_IDENTIFIER).join(SPOOL_FOLDER))
        .unwrap_or_else(|| std::env::temp_dir().join("poros").join(SPOOL_FOLDER));
    let transfers = context.settings.transfers.clone();
    let manager = MountManager::new(
        context.sessions.clone(),
        context.events.clone(),
        spool_root,
        Arc::new(move || (transfers.verify_checksums, transfers.read_back)),
    );
    let drive = manager
        .mount(MountRequest {
            session_id: connected.info.id.clone(),
            mount_point: mount_point.to_string_lossy().into_owned(),
            remote_root,
            read_only: arguments.read_only,
        })
        .await?;
    if !context.options.quiet {
        eprintln!(
            "Mounted {} of {} at {}{}. Press Ctrl+C to unmount.",
            drive.remote_root,
            drive.label,
            drive.mount_point,
            if drive.read_only { ", read-only" } else { "" }
        );
    }

    let mut ended_elsewhere = false;
    loop {
        tokio::select! {
            _ = context.interrupt.cancelled() => break,
            _ = tokio::time::sleep(WATCH_INTERVAL) => {
                if manager.list().is_empty() {
                    ended_elsewhere = true;
                    break;
                }
            }
        }
    }
    let last = manager
        .list()
        .into_iter()
        .find(|shown| shown.id == drive.id)
        .unwrap_or(drive.clone());
    if !ended_elsewhere {
        let waiting = Instant::now();
        while waiting.elapsed() < UPLOAD_GRACE
            && manager
                .list()
                .iter()
                .any(|shown| shown.id == drive.id && !shown.uploads.is_empty())
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if let Err(refused) = manager.unmount(&drive.id, false).await {
            log::info!("{}", refused.message);
            manager.unmount(&drive.id, true).await?;
        }
    }
    let kept: Vec<String> = manager
        .unsent()
        .into_iter()
        .filter(|file| PathBuf::from(&file.path).starts_with(manager.spool_folder(&drive.id)))
        .map(|file| file.path)
        .collect();

    let mut text = format!("Unmounted {}", drive.mount_point);
    if ended_elsewhere {
        text = format!("{} was unmounted outside poros-cli", drive.mount_point);
    }
    if !kept.is_empty() {
        text.push_str(&format!(
            "\nNot uploaded, and kept on this computer:\n  {}",
            kept.join("\n  ")
        ));
    }
    let reply = Reply::new(
        json!({
            "mountPoint": drive.mount_point,
            "path": drive.remote_root,
            "readOnly": drive.read_only,
            "bytesRead": last.bytes_read,
            "bytesWritten": last.bytes_written,
            "kept": kept,
        }),
        text,
    );
    Ok(if kept.is_empty() {
        reply
    } else {
        reply.with_exit(Exit::Partial)
    })
}
