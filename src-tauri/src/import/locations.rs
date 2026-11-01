//! Where each client keeps its sites on this computer.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{putty, read_inputs, winscp, ImportSource, Input};
use crate::error::{AppError, AppResult};

const SOURCES: [ImportSource; 8] = [
    ImportSource::FileZilla,
    ImportSource::WinScp,
    ImportSource::SmartFtp,
    ImportSource::Cyberduck,
    ImportSource::Putty,
    ImportSource::MobaXterm,
    ImportSource::OpenSsh,
    ImportSource::FileManager,
];

const SMARTFTP_OWN_FORMAT: &str = "SmartFTP keeps its favorites in a format only it reads. In \
    SmartFTP, open the Favorites window and choose Tools > Export, then choose the exported file \
    here.";

/// A client the user can import from, and whether its sites are on this computer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub source: ImportSource,
    /// The files, folders and registry keys found.
    pub found: Vec<String>,
    /// Where the client keeps its sites on this system, when none were found.
    pub places: Vec<String>,
    /// Why what was found cannot be read, and what to do instead.
    pub note: Option<String>,
}

enum Place {
    File(PathBuf),
    Folder(PathBuf),
    /// Under `HKEY_CURRENT_USER`, on Windows.
    Registry(&'static str),
}

impl Place {
    fn describe(&self) -> String {
        match self {
            Place::File(path) | Place::Folder(path) => path.display().to_string(),
            Place::Registry(key) => key.to_string(),
        }
    }
}

fn places(source: ImportSource) -> Vec<Place> {
    let windows = cfg!(windows);
    let config = dirs::config_dir().unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();
    match source {
        ImportSource::FileZilla if windows => {
            vec![Place::File(
                config.join("FileZilla").join("sitemanager.xml"),
            )]
        }
        ImportSource::FileZilla => vec![
            Place::File(config.join("filezilla").join("sitemanager.xml")),
            Place::File(home.join(".filezilla").join("sitemanager.xml")),
        ],
        ImportSource::WinScp if windows => vec![
            Place::Registry(winscp::REGISTRY_KEY),
            Place::File(config.join("WinSCP.ini")),
        ],
        ImportSource::SmartFtp if windows => vec![Place::Folder(
            config.join("SmartFTP").join("Client 2.0").join("Favorites"),
        )],
        ImportSource::Cyberduck if windows => vec![
            Place::Folder(config.join("Cyberduck").join("Bookmarks")),
            Place::Folder(config.join("Mountain Duck").join("Bookmarks")),
        ],
        ImportSource::Cyberduck => vec![Place::Folder(home.join(".duck").join("bookmarks"))],
        ImportSource::Putty if windows => putty::REGISTRY_KEYS
            .iter()
            .map(|key| Place::Registry(key))
            .collect(),
        ImportSource::Putty => vec![
            Place::Folder(config.join("putty").join("sessions")),
            Place::Folder(home.join(".putty").join("sessions")),
        ],
        ImportSource::MobaXterm if windows => vec![
            Place::File(config.join("MobaXterm").join("MobaXterm.ini")),
            Place::File(
                dirs::document_dir()
                    .unwrap_or_default()
                    .join("MobaXterm")
                    .join("MobaXterm.ini"),
            ),
        ],
        ImportSource::OpenSsh => vec![Place::File(home.join(".ssh").join("config"))],
        ImportSource::FileManager if !windows => vec![
            Place::File(config.join("gtk-3.0").join("bookmarks")),
            Place::File(
                dirs::data_dir()
                    .unwrap_or_default()
                    .join("user-places.xbel"),
            ),
        ],
        _ => Vec::new(),
    }
}

/// The files of a client's folder that hold its sites.
fn wanted(source: ImportSource) -> impl Fn(&Path) -> bool {
    move |path: &Path| {
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match source {
            ImportSource::SmartFtp => extension == "xml",
            ImportSource::Cyberduck => extension == "duck",
            _ => true,
        }
    }
}

fn exists(place: &Place, source: ImportSource) -> bool {
    match place {
        Place::File(path) => path.is_file(),
        Place::Folder(path) => folder_has(path, &wanted(source), 0),
        Place::Registry(key) => registry_key_exists(key),
    }
}

fn folder_has(folder: &Path, wanted: &dyn Fn(&Path) -> bool, depth: usize) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            depth < 4 && folder_has(&path, wanted, depth + 1)
        } else {
            wanted(&path)
        }
    })
}

pub fn source_infos() -> Vec<SourceInfo> {
    SOURCES
        .iter()
        .map(|&source| {
            let places = places(source);
            let found: Vec<String> = places
                .iter()
                .filter(|place| exists(place, source))
                .map(Place::describe)
                .collect();
            let note = (source == ImportSource::SmartFtp
                && found.is_empty()
                && places.iter().any(|place| match place {
                    Place::Folder(path) => path.is_dir(),
                    _ => false,
                }))
            .then(|| SMARTFTP_OWN_FORMAT.to_string());
            SourceInfo {
                source,
                places: if found.is_empty() {
                    places.iter().map(Place::describe).collect()
                } else {
                    Vec::new()
                },
                found,
                note,
            }
        })
        .collect()
}

/// Everything `source` keeps on this computer.
pub fn read(source: ImportSource) -> AppResult<Vec<Input>> {
    let mut inputs = Vec::new();
    for place in places(source) {
        if !exists(&place, source) {
            continue;
        }
        match place {
            Place::File(path) | Place::Folder(path) => {
                inputs.extend(read_inputs(&path, &wanted(source))?);
            }
            Place::Registry(key) => {
                if let Some(text) = export_registry(key) {
                    inputs.push(Input::from_text(key, text));
                }
            }
        }
    }
    if inputs.is_empty() && source == ImportSource::SmartFtp {
        return Err(AppError::invalid(SMARTFTP_OWN_FORMAT));
    }
    Ok(inputs)
}

#[cfg(windows)]
fn reg_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new("reg");
    command
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

#[cfg(windows)]
fn registry_key_exists(key: &str) -> bool {
    reg_command()
        .args(["query", key])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(windows))]
fn registry_key_exists(_key: &str) -> bool {
    false
}

/// The key with everything under it, as `reg export` writes it.
#[cfg(windows)]
fn export_registry(key: &str) -> Option<String> {
    let file = std::env::temp_dir().join(format!("poros-import-{}.reg", uuid::Uuid::new_v4()));
    let status = reg_command()
        .args(["export", key])
        .arg(&file)
        .arg("/y")
        .status();
    let bytes = std::fs::read(&file);
    let _ = std::fs::remove_file(&file);
    if !status.is_ok_and(|status| status.success()) {
        return None;
    }
    crate::text::decode(&bytes.ok()?)
        .ok()
        .map(|decoded| decoded.text)
}

#[cfg(not(windows))]
fn export_registry(_key: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_client() {
        let infos = source_infos();
        assert_eq!(infos.len(), SOURCES.len());
        let ssh = infos
            .iter()
            .find(|info| info.source == ImportSource::OpenSsh)
            .unwrap();
        assert!(!ssh.found.is_empty() || ssh.places[0].ends_with("config"));
    }

    #[test]
    fn finds_site_files_in_nested_folders() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("Work");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("a.duck"), "<plist/>").unwrap();
        assert!(folder_has(
            folder.path(),
            &wanted(ImportSource::Cyberduck),
            0
        ));
        assert!(!folder_has(
            folder.path(),
            &wanted(ImportSource::SmartFtp),
            0
        ));
    }
}
