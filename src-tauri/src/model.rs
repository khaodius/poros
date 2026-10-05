//! Types shared by the local and remote file sources. Mirrored in `src/lib/types.ts`.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

/// What a symlink resolves to. `Broken` when the target can't be stat'ed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkTarget {
    Dir,
    File,
    Broken,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    /// Absolute path in the source's own path syntax.
    pub path: String,
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<LinkTarget>,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<i64>,
    /// Permission bits (`mode & 0o7777`), when the source reports them.
    pub permissions: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub hidden: bool,
}

impl FileEntry {
    /// True for directories and symlinks that point at directories.
    pub fn is_dir_like(&self) -> bool {
        self.kind == EntryKind::Dir || self.link_target == Some(LinkTarget::Dir)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    /// Canonical absolute path of the listed directory.
    pub path: String,
    /// `None` at a filesystem root.
    pub parent: Option<String>,
    pub entries: Vec<FileEntry>,
}

/// Unix `S_IFMT` file-type bits, used by both local (unix) and SFTP modes.
pub const S_IFMT: u32 = 0o170000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFLNK: u32 = 0o120000;

pub fn kind_from_mode(mode: u32) -> EntryKind {
    match mode & S_IFMT {
        S_IFDIR => EntryKind::Dir,
        S_IFREG => EntryKind::File,
        S_IFLNK => EntryKind::Symlink,
        _ => EntryKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_bits_map_to_kinds() {
        assert_eq!(kind_from_mode(0o040755), EntryKind::Dir);
        assert_eq!(kind_from_mode(0o100644), EntryKind::File);
        assert_eq!(kind_from_mode(0o120777), EntryKind::Symlink);
        // Block devices share the 0o040000 bit with directories; must not be a dir.
        assert_eq!(kind_from_mode(0o060660), EntryKind::Other);
        assert_eq!(kind_from_mode(0o140755), EntryKind::Other);
    }
}
