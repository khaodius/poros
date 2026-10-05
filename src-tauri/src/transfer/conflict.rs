use serde::{Deserialize, Serialize};

/// What to do when the target of a file transfer already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ExistsAction {
    #[default]
    Ask,
    Overwrite,
    OverwriteIfNewer,
    OverwriteIfDifferent,
    Resume,
    Rename,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileFacts {
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Write { offset: u64 },
    Rename,
    Skip,
    Ask,
}

pub fn decide(action: ExistsAction, source: FileFacts, target: FileFacts) -> Decision {
    let overwrite = Decision::Write { offset: 0 };
    match action {
        ExistsAction::Ask => Decision::Ask,
        ExistsAction::Overwrite => overwrite,
        ExistsAction::OverwriteIfNewer => match (source.modified, target.modified) {
            (Some(source_time), Some(target_time)) if source_time <= target_time => Decision::Skip,
            _ => overwrite,
        },
        ExistsAction::OverwriteIfDifferent => {
            let same_time = source.modified.is_some() && source.modified == target.modified;
            if source.size == target.size && same_time {
                Decision::Skip
            } else {
                overwrite
            }
        }
        ExistsAction::Resume => match target.size.cmp(&source.size) {
            std::cmp::Ordering::Less => Decision::Write {
                offset: target.size,
            },
            std::cmp::Ordering::Equal => Decision::Skip,
            std::cmp::Ordering::Greater => overwrite,
        },
        ExistsAction::Rename => Decision::Rename,
        ExistsAction::Skip => Decision::Skip,
    }
}

/// `report.pdf` -> `report (2).pdf`; names without an extension, and dotfiles, get the number
/// at the end.
pub fn numbered_name(name: &str, number: u32) -> String {
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{} ({number}){}", &name[..dot], &name[dot..]),
        _ => format!("{name} ({number})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(size: u64, modified: i64) -> FileFacts {
        FileFacts {
            size,
            modified: Some(modified),
        }
    }

    #[test]
    fn overwrite_variants() {
        let old = facts(10, 100);
        let new = facts(12, 200);
        let overwrite = Decision::Write { offset: 0 };
        assert_eq!(decide(ExistsAction::Overwrite, old, new), overwrite);
        assert_eq!(decide(ExistsAction::OverwriteIfNewer, new, old), overwrite);
        assert_eq!(
            decide(ExistsAction::OverwriteIfNewer, old, new),
            Decision::Skip
        );
        assert_eq!(
            decide(ExistsAction::OverwriteIfDifferent, old, old),
            Decision::Skip
        );
        assert_eq!(
            decide(ExistsAction::OverwriteIfDifferent, old, facts(10, 101)),
            overwrite
        );
        let unknown_time = FileFacts {
            size: 10,
            modified: None,
        };
        assert_eq!(
            decide(
                ExistsAction::OverwriteIfDifferent,
                unknown_time,
                unknown_time
            ),
            overwrite
        );
    }

    #[test]
    fn resume_continues_shorter_targets_only() {
        let source = facts(100, 0);
        assert_eq!(
            decide(ExistsAction::Resume, source, facts(40, 0)),
            Decision::Write { offset: 40 }
        );
        assert_eq!(
            decide(ExistsAction::Resume, source, facts(100, 0)),
            Decision::Skip
        );
        assert_eq!(
            decide(ExistsAction::Resume, source, facts(150, 0)),
            Decision::Write { offset: 0 }
        );
    }

    #[test]
    fn numbered_names_keep_the_extension() {
        assert_eq!(numbered_name("report.pdf", 1), "report (1).pdf");
        assert_eq!(numbered_name("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(numbered_name("Makefile", 3), "Makefile (3)");
        assert_eq!(numbered_name(".bashrc", 1), ".bashrc (1)");
    }
}
