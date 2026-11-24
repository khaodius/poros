use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{AppError, AppResult};

/// How old a temporary file must be to count as left by a write that never finished.
const LEFT_FOR: Duration = Duration::from_secs(60);

/// Writes through a temporary file and a rename, so a crash never leaves a half-written file.
pub fn write_json(file: &Path, value: &impl Serialize) -> AppResult<()> {
    let text = serde_json::to_string_pretty(value).map_err(|error| {
        AppError::invalid(format!("Could not encode {}: {error}", file.display()))
    })?;
    write_text(file, &text)
}

/// Reads a JSON file, `None` when there is none. A file that cannot be read or understood is
/// moved aside to `<name>.corrupt-<time>` first, so the defaults that take its place never
/// overwrite it. The error says what was wrong and where the file went, for the user.
pub fn read_json<T: DeserializeOwned>(file: &Path) -> Result<Option<T>, String> {
    let (problem, contents) = match std::fs::read(file) {
        Ok(contents) => match serde_json::from_slice(&contents) {
            Ok(value) => return Ok(Some(value)),
            Err(error) => (format!("is not valid ({error})"), Some(contents)),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => (format!("could not be read ({error})"), None),
    };
    let name = file.display();
    Err(match set_aside(file, contents.as_deref()) {
        Ok(kept) => format!("{name} {problem}. It was kept as {}.", kept.display()),
        Err(error) => format!(
            "{name} {problem}, and could not be kept aside ({error}). Saving will replace it."
        ),
    })
}

/// Renames `file` to a name of its own, or copies `contents` there when it cannot be renamed.
fn set_aside(file: &Path, contents: Option<&[u8]>) -> io::Result<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let named = |suffix: String| {
        let mut name = file.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    let mut kept = named(format!(".corrupt-{stamp}"));
    let mut attempt = 1;
    while kept.exists() {
        attempt += 1;
        kept = named(format!(".corrupt-{stamp}-{attempt}"));
    }
    match (std::fs::rename(file, &kept), contents) {
        (Ok(()), _) => Ok(kept),
        (Err(_), Some(contents)) => std::fs::write(&kept, contents).map(|()| kept),
        (Err(error), None) => Err(error),
    }
}

pub fn write_text(file: &Path, text: &str) -> AppResult<()> {
    let with_path =
        |error: std::io::Error| AppError::from(error).with_path(file.display().to_string());
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(with_path)?;
    }
    let temporary = temporary_beside(file);
    let written = std::fs::write(&temporary, text).and_then(|()| std::fs::rename(&temporary, file));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    } else {
        remove_left_temporaries(file);
    }
    written.map_err(with_path)
}

/// A temporary file beside `file` with a name of its own, so two writes at once, from two
/// threads or two processes, never share one: each renames a whole file into place.
pub fn temporary_beside(file: &Path) -> PathBuf {
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    PathBuf::from(temporary)
}

/// Deletes the temporary files that writes of `file` cut short by a crash left beside it: those
/// `temporary_beside` names, and the one `<name>.tmp` that 0.7.0 used. Only those over a minute
/// old, so a write still under way keeps its own.
pub fn remove_left_temporaries(file: &Path) {
    let (Some(folder), Some(name)) = (file.parent(), file.file_name().and_then(OsStr::to_str))
    else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let Some(middle) = entry_name
            .to_str()
            .and_then(|entry_name| entry_name.strip_prefix(name))
            .and_then(|rest| rest.strip_suffix(".tmp"))
        else {
            continue;
        };
        let temporary = middle.is_empty()
            || middle.strip_prefix('.').is_some_and(|id| {
                id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
        // Not following a link: only a file of its own is deleted.
        let left = entry
            .metadata()
            .ok()
            .filter(std::fs::Metadata::is_file)
            .and_then(|metadata| metadata.modified().ok())
            .is_some_and(|modified| {
                now.duration_since(modified)
                    .is_ok_and(|age| age >= LEFT_FOR)
            });
        if temporary && left {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The files in `folder` whose names start with `prefix`.
    pub(crate) fn kept_copies(folder: &Path, prefix: &str) -> Vec<PathBuf> {
        std::fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(prefix)
            })
            .collect()
    }

    /// Makes an empty file in `folder`, last changed `age` ago.
    pub(crate) fn file_aged(folder: &Path, name: &str, age: Duration) -> PathBuf {
        let path = folder.join(name);
        let file = std::fs::File::create(&path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
        path
    }

    #[test]
    fn a_write_removes_the_temporary_files_writes_cut_short_left() {
        let folder = tempfile::tempdir().unwrap();
        let aged = |name: &str, age: u64| file_aged(folder.path(), name, Duration::from_secs(age));
        let id = || uuid::Uuid::new_v4().simple().to_string();
        let left = [
            aged(&format!("settings.json.{}.tmp", id()), 120),
            aged("settings.json.tmp", 120),
        ];
        let kept = [
            aged(&format!("settings.json.{}.tmp", id()), 0),
            aged("settings.json.notes.tmp", 120),
            aged(&format!("connections.json.{}.tmp", id()), 120),
        ];
        write_text(&folder.path().join("settings.json"), "{}").unwrap();
        for path in left {
            assert!(!path.exists(), "{} is left", path.display());
        }
        for path in kept {
            assert!(path.exists(), "{} is gone", path.display());
        }
    }

    #[test]
    fn writes_to_one_file_at_once_leave_one_of_them_whole() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("transfers.json");
        let payloads: Vec<String> = ["a", "b"]
            .iter()
            .map(|letter| serde_json::to_string(&vec![letter.repeat(1000); 4000]).unwrap())
            .collect();
        for _ in 0..20 {
            std::thread::scope(|scope| {
                let writers: Vec<_> = payloads
                    .iter()
                    .map(|payload| scope.spawn(|| write_text(&file, payload)))
                    .collect();
                for writer in writers {
                    writer.join().unwrap().unwrap();
                }
            });
            let written = std::fs::read_to_string(&file).unwrap();
            assert!(payloads.contains(&written), "a mixed file was left");
        }
        let names: Vec<_> = std::fs::read_dir(folder.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["transfers.json"]);
    }

    #[test]
    fn a_missing_file_reads_as_none() {
        let folder = tempfile::tempdir().unwrap();
        let read = read_json::<serde_json::Value>(&folder.path().join("settings.json"));
        assert_eq!(read, Ok(None));
    }

    #[test]
    fn a_corrupt_file_is_kept_aside_before_anything_replaces_it() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("settings.json");
        std::fs::write(&file, "{\"transfers\": {\"workers\": 8,").unwrap();

        let problem = read_json::<serde_json::Value>(&file).unwrap_err();
        assert!(problem.contains("settings.json is not valid"), "{problem}");
        let kept = kept_copies(folder.path(), "settings.json.corrupt-");
        assert_eq!(kept.len(), 1);
        assert!(
            problem.contains(&kept[0].display().to_string()),
            "{problem}"
        );
        assert!(!file.exists());

        write_json(&file, &serde_json::json!({ "transfers": {} })).unwrap();
        assert_eq!(
            std::fs::read_to_string(&kept[0]).unwrap(),
            "{\"transfers\": {\"workers\": 8,"
        );

        // A second bad file in the same second gets a name of its own.
        std::fs::write(&file, "[").unwrap();
        read_json::<serde_json::Value>(&file).unwrap_err();
        assert_eq!(
            kept_copies(folder.path(), "settings.json.corrupt-").len(),
            2
        );
    }
}
