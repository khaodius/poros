//! Whether the server's copy of a file changed behind an edit, decided before each upload so
//! what someone else saved there is never written over unseen.

use serde::{Deserialize, Serialize};

use crate::protocol::RemoteStat;

/// What Poros last knew of the server's copy: as it was downloaded, or as the last upload
/// left it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerState {
    pub size: u64,
    /// Seconds since the Unix epoch, where the server tells.
    pub modified: Option<i64>,
}

impl ServerState {
    pub fn of(stat: &RemoteStat) -> Self {
        Self {
            size: stat.size,
            modified: stat.modified,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerChange {
    Unchanged,
    /// Another size or another time than the baseline's.
    Changed,
    /// Nothing is at the path any more.
    Deleted,
}

/// How the server's copy stands against `baseline`. A time only counts when both sides have
/// one: a server that tells none is compared by size alone.
pub fn server_change(baseline: &ServerState, current: Option<&ServerState>) -> ServerChange {
    let Some(current) = current else {
        return ServerChange::Deleted;
    };
    let time_moved = matches!(
        (baseline.modified, current.modified),
        (Some(before), Some(now)) if before != now
    );
    if current.size != baseline.size || time_moved {
        ServerChange::Changed
    } else {
        ServerChange::Unchanged
    }
}

/// The user's answer when the server's copy changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Resolution {
    /// Replace the server's copy with the working copy.
    Overwrite,
    /// Upload the working copy under a new name beside the server's copy.
    KeepBoth,
    /// Upload nothing for now; the working copy stays as it is.
    Cancel,
}

/// What an upload does next, given how the server's copy stands and what the user said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadDecision {
    /// Write over what is at the path.
    Replace,
    /// Write beside what is at the path, under a name not taken.
    BesideServerCopy,
    /// Ask the user first.
    Ask,
    /// Leave the server alone until the user asks again.
    Hold,
}

pub fn upload_decision(change: ServerChange, answer: Option<Resolution>) -> UploadDecision {
    match (change, answer) {
        (ServerChange::Unchanged, _) => UploadDecision::Replace,
        (_, None) => UploadDecision::Ask,
        (_, Some(Resolution::Overwrite)) => UploadDecision::Replace,
        // With nothing left on the server there is no second file to keep.
        (ServerChange::Deleted, Some(Resolution::KeepBoth)) => UploadDecision::Replace,
        (ServerChange::Changed, Some(Resolution::KeepBoth)) => UploadDecision::BesideServerCopy,
        (_, Some(Resolution::Cancel)) => UploadDecision::Hold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(size: u64, modified: Option<i64>) -> ServerState {
        ServerState { size, modified }
    }

    #[test]
    fn the_same_size_and_time_is_unchanged() {
        let baseline = state(120, Some(1_700_000_000));
        assert_eq!(
            server_change(&baseline, Some(&state(120, Some(1_700_000_000)))),
            ServerChange::Unchanged
        );
    }

    #[test]
    fn another_size_or_time_is_a_change() {
        let baseline = state(120, Some(1_700_000_000));
        assert_eq!(
            server_change(&baseline, Some(&state(121, Some(1_700_000_000)))),
            ServerChange::Changed
        );
        assert_eq!(
            server_change(&baseline, Some(&state(120, Some(1_700_000_060)))),
            ServerChange::Changed
        );
        // An older time is a change too: someone put an earlier version back.
        assert_eq!(
            server_change(&baseline, Some(&state(120, Some(1_600_000_000)))),
            ServerChange::Changed
        );
    }

    #[test]
    fn a_server_without_times_is_compared_by_size() {
        let baseline = state(120, None);
        assert_eq!(
            server_change(&baseline, Some(&state(120, Some(5)))),
            ServerChange::Unchanged
        );
        assert_eq!(
            server_change(&state(120, Some(5)), Some(&state(120, None))),
            ServerChange::Unchanged
        );
        assert_eq!(
            server_change(&baseline, Some(&state(7, None))),
            ServerChange::Changed
        );
    }

    #[test]
    fn a_file_gone_from_the_server_is_told_apart() {
        assert_eq!(
            server_change(&state(120, Some(5)), None),
            ServerChange::Deleted
        );
    }

    #[test]
    fn an_unchanged_server_copy_is_replaced_without_asking() {
        for answer in [
            None,
            Some(Resolution::Overwrite),
            Some(Resolution::KeepBoth),
            Some(Resolution::Cancel),
        ] {
            assert_eq!(
                upload_decision(ServerChange::Unchanged, answer),
                UploadDecision::Replace
            );
        }
    }

    #[test]
    fn a_changed_server_copy_waits_for_the_user() {
        assert_eq!(
            upload_decision(ServerChange::Changed, None),
            UploadDecision::Ask
        );
        assert_eq!(
            upload_decision(ServerChange::Deleted, None),
            UploadDecision::Ask
        );
        assert_eq!(
            upload_decision(ServerChange::Changed, Some(Resolution::Overwrite)),
            UploadDecision::Replace
        );
        assert_eq!(
            upload_decision(ServerChange::Changed, Some(Resolution::KeepBoth)),
            UploadDecision::BesideServerCopy
        );
        assert_eq!(
            upload_decision(ServerChange::Changed, Some(Resolution::Cancel)),
            UploadDecision::Hold
        );
        assert_eq!(
            upload_decision(ServerChange::Deleted, Some(Resolution::KeepBoth)),
            UploadDecision::Replace
        );
        assert_eq!(
            upload_decision(ServerChange::Deleted, Some(Resolution::Cancel)),
            UploadDecision::Hold
        );
    }
}
