//! The adapter that attaches drives to this system: FUSE on Linux, WinFsp on Windows, and
//! none elsewhere. Each offers the same few calls, so nothing above it is written per system.

use serde::Serialize;

/// What is missing for mounting on this computer, and what to do about it. Mirrored in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Missing {
    pub problem: String,
    pub help: String,
    /// Where to get what is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// Whether drives are named by a letter (Windows) rather than attached to a folder.
pub const USES_DRIVE_LETTERS: bool = cfg!(windows);

#[cfg(target_os = "linux")]
pub use super::fuse::{clear_dead_drives, missing, FuseMount as SystemMount};

#[cfg(windows)]
pub use super::winfsp::{clear_dead_drives, missing, WinFspMount as SystemMount};

#[cfg(not(any(target_os = "linux", windows)))]
pub use elsewhere::{clear_dead_drives, missing, NoMount as SystemMount};

/// Systems Poros does not ship for. Compiled into the Linux tests as well, so it stays
/// buildable.
#[cfg(any(not(any(target_os = "linux", windows)), test))]
pub mod elsewhere {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use super::Missing;
    use crate::error::{AppError, AppResult};
    use crate::mount::vfs::MountFs;

    pub struct NoMount {
        mount_point: PathBuf,
    }

    impl NoMount {
        pub fn start(
            _core: Arc<MountFs>,
            mount_point: &Path,
            _label: &str,
            _on_end: Box<dyn Fn() + Send>,
        ) -> AppResult<Self> {
            let _ = mount_point;
            Err(AppError::unsupported(unavailable().problem))
        }

        pub fn mount_point(&self) -> &Path {
            &self.mount_point
        }

        pub fn unmount(&self, _force: bool) -> AppResult<()> {
            Ok(())
        }
    }

    fn unavailable() -> Missing {
        Missing {
            problem: "Mounting a server as a drive is not available on this system.".into(),
            help: "Open and save its files through the panes.".into(),
            link: None,
        }
    }

    pub fn missing() -> Option<Missing> {
        Some(unavailable())
    }

    pub fn clear_dead_drives() -> Vec<PathBuf> {
        Vec::new()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::mount::backend::{Limits, SharedBackend};
        use crate::mount::memory::MemoryFs;
        use crate::mount::vfs::FsOptions;

        #[test]
        fn other_systems_say_mounting_is_not_available() {
            assert!(missing().is_some());
            assert!(clear_dead_drives().is_empty());
            let folder = tempfile::tempdir().unwrap();
            let backend = Arc::new(SharedBackend::new(MemoryFs::new(), Limits::SHARED));
            let core = MountFs::new(
                backend,
                "/",
                FsOptions::new("memory", folder.path().join("spool")),
            );
            let refused = NoMount::start(core, Path::new("/Volumes/x"), "memory", Box::new(|| {}));
            assert!(refused.is_err());
        }
    }
}
