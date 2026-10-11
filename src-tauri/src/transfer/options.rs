//! What a synchronization asks of each file it queues, over the transfer settings: rsync's
//! options that act on one file as it is copied.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::remote_path;
use crate::settings::TransferSettings;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JobOptions {
    pub preserve_timestamps: Option<bool>,
    pub preserve_permissions: Option<bool>,
    pub temporary_files: Option<bool>,
    pub delta_transfers: Option<bool>,
    pub flush_to_disk: Option<bool>,
    pub verify_checksums: Option<bool>,
    pub log_each_file: Option<bool>,
    pub rsync_path: Option<String>,
    /// Bytes per rsync block.
    pub block_size: Option<u32>,
    /// Without preserved permissions, gives the target the source's executability.
    pub executability: bool,
    /// `--chmod` rules for the finished file.
    pub chmod: String,
    pub owner: Option<u32>,
    pub group: Option<u32>,
    /// Keeps the file a copy replaces.
    pub backup: Option<Backup>,
    /// Deletes the source once the copy is in place.
    pub remove_source: bool,
    /// Moves the file over a compressed connection.
    pub compress: bool,
    /// Bytes per second, shared by every job with the same `limit_group`; zero is unlimited.
    pub rate_limit: u64,
    pub limit_group: String,
}

/// Where a replaced file goes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Backup {
    /// The folder it moves into, on the target's side; `None` keeps it beside the new file.
    pub folder: Option<String>,
    pub suffix: String,
}

impl Backup {
    pub fn local_path(&self, target: &str) -> PathBuf {
        let target = Path::new(target);
        let name = format!(
            "{}{}",
            target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            self.suffix
        );
        match &self.folder {
            Some(folder) => Path::new(folder).join(name),
            None => target.with_file_name(name),
        }
    }

    pub fn remote_path(&self, target: &str) -> String {
        let name = format!("{}{}", remote_path::file_name(target), self.suffix);
        let folder = self
            .folder
            .clone()
            .or_else(|| remote_path::parent(target))
            .unwrap_or_else(|| "/".to_string());
        remote_path::join(&folder, &name)
    }
}

impl JobOptions {
    /// The transfer settings this job runs with.
    pub fn apply(&self, settings: &mut TransferSettings) {
        let overrides = [
            (&mut settings.preserve_timestamps, self.preserve_timestamps),
            (
                &mut settings.preserve_permissions,
                self.preserve_permissions,
            ),
            (&mut settings.temporary_files, self.temporary_files),
            (&mut settings.delta_transfers, self.delta_transfers),
            (&mut settings.flush_to_disk, self.flush_to_disk),
            (&mut settings.verify_checksums, self.verify_checksums),
            (&mut settings.log_each_file, self.log_each_file),
        ];
        for (setting, value) in overrides {
            if let Some(value) = value {
                *setting = value;
            }
        }
        if let Some(path) = self.rsync_path.as_deref().and_then(super::one_line) {
            settings.rsync_path = path;
        }
        // A backup is the file a temporary one replaces, so it needs one.
        if self.backup.is_some() {
            settings.temporary_files = true;
        }
    }

    /// Whether anything is left to do once the file itself is in place.
    pub fn finishes(&self) -> bool {
        self.executability
            || !self.chmod.is_empty()
            || self.owner.is_some()
            || self.group.is_some()
            || self.remove_source
    }
}

/// The permissions a finished file ends with: `current` adjusted for the source's
/// executability when permissions are not copied, then the `--chmod` rules.
pub fn finished_mode(
    current: u32,
    source: Option<u32>,
    options: &JobOptions,
    permissions_copied: bool,
) -> u32 {
    let mut mode = current & 0o7777;
    if options.executability && !permissions_copied {
        if let Some(source) = source {
            let source_runs = source & 0o111 != 0;
            let target_runs = mode & 0o111 != 0;
            if source_runs && !target_runs {
                // Execute for each class that may read it.
                mode |= (mode & 0o444) >> 2;
            } else if !source_runs && target_runs {
                mode &= !0o111;
            }
        }
    }
    crate::sync::Chmod::parse(&options.chmod)
        .unwrap_or_default()
        .apply(mode, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_apply_over_the_settings() {
        let mut settings = TransferSettings::default();
        let options = JobOptions {
            preserve_permissions: Some(true),
            temporary_files: Some(false),
            delta_transfers: Some(false),
            rsync_path: Some("/opt/bin/rsync".into()),
            backup: Some(Backup::default()),
            ..JobOptions::default()
        };
        options.apply(&mut settings);
        assert!(settings.preserve_permissions);
        assert!(settings.preserve_timestamps);
        assert!(!settings.delta_transfers);
        // Backups need a temporary file to replace the target.
        assert!(settings.temporary_files);
        assert_eq!(settings.rsync_path, "/opt/bin/rsync");
    }

    #[test]
    fn backup_paths_and_finished_modes() {
        let beside = Backup {
            folder: None,
            suffix: "~".into(),
        };
        assert_eq!(
            beside.remote_path("/var/www/index.html"),
            "/var/www/index.html~"
        );
        let away = Backup {
            folder: Some("/backups/2026".into()),
            suffix: String::new(),
        };
        assert_eq!(
            away.remote_path("/var/www/index.html"),
            "/backups/2026/index.html"
        );
        assert_eq!(
            away.local_path("/home/me/a.txt"),
            Path::new("/backups/2026").join("a.txt")
        );

        let executable = JobOptions {
            executability: true,
            ..JobOptions::default()
        };
        assert_eq!(finished_mode(0o644, Some(0o755), &executable, false), 0o755);
        assert_eq!(finished_mode(0o640, Some(0o700), &executable, false), 0o750);
        assert_eq!(finished_mode(0o755, Some(0o644), &executable, false), 0o644);
        assert_eq!(finished_mode(0o644, Some(0o755), &executable, true), 0o644);
        let chmod = JobOptions {
            chmod: "go-rwx".into(),
            ..executable
        };
        assert_eq!(finished_mode(0o644, Some(0o755), &chmod, false), 0o700);
    }
}
