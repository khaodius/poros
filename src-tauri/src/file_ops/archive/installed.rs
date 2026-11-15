//! The extracting programs installed on this computer, and running them.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::formats::ArchiveFormat;
use super::tools::{tar_kind, unrar_kind, Output, Plan, Tool, ToolKind};
use crate::error::{AppError, AppResult, ErrorKind};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// What a program wrote to stderr that is kept for its error message.
const ERROR_OUTPUT_LIMIT: usize = 16 * 1024;

/// A program found on this computer, for Settings to list and offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTool {
    pub kind: ToolKind,
    pub name: &'static str,
    pub path: String,
    pub formats: Vec<ArchiveFormat>,
}

impl InstalledTool {
    pub fn tool(&self) -> Tool {
        Tool {
            kind: self.kind,
            program: self.path.clone(),
        }
    }
}

/// File names to look for, and what each is. `tar` and `unrar` are told apart by what they say.
#[cfg(windows)]
const PROGRAMS: &[(&str, Option<ToolKind>)] = &[
    ("7z.exe", Some(ToolKind::SevenZip)),
    ("7zz.exe", Some(ToolKind::SevenZip)),
    ("7za.exe", Some(ToolKind::SevenZipStandalone)),
    ("WinRAR.exe", Some(ToolKind::WinRar)),
    ("UnRAR.exe", Some(ToolKind::UnRar)),
    ("Rar.exe", Some(ToolKind::UnRar)),
    ("unar.exe", Some(ToolKind::Unar)),
    ("bsdtar.exe", Some(ToolKind::BsdTar)),
    ("tar.exe", None),
    ("unzip.exe", Some(ToolKind::Unzip)),
    ("gzip.exe", Some(ToolKind::Gzip)),
    ("bzip2.exe", Some(ToolKind::Bzip2)),
    ("xz.exe", Some(ToolKind::Xz)),
    ("zstd.exe", Some(ToolKind::Zstd)),
];

#[cfg(not(windows))]
const PROGRAMS: &[(&str, Option<ToolKind>)] = &[
    ("7zz", Some(ToolKind::SevenZip)),
    ("7z", Some(ToolKind::SevenZip)),
    ("7za", Some(ToolKind::SevenZipStandalone)),
    ("unrar", None),
    ("unrar-free", Some(ToolKind::UnrarFree)),
    ("rar", Some(ToolKind::UnRar)),
    ("unar", Some(ToolKind::Unar)),
    ("bsdtar", Some(ToolKind::BsdTar)),
    ("tar", None),
    ("unzip", Some(ToolKind::Unzip)),
    ("gzip", Some(ToolKind::Gzip)),
    ("bzip2", Some(ToolKind::Bzip2)),
    ("xz", Some(ToolKind::Xz)),
    ("zstd", Some(ToolKind::Zstd)),
];

/// Where installers put programs that are not on `PATH`, relative to the folders in
/// `%ProgramFiles%` and the like.
#[cfg(windows)]
const INSTALL_FOLDERS: &[&str] = &["7-Zip", "WinRAR", "PeaZip\\res\\bin\\7z"];

/// Folders to look in: `PATH` first, then where Windows installers put these programs.
fn search_folders() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(install_folders())
        .collect()
}

#[cfg(windows)]
fn install_folders() -> Vec<PathBuf> {
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .chain(std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("Programs")))
        .collect();
    let mut folders: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| INSTALL_FOLDERS.iter().map(move |folder| root.join(folder)))
        .collect();
    if let Some(home) = dirs::home_dir() {
        for app in ["7zip", "winrar", "unar", "zstd", "xz"] {
            folders.push(home.join("scoop").join("apps").join(app).join("current"));
        }
    }
    if let Some(windows) = std::env::var_os("SystemRoot") {
        folders.push(PathBuf::from(windows).join("System32"));
    }
    folders
}

#[cfg(not(windows))]
fn install_folders() -> Vec<PathBuf> {
    Vec::new()
}

fn same_file_key(path: &Path) -> OsString {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        OsString::from(resolved.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        resolved.into_os_string()
    }
}

/// The first line `program` prints, on either output, run with `args`.
fn first_line(program: &Path, args: &[&str]) -> String {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    hide_window(&mut command);
    command
        .output()
        .map(|output| {
            let mut printed = String::from_utf8_lossy(&output.stdout).into_owned();
            printed.push_str(&String::from_utf8_lossy(&output.stderr));
            printed
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default()
}

/// The `tar` at `path` as a tool, if it is one Poros can drive. On Windows that is the one
/// built on libarchive: GNU tar there, from Git or MSYS2, reads `C:` as a host name.
fn tar_tool(path: &Path) -> Option<ToolKind> {
    let version = first_line(path, &["--version"]);
    if version.is_empty() {
        return None;
    }
    let kind = tar_kind(&version);
    if cfg!(windows) && kind == ToolKind::GnuTar {
        return None;
    }
    Some(kind)
}

/// Every extracting program on this computer, in the order Settings lists them.
pub fn installed_tools() -> Vec<InstalledTool> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for folder in search_folders() {
        for &(file_name, kind) in PROGRAMS {
            let path = folder.join(file_name);
            if !path.is_file() || !seen.insert(same_file_key(&path)) {
                continue;
            }
            let told_apart = || match file_name.trim_end_matches(".exe") {
                "unrar" => Some(unrar_kind(&first_line(&path, &[]))),
                _ => tar_tool(&path),
            };
            let Some(kind) = kind.or_else(told_apart) else {
                continue;
            };
            found.push(InstalledTool {
                kind,
                name: kind.display_name(),
                path: path.to_string_lossy().into_owned(),
                formats: kind.formats().to_vec(),
            });
        }
    }
    found.sort_by_key(|tool| PROGRAMS_ORDER.iter().position(|kind| *kind == tool.kind));
    found
}

const PROGRAMS_ORDER: &[ToolKind] = &[
    ToolKind::SevenZip,
    ToolKind::WinRar,
    ToolKind::UnRar,
    ToolKind::UnrarFree,
    ToolKind::SevenZipStandalone,
    ToolKind::Unar,
    ToolKind::BsdTar,
    ToolKind::GnuTar,
    ToolKind::Unzip,
    ToolKind::Gzip,
    ToolKind::Bzip2,
    ToolKind::Xz,
    ToolKind::Zstd,
];

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

/// How a run ended: whether it counts as done, and what the programs said.
pub struct Finished {
    pub succeeded: bool,
    pub error_output: String,
}

/// Collects what a program writes to stderr, so it never blocks on a full pipe.
fn drain(child: &mut Child) -> Option<JoinHandle<String>> {
    let mut stderr = child.stderr.take()?;
    Some(std::thread::spawn(move || {
        let mut collected = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(read) = stderr.read(&mut buffer) {
            if read == 0 {
                break;
            }
            if collected.len() < ERROR_OUTPUT_LIMIT {
                collected.extend_from_slice(&buffer[..read]);
            }
        }
        String::from_utf8_lossy(&collected).into_owned()
    }))
}

/// Exit codes a program uses for "done, with warnings", such as files it left alone.
pub(super) fn tolerated(kind: Option<ToolKind>, code: i32) -> bool {
    code == 0
        || (code == 1
            && matches!(
                kind,
                Some(
                    ToolKind::SevenZip
                        | ToolKind::SevenZipStandalone
                        | ToolKind::WinRar
                        | ToolKind::UnRar
                )
            ))
}

/// Runs `plan` here, each program's output into the next one's input, until all are done or
/// `cancel` stops them. `kind` is the tool's, for its exit codes; `None` for one the user named.
pub fn run(plan: &Plan, kind: Option<ToolKind>, cancel: &CancellationToken) -> AppResult<Finished> {
    let mut children: Vec<Child> = Vec::new();
    let mut readers = Vec::new();
    let mut previous_output: Option<Stdio> = None;
    let last = plan.stages.len().saturating_sub(1);
    let stop = |children: &mut Vec<Child>| {
        for child in children.iter_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    };
    for (index, stage) in plan.stages.iter().enumerate() {
        let mut command = Command::new(&stage.program);
        command
            .args(&stage.args)
            .stdin(previous_output.take().unwrap_or_else(Stdio::null))
            .stderr(Stdio::piped());
        hide_window(&mut command);
        if index < last {
            command.stdout(Stdio::piped());
        } else {
            match &plan.output {
                Output::IntoFolder => {
                    command.stdout(Stdio::null());
                }
                Output::Stdout(file) => {
                    let file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(file)
                        .map_err(|error| AppError::from(error).with_path(file.clone()))?;
                    command.stdout(file);
                }
            }
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                stop(&mut children);
                return Err(AppError::new(
                    ErrorKind::Io,
                    format!("Could not start {}: {error}", stage.program),
                ));
            }
        };
        readers.extend(drain(&mut child));
        if index < last {
            previous_output = child.stdout.take().map(Stdio::from);
        }
        children.push(child);
    }

    let mut codes: Vec<Option<i32>> = vec![None; children.len()];
    loop {
        if cancel.is_cancelled() {
            stop(&mut children);
            return Err(AppError::cancelled());
        }
        for (child, code) in children.iter_mut().zip(codes.iter_mut()) {
            if code.is_none() {
                if let Some(status) = child.try_wait()? {
                    *code = Some(status.code().unwrap_or(-1));
                }
            }
        }
        if codes.iter().all(Option::is_some) {
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    let error_output = readers
        .into_iter()
        .filter_map(|reader| reader.join().ok())
        .filter(|output| !output.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Finished {
        succeeded: codes.iter().flatten().all(|&code| tolerated(kind, code)),
        error_output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_ops::archive::tools::Invocation;

    #[test]
    fn lists_what_is_on_path() {
        // The test machines have at least gzip; whatever is found is listed once.
        let tools = installed_tools();
        let paths: HashSet<_> = tools.iter().map(|tool| tool.path.clone()).collect();
        assert_eq!(paths.len(), tools.len());
        for tool in &tools {
            assert!(!tool.formats.is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn pipes_stages_and_reports_failures() {
        let folder = std::env::temp_dir().join(format!("poros-run-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("out.txt");
        let plan = Plan {
            stages: vec![
                Invocation {
                    program: "printf".into(),
                    args: vec!["hello".into()],
                },
                Invocation {
                    program: "cat".into(),
                    args: vec![],
                },
            ],
            output: Output::Stdout(file.to_string_lossy().into_owned()),
        };
        let finished = run(&plan, None, &CancellationToken::new()).unwrap();
        assert!(finished.succeeded);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello");

        // The file is never replaced.
        assert!(run(&plan, None, &CancellationToken::new()).is_err());

        let failing = Plan {
            stages: vec![Invocation {
                program: "sh".into(),
                args: vec!["-c".into(), "echo broken >&2; exit 3".into()],
            }],
            output: Output::IntoFolder,
        };
        let finished = run(&failing, None, &CancellationToken::new()).unwrap();
        assert!(!finished.succeeded);
        assert_eq!(finished.error_output.trim(), "broken");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn tolerates_warnings_only_from_programs_that_mean_them() {
        assert!(tolerated(Some(ToolKind::SevenZip), 1));
        assert!(tolerated(Some(ToolKind::WinRar), 1));
        assert!(!tolerated(Some(ToolKind::GnuTar), 1));
        assert!(!tolerated(None, 1));
        assert!(!tolerated(Some(ToolKind::SevenZip), 2));
    }
}
