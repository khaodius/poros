//! Notices when a program has saved a working copy. The file is looked at on a short timer
//! rather than through the system's change notifications: those differ on every platform,
//! miss files replaced by a rename unless the folder is watched too, and are not delivered at
//! all on some file systems, while a handful of files cost nothing to look at twice a second.
//!
//! Programs save in three ways, and a look at the file's size, time and identity catches each:
//! writing a temporary file and renaming it over the original (a new file under the old name),
//! cutting the file short and writing it again in place, and deleting it and making it anew.
//! The content is only read once the file has stopped moving, and counts as changed only when
//! its size and hash differ from what the server has, so a save that changes nothing, or a
//! program that merely touches the file, uploads nothing.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How often a working copy is looked at, and how long it has to stay as it is before it is
/// read: long enough for a program to finish writing a file in several steps.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub poll: Duration,
    pub settle: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(500),
            settle: Duration::from_millis(900),
        }
    }
}

/// A file's content, told apart by size and hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    pub size: u64,
    /// SHA-256 in hexadecimal.
    pub sha256: String,
}

impl Fingerprint {
    /// Reads the whole file. Fails while another program keeps it locked, as on Windows
    /// during a save.
    pub fn of(path: &Path) -> std::io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            size += read as u64;
        }
        let sha256 = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self { size, sha256 })
    }
}

/// What tells one state of a file from the next without reading it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified_nanos: Option<u128>,
    /// On Unix, the file's identity and the time of its last change of any kind, which move
    /// even when a program puts the modification time back.
    identity: Option<(u64, i64, i64)>,
}

impl Stamp {
    /// `None` when nothing is at the path, or something that is not a file.
    fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        if !metadata.is_file() {
            return None;
        }
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since| since.as_nanos());
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            Some((metadata.ino(), metadata.ctime(), metadata.ctime_nsec()))
        };
        #[cfg(not(unix))]
        let identity = None;
        Some(Self {
            size: metadata.len(),
            modified_nanos,
            identity,
        })
    }
}

/// The coarsest step a file system counts modification times in (FAT's two seconds). A file
/// read within one step of its last write can be written again without its time moving.
const COARSE_TIME: Duration = Duration::from_secs(2);

impl Stamp {
    /// Written so recently that another write could carry the same time.
    fn is_fresh(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        self.modified_nanos
            .is_none_or(|modified| modified + COARSE_TIME.as_nanos() > now)
    }
}

struct ReadContent {
    stamp: Stamp,
    content: Fingerprint,
    /// When to read the file once more although it looks the same, for a file read while
    /// its time could still stand for a later write as well.
    check_again_at: Option<Instant>,
}

/// What a look at the working copy found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    /// The file holds what the server has.
    Unchanged,
    /// The file moved a moment ago, or cannot be read yet: look again.
    Settling,
    /// Nothing is at the path, as between a delete and the file made anew.
    Missing,
    /// The file has come to rest with content the server does not have.
    Changed(Fingerprint),
}

/// Follows one working copy from look to look.
pub struct ChangeDetector {
    path: PathBuf,
    settle: Duration,
    /// The content the server has; `None` when that is not known, so any content is new.
    synced: Option<Fingerprint>,
    /// The content last read, and the state the file was in then.
    read: Option<ReadContent>,
    /// The state the file was last seen in and when it was first seen so.
    seen: Option<(Option<Stamp>, Instant)>,
}

impl ChangeDetector {
    pub fn new(path: PathBuf, settle: Duration, synced: Option<Fingerprint>) -> Self {
        Self {
            path,
            settle,
            synced,
            read: None,
            seen: None,
        }
    }

    /// The content the server has, as far as this knows.
    pub fn synced(&self) -> Option<&Fingerprint> {
        self.synced.as_ref()
    }

    /// The server now has this content.
    pub fn mark_synced(&mut self, fingerprint: Fingerprint) {
        self.synced = Some(fingerprint);
    }

    /// Looks at the file. `now` is passed in so the waiting can be tested without waiting.
    pub fn look(&mut self, now: Instant) -> Observation {
        let current = Stamp::of(&self.path);
        let since = match self.seen {
            Some((seen, since)) if seen == current => since,
            _ => {
                self.seen = Some((current, now));
                now
            }
        };
        let Some(stamp) = current else {
            return Observation::Missing;
        };
        let read = self.read.as_ref().filter(|read| read.stamp == stamp);
        let content = match read {
            Some(read) if !read.check_again_at.is_some_and(|at| now >= at) => read.content.clone(),
            _ => {
                let known = read.is_some();
                if !known && now.duration_since(since) < self.settle {
                    return Observation::Settling;
                }
                let Ok(content) = Fingerprint::of(&self.path) else {
                    return Observation::Settling;
                };
                // Written to while it was read: what was read may be half of each.
                if Stamp::of(&self.path) != Some(stamp) {
                    self.seen = None;
                    return Observation::Settling;
                }
                self.read = Some(ReadContent {
                    stamp,
                    content: content.clone(),
                    check_again_at: (!known && stamp.is_fresh()).then(|| now + COARSE_TIME),
                });
                content
            }
        };
        if self.synced.as_ref() == Some(&content) {
            Observation::Unchanged
        } else {
            Observation::Changed(content)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTLE: Duration = Duration::from_millis(500);

    struct Watched {
        _folder: tempfile::TempDir,
        path: PathBuf,
        detector: ChangeDetector,
        now: Instant,
    }

    impl Watched {
        /// A working copy as it is right after its download.
        fn holding(content: &str) -> Self {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("report.txt");
            std::fs::write(&path, content).unwrap();
            let synced = Fingerprint::of(&path).unwrap();
            let detector = ChangeDetector::new(path.clone(), SETTLE, Some(synced));
            let mut watched = Self {
                _folder: folder,
                path,
                detector,
                now: Instant::now(),
            };
            assert_eq!(watched.settled(), Observation::Unchanged);
            watched
        }

        fn look(&mut self) -> Observation {
            self.detector.look(self.now)
        }

        fn later(&mut self, wait: Duration) -> Observation {
            self.now += wait;
            self.look()
        }

        /// Looks until the file has been still for the settle time.
        fn settled(&mut self) -> Observation {
            self.look();
            self.later(SETTLE)
        }

        fn changed_to(&mut self, content: &str) -> Fingerprint {
            match self.settled() {
                Observation::Changed(fingerprint) => {
                    assert_eq!(fingerprint, fingerprint_of(content));
                    fingerprint
                }
                other => panic!("expected a change to {content:?}, saw {other:?}"),
            }
        }
    }

    fn fingerprint_of(content: &str) -> Fingerprint {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("content");
        std::fs::write(&path, content).unwrap();
        Fingerprint::of(&path).unwrap()
    }

    #[test]
    fn fingerprints_tell_content_apart() {
        assert_eq!(
            fingerprint_of("abc"),
            Fingerprint {
                size: 3,
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                    .to_string(),
            }
        );
        assert_ne!(fingerprint_of("abc"), fingerprint_of("abd"));
        assert_eq!(fingerprint_of("").size, 0);
    }

    #[test]
    fn a_save_through_a_temporary_file_and_a_rename_is_seen() {
        let mut watched = Watched::holding("first");
        let temporary = watched.path.with_file_name(".report.txt.swp");
        std::fs::write(&temporary, "second version").unwrap();
        std::fs::rename(&temporary, &watched.path).unwrap();
        assert_eq!(watched.look(), Observation::Settling);
        watched.changed_to("second version");
    }

    #[test]
    fn a_save_that_truncates_and_rewrites_in_place_is_seen() {
        let mut watched = Watched::holding("first version, the longer one");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&watched.path)
            .unwrap();
        // Cut short and not yet written: nothing is read while the save is under way.
        assert_eq!(watched.look(), Observation::Settling);
        {
            use std::io::Write;
            let mut file = file;
            file.write_all(b"second").unwrap();
        }
        assert_eq!(watched.later(SETTLE / 2), Observation::Settling);
        watched.changed_to("second");
    }

    #[test]
    fn a_save_that_deletes_the_file_and_makes_it_anew_is_seen() {
        let mut watched = Watched::holding("first");
        std::fs::remove_file(&watched.path).unwrap();
        assert_eq!(watched.look(), Observation::Missing);
        assert_eq!(watched.later(SETTLE * 3), Observation::Missing);
        std::fs::write(&watched.path, "made anew").unwrap();
        assert_eq!(watched.look(), Observation::Settling);
        watched.changed_to("made anew");
    }

    #[test]
    fn the_same_size_with_other_content_is_a_change() {
        let mut watched = Watched::holding("aaaa");
        std::fs::write(&watched.path, "bbbb").unwrap();
        // Where the file system's clock is too coarse to tell the two writes apart, the
        // second look at a freshly written file finds it.
        let mut seen = watched.settled();
        for _ in 0..4 {
            if seen != Observation::Unchanged {
                break;
            }
            seen = watched.later(COARSE_TIME);
        }
        assert_eq!(seen, Observation::Changed(fingerprint_of("bbbb")));
    }

    #[test]
    fn a_save_of_the_same_content_is_no_change() {
        let mut watched = Watched::holding("same");
        let temporary = watched.path.with_file_name("report.txt.tmp");
        std::fs::write(&temporary, "same").unwrap();
        // A later time than the first write, which Windows clocks can give both writes.
        let later = std::fs::metadata(&watched.path).unwrap().modified().unwrap() + COARSE_TIME;
        std::fs::File::options()
            .write(true)
            .open(&temporary)
            .unwrap()
            .set_modified(later)
            .unwrap();
        std::fs::rename(&temporary, &watched.path).unwrap();
        assert_eq!(watched.look(), Observation::Settling);
        assert_eq!(watched.settled(), Observation::Unchanged);
    }

    #[test]
    fn saves_in_quick_succession_settle_on_the_last() {
        let mut watched = Watched::holding("first");
        std::fs::write(&watched.path, "second").unwrap();
        assert_eq!(watched.look(), Observation::Settling);
        assert_eq!(watched.later(SETTLE / 2), Observation::Settling);
        std::fs::write(&watched.path, "third and last").unwrap();
        // The second save starts the wait again.
        assert_eq!(watched.later(SETTLE / 2), Observation::Settling);
        assert_eq!(watched.later(SETTLE / 2), Observation::Settling);
        watched.changed_to("third and last");
    }

    #[test]
    fn a_change_is_reported_until_the_server_has_it() {
        let mut watched = Watched::holding("first");
        std::fs::write(&watched.path, "second").unwrap();
        let fingerprint = watched.changed_to("second");
        assert_eq!(
            watched.later(SETTLE),
            Observation::Changed(fingerprint.clone())
        );
        watched.detector.mark_synced(fingerprint);
        assert_eq!(watched.look(), Observation::Unchanged);
        assert_eq!(watched.later(SETTLE * 4), Observation::Unchanged);
    }

    #[test]
    fn going_back_to_what_the_server_has_is_no_change() {
        let mut watched = Watched::holding("first");
        std::fs::write(&watched.path, "second").unwrap();
        watched.changed_to("second");
        std::fs::write(&watched.path, "first").unwrap();
        assert_eq!(watched.settled(), Observation::Unchanged);
    }

    #[test]
    fn content_never_uploaded_counts_as_changed() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("recovered.txt");
        std::fs::write(&path, "kept from a crash").unwrap();
        let mut detector = ChangeDetector::new(path, SETTLE, None);
        let start = Instant::now();
        assert_eq!(detector.look(start), Observation::Settling);
        assert_eq!(
            detector.look(start + SETTLE),
            Observation::Changed(fingerprint_of("kept from a crash"))
        );
    }

    #[test]
    fn a_folder_in_the_file_s_place_is_not_read() {
        let mut watched = Watched::holding("first");
        std::fs::remove_file(&watched.path).unwrap();
        std::fs::create_dir(&watched.path).unwrap();
        assert_eq!(watched.settled(), Observation::Missing);
    }
}
