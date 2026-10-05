//! The single error type returned by every Tauri command.
//!
//! Serialized as `{ kind, message, hostKey?, path? }` so the frontend can branch on
//! `kind` (e.g. prompt for a host key or a key passphrase) and show `message` otherwise.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    /// Server presented a host key that is not in any known_hosts file.
    HostKeyUnknown,
    /// Server presented a different key than the one on record. Possible MITM.
    HostKeyChanged,
    AuthFailed,
    /// The private key file is encrypted and no (or a wrong) passphrase was given.
    PassphraseRequired,
    Connection,
    Timeout,
    SessionNotFound,
    Disconnected,
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    Io,
    Sftp,
    Ssh,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyInfo {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    /// `SHA256:<base64>` as printed by `ssh-keygen -l`.
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_key: Option<HostKeyInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            host_key: None,
            path: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn session_not_found() -> Self {
        Self::new(ErrorKind::SessionNotFound, "Session is no longer open")
    }

    pub fn host_key(kind: ErrorKind, info: HostKeyInfo) -> Self {
        let message = match kind {
            ErrorKind::HostKeyChanged => format!(
                "The host key for {}:{} has changed ({} {}). This can mean someone is intercepting the connection.",
                info.host, info.port, info.algorithm, info.fingerprint
            ),
            _ => format!(
                "The authenticity of {}:{} can't be established ({} {}).",
                info.host, info.port, info.algorithm, info.fingerprint
            ),
        };
        Self {
            kind,
            message,
            host_key: Some(info),
            path: None,
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind as Io;
        let kind = match e.kind() {
            Io::NotFound => ErrorKind::NotFound,
            Io::PermissionDenied => ErrorKind::PermissionDenied,
            Io::AlreadyExists => ErrorKind::AlreadyExists,
            Io::TimedOut => ErrorKind::Timeout,
            Io::InvalidInput => ErrorKind::InvalidInput,
            _ => ErrorKind::Io,
        };
        Self::new(kind, e.to_string())
    }
}

impl From<russh::Error> for AppError {
    fn from(e: russh::Error) -> Self {
        let kind = match &e {
            russh::Error::ConnectionTimeout | russh::Error::InactivityTimeout => ErrorKind::Timeout,
            russh::Error::Disconnect
            | russh::Error::HUP
            | russh::Error::SendError
            | russh::Error::RecvError
            | russh::Error::KeepaliveTimeout => ErrorKind::Disconnected,
            russh::Error::IO(_) => ErrorKind::Connection,
            _ => ErrorKind::Ssh,
        };
        Self::new(kind, e.to_string())
    }
}

impl From<russh::keys::Error> for AppError {
    fn from(e: russh::keys::Error) -> Self {
        match e {
            russh::keys::Error::KeyIsEncrypted => Self::new(
                ErrorKind::PassphraseRequired,
                "The private key is encrypted; enter its passphrase",
            ),
            russh::keys::Error::IO(io) => io.into(),
            other => Self::new(ErrorKind::AuthFailed, format!("Could not load key: {other}")),
        }
    }
}

impl From<russh_sftp::client::error::Error> for AppError {
    fn from(e: russh_sftp::client::error::Error) -> Self {
        use russh_sftp::client::error::Error as E;
        use russh_sftp::protocol::StatusCode;
        match e {
            E::Status(status) => {
                let kind = match status.status_code {
                    StatusCode::NoSuchFile => ErrorKind::NotFound,
                    StatusCode::PermissionDenied => ErrorKind::PermissionDenied,
                    StatusCode::NoConnection | StatusCode::ConnectionLost => {
                        ErrorKind::Disconnected
                    }
                    _ => ErrorKind::Sftp,
                };
                let message = if status.error_message.trim().is_empty() {
                    status.status_code.to_string()
                } else {
                    status.error_message
                };
                Self::new(kind, message)
            }
            E::Timeout => Self::new(ErrorKind::Timeout, "The server did not respond in time"),
            E::UnexpectedBehavior(msg) if msg.contains("session closed") => {
                Self::new(ErrorKind::Disconnected, "The connection was closed")
            }
            other => Self::new(ErrorKind::Sftp, other.to_string()),
        }
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::new(ErrorKind::Io, format!("Background task failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_kind_and_message() {
        let err = AppError::new(ErrorKind::NotFound, "gone").with_path("/tmp/x");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "notFound");
        assert_eq!(json["message"], "gone");
        assert_eq!(json["path"], "/tmp/x");
        assert!(json.get("hostKey").is_none());
    }

    #[test]
    fn host_key_errors_carry_fingerprint() {
        let err = AppError::host_key(
            ErrorKind::HostKeyUnknown,
            HostKeyInfo {
                host: "example.com".into(),
                port: 22,
                algorithm: "ssh-ed25519".into(),
                fingerprint: "SHA256:abc".into(),
            },
        );
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "hostKeyUnknown");
        assert_eq!(json["hostKey"]["fingerprint"], "SHA256:abc");
    }
}
