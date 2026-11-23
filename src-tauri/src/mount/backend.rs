//! Where a mounted drive's requests go. The drive itself only knows `RemoteFileSystem`; a
//! backend hands it a connection for each request and decides how many may run at once, so a
//! protocol with one connection per login is not asked for more than it has.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::{AppError, AppResult};
use crate::protocol::RemoteFileSystem;

/// What a connection is wanted for. Long reads and uploads get connections of their own where
/// the protocol has them, so listing a folder never waits behind a large file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// A listing, a change, or one block of a file: over in a moment.
    Request,
    /// A file read from start to end, held while a program keeps reading.
    Stream,
    /// A whole file sent to the server.
    Upload,
}

/// A connection held for one request or stream. Dropping it lets the next one in, and gives
/// a connection taken from a pool back to it.
pub struct Lease {
    pub files: Arc<dyn RemoteFileSystem>,
    _permit: Option<OwnedSemaphorePermit>,
    _returned: Option<Box<dyn Send + Sync>>,
}

impl Lease {
    pub fn new(files: Arc<dyn RemoteFileSystem>, permit: Option<OwnedSemaphorePermit>) -> Self {
        Self {
            files,
            _permit: permit,
            _returned: None,
        }
    }

    /// Keeps `guard` until the lease ends; its `Drop` gives the connection back.
    pub fn returning(mut self, guard: impl Send + Sync + 'static) -> Self {
        self._returned = Some(Box::new(guard));
        self
    }
}

#[async_trait]
pub trait Backend: Send + Sync {
    /// A connection for `purpose`, waiting for a free one, and briefly for a dropped
    /// connection to come back. Fails rather than waiting for long.
    async fn lease(&self, purpose: Purpose) -> AppResult<Lease>;

    /// A connection for `purpose` if one is free now, for work that can do without, such as
    /// reading ahead. `None` when every connection for it is taken.
    async fn try_lease(&self, purpose: Purpose) -> Option<AppResult<Lease>> {
        Some(self.lease(purpose).await)
    }

    /// Called when a request failed because its connection was lost, so the next lease looks
    /// for a new one.
    fn connection_lost(&self) {}

    /// Closes the connections the backend opened for itself, once the drive is gone.
    async fn close(&self) {}
}

/// How many requests of each kind run at once.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub requests: usize,
    pub streams: usize,
    pub uploads: usize,
}

impl Limits {
    /// For protocols that carry any number of requests over one connection or client.
    pub const SHARED: Limits = Limits {
        requests: 12,
        streams: 4,
        uploads: 2,
    };
}

/// Permits for each kind of request.
pub struct Gate {
    requests: Arc<Semaphore>,
    streams: Arc<Semaphore>,
    uploads: Arc<Semaphore>,
}

impl Gate {
    pub fn new(limits: Limits) -> Self {
        Self {
            requests: Arc::new(Semaphore::new(limits.requests.max(1))),
            streams: Arc::new(Semaphore::new(limits.streams.max(1))),
            uploads: Arc::new(Semaphore::new(limits.uploads.max(1))),
        }
    }

    pub async fn enter(&self, purpose: Purpose) -> AppResult<OwnedSemaphorePermit> {
        self.semaphore(purpose)
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::cancelled())
    }

    /// A permit if one is free now.
    pub fn try_enter(&self, purpose: Purpose) -> Option<OwnedSemaphorePermit> {
        self.semaphore(purpose).clone().try_acquire_owned().ok()
    }

    fn semaphore(&self, purpose: Purpose) -> &Arc<Semaphore> {
        match purpose {
            Purpose::Request => &self.requests,
            Purpose::Stream => &self.streams,
            Purpose::Upload => &self.uploads,
        }
    }
}

/// One file system for everything, with a cap on what runs at once: the backend of tests
/// and of servers reached without a session.
pub struct SharedBackend {
    files: Arc<dyn RemoteFileSystem>,
    gate: Gate,
}

impl SharedBackend {
    pub fn new(files: Arc<dyn RemoteFileSystem>, limits: Limits) -> Self {
        Self {
            files,
            gate: Gate::new(limits),
        }
    }
}

#[async_trait]
impl Backend for SharedBackend {
    async fn lease(&self, purpose: Purpose) -> AppResult<Lease> {
        let permit = self.gate.enter(purpose).await?;
        Ok(Lease::new(self.files.clone(), Some(permit)))
    }

    async fn try_lease(&self, purpose: Purpose) -> Option<AppResult<Lease>> {
        let permit = self.gate.try_enter(purpose)?;
        Some(Ok(Lease::new(self.files.clone(), Some(permit))))
    }
}
