//! File contents in a vault. A file starts with a header that holds a key of its own,
//! encrypted with the vault's keys. The contents follow in chunks of 32 KiB, each encrypted
//! and authenticated with that key and bound to its place in the file, so chunks can be read
//! in any order and cannot be swapped between files or positions.
//!
//! Two ciphers exist. Vaults from Cryptomator 1.7 on use AES-GCM. Older ones use AES-CTR with
//! an HMAC-SHA256 over each chunk, and keep doing so when a newer release opens them.

use aes::Aes256;
use ctr::cipher::{KeyIvInit, StreamCipher};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::hmac;
use zeroize::Zeroizing;

use super::config::CipherCombo;
use super::masterkey::{fill_random, Masterkey, KEY_LENGTH};
use crate::error::{AppError, AppResult, ErrorKind};

/// The bytes of a file each chunk holds before it is encrypted.
pub const CLEARTEXT_CHUNK: usize = 32 * 1024;
const GCM_NONCE: usize = 12;
const GCM_TAG: usize = 16;
const CTR_NONCE: usize = 16;
const MAC_LENGTH: usize = 32;
/// Eight reserved bytes, all ones, before the file's key.
const RESERVED: [u8; 8] = [0xff; 8];
const HEADER_PAYLOAD: usize = RESERVED.len() + KEY_LENGTH;

type AesCtr = ctr::Ctr128BE<Aes256>;

pub struct ContentCryptor {
    mode: Mode,
}

enum Mode {
    Gcm {
        header_key: Box<LessSafeKey>,
    },
    CtrMac {
        encryption_key: Zeroizing<[u8; KEY_LENGTH]>,
        mac_key: hmac::Key,
    },
}

/// What a file's header holds: the nonce every chunk is bound to, and the file's own key.
pub struct FileHeader {
    nonce: Vec<u8>,
    key: FileKey,
}

enum FileKey {
    Gcm(Box<LessSafeKey>),
    Ctr(Zeroizing<[u8; KEY_LENGTH]>),
}

impl ContentCryptor {
    pub fn new(masterkey: &Masterkey, cipher_combo: CipherCombo) -> Self {
        let mode = match cipher_combo {
            CipherCombo::SivGcm => Mode::Gcm {
                header_key: gcm_key(masterkey.encryption_key()),
            },
            CipherCombo::SivCtrMac => Mode::CtrMac {
                encryption_key: Zeroizing::new(*masterkey.encryption_key()),
                mac_key: hmac::Key::new(hmac::HMAC_SHA256, masterkey.mac_key()),
            },
        };
        Self { mode }
    }

    fn nonce_length(&self) -> usize {
        match self.mode {
            Mode::Gcm { .. } => GCM_NONCE,
            Mode::CtrMac { .. } => CTR_NONCE,
        }
    }

    fn tag_length(&self) -> usize {
        match self.mode {
            Mode::Gcm { .. } => GCM_TAG,
            Mode::CtrMac { .. } => MAC_LENGTH,
        }
    }

    pub fn header_length(&self) -> usize {
        self.nonce_length() + HEADER_PAYLOAD + self.tag_length()
    }

    /// The bytes encryption adds to each chunk.
    pub fn chunk_overhead(&self) -> usize {
        self.nonce_length() + self.tag_length()
    }

    /// The length of every stored chunk but a file's last.
    pub fn encrypted_chunk(&self) -> usize {
        CLEARTEXT_CHUNK + self.chunk_overhead()
    }

    /// The size of the file stored in `encrypted_size` bytes. A stored file cut short in its
    /// header or within a chunk's overhead counts for what is complete.
    pub fn cleartext_size(&self, encrypted_size: u64) -> u64 {
        let Some(chunks) = encrypted_size.checked_sub(self.header_length() as u64) else {
            return 0;
        };
        let encrypted_chunk = self.encrypted_chunk() as u64;
        let last = (chunks % encrypted_chunk).saturating_sub(self.chunk_overhead() as u64);
        (chunks / encrypted_chunk) * CLEARTEXT_CHUNK as u64 + last
    }

    /// The bytes a file of `cleartext_size` takes once encrypted.
    pub fn encrypted_size(&self, cleartext_size: u64) -> u64 {
        let chunk = CLEARTEXT_CHUNK as u64;
        let last = match cleartext_size % chunk {
            0 => 0,
            rest => rest + self.chunk_overhead() as u64,
        };
        self.header_length() as u64
            + (cleartext_size / chunk) * self.encrypted_chunk() as u64
            + last
    }

    /// A header for a new file, and its stored form.
    pub fn new_header(&self) -> AppResult<(FileHeader, Vec<u8>)> {
        let mut nonce = vec![0u8; self.nonce_length()];
        fill_random(&mut nonce)?;
        let mut file_key = Zeroizing::new([0u8; KEY_LENGTH]);
        fill_random(file_key.as_mut())?;

        let mut stored = Vec::with_capacity(self.header_length());
        stored.extend_from_slice(&nonce);
        stored.extend_from_slice(&RESERVED);
        stored.extend_from_slice(file_key.as_ref());
        let payload = nonce.len()..stored.len();
        match &self.mode {
            Mode::Gcm { header_key } => {
                let tag = header_key
                    .seal_in_place_separate_tag(
                        gcm_nonce(&nonce)?,
                        Aad::empty(),
                        &mut stored[payload],
                    )
                    .map_err(|_| encryption_failed())?;
                stored.extend_from_slice(tag.as_ref());
            }
            Mode::CtrMac {
                encryption_key,
                mac_key,
            } => {
                apply_ctr(encryption_key, &nonce, &mut stored[payload]);
                let mac = hmac::sign(mac_key, &stored);
                stored.extend_from_slice(mac.as_ref());
            }
        }
        let header = FileHeader {
            nonce,
            key: self.file_key(&file_key),
        };
        Ok((header, stored))
    }

    /// Opens a stored header, which fails when it was not written with this vault's keys.
    pub fn decrypt_header(&self, stored: &[u8]) -> AppResult<FileHeader> {
        if stored.len() != self.header_length() {
            return Err(damaged("its header is cut short"));
        }
        let (nonce, sealed) = stored.split_at(self.nonce_length());
        let mut payload = Zeroizing::new(sealed.to_vec());
        match &self.mode {
            Mode::Gcm { header_key } => {
                header_key
                    .open_in_place(gcm_nonce(nonce)?, Aad::empty(), payload.as_mut_slice())
                    .map_err(|_| damaged("its header does not belong to this vault"))?;
            }
            Mode::CtrMac {
                encryption_key,
                mac_key,
            } => {
                let (signed, mac) = stored.split_at(stored.len() - MAC_LENGTH);
                hmac::verify(mac_key, signed, mac)
                    .map_err(|_| damaged("its header does not belong to this vault"))?;
                apply_ctr(encryption_key, nonce, &mut payload[..HEADER_PAYLOAD]);
            }
        }
        let mut file_key = Zeroizing::new([0u8; KEY_LENGTH]);
        file_key.copy_from_slice(&payload[RESERVED.len()..HEADER_PAYLOAD]);
        Ok(FileHeader {
            nonce: nonce.to_vec(),
            key: self.file_key(&file_key),
        })
    }

    fn file_key(&self, key: &Zeroizing<[u8; KEY_LENGTH]>) -> FileKey {
        match self.mode {
            Mode::Gcm { .. } => FileKey::Gcm(gcm_key(key)),
            Mode::CtrMac { .. } => FileKey::Ctr(key.clone()),
        }
    }

    /// Appends the stored form of the chunk at `chunk_number` to `stored`.
    pub fn encrypt_chunk(
        &self,
        header: &FileHeader,
        chunk_number: u64,
        cleartext: &[u8],
        stored: &mut Vec<u8>,
    ) -> AppResult<()> {
        debug_assert!(cleartext.len() <= CLEARTEXT_CHUNK);
        let start = stored.len();
        stored.resize(start + self.nonce_length(), 0);
        fill_random(&mut stored[start..])?;
        stored.extend_from_slice(cleartext);
        match (&self.mode, &header.key) {
            (Mode::Gcm { .. }, FileKey::Gcm(key)) => {
                let (nonce, contents) = stored[start..].split_at_mut(GCM_NONCE);
                let tag = key
                    .seal_in_place_separate_tag(
                        gcm_nonce(nonce)?,
                        Aad::from(gcm_binding(chunk_number, &header.nonce)),
                        contents,
                    )
                    .map_err(|_| encryption_failed())?;
                stored.extend_from_slice(tag.as_ref());
            }
            (Mode::CtrMac { mac_key, .. }, FileKey::Ctr(key)) => {
                let (nonce, contents) = stored[start..].split_at_mut(CTR_NONCE);
                apply_ctr(key, nonce, contents);
                let mac = chunk_mac(mac_key, &header.nonce, chunk_number, &stored[start..]);
                stored.extend_from_slice(mac.as_ref());
            }
            _ => unreachable!("a header is only used with the cryptor that made it"),
        }
        Ok(())
    }

    /// Appends the contents of the stored chunk at `chunk_number` to `cleartext`. Fails when
    /// the chunk was changed, cut short, or belongs to another file or place.
    pub fn decrypt_chunk(
        &self,
        header: &FileHeader,
        chunk_number: u64,
        stored: &[u8],
        cleartext: &mut Vec<u8>,
    ) -> AppResult<()> {
        if stored.len() < self.chunk_overhead() || stored.len() > self.encrypted_chunk() {
            return Err(damaged("it is cut short"));
        }
        let failed = || damaged("part of it was changed or is missing");
        let start = cleartext.len();
        match (&self.mode, &header.key) {
            (Mode::Gcm { .. }, FileKey::Gcm(key)) => {
                let (nonce, sealed) = stored.split_at(GCM_NONCE);
                cleartext.extend_from_slice(sealed);
                let length = key
                    .open_in_place(
                        gcm_nonce(nonce)?,
                        Aad::from(gcm_binding(chunk_number, &header.nonce)),
                        &mut cleartext[start..],
                    )
                    .map_err(|_| failed())?
                    .len();
                cleartext.truncate(start + length);
            }
            (Mode::CtrMac { mac_key, .. }, FileKey::Ctr(key)) => {
                let (signed, mac) = stored.split_at(stored.len() - MAC_LENGTH);
                let expected = chunk_mac(mac_key, &header.nonce, chunk_number, signed);
                if !same_mac(mac_key, expected.as_ref(), mac) {
                    return Err(failed());
                }
                let (nonce, contents) = signed.split_at(CTR_NONCE);
                cleartext.extend_from_slice(contents);
                apply_ctr(key, nonce, &mut cleartext[start..]);
            }
            _ => unreachable!("a header is only used with the cryptor that made it"),
        }
        Ok(())
    }
}

/// Boxed: the expanded key is much larger than anything else a cryptor or a header holds.
fn gcm_key(key: &[u8; KEY_LENGTH]) -> Box<LessSafeKey> {
    let key = UnboundKey::new(&AES_256_GCM, key).expect("the key is 32 bytes long");
    Box::new(LessSafeKey::new(key))
}

fn gcm_nonce(nonce: &[u8]) -> AppResult<Nonce> {
    Nonce::try_assume_unique_for_key(nonce).map_err(|_| damaged("a nonce has the wrong length"))
}

/// What ties a chunk to its place: the chunk's number and the header's nonce.
fn gcm_binding(chunk_number: u64, header_nonce: &[u8]) -> [u8; 8 + GCM_NONCE] {
    let mut binding = [0u8; 8 + GCM_NONCE];
    binding[..8].copy_from_slice(&chunk_number.to_be_bytes());
    binding[8..].copy_from_slice(header_nonce);
    binding
}

fn apply_ctr(key: &[u8; KEY_LENGTH], nonce: &[u8], data: &mut [u8]) {
    let mut nonce_block = [0u8; CTR_NONCE];
    nonce_block.copy_from_slice(nonce);
    AesCtr::new(key.into(), &nonce_block.into()).apply_keystream(data);
}

/// The HMAC over the header's nonce, the chunk's number, and the chunk's nonce and contents.
fn chunk_mac(
    mac_key: &hmac::Key,
    header_nonce: &[u8],
    chunk_number: u64,
    nonce_and_contents: &[u8],
) -> hmac::Tag {
    let mut context = hmac::Context::with_key(mac_key);
    context.update(header_nonce);
    context.update(&chunk_number.to_be_bytes());
    context.update(nonce_and_contents);
    context.sign()
}

/// Compares two MACs by signing each again, so the comparison takes the same time wherever
/// they differ.
fn same_mac(mac_key: &hmac::Key, computed: &[u8], stored: &[u8]) -> bool {
    hmac::verify(mac_key, computed, hmac::sign(mac_key, stored).as_ref()).is_ok()
}

fn damaged(problem: &str) -> AppError {
    AppError::integrity(format!("The encrypted file is damaged: {problem}"))
}

fn encryption_failed() -> AppError {
    AppError::new(ErrorKind::Io, "Encrypting failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMBOS: [CipherCombo; 2] = [CipherCombo::SivGcm, CipherCombo::SivCtrMac];

    fn cryptor(combo: CipherCombo) -> ContentCryptor {
        ContentCryptor::new(&Masterkey::generate().unwrap(), combo)
    }

    fn pattern(length: usize) -> Vec<u8> {
        (0..length).map(|index| (index * 31 + 7) as u8).collect()
    }

    #[test]
    fn stored_sizes_match_the_format() {
        let gcm = cryptor(CipherCombo::SivGcm);
        assert_eq!(gcm.header_length(), 68);
        assert_eq!(gcm.chunk_overhead(), 28);
        assert_eq!(gcm.encrypted_chunk(), 32796);
        let ctr = cryptor(CipherCombo::SivCtrMac);
        assert_eq!(ctr.header_length(), 88);
        assert_eq!(ctr.chunk_overhead(), 48);
        assert_eq!(ctr.encrypted_chunk(), 32816);
    }

    #[test]
    fn sizes_convert_both_ways() {
        for combo in COMBOS {
            let cryptor = cryptor(combo);
            let header = cryptor.header_length() as u64;
            let overhead = cryptor.chunk_overhead() as u64;
            assert_eq!(cryptor.encrypted_size(0), header);
            assert_eq!(cryptor.encrypted_size(1), header + 1 + overhead);
            assert_eq!(cryptor.encrypted_size(32768), header + 32768 + overhead);
            assert_eq!(cryptor.encrypted_size(32769), header + 32769 + 2 * overhead);
            for size in [
                0u64,
                1,
                100,
                32767,
                32768,
                32769,
                65536,
                100_000,
                5_000_000_000,
            ] {
                assert_eq!(cryptor.cleartext_size(cryptor.encrypted_size(size)), size);
            }
            // Cut short: less than a header, and a last chunk with nothing but overhead.
            assert_eq!(cryptor.cleartext_size(0), 0);
            assert_eq!(cryptor.cleartext_size(header - 1), 0);
            assert_eq!(cryptor.cleartext_size(header + overhead), 0);
            assert_eq!(cryptor.cleartext_size(header + overhead - 1), 0);
        }
    }

    #[test]
    fn headers_open_with_the_vaults_keys_only() {
        for combo in COMBOS {
            let cryptor = cryptor(combo);
            let (_, stored) = cryptor.new_header().unwrap();
            assert_eq!(stored.len(), cryptor.header_length());
            assert!(cryptor.decrypt_header(&stored).is_ok());
            assert!(self::cryptor(combo).decrypt_header(&stored).is_err());
            assert!(cryptor.decrypt_header(&stored[1..]).is_err());
            for index in [0, stored.len() / 2, stored.len() - 1] {
                let mut changed = stored.clone();
                changed[index] ^= 1;
                assert!(cryptor.decrypt_header(&changed).is_err(), "byte {index}");
            }
        }
    }

    #[test]
    fn chunks_come_back_through_the_stored_header() {
        for combo in COMBOS {
            let cryptor = cryptor(combo);
            let (header, stored_header) = cryptor.new_header().unwrap();
            let reopened = cryptor.decrypt_header(&stored_header).unwrap();
            for length in [1, 15, 16, 17, 1000, CLEARTEXT_CHUNK - 1, CLEARTEXT_CHUNK] {
                let contents = pattern(length);
                let mut stored = Vec::new();
                cryptor
                    .encrypt_chunk(&header, 3, &contents, &mut stored)
                    .unwrap();
                assert_eq!(stored.len(), length + cryptor.chunk_overhead());
                let mut read = Vec::new();
                cryptor
                    .decrypt_chunk(&reopened, 3, &stored, &mut read)
                    .unwrap();
                assert_eq!(read, contents);
            }
        }
    }

    #[test]
    fn chunks_append_to_what_the_buffers_hold() {
        for combo in COMBOS {
            let cryptor = cryptor(combo);
            let (header, _) = cryptor.new_header().unwrap();
            let mut stored = vec![9u8; 5];
            cryptor
                .encrypt_chunk(&header, 0, b"first", &mut stored)
                .unwrap();
            let first_end = stored.len();
            cryptor
                .encrypt_chunk(&header, 1, b"second", &mut stored)
                .unwrap();
            assert_eq!(&stored[..5], &[9u8; 5]);

            let mut read = b"kept ".to_vec();
            cryptor
                .decrypt_chunk(&header, 0, &stored[5..first_end], &mut read)
                .unwrap();
            cryptor
                .decrypt_chunk(&header, 1, &stored[first_end..], &mut read)
                .unwrap();
            assert_eq!(read, b"kept firstsecond");
        }
    }

    #[test]
    fn a_changed_or_misplaced_chunk_is_refused() {
        for combo in COMBOS {
            let cryptor = cryptor(combo);
            let (header, _) = cryptor.new_header().unwrap();
            let (other_file, _) = cryptor.new_header().unwrap();
            let mut stored = Vec::new();
            cryptor
                .encrypt_chunk(&header, 5, &pattern(500), &mut stored)
                .unwrap();

            let mut read = b"before".to_vec();
            let mut refused = |header: &FileHeader, number: u64, stored: &[u8]| {
                let error = cryptor
                    .decrypt_chunk(header, number, stored, &mut read)
                    .unwrap_err();
                assert_eq!(error.kind, ErrorKind::Integrity);
            };
            refused(&header, 4, &stored);
            refused(&other_file, 5, &stored);
            refused(&header, 5, &stored[..stored.len() - 1]);
            refused(&header, 5, &stored[..cryptor.chunk_overhead() - 1]);
            for index in [
                0,
                cryptor.nonce_length(),
                stored.len() / 2,
                stored.len() - 1,
            ] {
                let mut changed = stored.clone();
                changed[index] ^= 0x80;
                refused(&header, 5, &changed);
            }
            let too_long = vec![0u8; cryptor.encrypted_chunk() + 1];
            refused(&header, 5, &too_long);
        }
    }

    #[test]
    fn the_same_contents_never_store_the_same_way_twice() {
        for combo in COMBOS {
            let cryptor = cryptor(combo);
            let (header, _) = cryptor.new_header().unwrap();
            let mut first = Vec::new();
            let mut second = Vec::new();
            cryptor
                .encrypt_chunk(&header, 0, b"same", &mut first)
                .unwrap();
            cryptor
                .encrypt_chunk(&header, 0, b"same", &mut second)
                .unwrap();
            assert_ne!(first, second);
        }
    }
}
