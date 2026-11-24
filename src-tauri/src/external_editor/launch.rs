//! Starts programs on files. A configured program is started directly with its arguments as a
//! list, and the system's default program through the platform's opener. Neither goes through
//! a shell.

#[cfg(any(windows, test))]
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use super::command_line::Invocation;
use crate::error::{AppError, AppResult, ErrorKind};

/// Starts a program. The process comes back so its end can be noticed.
pub fn start(invocation: &Invocation) -> AppResult<tokio::process::Child> {
    let mut command = tokio::process::Command::new(program_path(&invocation.program));
    command
        .args(&invocation.arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().map_err(|error| {
        let reason = if error.kind() == std::io::ErrorKind::NotFound {
            "the program was not found".to_string()
        } else {
            error.to_string()
        };
        AppError::new(
            ErrorKind::Io,
            format!("Could not start {}: {reason}", invocation.program),
        )
    })
}

/// The program file to start for `program`. Windows looks for a bare name such as `code` on
/// the PATH only as `code.exe`, so the other extensions its shell runs (`code.cmd`) are tried
/// as well.
#[cfg(windows)]
fn program_path(program: &str) -> PathBuf {
    let folders = std::env::var_os("PATH").unwrap_or_default();
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    find_on_path(program, &folders, &extensions, Path::is_file).unwrap_or_else(|| program.into())
}

#[cfg(not(windows))]
fn program_path(program: &str) -> PathBuf {
    program.into()
}

/// The first file called `name` with one of `extensions` in the folders of `folders`, for a
/// name with neither a folder nor an extension of its own.
#[cfg(any(windows, test))]
fn find_on_path(
    name: &str,
    folders: &OsStr,
    extensions: &str,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if name.contains(['/', '\\', ':']) || Path::new(name).extension().is_some() {
        return None;
    }
    std::env::split_paths(folders).find_map(|folder| {
        extensions
            .split(';')
            .map(str::trim)
            .filter(|extension| !extension.is_empty())
            .map(|extension| folder.join(format!("{name}{extension}")))
            .find(|candidate| exists(candidate))
    })
}

/// Opens a file with the program the system has for its type.
pub async fn open_with_default(file: &Path) -> AppResult<()> {
    platform::open(file).await
}

fn no_default_program(file: &Path) -> AppError {
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    AppError::new(
        ErrorKind::Unsupported,
        format!("No program is set up to open {name}. Pick one with Open with."),
    )
}

#[cfg(windows)]
mod platform {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use crate::error::{AppError, AppResult, ErrorKind};

    /// `ShellExecuteW` answers with a number above this when it worked.
    const SHELL_EXECUTE_FAILED_UP_TO: usize = 32;
    /// Its answer when no program is registered for the file's type.
    const NO_ASSOCIATION: usize = 31;

    pub async fn open(file: &Path) -> AppResult<()> {
        let file = file.to_path_buf();
        tokio::task::spawn_blocking(move || shell_open(&file)).await?
    }

    fn shell_open(file: &Path) -> AppResult<()> {
        let path: Vec<u16> = file.as_os_str().encode_wide().chain([0]).collect();
        let operation: Vec<u16> = "open".encode_utf16().chain([0]).collect();
        // SAFETY: both strings end with a NUL and live until the call returns; the other
        // pointers may be null.
        let answer = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        } as usize;
        if answer > SHELL_EXECUTE_FAILED_UP_TO {
            Ok(())
        } else if answer == NO_ASSOCIATION {
            Err(super::no_default_program(file))
        } else {
            Err(AppError::new(
                ErrorKind::Io,
                format!("Windows could not open {} (error {answer})", file.display()),
            ))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;
    use std::process::Stdio;
    use std::time::Duration;

    use crate::error::{AppError, AppResult, ErrorKind};

    /// The openers to try in turn, each with the arguments that go before the file.
    #[cfg(target_os = "macos")]
    const OPENERS: &[(&str, &[&str])] = &[("open", &[])];
    #[cfg(not(target_os = "macos"))]
    const OPENERS: &[(&str, &[&str])] = &[("xdg-open", &[]), ("gio", &["open"])];

    /// An opener that has not ended by now is taken to have started the program, or to be it.
    const OPENER_WAIT: Duration = Duration::from_secs(4);

    pub async fn open(file: &Path) -> AppResult<()> {
        for (opener, leading) in OPENERS {
            let mut command = tokio::process::Command::new(opener);
            command
                .args(*leading)
                .arg(file)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(AppError::new(
                        ErrorKind::Io,
                        format!("Could not start {opener}: {error}"),
                    ))
                }
            };
            return match tokio::time::timeout(OPENER_WAIT, child.wait()).await {
                Ok(Ok(status)) if !status.success() => Err(super::no_default_program(file)),
                _ => Ok(()),
            };
        }
        Err(AppError::new(
            ErrorKind::Unsupported,
            "This system has no xdg-open or gio to open files with. Pick a program with Open with.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_program_that_is_not_there_says_so() {
        let error = start(&Invocation {
            program: "/nonexistent/poros-test-editor".to_string(),
            arguments: vec!["file.txt".to_string()],
        })
        .unwrap_err();
        assert!(error.message.contains("was not found"), "{}", error.message);
    }

    #[test]
    fn bare_names_are_found_with_the_extensions_the_shell_runs() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::write(second.path().join("code.cmd"), "").unwrap();
        std::fs::write(second.path().join("notepad.exe"), "").unwrap();
        std::fs::write(first.path().join("notepad.com"), "").unwrap();
        let folders = std::env::join_paths([first.path(), second.path()]).unwrap();
        let find = |name: &str| find_on_path(name, &folders, ".com;.exe; .cmd;", Path::is_file);

        assert_eq!(find("code"), Some(second.path().join("code.cmd")));
        // Folders are searched in order, each with every extension.
        assert_eq!(find("notepad"), Some(first.path().join("notepad.com")));
        assert_eq!(find("missing"), None);
        // A name with an extension or a folder is started as it is.
        assert_eq!(find("code.cmd"), None);
        assert_eq!(find(r"C:\Tools\code"), None);
        assert_eq!(find("tools/code"), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn arguments_reach_the_program_as_they_are() {
        let folder = tempfile::tempdir().unwrap();
        let output = folder
            .path()
            .join("seen by $(the) program; it's \"one\" argument");
        // `touch` makes the file it is given: a shell in between would make several others.
        let mut child = start(&Invocation {
            program: "touch".to_string(),
            arguments: vec![output.to_string_lossy().into_owned()],
        })
        .unwrap();
        assert!(child.wait().await.unwrap().success());
        let made: Vec<_> = std::fs::read_dir(folder.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(made, [output]);
    }
}
