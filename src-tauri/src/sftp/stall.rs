//! An SFTP session whose requests time out only when the connection stalls. Requests are
//! pipelined, so on a slow link one may wait behind many others before the server even sees it.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::rawsession::Limits;
use russh_sftp::client::RawSftpSession;
use russh_sftp::extensions::LimitsExtension;
use russh_sftp::protocol::{
    Attrs, Data, FileAttributes, Handle, Name, OpenFlags, Packet, Status, Version,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::Instant;

use crate::stall::Activity;

/// Gives each request of a `RawSftpSession` the same signature it has there, wrapped in
/// `StallGuardedSession::unless_stalled`.
macro_rules! unless_stalled {
    ($(fn $request:ident($($argument:ident: $kind:ty),*) -> $reply:ty;)*) => {
        $(
            pub async fn $request(&self, $($argument: $kind),*) -> Result<$reply, SftpError> {
                self.unless_stalled(self.session.$request($($argument),*)).await
            }
        )*
    };
}

/// A `RawSftpSession` whose requests fail only once the connection stalls.
pub struct StallGuardedSession {
    session: RawSftpSession,
    activity: Activity,
}

impl StallGuardedSession {
    pub fn new(session: RawSftpSession, activity: Activity) -> Self {
        Self { session, activity }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.session.set_limits(limits);
    }

    pub fn close_session(&self) -> Result<(), SftpError> {
        self.session.close_session()
    }

    async fn unless_stalled<T>(
        &self,
        request: impl Future<Output = Result<T, SftpError>>,
    ) -> Result<T, SftpError> {
        let started = Instant::now();
        tokio::select! {
            biased;
            result = request => result,
            () = self.activity.stalled(started) => Err(SftpError::Timeout),
        }
    }

    unless_stalled! {
        fn init() -> Version;
        fn limits() -> LimitsExtension;
        fn realpath(path: impl Into<String>) -> Name;
        fn stat(path: impl Into<String>) -> Attrs;
        fn lstat(path: impl Into<String>) -> Attrs;
        fn fstat(handle: impl Into<String>) -> Attrs;
        fn setstat(path: impl Into<String>, attributes: FileAttributes) -> Status;
        fn open(path: impl Into<String>, flags: OpenFlags, attributes: FileAttributes) -> Handle;
        fn close(handle: impl Into<String>) -> Status;
        fn read(handle: impl Into<String>, offset: u64, len: u32) -> Data;
        fn write(handle: impl Into<String>, offset: u64, data: Vec<u8>) -> Status;
        fn fsync(handle: impl Into<String>) -> Status;
        fn opendir(path: impl Into<String>) -> Handle;
        fn readdir(handle: impl Into<String>) -> Name;
        fn mkdir(path: impl Into<String>, attributes: FileAttributes) -> Status;
        fn rmdir(path: impl Into<String>) -> Status;
        fn remove(path: impl Into<String>) -> Status;
        fn rename(from: impl Into<String>, to: impl Into<String>) -> Status;
        fn readlink(path: impl Into<String>) -> Name;
        fn symlink(path: impl Into<String>, target: impl Into<String>) -> Status;
        fn extended(request: impl Into<String>, data: Vec<u8>) -> Packet;
    }
}

/// The SFTP channel, recording each time bytes go through it.
pub struct TrackedStream<S> {
    inner: S,
    activity: Activity,
}

impl<S> TrackedStream<S> {
    pub fn new(inner: S, activity: Activity) -> Self {
        Self { inner, activity }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for TrackedStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buffer.filled().len();
        let polled = Pin::new(&mut this.inner).poll_read(context, buffer);
        if buffer.filled().len() > before {
            this.activity.mark_progress();
        }
        polled
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for TrackedStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_write(context, data);
        if matches!(polled, Poll::Ready(Ok(written)) if written > 0) {
            this.activity.mark_progress();
        }
        polled
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(context)
    }
}
