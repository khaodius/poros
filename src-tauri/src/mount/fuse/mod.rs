//! The Linux side of a mounted drive. A thread reads the kernel's requests from `/dev/fuse`,
//! each is handed to the file system in `vfs` on the async runtime, and the answer is
//! written back. Nothing else happens here: what a request means is decided in `vfs`.
//!
//! The kernel protocol is spoken directly instead of through libfuse or a binding to it, for
//! one reason above all: a program waiting on a slow server must be interruptible, and that
//! takes answering the kernel's `INTERRUPT` messages, which the bindings do not.

mod device;
#[cfg(test)]
mod tests;
mod wire;

use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::vfs::{AttributeChanges, Attributes, MountFs, NodeKind, OpenMode, VfsError, VfsResult};
use crate::error::{AppError, AppResult, ErrorKind};
use device::UnmountError;
use wire::{opcode, setattr_flag, Fields, FolderReply, Header, Reply, Truncated};

pub use device::{clear_dead_drives, missing, Missing};

/// How long the kernel may go on using a name or attributes without asking again.
const KERNEL_VALID_SECONDS: u64 = 1;
/// The longest a program waits for an answer about names and attributes, a listing, or a
/// read. The file system has its own, shorter limits on each step it takes on a server;
/// these catch whatever slips past them. Opening, writing and closing have none here: they
/// wait on transfers, which may take as long as the file is large.
const METADATA_LIMIT: Duration = Duration::from_secs(60);
const LISTING_LIMIT: Duration = Duration::from_secs(180);
const READ_LIMIT: Duration = Duration::from_secs(120);
/// An interrupted request that finishes within this time is still answered properly, so a
/// passing signal does not fail work that was about to succeed.
const INTERRUPT_GRACE: Duration = Duration::from_millis(200);

struct Errno(i32);

impl From<Truncated> for Errno {
    fn from(_: Truncated) -> Self {
        Errno(libc::EINVAL)
    }
}

impl From<VfsError> for Errno {
    fn from(error: VfsError) -> Self {
        Errno(match error {
            VfsError::NotFound => libc::ENOENT,
            VfsError::AlreadyExists => libc::EEXIST,
            VfsError::NotAFolder => libc::ENOTDIR,
            VfsError::IsAFolder => libc::EISDIR,
            VfsError::NotEmpty => libc::ENOTEMPTY,
            VfsError::ReadOnly => libc::EROFS,
            VfsError::PermissionDenied => libc::EACCES,
            VfsError::InvalidName => libc::EINVAL,
            VfsError::CrossFolder => libc::EXDEV,
            VfsError::Unsupported => libc::EOPNOTSUPP,
            VfsError::Interrupted => libc::EINTR,
            VfsError::TimedOut => libc::ETIMEDOUT,
            VfsError::BadHandle => libc::EBADF,
            VfsError::NoSpace => libc::ENOSPC,
            VfsError::Io(message) => {
                log::debug!("A request on a mounted drive failed: {message}");
                libc::EIO
            }
        })
    }
}

/// Runs `work` to its end whatever happens, and waits for it until the program that asked is
/// interrupted or `limit` passes. Work on a server is never cut off halfway, which could
/// leave a connection or a file there in an unknown state. A result nobody waits for any
/// more goes to `unwanted`, to give back what it holds.
async fn wait<T, Work, Unwanted>(
    interrupt: &CancellationToken,
    limit: Option<Duration>,
    work: Work,
    unwanted: Unwanted,
) -> VfsResult<T>
where
    T: Send + 'static,
    Work: Future<Output = VfsResult<T>> + Send + 'static,
    Unwanted: FnOnce(T) + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    tokio::spawn(async move {
        if let Err(Ok(value)) = sender.send(work.await) {
            unwanted(value);
        }
    });
    let interrupted = async {
        interrupt.cancelled().await;
        tokio::time::sleep(INTERRUPT_GRACE).await;
    };
    let overdue = async {
        match limit {
            Some(limit) => tokio::time::sleep(limit).await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        result = receiver => {
            result.unwrap_or_else(|_| Err(VfsError::Io("The request was abandoned".into())))
        }
        _ = interrupted => Err(VfsError::Interrupted),
        _ = overdue => Err(VfsError::TimedOut),
    }
}

fn nothing<T>(_: T) {}

/// A name as the file system takes it. Names that are not UTF-8 cannot exist on a drive.
fn name_of(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes).ok().map(str::to_string)
}

fn existing_name(bytes: &[u8]) -> Result<String, Errno> {
    name_of(bytes).ok_or(Errno(libc::ENOENT))
}

fn new_name(bytes: &[u8]) -> Result<String, Errno> {
    name_of(bytes).ok_or(Errno(libc::EINVAL))
}

fn open_mode(flags: u32) -> OpenMode {
    let flags = flags as i32;
    let access = flags & libc::O_ACCMODE;
    OpenMode {
        read: access != libc::O_WRONLY,
        write: access != libc::O_RDONLY,
        truncate: flags & libc::O_TRUNC != 0,
        append: flags & libc::O_APPEND != 0,
    }
}

type FolderEntries = Arc<Vec<(String, Attributes)>>;

/// A folder a program has open: its entries as they were when it was opened, so a program
/// deleting what it lists does not lose its place.
struct OpenFolder {
    entries: FolderEntries,
    /// Whether the entries were handed out already; reading from the start again then
    /// lists the folder anew.
    read: bool,
}

struct Session {
    core: Arc<MountFs>,
    /// Closed to end the drive: the kernel then fails whatever still waits on it.
    device: RwLock<Option<File>>,
    runtime: tokio::runtime::Handle,
    /// The requests being worked on, by the kernel's number for them.
    interrupts: Mutex<HashMap<u64, CancellationToken>>,
    folders: Mutex<HashMap<u64, OpenFolder>>,
    next_folder: AtomicU64,
    user: u32,
    group: u32,
    mounted_at: i64,
    /// Set once the kernel has let go of the drive.
    ended: AtomicBool,
}

impl Session {
    fn send(&self, reply: Reply) {
        let bytes = reply.into_bytes();
        let device = self.device.read().unwrap();
        let Some(mut device) = device.as_ref() else {
            return;
        };
        if let Err(error) = device.write(&bytes) {
            // ENOENT: the program stopped waiting, as after an interrupt.
            if error.raw_os_error() != Some(libc::ENOENT) && !self.ended.load(Ordering::Relaxed) {
                log::debug!("An answer to the kernel was not taken: {error}");
            }
        }
    }

    fn attr(&self, attributes: &Attributes) -> wire::Attr {
        let (kind, usual, links) = match attributes.kind {
            NodeKind::Folder => (libc::S_IFDIR, 0o755, 2),
            NodeKind::File => (libc::S_IFREG, 0o644, 1),
        };
        wire::Attr {
            node: attributes.node,
            size: attributes.size,
            modified: attributes.modified.unwrap_or(self.mounted_at),
            mode: kind | (attributes.permissions.unwrap_or(usual) & 0o7777),
            links,
            uid: self.user,
            gid: self.group,
        }
    }

    /// Takes one message from the kernel. Called on the reading thread, in order.
    fn accept(self: &Arc<Self>, message: &[u8]) {
        let Ok((header, mut fields)) = wire::header(message) else {
            return;
        };
        match header.opcode {
            opcode::INIT => self.send(self.begin(&header, fields)),
            opcode::DESTROY => self.send(Reply::empty(header.unique)),
            opcode::FORGET => {
                if let Ok(count) = fields.u64() {
                    self.core.forget(header.node, count);
                }
            }
            opcode::BATCH_FORGET => {
                let count = fields.u32().unwrap_or(0);
                let _ = fields.skip(4);
                for _ in 0..count {
                    let (Ok(node), Ok(lookups)) = (fields.u64(), fields.u64()) else {
                        break;
                    };
                    self.core.forget(node, lookups);
                }
            }
            opcode::INTERRUPT => {
                if let Ok(unique) = fields.u64() {
                    if let Some(interrupt) = self.interrupts.lock().unwrap().get(&unique) {
                        interrupt.cancel();
                    }
                }
            }
            _ => {
                let interrupt = CancellationToken::new();
                self.interrupts
                    .lock()
                    .unwrap()
                    .insert(header.unique, interrupt.clone());
                let body = Bytes::copy_from_slice(fields.rest());
                let session = self.clone();
                self.runtime.spawn(async move {
                    let reply = match session.serve(&header, body, &interrupt).await {
                        Ok(reply) => reply,
                        Err(Errno(errno)) => Reply::error(header.unique, errno),
                    };
                    session.interrupts.lock().unwrap().remove(&header.unique);
                    session.send(reply);
                });
            }
        }
    }

    /// Answers the kernel's opening message, settling which protocol features are used.
    fn begin(&self, header: &Header, mut fields: Fields<'_>) -> Reply {
        let (Ok(major), Ok(minor), Ok(max_readahead), Ok(offered)) =
            (fields.u32(), fields.u32(), fields.u32(), fields.u32())
        else {
            return Reply::error(header.unique, libc::EPROTO);
        };
        if major != wire::KERNEL_VERSION || minor < wire::OLDEST_MINOR_VERSION {
            log::warn!("This kernel speaks FUSE {major}.{minor}, which is too old for a drive");
            return Reply::error(header.unique, libc::EPROTO);
        }
        let wanted = wire::init_flag::ASYNC_READ
            | wire::init_flag::ATOMIC_O_TRUNC
            | wire::init_flag::BIG_WRITES
            | wire::init_flag::AUTO_INVAL_DATA
            | wire::init_flag::PARALLEL_DIROPS
            | wire::init_flag::MAX_PAGES;
        Reply::init(header.unique, max_readahead, offered & wanted)
    }

    async fn serve(
        self: &Arc<Self>,
        header: &Header,
        body: Bytes,
        interrupt: &CancellationToken,
    ) -> Result<Reply, Errno> {
        let unique = header.unique;
        let node = header.node;
        let core = self.core.clone();
        let mut fields = Fields::new(&body);
        match header.opcode {
            opcode::LOOKUP => {
                let name = existing_name(fields.name()?)?;
                let found = wait(
                    interrupt,
                    Some(METADATA_LIMIT),
                    {
                        let core = core.clone();
                        async move { core.lookup(node, &name).await }
                    },
                    move |found: Attributes| core.forget(found.node, 1),
                )
                .await?;
                Ok(Reply::entry(
                    unique,
                    &self.attr(&found),
                    KERNEL_VALID_SECONDS,
                ))
            }
            opcode::GETATTR => {
                let found = wait(
                    interrupt,
                    Some(METADATA_LIMIT),
                    async move { core.attributes(node).await },
                    nothing,
                )
                .await?;
                Ok(Reply::attributes(
                    unique,
                    &self.attr(&found),
                    KERNEL_VALID_SECONDS,
                ))
            }
            opcode::SETATTR => {
                let valid = fields.u32()?;
                fields.skip(4 + 8)?;
                let size = fields.u64()?;
                fields.skip(8 + 8)?;
                let modified = fields.u64()?;
                fields.skip(8 + 4 + 4 + 4)?;
                let mode = fields.u32()?;
                let changes = AttributeChanges {
                    size: (valid & setattr_flag::SIZE != 0).then_some(size),
                    modified: if valid & setattr_flag::MTIME_NOW != 0 {
                        Some(now_seconds())
                    } else if valid & setattr_flag::MTIME != 0 {
                        Some(modified as i64)
                    } else {
                        None
                    },
                    permissions: (valid & setattr_flag::MODE != 0).then_some(mode & 0o7777),
                };
                // Changing a size may send the file, which takes as long as it is large.
                let limit = changes.size.is_none().then_some(METADATA_LIMIT);
                let found = wait(
                    interrupt,
                    limit,
                    async move { core.set_attributes(node, changes).await },
                    nothing,
                )
                .await?;
                Ok(Reply::attributes(
                    unique,
                    &self.attr(&found),
                    KERNEL_VALID_SECONDS,
                ))
            }
            opcode::MKNOD => {
                let mode = fields.u32()?;
                fields.skip(4)?;
                let umask = fields.u32()?;
                fields.skip(4)?;
                let name = new_name(fields.name()?)?;
                if mode & libc::S_IFMT != libc::S_IFREG {
                    return Err(Errno(libc::EPERM));
                }
                let permissions = mode & !umask & 0o7777;
                let made = wait(
                    interrupt,
                    Some(METADATA_LIMIT),
                    {
                        let core = core.clone();
                        async move {
                            let (attributes, handle) = core
                                .create(node, &name, OpenMode::READ_WRITE, true, Some(permissions))
                                .await?;
                            core.release(handle);
                            Ok(attributes)
                        }
                    },
                    move |made: Attributes| core.forget(made.node, 1),
                )
                .await?;
                Ok(Reply::entry(
                    unique,
                    &self.attr(&made),
                    KERNEL_VALID_SECONDS,
                ))
            }
            opcode::MKDIR => {
                fields.skip(8)?;
                let name = new_name(fields.name()?)?;
                let made = wait(
                    interrupt,
                    Some(METADATA_LIMIT),
                    {
                        let core = core.clone();
                        async move { core.make_folder(node, &name).await }
                    },
                    move |made: Attributes| core.forget(made.node, 1),
                )
                .await?;
                Ok(Reply::entry(
                    unique,
                    &self.attr(&made),
                    KERNEL_VALID_SECONDS,
                ))
            }
            opcode::UNLINK => {
                let name = existing_name(fields.name()?)?;
                wait(
                    interrupt,
                    Some(METADATA_LIMIT),
                    async move { core.remove_file(node, &name).await },
                    nothing,
                )
                .await?;
                Ok(Reply::empty(unique))
            }
            opcode::RMDIR => {
                let name = existing_name(fields.name()?)?;
                wait(
                    interrupt,
                    Some(METADATA_LIMIT),
                    async move { core.remove_folder(node, &name).await },
                    nothing,
                )
                .await?;
                Ok(Reply::empty(unique))
            }
            opcode::RENAME | opcode::RENAME2 => {
                let new_parent = fields.u64()?;
                let flags = if header.opcode == opcode::RENAME2 {
                    let flags = fields.u32()?;
                    fields.skip(4)?;
                    flags
                } else {
                    0
                };
                if flags & !wire::RENAME_NOREPLACE != 0 {
                    return Err(Errno(libc::EINVAL));
                }
                let name = existing_name(fields.name()?)?;
                let target = new_name(fields.name()?)?;
                let replace = flags & wire::RENAME_NOREPLACE == 0;
                wait(
                    interrupt,
                    Some(METADATA_LIMIT),
                    async move { core.rename(node, &name, new_parent, &target, replace).await },
                    nothing,
                )
                .await?;
                Ok(Reply::empty(unique))
            }
            opcode::OPEN => {
                let mode = open_mode(fields.u32()?);
                let handle = wait(
                    interrupt,
                    None,
                    {
                        let core = core.clone();
                        async move { core.open(node, mode).await }
                    },
                    move |handle| core.release(handle),
                )
                .await?;
                Ok(Reply::opened(unique, handle))
            }
            opcode::CREATE => {
                let flags = fields.u32()?;
                let mode = fields.u32()?;
                let umask = fields.u32()?;
                fields.skip(4)?;
                let name = new_name(fields.name()?)?;
                let exclusive = flags as i32 & libc::O_EXCL != 0;
                let permissions = mode & !umask & 0o7777;
                let (made, handle) = wait(
                    interrupt,
                    None,
                    {
                        let core = core.clone();
                        async move {
                            core.create(node, &name, open_mode(flags), exclusive, Some(permissions))
                                .await
                        }
                    },
                    move |(made, handle): (Attributes, u64)| {
                        core.release(handle);
                        core.forget(made.node, 1);
                    },
                )
                .await?;
                Ok(Reply::created(
                    unique,
                    &self.attr(&made),
                    KERNEL_VALID_SECONDS,
                    handle,
                ))
            }
            opcode::READ => {
                let handle = fields.u64()?;
                let offset = fields.u64()?;
                let size = fields.u32()?;
                let data = wait(
                    interrupt,
                    Some(READ_LIMIT),
                    async move { core.read(handle, offset, size).await },
                    nothing,
                )
                .await?;
                Ok(Reply::data(unique, &data))
            }
            opcode::WRITE => {
                const WRITE_FIELDS: usize = 40;
                let handle = fields.u64()?;
                let offset = fields.u64()?;
                let size = fields.u32()? as usize;
                if body.len() < WRITE_FIELDS + size {
                    return Err(Errno(libc::EINVAL));
                }
                let data = body.slice(WRITE_FIELDS..WRITE_FIELDS + size);
                let written = wait(
                    interrupt,
                    None,
                    async move { core.write(handle, offset, &data).await },
                    nothing,
                )
                .await?;
                Ok(Reply::written(unique, written))
            }
            // Closing a descriptor and syncing a file both mean: have it on the server, and
            // say so if that fails.
            opcode::FLUSH | opcode::FSYNC => {
                let handle = fields.u64()?;
                wait(
                    interrupt,
                    None,
                    async move { core.flush(handle).await },
                    nothing,
                )
                .await?;
                Ok(Reply::empty(unique))
            }
            opcode::RELEASE => {
                core.release(fields.u64()?);
                Ok(Reply::empty(unique))
            }
            opcode::OPENDIR => {
                let entries = wait(
                    interrupt,
                    Some(LISTING_LIMIT),
                    async move { core.read_folder(node).await },
                    nothing,
                )
                .await?;
                let handle = self.next_folder.fetch_add(1, Ordering::Relaxed);
                self.folders.lock().unwrap().insert(
                    handle,
                    OpenFolder {
                        entries: Arc::new(entries),
                        read: false,
                    },
                );
                Ok(Reply::opened(unique, handle))
            }
            opcode::READDIR => {
                let handle = fields.u64()?;
                let offset = fields.u64()?;
                let size = fields.u32()?;
                let entries = self.folder_entries(handle, node, offset, interrupt).await?;
                let mut reply = FolderReply::new(unique, size);
                let mut index = usize::try_from(offset).unwrap_or(usize::MAX);
                loop {
                    let (entry_node, kind, name) = match index {
                        0 => (node, wire::DIRENT_FOLDER, "."),
                        1 => (node, wire::DIRENT_FOLDER, ".."),
                        _ => match entries.get(index - 2) {
                            Some((name, attributes)) => (
                                attributes.node,
                                match attributes.kind {
                                    NodeKind::Folder => wire::DIRENT_FOLDER,
                                    NodeKind::File => wire::DIRENT_FILE,
                                },
                                name.as_str(),
                            ),
                            None => break,
                        },
                    };
                    if !reply.add(entry_node, index as u64 + 1, kind, name.as_bytes()) {
                        break;
                    }
                    index += 1;
                }
                Ok(reply.finish())
            }
            opcode::RELEASEDIR => {
                self.folders.lock().unwrap().remove(&fields.u64()?);
                Ok(Reply::empty(unique))
            }
            opcode::FSYNCDIR => Ok(Reply::empty(unique)),
            opcode::STATFS => Ok(Reply::statistics(unique)),
            opcode::SYMLINK | opcode::LINK => Err(Errno(libc::EPERM)),
            opcode::READLINK => Err(Errno(libc::EINVAL)),
            opcode::FALLOCATE => Err(Errno(libc::EOPNOTSUPP)),
            opcode::IOCTL => Err(Errno(libc::ENOTTY)),
            // The kernel takes this answer as "never ask again" and does without: access
            // checks, extended attributes, locks kept on this computer, and the rest.
            _ => Err(Errno(libc::ENOSYS)),
        }
    }

    /// The entries an open folder is read from. Reading from the start a second time lists
    /// the folder again, as `rewinddir` promises.
    async fn folder_entries(
        &self,
        handle: u64,
        node: u64,
        offset: u64,
        interrupt: &CancellationToken,
    ) -> Result<FolderEntries, Errno> {
        {
            let mut folders = self.folders.lock().unwrap();
            let folder = folders.get_mut(&handle).ok_or(Errno(libc::EBADF))?;
            if offset != 0 || !folder.read {
                folder.read = true;
                return Ok(folder.entries.clone());
            }
        }
        let core = self.core.clone();
        let entries = Arc::new(
            wait(
                interrupt,
                Some(LISTING_LIMIT),
                async move { core.read_folder(node).await },
                nothing,
            )
            .await?,
        );
        if let Some(folder) = self.folders.lock().unwrap().get_mut(&handle) {
            folder.entries = entries.clone();
        }
        Ok(entries)
    }
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Reads requests until the kernel lets go of the drive or `stop` is closed.
fn read_requests(
    session: Arc<Session>,
    mut device: File,
    stop: UnixStream,
    on_end: Box<dyn Fn() + Send>,
) {
    let mut buffer = vec![0u8; wire::READ_BUFFER_SIZE];
    let stopped = loop {
        let mut waiting = [
            libc::pollfd {
                fd: device.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: stop.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `waiting` is a valid array of two descriptors that are open for the call.
        let ready = unsafe { libc::poll(waiting.as_mut_ptr(), 2, -1) };
        if ready < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break false;
        }
        if waiting[1].revents != 0 {
            break true;
        }
        match device.read(&mut buffer) {
            Ok(0) => break false,
            Ok(length) => session.accept(&buffer[..length]),
            Err(error) => match error.raw_os_error() {
                // The request was withdrawn before it could be read, or the wait was cut
                // short.
                Some(libc::ENOENT | libc::EINTR | libc::EAGAIN) => {}
                // ENODEV: the drive was unmounted.
                _ => break false,
            },
        }
    };
    if !stopped {
        session.ended.store(true, Ordering::Relaxed);
        on_end();
    }
}

/// A drive mounted on a folder. Dropping it unmounts, by force if need be.
pub struct FuseMount {
    mount_point: PathBuf,
    /// Whether the folder was made for this drive, and goes with it.
    made_folder: bool,
    session: Arc<Session>,
    /// Closing this wakes the reading thread so it stops.
    stop: Mutex<Option<UnixStream>>,
    reader: Mutex<Option<std::thread::JoinHandle<()>>>,
    finished: AtomicBool,
}

impl FuseMount {
    /// Mounts `core` at `mount_point`. Must be called on the Tokio runtime that is to serve
    /// the drive's requests. `on_end` is called, on the reading thread, if the drive is
    /// unmounted by anything other than `unmount`.
    pub fn start(
        core: Arc<MountFs>,
        mount_point: &Path,
        label: &str,
        on_end: Box<dyn Fn() + Send>,
    ) -> AppResult<Self> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            AppError::new(
                ErrorKind::Io,
                "A drive can only be mounted from a running task",
            )
        })?;
        let (stop, stopped) = UnixStream::pair()?;
        let made_folder = device::prepare_mount_point(mount_point)?;
        let remove_folder = || {
            if made_folder {
                let _ = std::fs::remove_dir(mount_point);
            }
        };
        let device = device::mount(mount_point, label, core.is_read_only()).inspect_err(|_| {
            remove_folder();
        })?;
        let undo = |error: std::io::Error| {
            let _ = device::unmount(mount_point, true);
            remove_folder();
            AppError::from(error)
        };
        let reading = device.try_clone().map_err(undo)?;
        // SAFETY: these calls only read the identity of this process.
        let (user, group) = unsafe { (libc::getuid(), libc::getgid()) };
        let session = Arc::new(Session {
            core,
            device: RwLock::new(Some(device)),
            runtime,
            interrupts: Mutex::new(HashMap::new()),
            folders: Mutex::new(HashMap::new()),
            next_folder: AtomicU64::new(1),
            user,
            group,
            mounted_at: now_seconds(),
            ended: AtomicBool::new(false),
        });
        let reader = std::thread::Builder::new()
            .name("poros-drive".into())
            .spawn({
                let session = session.clone();
                move || read_requests(session, reading, stopped, on_end)
            })
            .map_err(undo)?;
        Ok(Self {
            mount_point: mount_point.to_path_buf(),
            made_folder,
            session,
            stop: Mutex::new(Some(stop)),
            reader: Mutex::new(Some(reader)),
            finished: AtomicBool::new(false),
        })
    }

    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// Detaches the drive. Without `force` this fails while a program has something open on
    /// it or works inside it; with `force` the drive goes at once and such programs get
    /// errors from then on. Blocks briefly, so call it off the async workers.
    pub fn unmount(&self, force: bool) -> AppResult<()> {
        if self.finished.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !self.session.ended.load(Ordering::Relaxed) {
            match device::unmount(&self.mount_point, false) {
                Ok(()) => {}
                Err(UnmountError::Busy) if !force => {
                    return Err(AppError::new(
                        ErrorKind::Io,
                        "A program is still using the drive. Close what it has open there, \
                         or unmount by force.",
                    ));
                }
                Err(UnmountError::Busy) => {
                    if let Err(UnmountError::Failed(error)) =
                        device::unmount(&self.mount_point, true)
                    {
                        log::warn!("{}", error.message);
                    }
                }
                Err(UnmountError::Failed(error)) => {
                    // Someone else detaching it first is as good.
                    let attached = device::mounted()
                        .iter()
                        .any(|entry| entry.mount_point == self.mount_point);
                    if attached && !force {
                        return Err(error);
                    }
                }
            }
        }
        self.finish();
        Ok(())
    }

    fn finish(&self) {
        if self.finished.swap(true, Ordering::Relaxed) {
            return;
        }
        drop(self.stop.lock().unwrap().take());
        if let Some(reader) = self.reader.lock().unwrap().take() {
            if reader.thread().id() != std::thread::current().id() {
                let _ = reader.join();
            }
        }
        drop(self.session.device.write().unwrap().take());
        if self.made_folder {
            let _ = std::fs::remove_dir(&self.mount_point);
        }
    }
}

impl Drop for FuseMount {
    fn drop(&mut self) {
        let _ = self.unmount(true);
    }
}
