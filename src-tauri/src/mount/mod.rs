//! A connected server, or an unlocked vault on one, shown as a drive on this computer, so
//! other programs open and save its files. `vfs` holds the file system itself, over any
//! `RemoteFileSystem`; `fuse` (Linux) and `winfsp` (Windows) hand it the kernel's requests;
//! `manager` keeps the drives of a run and ties them to their sessions.

pub mod backend;
pub mod blocks;
#[cfg(target_os = "linux")]
pub mod fuse;
pub mod manager;
pub mod names;
pub mod platform;
pub mod session_backend;
pub mod spool;
pub mod upload;
pub mod vfs;
#[cfg(any(windows, test))]
pub mod winfsp;

#[cfg(test)]
mod memory;
#[cfg(test)]
mod vfs_tests;
