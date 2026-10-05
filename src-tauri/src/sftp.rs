//! Remote filesystem over SFTP v3.
//!
//! Built on `RawSftpSession` rather than the high-level `SftpSession` because directory
//! listings need the `longname` field (owner/group names) and the transfer engine needs
//! offset-addressed, pipelined reads and writes. All methods take `&self` and can run
//! concurrently; requests are multiplexed over the one channel.

use futures::stream::{self, StreamExt};
use futures::future::BoxFuture;
use russh::client::Msg;
use russh::Channel;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::rawsession::Limits;
use russh_sftp::client::{Config, RawSftpSession};
use russh_sftp::extensions;
use russh_sftp::protocol::{FileAttributes, StatusCode};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{kind_from_mode, DirListing, EntryKind, FileEntry, LinkTarget};
use crate::remote_path;

/// Concurrent requests used when stat'ing symlinks or deleting a tree.
const PARALLEL_REQUESTS: usize = 16;
const REQUEST_TIMEOUT_SECS: u64 = 30;

pub struct RemoteFs {
    raw: RawSftpSession,
    /// The login directory, resolved once at connect.
    pub home: String,
}

impl RemoteFs {
    pub async fn open(channel: Channel<Msg>) -> AppResult<Self> {
        channel.request_subsystem(true, "sftp").await?;
        let config = Config {
            request_timeout_secs: REQUEST_TIMEOUT_SECS,
            ..Default::default()
        };
        let mut raw = RawSftpSession::new_with_config(channel.into_stream(), config);
        let version = raw.init().await?;
        if version.extensions.get(extensions::LIMITS).map(String::as_str) == Some("1") {
            if let Ok(limits) = raw.limits().await {
                raw.set_limits(Limits::from(limits));
            }
        }
        let home = canonicalize(&raw, ".").await?;
        Ok(Self { raw, home })
    }

    pub fn close(&self) {
        let _ = self.raw.close_session();
    }

    pub async fn canonicalize(&self, path: &str) -> AppResult<String> {
        canonicalize(&self.raw, path).await
    }

    /// Resolves `~`, `~/x` and relative paths against the login directory.
    pub fn resolve(&self, path: &str) -> String {
        let path = path.trim();
        if path.is_empty() || path == "~" {
            self.home.clone()
        } else if let Some(rest) = path.strip_prefix("~/") {
            remote_path::join(&self.home, rest)
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            remote_path::join(&self.home, path)
        }
    }

    pub async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let resolved = self.resolve(path);
        let dir = self
            .canonicalize(&resolved)
            .await
            .map_err(|e| e.with_path(resolved.clone()))?;
        let raw_entries = self
            .read_dir(&dir)
            .await
            .map_err(|e| e.with_path(dir.clone()))?;

        let mut entries: Vec<FileEntry> = raw_entries
            .into_iter()
            .map(|(name, longname, attrs)| entry_from_attrs(&dir, name, &longname, &attrs))
            .collect();

        // Resolve symlink targets concurrently so the UI knows which ones are folders.
        let links: Vec<(usize, String)> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == EntryKind::Symlink)
            .map(|(i, e)| (i, e.path.clone()))
            .collect();
        let resolved_links: Vec<(usize, Option<FileAttributes>)> = stream::iter(links)
            .map(|(i, path)| async move { (i, self.raw.stat(path).await.ok().map(|a| a.attrs)) })
            .buffer_unordered(PARALLEL_REQUESTS)
            .collect()
            .await;
        for (i, target) in resolved_links {
            let entry = &mut entries[i];
            match target {
                Some(attrs) => {
                    let is_dir = attrs
                        .permissions
                        .is_some_and(|m| kind_from_mode(m) == EntryKind::Dir);
                    entry.link_target = Some(if is_dir {
                        LinkTarget::Dir
                    } else {
                        LinkTarget::File
                    });
                    if !is_dir {
                        entry.size = attrs.size.unwrap_or(0);
                    }
                }
                None => entry.link_target = Some(LinkTarget::Broken),
            }
        }

        Ok(DirListing {
            parent: remote_path::parent(&dir),
            path: dir,
            entries,
        })
    }

    pub async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        validate_name(name)?;
        let path = remote_path::join(&self.resolve(parent), name);
        // SFTP v3 reports an existing target as a generic failure; check first.
        self.ensure_absent(&path, name).await?;
        self.raw
            .mkdir(path.clone(), FileAttributes::empty())
            .await
            .map_err(|e| AppError::from(e).with_path(path.clone()))?;
        Ok(path)
    }

    pub async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        validate_name(new_name)?;
        let path = self.resolve(path);
        let parent = remote_path::parent(&path)
            .ok_or_else(|| AppError::invalid("Cannot rename the root directory"))?;
        let target = remote_path::join(&parent, new_name);
        // SFTP v3 rename semantics on existing targets vary by server; refuse up front.
        self.ensure_absent(&target, new_name).await?;
        self.raw
            .rename(path.clone(), target.clone())
            .await
            .map_err(|e| AppError::from(e).with_path(path))?;
        Ok(target)
    }

    /// Deletes files and symlinks, and directories recursively. Symlinks are never followed.
    pub async fn delete(&self, paths: &[String]) -> AppResult<()> {
        for path in paths {
            let path = self.resolve(path);
            if path == "/" {
                return Err(AppError::invalid("Refusing to delete /"));
            }
            let attrs = self
                .raw
                .lstat(path.clone())
                .await
                .map_err(|e| AppError::from(e).with_path(path.clone()))?
                .attrs;
            if is_dir(&attrs) {
                self.remove_tree(path).await?;
            } else {
                self.raw
                    .remove(path.clone())
                    .await
                    .map_err(|e| AppError::from(e).with_path(path))?;
            }
        }
        Ok(())
    }

    fn remove_tree(&self, dir: String) -> BoxFuture<'_, AppResult<()>> {
        Box::pin(async move {
            let children = self
                .read_dir(&dir)
                .await
                .map_err(|e| e.with_path(dir.clone()))?;
            let mut subdirs = Vec::new();
            let mut files = Vec::new();
            for (name, _, attrs) in children {
                let path = remote_path::join(&dir, &name);
                if is_dir(&attrs) {
                    subdirs.push(path);
                } else {
                    files.push(path);
                }
            }
            let results: Vec<AppResult<()>> = stream::iter(files)
                .map(|path| async move {
                    self.raw
                        .remove(path.clone())
                        .await
                        .map(|_| ())
                        .map_err(|e| AppError::from(e).with_path(path))
                })
                .buffer_unordered(PARALLEL_REQUESTS)
                .collect()
                .await;
            results.into_iter().collect::<AppResult<Vec<_>>>()?;
            for sub in subdirs {
                self.remove_tree(sub).await?;
            }
            self.raw
                .rmdir(dir.clone())
                .await
                .map_err(|e| AppError::from(e).with_path(dir))?;
            Ok(())
        })
    }

    /// Raw directory read: `(name, longname, attrs)` without `.` and `..`.
    async fn read_dir(&self, dir: &str) -> AppResult<Vec<(String, String, FileAttributes)>> {
        let handle = self.raw.opendir(dir).await?.handle;
        let mut out = Vec::new();
        let result = loop {
            match self.raw.readdir(handle.as_str()).await {
                Ok(name) => out.extend(
                    name.files
                        .into_iter()
                        .filter(|f| f.filename != "." && f.filename != "..")
                        .map(|f| (f.filename, f.longname, f.attrs)),
                ),
                Err(SftpError::Status(s)) if s.status_code == StatusCode::Eof => break Ok(()),
                Err(e) => break Err(AppError::from(e)),
            }
        };
        let _ = self.raw.close(handle).await;
        result.map(|_| out)
    }

    async fn ensure_absent(&self, path: &str, name: &str) -> AppResult<()> {
        if self.raw.lstat(path).await.is_ok() {
            Err(
                AppError::new(ErrorKind::AlreadyExists, format!("{name} already exists"))
                    .with_path(path),
            )
        } else {
            Ok(())
        }
    }
}

async fn canonicalize(raw: &RawSftpSession, path: &str) -> AppResult<String> {
    let name = raw.realpath(path).await?;
    name.files
        .into_iter()
        .next()
        .map(|f| f.filename)
        .ok_or_else(|| AppError::new(ErrorKind::Sftp, "Server returned no path"))
}

fn is_dir(attrs: &FileAttributes) -> bool {
    attrs
        .permissions
        .is_some_and(|m| kind_from_mode(m) == EntryKind::Dir)
}

fn validate_name(name: &str) -> AppResult<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        Err(AppError::invalid(format!("\"{name}\" is not a valid name")))
    } else {
        Ok(())
    }
}

fn entry_from_attrs(dir: &str, name: String, longname: &str, attrs: &FileAttributes) -> FileEntry {
    let kind = attrs.permissions.map(kind_from_mode).unwrap_or(EntryKind::File);
    let (owner, group) = owner_group(longname, attrs);
    FileEntry {
        path: remote_path::join(dir, &name),
        hidden: name.starts_with('.'),
        kind,
        link_target: None,
        size: if kind == EntryKind::Dir {
            0
        } else {
            attrs.size.unwrap_or(0)
        },
        modified: attrs.mtime.map(i64::from),
        permissions: attrs.permissions.map(|m| m & 0o7777),
        owner,
        group,
        name,
    }
}

/// SFTP v3 only carries numeric uid/gid; OpenSSH and most servers put the names in the
/// `ls -l` style longname (`drwxr-xr-x  2 alice staff 4096 Jan 1 00:00 name`).
fn owner_group(longname: &str, attrs: &FileAttributes) -> (Option<String>, Option<String>) {
    let mut fields = longname.split_whitespace();
    let looks_like_ls = fields
        .next()
        .is_some_and(|p| p.len() >= 10 && p.starts_with(['-', 'd', 'l', 'c', 'b', 'p', 's']));
    if looks_like_ls {
        let _links = fields.next();
        if let (Some(owner), Some(group)) = (fields.next(), fields.next()) {
            return (Some(owner.to_string()), Some(group.to_string()));
        }
    }
    (
        attrs.uid.map(|u| u.to_string()),
        attrs.gid.map(|g| g.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_group_from_longname() {
        let attrs = FileAttributes {
            uid: Some(1000),
            gid: Some(1000),
            ..Default::default()
        };
        assert_eq!(
            owner_group("-rw-r--r--    1 alice    staff        42 Jan  1 00:00 f.txt", &attrs),
            (Some("alice".into()), Some("staff".into()))
        );
        assert_eq!(
            owner_group("", &attrs),
            (Some("1000".into()), Some("1000".into()))
        );
    }

    #[test]
    fn entry_kinds_and_sizes() {
        let dir = FileAttributes {
            permissions: Some(0o040755),
            size: Some(4096),
            mtime: Some(1_700_000_000),
            ..Default::default()
        };
        let e = entry_from_attrs("/srv", "www".into(), "", &dir);
        assert_eq!(e.kind, EntryKind::Dir);
        assert_eq!(e.size, 0);
        assert_eq!(e.path, "/srv/www");
        assert_eq!(e.permissions, Some(0o755));
        assert_eq!(e.modified, Some(1_700_000_000));

        let file = FileAttributes {
            permissions: Some(0o100600),
            size: Some(12),
            ..Default::default()
        };
        let e = entry_from_attrs("/", ".env".into(), "", &file);
        assert_eq!(e.kind, EntryKind::File);
        assert_eq!(e.size, 12);
        assert!(e.hidden);
        assert_eq!(e.path, "/.env");
    }
}
