//! The messages the Linux kernel and a FUSE file system exchange over `/dev/fuse`, as far as
//! a mounted drive needs them. Fields are in host byte order and every structure is padded
//! to eight bytes, so messages are read and built field by field, without unsafe code.
//! Layouts follow the kernel's `include/uapi/linux/fuse.h`, protocol 7.31.

pub const KERNEL_VERSION: u32 = 7;
/// The newest minor version spoken here; the kernel uses the lower of its own and this.
pub const KERNEL_MINOR_VERSION: u32 = 31;
/// The oldest kernel protocol whose messages have the layouts used here (Linux 4.2).
pub const OLDEST_MINOR_VERSION: u32 = 23;

const OUT_HEADER_SIZE: usize = 16;
const ATTR_SIZE: usize = 88;
const ENTRY_OUT_SIZE: usize = 40 + ATTR_SIZE;
const DIRENT_SIZE: usize = 24;

/// The largest write the kernel may send in one request, and the most pages per request.
pub const MAX_WRITE: u32 = 1024 * 1024;
pub const MAX_PAGES: u16 = 256;
/// Room for the largest request: a write of `MAX_WRITE` bytes with its headers.
pub const READ_BUFFER_SIZE: usize = MAX_WRITE as usize + 64 * 1024;

pub mod opcode {
    pub const LOOKUP: u32 = 1;
    pub const FORGET: u32 = 2;
    pub const GETATTR: u32 = 3;
    pub const SETATTR: u32 = 4;
    pub const READLINK: u32 = 5;
    pub const SYMLINK: u32 = 6;
    pub const MKNOD: u32 = 8;
    pub const MKDIR: u32 = 9;
    pub const UNLINK: u32 = 10;
    pub const RMDIR: u32 = 11;
    pub const RENAME: u32 = 12;
    pub const LINK: u32 = 13;
    pub const OPEN: u32 = 14;
    pub const READ: u32 = 15;
    pub const WRITE: u32 = 16;
    pub const STATFS: u32 = 17;
    pub const RELEASE: u32 = 18;
    pub const FSYNC: u32 = 20;
    pub const FLUSH: u32 = 25;
    pub const INIT: u32 = 26;
    pub const OPENDIR: u32 = 27;
    pub const READDIR: u32 = 28;
    pub const RELEASEDIR: u32 = 29;
    pub const FSYNCDIR: u32 = 30;
    pub const CREATE: u32 = 35;
    pub const INTERRUPT: u32 = 36;
    pub const DESTROY: u32 = 38;
    pub const IOCTL: u32 = 39;
    pub const BATCH_FORGET: u32 = 42;
    pub const FALLOCATE: u32 = 43;
    pub const RENAME2: u32 = 45;
}

pub mod init_flag {
    pub const ASYNC_READ: u32 = 1 << 0;
    pub const ATOMIC_O_TRUNC: u32 = 1 << 3;
    pub const BIG_WRITES: u32 = 1 << 5;
    pub const AUTO_INVAL_DATA: u32 = 1 << 12;
    pub const PARALLEL_DIROPS: u32 = 1 << 18;
    pub const MAX_PAGES: u32 = 1 << 22;
}

pub mod setattr_flag {
    pub const MODE: u32 = 1 << 0;
    pub const SIZE: u32 = 1 << 3;
    pub const MTIME: u32 = 1 << 5;
    pub const MTIME_NOW: u32 = 1 << 8;
}

pub const RENAME_NOREPLACE: u32 = 1;

/// The header every request starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub length: u32,
    pub opcode: u32,
    pub unique: u64,
    pub node: u64,
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
}

/// Reads the fields of a request in order.
pub struct Fields<'a> {
    bytes: &'a [u8],
}

/// A request shorter than its opcode's fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Truncated;

impl<'a> Fields<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8], Truncated> {
        if self.bytes.len() < count {
            return Err(Truncated);
        }
        let (taken, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(taken)
    }

    pub fn u32(&mut self) -> Result<u32, Truncated> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_ne_bytes(bytes.try_into().expect("four bytes")))
    }

    pub fn u64(&mut self) -> Result<u64, Truncated> {
        let bytes = self.bytes(8)?;
        Ok(u64::from_ne_bytes(bytes.try_into().expect("eight bytes")))
    }

    pub fn skip(&mut self, count: usize) -> Result<(), Truncated> {
        self.bytes(count).map(drop)
    }

    /// A name ending in a NUL byte, as file names are sent.
    pub fn name(&mut self) -> Result<&'a [u8], Truncated> {
        let end = self
            .bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(Truncated)?;
        let name = &self.bytes[..end];
        self.bytes = &self.bytes[end + 1..];
        Ok(name)
    }

    pub fn rest(self) -> &'a [u8] {
        self.bytes
    }
}

pub fn header(message: &[u8]) -> Result<(Header, Fields<'_>), Truncated> {
    let mut fields = Fields::new(message);
    let header = Header {
        length: fields.u32()?,
        opcode: fields.u32()?,
        unique: fields.u64()?,
        node: fields.u64()?,
        uid: fields.u32()?,
        gid: fields.u32()?,
        pid: fields.u32()?,
    };
    fields.skip(4)?;
    Ok((header, fields))
}

/// What a file looks like to `stat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attr {
    pub node: u64,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: i64,
    /// File type and permission bits.
    pub mode: u32,
    pub links: u32,
    pub uid: u32,
    pub gid: u32,
}

/// The block size `stat` reports, which programs take as a hint for how much to read at once.
const PREFERRED_IO_SIZE: u32 = 128 * 1024;

/// A reply under construction.
pub struct Reply {
    bytes: Vec<u8>,
}

impl Reply {
    fn with_capacity(unique: u64, error: i32, capacity: usize) -> Self {
        let mut bytes = Vec::with_capacity(OUT_HEADER_SIZE + capacity);
        bytes.extend_from_slice(&0u32.to_ne_bytes());
        bytes.extend_from_slice(&error.to_ne_bytes());
        bytes.extend_from_slice(&unique.to_ne_bytes());
        Self { bytes }
    }

    /// An answer with nothing in it, which is how most requests succeed.
    pub fn empty(unique: u64) -> Self {
        Self::with_capacity(unique, 0, 0)
    }

    /// A failure. `errno` is positive, as in `errno.h`.
    pub fn error(unique: u64, errno: i32) -> Self {
        Self::with_capacity(unique, -errno, 0)
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_ne_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_ne_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_ne_bytes());
    }

    fn zeros(&mut self, count: usize) {
        self.bytes.resize(self.bytes.len() + count, 0);
    }

    fn attr(&mut self, attr: &Attr) {
        let seconds = attr.modified.max(0) as u64;
        self.u64(attr.node);
        self.u64(attr.size);
        self.u64(attr.size.div_ceil(512));
        self.u64(seconds);
        self.u64(seconds);
        self.u64(seconds);
        self.zeros(12);
        self.u32(attr.mode);
        self.u32(attr.links);
        self.u32(attr.uid);
        self.u32(attr.gid);
        self.u32(0);
        self.u32(PREFERRED_IO_SIZE);
        self.u32(0);
    }

    fn entry_fields(&mut self, attr: &Attr, valid_seconds: u64) {
        self.u64(attr.node);
        self.u64(0);
        self.u64(valid_seconds);
        self.u64(valid_seconds);
        self.u32(0);
        self.u32(0);
        self.attr(attr);
    }

    /// The answer to a lookup, or to anything that makes a new entry.
    pub fn entry(unique: u64, attr: &Attr, valid_seconds: u64) -> Self {
        let mut reply = Self::with_capacity(unique, 0, ENTRY_OUT_SIZE);
        reply.entry_fields(attr, valid_seconds);
        reply
    }

    pub fn attributes(unique: u64, attr: &Attr, valid_seconds: u64) -> Self {
        let mut reply = Self::with_capacity(unique, 0, 16 + ATTR_SIZE);
        reply.u64(valid_seconds);
        reply.u32(0);
        reply.u32(0);
        reply.attr(attr);
        reply
    }

    fn open_fields(&mut self, handle: u64) {
        self.u64(handle);
        self.u32(0);
        self.u32(0);
    }

    pub fn opened(unique: u64, handle: u64) -> Self {
        let mut reply = Self::with_capacity(unique, 0, 16);
        reply.open_fields(handle);
        reply
    }

    pub fn created(unique: u64, attr: &Attr, valid_seconds: u64, handle: u64) -> Self {
        let mut reply = Self::with_capacity(unique, 0, ENTRY_OUT_SIZE + 16);
        reply.entry_fields(attr, valid_seconds);
        reply.open_fields(handle);
        reply
    }

    pub fn data(unique: u64, data: &[u8]) -> Self {
        let mut reply = Self::with_capacity(unique, 0, data.len());
        reply.bytes.extend_from_slice(data);
        reply
    }

    pub fn written(unique: u64, count: u32) -> Self {
        let mut reply = Self::with_capacity(unique, 0, 8);
        reply.u32(count);
        reply.u32(0);
        reply
    }

    /// A file system with room to spare: the servers behind a drive do not say how full
    /// they are.
    pub fn statistics(unique: u64) -> Self {
        const BLOCK: u32 = 4096;
        const BLOCKS: u64 = 1 << 38;
        let mut reply = Self::with_capacity(unique, 0, 80);
        reply.u64(BLOCKS);
        reply.u64(BLOCKS);
        reply.u64(BLOCKS);
        reply.u64(1 << 32);
        reply.u64(1 << 32);
        reply.u32(BLOCK);
        reply.u32(255);
        reply.u32(BLOCK);
        reply.zeros(28);
        reply
    }

    /// The answer to the kernel's opening message.
    pub fn init(unique: u64, max_readahead: u32, flags: u32) -> Self {
        let mut reply = Self::with_capacity(unique, 0, 64);
        reply.u32(KERNEL_VERSION);
        reply.u32(KERNEL_MINOR_VERSION);
        reply.u32(max_readahead);
        reply.u32(flags);
        // Requests the kernel may have waiting on the drive at once, and when it starts
        // to hold programs back.
        reply.u16(64);
        reply.u16(48);
        reply.u32(MAX_WRITE);
        // Servers keep times to the second.
        reply.u32(1_000_000_000);
        reply.u16(MAX_PAGES);
        reply.zeros(2 + 4 + 4 + 24);
        reply
    }

    /// The finished message, with its length filled in.
    pub fn into_bytes(mut self) -> Vec<u8> {
        let length = self.bytes.len() as u32;
        self.bytes[..4].copy_from_slice(&length.to_ne_bytes());
        self.bytes
    }
}

pub const DIRENT_FOLDER: u32 = 4;
pub const DIRENT_FILE: u32 = 8;
pub const DIRENT_LINK: u32 = 10;

/// The entries of a folder, packed for as many as fit the size the kernel asked for.
pub struct FolderReply {
    reply: Reply,
    limit: usize,
}

impl FolderReply {
    pub fn new(unique: u64, limit: u32) -> Self {
        Self {
            reply: Reply::with_capacity(unique, 0, limit as usize),
            limit: OUT_HEADER_SIZE + limit as usize,
        }
    }

    /// Adds an entry; `false` once it no longer fits. `next_offset` is what the kernel sends
    /// back to continue after this entry.
    pub fn add(&mut self, node: u64, next_offset: u64, kind: u32, name: &[u8]) -> bool {
        let padded = (DIRENT_SIZE + name.len()).div_ceil(8) * 8;
        if self.reply.bytes.len() + padded > self.limit {
            return false;
        }
        self.reply.u64(node);
        self.reply.u64(next_offset);
        self.reply.u32(name.len() as u32);
        self.reply.u32(kind);
        self.reply.bytes.extend_from_slice(name);
        self.reply.zeros(padded - DIRENT_SIZE - name.len());
        true
    }

    pub fn finish(self) -> Reply {
        self.reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr() -> Attr {
        Attr {
            node: 42,
            size: 1025,
            modified: 1_700_000_000,
            mode: 0o100644,
            links: 1,
            uid: 1000,
            gid: 1000,
        }
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn u64_at(bytes: &[u8], offset: usize) -> u64 {
        u64::from_ne_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    #[test]
    fn replies_have_the_sizes_the_kernel_expects() {
        assert_eq!(Reply::empty(1).into_bytes().len(), 16);
        assert_eq!(Reply::entry(1, &attr(), 1).into_bytes().len(), 16 + 128);
        assert_eq!(
            Reply::attributes(1, &attr(), 1).into_bytes().len(),
            16 + 104
        );
        assert_eq!(Reply::opened(1, 7).into_bytes().len(), 16 + 16);
        assert_eq!(
            Reply::created(1, &attr(), 1, 7).into_bytes().len(),
            16 + 128 + 16
        );
        assert_eq!(Reply::written(1, 5).into_bytes().len(), 16 + 8);
        assert_eq!(Reply::statistics(1).into_bytes().len(), 16 + 80);
        assert_eq!(Reply::init(1, 131072, 0).into_bytes().len(), 16 + 64);
        assert_eq!(Reply::data(1, b"abc").into_bytes().len(), 19);
    }

    #[test]
    fn the_header_carries_length_error_and_request_id() {
        let failure = Reply::error(0xABCD, 2).into_bytes();
        assert_eq!(u32_at(&failure, 0), 16);
        assert_eq!(u32_at(&failure, 4) as i32, -2);
        assert_eq!(u64_at(&failure, 8), 0xABCD);
    }

    #[test]
    fn attributes_land_at_their_offsets() {
        let bytes = Reply::attributes(9, &attr(), 3).into_bytes();
        assert_eq!(u64_at(&bytes, 16), 3);
        let at = 16 + 16;
        assert_eq!(u64_at(&bytes, at), 42);
        assert_eq!(u64_at(&bytes, at + 8), 1025);
        assert_eq!(
            u64_at(&bytes, at + 16),
            3,
            "blocks of 512 bytes, rounded up"
        );
        assert_eq!(u64_at(&bytes, at + 32), 1_700_000_000);
        assert_eq!(u32_at(&bytes, at + 60), 0o100644);
        assert_eq!(u32_at(&bytes, at + 64), 1);
        assert_eq!(u32_at(&bytes, at + 68), 1000);
        assert_eq!(u32_at(&bytes, at + 80), PREFERRED_IO_SIZE);
    }

    #[test]
    fn folder_entries_are_padded_and_stop_at_the_limit() {
        let mut folder = FolderReply::new(5, 72);
        assert!(folder.add(1, 1, DIRENT_FOLDER, b"."));
        assert!(folder.add(2, 2, DIRENT_FILE, b"notes.txt"));
        assert!(!folder.add(3, 3, DIRENT_FILE, b"more"));
        let bytes = folder.finish().into_bytes();
        assert_eq!(bytes.len(), 16 + 32 + 40);
        assert_eq!(u64_at(&bytes, 16), 1);
        assert_eq!(u32_at(&bytes, 32), 1);
        assert_eq!(u32_at(&bytes, 36), DIRENT_FOLDER);
        assert_eq!(&bytes[40..41], b".");
        assert_eq!(u64_at(&bytes, 48 + 8), 2);
        assert_eq!(&bytes[48 + 24..48 + 33], b"notes.txt");
    }

    #[test]
    fn requests_are_read_field_by_field() {
        let mut message = Vec::new();
        message.extend_from_slice(&57u32.to_ne_bytes());
        message.extend_from_slice(&opcode::LOOKUP.to_ne_bytes());
        message.extend_from_slice(&77u64.to_ne_bytes());
        message.extend_from_slice(&1u64.to_ne_bytes());
        message.extend_from_slice(&1000u32.to_ne_bytes());
        message.extend_from_slice(&1001u32.to_ne_bytes());
        message.extend_from_slice(&4321u32.to_ne_bytes());
        message.extend_from_slice(&0u32.to_ne_bytes());
        message.extend_from_slice(b"old name\0new name\0");
        let (header, mut fields) = header(&message).unwrap();
        assert_eq!(
            header,
            Header {
                length: 57,
                opcode: opcode::LOOKUP,
                unique: 77,
                node: 1,
                uid: 1000,
                gid: 1001,
                pid: 4321,
            }
        );
        assert_eq!(fields.name().unwrap(), b"old name");
        assert_eq!(fields.name().unwrap(), b"new name");
        assert_eq!(fields.name(), Err(Truncated));
        assert!(super::header(&message[..20]).is_err());
    }
}
