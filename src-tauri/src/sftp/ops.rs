//! Requests for working with files where they are: renames that may replace, links, owners,
//! permissions, server-side copies and whole-file reads.

use futures::stream::{FuturesOrdered, StreamExt};
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, Packet, StatusCode};

use super::RemoteFs;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::model::{kind_from_mode, EntryKind};

/// Copies a byte range between two open handles on the server (OpenSSH 9.0 and later).
pub const COPY_DATA: &str = "copy-data";
/// A rename that replaces an existing file in one step.
pub const POSIX_RENAME: &str = "posix-rename@openssh.com";

const READS_IN_FLIGHT: usize = 16;
const READ_CHUNK: u32 = 256 * 1024;

/// An entry's own attributes; links are not followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryStat {
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub permissions: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

impl From<&FileAttributes> for EntryStat {
    fn from(attributes: &FileAttributes) -> Self {
        Self {
            kind: attributes
                .permissions
                .map(kind_from_mode)
                .unwrap_or(EntryKind::File),
            size: attributes.size.unwrap_or(0),
            modified: attributes.mtime.map(i64::from),
            accessed: attributes.atime.map(i64::from),
            permissions: attributes.permissions.map(|mode| mode & 0o7777),
            uid: attributes.uid,
            gid: attributes.gid,
        }
    }
}

impl RemoteFs {
    pub fn supports_copy_data(&self) -> bool {
        self.copy_data
    }

    /// `None` when nothing exists at `path`.
    pub async fn lstat_entry(&self, path: &str) -> AppResult<Option<EntryStat>> {
        match self.raw.lstat(path).await {
            Ok(reply) => Ok(Some(EntryStat::from(&reply.attrs))),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {
                Ok(None)
            }
            Err(error) => Err(AppError::from(error).with_path(path)),
        }
    }

    /// The names in a folder with each entry's own attributes.
    pub async fn entries(&self, directory: &str) -> AppResult<Vec<(String, EntryStat)>> {
        let children = self
            .read_dir(directory)
            .await
            .map_err(|error| error.with_path(directory))?;
        Ok(children
            .into_iter()
            .map(|(name, _, attributes)| (name, EntryStat::from(&attributes)))
            .collect())
    }

    /// Renames to a full path. What happens when `to` exists depends on the server.
    pub async fn rename_path(&self, from: &str, to: &str) -> AppResult<()> {
        self.raw
            .rename(from, to)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(from))
    }

    /// Renames over an existing file, in one step where the server allows it.
    pub async fn rename_replacing(&self, from: &str, to: &str) -> AppResult<()> {
        if self.posix_rename {
            let mut request = Vec::with_capacity(8 + from.len() + to.len());
            put_string(&mut request, from.as_bytes());
            put_string(&mut request, to.as_bytes());
            let reply = self.raw.extended(POSIX_RENAME, request).await;
            return status_reply(reply, from);
        }
        if self.lstat_entry(to).await?.is_some() {
            self.remove_file(to).await?;
        }
        self.rename_path(from, to).await
    }

    pub async fn remove_file(&self, path: &str) -> AppResult<()> {
        self.raw
            .remove(path)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(path))
    }

    pub async fn remove_empty_dir(&self, path: &str) -> AppResult<()> {
        self.raw
            .rmdir(path)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(path))
    }

    pub async fn make_dir_at(&self, path: &str, permissions: Option<u32>) -> AppResult<()> {
        let attributes = FileAttributes {
            permissions,
            ..Default::default()
        };
        self.raw
            .mkdir(path, attributes)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(path))
    }

    pub async fn read_link(&self, path: &str) -> AppResult<String> {
        let name = self
            .raw
            .readlink(path)
            .await
            .map_err(|error| AppError::from(error).with_path(path))?;
        name.files
            .into_iter()
            .next()
            .map(|file| file.filename)
            .ok_or_else(|| AppError::new(ErrorKind::Sftp, "Server returned no link target"))
    }

    pub async fn make_symlink(&self, link: &str, target: &str) -> AppResult<()> {
        // OpenSSH takes the target first, the reverse of the protocol draft, and other servers
        // follow it.
        self.raw
            .symlink(target, link)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(link))
    }

    pub async fn set_permissions(&self, path: &str, permissions: u32) -> AppResult<()> {
        let attributes = FileAttributes {
            permissions: Some(permissions),
            ..Default::default()
        };
        self.raw
            .setstat(path, attributes)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(path))
    }

    /// SFTP sets the owner and group together, by number.
    pub async fn set_owner(&self, path: &str, uid: u32, gid: u32) -> AppResult<()> {
        let attributes = FileAttributes {
            uid: Some(uid),
            gid: Some(gid),
            ..Default::default()
        };
        self.raw
            .setstat(path, attributes)
            .await
            .map(|_| ())
            .map_err(|error| AppError::from(error).with_path(path))
    }

    /// Has the server copy `length` bytes at `offset` from one open file into another.
    pub async fn copy_data(
        &self,
        from_handle: &str,
        to_handle: &str,
        offset: u64,
        length: u64,
    ) -> AppResult<()> {
        let mut request = Vec::with_capacity(32 + from_handle.len() + to_handle.len());
        put_string(&mut request, from_handle.as_bytes());
        request.extend_from_slice(&offset.to_be_bytes());
        request.extend_from_slice(&length.to_be_bytes());
        put_string(&mut request, to_handle.as_bytes());
        request.extend_from_slice(&offset.to_be_bytes());
        let reply = self.raw.extended(COPY_DATA, request).await;
        status_reply(reply, "")
    }

    /// The whole file, refusing files that grow past `limit` bytes.
    pub async fn read_to_end(&self, path: &str, limit: u64) -> AppResult<Vec<u8>> {
        let handle = self.open_for_read(path).await?;
        let result = self.read_handle_to_end(&handle, limit).await;
        let _ = self.close_handle(handle).await;
        result.map_err(|error| error.with_path(path))
    }

    async fn read_handle_to_end(&self, handle: &str, limit: u64) -> AppResult<Vec<u8>> {
        let chunk = self.read_size(READ_CHUNK).max(1);
        let mut data = Vec::new();
        let mut next_offset = 0u64;
        let mut in_flight = FuturesOrdered::new();
        // Reads cover one byte past the limit, so a larger file shows itself.
        while next_offset <= limit || !in_flight.is_empty() {
            while next_offset <= limit && in_flight.len() < READS_IN_FLIGHT {
                in_flight.push_back(self.read_range(handle, next_offset, chunk));
                next_offset += u64::from(chunk);
            }
            let Some(bytes) = in_flight.next().await else {
                break;
            };
            let bytes = bytes?;
            let reached_end = bytes.len() < chunk as usize;
            data.extend_from_slice(&bytes);
            if data.len() as u64 > limit {
                return Err(AppError::invalid("The file is too large to open here"));
            }
            if reached_end {
                break;
            }
        }
        Ok(data)
    }

    /// `len` bytes at `offset`, fewer only at the end of the file.
    pub async fn read_range(&self, handle: &str, offset: u64, len: u32) -> AppResult<Vec<u8>> {
        let mut data = Vec::with_capacity(len as usize);
        while data.len() < len as usize {
            let position = offset + data.len() as u64;
            let wanted = self.read_size(len - data.len() as u32).max(1);
            match self.read_chunk(handle, position, wanted).await? {
                super::ReadChunk::Eof => break,
                super::ReadChunk::Data(bytes) => data.extend_from_slice(&bytes),
            }
        }
        Ok(data)
    }
}

fn put_string(buffer: &mut Vec<u8>, value: &[u8]) {
    buffer.extend_from_slice(&(value.len() as u32).to_be_bytes());
    buffer.extend_from_slice(value);
}

fn status_reply(reply: Result<Packet, SftpError>, path: &str) -> AppResult<()> {
    let with_path = |error: AppError| {
        if path.is_empty() {
            error
        } else {
            error.with_path(path)
        }
    };
    match reply {
        Ok(Packet::Status(status)) if status.status_code == StatusCode::Ok => Ok(()),
        Ok(Packet::Status(status)) => Err(with_path(SftpError::Status(status).into())),
        Ok(_) => Err(with_path(AppError::new(
            ErrorKind::Sftp,
            "The server sent an unexpected reply",
        ))),
        Err(error) => Err(with_path(error.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_carry_their_length() {
        let mut buffer = Vec::new();
        put_string(&mut buffer, b"/srv/a");
        assert_eq!(buffer, b"\0\0\0\x06/srv/a");
    }

    #[test]
    fn entry_stat_keeps_link_kind_and_ids() {
        let attributes = FileAttributes {
            permissions: Some(0o120777),
            uid: Some(1000),
            gid: Some(100),
            mtime: Some(1_700_000_000),
            ..Default::default()
        };
        let stat = EntryStat::from(&attributes);
        assert_eq!(stat.kind, EntryKind::Symlink);
        assert_eq!(stat.permissions, Some(0o777));
        assert_eq!((stat.uid, stat.gid), (Some(1000), Some(100)));
        assert_eq!(stat.modified, Some(1_700_000_000));
    }
}
