//! Changing things where they are on the server: folders, deletes, moves, copies, permissions
//! and owners. SFTP gets the app's own operations, which run `rm`, `cp` and `mv` on the server
//! where it can and work in parallel otherwise.

use serde_json::json;

use super::{each, expand, make_dirs, missing, operand, refuse_dots, same_name};
use crate::cli::args::{ChmodArgs, ChownArgs, Conflict, MakeDirArgs, PlaceArgs, RemoveArgs};
use crate::cli::context::{Connected, Context};
use crate::cli::exit::Exit;
use crate::cli::output::{plural, Reply};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::file_ops::{
    self, names, DeleteRequest, FileOperations, Location, Mode, ModeChange, MoveCopyRequest,
    PermissionRequest,
};
use crate::protocol::{self, Protocol, RemoteFileSystem};
use crate::remote_path;

const MAX_FREE_NAME_ATTEMPTS: usize = 10_000;

fn operations(context: &Context) -> FileOperations {
    FileOperations::new(context.sessions.clone(), context.events.clone())
}

fn operation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub async fn make_dirs_command(context: &Context, arguments: &MakeDirArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    for path in &arguments.paths {
        operand(connected, path)?;
    }
    each(&arguments.paths, |path| {
        let path = connected.resolve(path);
        let files = files.clone();
        let parents = arguments.parents;
        async move {
            let created = if parents {
                make_dirs(&*files, &path).await?
            } else {
                let parent = remote_path::parent(&path)
                    .ok_or_else(|| AppError::invalid("The top folder always exists"))?;
                files
                    .make_dir(&parent, remote_path::file_name(&path))
                    .await?;
                true
            };
            let text = if created {
                format!("Created {path}")
            } else {
                format!("{path} already exists")
            };
            Ok((json!({ "path": path, "created": created }), text))
        }
    })
    .await
}

pub async fn remove(context: &Context, arguments: &RemoveArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    refuse_dots(&arguments.paths)?;
    // One operand at a time, so -f passes over only a pattern that matches nothing.
    let mut paths = Vec::new();
    for path in &arguments.paths {
        match expand(connected, std::slice::from_ref(path)).await {
            Err(error) if arguments.force && error.kind == ErrorKind::NotFound => {}
            result => paths.extend(result?),
        }
    }
    let mut doomed = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        if protocol::normalize(&path) == "/" {
            return Err(AppError::invalid(
                "The top folder of the server cannot be deleted",
            ));
        }
        match files.stat(&path).await? {
            None if arguments.force => {}
            None => refused.push(missing(&path)),
            Some(stat) if stat.is_dir && !arguments.recursive => refused.push(
                AppError::invalid(format!("{path} is a folder; add -r to delete it"))
                    .with_path(path),
            ),
            Some(_) => doomed.push(path),
        }
    }
    if let ([], [only]) = (doomed.as_slice(), refused.as_slice()) {
        return Err(only.clone());
    }
    let (removed_entries, mut failures) = if doomed.is_empty() {
        (0, Vec::new())
    } else if connected.info.protocol == Protocol::Sftp {
        let summary = operations(context)
            .delete(DeleteRequest {
                operation_id: operation_id(),
                session_id: connected.info.id.clone(),
                paths: doomed.clone(),
            })
            .await?;
        (summary.deleted, summary.failures)
    } else {
        files.delete(&doomed).await?;
        (doomed.len() as u64, Vec::new())
    };
    let all_failed = refused
        .first()
        .map_or(Exit::Failure, |error| Exit::for_error(error.kind));
    failures.extend(refused.into_iter().map(|error| file_ops::Failure {
        path: error.path.clone().unwrap_or_default(),
        message: error.message,
        kind: error.kind,
    }));
    let failed_paths: Vec<&str> = failures
        .iter()
        .map(|failure| failure.path.as_str())
        .collect();
    let deleted: Vec<&String> = doomed
        .iter()
        .filter(|path| !failed_paths.contains(&path.as_str()))
        .collect();
    let items = plural(deleted.len() as u64, "item");
    let mut text = match removed_entries {
        0 if failures.is_empty() => "Nothing to delete".to_string(),
        entries if entries <= deleted.len() as u64 => format!("Deleted {items}"),
        entries => format!("Deleted {items} ({entries} entries in all)"),
    };
    for failure in &failures {
        text.push_str(&format!(
            "\n  not deleted: {}: {}",
            failure.path, failure.message
        ));
    }
    let total = doomed.len() + failures.len();
    let failed = failures.len();
    Ok(Reply::new(
        json!({ "deleted": deleted, "entries": removed_entries, "failures": failures }),
        text,
    )
    .with_failures(failed, total, all_failed))
}

/// `mv` and `cp`: into a folder that exists, or one item to a new name.
pub async fn place(context: &Context, arguments: &PlaceArgs, moving: bool) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    let (destination, sources) = arguments
        .paths
        .split_last()
        .ok_or_else(|| AppError::invalid("Name what to move and where"))?;
    if moving {
        refuse_dots(sources)?;
    }
    let sources = expand(connected, sources).await?;
    let mut source_stats = Vec::with_capacity(sources.len());
    for source in &sources {
        source_stats.push(files.stat(source).await?.ok_or_else(|| missing(source))?);
    }
    let destination = operand(connected, destination)?;
    let found = files.stat(&destination).await?;
    let verb = if moving { "Moved" } else { "Copied" };

    if let (Some(stat), true) = (found, sources.len() > 1) {
        if !stat.is_dir {
            return Err(protocol::not_a_folder(&destination));
        }
    }
    let (folder, new_name) = match found {
        Some(stat) if stat.is_dir => (destination.clone(), None),
        _ if sources.len() > 1 => return Err(missing(&destination)),
        Some(_) => match arguments.conflict {
            Conflict::Fail => return Err(protocol::already_exists(&destination)),
            Conflict::Skip => {
                return Ok(Reply::new(
                    json!({ "placed": [], "skipped": 1 }),
                    format!("Skipped: {destination} exists"),
                ))
            }
            Conflict::Replace if sources[0] == destination => {
                return Err(AppError::invalid(format!(
                    "{destination} would be replaced with itself"
                ))
                .with_path(destination))
            }
            Conflict::Replace if source_stats[0].is_dir => {
                return Err(AppError::new(
                    ErrorKind::AlreadyExists,
                    format!("{destination} is a file, and a folder cannot replace it"),
                )
                .with_path(destination))
            }
            Conflict::Replace | Conflict::KeepBoth => split(&destination)?,
        },
        None => split(&destination)?,
    };

    if connected.info.protocol != Protocol::Sftp {
        if !moving {
            return Err(AppError::unsupported(format!(
                "{} cannot copy on the server; copying needs SFTP",
                connected.info.protocol.display_name()
            )));
        }
        let ([source], [stat], Some(name)) = (&sources[..], &source_stats[..], &new_name) else {
            return Err(unsupported_elsewhere(connected));
        };
        if remote_path::parent(source).as_deref() != Some(folder.as_str()) {
            return Err(unsupported_elsewhere(connected));
        }
        let clash = found.map(|_| arguments.conflict);
        return rename_in_place(&*files, source, stat.is_dir, &folder, name, clash).await;
    }
    if matches!(arguments.conflict, Conflict::Fail | Conflict::Replace) {
        let named = sources
            .iter()
            .map(|source| (remote_path::file_name(source), source.as_str()));
        if let Some(mut error) = same_name(named, &folder) {
            error
                .message
                .push_str("; keep both with --conflict keep-both");
            return Err(error);
        }
    }
    let operations = operations(context);
    let conflict = match arguments.conflict {
        Conflict::Fail => {
            if new_name.is_none() {
                let names: Vec<String> = sources
                    .iter()
                    .map(|source| remote_path::file_name(source).to_string())
                    .collect();
                let taken = operations
                    .conflicts(
                        Location::Remote {
                            session_id: connected.info.id.clone(),
                        },
                        names,
                        folder.clone(),
                    )
                    .await?;
                if !taken.is_empty() {
                    return Err(AppError::new(
                        ErrorKind::AlreadyExists,
                        format!(
                            "{folder} already has {}; choose what happens with --conflict",
                            taken.join(", ")
                        ),
                    )
                    .with_path(folder));
                }
            }
            // Nothing is in the way. Should something take one of the names meanwhile, both
            // are kept rather than one replaced.
            file_ops::Conflict::KeepBoth
        }
        Conflict::Replace => file_ops::Conflict::Replace,
        Conflict::Skip => file_ops::Conflict::Skip,
        Conflict::KeepBoth => file_ops::Conflict::KeepBoth,
    };
    let summary = operations
        .move_or_copy(MoveCopyRequest {
            operation_id: operation_id(),
            location: Location::Remote {
                session_id: connected.info.id.clone(),
            },
            mode: if moving { Mode::Move } else { Mode::Copy },
            sources: sources.clone(),
            target_directory: folder,
            conflict,
            target_name: new_name,
        })
        .await?;
    // One item that could not be placed is the command's error, with its own status, as with
    // the other commands given one path.
    if let ([_], [], 0, [failure]) = (
        &sources[..],
        &summary.placed[..],
        summary.skipped,
        &summary.failures[..],
    ) {
        return Err(
            AppError::new(failure.kind, failure.message.clone()).with_path(failure.path.clone())
        );
    }
    let mut text = summary
        .placed
        .iter()
        .map(|placed| format!("{verb} to {placed}"))
        .collect::<Vec<_>>()
        .join("\n");
    if summary.skipped > 0 {
        text.push_str(&format!("\nSkipped {}", plural(summary.skipped, "item")));
    }
    for failure in &summary.failures {
        text.push_str(&format!(
            "\n  failed: {}: {}",
            failure.path, failure.message
        ));
    }
    let failed = summary.failures.len();
    Ok(Reply::new(
        json!({
            "placed": summary.placed,
            "skipped": summary.skipped,
            "failures": summary.failures,
            "methods": summary.methods,
        }),
        text,
    )
    .with_failures(failed, sources.len(), Exit::Failure))
}

fn split(path: &str) -> AppResult<(String, Option<String>)> {
    let folder = remote_path::parent(path)
        .ok_or_else(|| AppError::invalid(format!("{path} has no folder above it")))?;
    Ok((folder, Some(remote_path::file_name(path).to_string())))
}

/// A move that stays in `folder` is a rename, the only move protocols other than SFTP offer.
/// `clash` is what to do about the file that has the new name, when one does.
async fn rename_in_place(
    files: &dyn RemoteFileSystem,
    source: &str,
    is_dir: bool,
    folder: &str,
    name: &str,
    clash: Option<Conflict>,
) -> AppResult<Reply> {
    let placed = match clash {
        Some(Conflict::Replace) => {
            replace_by_renaming(files, source, &remote_path::join(folder, name)).await?
        }
        Some(Conflict::KeepBoth) => {
            let free = free_name(files, folder, name, is_dir).await?;
            files.rename(source, &free).await?
        }
        _ => files.rename(source, name).await?,
    };
    Ok(Reply::new(
        json!({ "placed": [placed], "skipped": 0, "failures": [], "methods": ["rename"] }),
        format!("Moved to {placed}"),
    ))
}

/// Puts `source` in the place of the file `target` with renames alone, since these protocols
/// never rename over a file. The old one is moved aside first, and deleted only once the new
/// one is in place, or put back when the rename fails.
async fn replace_by_renaming(
    files: &dyn RemoteFileSystem,
    source: &str,
    target: &str,
) -> AppResult<String> {
    let name = remote_path::file_name(target);
    let aside = files.rename(target, &names::temporary(name)).await?;
    let placed = match files.rename(source, name).await {
        Ok(placed) => placed,
        Err(error) => {
            if let Err(problem) = files.rename(&aside, name).await {
                log::warn!("{target} is left as {aside}: {}", problem.message);
            }
            return Err(error);
        }
    };
    if let Err(problem) = files.delete(std::slice::from_ref(&aside)).await {
        log::warn!("The old {target} is left as {aside}: {}", problem.message);
    }
    Ok(placed)
}

/// A numbered name for `name` that nothing in `folder` has.
async fn free_name(
    files: &dyn RemoteFileSystem,
    folder: &str,
    name: &str,
    is_dir: bool,
) -> AppResult<String> {
    for candidate in names::numbered(name, is_dir).take(MAX_FREE_NAME_ATTEMPTS) {
        if files
            .stat(&remote_path::join(folder, &candidate))
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(AppError::new(
        ErrorKind::AlreadyExists,
        format!("Every name like {name} is taken in {folder}"),
    ))
}

fn unsupported_elsewhere(connected: &Connected) -> AppError {
    AppError::unsupported(format!(
        "{} can only rename within a folder; moving to another folder needs SFTP",
        connected.info.protocol.display_name()
    ))
}

/// Octal (`644`) or symbolic (`u+x,go-w`) permissions, as changes for files and for folders;
/// `X` adds execute to folders only.
pub fn parse_mode_change(text: &str) -> AppResult<(ModeChange, ModeChange)> {
    let text = text.trim();
    if !text.is_empty() && text.chars().all(|character| character.is_ascii_digit()) {
        let mode = super::parse_mode(text)?;
        let change = ModeChange {
            set: mode,
            clear: !mode & 0o7777,
        };
        return Ok((change, change));
    }
    let invalid = || {
        AppError::invalid(format!(
            "{text} is not a mode: use octal such as 644, or symbols such as u+x,go-w"
        ))
    };
    let mut files = ModeChange::default();
    let mut folders = ModeChange::default();
    for clause in text.split(',') {
        let operator_at = clause.find(['+', '-', '=']).ok_or_else(invalid)?;
        let (who, rest) = clause.split_at(operator_at);
        let mut classes = 0u32;
        for character in who.chars() {
            classes |= match character {
                'u' => 0o4700,
                'g' => 0o2070,
                'o' => 0o1007,
                'a' => 0o7777,
                _ => return Err(invalid()),
            };
        }
        if classes == 0 {
            classes = 0o7777;
        }
        let operator = rest.chars().next().ok_or_else(invalid)?;
        let (mut file_bits, mut folder_bits) = (0u32, 0u32);
        for character in rest[1..].chars() {
            let bits = match character {
                'r' => 0o444,
                'w' => 0o222,
                'x' => 0o111,
                's' => 0o6000,
                't' => 0o1000,
                'X' => {
                    folder_bits |= 0o111 & classes;
                    continue;
                }
                _ => return Err(invalid()),
            };
            file_bits |= bits & classes;
            folder_bits |= bits & classes;
        }
        for (change, bits) in [(&mut files, file_bits), (&mut folders, folder_bits)] {
            match operator {
                '+' => {
                    change.set |= bits;
                    change.clear &= !bits;
                }
                '-' => {
                    change.clear |= bits;
                    change.set &= !bits;
                }
                _ => {
                    change.clear |= classes & !bits;
                    change.clear &= !bits;
                    change.set = (change.set & !classes) | bits;
                }
            }
        }
    }
    Ok((files, folders))
}

pub async fn change_mode(context: &Context, arguments: &ChmodArgs) -> AppResult<Reply> {
    let (files, folders) = parse_mode_change(&arguments.mode)?;
    permissions(
        context,
        &arguments.paths,
        arguments.recursive,
        files,
        folders,
        None,
        None,
    )
    .await
}

pub async fn change_owner(context: &Context, arguments: &ChownArgs) -> AppResult<Reply> {
    let (owner, group) = match arguments.owner.split_once(':') {
        Some((owner, group)) => (owner.trim(), group.trim()),
        None => (arguments.owner.trim(), ""),
    };
    if owner.is_empty() && group.is_empty() {
        return Err(AppError::invalid("Name an owner, OWNER:GROUP or :GROUP"));
    }
    let named = |name: &str| (!name.is_empty()).then(|| name.to_string());
    permissions(
        context,
        &arguments.paths,
        arguments.recursive,
        ModeChange::default(),
        ModeChange::default(),
        named(owner),
        named(group),
    )
    .await
}

async fn permissions(
    context: &Context,
    paths: &[String],
    recursive: bool,
    files: ModeChange,
    folders: ModeChange,
    owner: Option<String>,
    group: Option<String>,
) -> AppResult<Reply> {
    let ownership = owner.is_some() || group.is_some();
    let connected = context.connected().await?;
    let paths = expand(connected, paths).await?;
    let summary = operations(context)
        .set_permissions(PermissionRequest {
            operation_id: operation_id(),
            session_id: connected.info.id.clone(),
            paths: paths.clone(),
            files,
            folders,
            recursive,
            owner,
            group,
        })
        .await?;
    // Owner changes run as one server command per batch, so only the paths named are counted.
    let owners_changed = paths.len().saturating_sub(summary.failures.len()) as u64;
    let mut text = if ownership {
        format!("Changed the owner of {}", plural(owners_changed, "item"))
    } else {
        format!("Changed {}", plural(summary.changed, "item"))
    };
    if summary.skipped_links > 0 {
        text.push_str(&format!(
            ", left {} alone",
            plural(summary.skipped_links, "link")
        ));
    }
    for failure in &summary.failures {
        text.push_str(&format!(
            "\n  failed: {}: {}",
            failure.path, failure.message
        ));
    }
    let failed = summary.failures.len();
    let total = (summary.changed as usize + failed).max(paths.len());
    let mut value = serde_json::to_value(&summary).unwrap_or_default();
    if let (true, Some(fields)) = (ownership, value.as_object_mut()) {
        fields.insert("ownerChanged".into(), owners_changed.into());
    }
    Ok(Reply::new(value, text).with_failures(failed, total, Exit::Failure))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, mode: u32) -> (u32, u32) {
        let (files, folders) = parse_mode_change(text).unwrap();
        (files.apply(mode), folders.apply(mode))
    }

    #[test]
    fn modes_change_as_chmod_changes_them() {
        assert_eq!(apply("644", 0o777), (0o644, 0o644));
        assert_eq!(apply("u+x", 0o644), (0o744, 0o744));
        assert_eq!(apply("go-w", 0o666), (0o644, 0o644));
        assert_eq!(apply("+x", 0o644), (0o755, 0o755));
        assert_eq!(apply("a=r", 0o755), (0o444, 0o444));
        assert_eq!(apply("u=rwx,g=rx,o=", 0o000), (0o750, 0o750));
        assert_eq!(apply("o=r", 0o777), (0o774, 0o774));
        assert_eq!(apply("a+X", 0o644), (0o644, 0o755));
        assert_eq!(apply("u+s", 0o755), (0o4755, 0o4755));
        assert_eq!(apply("+t", 0o777), (0o1777, 0o1777));
        assert!(parse_mode_change("u*x").is_err());
        assert!(parse_mode_change("z+x").is_err());
        assert!(parse_mode_change("u+q").is_err());
    }
}
