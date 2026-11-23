//! Cryptomator vaults (format 8) on any server: a folder whose file names, folder structure
//! and contents are encrypted on this computer, so the server only ever holds ciphertext.
//! Vaults made here open in the Cryptomator apps and the other way round.
//!
//! `masterkey` and `config` open a vault with its password, `names` and `content` are its two
//! ciphers, and `fs` shows the decrypted tree as a `RemoteFileSystem` over the server's own.

pub mod config;
pub mod content;
pub mod fs;
pub mod masterkey;
pub mod names;
mod stream;

use crate::error::AppError;

pub use fs::VaultFs;

/// The answer to a vault given as a server to connect to or save.
pub fn not_a_connection() -> AppError {
    AppError::invalid("A vault is unlocked in its folder on a server, not connected to")
}
