//! User themes: one JSON file each in the `themes` folder of the app config folder, so people
//! can write, share and drop in their own. The frontend validates the contents
//! (`src/lib/theme.ts`); this side only keeps the files.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::storage;

const MAX_ID_LENGTH: usize = 48;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFile {
    pub id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<serde_json::Value>,
    /// Why the file could not be read, for the settings page to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct ThemeStore {
    dir: PathBuf,
}

impl ThemeStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn list(&self) -> AppResult<Vec<ThemeFile>> {
        std::fs::create_dir_all(&self.dir)?;
        let mut themes = Vec::new();
        for item in std::fs::read_dir(&self.dir)? {
            let path = item?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
            else {
                continue;
            };
            let parsed = std::fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| parse(&bytes));
            let (theme, error) = match parsed {
                Ok(theme) => (Some(theme), None),
                Err(error) => (None, Some(error)),
            };
            themes.push(ThemeFile {
                id,
                path: path.to_string_lossy().into_owned(),
                theme,
                error,
            });
        }
        themes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(themes)
    }

    /// Without an id, picks a free one from the theme's name. Returns the id.
    pub fn save(&self, id: Option<String>, theme: serde_json::Value) -> AppResult<String> {
        if !theme.is_object() {
            return Err(AppError::invalid("A theme must be a JSON object"));
        }
        let id = match id {
            Some(id) => {
                validate_id(&id)?;
                id
            }
            None => self.free_id(&slug(
                theme
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or("theme"),
            )),
        };
        storage::write_json(&self.path_of(&id), &theme)?;
        Ok(id)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        validate_id(id)?;
        match std::fs::remove_file(self.path_of(id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Copies a theme file from anywhere into the themes folder.
    pub fn import(&self, source: &Path) -> AppResult<String> {
        let bytes = std::fs::read(source)
            .map_err(|error| AppError::from(error).with_path(source.display().to_string()))?;
        let theme = parse(&bytes).map_err(AppError::invalid)?;
        let stem = source
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let id = self.free_id(&slug(&stem));
        self.save(Some(id), theme)
    }

    fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn free_id(&self, base: &str) -> String {
        let mut id = base.to_string();
        let mut number = 2;
        while self.path_of(&id).exists() {
            id = format!("{base}-{number}");
            number += 1;
        }
        id
    }
}

fn parse(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if value.is_object() {
        Ok(value)
    } else {
        Err("A theme must be a JSON object".into())
    }
}

/// `My Dark Theme!` -> `my-dark-theme`.
fn slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug
        .trim_end_matches('-')
        .chars()
        .take(MAX_ID_LENGTH)
        .collect();
    if slug.is_empty() {
        "theme".into()
    } else {
        slug
    }
}

fn validate_id(id: &str) -> AppResult<()> {
    let valid = !id.is_empty()
        && id.len() <= MAX_ID_LENGTH + 8
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(AppError::invalid(format!(
            "\"{id}\" is not a valid theme id"
        )))
    }
}

/// Opens a folder in the system file manager.
pub fn open_folder(path: &Path) -> AppResult<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program).arg(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_ids() {
        assert_eq!(slug("My Dark Theme!"), "my-dark-theme");
        assert_eq!(slug("  "), "theme");
        assert!(validate_id("my-theme_2").is_ok());
        assert!(validate_id("../escape").is_err());
        assert!(validate_id("").is_err());
    }

    #[test]
    fn save_list_import_delete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ThemeStore::new(temp_dir.path().join("themes"));
        let id = store
            .save(
                None,
                serde_json::json!({ "name": "Night Owl", "colors": {} }),
            )
            .unwrap();
        assert_eq!(id, "night-owl");
        let second = store
            .save(None, serde_json::json!({ "name": "Night Owl" }))
            .unwrap();
        assert_eq!(second, "night-owl-2");

        std::fs::write(store.dir().join("broken.json"), "{ nope").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 3);
        let broken = listed.iter().find(|theme| theme.id == "broken").unwrap();
        assert!(broken.error.is_some());

        let outside = temp_dir.path().join("Shared Theme.json");
        std::fs::write(&outside, r#"{"name": "Shared"}"#).unwrap();
        assert_eq!(store.import(&outside).unwrap(), "shared-theme");

        store.delete("night-owl").unwrap();
        assert_eq!(store.list().unwrap().len(), 3);
        assert!(store.save(None, serde_json::json!([1, 2])).is_err());
    }
}
