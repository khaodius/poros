//! Single files queued by a part of the app that follows each one to its end itself, such as
//! a server file open in another program: downloaded to a working copy, then uploaded again
//! each time it is saved. Such a caller outlives the command that started it and often the
//! session too, so it keeps a route to the server and a handle on the queue of its own.

use std::sync::Arc;

use super::queue::JobSpec;
use super::{FileTransfer, JobId, JobKind, JobState, SessionTarget, Shared, TransferManager};
use crate::error::AppResult;
use crate::protocol::RemoteStat;
use crate::session::{self, RemoteTarget, Session};
use crate::ssh::{self, ConnectProfile};

/// What the queue needs to reach a session's server, kept by a caller that queues files for
/// it later, when the session may have dropped or closed.
#[derive(Clone)]
pub struct TransferRoute {
    target: Arc<SessionTarget>,
    /// For a connection of the caller's own to an SFTP server whose session is gone.
    remote: RemoteTarget,
}

impl TransferRoute {
    /// The session the route's jobs are queued for.
    pub fn session_id(&self) -> &str {
        &self.target.session_id
    }

    pub fn label(&self) -> &str {
        &self.target.label
    }

    /// The login of the server, which for a vault is the server the vault is on.
    pub fn profile(&self) -> ConnectProfile {
        self.target.login().profile
    }

    /// The folder of the unlocked vault the route leads into, if it leads into one.
    pub fn vault_root(&self) -> Option<String> {
        self.target
            .login()
            .vault
            .map(|vault| vault.root().to_string())
    }

    /// The vault the route leads into has been locked, so nothing more goes through it.
    pub fn vault_locked(&self) -> bool {
        self.target
            .login()
            .vault
            .is_some_and(|vault| vault.is_locked())
    }
}

/// How a queued file stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobProgress {
    pub state: JobState,
    pub transferred: u64,
    pub size: u64,
    pub error: Option<String>,
    /// Where the file is written, which a job told to keep both files changes.
    pub target: String,
    /// Waiting for its server to come back after the connection dropped.
    pub reconnecting: bool,
}

/// Queues single files and follows them. Unlike the manager, dropping it leaves the queue
/// running.
#[derive(Clone)]
pub struct FileQueue {
    shared: Arc<Shared>,
}

impl TransferManager {
    pub fn files(&self) -> FileQueue {
        FileQueue {
            shared: self.shared.clone(),
        }
    }
}

impl FileQueue {
    /// The route to the server of a session that is open now.
    pub async fn route_to(&self, session_id: &str) -> AppResult<TransferRoute> {
        let session = self.shared.sessions.get(session_id).await?;
        Ok(self.route_of(&session))
    }

    fn route_of(&self, session: &Session) -> TransferRoute {
        let target = self
            .shared
            .targets
            .lock()
            .unwrap()
            .entry(session.id.clone())
            .or_insert_with(|| Arc::new(SessionTarget::for_session(session)))
            .clone();
        TransferRoute {
            target,
            remote: session.target(),
        }
    }

    /// The route to the same server through a session that is open now, such as the one a tab
    /// reconnected with, so the panes showing that session hear of the files written. The
    /// route itself when its own session is open, or when none to its server is.
    pub async fn current_route(&self, route: &TransferRoute) -> TransferRoute {
        let sessions = &self.shared.sessions;
        if sessions.is_live(route.session_id()).await {
            return route.clone();
        }
        let login = route.target.login();
        let live = match &login.vault {
            Some(vault) => sessions.find_vault(&login.profile, vault.root()).await,
            None => sessions.find_live(&login.profile, None).await,
        };
        match live {
            Some(session) => self.route_of(&session),
            None => route.clone(),
        }
    }

    /// Queues one file ahead of everything else, started even while transfers wait for
    /// Start: something is waiting on it.
    pub fn enqueue(&self, route: &TransferRoute, file: FileTransfer) -> JobId {
        self.enqueue_for(&route.target, file)
    }

    fn enqueue_for(&self, target: &Arc<SessionTarget>, file: FileTransfer) -> JobId {
        let session_id = target.session_id.clone();
        let spec = JobSpec {
            session_id: session_id.clone(),
            source_session_id: None,
            direction: file.direction,
            kind: JobKind::File,
            name: file.name,
            source: file.source,
            target: file.target,
            target_directory: file.target_directory,
            size: file.size,
            ancestors: Arc::from([]),
            options: file.options,
        };
        let id = {
            // The server is kept and the job added under the one lock of the queue: an idle
            // queue forgets, under that lock, the servers no job needs. Targets leave the queue
            // with their last job, and this one may be for a session that closed since.
            let mut queue = self.shared.queue.lock().unwrap();
            self.shared
                .targets
                .lock()
                .unwrap()
                .entry(session_id)
                .or_insert_with(|| target.clone());
            let id = queue.add_resolved(spec, file.resolution);
            queue.move_to(&[id], true);
            id
        };
        self.shared.work.notify_waiters();
        id
    }

    /// `None` once the job has left the queue.
    pub fn job(&self, id: JobId) -> Option<JobProgress> {
        let queue = self.shared.queue.lock().unwrap();
        let job = queue.job(id)?;
        Some(JobProgress {
            state: job.state,
            transferred: job.transferred,
            size: job.spec.size,
            error: job.error.clone(),
            target: job.spec.target.clone(),
            reconnecting: job.reconnecting,
        })
    }

    /// Takes jobs off the queue, stopping the running ones.
    pub fn remove(&self, ids: &[JobId]) {
        super::update_queue(&self.shared, |queue| queue.remove(ids));
    }

    /// The whole queue is paused, so nothing moves until it runs again.
    pub fn is_paused(&self) -> bool {
        self.shared.queue.lock().unwrap().paused
    }

    /// What is at `path` on the route's server now; `None` when nothing is. Asked through a
    /// session open to that server, else through the files its session had, which FTP
    /// reconnects by itself and cloud storage never loses, else for SFTP over a connection
    /// opened for this one question.
    pub async fn stat(&self, route: &TransferRoute, path: &str) -> AppResult<Option<RemoteStat>> {
        let sessions = &self.shared.sessions;
        route.target.adopt_live_session(sessions).await;
        let login = route.target.login();
        if let Some(vault) = &login.vault {
            vault.check_unlocked()?;
        }
        if let Ok(session) = sessions.get(&login.browsing_session).await {
            return session.files().stat(path).await;
        }
        if let Some(files) = login.browsing_files {
            return files.stat(path).await;
        }
        let handle = route
            .remote
            .connect(&sessions.known_hosts, "editing")
            .await?;
        let stat = match session::open_sftp(&handle).await {
            Ok(fs) => {
                let stat = fs.stat(path).await;
                fs.close();
                stat
            }
            Err(error) => Err(error),
        };
        ssh::disconnect(&handle).await;
        stat
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::events::Events;
    use crate::protocol::Protocol;
    use crate::session::SessionManager;
    use crate::settings::TransferSettings;
    use crate::ssh::{AuthMethod, Route};
    use crate::transfer::{Direction, ExistsAction, Login};

    fn target(session_id: &str) -> Arc<SessionTarget> {
        let login = Login {
            profile: ConnectProfile {
                protocol: Protocol::Sftp,
                host: "files.example".into(),
                port: 22,
                username: "ada".into(),
                auth: AuthMethod::Agent,
                initial_path: None,
                timeout_secs: None,
                keepalive_secs: None,
                compression: false,
                receive_buffer_kib: None,
                send_buffer_kib: None,
                saved_connection_id: None,
                ftp_active: false,
                bypass_proxy: false,
                jump_connection_id: None,
                rsync_path: None,
                s3: Default::default(),
                route: Route::default(),
            },
            host_key_fingerprint: String::new(),
            browsing_session: session_id.into(),
            browsing_files: None,
            vault: None,
        };
        Arc::new(SessionTarget::new(session_id.into(), login, false))
    }

    /// Once the queue is idle, each tick of the progress loop forgets, with the queue locked,
    /// the servers of sessions no job involves. A file queued meanwhile, for a session that
    /// closed since and so is known only by its route, still finds its server when it runs.
    #[test]
    fn a_file_queued_while_unused_servers_are_forgotten_keeps_its_server() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sessions = Arc::new(SessionManager::new(
            temp_dir.path().join("known_hosts"),
            Events::default(),
        ));
        let manager =
            TransferManager::new(sessions, Events::default(), TransferSettings::default());
        let files = manager.files();
        let upload = FileTransfer {
            direction: Direction::Upload,
            name: "report.txt".into(),
            source: temp_dir
                .path()
                .join("report.txt")
                .to_string_lossy()
                .into_owned(),
            target: "/srv/report.txt".into(),
            target_directory: "/srv".into(),
            size: 5,
            resolution: ExistsAction::Overwrite,
            options: None,
        };

        // A tick holds the idle queue while the file is queued on another thread, which waits
        // for it to let go. Paused, so the job stays put once it is in. Should the file's
        // server be kept before the job goes in, the tick forgets it.
        let mut tick = manager.shared.queue.lock().unwrap();
        tick.paused = true;
        let queuing = {
            let files = files.clone();
            let target = target("closed");
            std::thread::spawn(move || files.enqueue_for(&target, upload))
        };
        let deadline = Instant::now() + Duration::from_millis(300);
        while manager.shared.target("closed").is_err() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        manager
            .shared
            .targets
            .lock()
            .unwrap()
            .retain(|session_id, _| tick.has_session_jobs(session_id));
        drop(tick);
        let id = queuing.join().unwrap();

        assert!(files.job(id).is_some());
        assert!(manager.shared.target("closed").is_ok());
    }
}
