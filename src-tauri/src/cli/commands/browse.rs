//! Looking around: the session, saved connections, listings, attributes, and file contents.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;

use base64::Engine;
use bytes::Bytes;
use serde::Serialize;
use serde_json::{json, Value};

use super::{each, make_dirs, missing, one_or_many, parse_mode};
use crate::cli::args::{CatArgs, ListArgs, PathsArgs, WriteArgs};
use crate::cli::context::Context;
use crate::cli::output::{mode_string, octal, plural, time_iso, time_string, Reply};
use crate::connections::{AuthType, SavedConnection};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::file_ops::FileOperations;
use crate::format::format_size;
use crate::model::{DirListing, EntryKind, FileEntry};
use crate::protocol::{Protocol, RemoteFileSystem, RemoteStat, WriteRequest};
use crate::remote_path;

/// `cat --json` holds files in memory, so larger ones need `--length`.
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
/// `write` holds what it writes in memory, since some protocols announce the size first.
const MAX_WRITE_BYTES: usize = 512 * 1024 * 1024;
const WRITE_CHUNK: usize = 256 * 1024;

pub async fn info(context: &Context) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let info = &connected.info;
    let files = connected.files();
    let capabilities = files.capabilities();
    let runs_commands = info.protocol == Protocol::Sftp;
    let value = json!({
        "label": info.label,
        "protocol": info.protocol,
        "host": info.host,
        "port": info.port,
        "username": info.username,
        "home": info.home,
        "initialPath": info.initial_path,
        "savedConnectionId": info.saved_connection_id,
        "hostKeyFingerprint": connected.session.host_key_fingerprint,
        "capabilities": {
            "rangedReads": capabilities.ranged_reads,
            "resumableWrites": capabilities.resumable_writes,
            "commands": runs_commands,
            "sync": runs_commands,
        },
    });
    let mut text = format!(
        "Connected to {} over {}\nHome folder: {}\nStarts in: {}",
        info.label,
        info.protocol.display_name(),
        info.home,
        info.initial_path
    );
    if !connected.session.host_key_fingerprint.is_empty() {
        text.push_str(&format!(
            "\nHost key: {}",
            connected.session.host_key_fingerprint
        ));
    }
    Ok(Reply::new(value, text))
}

pub async fn connections(context: &Context) -> AppResult<Reply> {
    let store = context.store.clone();
    let mut saved = tokio::task::spawn_blocking(move || store.list()).await??;
    saved.sort_by_key(|connection| connection.name.to_lowercase());
    let rows: Vec<[String; 4]> = saved
        .iter()
        .map(|connection| {
            [
                connection.name.clone(),
                connection.protocol.display_name().to_string(),
                address(connection),
                auth_label(connection).to_string(),
            ]
        })
        .collect();
    let mut text = table(
        &[["NAME", "PROTOCOL", "ADDRESS", "SIGN-IN"].map(String::from)],
        &rows,
    );
    if saved.is_empty() {
        text = "No saved connections".into();
    }
    Ok(Reply::new(&saved, text))
}

fn address(connection: &SavedConnection) -> String {
    let user = match connection.username.trim() {
        "" => String::new(),
        name => format!("{name}@"),
    };
    if connection.port == connection.protocol.default_port() {
        format!("{user}{}", connection.host)
    } else {
        format!("{user}{}:{}", connection.host, connection.port)
    }
}

fn auth_label(connection: &SavedConnection) -> &'static str {
    match connection.auth_type {
        AuthType::Password if connection.save_secret => "password (saved)",
        AuthType::Password => "password",
        AuthType::PublicKey => "key",
        AuthType::Agent => "agent",
        AuthType::OAuth => "browser",
    }
}

fn table(header: &[[String; 4]], rows: &[[String; 4]]) -> String {
    let mut widths = [0usize; 4];
    for row in header.iter().chain(rows) {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    header
        .iter()
        .chain(rows)
        .map(|row| {
            row.iter()
                .zip(widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn list(context: &Context, arguments: &ListArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    let paths = if arguments.paths.is_empty() {
        vec![String::new()]
    } else {
        arguments.paths.clone()
    };
    let mut listings = Vec::with_capacity(paths.len());
    for path in &paths {
        let root = connected.resolve(path);
        let mut listing = if arguments.recursive {
            walk(&*files, &root, arguments.max_depth, arguments.all).await?
        } else {
            files.list_dir(&root).await?
        };
        if !arguments.all {
            listing.entries.retain(|entry| !entry.hidden);
        }
        if !arguments.recursive {
            listing
                .entries
                .sort_by(|left, right| left.name.cmp(&right.name));
        }
        listings.push(listing);
    }
    let several = listings.len() > 1;
    let text = listings
        .iter()
        .map(|listing| {
            let body = render_entries(&listing.entries, arguments.long, arguments.recursive);
            if several {
                format!("{}:\n{body}", listing.path)
            } else {
                body
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(Reply::new(one_or_many(listings), text))
}

/// Every entry under `root`, folders before what they hold, sorted by path.
async fn walk(
    files: &dyn RemoteFileSystem,
    root: &str,
    max_depth: Option<usize>,
    include_hidden: bool,
) -> AppResult<DirListing> {
    let top = files.list_dir(root).await?;
    let mut entries = Vec::new();
    let mut pending: VecDeque<(Vec<FileEntry>, usize)> = VecDeque::from([(top.entries, 1)]);
    while let Some((found, depth)) = pending.pop_front() {
        for entry in found {
            if !include_hidden && entry.hidden {
                continue;
            }
            // Links to folders are listed but not followed, so a loop cannot trap the walk.
            if entry.kind == EntryKind::Dir && max_depth.is_none_or(|limit| depth < limit) {
                match files.list_dir(&entry.path).await {
                    Ok(inside) => pending.push_back((inside.entries, depth + 1)),
                    Err(error) => log::warn!("{}: {}", entry.path, error.message),
                }
            }
            entries.push(entry);
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(DirListing {
        path: top.path,
        parent: top.parent,
        entries,
    })
}

fn render_entries(entries: &[FileEntry], long: bool, show_paths: bool) -> String {
    let shown_name = |entry: &FileEntry| {
        let name = if show_paths { &entry.path } else { &entry.name };
        if entry.is_dir_like() {
            format!("{name}/")
        } else {
            name.clone()
        }
    };
    if !long {
        return entries
            .iter()
            .map(shown_name)
            .collect::<Vec<_>>()
            .join("\n");
    }
    let rows: Vec<[String; 5]> = entries
        .iter()
        .map(|entry| {
            let kind = match entry.kind {
                EntryKind::Dir => 'd',
                EntryKind::Symlink => 'l',
                EntryKind::File => '-',
                EntryKind::Other => '?',
            };
            [
                mode_string(kind, entry.permissions),
                entry.owner.clone().unwrap_or_else(|| "-".into()),
                entry.group.clone().unwrap_or_else(|| "-".into()),
                entry.size.to_string(),
                time_string(entry.modified),
            ]
        })
        .collect();
    let width = |column: usize| {
        rows.iter()
            .map(|row| row[column].chars().count())
            .max()
            .unwrap_or(0)
    };
    let (owner_width, group_width, size_width) = (width(1), width(2), width(3));
    rows.iter()
        .zip(entries)
        .map(|(row, entry)| {
            format!(
                "{}  {:<owner_width$}  {:<group_width$}  {:>size_width$}  {}  {}",
                row[0],
                row[1],
                row[2],
                row[3],
                row[4],
                shown_name(entry)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatView {
    path: String,
    kind: EntryKind,
    size: u64,
    modified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_iso: Option<String>,
    permissions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
}

fn stat_view(path: String, stat: RemoteStat) -> StatView {
    StatView {
        path,
        kind: if stat.is_dir {
            EntryKind::Dir
        } else {
            EntryKind::File
        },
        size: stat.size,
        modified: stat.modified,
        modified_iso: time_iso(stat.modified),
        permissions: stat.permissions,
        mode: octal(stat.permissions),
    }
}

pub async fn stat(context: &Context, arguments: &PathsArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    each(&arguments.paths, |path| {
        let files = files.clone();
        let path = connected.resolve(path);
        async move {
            let stat = files.stat(&path).await?.ok_or_else(|| missing(&path))?;
            let view = stat_view(path, stat);
            let text = format!(
                "{}  {}  {}  {}  {}",
                view.mode.as_deref().unwrap_or("----"),
                if stat.is_dir { "folder" } else { "file  " },
                format_size(view.size),
                time_string(view.modified),
                view.path
            );
            Ok((serde_json::to_value(&view).unwrap_or(Value::Null), text))
        }
    })
    .await
}

pub async fn usage(context: &Context, arguments: &PathsArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let operations = FileOperations::new(context.sessions.clone(), context.events.clone());
    each(&arguments.paths, |path| {
        let path = connected.resolve(path);
        let operations = &operations;
        async move {
            let usage = if connected.info.protocol == Protocol::Sftp {
                operations
                    .measure(
                        &uuid::Uuid::new_v4().to_string(),
                        &connected.info.id,
                        std::slice::from_ref(&path),
                    )
                    .await?
            } else {
                count_generic(&*connected.files(), &path).await?
            };
            let text = format!(
                "{}  {}, {}  {path}",
                format_size(usage.bytes),
                plural(usage.files, "file"),
                plural(usage.folders, "folder")
            );
            let value = json!({
                "path": path,
                "files": usage.files,
                "folders": usage.folders,
                "bytes": usage.bytes,
            });
            Ok((value, text))
        }
    })
    .await
}

async fn count_generic(
    files: &dyn RemoteFileSystem,
    path: &str,
) -> AppResult<crate::file_ops::Usage> {
    let mut usage = crate::file_ops::Usage::default();
    let stat = files.stat(path).await?.ok_or_else(|| missing(path))?;
    if !stat.is_dir {
        usage.files = 1;
        usage.bytes = stat.size;
        return Ok(usage);
    }
    let mut pending = VecDeque::from([path.to_string()]);
    while let Some(folder) = pending.pop_front() {
        for entry in files.list_dir(&folder).await?.entries {
            match entry.kind {
                EntryKind::Dir => {
                    usage.folders += 1;
                    pending.push_back(entry.path);
                }
                _ => {
                    usage.files += 1;
                    usage.bytes += entry.size;
                }
            }
        }
    }
    Ok(usage)
}

pub async fn cat(context: &Context, arguments: &CatArgs, json: bool) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    let mut contents = Vec::new();
    for path in &arguments.paths {
        let path = connected.resolve(path);
        let stat = files.stat(&path).await?.ok_or_else(|| missing(&path))?;
        if stat.is_dir {
            return Err(AppError::invalid(format!("{path} is a folder")).with_path(path));
        }
        let start = arguments.offset.min(stat.size);
        let end = match arguments.length {
            Some(length) => start.saturating_add(length).min(stat.size),
            None => stat.size,
        };
        if json && end - start > MAX_JSON_BYTES {
            return Err(AppError::invalid(format!(
                "{path} is {}; with --json, read at most {} at a time with --offset and --length",
                format_size(end - start),
                format_size(MAX_JSON_BYTES)
            )));
        }
        let mut kept = Vec::new();
        let finished = read_range(files.clone(), &path, start..end, |chunk| {
            if json {
                kept.extend_from_slice(chunk);
                true
            } else {
                // A reader that went away, as `| head` does, ends the output quietly.
                std::io::stdout().write_all(chunk).is_ok()
            }
        })
        .await?;
        if !finished {
            break;
        }
        if json {
            contents.push(content_value(&path, start, kept));
        }
    }
    let _ = std::io::stdout().flush();
    if json {
        return Ok(Reply::new(one_or_many(contents), ""));
    }
    Ok(Reply::new(Value::Null, ""))
}

/// Hands each piece of the range to `sink` until it returns false; returns whether it read
/// all of it.
pub async fn read_range(
    files: Arc<dyn RemoteFileSystem>,
    path: &str,
    range: std::ops::Range<u64>,
    mut sink: impl FnMut(&[u8]) -> bool,
) -> AppResult<bool> {
    if range.is_empty() {
        return Ok(true);
    }
    // Without ranged reads the file is read from the start and the part before is passed over.
    let ranged = files.capabilities().ranged_reads;
    let (requested, mut skip) = if ranged || range.start == 0 {
        (range.clone(), 0)
    } else {
        (0..range.end, range.start)
    };
    let mut stream = files.open_read(path, requested).await?;
    let mut complete = true;
    while let Some(chunk) = stream.next_chunk().await? {
        let mut piece: &[u8] = &chunk;
        if skip > 0 {
            let passed = skip.min(piece.len() as u64);
            piece = &piece[passed as usize..];
            skip -= passed;
        }
        if !piece.is_empty() && !sink(piece) {
            complete = false;
            break;
        }
    }
    stream.finish().await?;
    Ok(complete)
}

fn content_value(path: &str, offset: u64, bytes: Vec<u8>) -> Value {
    let size = bytes.len();
    match String::from_utf8(bytes) {
        Ok(text) => json!({
            "path": path,
            "offset": offset,
            "bytes": size,
            "encoding": "utf8",
            "content": text,
        }),
        Err(error) => json!({
            "path": path,
            "offset": offset,
            "bytes": size,
            "encoding": "base64",
            "content": base64::engine::general_purpose::STANDARD.encode(error.into_bytes()),
        }),
    }
}

/// `input` is what a batch line carries; otherwise standard input is read to its end.
pub async fn write(
    context: &Context,
    arguments: &WriteArgs,
    input: Option<Vec<u8>>,
) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    let path = connected.resolve(&arguments.path);
    let permissions = arguments.mode.as_deref().map(parse_mode).transpose()?;
    let data = match input {
        Some(data) => data,
        None => read_stdin().await?,
    };
    let existing = files.stat(&path).await?;
    match existing {
        Some(stat) if stat.is_dir => {
            return Err(AppError::invalid(format!("{path} is a folder")).with_path(path))
        }
        Some(_) if arguments.no_clobber => {
            return Err(
                AppError::new(ErrorKind::AlreadyExists, format!("{path} already exists"))
                    .with_path(path),
            )
        }
        None if arguments.parents => {
            if let Some(parent) = remote_path::parent(&path) {
                make_dirs(&*files, &parent).await?;
            }
        }
        _ => {}
    }
    let request = WriteRequest {
        path: path.clone(),
        offset: 0,
        size: data.len() as u64,
        modified: None,
        permissions,
    };
    let mut stream = files.clone().open_write(request).await?;
    for chunk in data.chunks(WRITE_CHUNK) {
        stream.write(Bytes::copy_from_slice(chunk)).await?;
    }
    stream.finish().await?;
    if permissions.is_some() {
        if let Err(error) = files.set_attributes(&path, None, permissions).await {
            log::warn!("Could not set the permissions of {path}: {}", error.message);
        }
    }
    let text = format!("Wrote {} to {path}", format_size(data.len() as u64));
    Ok(Reply::new(
        json!({ "path": path, "bytes": data.len(), "replaced": existing.is_some() }),
        text,
    ))
}

async fn read_stdin() -> AppResult<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut data = Vec::new();
    let read = tokio::io::stdin()
        .take(MAX_WRITE_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .await?;
    if read > MAX_WRITE_BYTES {
        return Err(AppError::invalid(format!(
            "write takes at most {}; upload larger files with put",
            format_size(MAX_WRITE_BYTES as u64)
        )));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: EntryKind, size: u64) -> FileEntry {
        FileEntry {
            name: name.into(),
            path: format!("/srv/{name}"),
            kind,
            link_target: None,
            size,
            modified: Some(1_700_000_000),
            permissions: Some(if kind == EntryKind::Dir { 0o755 } else { 0o644 }),
            owner: Some("www".into()),
            group: Some("www-data".into()),
            hidden: false,
        }
    }

    #[test]
    fn listings_read_like_ls() {
        let entries = [
            entry("site", EntryKind::Dir, 4096),
            entry("index.html", EntryKind::File, 12),
        ];
        assert_eq!(render_entries(&entries, false, false), "site/\nindex.html");
        assert_eq!(
            render_entries(&entries, false, true),
            "/srv/site/\n/srv/index.html"
        );
        let long = render_entries(&entries, true, false);
        let lines: Vec<&str> = long.lines().collect();
        assert!(lines[0].starts_with("drwxr-xr-x  www  www-data  4096  "));
        assert!(lines[1].starts_with("-rw-r--r--  www  www-data    12  "));
        assert!(lines[1].ends_with("  index.html"));
    }

    #[test]
    fn contents_are_text_when_they_can_be() {
        let text = content_value("/a", 0, b"hello".to_vec());
        assert_eq!(text["encoding"], "utf8");
        assert_eq!(text["content"], "hello");
        let binary = content_value("/b", 4, vec![0xff, 0x00]);
        assert_eq!(binary["encoding"], "base64");
        assert_eq!(binary["content"], "/wA=");
        assert_eq!(binary["offset"], 4);
    }

    #[test]
    fn tables_line_up() {
        let rows = [
            [
                "prod".to_string(),
                "SFTP".into(),
                "me@host".into(),
                "key".into(),
            ],
            [
                "backups".to_string(),
                "S3".into(),
                "s3.amazonaws.com".into(),
                "password".into(),
            ],
        ];
        let text = table(
            &[["NAME", "PROTOCOL", "ADDRESS", "SIGN-IN"].map(String::from)],
            &rows,
        );
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "NAME     PROTOCOL  ADDRESS           SIGN-IN");
        assert_eq!(lines[1], "prod     SFTP      me@host           key");
    }
}
