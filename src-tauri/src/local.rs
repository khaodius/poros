//! Local filesystem operations. All functions are blocking; commands run them on
//! the blocking thread pool.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::{AppError, AppResult};
use crate::model::{DirListing, EntryKind, FileEntry, LinkTarget};

pub fn home_dir() -> AppResult<String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::invalid("Could not determine the home directory"))
}

/// Filesystem roots: `/` on unix, every mounted drive letter on Windows.
pub fn roots() -> Vec<String> {
    #[cfg(windows)]
    {
        (b'A'..=b'Z')
            .map(|letter| format!("{}:\\", letter as char))
            .filter(|root| Path::new(root).exists())
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec!["/".to_string()]
    }
}

pub fn list_dir(path: &str) -> AppResult<DirListing> {
    let dir = fs::canonicalize(path).map_err(|e| AppError::from(e).with_path(path))?;
    let mut entries = Vec::new();
    for item in fs::read_dir(&dir).map_err(|e| AppError::from(e).with_path(path))? {
        let Ok(item) = item else { continue };
        // Entries that vanish or can't be stat'ed mid-listing are skipped, not fatal.
        if let Some(entry) = entry_from_path(&item.path()) {
            entries.push(entry);
        }
    }
    Ok(DirListing {
        path: display_path(&dir),
        parent: dir.parent().map(display_path),
        entries,
    })
}

pub fn make_dir(parent: &str, name: &str) -> AppResult<String> {
    let target = child_path(parent, name)?;
    fs::create_dir(&target).map_err(|e| AppError::from(e).with_path(display_path(&target)))?;
    Ok(display_path(&target))
}

pub fn rename(path: &str, new_name: &str) -> AppResult<String> {
    let source = PathBuf::from(path);
    let parent = source
        .parent()
        .ok_or_else(|| AppError::invalid("Cannot rename a filesystem root"))?;
    let target = child_path(&parent.to_string_lossy(), new_name)?;
    if target.symlink_metadata().is_ok() {
        return Err(AppError::new(
            crate::error::ErrorKind::AlreadyExists,
            format!("{new_name} already exists"),
        )
        .with_path(display_path(&target)));
    }
    fs::rename(&source, &target).map_err(|e| AppError::from(e).with_path(path))?;
    Ok(display_path(&target))
}

/// Deletes files, symlinks (never their targets) and directories recursively.
pub fn delete(paths: &[String]) -> AppResult<()> {
    for path in paths {
        let p = Path::new(path);
        let meta = p
            .symlink_metadata()
            .map_err(|e| AppError::from(e).with_path(path.as_str()))?;
        let result = if meta.is_dir() {
            fs::remove_dir_all(p)
        } else {
            fs::remove_file(p).or_else(|e| {
                // Windows directory symlinks/junctions must be removed with remove_dir.
                if cfg!(windows) && meta.file_type().is_symlink() {
                    fs::remove_dir(p)
                } else {
                    Err(e)
                }
            })
        };
        result.map_err(|e| AppError::from(e).with_path(path.as_str()))?;
    }
    Ok(())
}

/// Joins a single path component onto `parent`, rejecting separators and `..`.
fn child_path(parent: &str, name: &str) -> AppResult<PathBuf> {
    validate_name(name)?;
    Ok(Path::new(parent).join(name))
}

pub fn validate_name(name: &str) -> AppResult<()> {
    let invalid = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || (cfg!(windows) && name.contains('\\'))
        || name.contains('\0');
    if invalid {
        Err(AppError::invalid(format!("\"{name}\" is not a valid name")))
    } else {
        Ok(())
    }
}

fn entry_from_path(path: &Path) -> Option<FileEntry> {
    let meta = path.symlink_metadata().ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let file_type = meta.file_type();
    let (kind, link_target, size, modified_meta) = if file_type.is_symlink() {
        match fs::metadata(path) {
            Ok(target) => {
                let t = if target.is_dir() {
                    LinkTarget::Dir
                } else {
                    LinkTarget::File
                };
                (EntryKind::Symlink, Some(t), target.len(), target)
            }
            Err(_) => (EntryKind::Symlink, Some(LinkTarget::Broken), 0, meta.clone()),
        }
    } else if file_type.is_dir() {
        (EntryKind::Dir, None, 0, meta.clone())
    } else if file_type.is_file() {
        (EntryKind::File, None, meta.len(), meta.clone())
    } else {
        (EntryKind::Other, None, 0, meta.clone())
    };

    let modified = modified_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    Some(FileEntry {
        hidden: is_hidden(&name, &meta),
        name,
        path: display_path(path),
        kind,
        link_target,
        size,
        modified,
        permissions: permissions(&meta),
        owner: None,
        group: None,
    })
}

#[cfg(unix)]
fn permissions(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn permissions(_meta: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(windows)]
fn is_hidden(name: &str, meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    name.starts_with('.') || meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _meta: &fs::Metadata) -> bool {
    name.starts_with('.')
}

/// Strips the `\\?\` verbatim prefix `canonicalize` adds on Windows.
fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    s.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_files_dirs_and_hidden_entries() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
        fs::write(tmp.path().join(".hidden"), b"").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();

        let listing = list_dir(tmp.path().to_str().unwrap()).unwrap();
        assert!(listing.parent.is_some());
        let find = |n: &str| listing.entries.iter().find(|e| e.name == n).unwrap();
        assert_eq!(find("a.txt").kind, EntryKind::File);
        assert_eq!(find("a.txt").size, 5);
        assert!(find(".hidden").hidden);
        assert_eq!(find("sub").kind, EntryKind::Dir);
        assert!(find("sub").is_dir_like());
    }

    #[cfg(unix)]
    #[test]
    fn resolves_symlink_targets() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("dir")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("dir"), tmp.path().join("to-dir")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("nope"), tmp.path().join("dangling")).unwrap();

        let listing = list_dir(tmp.path().to_str().unwrap()).unwrap();
        let find = |n: &str| listing.entries.iter().find(|e| e.name == n).unwrap();
        assert_eq!(find("to-dir").link_target, Some(LinkTarget::Dir));
        assert!(find("to-dir").is_dir_like());
        assert_eq!(find("dangling").link_target, Some(LinkTarget::Broken));
    }

    #[test]
    fn mkdir_rename_delete_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_str().unwrap();
        let made = make_dir(root, "new").unwrap();
        fs::write(Path::new(&made).join("f"), b"x").unwrap();
        let renamed = rename(&made, "renamed").unwrap();
        assert!(Path::new(&renamed).is_dir());
        assert!(!Path::new(&made).exists());
        delete(&[renamed.clone()]).unwrap();
        assert!(!Path::new(&renamed).exists());
    }

    #[test]
    fn rename_refuses_to_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a"), b"a").unwrap();
        fs::write(tmp.path().join("b"), b"b").unwrap();
        let err = rename(tmp.path().join("a").to_str().unwrap(), "b").unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(tmp.path().join("b")).unwrap(), b"b");
    }

    #[test]
    fn rejects_path_traversal_names() {
        assert!(validate_name("..").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("ok name.txt").is_ok());
    }
}
