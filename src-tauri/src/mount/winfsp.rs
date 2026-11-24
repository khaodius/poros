//! The Windows side of a mounted drive, which is not built yet.
//!
//! Windows drives would go through WinFsp, a file system driver under the GPLv3 with an
//! exception for free and open source programs. That exception has conditions (a notice in
//! the program's interface, and no linking or shipping with software that is not free, which
//! the Windows build may do through Microsoft's WebView2 loader), and the one Rust binding
//! that wraps WinFsp safely is GPLv3 itself, which would put the whole Windows build under the
//! GPL. Which way to go is the project owner's to decide, so until then this adapter mounts
//! nothing and says so. The file system in `vfs` is ready for it: an adapter only has to
//! translate WinFsp's callbacks into its calls, as `fuse` does for the Linux kernel.
//!
//! Compiled into the Linux tests as well, so CI checks it on both systems.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::platform::Missing;
use super::vfs::MountFs;
use crate::error::{AppError, AppResult};

/// Stands where a mounted drive will.
pub struct WinFspMount {
    mount_point: PathBuf,
}

impl WinFspMount {
    pub fn start(
        _core: Arc<MountFs>,
        mount_point: &Path,
        _label: &str,
        _on_end: Box<dyn Fn() + Send>,
    ) -> AppResult<Self> {
        let _ = mount_point;
        Err(AppError::unsupported(not_yet().problem))
    }

    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    pub fn unmount(&self, _force: bool) -> AppResult<()> {
        Ok(())
    }
}

fn not_yet() -> Missing {
    Missing {
        problem: "Mounting a server as a drive is not available on Windows yet.".into(),
        help: "Open and save its files through the panes meanwhile.".into(),
        link: None,
    }
}

pub fn missing() -> Option<Missing> {
    Some(not_yet())
}

/// Nothing is ever mounted, so nothing is left behind.
pub fn clear_dead_drives() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::super::backend::{Limits, SharedBackend};
    use super::super::memory::MemoryFs;
    use super::super::vfs::FsOptions;
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn windows_says_mounting_is_not_available_yet() {
        let missing = missing().unwrap();
        assert!(missing.problem.contains("not available on Windows yet"));
        assert!(clear_dead_drives().is_empty());

        let folder = tempfile::tempdir().unwrap();
        let backend = Arc::new(SharedBackend::new(MemoryFs::new(), Limits::SHARED));
        let core = MountFs::new(
            backend,
            "/",
            FsOptions::new("memory", folder.path().join("spool")),
        );
        let refused = WinFspMount::start(core, Path::new("P:\\"), "memory", Box::new(|| {}));
        assert!(matches!(refused, Err(error) if error.kind == ErrorKind::Unsupported));
    }
}
