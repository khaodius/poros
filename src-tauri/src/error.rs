use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    HostKeyUnknown,
    HostKeyChanged,
    AuthFailed,
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
    Rsync,
    Ftp,
    /// A Google Drive or OneDrive request failed.
    Cloud,
    /// The connection's protocol cannot do what was asked, such as rsync over FTP.
    Unsupported,
    Cancelled,
    Keychain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyInfo {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    /// Why the system did not trust a TLS certificate; absent for SSH host keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_problem: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_key: Option<Box<HostKeyInfo>>,
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

    pub fn cancelled() -> Self {
        Self::new(ErrorKind::Cancelled, "Cancelled")
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, message)
    }

    /// Network trouble that a later attempt, on a fresh connection, may not hit.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind,
            ErrorKind::Timeout
                | ErrorKind::Disconnected
                | ErrorKind::Connection
                | ErrorKind::Ssh
                | ErrorKind::Sftp
                | ErrorKind::Ftp
                | ErrorKind::Cloud
        )
    }

    /// The connection carrying the request is unusable and must be reopened.
    pub fn is_connection_lost(&self) -> bool {
        matches!(
            self.kind,
            ErrorKind::Timeout | ErrorKind::Disconnected | ErrorKind::Connection | ErrorKind::Ssh
        )
    }

    pub fn host_key(kind: ErrorKind, info: HostKeyInfo) -> Self {
        let message = match (kind, &info.certificate_problem) {
            (ErrorKind::HostKeyChanged, Some(_)) => format!(
                "The TLS certificate of {}:{} is not the one you trusted before ({}). This can mean someone is intercepting the connection.",
                info.host, info.port, info.fingerprint
            ),
            (_, Some(problem)) => format!(
                "The TLS certificate of {}:{} is not trusted by this computer ({}). {problem}",
                info.host, info.port, info.fingerprint
            ),
            _ => Self::host_key_message(kind, &info),
        };
        Self {
            kind,
            message,
            host_key: Some(Box::new(info)),
            path: None,
        }
    }

    fn host_key_message(kind: ErrorKind, info: &HostKeyInfo) -> String {
        match kind {
            ErrorKind::HostKeyChanged => format!(
                "The host key for {}:{} has changed ({} {}). This can mean someone is intercepting the connection.",
                info.host, info.port, info.algorithm, info.fingerprint
            ),
            _ => format!(
                "The authenticity of {}:{} can't be established ({} {}).",
                info.host, info.port, info.algorithm, info.fingerprint
            ),
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
            other => Self::new(
                ErrorKind::AuthFailed,
                format!("Could not load key: {other}"),
            ),
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
        let error = AppError::new(ErrorKind::NotFound, "gone").with_path("/tmp/x");
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["kind"], "notFound");
        assert_eq!(json["message"], "gone");
        assert_eq!(json["path"], "/tmp/x");
        assert!(json.get("hostKey").is_none());
    }

    #[test]
    fn host_key_errors_carry_fingerprint() {
        let error = AppError::host_key(
            ErrorKind::HostKeyUnknown,
            HostKeyInfo {
                host: "example.com".into(),
                port: 22,
                algorithm: "ssh-ed25519".into(),
                fingerprint: "SHA256:abc".into(),
                certificate_problem: None,
            },
        );
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["kind"], "hostKeyUnknown");
        assert_eq!(json["hostKey"]["fingerprint"], "SHA256:abc");
    }
}
