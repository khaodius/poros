//! Exit statuses, stable so scripts and agents can branch on them without reading messages.

use std::process::ExitCode;

use crate::error::ErrorKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Success,
    /// Anything without a status of its own.
    Failure,
    /// The arguments were wrong, or a value given was not valid.
    Usage,
    /// The server could not be reached, or the connection dropped.
    Connection,
    /// The server refused the sign-in, or no password, key or passphrase was given.
    Authentication,
    /// The server's host key or certificate is not trusted yet.
    HostKeyUnknown,
    /// The server's host key or certificate is not the one trusted before.
    HostKeyChanged,
    NotFound,
    PermissionDenied,
    AlreadyExists,
    /// Some items succeeded and some failed.
    Partial,
    /// Contents differ: a verification found differences, or a transfer failed its check.
    Integrity,
    /// The protocol cannot do what was asked.
    Unsupported,
    Interrupted,
    /// A command run on the server ended with this status.
    Remote(u8),
}

/// Shown at the end of `--help`.
pub const TABLE: &str = "\
Exit status:
  0    success
  1    failure without a status of its own
  2    invalid arguments
  3    could not connect, or the connection dropped
  4    sign-in refused, or no password, key or passphrase given
  5    host key or certificate not trusted yet (see --host-key, --trust-new-host)
  6    host key or certificate changed since it was trusted
  7    not found
  8    permission denied
  9    already exists
  10   some items failed (transfers, deletes, synchronization)
  11   contents differ, or a file failed its integrity check or published checksum
  12   not supported by the connection's protocol
  130  interrupted
  exec exits with the status of the command it ran.";

impl Exit {
    pub fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Failure => 1,
            Self::Usage => 2,
            Self::Connection => 3,
            Self::Authentication => 4,
            Self::HostKeyUnknown => 5,
            Self::HostKeyChanged => 6,
            Self::NotFound => 7,
            Self::PermissionDenied => 8,
            Self::AlreadyExists => 9,
            Self::Partial => 10,
            Self::Integrity => 11,
            Self::Unsupported => 12,
            Self::Interrupted => 130,
            Self::Remote(status) => status,
        }
    }

    pub fn for_error(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::HostKeyUnknown => Self::HostKeyUnknown,
            ErrorKind::HostKeyChanged => Self::HostKeyChanged,
            ErrorKind::AuthFailed | ErrorKind::PassphraseRequired => Self::Authentication,
            ErrorKind::Connection
            | ErrorKind::Timeout
            | ErrorKind::Disconnected
            | ErrorKind::SessionNotFound => Self::Connection,
            ErrorKind::NotFound => Self::NotFound,
            ErrorKind::PermissionDenied => Self::PermissionDenied,
            ErrorKind::AlreadyExists => Self::AlreadyExists,
            ErrorKind::InvalidInput => Self::Usage,
            ErrorKind::Integrity | ErrorKind::Untrusted => Self::Integrity,
            ErrorKind::Unsupported => Self::Unsupported,
            ErrorKind::Cancelled => Self::Interrupted,
            ErrorKind::Io
            | ErrorKind::Sftp
            | ErrorKind::Ssh
            | ErrorKind::Rsync
            | ErrorKind::Ftp
            | ErrorKind::Cloud
            | ErrorKind::Keychain
            | ErrorKind::Update => Self::Failure,
        }
    }
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        ExitCode::from(exit.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_in_the_table_is_the_one_returned() {
        let listed: Vec<u8> = TABLE
            .lines()
            .filter_map(|line| line.split_whitespace().next()?.parse().ok())
            .collect();
        let statuses = [
            Exit::Success,
            Exit::Failure,
            Exit::Usage,
            Exit::Connection,
            Exit::Authentication,
            Exit::HostKeyUnknown,
            Exit::HostKeyChanged,
            Exit::NotFound,
            Exit::PermissionDenied,
            Exit::AlreadyExists,
            Exit::Partial,
            Exit::Integrity,
            Exit::Unsupported,
            Exit::Interrupted,
        ];
        assert_eq!(
            listed,
            statuses.iter().map(|exit| exit.code()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn errors_map_to_their_statuses() {
        assert_eq!(
            Exit::for_error(ErrorKind::PassphraseRequired),
            Exit::Authentication
        );
        assert_eq!(Exit::for_error(ErrorKind::Timeout), Exit::Connection);
        assert_eq!(Exit::for_error(ErrorKind::InvalidInput), Exit::Usage);
        assert_eq!(Exit::for_error(ErrorKind::Cancelled).code(), 130);
    }
}
