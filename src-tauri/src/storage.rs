use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{AppError, AppResult};

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
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(".tmp");
    std::fs::write(&temporary, text).map_err(with_path)?;
    std::fs::rename(&temporary, file).map_err(with_path)
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
