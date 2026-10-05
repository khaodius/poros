use std::path::PathBuf;
use std::sync::Arc;

use russh::client::KeyboardInteractiveAuthResponse;
use russh::keys::{self as russh_keys, PrivateKeyWithHashAlg};
use russh::{MethodKind, MethodSet};

use super::{keys, AuthMethod, ConnectProfile, SshHandle};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::{Events, LogLevel};

/// Servers sometimes send an empty info request before the real prompt.
const MAX_KEYBOARD_INTERACTIVE_ROUNDS: usize = 5;

pub(super) async fn authenticate(
    handle: &mut SshHandle,
    profile: &ConnectProfile,
    events: &Events,
    session_id: &str,
) -> AppResult<()> {
    let user = profile.username.trim();
    let log = |level, message: String| events.log(level, Some(session_id), message);

    let remaining = match &profile.auth {
        AuthMethod::Password { password } => {
            log(LogLevel::Info, "Trying password authentication".into());
            match handle.authenticate_password(user, password).await? {
                russh::client::AuthResult::Success => return Ok(()),
                russh::client::AuthResult::Failure {
                    remaining_methods, ..
                } => {
                    if remaining_methods.contains(&MethodKind::KeyboardInteractive) {
                        log(
                            LogLevel::Info,
                            "Trying keyboard-interactive authentication".into(),
                        );
                        if keyboard_interactive(handle, user, password).await? {
                            return Ok(());
                        }
                    }
                    remaining_methods
                }
            }
        }
        AuthMethod::PublicKey {
            key_path,
            passphrase,
        } => {
            let path = expand_home(key_path);
            log(
                LogLevel::Info,
                format!("Trying public key {}", path.display()),
            );
            let key = keys::load_private_key(&path, passphrase.as_deref())?;
            let hash = if key.algorithm().is_rsa() {
                handle.best_supported_rsa_hash().await?.flatten()
            } else {
                None
            };
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
            match handle.authenticate_publickey(user, key).await? {
                russh::client::AuthResult::Success => return Ok(()),
                russh::client::AuthResult::Failure {
                    remaining_methods, ..
                } => remaining_methods,
            }
        }
        AuthMethod::Agent => match agent_auth(handle, user, &log).await? {
            None => return Ok(()),
            Some(remaining) => remaining,
        },
    };

    Err(AppError::new(
        ErrorKind::AuthFailed,
        format!(
            "Authentication failed. The server accepts: {}",
            describe_methods(&remaining)
        ),
    ))
}

/// Answers every hidden prompt with the password; echoed prompts get an empty answer.
async fn keyboard_interactive(
    handle: &mut SshHandle,
    user: &str,
    password: &str,
) -> AppResult<bool> {
    let mut response = handle
        .authenticate_keyboard_interactive_start(user, None::<String>)
        .await?;
    for _ in 0..MAX_KEYBOARD_INTERACTIVE_ROUNDS {
        match response {
            KeyboardInteractiveAuthResponse::Success => return Ok(true),
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = prompts
                    .iter()
                    .map(|prompt| {
                        if prompt.echo {
                            String::new()
                        } else {
                            password.to_string()
                        }
                    })
                    .collect();
                response = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await?;
            }
        }
    }
    Ok(false)
}

/// `Ok(None)` on success, otherwise the methods the server still accepts.
async fn agent_auth(
    handle: &mut SshHandle,
    user: &str,
    log: &impl Fn(LogLevel, String),
) -> AppResult<Option<MethodSet>> {
    let mut agent = connect_agent().await?;
    let identities = agent.request_identities().await.map_err(|error| {
        AppError::new(ErrorKind::AuthFailed, format!("SSH agent error: {error}"))
    })?;
    if identities.is_empty() {
        return Err(AppError::new(
            ErrorKind::AuthFailed,
            "The SSH agent has no keys loaded",
        ));
    }

    let mut remaining = MethodSet::empty();
    for identity in identities {
        let public = identity.public_key().into_owned();
        log(
            LogLevel::Info,
            format!(
                "Trying agent key {}",
                public.fingerprint(russh_keys::HashAlg::Sha256)
            ),
        );
        let hash = if public.algorithm().is_rsa() {
            handle.best_supported_rsa_hash().await?.flatten()
        } else {
            None
        };
        let result = handle
            .authenticate_publickey_with(user, public, hash, &mut agent)
            .await
            .map_err(|error| {
                AppError::new(ErrorKind::AuthFailed, format!("SSH agent error: {error}"))
            })?;
        match result {
            russh::client::AuthResult::Success => return Ok(None),
            russh::client::AuthResult::Failure {
                remaining_methods, ..
            } => remaining = remaining_methods,
        }
    }
    Ok(Some(remaining))
}

type DynAgent = russh_keys::agent::client::AgentClient<
    Box<dyn russh_keys::agent::client::AgentStream + Send + Unpin + 'static>,
>;

#[cfg(unix)]
async fn connect_agent() -> AppResult<DynAgent> {
    russh_keys::agent::client::AgentClient::connect_env()
        .await
        .map(|agent| agent.dynamic())
        .map_err(|error| {
            AppError::new(
                ErrorKind::AuthFailed,
                format!("No SSH agent available (is SSH_AUTH_SOCK set?): {error}"),
            )
        })
}

/// Windows: the OpenSSH agent service pipe first, then Pageant.
#[cfg(windows)]
async fn connect_agent() -> AppResult<DynAgent> {
    use russh_keys::agent::client::AgentClient;
    if let Ok(agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        return Ok(agent.dynamic());
    }
    AgentClient::connect_pageant()
        .await
        .map(|agent| agent.dynamic())
        .map_err(|error| {
            AppError::new(
                ErrorKind::AuthFailed,
                format!("No SSH agent available (OpenSSH agent service or Pageant): {error}"),
            )
        })
}

fn describe_methods(methods: &MethodSet) -> String {
    let names: Vec<&str> = methods
        .iter()
        .map(|method| match method {
            MethodKind::None => "none",
            MethodKind::Password => "password",
            MethodKind::PublicKey => "public key",
            MethodKind::HostBased => "host-based",
            MethodKind::KeyboardInteractive => "keyboard-interactive",
            _ => "other",
        })
        .collect();
    if names.is_empty() {
        "no further methods".to_string()
    } else {
        names.join(", ")
    }
}

fn expand_home(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}
