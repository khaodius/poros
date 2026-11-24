//! Attaching a drive to a folder and detaching it. Mounting needs rights a desktop program
//! does not have, so the `fusermount3` helper that comes with FUSE does it and hands back the
//! open `/dev/fuse` descriptor the kernel's requests then arrive on. Nothing of libfuse is
//! linked or needed to build.

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::mount::platform::Missing;

const HELPER: &str = "fusermount3";
const DEVICE: &str = "/dev/fuse";
/// What mounted drives are called in `mount` and `df`: the type reads "fuse.poros".
const SUBTYPE: &str = "poros";
/// How long the helper may take to mount before it counts as stuck.
const HELPER_LIMIT: Duration = Duration::from_secs(30);

fn helper() -> Option<PathBuf> {
    let listed = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    listed
        .into_iter()
        .chain(["/usr/bin", "/bin", "/usr/local/bin"].map(PathBuf::from))
        .map(|folder| folder.join(HELPER))
        .find(|candidate| candidate.is_file())
}

/// `None` when drives can be mounted here.
pub fn missing() -> Option<Missing> {
    if !Path::new(DEVICE).exists() {
        return Some(Missing {
            problem: "This system has no FUSE support: /dev/fuse is missing.".into(),
            help: "Load the kernel module with: sudo modprobe fuse".into(),
            link: None,
        });
    }
    if helper().is_none() {
        return Some(Missing {
            problem: "Mounting needs the fusermount3 program, which is not installed.".into(),
            help: "Install it with: sudo apt install fuse3 (Debian, Ubuntu), \
                   sudo dnf install fuse3 (Fedora) or sudo pacman -S fuse3 (Arch)"
                .into(),
            link: None,
        });
    }
    None
}

fn require_helper() -> AppResult<PathBuf> {
    match missing() {
        Some(missing) => Err(AppError::unsupported(format!(
            "{} {}",
            missing.problem, missing.help
        ))),
        None => helper().ok_or_else(|| AppError::unsupported("fusermount3 is not installed")),
    }
}

/// The name a drive shows as its source in `mount` and `df`. The helper splits its options
/// at commas and the mount table at spaces, so only plain characters are let through.
fn source_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || "@._:/-".contains(character) {
                character
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    format!("{SUBTYPE}:{cleaned}")
}

fn helper_failure(output: &[u8], fallback: &str) -> String {
    let text = String::from_utf8_lossy(output);
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("");
    let reason = line
        .strip_prefix(HELPER)
        .map_or(line, |rest| rest.trim_start_matches(':').trim());
    if reason.is_empty() {
        fallback.to_string()
    } else {
        format!("{fallback}: {reason}")
    }
}

/// Mounts a FUSE file system at `mount_point` and returns the device its requests come from.
pub fn mount(mount_point: &Path, label: &str, read_only: bool) -> AppResult<File> {
    let helper = require_helper()?;
    let (ours, theirs) = UnixStream::pair()?;
    ours.set_read_timeout(Some(HELPER_LIMIT))?;
    let passed = theirs.as_raw_fd();
    let mut options = format!(
        "fsname={},subtype={SUBTYPE},nosuid,nodev",
        source_label(label)
    );
    if read_only {
        options.push_str(",ro");
    }
    let mut command = Command::new(helper);
    command
        .arg("-o")
        .arg(options)
        .arg("--")
        .arg(mount_point)
        .env("_FUSE_COMMFD", passed.to_string())
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // SAFETY: the closure runs between fork and exec and only calls fcntl, which is safe
    // there. It lets the helper inherit the socket it answers on.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(passed, libc::F_GETFD);
            if flags < 0 || libc::fcntl(passed, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    drop(theirs);
    let received = receive_descriptor(&ours);
    if received.is_err() {
        let _ = child.kill();
    }
    let output = child.wait_with_output()?;
    match received {
        Ok(descriptor) if output.status.success() => Ok(File::from(descriptor)),
        _ => Err(AppError::new(
            ErrorKind::Io,
            helper_failure(
                &output.stderr,
                &format!("Could not mount at {}", mount_point.display()),
            ),
        )),
    }
}

/// Reads the one descriptor the helper sends over `socket`.
fn receive_descriptor(socket: &UnixStream) -> io::Result<OwnedFd> {
    let mut payload = [0u8; 1];
    // Room for a control message holding one descriptor, aligned as its header needs.
    let mut control = [0u64; 8];
    let mut vector = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };
    // SAFETY: a zeroed msghdr is valid, and the pointers put in it outlive the call below.
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut vector;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&control) as _;
    let received = loop {
        // SAFETY: `message` points at buffers that live until the call returns.
        let received =
            unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) };
        if received >= 0 {
            break received;
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    };
    if received == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    // SAFETY: the kernel filled `control` with well-formed control messages, which the
    // CMSG macros walk within `msg_controllen`.
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let mut descriptor: RawFd = -1;
                std::ptr::copy_nonoverlapping(
                    libc::CMSG_DATA(header),
                    (&mut descriptor as *mut RawFd).cast::<u8>(),
                    std::mem::size_of::<RawFd>(),
                );
                if descriptor >= 0 {
                    return Ok(OwnedFd::from_raw_fd(descriptor));
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "the mount helper sent no device",
    ))
}

/// Why a drive could not be detached.
#[derive(Debug)]
pub enum UnmountError {
    /// A program has a file or folder of the drive open, or works inside it.
    Busy,
    Failed(AppError),
}

/// Detaches the file system at `mount_point`. With `lazy` it goes at once even while
/// programs use it; they keep what they have open until they let go.
pub fn unmount(mount_point: &Path, lazy: bool) -> Result<(), UnmountError> {
    let helper = require_helper().map_err(UnmountError::Failed)?;
    let mut command = Command::new(helper);
    command.arg("-u");
    if lazy {
        command.arg("-z");
    }
    let output = command
        .arg("--")
        .arg(mount_point)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| UnmountError::Failed(error.into()))?;
    if output.status.success() {
        return Ok(());
    }
    if String::from_utf8_lossy(&output.stderr).contains("busy") {
        return Err(UnmountError::Busy);
    }
    Err(UnmountError::Failed(AppError::new(
        ErrorKind::Io,
        helper_failure(
            &output.stderr,
            &format!("Could not unmount {}", mount_point.display()),
        ),
    )))
}

/// A file system in this process's mount table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mounted {
    pub mount_point: PathBuf,
    pub kind: String,
}

impl Mounted {
    fn is_drive(&self) -> bool {
        self.kind == format!("fuse.{SUBTYPE}")
    }
}

/// The mount table writes spaces, tabs, newlines and backslashes as `\` and three octal
/// digits.
fn unescape(field: &str) -> PathBuf {
    let bytes = field.as_bytes();
    let mut plain = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let octal = bytes
            .get(index + 1..index + 4)
            .filter(|_| bytes[index] == b'\\')
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u8::from_str_radix(digits, 8).ok());
        match octal {
            Some(byte) => {
                plain.push(byte);
                index += 4;
            }
            None => {
                plain.push(bytes[index]);
                index += 1;
            }
        }
    }
    PathBuf::from(OsString::from_vec(plain))
}

/// One line of `/proc/self/mountinfo`: the mount point is the fifth field, and the type
/// follows the lone dash that ends the optional fields.
fn parse_mount_line(line: &str) -> Option<Mounted> {
    let (before, after) = line.split_once(" - ")?;
    let mount_point = before.split(' ').nth(4)?;
    let kind = after.split(' ').next()?;
    Some(Mounted {
        mount_point: unescape(mount_point),
        kind: kind.to_string(),
    })
}

pub fn mounted() -> Vec<Mounted> {
    std::fs::read_to_string("/proc/self/mountinfo")
        .map(|table| table.lines().filter_map(parse_mount_line).collect())
        .unwrap_or_default()
}

/// Whether `path` is a mount whose program is gone: every access then fails with
/// "Transport endpoint is not connected".
fn is_dead(path: &Path) -> bool {
    std::fs::metadata(path)
        .err()
        .is_some_and(|error| error.raw_os_error() == Some(libc::ENOTCONN))
}

/// Detaches the drives a run of Poros that ended abruptly left behind. Returns where they
/// were.
pub fn clear_dead_drives() -> Vec<PathBuf> {
    let mut cleared = Vec::new();
    for entry in mounted() {
        if entry.is_drive()
            && is_dead(&entry.mount_point)
            && unmount(&entry.mount_point, true).is_ok()
        {
            cleared.push(entry.mount_point);
        }
    }
    cleared
}

/// Gets `mount_point` ready to be mounted on: a drive left dead there is detached, a missing
/// folder is made, and a folder with anything in it is refused, since the drive would hide
/// it. Returns whether the folder was made here, so it can be removed again.
pub fn prepare_mount_point(mount_point: &Path) -> AppResult<bool> {
    if !mount_point.is_absolute() {
        return Err(AppError::invalid(
            "The folder to mount at must be a full path",
        ));
    }
    let shown = mount_point.display();
    let table = mounted();
    let here = table
        .iter()
        .rev()
        .find(|entry| entry.mount_point == mount_point);
    if is_dead(mount_point) {
        if !here.is_some_and(Mounted::is_drive) {
            return Err(AppError::invalid(format!(
                "{shown} holds a mount of another program that no longer answers"
            )));
        }
        match unmount(mount_point, true) {
            Ok(()) => {}
            Err(UnmountError::Busy) => {
                return Err(AppError::new(
                    ErrorKind::Io,
                    format!("A drive left at {shown} could not be removed"),
                ));
            }
            Err(UnmountError::Failed(error)) => return Err(error),
        }
        log::info!("Removed a drive left at {shown} by an earlier run");
    } else if here.is_some() {
        return Err(AppError::invalid(format!(
            "Something is already mounted at {shown}"
        )));
    }
    match std::fs::read_dir(mount_point) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(AppError::invalid(format!(
                    "{shown} is not empty. Choose an empty folder or a new one."
                )));
            }
            Ok(false)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            std::fs::create_dir_all(mount_point).map_err(|error| {
                AppError::new(
                    ErrorKind::Io,
                    format!("Could not make the folder {shown}: {error}"),
                )
            })?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => Err(AppError::invalid(
            format!("{shown} is a file. Choose a folder."),
        )),
        Err(error) => Err(AppError::new(
            ErrorKind::Io,
            format!("Could not open {shown}: {error}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_table_lines_give_the_mount_point_and_type() {
        let line = "731 29 0:62 / /home/ada/Poros/My\\040server rw,nosuid,nodev,relatime \
                    shared:400 - fuse.poros poros:ada@example.com rw,user_id=1000,group_id=1000";
        let entry = parse_mount_line(line).unwrap();
        assert_eq!(
            entry.mount_point,
            PathBuf::from("/home/ada/Poros/My server")
        );
        assert!(entry.is_drive());

        let other = parse_mount_line(
            "25 1 8:2 / / rw,relatime shared:1 master:2 - ext4 /dev/sda2 rw,errors=remount-ro",
        )
        .unwrap();
        assert_eq!(other.mount_point, PathBuf::from("/"));
        assert!(!other.is_drive());
        assert_eq!(parse_mount_line("not a mount line"), None);
    }

    #[test]
    fn escaped_characters_in_mount_points_are_read_back() {
        assert_eq!(unescape("/a\\040b\\134c"), PathBuf::from("/a b\\c"));
        assert_eq!(unescape("/plain"), PathBuf::from("/plain"));
        assert_eq!(unescape("/ends\\04"), PathBuf::from("/ends\\04"));
    }

    #[test]
    fn the_source_label_cannot_add_mount_options() {
        assert_eq!(
            source_label("ada@example.com:/srv,allow_other x"),
            "poros:ada@example.com:/srv_allow_other_x"
        );
    }

    #[test]
    fn the_helper_message_is_passed_on_without_its_name() {
        assert_eq!(
            helper_failure(
                b"fusermount3: mountpoint is not empty\n",
                "Could not mount at /mnt/x"
            ),
            "Could not mount at /mnt/x: mountpoint is not empty"
        );
        assert_eq!(helper_failure(b"", "Could not mount"), "Could not mount");
    }

    #[test]
    fn a_folder_with_files_or_a_relative_path_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        assert!(prepare_mount_point(Path::new("relative/path")).is_err());
        let fresh = folder.path().join("new/drive");
        assert!(prepare_mount_point(&fresh).unwrap());
        assert!(!prepare_mount_point(&fresh).unwrap());
        std::fs::write(fresh.join("file.txt"), b"x").unwrap();
        assert!(prepare_mount_point(&fresh).is_err());
        assert!(prepare_mount_point(&fresh.join("file.txt")).is_err());
    }
}
