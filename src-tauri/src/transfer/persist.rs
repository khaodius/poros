//! Unfinished transfers saved in `transfers.json`, so closing Poros, a crash or a power cut does
//! not lose the queue. They come back paused. Passwords and passphrases are never written: a
//! restored server takes them from a session the user opens to it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::conflict::ExistsAction;
use super::queue::{Direction, JobKind, JobSpec, JobState, ResumePoint, Unfinished};
use super::{Login, SessionTarget, Shared};
use crate::ssh::{AuthMethod, ConnectProfile};
use crate::storage;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedQueue {
    servers: Vec<SavedServer>,
    jobs: Vec<SavedJob>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedServer {
    host: String,
    port: u16,
    username: String,
    auth: SavedAuth,
    timeout_secs: Option<u64>,
    keepalive_secs: Option<u64>,
    compression: bool,
    receive_buffer_kib: Option<u32>,
    send_buffer_kib: Option<u32>,
    saved_connection_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum SavedAuth {
    Password,
    #[serde(rename_all = "camelCase")]
    PublicKey {
        key_path: String,
    },
    Agent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedJob {
    /// Index into `servers`.
    server: usize,
    direction: Direction,
    kind: JobKind,
    name: String,
    source: String,
    target: String,
    target_directory: String,
    size: u64,
    #[serde(default)]
    ancestors: Vec<String>,
    #[serde(default)]
    failed: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    resolution: Option<ExistsAction>,
    #[serde(default)]
    resume: Option<ResumePoint>,
}

impl From<&ConnectProfile> for SavedServer {
    fn from(profile: &ConnectProfile) -> Self {
        Self {
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            auth: match &profile.auth {
                AuthMethod::Password { .. } => SavedAuth::Password,
                AuthMethod::PublicKey { key_path, .. } => SavedAuth::PublicKey {
                    key_path: key_path.clone(),
                },
                AuthMethod::Agent => SavedAuth::Agent,
            },
            timeout_secs: profile.timeout_secs,
            keepalive_secs: profile.keepalive_secs,
            compression: profile.compression,
            receive_buffer_kib: profile.receive_buffer_kib,
            send_buffer_kib: profile.send_buffer_kib,
            saved_connection_id: profile.saved_connection_id.clone(),
        }
    }
}

impl SavedServer {
    fn profile(&self) -> ConnectProfile {
        ConnectProfile {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth: match &self.auth {
                SavedAuth::Password => AuthMethod::Password {
                    password: String::new(),
                },
                SavedAuth::PublicKey { key_path } => AuthMethod::PublicKey {
                    key_path: key_path.clone(),
                    passphrase: None,
                },
                SavedAuth::Agent => AuthMethod::Agent,
            },
            initial_path: None,
            timeout_secs: self.timeout_secs,
            keepalive_secs: self.keepalive_secs,
            compression: self.compression,
            receive_buffer_kib: self.receive_buffer_kib,
            send_buffer_kib: self.send_buffer_kib,
            saved_connection_id: self.saved_connection_id.clone(),
        }
    }
}

/// The unfinished jobs and their servers, or `None` when saving is off or there is no file.
fn snapshot(shared: &Shared) -> Option<(SavedQueue, &Path)> {
    let file = shared.queue_file.get()?;
    if !shared.settings().keep_queue {
        return Some((SavedQueue::default(), file));
    }
    let unfinished = {
        let mut queue = shared.queue.lock().unwrap();
        queue.unsaved = false;
        queue.unfinished()
    };
    let targets = shared.targets.lock().unwrap();
    let mut saved = SavedQueue::default();
    let mut server_index: HashMap<&str, usize> = HashMap::new();
    for job in unfinished {
        let Some(target) = targets.get(&job.spec.session_id) else {
            continue;
        };
        let server = *server_index
            .entry(target.session_id.as_str())
            .or_insert_with(|| {
                let server = SavedServer::from(&target.login().profile);
                match saved.servers.iter().position(|known| *known == server) {
                    Some(index) => index,
                    None => {
                        saved.servers.push(server);
                        saved.servers.len() - 1
                    }
                }
            });
        let spec = job.spec;
        saved.jobs.push(SavedJob {
            server,
            direction: spec.direction,
            kind: spec.kind,
            name: spec.name,
            source: spec.source,
            target: spec.target,
            target_directory: spec.target_directory,
            size: spec.size,
            ancestors: spec.ancestors.to_vec(),
            failed: job.state == JobState::Failed,
            error: job.error,
            resolution: job.resolution,
            resume: job.resume,
        });
    }
    Some((saved, file))
}

fn write(file: &Path, saved: &SavedQueue) {
    let result = if saved.jobs.is_empty() {
        match std::fs::remove_file(file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    } else {
        serde_json::to_string(saved)
            .map_err(|error| crate::error::AppError::invalid(error.to_string()))
            .and_then(|text| storage::write_text(file, &text))
    };
    if let Err(error) = result {
        log::warn!("Could not save the transfer queue: {error}");
    }
}

/// Saves on a blocking thread, skipping the save if one is still being written.
pub(super) fn save_in_background(shared: &Arc<Shared>) {
    if shared.saving.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some((saved, file)) = snapshot(shared) else {
        shared.saving.store(false, Ordering::Release);
        return;
    };
    let file = file.to_path_buf();
    let shared = shared.clone();
    tauri::async_runtime::spawn_blocking(move || {
        write(&file, &saved);
        shared.saving.store(false, Ordering::Release);
    });
}

pub(super) fn save(shared: &Shared) {
    if let Some((saved, file)) = snapshot(shared) {
        write(file, &saved);
    }
}

/// Puts the saved jobs back in the queue, paused; returns how many there were.
pub(super) fn restore(shared: &Shared, file: &Path) -> usize {
    let saved: SavedQueue = match std::fs::read(file) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(saved) => saved,
            Err(error) => {
                log::warn!("Ignoring the saved transfer queue: {error}");
                return 0;
            }
        },
        Err(_) => return 0,
    };
    let session_ids: Vec<String> = saved
        .servers
        .iter()
        .map(|_| format!("restored-{}", uuid::Uuid::new_v4()))
        .collect();
    let mut targets = shared.targets.lock().unwrap();
    for (server, session_id) in saved.servers.iter().zip(&session_ids) {
        let login = Login {
            profile: server.profile(),
            host_key_fingerprint: String::new(),
            browsing_session: session_id.clone(),
        };
        targets.insert(
            session_id.clone(),
            Arc::new(SessionTarget::new(session_id.clone(), login, true)),
        );
    }
    drop(targets);

    let mut queue = shared.queue.lock().unwrap();
    let mut restored = 0;
    for job in saved.jobs {
        let Some(session_id) = session_ids.get(job.server) else {
            continue;
        };
        queue.restore(Unfinished {
            spec: JobSpec {
                session_id: session_id.clone(),
                direction: job.direction,
                kind: job.kind,
                name: job.name,
                source: job.source,
                target: job.target,
                target_directory: job.target_directory,
                size: job.size,
                ancestors: job.ancestors.into(),
            },
            state: if job.failed {
                JobState::Failed
            } else {
                JobState::Paused
            },
            error: job.error,
            resolution: job.resolution,
            resume: job.resume,
        });
        restored += 1;
    }
    // Nothing changed yet; the file already holds this.
    queue.unsaved = false;
    restored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_servers_never_hold_secrets() {
        let profile = ConnectProfile {
            host: "example.com".into(),
            port: 22,
            username: "me".into(),
            auth: AuthMethod::Password {
                password: "hunter2".into(),
            },
            initial_path: Some("/home/me".into()),
            timeout_secs: Some(10),
            keepalive_secs: None,
            compression: false,
            receive_buffer_kib: None,
            send_buffer_kib: None,
            saved_connection_id: Some("saved".into()),
        };
        let saved = SavedServer::from(&profile);
        let text = serde_json::to_string(&saved).unwrap();
        assert!(!text.contains("hunter2"));
        let restored = serde_json::from_str::<SavedServer>(&text)
            .unwrap()
            .profile();
        assert_eq!(restored.host, "example.com");
        assert_eq!(restored.saved_connection_id.as_deref(), Some("saved"));
        assert!(matches!(
            restored.auth,
            AuthMethod::Password { password } if password.is_empty()
        ));

        let key = ConnectProfile {
            auth: AuthMethod::PublicKey {
                key_path: "/keys/id".into(),
                passphrase: Some("secret".into()),
            },
            ..profile
        };
        let text = serde_json::to_string(&SavedServer::from(&key)).unwrap();
        assert!(!text.contains("secret"));
        assert!(text.contains("/keys/id"));
    }
}
