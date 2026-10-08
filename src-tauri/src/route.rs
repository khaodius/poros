//! Works out how to reach an SSH server: through the proxy from the connection settings, and
//! through the saved connections it names as jump hosts. Secrets come from the keychain here,
//! so they never pass through the frontend. Other protocols connect directly.

use std::collections::HashSet;

use crate::connections::{AuthType, ConnectionStore, SavedConnection};
use crate::error::{AppError, AppResult};
use crate::protocol::Protocol;
use crate::settings::ConnectionSettings;
use crate::ssh::proxy::Proxy;
use crate::ssh::{AuthMethod, ConnectProfile, Route};

/// Longer chains are almost certainly a mistake, and each hop adds a handshake.
pub const MAX_JUMP_HOSTS: usize = 8;

/// Fills in `profile.route`. Blocks on the keychain.
pub fn resolve(
    profile: &mut ConnectProfile,
    settings: &ConnectionSettings,
    store: &ConnectionStore,
) -> AppResult<()> {
    if profile.protocol != Protocol::Sftp {
        profile.route = Route::default();
        return Ok(());
    }
    let mut jump_hosts = Vec::new();
    if let Some(first) = profile
        .jump_connection_id
        .clone()
        .filter(|id| !id.is_empty())
    {
        let connections = store.list()?;
        let mut visited: HashSet<String> = profile.saved_connection_id.iter().cloned().collect();
        let mut next = Some(first);
        while let Some(id) = next {
            if !visited.insert(id.clone()) {
                return Err(AppError::invalid(
                    "The jump hosts of this connection lead back to a connection already on the way",
                ));
            }
            if jump_hosts.len() == MAX_JUMP_HOSTS {
                return Err(AppError::invalid(format!(
                    "A connection can go through at most {MAX_JUMP_HOSTS} jump hosts"
                )));
            }
            let saved = connections
                .iter()
                .find(|connection| connection.id == id)
                .ok_or_else(|| {
                    AppError::invalid(
                        "The saved connection used as a jump host no longer exists; choose another in the connection's settings",
                    )
                })?;
            jump_hosts.push(jump_profile(saved, store.secret(&saved.id)?, profile)?);
            next = saved.jump_connection_id.clone().filter(|id| !id.is_empty());
        }
        jump_hosts.reverse();
    }

    let first_hop_bypasses = jump_hosts
        .first()
        .map_or(profile.bypass_proxy, |hop| hop.bypass_proxy);
    let proxy = if settings.proxy.is_enabled() && !first_hop_bypasses {
        let password = if settings.proxy.has_password {
            store.proxy_password()?.unwrap_or_default()
        } else {
            String::new()
        };
        Some(Proxy {
            kind: settings.proxy.kind,
            host: settings.proxy.host.clone(),
            port: settings.proxy.port,
            username: settings.proxy.username.clone(),
            password,
            remote_dns: settings.proxy.remote_dns,
        })
    } else {
        None
    };
    profile.route = Route { proxy, jump_hosts };
    Ok(())
}

/// A saved connection as a jump host for `through`, with its timeouts and buffers.
fn jump_profile(
    saved: &SavedConnection,
    secret: Option<String>,
    through: &ConnectProfile,
) -> AppResult<ConnectProfile> {
    require_sftp(saved, "be a jump host")?;
    let auth = auth_for(saved, secret, |name| {
        format!("To use {name} as a jump host, save its password in the system keychain")
    })?;
    Ok(ConnectProfile {
        timeout_secs: through.timeout_secs,
        keepalive_secs: through.keepalive_secs,
        compression: through.compression,
        receive_buffer_kib: through.receive_buffer_kib,
        send_buffer_kib: through.send_buffer_kib,
        initial_path: None,
        jump_connection_id: None,
        ..profile_from_saved(saved, auth)
    })
}

fn auth_for(
    saved: &SavedConnection,
    secret: Option<String>,
    password_missing: impl FnOnce(&str) -> String,
) -> AppResult<AuthMethod> {
    Ok(match saved.auth_type {
        AuthType::Password => AuthMethod::Password {
            password: secret.ok_or_else(|| AppError::invalid(password_missing(&saved.name)))?,
        },
        AuthType::PublicKey => AuthMethod::PublicKey {
            key_path: saved.key_path.clone().unwrap_or_default(),
            passphrase: secret,
        },
        AuthType::Agent => AuthMethod::Agent,
        AuthType::OAuth => {
            return Err(AppError::invalid(format!(
                "{} signs in through the browser, which needs a window",
                saved.name
            )))
        }
    })
}

/// Jump hosts and scheduled tasks work over SSH.
fn require_sftp(saved: &SavedConnection, purpose: &str) -> AppResult<()> {
    if saved.protocol == Protocol::Sftp {
        return Ok(());
    }
    Err(AppError::invalid(format!(
        "{} uses {}, so it cannot {purpose}; choose an SFTP connection",
        saved.name,
        saved.protocol.display_name()
    )))
}

/// A saved connection's profile, before connection options and the route are added.
fn profile_from_saved(saved: &SavedConnection, auth: AuthMethod) -> ConnectProfile {
    ConnectProfile {
        protocol: saved.protocol,
        host: saved.host.clone(),
        port: saved.port,
        username: saved.username.clone(),
        auth,
        initial_path: saved.remote_path.clone(),
        timeout_secs: None,
        keepalive_secs: None,
        compression: false,
        receive_buffer_kib: None,
        send_buffer_kib: None,
        saved_connection_id: Some(saved.id.clone()),
        ftp_active: saved.ftp_active,
        bypass_proxy: saved.bypass_proxy,
        jump_connection_id: saved.jump_connection_id.clone(),
        route: Route::default(),
    }
}

/// A saved connection's profile with the connection settings and its route, for connecting
/// without a window, as scheduled tasks do. Blocks on the keychain.
pub fn saved_profile(
    store: &ConnectionStore,
    settings: &ConnectionSettings,
    id: &str,
) -> AppResult<ConnectProfile> {
    let saved = store
        .list()?
        .into_iter()
        .find(|connection| connection.id == id)
        .ok_or_else(|| AppError::invalid("The saved connection no longer exists"))?;
    require_sftp(&saved, "run scheduled tasks")?;
    let auth = auth_for(&saved, store.secret(id)?, |name| {
        format!(
            "Save the password of {name} in the system keychain so Poros can connect on its own"
        )
    })?;
    let mut profile = ConnectProfile {
        timeout_secs: Some(settings.timeout_secs),
        keepalive_secs: Some(settings.keepalive_secs),
        compression: settings.compression,
        receive_buffer_kib: (!settings.auto_tune_receive_buffer)
            .then_some(settings.receive_buffer_kib),
        send_buffer_kib: (!settings.auto_tune_send_buffer).then_some(settings.send_buffer_kib),
        ..profile_from_saved(&saved, auth)
    };
    resolve(&mut profile, settings, store)?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::tests::{connection, MemorySecrets};
    use crate::settings::ProxySettings;
    use crate::ssh::proxy::ProxyKind;

    fn store() -> (tempfile::TempDir, ConnectionStore) {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            temp_dir.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        (temp_dir, store)
    }

    fn save(
        store: &ConnectionStore,
        host: &str,
        auth_type: AuthType,
        jump: Option<&str>,
        secret: Option<&str>,
    ) -> SavedConnection {
        store
            .save(
                SavedConnection {
                    name: host.into(),
                    host: host.into(),
                    auth_type,
                    key_path: (auth_type == AuthType::PublicKey).then(|| "/keys/id".into()),
                    save_secret: secret.is_some(),
                    jump_connection_id: jump.map(str::to_string),
                    ..connection()
                },
                secret.map(str::to_string),
            )
            .unwrap()
    }

    fn proxy_settings() -> ConnectionSettings {
        ConnectionSettings {
            proxy: ProxySettings {
                kind: ProxyKind::Socks5,
                host: "proxy.lan".into(),
                port: 1080,
                username: "me".into(),
                has_password: true,
                remote_dns: true,
            },
            ..ConnectionSettings::default()
        }
    }

    #[test]
    fn jump_chain_is_outermost_first_with_secrets_and_the_proxy_on_the_first_hop() {
        let (_dir, store) = store();
        store.set_proxy_password(Some("proxy-pw")).unwrap();
        let outer = save(&store, "outer", AuthType::Password, None, Some("outer-pw"));
        let inner = save(&store, "inner", AuthType::PublicKey, Some(&outer.id), None);
        let target = save(&store, "target", AuthType::Agent, Some(&inner.id), None);

        let mut profile = saved_profile(&store, &proxy_settings(), &target.id).unwrap();
        let hops: Vec<&str> = profile
            .route
            .jump_hosts
            .iter()
            .map(|hop| hop.host.as_str())
            .collect();
        assert_eq!(hops, ["outer", "inner"]);
        assert!(matches!(
            &profile.route.jump_hosts[0].auth,
            AuthMethod::Password { password } if password == "outer-pw"
        ));
        assert_eq!(profile.route.jump_hosts[0].timeout_secs, Some(20));
        let proxy = profile.route.proxy.as_ref().unwrap();
        assert_eq!(proxy.password, "proxy-pw");
        assert_eq!(proxy.host, "proxy.lan");

        // The first hop decides whether the proxy is used.
        let direct_outer = SavedConnection {
            bypass_proxy: true,
            ..outer
        };
        store.save(direct_outer, None).unwrap();
        resolve(&mut profile, &proxy_settings(), &store).unwrap();
        assert!(profile.route.proxy.is_none());
    }

    #[test]
    fn loops_missing_hosts_and_unsaved_passwords_are_refused() {
        let (_dir, store) = store();
        let first = save(&store, "first", AuthType::Agent, None, None);
        let second = save(&store, "second", AuthType::Agent, Some(&first.id), None);
        store
            .save(
                SavedConnection {
                    jump_connection_id: Some(second.id.clone()),
                    ..first.clone()
                },
                None,
            )
            .unwrap();
        let error = saved_profile(&store, &ConnectionSettings::default(), &second.id).unwrap_err();
        assert!(error.message.contains("lead back"));

        let orphan = save(&store, "orphan", AuthType::Agent, Some("gone"), None);
        let error = saved_profile(&store, &ConnectionSettings::default(), &orphan.id).unwrap_err();
        assert!(error.message.contains("no longer exists"));

        let no_password = save(&store, "bastion", AuthType::Password, None, None);
        let behind = save(
            &store,
            "behind",
            AuthType::Agent,
            Some(&no_password.id),
            None,
        );
        let error = saved_profile(&store, &ConnectionSettings::default(), &behind.id).unwrap_err();
        assert!(error.message.contains("save its password"));
    }

    #[test]
    fn quick_connections_use_the_proxy_unless_told_not_to() {
        let (_dir, store) = store();
        let mut profile = ConnectProfile {
            host: "example.com".into(),
            username: "me".into(),
            saved_connection_id: None,
            jump_connection_id: None,
            ..profile_from_saved(&connection(), AuthMethod::Agent)
        };
        resolve(&mut profile, &ConnectionSettings::default(), &store).unwrap();
        assert!(profile.route.proxy.is_none());
        assert!(profile.route.jump_hosts.is_empty());

        resolve(&mut profile, &proxy_settings(), &store).unwrap();
        assert_eq!(profile.route.proxy.as_ref().unwrap().password, "");

        profile.bypass_proxy = true;
        resolve(&mut profile, &proxy_settings(), &store).unwrap();
        assert!(profile.route.proxy.is_none());
    }

    #[test]
    fn only_ssh_connections_use_the_proxy_and_jump_hosts() {
        let (_dir, store) = store();
        let ftp = store
            .save(
                SavedConnection {
                    protocol: Protocol::Ftp,
                    name: "files".into(),
                    port: 21,
                    username: String::new(),
                    auth_type: AuthType::Password,
                    save_secret: false,
                    ..connection()
                },
                None,
            )
            .unwrap();
        let mut profile = ConnectProfile {
            jump_connection_id: Some(ftp.id.clone()),
            ..profile_from_saved(&ftp, AuthMethod::Agent)
        };
        resolve(&mut profile, &proxy_settings(), &store).unwrap();
        assert!(profile.route.proxy.is_none());
        assert!(profile.route.jump_hosts.is_empty());
        let error = saved_profile(&store, &proxy_settings(), &ftp.id).unwrap_err();
        assert!(error.message.contains("cannot run scheduled tasks"));

        let behind_ftp = save(&store, "behind", AuthType::Agent, Some(&ftp.id), None);
        let error =
            saved_profile(&store, &ConnectionSettings::default(), &behind_ftp.id).unwrap_err();
        assert!(error.message.contains("cannot be a jump host"));
    }
}
