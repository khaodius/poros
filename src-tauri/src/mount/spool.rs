//! Local files holding what a program wrote to a mounted drive until it is on the server.
//! Most protocols cannot change part of a file, so a file open for writing lives here whole
//! and is sent when the program closes or syncs it. A spool file is removed once its contents
//! are on the server, and kept, with a note saying where it belongs, if they never got there.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

/// The extension of the note written beside a spool file that was kept.
pub const NOTE_EXTENSION: &str = "unsent.json";
/// A note being written, renamed into place once whole so it is never read half done.
const NOTE_WRITING_EXTENSION: &str = "unsent.tmp";

pub struct Spool {
    path: PathBuf,
    file: File,
    kept: AtomicBool,
}

impl Spool {
    pub fn create(directory: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(directory)?;
        let name = format!("{}.spool", uuid::Uuid::new_v4());
        let path = directory.join(name);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // The files of a private server, readable by their owner alone.
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        Ok(Self {
            path,
            file,
            kept: AtomicBool::new(false),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn set_len(&self, len: u64) -> io::Result<()> {
        self.file.set_len(len)
    }

    /// Up to `len` bytes from `offset`; fewer where the file ends.
    pub fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut buffer = vec![0; len];
        let mut filled = 0;
        while filled < len {
            let read = read_at(&self.file, &mut buffer[filled..], offset + filled as u64)?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        buffer.truncate(filled);
        Ok(buffer)
    }

    pub fn write_at(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        let mut written = 0;
        while written < data.len() {
            let count = write_at(&self.file, &data[written..], offset + written as u64)?;
            if count == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            written += count;
        }
        Ok(())
    }

    /// Leaves the file on disk when the spool is dropped, with a note of where it belongs,
    /// and makes sure its contents reached the disk.
    pub fn keep(&self, note: &UnsentNote) -> io::Result<()> {
        self.file.sync_all()?;
        self.note(note)
    }

    /// Writes the note beside the file, which then stays on disk until `release`, without
    /// waiting for the disk: for contents that are still changing.
    pub fn note(&self, note: &UnsentNote) -> io::Result<()> {
        let text = serde_json::to_vec_pretty(note).map_err(io::Error::other)?;
        // Named apart from any other write of this note under way at the same time.
        let writing = self.path.with_extension(format!(
            "{}.{NOTE_WRITING_EXTENSION}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&writing, text)?;
        std::fs::rename(&writing, note_path(&self.path))?;
        self.kept.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Undoes `keep` once the contents reached the server after all.
    pub fn release(&self) {
        if self.kept.swap(false, Ordering::Relaxed) {
            let _ = std::fs::remove_file(note_path(&self.path));
        }
    }
}

impl Drop for Spool {
    fn drop(&mut self) {
        if !self.kept.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buffer, offset)
}

#[cfg(unix)]
fn write_at(file: &File, data: &[u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::write_at(file, data, offset)
}

#[cfg(windows)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buffer, offset)
}

#[cfg(windows)]
fn write_at(file: &File, data: &[u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_write(file, data, offset)
}

fn note_path(spool: &Path) -> PathBuf {
    spool.with_extension(NOTE_EXTENSION)
}

/// What is known about a file that never reached its server. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsentNote {
    /// The connection the drive showed.
    pub connection: String,
    pub remote_path: String,
    pub size: u64,
    /// Why the upload failed, if it was tried.
    pub error: Option<String>,
    /// Seconds since the Unix epoch.
    pub kept_at: i64,
}

/// A file kept from an earlier mount, with its note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsentFile {
    /// The local file holding the contents.
    pub path: String,
    #[serde(flatten)]
    pub note: UnsentNote,
}

/// The files kept under `directory` and its folders, newest first. A spool file has its note
/// from the first change on, so one without a note holds nothing that was changed.
pub fn unsent_files(directory: &Path) -> Vec<UnsentFile> {
    let mut found = Vec::new();
    let mut folders = vec![directory.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let is_note = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(NOTE_EXTENSION));
            if !is_note {
                continue;
            }
            let Some(note) = std::fs::read(&path)
                .ok()
                .and_then(|text| serde_json::from_slice::<UnsentNote>(&text).ok())
            else {
                continue;
            };
            let spool = path.with_extension("").with_extension("spool");
            if spool.is_file() {
                found.push(UnsentFile {
                    path: spool.to_string_lossy().into_owned(),
                    note,
                });
            }
        }
    }
    found.sort_by_key(|file| std::cmp::Reverse(file.note.kept_at));
    found
}

/// Removes a kept file and its note. Only files inside `directory` are touched.
pub fn discard_unsent(directory: &Path, spool: &Path) -> io::Result<()> {
    let inside = spool
        .canonicalize()
        .ok()
        .zip(directory.canonicalize().ok())
        .is_some_and(|(spool, directory)| spool.starts_with(directory));
    let is_spool = spool
        .extension()
        .is_some_and(|extension| extension == "spool");
    if !inside || !is_spool {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Not a file kept from a mounted drive",
        ));
    }
    std::fs::remove_file(spool)?;
    let _ = std::fs::remove_file(note_path(spool));
    Ok(())
}

/// Removes spool files left without a note by a run of Poros that ended abruptly, and notes
/// it left half written: nothing says where those files belong.
pub fn remove_orphans(directory: &Path) {
    let mut folders = vec![directory.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let unnoted_spool = path
                .extension()
                .is_some_and(|extension| extension == "spool")
                && !note_path(&path).exists();
            let half_written_note = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(NOTE_WRITING_EXTENSION));
            if unnoted_spool || half_written_note {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str) -> UnsentNote {
        UnsentNote {
            connection: "poros@example.com".into(),
            remote_path: path.into(),
            size: 5,
            error: Some("The connection was closed".into()),
            kept_at: 1_700_000_000,
        }
    }

    #[test]
    fn reads_and_writes_at_offsets_and_removes_itself() {
        let folder = tempfile::tempdir().unwrap();
        let spool = Spool::create(folder.path()).unwrap();
        spool.write_at(4, b"world").unwrap();
        spool.write_at(0, b"hell").unwrap();
        assert_eq!(std::fs::metadata(spool.path()).unwrap().len(), 9);
        assert_eq!(spool.read_at(0, 100).unwrap(), b"hellworld");
        assert_eq!(spool.read_at(4, 3).unwrap(), b"wor");
        assert!(spool.read_at(50, 3).unwrap().is_empty());
        spool.set_len(2).unwrap();
        assert_eq!(spool.read_at(0, 100).unwrap(), b"he");
        let path = spool.path().to_path_buf();
        drop(spool);
        assert!(!path.exists());
    }

    #[test]
    fn a_kept_spool_stays_with_its_note_until_discarded() {
        let folder = tempfile::tempdir().unwrap();
        let spool = Spool::create(&folder.path().join("mount-1")).unwrap();
        spool.write_at(0, b"draft").unwrap();
        spool.keep(&note("/docs/draft.txt")).unwrap();
        let path = spool.path().to_path_buf();
        drop(spool);
        assert!(path.exists());

        let unsent = unsent_files(folder.path());
        assert_eq!(unsent.len(), 1);
        assert_eq!(unsent[0].note, note("/docs/draft.txt"));
        assert_eq!(std::fs::read(&unsent[0].path).unwrap(), b"draft");

        let outside = folder.path().join("../elsewhere.spool");
        assert!(discard_unsent(folder.path(), &outside).is_err());
        discard_unsent(folder.path(), &path).unwrap();
        assert!(unsent_files(folder.path()).is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn spools_without_a_note_are_cleared_and_kept_ones_stay() {
        let folder = tempfile::tempdir().unwrap();
        let kept = Spool::create(folder.path()).unwrap();
        kept.keep(&note("/a")).unwrap();
        let orphan = folder.path().join("0b4e5a9e-orphan.spool");
        std::fs::write(&orphan, b"half written").unwrap();
        let half_noted = folder.path().join("0b4e5a9e-orphan.5c2d.unsent.tmp");
        std::fs::write(&half_noted, b"{\"conn").unwrap();
        remove_orphans(folder.path());
        assert!(!orphan.exists());
        assert!(!half_noted.exists());
        assert!(kept.path().exists());
        assert_eq!(unsent_files(folder.path()).len(), 1);
    }
}
