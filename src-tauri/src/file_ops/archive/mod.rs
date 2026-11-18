//! Extracting archives where they are: on a server with the programs it has, found with one
//! command over SSH, or on this computer with the installed program Settings picks. The first
//! bytes of the file decide its format, and nothing already there is overwritten.
//!
//! Both ways to extract run the program the same way, into a new, empty folder beside the
//! archive. Extract to a folder keeps that folder under the archive's name. Extract here
//! unpacks into a hidden working folder, then moves what it holds out beside the archive, each
//! item under a free name such as `site (2)`, and removes the working folder.

mod formats;
mod installed;
mod tools;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

pub use formats::ArchiveFormat;
pub use installed::{installed_tools, InstalledTool};
pub use tools::{CustomTool, Tool, ToolKind};

use super::{names, Progress};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::EntryKind;
use crate::remote_path;
use crate::rsync::{run_command, shell_quote};
use crate::session::Session;
use crate::sftp::RemoteFs;
use formats::{unpacked_name, HEAD_BYTES};
use tools::{choose, custom_plan, plan, shell_script, Placement, Plan};

const COMMAND_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_FREE_NAME_ATTEMPTS: usize = 10_000;
/// How long a move out of the working folder is retried while Windows reports it busy.
const SETTLE_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtractInto {
    /// The folder the archive is in.
    Here,
    /// A new folder beside the archive, named after it.
    Folder,
}

/// The program that extracts on this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Extractor {
    /// The best installed program for each format.
    #[default]
    Auto,
    /// An installed program, for every format it opens; others go to the best for them.
    Installed { path: String },
    /// A program the user named, for every archive.
    Custom(CustomTool),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractSummary {
    /// What to show: the new folder, the unpacked file, the one item extracted here, or the
    /// folder extracted into.
    pub destination: String,
    /// The folder the archive is in.
    pub folder: String,
    /// What now stands beside the archive: the new folder, or each item extracted here.
    pub placed: Vec<String>,
    pub format: ArchiveFormat,
    /// The program that extracted it.
    pub tool: String,
    /// What the program said while it succeeded, such as files it left alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

fn not_an_archive(name: &str) -> AppError {
    AppError::unsupported(format!("{name} is not an archive Poros can extract"))
}

/// What to install for a format, for messages about missing programs.
fn install_hint(format: ArchiveFormat) -> &'static str {
    use ArchiveFormat as Format;
    match format {
        Format::Zip => "unzip or 7-Zip",
        Format::SevenZip | Format::Iso | Format::Cab => "7-Zip",
        Format::Rar => "unrar or 7-Zip",
        Format::TarZstd | Format::Zstd => "zstd and tar",
        Format::Tar | Format::TarGzip | Format::TarBzip2 | Format::TarXz | Format::TarLzma => "tar",
        Format::Gzip => "gzip",
        Format::Bzip2 => "bzip2",
        Format::Xz | Format::Lzma => "xz",
    }
}

/// The last thing a program said on stderr, or how it ended.
fn failure(tool: &str, error_output: &str, status: Option<u32>) -> AppError {
    let said = error_output
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty());
    let message = match (said, status) {
        (Some(said), _) => format!("{tool}: {said}"),
        (None, Some(status)) => format!("{tool} exited with status {status}"),
        (None, None) => format!("{tool} did not finish"),
    };
    let kind = if error_output.contains("Permission denied") {
        ErrorKind::PermissionDenied
    } else {
        ErrorKind::Io
    };
    AppError::new(kind, message)
}

/// Lines worth keeping from a program that succeeded.
fn warning_from(error_output: &str) -> Option<String> {
    let kept: Vec<&str> = error_output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(20)
        .collect();
    (!kept.is_empty()).then(|| kept.join("\n"))
}

async fn cancellable<T>(
    cancel: &CancellationToken,
    future: impl std::future::Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(AppError::cancelled()),
        result = future => result,
    }
}

/// The tools a server has, found with one command. Servers that run no commands have none.
pub async fn server_tools(session: &Session) -> AppResult<Vec<Tool>> {
    let channel = session.open_command_channel().await?;
    let output = run_command(channel, &tools::server_probe(), COMMAND_OUTPUT_LIMIT).await?;
    tools::parse_server_probe(&String::from_utf8_lossy(&output.output)).ok_or_else(|| {
        AppError::unsupported(
            "This server runs no commands, so archives on it cannot be extracted there. \
             Download one to extract it on this computer.",
        )
    })
}

/// A name in `folder` nothing has yet: `name`, else `name (2)` and so on.
async fn free_remote_name(
    fs: &RemoteFs,
    folder: &str,
    name: &str,
    is_dir: bool,
) -> AppResult<String> {
    let candidates = std::iter::once(name.to_string()).chain(names::numbered(name, is_dir));
    for candidate in candidates.take(MAX_FREE_NAME_ATTEMPTS) {
        if fs
            .lstat_entry(&remote_path::join(folder, &candidate))
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

/// The new folder beside the archive that a program extracts into: under the archive's own
/// name for Extract to a folder, or a hidden working folder for Extract here.
fn target_name(into: ExtractInto, unpacked: &str) -> Option<String> {
    match into {
        ExtractInto::Folder => None,
        ExtractInto::Here => Some(names::temporary(unpacked)),
    }
}

/// Moves each item in `working` out into `parent` under a free name, and removes `working`.
/// Returns where the items now are.
async fn place_remote(fs: &RemoteFs, working: &str, parent: &str) -> AppResult<Vec<String>> {
    // An archive can make the folder it unpacked into read-only.
    let _ = fs.set_permissions(working, 0o700).await;
    let mut items = fs.entries(working).await?;
    items.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut placed = Vec::with_capacity(items.len());
    for (name, stat) in items {
        let is_dir = stat.kind == EntryKind::Dir;
        let free = free_remote_name(fs, parent, &name, is_dir).await?;
        let to = remote_path::join(parent, &free);
        fs.rename_path(&remote_path::join(working, &name), &to)
            .await?;
        placed.push(to);
    }
    // Left behind empty if the server refuses; it is hidden, and holds nothing.
    let _ = fs.remove_empty_dir(working).await;
    Ok(placed)
}

/// Extracts an archive on the server it is on, with the programs `tools` finds there once the
/// file is known to be an archive.
pub(crate) async fn extract_on_server<Tools>(
    session: &Session,
    tools: impl FnOnce() -> Tools,
    archive: &str,
    into: ExtractInto,
    cancel: &CancellationToken,
    progress: &Progress,
) -> AppResult<ExtractSummary>
where
    Tools: std::future::Future<Output = AppResult<Arc<Vec<Tool>>>>,
{
    let fs = session.sftp()?;
    let archive = fs.resolve(archive);
    let name = remote_path::file_name(&archive).to_string();
    let parent = remote_path::parent(&archive).ok_or_else(|| not_an_archive(&name))?;
    let head = fs.read_range(&archive, 0, HEAD_BYTES).await?;
    let format = ArchiveFormat::detect(&name, &head).ok_or_else(|| not_an_archive(&name))?;
    let tools = tools().await?;
    let tool = choose(&tools, format, None).ok_or_else(|| {
        AppError::unsupported(format!(
            "The server has no program that extracts {} archives. Install {} on it, or \
             download the archive to extract it on this computer.",
            format.display_name(),
            install_hint(format)
        ))
    })?;

    progress.working_on(&format!("With {} on the server", tool.kind.display_name()));
    let unpacked = unpacked_name(&name, format);
    let folder_name = match target_name(into, &unpacked) {
        Some(working) => working,
        None => free_remote_name(fs, &parent, &unpacked, true).await?,
    };
    let folder = remote_path::join(&parent, &folder_name);
    let single_file = format
        .is_single_file()
        .then(|| remote_path::join(&folder, &unpacked));
    let plan = plan(
        tool,
        format,
        &Placement {
            archive: &archive,
            folder: &folder,
            single_file: single_file.as_deref(),
            separator: '/',
        },
    )
    .ok_or_else(|| not_an_archive(&name))?;
    let command = format!("sh -c {}", shell_quote(&shell_script(&plan)));
    fs.make_dir(&parent, &folder_name).await?;

    let result = cancellable(cancel, async {
        let channel = session.open_command_channel().await?;
        run_command(channel, &command, COMMAND_OUTPUT_LIMIT).await
    })
    .await;
    let tool_name = tool.kind.display_name();
    let failed = match result {
        Ok(output) if tolerated(tool.kind, output.exit_status) => {
            let placed = match into {
                ExtractInto::Folder => Ok(vec![folder.clone()]),
                ExtractInto::Here => place_remote(fs, &folder, &parent).await,
            };
            match placed {
                Ok(placed) => {
                    let destination = match into {
                        ExtractInto::Folder => single_file.unwrap_or_else(|| folder.clone()),
                        ExtractInto::Here => shown_here(&placed, &parent),
                    };
                    return Ok(ExtractSummary {
                        destination,
                        folder: parent,
                        placed,
                        format,
                        tool: tool_name.to_string(),
                        warning: warning_from(&output.error_output),
                    });
                }
                Err(error) => error,
            }
        }
        Ok(output) => failure(tool_name, &output.error_output, output.exit_status),
        Err(error) => error,
    };
    // Nothing half done stays behind where Poros made the place for it.
    match into {
        ExtractInto::Here => {
            let _ = fs.delete(std::slice::from_ref(&folder)).await;
        }
        ExtractInto::Folder => {
            if let Some(file) = &single_file {
                let _ = fs.delete(std::slice::from_ref(file)).await;
            }
            if fs
                .list_dir(&folder)
                .await
                .is_ok_and(|listing| listing.entries.is_empty())
            {
                let _ = fs.delete(std::slice::from_ref(&folder)).await;
            }
        }
    }
    Err(failed)
}

/// What an extraction here shows: the one item it placed, or the folder for several.
fn shown_here(placed: &[String], parent: &str) -> String {
    match placed {
        [only] => only.clone(),
        _ => parent.to_string(),
    }
}

fn tolerated(kind: ToolKind, status: Option<u32>) -> bool {
    status
        .and_then(|status| i32::try_from(status).ok())
        .is_some_and(|code| installed::tolerated(Some(kind), code))
}

fn free_local_name(folder: &Path, name: &str, is_dir: bool) -> AppResult<PathBuf> {
    std::iter::once(name.to_string())
        .chain(names::numbered(name, is_dir))
        .take(MAX_FREE_NAME_ATTEMPTS)
        .map(|candidate| folder.join(candidate))
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .ok_or_else(|| {
            AppError::new(
                ErrorKind::AlreadyExists,
                format!("Every name like {name} is taken in {}", folder.display()),
            )
        })
}

/// Moves each item in `working` out into `parent` under a free name, and removes `working`.
/// Returns where the items now are.
fn place_local(working: &Path, parent: &Path) -> AppResult<Vec<PathBuf>> {
    fn at(path: &Path) -> impl FnOnce(std::io::Error) -> AppError + '_ {
        move |error| AppError::from(error).with_path(text(path))
    }
    open_up_local(working);
    let mut items: Vec<(std::ffi::OsString, bool)> = std::fs::read_dir(working)
        .map_err(at(working))?
        .map(|item| {
            let item = item?;
            Ok((item.file_name(), item.file_type()?.is_dir()))
        })
        .collect::<std::io::Result<_>>()
        .map_err(at(working))?;
    items.sort();
    let mut placed = Vec::with_capacity(items.len());
    for (name, is_dir) in items {
        let from = working.join(&name);
        let same = parent.join(&name);
        let to = if std::fs::symlink_metadata(&same).is_err() {
            same
        } else {
            free_local_name(parent, &name.to_string_lossy(), is_dir)?
        };
        settle(|| std::fs::rename(&from, &to)).map_err(at(&from))?;
        placed.push(to);
    }
    // Left behind empty if it stays busy; it is hidden, and holds nothing.
    let _ = settle(|| std::fs::remove_dir(working));
    Ok(placed)
}

/// Lets Poros move items out of a working folder whose archive made it read-only.
#[cfg(unix)]
fn open_up_local(working: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(working, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn open_up_local(_working: &Path) {}

/// Runs `step` again for a few seconds while Windows refuses it because something still has
/// a file open in it, as virus scanners and the search indexer do right after it was written.
fn settle(mut step: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut wait = std::time::Duration::from_millis(25);
    let mut waited = std::time::Duration::ZERO;
    loop {
        match step() {
            Err(error) if cfg!(windows) && busy(&error) && waited < SETTLE_LIMIT => {
                std::thread::sleep(wait);
                waited += wait;
                wait = (wait * 2).min(std::time::Duration::from_millis(500));
            }
            result => return result,
        }
    }
}

/// Access denied or a sharing violation: what Windows says while a file is open.
fn busy(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(32)
}

fn read_head(path: &Path) -> AppResult<Vec<u8>> {
    let mut head = Vec::with_capacity(HEAD_BYTES as usize);
    std::fs::File::open(path)
        .map_err(|error| AppError::from(error).with_path(path.to_string_lossy()))?
        .take(u64::from(HEAD_BYTES))
        .read_to_end(&mut head)?;
    Ok(head)
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Extracts an archive on this computer with `extractor`. Blocking; run it off the async
/// runtime.
pub(crate) fn extract_local(
    archive: &str,
    into: ExtractInto,
    extractor: &Extractor,
    cancel: &CancellationToken,
    progress: &Progress,
) -> AppResult<ExtractSummary> {
    let archive = PathBuf::from(archive);
    let name = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = archive
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| not_an_archive(&name))?;
    let format =
        ArchiveFormat::detect(&name, &read_head(&archive)?).ok_or_else(|| not_an_archive(&name))?;

    let installed = match extractor {
        Extractor::Custom(_) => Vec::new(),
        _ => installed_tools(),
    };
    let preferred = match extractor {
        Extractor::Installed { path } => Some(
            installed
                .iter()
                .find(|tool| tool.path == *path)
                .map(InstalledTool::tool)
                .ok_or_else(|| {
                    AppError::new(
                        ErrorKind::NotFound,
                        format!(
                            "{path}, picked in Settings > Archives, is no longer there. Pick \
                             another program there."
                        ),
                    )
                })?,
        ),
        _ => None,
    };
    let tools: Vec<Tool> = installed.iter().map(InstalledTool::tool).collect();
    let chosen = match extractor {
        Extractor::Custom(_) => None,
        _ => Some(choose(&tools, format, preferred.as_ref()).ok_or_else(|| {
            AppError::unsupported(format!(
                "No program on this computer extracts {} archives. Install {}, or name the \
                 program to use in Settings > Archives.",
                format.display_name(),
                install_hint(format)
            ))
        })?),
    };

    let unpacked = unpacked_name(&name, format);
    let folder = match target_name(into, &unpacked) {
        Some(working) => parent.join(working),
        None => free_local_name(&parent, &unpacked, true)?,
    };
    let single_file = (format.is_single_file() && chosen.is_some()).then(|| folder.join(&unpacked));
    let (plan, tool_name, kind): (Plan, String, Option<ToolKind>) = match (extractor, chosen) {
        (Extractor::Custom(custom), _) => (
            custom_plan(custom, &text(&archive), &text(&folder)),
            Path::new(custom.program.trim())
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| custom.program.clone()),
            None,
        ),
        (_, Some(tool)) => (
            plan(
                tool,
                format,
                &Placement {
                    archive: &text(&archive),
                    folder: &text(&folder),
                    single_file: single_file.as_deref().map(text).as_deref(),
                    separator: std::path::MAIN_SEPARATOR,
                },
            )
            .ok_or_else(|| not_an_archive(&name))?,
            tool.kind.display_name().to_string(),
            Some(tool.kind),
        ),
        (_, None) => return Err(not_an_archive(&name)),
    };
    std::fs::create_dir(&folder).map_err(|error| AppError::from(error).with_path(text(&folder)))?;

    progress.working_on(&format!("With {tool_name}"));
    let failed = match installed::run(&plan, kind, cancel) {
        Ok(finished) if finished.succeeded => {
            let placed = match into {
                ExtractInto::Folder => Ok(vec![folder.clone()]),
                ExtractInto::Here => place_local(&folder, &parent),
            };
            match placed {
                Ok(placed) => {
                    let placed: Vec<String> = placed.iter().map(|path| text(path)).collect();
                    let destination = match into {
                        ExtractInto::Folder => text(single_file.as_ref().unwrap_or(&folder)),
                        ExtractInto::Here => shown_here(&placed, &text(&parent)),
                    };
                    return Ok(ExtractSummary {
                        destination,
                        folder: text(&parent),
                        placed,
                        format,
                        tool: tool_name,
                        warning: warning_from(&finished.error_output),
                    });
                }
                Err(error) => error,
            }
        }
        Ok(finished) => failure(&tool_name, &finished.error_output, None),
        Err(error) => error,
    };
    match into {
        // The working folder is Poros's own, so nothing left in it is kept.
        ExtractInto::Here => {
            let _ = std::fs::remove_dir_all(&folder);
        }
        ExtractInto::Folder => {
            if let Some(file) = &single_file {
                let _ = std::fs::remove_file(file);
            }
            // Only removes the folder while it is empty.
            let _ = std::fs::remove_dir(&folder);
        }
    }
    Err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("poros-archive-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[cfg(unix)]
    fn has(program: &str) -> bool {
        installed_tools().iter().any(|tool| {
            Path::new(&tool.path)
                .file_stem()
                .is_some_and(|stem| stem == program)
        })
    }

    #[cfg(unix)]
    fn shell(folder: &Path, script: &str) {
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .current_dir(folder)
            .status()
            .unwrap();
        assert!(status.success(), "{script}");
    }

    #[cfg(unix)]
    #[test]
    fn extracts_a_tar_gz_into_a_folder_named_after_it_beside_one_taken() {
        if !has("tar") || !has("gzip") {
            return;
        }
        let folder = scratch();
        shell(
            &folder,
            "mkdir -p src/inner && echo one > src/a.txt && echo two > src/inner/b.txt && \
             tar -czf site.tar.gz -C src . && mkdir site",
        );
        let archive = text(&folder.join("site.tar.gz"));
        let summary = extract_local(
            &archive,
            ExtractInto::Folder,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(summary.format, ArchiveFormat::TarGzip);
        assert_eq!(summary.destination, text(&folder.join("site (2)")));
        let unpacked = folder.join("site (2)");
        assert_eq!(
            std::fs::read_to_string(unpacked.join("inner/b.txt")).unwrap(),
            "two\n"
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn extracts_here_without_overwriting() {
        let zip = std::process::Command::new("zip").arg("-v").output();
        if !has("unzip") || !has("gzip") || zip.is_err() {
            return;
        }
        let folder = scratch();
        shell(
            &folder,
            "echo new > a.txt && echo fresh > c.txt && zip -q pack.zip a.txt c.txt && \
             echo old > a.txt && rm c.txt",
        );
        let summary = extract_local(
            &text(&folder.join("pack.zip")),
            ExtractInto::Here,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(summary.destination, text(&folder));
        assert_eq!(
            summary.placed,
            [text(&folder.join("a (2).txt")), text(&folder.join("c.txt"))]
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("a.txt")).unwrap(),
            "old\n"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("a (2).txt")).unwrap(),
            "new\n"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("c.txt")).unwrap(),
            "fresh\n"
        );

        shell(&folder, "echo data > dump.sql && gzip -k dump.sql");
        let single = extract_local(
            &text(&folder.join("dump.sql.gz")),
            ExtractInto::Here,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(single.format, ArchiveFormat::Gzip);
        assert_eq!(single.destination, text(&folder.join("dump (2).sql")));
        assert_eq!(single.placed, [text(&folder.join("dump (2).sql"))]);
        assert_eq!(
            std::fs::read_to_string(folder.join("dump (2).sql")).unwrap(),
            "data\n"
        );
        assert_eq!(
            names_in(&folder),
            [
                "a (2).txt",
                "a.txt",
                "c.txt",
                "dump (2).sql",
                "dump.sql",
                "dump.sql.gz",
                "pack.zip"
            ]
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// Names in a folder, sorted, to show nothing else was left there.
    fn names_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .map(|item| item.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Extract here unpacks the way Extract to a folder does, then moves the items out: a
    /// folder in the archive that is already there comes out beside it, not merged into it.
    #[cfg(unix)]
    #[test]
    fn extracts_here_beside_a_folder_of_the_same_name() {
        if !has("tar") || !has("gzip") {
            return;
        }
        let folder = scratch();
        shell(
            &folder,
            "mkdir -p src/site/inner && echo one > src/site/a.txt && echo two > \
             src/site/inner/b.txt && tar -czf site.tar.gz -C src site && rm -r src && \
             mkdir site && echo old > site/a.txt",
        );
        let summary = extract_local(
            &text(&folder.join("site.tar.gz")),
            ExtractInto::Here,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(summary.destination, text(&folder.join("site (2)")));
        assert_eq!(summary.placed, [text(&folder.join("site (2)"))]);
        assert_eq!(summary.folder, text(&folder));
        assert_eq!(
            std::fs::read_to_string(folder.join("site/a.txt")).unwrap(),
            "old\n"
        );
        assert!(!folder.join("site/inner").exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("site (2)/inner/b.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(names_in(&folder), ["site", "site (2)", "site.tar.gz"]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// An archive whose own folder entry is read-only still comes out here.
    #[cfg(unix)]
    #[test]
    fn extracts_here_from_an_archive_with_a_read_only_root() {
        if !has("tar") {
            return;
        }
        let folder = scratch();
        shell(
            &folder,
            "mkdir src && echo hi > src/a.txt && chmod 555 src && tar -cf pack.tar -C src . && \
             chmod 755 src && rm -r src",
        );
        let summary = extract_local(
            &text(&folder.join("pack.tar")),
            ExtractInto::Here,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(summary.placed, [text(&folder.join("a.txt"))]);
        assert_eq!(names_in(&folder), ["a.txt", "pack.tar"]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn leaves_no_empty_folder_after_a_failure() {
        if !has("tar") {
            return;
        }
        let folder = scratch();
        let archive = folder.join("broken.tar.gz");
        std::fs::write(&archive, b"\x1F\x8B\x08\x00not really gzip").unwrap();
        for into in [ExtractInto::Folder, ExtractInto::Here] {
            let error = extract_local(
                &text(&archive),
                into,
                &Extractor::Auto,
                &CancellationToken::new(),
                &Progress::default(),
            )
            .unwrap_err();
            assert!(error.message.starts_with("GNU tar") || error.message.starts_with("bsdtar"));
            assert_eq!(names_in(&folder), ["broken.tar.gz"], "{into:?}");
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn runs_a_custom_program() {
        if !has("tar") {
            return;
        }
        let folder = scratch();
        shell(
            &folder,
            "echo hi > a.txt && tar -cf pack.tar a.txt && rm a.txt",
        );
        let custom = Extractor::Custom(CustomTool {
            program: "tar".into(),
            arguments: "-xf \"{archive}\" -C \"{folder}\"".into(),
        });
        let summary = extract_local(
            &text(&folder.join("pack.tar")),
            ExtractInto::Folder,
            &custom,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(summary.tool, "tar");
        assert_eq!(
            std::fs::read_to_string(folder.join("pack/a.txt")).unwrap(),
            "hi\n"
        );
        let here = extract_local(
            &text(&folder.join("pack.tar")),
            ExtractInto::Here,
            &custom,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(here.placed, [text(&folder.join("a.txt"))]);
        assert_eq!(names_in(&folder), ["a.txt", "pack", "pack.tar"]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// Every tool this machine has, driven by the same shell scripts servers run, over the
    /// formats it opens: each extracts the archive and leaves a file already there alone.
    #[cfg(unix)]
    #[test]
    fn server_scripts_extract_with_every_tool_here() {
        let has_program = |program: &str| {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("command -v {program}"))
                .output()
                .is_ok_and(|output| output.status.success())
        };
        if !["tar", "gzip", "zip", "xz"]
            .iter()
            .all(|program| has_program(program))
        {
            return;
        }
        let folder = scratch();
        shell(
            &folder,
            "mkdir -p src/deep && echo one > src/a.txt && echo two > src/deep/b.txt && \
             tar -cf pack.tar -C src . && tar -czf pack.tgz -C src . && \
             tar -cJf pack.txz -C src . && (cd src && zip -qr ../pack.zip .) && \
             echo single > one.txt && gzip -k one.txt && xz -k one.txt",
        );
        if has_program("zstd") {
            shell(
                &folder,
                "zstd -q -k one.txt && tar -c -C src . | zstd -q -o pack.tar.zst",
            );
        }
        if has_program("7z") {
            shell(
                &folder,
                "(cd src && 7z a -bd -bso0 ../pack.7z . >/dev/null)",
            );
        }
        let candidates = [
            (ToolKind::SevenZip, "7z"),
            (ToolKind::SevenZipStandalone, "7za"),
            (ToolKind::BsdTar, "bsdtar"),
            (ToolKind::GnuTar, "tar"),
            (ToolKind::Unzip, "unzip"),
            (ToolKind::Gzip, "gzip"),
            (ToolKind::Xz, "xz"),
            (ToolKind::Zstd, "zstd"),
        ];
        let archives = [
            ("pack.tar", ArchiveFormat::Tar),
            ("pack.tgz", ArchiveFormat::TarGzip),
            ("pack.txz", ArchiveFormat::TarXz),
            ("pack.tar.zst", ArchiveFormat::TarZstd),
            ("pack.zip", ArchiveFormat::Zip),
            ("pack.7z", ArchiveFormat::SevenZip),
            ("one.txt.gz", ArchiveFormat::Gzip),
            ("one.txt.xz", ArchiveFormat::Xz),
            ("one.txt.zst", ArchiveFormat::Zstd),
        ];
        let mut runs = 0;
        for (kind, program) in candidates {
            if !has_program(program) || (kind == ToolKind::GnuTar && tar_kind_here() != kind) {
                continue;
            }
            let tool = Tool {
                kind,
                program: program.to_string(),
            };
            for (archive, format) in archives {
                let archive = folder.join(archive);
                if !kind.opens(format) || !archive.exists() {
                    continue;
                }
                let target = folder.join(format!("out-{runs}"));
                std::fs::create_dir(&target).unwrap();
                std::fs::write(target.join("a.txt"), "kept\n").unwrap();
                let single = target.join("one.txt");
                let single_file = format.is_single_file().then(|| text(&single));
                let (archive_text, target_text) = (text(&archive), text(&target));
                let placement = Placement {
                    archive: &archive_text,
                    folder: &target_text,
                    single_file: single_file.as_deref(),
                    separator: '/',
                };
                let planned = plan(&tool, format, &placement).unwrap();
                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(shell_script(&planned))
                    .output()
                    .unwrap();
                let code = status.status.code().map(|code| code as u32);
                assert!(
                    tolerated(kind, code),
                    "{program} on {}: {}",
                    archive.display(),
                    String::from_utf8_lossy(&status.stderr)
                );
                if format.is_single_file() {
                    assert_eq!(std::fs::read_to_string(&single).unwrap(), "single\n");
                } else {
                    assert_eq!(
                        std::fs::read_to_string(target.join("a.txt")).unwrap(),
                        "kept\n",
                        "{program} replaced a file on {}",
                        archive.display()
                    );
                    assert_eq!(
                        std::fs::read_to_string(target.join("deep/b.txt")).unwrap(),
                        "two\n",
                        "{program} on {}",
                        archive.display()
                    );
                }
                runs += 1;
            }
        }
        assert!(runs >= 4);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[cfg(unix)]
    fn tar_kind_here() -> ToolKind {
        let output = std::process::Command::new("tar")
            .arg("--version")
            .output()
            .unwrap();
        tools::tar_kind(&String::from_utf8_lossy(&output.stdout))
    }

    #[test]
    fn refuses_files_that_are_not_archives() {
        let folder = scratch();
        let file = folder.join("notes.txt");
        std::fs::write(&file, "just text").unwrap();
        let error = extract_local(
            &text(&file),
            ExtractInto::Here,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unsupported);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// Every extracting program on this Windows machine, both ways, beside items of the same
    /// names, in a folder with spaces and at the root of a drive.
    #[cfg(windows)]
    #[test]
    fn extracts_both_ways_with_every_program_on_windows() {
        let run = |program: &Path, args: &[&str], dir: &Path| {
            let output = std::process::Command::new(program)
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        let base = std::env::temp_dir().join(format!("poros archive {}", uuid::Uuid::new_v4()));
        let source = base.join("source");
        std::fs::create_dir_all(source.join("site").join("inner")).unwrap();
        std::fs::write(source.join("site").join("a.txt"), "one").unwrap();
        std::fs::write(source.join("site").join("inner").join("b.txt"), "two").unwrap();
        let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
        let tar = system.join("tar.exe");
        run(
            &tar,
            &["--format", "zip", "-cf", "..\\site.zip", "site"],
            &source,
        );
        run(&tar, &["-czf", "..\\site.tar.gz", "site"], &source);
        run(
            &tar,
            &["--format", "zip", "-cf", "..\\..\\flat.zip", "."],
            &source.join("site"),
        );

        let mut seen = std::collections::HashSet::new();
        let extractors: Vec<Extractor> = std::iter::once(Extractor::Auto)
            .chain(
                installed_tools()
                    .into_iter()
                    .filter(|tool| seen.insert(tool.kind))
                    .map(|tool| Extractor::Installed { path: tool.path }),
            )
            .collect();
        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap();
        let mut case = 0;
        let mut runs = 0;
        for extractor in &extractors {
            for archive in ["site.zip", "site.tar.gz", "flat.zip"] {
                let format =
                    ArchiveFormat::detect(archive, &read_head(&base.join(archive)).unwrap())
                        .unwrap();
                if let Extractor::Installed { path } = extractor {
                    let opens = installed_tools()
                        .iter()
                        .any(|tool| tool.path == *path && tool.kind.opens(format));
                    if !opens {
                        continue;
                    }
                }
                for into in [ExtractInto::Here, ExtractInto::Folder] {
                    case += 1;
                    let folder = base.join(format!("case {case}"));
                    std::fs::create_dir_all(folder.join("site")).unwrap();
                    std::fs::write(folder.join("site").join("a.txt"), "old").unwrap();
                    std::fs::write(folder.join("a.txt"), "old").unwrap();
                    std::fs::copy(base.join(archive), folder.join(archive)).unwrap();
                    let label = format!("{extractor:?} {archive} {into:?}");
                    let summary = extract_local(
                        &text(&folder.join(archive)),
                        into,
                        extractor,
                        &CancellationToken::new(),
                        &Progress::default(),
                    )
                    .unwrap_or_else(|error| panic!("{label}: {}", error.message));
                    runs += 1;
                    assert_eq!(read(folder.join("a.txt")), "old", "{label}");
                    assert_eq!(read(folder.join("site").join("a.txt")), "old", "{label}");
                    let mut expected = vec!["a.txt", "site", archive];
                    match (into, archive) {
                        (ExtractInto::Here, "flat.zip") => {
                            assert_eq!(
                                summary.placed,
                                [text(&folder.join("a (2).txt")), text(&folder.join("inner"))],
                                "{label}"
                            );
                            assert_eq!(read(folder.join("a (2).txt")), "one", "{label}");
                            assert_eq!(read(folder.join("inner").join("b.txt")), "two", "{label}");
                            expected.extend(["a (2).txt", "inner"]);
                        }
                        (ExtractInto::Here, _) => {
                            assert_eq!(summary.placed, [text(&folder.join("site (2)"))], "{label}");
                            assert_eq!(
                                summary.destination,
                                text(&folder.join("site (2)")),
                                "{label}"
                            );
                            assert_eq!(
                                read(folder.join("site (2)").join("a.txt")),
                                "one",
                                "{label}"
                            );
                            expected.push("site (2)");
                        }
                        (ExtractInto::Folder, _) => {
                            let unpacked = if archive == "flat.zip" {
                                "flat"
                            } else {
                                "site (2)"
                            };
                            assert_eq!(summary.placed, [text(&folder.join(unpacked))], "{label}");
                            expected.push(unpacked);
                        }
                    }
                    expected.sort();
                    assert_eq!(names_in(&folder), expected, "{label}");
                }
            }
        }
        assert!(runs >= 6, "{runs}");

        // At the root of a drive, as `subst` makes one of a folder.
        let root = base.join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::copy(base.join("site.zip"), root.join("site.zip")).unwrap();
        let subst = system.join("subst.exe");
        let letter = ('P'..='Z')
            .find(|letter| !Path::new(&format!("{letter}:\\")).exists())
            .unwrap();
        let drive = format!("{letter}:");
        run(&subst, &[&drive, &text(&root)], &base);
        let here = extract_local(
            &format!("{drive}\\site.zip"),
            ExtractInto::Here,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        );
        let _ = std::process::Command::new(&subst)
            .args([&drive, "/D"])
            .output();
        let here = here.unwrap();
        assert_eq!(here.placed, [format!("{drive}\\site")]);
        assert_eq!(read(root.join("site").join("a.txt")), "one");
        assert_eq!(names_in(&root), ["site", "site.zip"]);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
