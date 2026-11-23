//! File and folder names in a vault. A name is encrypted with AES-SIV, bound to the id of the
//! folder it is in, and stored as base64 with the `.c9r` extension. A folder's contents live in
//! `d/XX/YYYY...`, named by a hash of the folder's id.

use std::sync::Mutex;

use aes_siv::siv::Aes256Siv;
use aes_siv::KeyInit;
use base64::engine::general_purpose::URL_SAFE;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::{alphabet, Engine};
use ring::digest;
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use super::masterkey::{Masterkey, KEY_LENGTH};
use crate::error::{AppError, AppResult};

/// The folder at the top of a vault that holds every folder's contents.
pub const DATA_FOLDER: &str = "d";
/// The id of a vault's top folder.
pub const ROOT_DIRECTORY_ID: &str = "";
pub const ENCRYPTED_SUFFIX: &str = ".c9r";
pub const SHORTENED_SUFFIX: &str = ".c9s";
/// Inside a folder's node: the id of the folder.
pub const DIRECTORY_FILE: &str = "dir.c9r";
/// Inside a link's node: the link's target, encrypted like a file.
pub const SYMLINK_FILE: &str = "symlink.c9r";
/// Inside a shortened node: the file's contents.
pub const CONTENTS_FILE: &str = "contents.c9r";
/// Inside a shortened node: the full encrypted name.
pub const FULL_NAME_FILE: &str = "name.c9s";
/// Inside a folder's contents: the folder's own id, encrypted like a file, so a vault with a
/// lost `dir.c9r` can be repaired.
pub const DIRECTORY_ID_BACKUP: &str = "dirid.c9r";

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Reads base64url names with or without their padding.
const LENIENT_URL_SAFE: GeneralPurpose = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

pub struct NameCryptor {
    siv: Mutex<Aes256Siv>,
}

impl NameCryptor {
    pub fn new(masterkey: &Masterkey) -> Self {
        // AES-SIV takes the MAC key first, then the encryption key.
        let mut key = Zeroizing::new([0u8; KEY_LENGTH * 2]);
        key[..KEY_LENGTH].copy_from_slice(masterkey.mac_key());
        key[KEY_LENGTH..].copy_from_slice(masterkey.encryption_key());
        Self {
            siv: Mutex::new(Aes256Siv::new((&*key).into())),
        }
    }

    /// The stored name of `name` in the folder with `directory_id`, without its extension.
    /// Names are normalized (NFC) first, as Cryptomator does, so a name reads the same
    /// whichever system wrote it.
    pub fn encrypt(&self, directory_id: &str, name: &str) -> String {
        let normalized: String = name.nfc().collect();
        let encrypted = self
            .siv
            .lock()
            .unwrap()
            .encrypt([directory_id.as_bytes()], normalized.as_bytes())
            .expect("one header is within the limit");
        URL_SAFE.encode(encrypted)
    }

    /// The name behind a stored name, given without its extension.
    pub fn decrypt(&self, directory_id: &str, encrypted: &str) -> AppResult<String> {
        let unreadable = || {
            AppError::integrity(format!(
                "{encrypted}{ENCRYPTED_SUFFIX} is not a name this vault encrypted"
            ))
        };
        let bytes = LENIENT_URL_SAFE
            .decode(encrypted)
            .map_err(|_| unreadable())?;
        let name = self
            .siv
            .lock()
            .unwrap()
            .decrypt([directory_id.as_bytes()], &bytes)
            .map_err(|_| unreadable())?;
        String::from_utf8(name).map_err(|_| unreadable())
    }

    /// Where the contents of the folder with `directory_id` are kept, relative to the vault's
    /// folder: `d/XX/YYYY...`.
    pub fn directory_path(&self, directory_id: &str) -> String {
        let encrypted = self
            .siv
            .lock()
            .unwrap()
            .encrypt(std::iter::empty::<&[u8]>(), directory_id.as_bytes())
            .expect("no headers are within the limit");
        let hash = base32(digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, &encrypted).as_ref());
        format!("{DATA_FOLDER}/{}/{}", &hash[..2], &hash[2..])
    }
}

/// The name of the node that stands in for an encrypted name too long to store, which is
/// given with its extension.
pub fn shortened_name(encrypted_name: &str) -> String {
    let hash = digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, encrypted_name.as_bytes());
    format!("{}{SHORTENED_SUFFIX}", URL_SAFE.encode(hash))
}

/// RFC 4648 base32 with padding.
fn base32(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(5) * 8);
    for group in bytes.chunks(5) {
        let mut buffer = [0u8; 5];
        buffer[..group.len()].copy_from_slice(group);
        let bits = u64::from_be_bytes([
            0, 0, 0, buffer[0], buffer[1], buffer[2], buffer[3], buffer[4],
        ]);
        let characters = (group.len() * 8).div_ceil(5);
        for index in 0..8 {
            if index < characters {
                let value = (bits >> (35 - index * 5)) & 0x1f;
                encoded.push(BASE32_ALPHABET[value as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cryptor() -> NameCryptor {
        NameCryptor::new(&Masterkey::generate().unwrap())
    }

    #[test]
    fn base32_matches_the_rfc_examples() {
        assert_eq!(base32(b""), "");
        assert_eq!(base32(b"f"), "MY======");
        assert_eq!(base32(b"fo"), "MZXQ====");
        assert_eq!(base32(b"foo"), "MZXW6===");
        assert_eq!(base32(b"foob"), "MZXW6YQ=");
        assert_eq!(base32(b"fooba"), "MZXW6YTB");
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI======");
    }

    #[test]
    fn names_come_back_only_in_their_own_folder() {
        let names = cryptor();
        let encrypted = names.encrypt("folder-id", "report.txt");
        assert_eq!(
            names.decrypt("folder-id", &encrypted).unwrap(),
            "report.txt"
        );
        assert!(names.decrypt("other-id", &encrypted).is_err());
        assert!(names.decrypt(ROOT_DIRECTORY_ID, &encrypted).is_err());
    }

    #[test]
    fn the_same_name_encrypts_the_same_way_each_time() {
        let names = cryptor();
        assert_eq!(names.encrypt("", "a.txt"), names.encrypt("", "a.txt"));
        assert_ne!(names.encrypt("", "a.txt"), names.encrypt("", "b.txt"));
        assert_ne!(names.encrypt("", "a.txt"), names.encrypt("x", "a.txt"));
    }

    #[test]
    fn decomposed_names_are_stored_composed() {
        let names = cryptor();
        let decomposed = names.encrypt("", "e\u{0301}te\u{0301}.txt");
        assert_eq!(decomposed, names.encrypt("", "\u{00e9}t\u{00e9}.txt"));
        assert_eq!(
            names.decrypt("", &decomposed).unwrap(),
            "\u{00e9}t\u{00e9}.txt"
        );
    }

    #[test]
    fn stored_names_keep_their_padding_and_read_without_it() {
        let names = cryptor();
        let encrypted = names.encrypt("", "a");
        assert!(encrypted.ends_with('='), "{encrypted}");
        let unpadded = encrypted.trim_end_matches('=');
        assert_eq!(names.decrypt("", unpadded).unwrap(), "a");
    }

    #[test]
    fn garbage_is_not_a_name() {
        let names = cryptor();
        for garbage in ["", "AAAA", "not base64 !", "dGVzdA=="] {
            assert!(names.decrypt("", garbage).is_err(), "{garbage}");
        }
    }

    #[test]
    fn folder_contents_sit_two_levels_under_the_data_folder() {
        let names = cryptor();
        let path = names.directory_path(ROOT_DIRECTORY_ID);
        let parts: Vec<&str> = path.split('/').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "d");
        assert_eq!(parts[1].len(), 2);
        assert_eq!(parts[2].len(), 30);
        assert!(path[2..]
            .chars()
            .all(|character| character == '/' || BASE32_ALPHABET.contains(&(character as u8))));
        assert_eq!(path, names.directory_path(ROOT_DIRECTORY_ID));
        assert_ne!(path, names.directory_path("some-id"));
    }

    #[test]
    fn shortened_names_are_a_hash_of_the_full_name() {
        let shortened = shortened_name("abc.c9r");
        assert_eq!(shortened.len(), 28 + SHORTENED_SUFFIX.len());
        assert!(shortened.ends_with("=.c9s"), "{shortened}");
        assert_eq!(shortened, shortened_name("abc.c9r"));
        assert_ne!(shortened, shortened_name("abd.c9r"));
    }
}
