//! Extracting archives where they are: on a server with the programs it has, found with one
//! command over SSH, or on this computer with the installed program Settings picks. The first
//! bytes of the file decide its format, and nothing already there is overwritten.

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
use crate::remote_path;
use crate::rsync::{run_command, shell_quote};
use crate::session::Session;
use crate::sftp::RemoteFs;
use formats::{unpacked_name, HEAD_BYTES};
use tools::{choose, custom_plan, plan, shell_script, Placement, Plan};

const COMMAND_OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_FREE_NAME_ATTEMPTS: usize = 10_000;

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
    /// What to show: the new folder, the unpacked file, or the folder extracted into.
    pub destination: String,
    /// The folder the archive is in.
    pub folder: String,
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
        .filter(|line| !line.is_empty())
        .last();
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
    let created = match into {
        ExtractInto::Folder => {
            let free = free_remote_name(fs, &parent, &unpacked, true).await?;
            Some(fs.make_dir(&parent, &free).await?)
        }
        ExtractInto::Here => None,
    };
    let folder = created.clone().unwrap_or_else(|| parent.clone());
    let single_file = match format.is_single_file() {
        true => Some(remote_path::join(
            &folder,
            &free_remote_name(fs, &folder, &unpacked, false).await?,
        )),
        false => None,
    };
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

    let result = cancellable(cancel, async {
        let channel = session.open_command_channel().await?;
        run_command(channel, &command, COMMAND_OUTPUT_LIMIT).await
    })
    .await;
    let tool_name = tool.kind.display_name();
    let failed = match result {
        Ok(output) if tolerated(tool.kind, output.exit_status) => {
            return Ok(ExtractSummary {
                destination: single_file.or(created).unwrap_or_else(|| parent.clone()),
                folder: parent,
                format,
                tool: tool_name.to_string(),
                warning: warning_from(&output.error_output),
            });
        }
        Ok(output) => failure(tool_name, &output.error_output, output.exit_status),
        Err(error) => error,
    };
    // Nothing half done stays behind where Poros made the place for it.
    if let Some(file) = &single_file {
        let _ = fs.delete(std::slice::from_ref(file)).await;
    }
    if let Some(created) = &created {
        if fs
            .list_dir(created)
            .await
            .is_ok_and(|listing| listing.entries.is_empty())
        {
            let _ = fs.delete(std::slice::from_ref(created)).await;
        }
    }
    Err(failed)
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
    let format = ArchiveFormat::detect(&name, &read_head(&archive)?)
        .ok_or_else(|| not_an_archive(&name))?;

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
    let created = match into {
        ExtractInto::Folder => {
            let folder = free_local_name(&parent, &unpacked, true)?;
            std::fs::create_dir(&folder)
                .map_err(|error| AppError::from(error).with_path(text(&folder)))?;
            Some(folder)
        }
        ExtractInto::Here => None,
    };
    let folder = created.clone().unwrap_or_else(|| parent.clone());
    let single_file = match (format.is_single_file(), chosen) {
        (true, Some(_)) => Some(free_local_name(&folder, &unpacked, false)?),
        _ => None,
    };
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

    progress.working_on(&format!("With {tool_name}"));
    let failed = match installed::run(&plan, kind, cancel) {
        Ok(finished) if finished.succeeded => {
            return Ok(ExtractSummary {
                destination: text(single_file.as_ref().or(created.as_ref()).unwrap_or(&parent)),
                folder: text(&parent),
                format,
                tool: tool_name,
                warning: warning_from(&finished.error_output),
            });
        }
        Ok(finished) => failure(&tool_name, &finished.error_output, None),
        Err(error) => error,
    };
    if let Some(file) = &single_file {
        let _ = std::fs::remove_file(file);
    }
    if let Some(created) = &created {
        // Only removes the folder while it is empty.
        let _ = std::fs::remove_dir(created);
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

    fn has(program: &str) -> bool {
        installed_tools()
            .iter()
            .any(|tool| Path::new(&tool.path).file_stem().is_some_and(|stem| stem == program))
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
        assert_eq!(std::fs::read_to_string(folder.join("a.txt")).unwrap(), "old\n");
        assert_eq!(std::fs::read_to_string(folder.join("c.txt")).unwrap(), "fresh\n");

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
        assert_eq!(
            std::fs::read_to_string(folder.join("dump (2).sql")).unwrap(),
            "data\n"
        );
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
        let error = extract_local(
            &text(&archive),
            ExtractInto::Folder,
            &Extractor::Auto,
            &CancellationToken::new(),
            &Progress::default(),
        )
        .unwrap_err();
        assert!(error.message.starts_with("GNU tar") || error.message.starts_with("bsdtar"));
        assert!(!folder.join("broken").exists());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn runs_a_custom_program() {
        if !has("tar") {
            return;
        }
        let folder = scratch();
        shell(&folder, "echo hi > a.txt && tar -cf pack.tar a.txt && rm a.txt");
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
        if !["tar", "gzip", "zip", "xz"].iter().all(|program| has_program(program)) {
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
            shell(&folder, "zstd -q -k one.txt && tar -c -C src . | zstd -q -o pack.tar.zst");
        }
        if has_program("7z") {
            shell(&folder, "(cd src && 7z a -bd -bso0 ../pack.7z . >/dev/null)");
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
        let output = std::process::Command::new("tar").arg("--version").output().unwrap();
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
}
