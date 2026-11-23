//! Connections for a drive showing a session's server, or an unlocked vault on one.
//!
//! - SFTP: listings and changes go over the session's own channel, which carries any number
//!   of requests at once; long reads and uploads each get a channel of their own on the
//!   session's connection, so a large file never holds up a listing.
//! - FTP: one control connection carries one request at a time, so the drive opens its own
//!   connections, as transfer workers do, and leaves the session's to the pane. A server that
//!   refuses more connections is shared with the pane instead.
//! - HTTP services (WebDAV, S3 and the cloud drives) take any number of requests over the
//!   session's client.
//!
//! When the session's connection drops, requests wait a moment for it to come back (the
//! session reconnects on its own and the drive follows it to the new session), then fail,
//! and fail quickly from then on until it is back.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::OwnedSemaphorePermit;

use super::backend::{Backend, Gate, Lease, Limits, Purpose};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::Events;
use crate::ftp::FtpFs;
use crate::protocol::RemoteFileSystem;
use crate::session::{Session, SessionManager};
use crate::ssh::HostKeyApproval;

/// How long a request waits for a dropped connection to come back before it fails.
const RECONNECT_WAIT: Duration = Duration::from_secs(15);
/// How long it waits once a request has waited the whole time in vain: programs listing a
/// folder of a server that is away then get their errors in good time.
const STILL_AWAY_WAIT: Duration = Duration::from_millis(500);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carrier {
    /// One client takes every request.
    Shared,
    /// Channels of their own for streams and uploads.
    Sftp,
    /// Connections of their own for everything.
    Ftp,
}

impl Carrier {
    fn limits(self) -> Limits {
        match self {
            Carrier::Shared => Limits::SHARED,
            Carrier::Sftp => Limits {
                requests: 12,
                streams: 4,
                uploads: 2,
            },
            // Each is a login of its own on the server, which may allow few.
            Carrier::Ftp => Limits {
                requests: 1,
                streams: 2,
                uploads: 1,
            },
        }
    }

    fn opens_own(self, purpose: Purpose) -> bool {
        match self {
            Carrier::Shared => false,
            Carrier::Sftp => purpose != Purpose::Request,
            Carrier::Ftp => true,
        }
    }
}

/// A connection the drive opened for itself.
struct Own {
    /// What requests go through: the connection, or the vault seen through it.
    files: Arc<dyn RemoteFileSystem>,
    /// The connection itself, closed once it is no longer wanted.
    connection: Arc<dyn RemoteFileSystem>,
    generation: u64,
}

#[derive(Default)]
struct Pool {
    idle: Vec<Own>,
    /// Raised when the connections may be dead or belong to a session that is gone; older
    /// ones are closed rather than used again.
    generation: u64,
    closed: bool,
    /// The server refused another connection: the session's own is shared from then on.
    refused: bool,
}

/// Gives a leased connection back to its pool, or closes it if the pool has moved on.
struct ReturnToPool {
    pool: Weak<Mutex<Pool>>,
    own: Option<Own>,
}

impl Drop for ReturnToPool {
    fn drop(&mut self) {
        let Some(own) = self.own.take() else {
            return;
        };
        if let Some(pool) = self.pool.upgrade() {
            let mut pool = pool.lock().unwrap();
            if !pool.closed && own.generation == pool.generation {
                pool.idle.push(own);
                return;
            }
        }
        close_in_background(vec![own]);
    }
}

fn close_in_background(connections: Vec<Own>) {
    if connections.is_empty() {
        return;
    }
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            for own in connections {
                own.connection.close().await;
            }
        });
    }
}

pub struct SessionBackend {
    sessions: Arc<SessionManager>,
    session_id: Mutex<String>,
    label: String,
    carrier: Carrier,
    gate: Gate,
    pool: Arc<Mutex<Pool>>,
    /// A request waited the whole time for the session in vain, and none has reached it since.
    waited_out: AtomicBool,
}

impl SessionBackend {
    pub fn new(sessions: Arc<SessionManager>, session: &Session) -> Arc<Self> {
        let transport = session.profile.protocol;
        let carrier = if transport.is_ftp() {
            Carrier::Ftp
        } else if transport == crate::protocol::Protocol::Sftp {
            Carrier::Sftp
        } else {
            Carrier::Shared
        };
        Arc::new(Self {
            sessions,
            session_id: Mutex::new(session.id.clone()),
            label: session.target().label,
            carrier,
            gate: Gate::new(carrier.limits()),
            pool: Arc::new(Mutex::new(Pool::default())),
            waited_out: AtomicBool::new(false),
        })
    }

    pub fn session_id(&self) -> String {
        self.session_id.lock().unwrap().clone()
    }

    /// Moves the drive onto the session that replaced its own after a reconnect.
    pub fn follow(&self, session_id: &str) {
        *self.session_id.lock().unwrap() = session_id.to_string();
        self.retire_connections();
    }

    fn retire_connections(&self) {
        let retired = {
            let mut pool = self.pool.lock().unwrap();
            pool.generation += 1;
            std::mem::take(&mut pool.idle)
        };
        close_in_background(retired);
    }

    /// The session, waiting a moment for it if its connection dropped.
    async fn live_session(&self) -> AppResult<Arc<Session>> {
        let started = Instant::now();
        let patience = if self.waited_out.load(Ordering::Relaxed) {
            STILL_AWAY_WAIT
        } else {
            RECONNECT_WAIT
        };
        loop {
            let session_id = self.session_id();
            let found = self.sessions.get(&session_id).await;
            let error = match found {
                Ok(session) => {
                    if let Some(vault) = session.vault() {
                        vault.check_unlocked()?;
                    }
                    self.waited_out.store(false, Ordering::Relaxed);
                    return Ok(session);
                }
                Err(error) => error,
            };
            let waits = matches!(
                error.kind,
                ErrorKind::Disconnected | ErrorKind::SessionNotFound
            );
            if waits && started.elapsed() < patience {
                tokio::time::sleep(POLL_INTERVAL).await;
                continue;
            }
            if waits {
                self.waited_out.store(true, Ordering::Relaxed);
                return Err(AppError::new(
                    ErrorKind::Disconnected,
                    format!("{} is out of reach", self.label),
                ));
            }
            return Err(error);
        }
    }

    async fn lease_with(&self, purpose: Purpose, permit: OwnedSemaphorePermit) -> AppResult<Lease> {
        let session = self.live_session().await?;
        if self.carrier.opens_own(purpose) {
            if let Some(own) = self.own_connection(&session).await {
                let files = own.files.clone();
                let returned = ReturnToPool {
                    pool: Arc::downgrade(&self.pool),
                    own: Some(own),
                };
                return Ok(Lease::new(files, Some(permit)).returning(returned));
            }
        }
        Ok(Lease::new(session.files(), Some(permit)))
    }

    /// An idle connection of the drive's own, or a new one. `None` when there is none to be
    /// had, and the session's own is to be used.
    async fn own_connection(&self, session: &Arc<Session>) -> Option<Own> {
        let generation = {
            let mut pool = self.pool.lock().unwrap();
            if pool.closed || pool.refused {
                return None;
            }
            if let Some(own) = pool.idle.pop() {
                return Some(own);
            }
            pool.generation
        };
        let opened = match self.carrier {
            Carrier::Sftp => self.open_channel(session).await,
            Carrier::Ftp => self.open_ftp(session).await,
            Carrier::Shared => return None,
        };
        match opened {
            Ok(connection) => {
                let files = match session.vault() {
                    Some(vault) => vault.through(connection.clone()) as Arc<dyn RemoteFileSystem>,
                    None => connection.clone(),
                };
                Some(Own {
                    files,
                    connection,
                    generation,
                })
            }
            Err(error) if error.is_connection_lost() => None,
            Err(error) => {
                let first = !std::mem::replace(&mut self.pool.lock().unwrap().refused, true);
                if first {
                    log::warn!(
                        "The server of {} did not accept another connection for its drive \
                         ({}); the drive shares the session's",
                        self.label,
                        error.message
                    );
                }
                None
            }
        }
    }

    async fn open_channel(&self, session: &Arc<Session>) -> AppResult<Arc<dyn RemoteFileSystem>> {
        // A vault's session has no connection of its own: the channel opens on its server's.
        let carrier = match session.server_session() {
            Some(server_session) => self.sessions.get(server_session).await?,
            None => session.clone(),
        };
        Ok(Arc::new(carrier.open_channel().await?))
    }

    async fn open_ftp(&self, session: &Arc<Session>) -> AppResult<Arc<dyn RemoteFileSystem>> {
        let approval = (!session.host_key_fingerprint.is_empty()).then(|| HostKeyApproval {
            fingerprint: session.host_key_fingerprint.clone(),
            remember: false,
        });
        let (files, _) = FtpFs::connect(
            &format!("{}#drive", session.id),
            &session.profile,
            &self.sessions.certificates,
            approval,
            // The drive's connections log nothing of their own; failures reach the program.
            &Events::default(),
        )
        .await?;
        Ok(files)
    }
}

#[async_trait]
impl Backend for SessionBackend {
    async fn lease(&self, purpose: Purpose) -> AppResult<Lease> {
        let permit = self.gate.enter(purpose).await?;
        self.lease_with(purpose, permit).await
    }

    async fn try_lease(&self, purpose: Purpose) -> Option<AppResult<Lease>> {
        let permit = self.gate.try_enter(purpose)?;
        Some(self.lease_with(purpose, permit).await)
    }

    fn connection_lost(&self) {
        self.retire_connections();
    }

    async fn close(&self) {
        let idle = {
            let mut pool = self.pool.lock().unwrap();
            pool.closed = true;
            std::mem::take(&mut pool.idle)
        };
        for own in idle {
            own.connection.close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftp_drives_ask_for_few_connections_and_sftp_shares_its_requests() {
        let ftp = Carrier::Ftp.limits();
        assert_eq!(ftp.requests + ftp.streams + ftp.uploads, 4);
        assert!(Carrier::Ftp.opens_own(Purpose::Request));
        assert!(!Carrier::Sftp.opens_own(Purpose::Request));
        assert!(Carrier::Sftp.opens_own(Purpose::Stream));
        assert!(Carrier::Sftp.opens_own(Purpose::Upload));
        assert!(!Carrier::Shared.opens_own(Purpose::Upload));
    }
}
