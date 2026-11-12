//! What every command works with: Poros's settings and saved connections, read but never
//! rewritten, and the one session a run opens.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::args::{GlobalOptions, ProgressMode};
use super::target;
use crate::connections::{ConnectionStore, Keychain, SecretScope, SecretStore};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::Events;
use crate::protocol::{self, RemoteFileSystem};
use crate::route::MAX_JUMP_HOSTS;
use crate::session::{Session, SessionInfo, SessionManager};
use crate::settings::Settings;
use crate::ssh::HostKeyApproval;

/// Sessions belong to a window in the app; the command line's belong to this name.
pub const OWNER: &str = "cli";
/// The app's identifier, which names its config folder.
const APP_IDENTIFIER: &str = "io.github.khaodius.poros";

pub struct Context {
    pub options: GlobalOptions,
    pub settings: Settings,
    pub store: Arc<ConnectionStore>,
    pub sessions: Arc<SessionManager>,
    pub events: Events,
    /// Cancelled on Ctrl+C.
    pub interrupt: CancellationToken,
    session: OnceCell<Connected>,
}

/// The open session and where relative paths start.
pub struct Connected {
    pub info: SessionInfo,
    pub session: Arc<Session>,
    /// The rsync options a saved connection's synchronizations start from.
    pub sync_arguments: Option<String>,
}

impl Connected {
    pub fn files(&self) -> Arc<dyn RemoteFileSystem> {
        self.session.files()
    }

    /// Absolute paths stay; `~` is the home folder; anything else starts in the connection's
    /// own folder.
    pub fn resolve(&self, path: &str) -> String {
        let path = path.trim();
        let resolved = if path == "~" || path.starts_with("~/") {
            protocol::resolve_against(&self.info.home, path)
        } else {
            protocol::resolve_against(&self.info.initial_path, path)
        };
        protocol::normalize(&resolved)
    }
}

/// Secrets for `--no-keychain`: none are found, and none are kept.
struct NoKeychain;

impl SecretStore for NoKeychain {
    fn get(&self, _account: &str) -> AppResult<Option<String>> {
        Ok(None)
    }

    fn set(&self, _account: &str, _secret: &str) -> AppResult<()> {
        Ok(())
    }

    fn delete(&self, _account: &str) -> AppResult<()> {
        Ok(())
    }
}

pub fn config_dir(options: &GlobalOptions) -> AppResult<PathBuf> {
    match &options.config_dir {
        Some(folder) => Ok(folder.clone()),
        None => dirs::config_dir()
            .map(|folder| folder.join(APP_IDENTIFIER))
            .ok_or_else(|| {
                AppError::invalid("This system has no config folder; give one with --config-dir")
            }),
    }
}

impl Context {
    pub fn new(options: GlobalOptions, interrupt: CancellationToken) -> AppResult<Self> {
        let folder = config_dir(&options)?;
        let settings = if options.default_settings {
            Settings::default()
        } else {
            Settings::read_only(&folder.join("settings.json")).unwrap_or_else(|problem| {
                log::warn!("{problem} The built-in settings are used instead.");
                Settings::default()
            })
        };
        let secrets: Box<dyn SecretStore> = if options.no_keychain {
            Box::new(NoKeychain)
        } else {
            Box::new(Keychain)
        };
        let store = Arc::new(ConnectionStore::new(
            folder.join("connections.json"),
            secrets,
        ));
        let events = Events::default();
        let sessions = Arc::new(SessionManager::new(
            folder.join("known_hosts"),
            events.clone(),
        ));
        // A cloud provider that replaces the refresh token makes the saved one useless, so the
        // new one is kept as the app keeps it.
        let keeper = store.clone();
        sessions.set_secret_sink(Arc::new(
            move |id: &str, scope: &SecretScope, secret: &str| {
                if let Err(error) = keeper.update_secret(id, scope, secret) {
                    log::warn!("Could not keep the renewed sign-in: {}", error.message);
                }
            },
        ));
        Ok(Self {
            options,
            settings,
            store,
            sessions,
            events,
            interrupt,
            session: OnceCell::new(),
        })
    }

    /// Progress goes to standard error only when asked for, or when a person watches it.
    pub fn progress(&self) -> ProgressMode {
        use std::io::IsTerminal;
        match self.options.progress {
            _ if self.options.quiet => ProgressMode::None,
            ProgressMode::Auto if std::io::stderr().is_terminal() => ProgressMode::Text,
            ProgressMode::Auto => ProgressMode::None,
            mode => mode,
        }
    }

    /// The session, opened on first use.
    pub async fn connected(&self) -> AppResult<&Connected> {
        self.session
            .get_or_try_init(|| async {
                super::logging::hold();
                let connected = self.connect().await;
                match &connected {
                    Ok(_) => super::logging::release(),
                    Err(_) => super::logging::silence(),
                }
                connected
            })
            .await
    }

    async fn connect(&self) -> AppResult<Connected> {
        let plan = {
            let options = self.options.clone();
            let settings = self.settings.clone();
            let store = self.store.clone();
            tokio::task::spawn_blocking(move || target::plan(&options, &settings, &store)).await??
        };
        let mut approval = self
            .options
            .host_key
            .as_ref()
            .map(|fingerprint| HostKeyApproval {
                fingerprint: fingerprint.trim().to_string(),
                remember: self.options.save_host_key,
            });
        let mut last_error = None;
        for profile in plan.attempts {
            let method = describe_method(&profile.auth);
            // Each jump host can stop the attempt once to have its key trusted.
            let mut tries = MAX_JUMP_HOSTS + 2;
            let result = loop {
                tries -= 1;
                match self
                    .sessions
                    .connect(profile.clone(), approval.clone(), OWNER)
                    .await
                {
                    Err(error)
                        if tries > 0
                            && error.kind == ErrorKind::HostKeyUnknown
                            && self.options.trust_new_host =>
                    {
                        let Some(offered) = &error.host_key else {
                            break Err(error);
                        };
                        if approval
                            .as_ref()
                            .is_some_and(|approved| approved.fingerprint == offered.fingerprint)
                        {
                            break Err(error);
                        }
                        log::info!(
                            "Trusting the new key of {}:{} ({} {})",
                            offered.host,
                            offered.port,
                            offered.algorithm,
                            offered.fingerprint
                        );
                        approval = Some(HostKeyApproval {
                            fingerprint: offered.fingerprint.clone(),
                            remember: self.options.save_host_key,
                        });
                    }
                    result => break result,
                }
            };
            match result {
                Ok(info) => {
                    let session = self.sessions.get(&info.id).await?;
                    let sync_arguments = plan
                        .saved
                        .as_ref()
                        .and_then(|saved| saved.sync_arguments.clone());
                    return Ok(Connected {
                        info,
                        session,
                        sync_arguments,
                    });
                }
                Err(error)
                    if matches!(
                        error.kind,
                        ErrorKind::AuthFailed | ErrorKind::PassphraseRequired
                    ) =>
                {
                    log::info!("Signing in with {method} failed: {}", error.message);
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            AppError::new(
                ErrorKind::AuthFailed,
                format!("No way to sign in to {}", plan.name),
            )
        }))
    }

    pub async fn close(&self) {
        self.sessions.disconnect_all().await;
    }
}

fn describe_method(auth: &crate::ssh::AuthMethod) -> String {
    use crate::ssh::AuthMethod;
    match auth {
        AuthMethod::Password { .. } => "a password".into(),
        AuthMethod::PublicKey { key_path, .. } => format!("the key {key_path}"),
        AuthMethod::Agent => "the SSH agent".into(),
        AuthMethod::OAuth { .. } => "the saved sign-in".into(),
    }
}
