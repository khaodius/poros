//! Working copies on disk: a folder of its own for each file being edited, readable by this
//! user only, and beside it a small record of where the file belongs. The records are what a
//! later run goes by after a crash, to clear away what was left and to offer back the files
//! that were saved but never uploaded. A record holds no password or key.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::conflict::ServerState;
use super::watch::Fingerprint;
use crate::error::{AppError, AppResult};
use crate::protocol::Protocol;
use crate::storage;
use crate::transfer::TransferRoute;

const RECORD_EXTENSION: &str = "json";

/// The server a file belongs to, enough to tell whether a session leads to the same place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerIdentity {
    /// How the server is shown, such as `user@host`.
    pub label: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// The folder of the Cryptomator vault the file is in, for a file in one.
    pub vault_root: Option<String>,
}

impl ServerIdentity {
    pub fn of(route: &TransferRoute) -> Self {
        let profile = route.profile();
        Self {
            label: route.label().to_string(),
            protocol: profile.protocol,
            host: profile.host,
            port: profile.port,
            username: profile.username,
            vault_root: route.vault_root(),
        }
    }

    /// The same account on the same server, and the same vault there or none.
    pub fn is_same_place(&self, other: &ServerIdentity) -> bool {
        self.protocol == other.protocol
            && self.host == other.host
            && self.port == other.port
            && self.username == other.username
            && self.vault_root == other.vault_root
    }

    pub fn in_vault(&self) -> bool {
        self.vault_root.is_some()
    }
}

/// What is kept about one working copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub server: ServerIdentity,
    pub remote_path: String,
    /// The file's name on the server, for showing.
    pub name: String,
    /// The working copy's name in its folder.
    pub local_name: String,
    /// The content the server has, once the download is in.
    pub synced: Option<Fingerprint>,
    /// The server's copy as Poros last knew it.
    pub baseline: ServerState,
}

/// A working copy from an earlier run that holds changes the server never got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leftover {
    pub record: Record,
    pub size: u64,
    /// When the working copy was last saved, in milliseconds since the Unix epoch.
    pub modified: Option<u64>,
}

/// The folder working copies are kept under, made for this user alone.
pub fn prepare_root(root: &Path) -> AppResult<()> {
    make_private_folder(root)
}

/// The folder of the working copy `id`.
pub fn edit_folder(root: &Path, id: &str) -> PathBuf {
    root.join(id)
}

fn record_file(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.{RECORD_EXTENSION}"))
}

/// Makes the working copy's own folder.
pub fn create_edit_folder(root: &Path, id: &str) -> AppResult<PathBuf> {
    let folder = edit_folder(root, id);
    make_private_folder(&folder)?;
    Ok(folder)
}

fn make_private_folder(folder: &Path) -> AppResult<()> {
    let with_path =
        |error: std::io::Error| AppError::from(error).with_path(folder.display().to_string());
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(folder).map_err(with_path)?;
    // A folder that was already there keeps the permissions it had.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
            .map_err(with_path)?;
    }
    Ok(())
}

/// Lets only this user read and write the working copy, and nobody run it.
pub fn make_private_file(file: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| AppError::from(error).with_path(file.display().to_string()))?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

pub fn write_record(root: &Path, record: &Record) -> AppResult<()> {
    storage::write_json(&record_file(root, &record.id), record)
}

/// Removes a working copy with everything a program left beside it, and its record. `wipe`
/// overwrites the files first, for the decrypted files of a vault.
pub fn discard(root: &Path, id: &str, wipe: bool) {
    let folder = edit_folder(root, id);
    if wipe {
        wipe_files(&folder);
    }
    let _ = std::fs::remove_dir_all(&folder);
    let _ = std::fs::remove_file(record_file(root, id));
}

/// Writes zeros over every file under `folder`, as far as the file system writes in place.
fn wipe_files(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            wipe_files(&path);
        } else if file_type.is_file() {
            let _ = overwrite_with_zeros(&path);
        }
    }
}

fn overwrite_with_zeros(path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    let mut remaining = file.metadata()?.len();
    let zeros = [0u8; 64 * 1024];
    while remaining > 0 {
        let step = remaining.min(zeros.len() as u64) as usize;
        file.write_all(&zeros[..step])?;
        remaining -= step as u64;
    }
    file.sync_all()
}

/// Goes through what an earlier run left under `root`. Working copies that hold what the
/// server has, that never finished downloading, or that nothing is known about are removed;
/// the ones saved but never uploaded are returned. Ids in `open` belong to this run.
pub fn collect_leftovers(root: &Path, open: &[String]) -> Vec<Leftover> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_dir() {
            folders.push(name.to_string());
        } else if let Some(id) = name.strip_suffix(&format!(".{RECORD_EXTENSION}")) {
            records.push(id.to_string());
        } else {
            // A record's temporary file from a write that never finished.
            let _ = std::fs::remove_file(&path);
        }
    }
    let mut leftovers = Vec::new();
    for id in &records {
        if open.contains(id) {
            continue;
        }
        match leftover(root, id) {
            Some(found) => leftovers.push(found),
            None => discard(root, id, true),
        }
    }
    for id in folders {
        if !open.contains(&id) && !records.contains(&id) {
            discard(root, &id, true);
        }
    }
    leftovers.sort_by(|first, second| first.record.name.cmp(&second.record.name));
    leftovers
}

/// The working copy `id` when it holds content the server never got.
fn leftover(root: &Path, id: &str) -> Option<Leftover> {
    let text = std::fs::read(record_file(root, id)).ok()?;
    let record: Record = serde_json::from_slice(&text).ok()?;
    // The record is only trusted for the folder it lies beside.
    if record.id != id || !is_plain_name(&record.local_name) {
        return None;
    }
    let synced = record.synced.as_ref()?;
    let file = working_copy(root, &record);
    let metadata = std::fs::symlink_metadata(&file).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let content = Fingerprint::of(&file).ok()?;
    if content == *synced {
        return None;
    }
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as u64);
    Some(Leftover {
        size: content.size,
        modified,
        record,
    })
}

pub fn working_copy(root: &Path, record: &Record) -> PathBuf {
    edit_folder(root, &record.id).join(&record.local_name)
}

/// One path component that stays inside its folder.
fn is_plain_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, synced: Option<&str>) -> Record {
        Record {
            id: id.to_string(),
            server: ServerIdentity {
                label: "ada@files.example".to_string(),
                protocol: Protocol::Sftp,
                host: "files.example".to_string(),
                port: 22,
                username: "ada".to_string(),
                vault_root: None,
            },
            remote_path: "/srv/report.txt".to_string(),
            name: "report.txt".to_string(),
            local_name: "report.txt".to_string(),
            synced: synced.map(fingerprint_of),
            baseline: ServerState {
                size: 5,
                modified: Some(1_700_000_000),
            },
        }
    }

    fn fingerprint_of(content: &str) -> Fingerprint {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("content");
        std::fs::write(&file, content).unwrap();
        Fingerprint::of(&file).unwrap()
    }

    /// A working copy as an earlier run left it.
    fn left_behind(root: &Path, record: &Record, content: Option<&str>) {
        let folder = create_edit_folder(root, &record.id).unwrap();
        if let Some(content) = content {
            std::fs::write(folder.join(&record.local_name), content).unwrap();
        }
        write_record(root, record).unwrap();
    }

    #[test]
    fn working_copy_folders_are_for_this_user_only() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("editing");
        prepare_root(&root).unwrap();
        let folder = create_edit_folder(&root, "one").unwrap();
        let file = folder.join("secret.txt");
        std::fs::write(&file, "secret").unwrap();
        make_private_file(&file).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&root), 0o700);
            assert_eq!(mode(&folder), 0o700);
            assert_eq!(mode(&file), 0o600);
        }
    }

    #[test]
    fn only_changes_never_uploaded_are_left_over() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("editing");
        prepare_root(&root).unwrap();

        left_behind(
            &root,
            &record("changed", Some("first")),
            Some("second, saved later"),
        );
        left_behind(&root, &record("unchanged", Some("first")), Some("first"));
        left_behind(&root, &record("half-downloaded", None), Some("fir"));
        left_behind(&root, &record("file-gone", Some("first")), None);
        left_behind(
            &root,
            &record("still-open", Some("first")),
            Some("being edited"),
        );
        create_edit_folder(&root, "no-record").unwrap();
        std::fs::write(root.join("orphan.json"), "{ not a record").unwrap();
        std::fs::write(root.join("changed.json.tmp"), "half").unwrap();

        let leftovers = collect_leftovers(&root, &["still-open".to_string()]);
        assert_eq!(leftovers.len(), 1);
        assert_eq!(leftovers[0].record.id, "changed");
        assert_eq!(leftovers[0].size, "second, saved later".len() as u64);
        assert!(leftovers[0].modified.is_some());

        let mut kept: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        kept.sort();
        assert_eq!(
            kept,
            ["changed", "changed.json", "still-open", "still-open.json"]
        );
    }

    #[test]
    fn a_record_cannot_point_outside_its_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("editing");
        prepare_root(&root).unwrap();
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, "not a working copy").unwrap();

        let mut escaping = record("escaping", Some("first"));
        escaping.local_name = "../../outside.txt".to_string();
        left_behind(&root, &escaping, None);
        let mut renamed = record("renamed", Some("first"));
        renamed.id = "other".to_string();
        create_edit_folder(&root, "renamed").unwrap();
        std::fs::write(
            root.join("renamed.json"),
            serde_json::to_string(&renamed).unwrap(),
        )
        .unwrap();

        assert!(collect_leftovers(&root, &[]).is_empty());
        assert!(outside.exists());
        assert!(!root.join("escaping").exists());
        assert!(!root.join("renamed").exists());
    }

    #[test]
    fn discarding_removes_the_copy_and_what_lay_beside_it() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("editing");
        prepare_root(&root).unwrap();
        let kept = record("vault-file", Some("first"));
        left_behind(&root, &kept, Some("decrypted words"));
        let folder = edit_folder(&root, "vault-file");
        std::fs::write(folder.join("report.txt~"), "an editor's backup").unwrap();
        std::fs::create_dir(folder.join("nested")).unwrap();
        std::fs::write(folder.join("nested").join("swap"), "more").unwrap();

        discard(&root, "vault-file", true);
        assert!(!folder.exists());
        assert!(!root.join("vault-file.json").exists());
        // Discarding what is not there is not an error.
        discard(&root, "vault-file", false);
    }

    #[test]
    fn wiping_overwrites_before_deleting() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("plain.txt");
        std::fs::write(&file, "decrypted words").unwrap();
        overwrite_with_zeros(&file).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), vec![0u8; 15]);
    }

    #[test]
    fn servers_are_told_apart_by_account_and_vault() {
        let server = record("one", None).server;
        let mut relabeled = server.clone();
        relabeled.label = "Work server".to_string();
        assert!(server.is_same_place(&relabeled));
        let mut other_user = server.clone();
        other_user.username = "bob".to_string();
        assert!(!server.is_same_place(&other_user));
        let mut in_vault = server.clone();
        in_vault.vault_root = Some("/srv/vault".to_string());
        assert!(!server.is_same_place(&in_vault));
        assert!(in_vault.in_vault());
    }
}
