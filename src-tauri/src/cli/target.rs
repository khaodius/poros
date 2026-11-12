//! Turns `--connection` and the sign-in options into the profiles to connect with: a saved
//! connection as Poros keeps it, or an address typed on the command line. Nothing here asks
//! for input; a missing secret is an error that says where to put it.

use std::io::BufRead;
use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;
use reqwest::Url;

use super::args::GlobalOptions;
use crate::cloud::s3::{Addressing, S3Options};
use crate::cloud::{CloudProvider, OAuthClient};
use crate::connections::{AuthType, ConnectionStore, SavedConnection};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol::Protocol;
use crate::route;
use crate::settings::Settings;
use crate::ssh::{AuthMethod, ConnectProfile};

pub const PASSWORD_VARIABLE: &str = "POROS_PASSWORD";
pub const PASSPHRASE_VARIABLE: &str = "POROS_PASSPHRASE";
/// Tried in this order when nothing else is given, as OpenSSH does.
const DEFAULT_KEYS: [&str; 3] = ["id_ed25519", "id_ecdsa", "id_rsa"];

/// The connection to open: what it is called, and every way of signing in to try in turn.
pub struct Plan {
    pub name: String,
    pub saved: Option<SavedConnection>,
    pub attempts: Vec<ConnectProfile>,
}

/// Where a password or passphrase comes from.
enum SecretSource {
    Variable(String),
    File(PathBuf),
    Stdin,
}

impl SecretSource {
    fn read(&self) -> AppResult<String> {
        let first_line = |text: &str| {
            text.lines()
                .next()
                .unwrap_or_default()
                .trim_end_matches('\r')
                .to_string()
        };
        match self {
            Self::Variable(name) => std::env::var(name).map_err(|_| {
                AppError::invalid(format!("The environment variable {name} is not set"))
            }),
            Self::File(path) => std::fs::read_to_string(path)
                .map(|text| first_line(&text))
                .map_err(|error| AppError::from(error).with_path(path.display().to_string())),
            Self::Stdin => {
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line)?;
                Ok(first_line(&line))
            }
        }
    }
}

fn password_source(options: &GlobalOptions) -> Option<SecretSource> {
    if let Some(name) = &options.password_env {
        Some(SecretSource::Variable(name.clone()))
    } else if let Some(path) = &options.password_file {
        Some(SecretSource::File(path.clone()))
    } else if options.password_stdin {
        Some(SecretSource::Stdin)
    } else {
        std::env::var_os(PASSWORD_VARIABLE)
            .map(|_| SecretSource::Variable(PASSWORD_VARIABLE.to_string()))
    }
}

fn passphrase_source(options: &GlobalOptions) -> Option<SecretSource> {
    if let Some(name) = &options.passphrase_env {
        Some(SecretSource::Variable(name.clone()))
    } else if let Some(path) = &options.passphrase_file {
        Some(SecretSource::File(path.clone()))
    } else {
        std::env::var_os(PASSPHRASE_VARIABLE)
            .map(|_| SecretSource::Variable(PASSPHRASE_VARIABLE.to_string()))
    }
}

/// A saved connection by id, or by name ignoring case.
pub fn find_saved(store: &ConnectionStore, wanted: &str) -> AppResult<Option<SavedConnection>> {
    let connections = store.list()?;
    if let Some(saved) = connections.iter().find(|saved| saved.id == wanted) {
        return Ok(Some(saved.clone()));
    }
    let named: Vec<&SavedConnection> = connections
        .iter()
        .filter(|saved| saved.name.eq_ignore_ascii_case(wanted.trim()))
        .collect();
    match named.as_slice() {
        [] => Ok(None),
        [saved] => Ok(Some((*saved).clone())),
        several => Err(AppError::invalid(format!(
            "{} saved connections are named {wanted}; name one by its id: {}",
            several.len(),
            several
                .iter()
                .map(|saved| saved.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// An address typed on the command line, as an unsaved connection and the password it carried.
pub fn parse_address(address: &str) -> AppResult<(SavedConnection, Option<String>)> {
    let address = address.trim();
    let with_scheme = if address.contains("://") {
        address.to_string()
    } else {
        format!("sftp://{address}")
    };
    let url = Url::parse(&with_scheme).map_err(|error| {
        AppError::invalid(format!("{address} is not a valid address ({error})"))
    })?;
    let scheme = url.scheme().to_ascii_lowercase();
    let (protocol, insecure_s3) = match scheme.as_str() {
        "sftp" | "ssh" | "scp" => (Protocol::Sftp, false),
        "ftp" => (Protocol::Ftp, false),
        "ftpes" => (Protocol::Ftps, false),
        "ftps" => (Protocol::FtpsImplicit, false),
        "dav" | "webdav" | "http" => (Protocol::WebDav, false),
        "davs" | "webdavs" | "https" => (Protocol::WebDavs, false),
        "s3" => (Protocol::S3, false),
        "s3+http" => (Protocol::S3, true),
        "b2" => (Protocol::BackblazeB2, false),
        other => {
            return Err(AppError::invalid(format!(
                "Poros does not connect with {other}://; use sftp, ftp, ftps, ftpes, dav, davs, s3 or b2"
            )))
        }
    };
    let decode = |text: &str| percent_decode_str(text).decode_utf8_lossy().into_owned();
    let mut host = url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    if let Some(service_host) = protocol.service_host() {
        host = service_host.to_string();
    }
    if host.is_empty() {
        return Err(AppError::invalid(format!("{address} names no server")));
    }
    let path = decode(url.path());
    let path = path.trim();
    let mut remote_path = (!path.is_empty() && path != "/").then(|| path.to_string());
    if protocol.is_webdav() {
        // WebDAV servers live under a path of their own, which becomes part of the address.
        if let Some(folder) = remote_path.take() {
            host = format!("{host}/{}", folder.trim_matches('/'));
        }
    }
    if insecure_s3 {
        host = format!("http://{host}");
    }
    let port = match url.port() {
        Some(port) => port,
        None if insecure_s3 => 80,
        None => protocol.default_port(),
    };
    let password = url
        .password()
        .map(decode)
        .filter(|secret| !secret.is_empty());
    let connection = SavedConnection {
        id: String::new(),
        protocol,
        name: address.to_string(),
        host,
        port,
        username: decode(url.username()),
        auth_type: AuthType::Password,
        key_path: None,
        remote_path,
        save_secret: false,
        last_used: None,
        ftp_active: false,
        bypass_proxy: false,
        jump_connection_id: None,
        rsync_path: None,
        sync_arguments: None,
        s3: S3Options::default(),
    };
    Ok((connection, password))
}

/// Blocks on the keychain and on standard input when the password is read from it.
pub fn plan(
    options: &GlobalOptions,
    settings: &Settings,
    store: &ConnectionStore,
) -> AppResult<Plan> {
    let wanted = options.connection.as_deref().map(str::trim).unwrap_or("");
    if wanted.is_empty() {
        return Err(AppError::invalid(
            "Name the server with --connection (or POROS_CONNECTION): a saved connection, user@host or an address such as sftp://user@host",
        ));
    }
    let saved = find_saved(store, wanted)?;
    let (mut connection, typed_password) = match &saved {
        Some(saved) => (saved.clone(), None),
        None => parse_address(wanted)?,
    };
    apply_overrides(&mut connection, options, store)?;

    let password = match password_source(options) {
        Some(source) => Some(source.read()?),
        None => typed_password,
    };
    let passphrase = passphrase_source(options)
        .map(|source| source.read())
        .transpose()?;
    let keychain = |connection: &SavedConnection| -> Option<String> {
        if connection.id.is_empty() || !connection.save_secret {
            return None;
        }
        match store.secret(&connection.id) {
            Ok(secret) => secret,
            Err(error) => {
                log::warn!("{}", error.message);
                None
            }
        }
    };

    let methods = if let Some(identity) = &options.identity {
        let passphrase = passphrase.or_else(|| {
            (connection.auth_type == AuthType::PublicKey
                && connection.key_path.as_deref() == Some(&*identity.to_string_lossy()))
            .then(|| keychain(&connection))
            .flatten()
        });
        vec![AuthMethod::PublicKey {
            key_path: identity.to_string_lossy().into_owned(),
            passphrase,
        }]
    } else if options.agent {
        vec![AuthMethod::Agent]
    } else if saved.is_some() && options.user.is_none() {
        vec![saved_method(
            &connection,
            password,
            passphrase,
            keychain(&connection),
            settings,
        )?]
    } else {
        typed_methods(&connection, password, passphrase)?
    };

    let attempts = methods
        .into_iter()
        .map(|auth| {
            let mut profile = profile_for(&connection, auth, options, settings);
            route::resolve(&mut profile, &settings.connection, store)?;
            Ok(profile)
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Plan {
        name: if saved.is_some() {
            connection.name.clone()
        } else {
            wanted.to_string()
        },
        saved,
        attempts,
    })
}

fn apply_overrides(
    connection: &mut SavedConnection,
    options: &GlobalOptions,
    store: &ConnectionStore,
) -> AppResult<()> {
    if let Some(user) = &options.user {
        connection.username = user.clone();
    }
    if let Some(jump) = &options.jump {
        let through = find_saved(store, jump)?.ok_or_else(|| {
            AppError::invalid(format!(
                "There is no saved connection {jump} to tunnel through"
            ))
        })?;
        connection.jump_connection_id = Some(through.id);
    }
    connection.bypass_proxy |= options.no_proxy;
    connection.ftp_active |= options.ftp_active;
    if let Some(region) = &options.s3_region {
        connection.s3.region = region.trim().to_string();
    }
    if options.s3_path_style {
        connection.s3.addressing = Addressing::Path;
    }
    Ok(())
}

/// The sign-in a saved connection was saved with, its secret from the options or the keychain.
fn saved_method(
    connection: &SavedConnection,
    password: Option<String>,
    passphrase: Option<String>,
    from_keychain: Option<String>,
    settings: &Settings,
) -> AppResult<AuthMethod> {
    Ok(match connection.auth_type {
        AuthType::Password => AuthMethod::Password {
            password: password
                .or(from_keychain)
                .ok_or_else(|| missing_password(&connection.name))?,
        },
        AuthType::PublicKey => AuthMethod::PublicKey {
            key_path: connection.key_path.clone().unwrap_or_default(),
            passphrase: passphrase.or(from_keychain),
        },
        AuthType::Agent => AuthMethod::Agent,
        AuthType::OAuth => {
            let provider = CloudProvider::for_protocol(connection.protocol)
                .ok_or_else(|| AppError::invalid("Only cloud drives sign in through a browser"))?;
            let refresh_token = from_keychain.ok_or_else(|| {
                AppError::new(
                    ErrorKind::AuthFailed,
                    format!(
                        "The sign-in of {} is not in the system keychain; open it once in Poros",
                        connection.name
                    ),
                )
            })?;
            AuthMethod::OAuth {
                grant_id: None,
                refresh_token,
                client: OAuthClient::configured(provider, &settings.cloud),
            }
        }
    })
}

/// For a typed address: the password when there is one, else what OpenSSH would try.
fn typed_methods(
    connection: &SavedConnection,
    password: Option<String>,
    passphrase: Option<String>,
) -> AppResult<Vec<AuthMethod>> {
    if let Some(password) = password {
        return Ok(vec![AuthMethod::Password { password }]);
    }
    match connection.protocol {
        Protocol::Sftp => {
            let mut methods = Vec::new();
            if agent_available() {
                methods.push(AuthMethod::Agent);
            }
            methods.extend(default_keys().into_iter().map(|key_path| {
                AuthMethod::PublicKey {
                    key_path: key_path.to_string_lossy().into_owned(),
                    passphrase: passphrase.clone(),
                }
            }));
            if methods.is_empty() {
                return Err(missing_password(&connection.name));
            }
            Ok(methods)
        }
        // Anonymous FTP, and WebDAV servers open to anyone.
        protocol if protocol.is_ftp() || protocol.is_webdav() => Ok(vec![AuthMethod::Password {
            password: String::new(),
        }]),
        protocol if protocol.signs_in_with_oauth() => Err(AppError::invalid(format!(
            "{} signs in through the browser: save the connection in Poros once, then name it with --connection",
            protocol.display_name()
        ))),
        _ => Err(missing_password(&connection.name)),
    }
}

fn profile_for(
    connection: &SavedConnection,
    auth: AuthMethod,
    options: &GlobalOptions,
    settings: &Settings,
) -> ConnectProfile {
    let connection_settings = &settings.connection;
    let mut profile = ConnectProfile {
        timeout_secs: Some(options.timeout.unwrap_or(connection_settings.timeout_secs)),
        keepalive_secs: Some(connection_settings.keepalive_secs),
        compression: connection_settings.compression,
        receive_buffer_kib: (!connection_settings.auto_tune_receive_buffer)
            .then_some(connection_settings.receive_buffer_kib),
        send_buffer_kib: (!connection_settings.auto_tune_send_buffer)
            .then_some(connection_settings.send_buffer_kib),
        ..route::profile_from_saved(connection, auth)
    };
    if connection.id.is_empty() {
        profile.saved_connection_id = None;
    }
    profile
}

fn missing_password(name: &str) -> AppError {
    AppError::new(
        ErrorKind::AuthFailed,
        format!(
            "No password for {name}: set {PASSWORD_VARIABLE}, or use --password-env, --password-file or --password-stdin"
        ),
    )
}

fn agent_available() -> bool {
    cfg!(windows) || std::env::var_os("SSH_AUTH_SOCK").is_some_and(|socket| !socket.is_empty())
}

fn default_keys() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let folder = home.join(".ssh");
    DEFAULT_KEYS
        .iter()
        .map(|name| folder.join(name))
        .filter(|path| Path::is_file(path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_addresses_of_every_protocol() {
        let (sftp, password) = parse_address("deploy@files.example.com:2222/srv/www").unwrap();
        assert_eq!(sftp.protocol, Protocol::Sftp);
        assert_eq!(
            (sftp.host.as_str(), sftp.port, sftp.username.as_str()),
            ("files.example.com", 2222, "deploy")
        );
        assert_eq!(sftp.remote_path.as_deref(), Some("/srv/www"));
        assert_eq!(password, None);

        let (ftp, password) = parse_address("ftps://me:p%40ss@[2001:db8::1]/pub").unwrap();
        assert_eq!(ftp.protocol, Protocol::FtpsImplicit);
        assert_eq!((ftp.host.as_str(), ftp.port), ("2001:db8::1", 990));
        assert_eq!(password.as_deref(), Some("p@ss"));

        let (explicit, _) = parse_address("ftpes://files.lan").unwrap();
        assert_eq!((explicit.protocol, explicit.port), (Protocol::Ftps, 21));

        let (dav, _) =
            parse_address("davs://me@cloud.example.com/remote.php/dav/files/me").unwrap();
        assert_eq!(dav.protocol, Protocol::WebDavs);
        assert_eq!(dav.host, "cloud.example.com/remote.php/dav/files/me");
        assert_eq!(dav.remote_path, None);

        let (minio, _) = parse_address("s3+http://KEY@127.0.0.1:9000/bucket/logs").unwrap();
        assert_eq!(minio.protocol, Protocol::S3);
        assert_eq!(
            (minio.host.as_str(), minio.port),
            ("http://127.0.0.1", 9000)
        );
        assert_eq!(minio.remote_path.as_deref(), Some("/bucket/logs"));

        let (b2, _) = parse_address("b2://KEYID@anything").unwrap();
        assert_eq!(b2.host, "api.backblazeb2.com");

        assert!(parse_address("smb://nas/share").is_err());
        assert!(parse_address("sftp://").is_err());
    }

    #[test]
    fn typed_addresses_sign_in_as_openssh_would() {
        let (ftp, _) = parse_address("ftp://files.lan").unwrap();
        assert!(matches!(
            typed_methods(&ftp, None, None).unwrap().as_slice(),
            [AuthMethod::Password { password }] if password.is_empty()
        ));
        let (sftp, _) = parse_address("me@host").unwrap();
        assert!(matches!(
            typed_methods(&sftp, Some("secret".into()), None)
                .unwrap()
                .as_slice(),
            [AuthMethod::Password { password }] if password == "secret"
        ));
        let (s3, _) = parse_address("s3://KEY@s3.amazonaws.com").unwrap();
        let error = typed_methods(&s3, None, None).unwrap_err();
        assert_eq!(error.kind, ErrorKind::AuthFailed);
        assert!(error.message.contains(PASSWORD_VARIABLE));
    }

    #[test]
    fn saved_connections_are_found_by_id_or_name() {
        use crate::connections::tests::{connection, MemorySecrets};
        let folder = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(
            folder.path().join("connections.json"),
            Box::<MemorySecrets>::default(),
        );
        let saved = |name: &str| {
            store
                .save(
                    SavedConnection {
                        name: name.into(),
                        ..connection()
                    },
                    None,
                )
                .unwrap()
        };
        let production = saved("Production");
        saved("Twin");
        saved("twin");
        assert_eq!(
            find_saved(&store, "production").unwrap().unwrap().id,
            production.id
        );
        assert_eq!(
            find_saved(&store, &production.id).unwrap().unwrap().name,
            "Production"
        );
        assert!(find_saved(&store, "elsewhere").unwrap().is_none());
        let error = find_saved(&store, "TWIN").unwrap_err();
        assert!(
            error.message.contains("2 saved connections"),
            "{}",
            error.message
        );
    }
}
