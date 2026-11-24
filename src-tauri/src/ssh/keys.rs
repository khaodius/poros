//! Key files that passed through Windows editors or mail clients often carry a BOM, CRLF,
//! blank lines or a stripped `Comment: ` space, which other clients reject. They are
//! normalized before parsing; PPK MACs cover decoded values, so this keeps them valid.

use std::io::Read;
use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use pkcs8::pkcs5::pbes2;
use pkcs8::EncryptedPrivateKeyInfoRef;
use russh::keys::ssh_key::Kdf;
use russh::keys::{self, PrivateKey};

use crate::error::{AppError, AppResult, ErrorKind};

const PPK_PREFIX: &str = "PuTTY-User-Key-File-";
/// Private keys are a few kilobytes long.
const MAX_KEY_FILE_BYTES: u64 = 1024 * 1024;

// A key file sets how much memory and time turning its passphrase into a key takes, so a
// crafted one could exhaust the computer's memory or keep a processor busy for hours. Keys made
// by PuTTYgen, ssh-keygen or OpenSSL stay far below these limits; reaching one takes about a
// minute.

/// The memory a key derivation may hold at once, as for Cryptomator vaults.
const MAX_DERIVATION_MEMORY: u64 = 1 << 30;
/// The memory a key derivation may go through in all, counted once per pass over it.
const MAX_DERIVATION_WORK: u64 = 64 << 30;
const MAX_BCRYPT_ROUNDS: u32 = 10_000;
const MAX_PBKDF2_ITERATIONS: u32 = pbes2::Pbkdf2Params::MAX_ITERATION_COUNT;

/// Reads and decodes a key file. Deriving the key from a passphrase can take a second or more,
/// so this belongs on a blocking thread.
pub fn load_private_key(path: &Path, passphrase: Option<&str>) -> AppResult<PrivateKey> {
    let display = path.to_string_lossy().into_owned();
    let bytes = read_key_file(path).map_err(|error| error.with_path(display.clone()))?;
    decode_private_key(&bytes, passphrase).map_err(|error| error.with_path(display))
}

fn read_key_file(path: &Path) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_KEY_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_KEY_FILE_BYTES {
        return Err(key_error("This file is too large to be a private key"));
    }
    Ok(bytes)
}

pub fn decode_private_key(bytes: &[u8], passphrase: Option<&str>) -> AppResult<PrivateKey> {
    let passphrase = passphrase.filter(|value| !value.is_empty());
    let text = std::str::from_utf8(bytes)
        .map_err(|_| key_error("This file is not a private key (it is not text)"))?;
    let text = normalize(text);

    if text.starts_with(PPK_PREFIX) {
        return decode_ppk(&text, passphrase);
    }
    if text.starts_with("ssh-")
        || text.starts_with("ecdsa-")
        || text.starts_with("---- BEGIN SSH2 PUBLIC KEY")
    {
        return Err(key_error(
            "This is a public key. Select the matching private key file instead.",
        ));
    }

    match pem_contents(&text) {
        Some(("OPENSSH PRIVATE KEY", contents)) => check_bcrypt_cost(&contents)?,
        // With a passphrase, `russh` reads each of these as an encrypted PKCS#8 key.
        Some(("ENCRYPTED PRIVATE KEY" | "PRIVATE KEY" | "EC PRIVATE KEY", contents)) => {
            check_pbes2_cost(&contents)?
        }
        _ => {}
    }
    match keys::decode_secret_key(&text, passphrase) {
        Ok(key) => Ok(key),
        Err(keys::Error::KeyIsEncrypted) => Err(passphrase_required()),
        // A failure with a passphrase on a key that needs one means the passphrase is wrong.
        Err(_)
            if passphrase.is_some()
                && matches!(
                    keys::decode_secret_key(&text, None),
                    Err(keys::Error::KeyIsEncrypted)
                ) =>
        {
            Err(wrong_passphrase())
        }
        Err(error) => Err(key_error(format!(
            "Could not read the private key: {error}"
        ))),
    }
}

fn decode_ppk(text: &str, passphrase: Option<&str>) -> AppResult<PrivateKey> {
    let header = text.lines().next().unwrap_or_default();
    let (version, algorithm) = header[PPK_PREFIX.len()..]
        .split_once(':')
        .map(|(version, algorithm)| (version.trim(), algorithm.trim()))
        .ok_or_else(|| key_error("The PuTTY key file header is malformed"))?;
    match version {
        "2" | "3" => {}
        "1" => {
            return Err(key_error(
                "PuTTY key format version 1 is obsolete. Re-save the key with a current PuTTYgen.",
            ))
        }
        other => {
            return Err(key_error(format!(
                "Unsupported PuTTY key format version {other}"
            )))
        }
    }
    match algorithm {
        "ssh-dss" => {
            return Err(key_error(
                "DSA keys are no longer supported by OpenSSH servers. Generate an Ed25519 key.",
            ))
        }
        "ssh-ed448" => return Err(key_error("Ed448 keys are not supported yet")),
        _ => {}
    }
    check_argon2_cost(text)?;

    let encrypted = text
        .lines()
        .find_map(|line| line.strip_prefix("Encryption:"))
        .is_some_and(|value| value.trim() != "none");
    if encrypted && passphrase.is_none() {
        return Err(passphrase_required());
    }

    PrivateKey::from_ppk(text, passphrase.map(str::to_owned)).map_err(|error| {
        // `ssh_key`'s PPK error type is private; its MAC failure is the one case we need
        // to tell apart, since with encryption it means "wrong passphrase".
        let mac_failed = format!("{error:?}").contains("IncorrectMac");
        match (mac_failed, encrypted) {
            (true, true) => wrong_passphrase(),
            (true, false) => key_error("The PuTTY key file is corrupted (integrity check failed)"),
            _ => key_error(format!("Could not read the PuTTY key: {error}")),
        }
    })
}

/// The PPK comment stays byte-exact because the MAC covers it.
fn normalize(text: &str) -> String {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let is_ppk = text.starts_with(PPK_PREFIX);
    let mut out = String::with_capacity(text.len() + 16);
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        // Base64 lines never contain ':', so this only touches `Key: value` lines.
        match line.split_once(':') {
            Some((key, value)) if is_ppk => {
                let value = value.strip_prefix(' ').unwrap_or(value);
                out.push_str(key.trim());
                out.push_str(": ");
                out.push_str(if key.trim() == "Comment" {
                    value
                } else {
                    value.trim()
                });
            }
            // The PPK parser rejects blank lines, including a trailing one.
            _ if is_ppk && line.trim().is_empty() => continue,
            _ => out.push_str(line.trim_end()),
        }
        out.push('\n');
    }
    out
}

/// The parser keeps one of the values a file gives each setting, so every value counts.
fn check_argon2_cost(text: &str) -> AppResult<()> {
    let largest = |setting: &str| {
        text.lines()
            .filter_map(|line| line.strip_prefix(setting)?.strip_prefix(": ")?.parse().ok())
            .max()
            .unwrap_or(0u64)
    };
    let memory = largest("Argon2-Memory").saturating_mul(1024);
    check_derivation(memory, memory.saturating_mul(largest("Argon2-Passes")))
}

fn check_bcrypt_cost(contents: &[u8]) -> AppResult<()> {
    match PrivateKey::from_bytes(contents) {
        Ok(key) if matches!(key.kdf(), Kdf::Bcrypt { rounds, .. } if *rounds > MAX_BCRYPT_ROUNDS) => {
            Err(takes_too_long())
        }
        _ => Ok(()),
    }
}

fn check_pbes2_cost(contents: &[u8]) -> AppResult<()> {
    let Ok(key) = EncryptedPrivateKeyInfoRef::try_from(contents) else {
        return Ok(());
    };
    match key
        .encryption_algorithm
        .pbes2()
        .map(|parameters| &parameters.kdf)
    {
        Some(pbes2::Kdf::Pbkdf2(pbkdf2)) if pbkdf2.iteration_count > MAX_PBKDF2_ITERATIONS => {
            Err(takes_too_long())
        }
        Some(pbes2::Kdf::Scrypt(scrypt)) => {
            let memory = scrypt
                .cost_parameter
                .saturating_mul(u64::from(scrypt.block_size))
                .saturating_mul(128);
            check_derivation(
                memory,
                memory.saturating_mul(u64::from(scrypt.parallelization)),
            )
        }
        _ => Ok(()),
    }
}

/// `memory` is what a key derivation holds at once, `work` what it goes through in all.
fn check_derivation(memory: u64, work: u64) -> AppResult<()> {
    if memory > MAX_DERIVATION_MEMORY {
        return Err(key_error(format!(
            "Unlocking this key takes {} MiB of memory, more than Poros allows. Re-save the key with the usual settings.",
            memory >> 20
        )));
    }
    if work > MAX_DERIVATION_WORK {
        return Err(takes_too_long());
    }
    Ok(())
}

fn takes_too_long() -> AppError {
    key_error(
        "Unlocking this key takes far longer than Poros allows. Re-save the key with the usual settings.",
    )
}

/// The labels of the PEM files `russh` decodes.
const PEM_LABELS: [&str; 5] = [
    "OPENSSH PRIVATE KEY",
    "RSA PRIVATE KEY",
    "ENCRYPTED PRIVATE KEY",
    "PRIVATE KEY",
    "EC PRIVATE KEY",
];

/// The label and contents of a PEM key, read the way `russh` reads them so that the limits
/// above see the bytes it decodes: from the first begin line it knows to the next end line,
/// keeping the lines that are all base64, under the label of the last begin line on the way.
/// Its decoder lets padding end any group of four characters.
fn pem_contents(text: &str) -> Option<(&'static str, Vec<u8>)> {
    let mut label = None;
    let mut encoded = String::new();
    for line in text.lines() {
        if label.is_some() {
            if line.starts_with("-----END ") {
                break;
            }
            if line.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=')
            }) {
                encoded.push_str(line);
            }
        }
        let begins = line
            .strip_prefix("-----BEGIN ")
            .and_then(|rest| rest.strip_suffix("-----"));
        if let Some(known) = PEM_LABELS.into_iter().find(|known| begins == Some(*known)) {
            label = Some(known);
        }
    }
    let mut contents = Vec::new();
    for group in encoded.as_bytes().chunks(4) {
        contents.extend(STANDARD.decode(group).ok()?);
    }
    Some((label?, contents))
}

fn passphrase_required() -> AppError {
    AppError::new(
        ErrorKind::PassphraseRequired,
        "This key is protected by a passphrase. Enter it to continue.",
    )
}

fn wrong_passphrase() -> AppError {
    AppError::new(
        ErrorKind::PassphraseRequired,
        "The passphrase is incorrect.",
    )
}

fn key_error(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::AuthFailed, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use pkcs8::der::{Decode, Encode};
    use pkcs8::pkcs5::{pbes2, EncryptionScheme};
    use pkcs8::EncryptedPrivateKeyInfoOwned;
    use std::path::PathBuf;

    const PASSPHRASE: &str = "poros-test";

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/keys")
            .join(name)
    }

    fn expected_public(name: &str) -> String {
        let text = std::fs::read_to_string(fixture(&format!("{name}.pub"))).unwrap();
        let mut parts = text.split_whitespace();
        format!("{} {}", parts.next().unwrap(), parts.next().unwrap())
    }

    fn public_of(key: &PrivateKey) -> String {
        let text = key.public_key().to_openssh().unwrap();
        let mut parts = text.split_whitespace();
        format!("{} {}", parts.next().unwrap(), parts.next().unwrap())
    }

    const PPK_KEYS: [&str; 5] = ["rsa", "ed25519", "ecdsa256", "ecdsa384", "ecdsa521"];

    #[test]
    fn ppk_v2_and_v3_plain_for_every_key_type() {
        for name in PPK_KEYS {
            for version in ["v2", "v3"] {
                let file = format!("{name}-{version}-plain.ppk");
                let key = load_private_key(&fixture(&file), None)
                    .unwrap_or_else(|error| panic!("{file}: {}", error.message));
                assert_eq!(public_of(&key), expected_public(name), "{file}");
            }
        }
    }

    #[test]
    fn ppk_v2_and_v3_encrypted_for_every_key_type() {
        for name in PPK_KEYS {
            for version in ["v2", "v3"] {
                let file = format!("{name}-{version}-enc.ppk");
                let key = load_private_key(&fixture(&file), Some(PASSPHRASE))
                    .unwrap_or_else(|error| panic!("{file}: {}", error.message));
                assert_eq!(public_of(&key), expected_public(name), "{file}");
            }
        }
    }

    #[test]
    fn ppk_v3_every_argon2_variant() {
        for file in [
            "ed25519-v3-enc.ppk",
            "ed25519-v3-argon2i-enc.ppk",
            "ed25519-v3-argon2d-enc.ppk",
            // puttygen's default (calibrated) Argon2id parameters.
            "ed25519-v3-default-kdf-enc.ppk",
        ] {
            let key = load_private_key(&fixture(file), Some(PASSPHRASE))
                .unwrap_or_else(|error| panic!("{file}: {}", error.message));
            assert_eq!(public_of(&key), expected_public("ed25519"), "{file}");
        }
    }

    #[test]
    fn encrypted_ppk_without_passphrase_asks_for_one() {
        for file in ["rsa-v2-enc.ppk", "ed25519-v3-enc.ppk"] {
            let error = load_private_key(&fixture(file), None).unwrap_err();
            assert_eq!(error.kind, ErrorKind::PassphraseRequired, "{file}");
            let error = load_private_key(&fixture(file), Some("")).unwrap_err();
            assert_eq!(error.kind, ErrorKind::PassphraseRequired, "{file}");
        }
    }

    #[test]
    fn wrong_ppk_passphrase_is_reported_as_such() {
        for file in ["ecdsa256-v2-enc.ppk", "ed25519-v3-enc.ppk"] {
            let error = load_private_key(&fixture(file), Some("nope")).unwrap_err();
            assert_eq!(error.kind, ErrorKind::PassphraseRequired, "{file}");
            assert!(
                error.message.contains("incorrect"),
                "{file}: {}",
                error.message
            );
        }
    }

    #[test]
    fn ppk_survives_bom_crlf_and_stripped_trailing_space() {
        // nocomment-v3-plain.ppk has `Comment: ` with a trailing space, which editors strip.
        for (file, name) in [
            ("nocomment-v3-plain.ppk", "nocomment"),
            ("ed25519-v3-enc.ppk", "ed25519"),
            ("rsa-v2-enc.ppk", "rsa"),
        ] {
            let original = std::fs::read_to_string(fixture(file)).unwrap();
            let mangled = format!(
                "\u{feff}{}",
                original
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join("\r\n")
            );
            assert!(file != "nocomment-v3-plain.ppk" || mangled.contains("Comment:\r\n"));
            let passphrase = file.contains("enc").then_some(PASSPHRASE);
            let key = decode_private_key(mangled.as_bytes(), passphrase)
                .unwrap_or_else(|error| panic!("{file}: {}", error.message));
            assert_eq!(public_of(&key), expected_public(name), "{file}");
        }
    }

    #[test]
    fn tampered_ppk_is_reported_as_corrupted() {
        let original = std::fs::read_to_string(fixture("ed25519-v3-plain.ppk")).unwrap();
        let tampered = original.replace("Comment: poros-ed25519", "Comment: tampered");
        let error = decode_private_key(tampered.as_bytes(), None).unwrap_err();
        assert!(error.message.contains("corrupted"), "{}", error.message);
    }

    #[test]
    fn unsupported_ppk_types_get_clear_errors() {
        let error = load_private_key(&fixture("dsa-v3-plain.ppk"), None).unwrap_err();
        assert!(error.message.contains("DSA"), "{}", error.message);
        let error = load_private_key(&fixture("ed448-v3-plain.ppk"), None).unwrap_err();
        assert!(error.message.contains("Ed448"), "{}", error.message);
        let error = decode_private_key(b"PuTTY-User-Key-File-1: ssh-rsa\n", None).unwrap_err();
        assert!(error.message.contains("version 1"), "{}", error.message);
    }

    #[test]
    fn openssh_pem_and_pkcs8_keys() {
        for (file, pass) in [
            ("openssh-ed25519", None),
            ("openssh-ed25519-enc", Some(PASSPHRASE)),
            ("pem-rsa", None),
            ("pem-rsa-enc", Some(PASSPHRASE)),
            ("pkcs8-ecdsa", None),
        ] {
            let key = load_private_key(&fixture(file), pass)
                .unwrap_or_else(|error| panic!("{file}: {}", error.message));
            assert_eq!(public_of(&key), expected_public(file), "{file}");
        }
    }

    #[test]
    fn encrypted_openssh_keys_ask_for_passphrase() {
        for file in ["openssh-ed25519-enc", "pem-rsa-enc"] {
            let error = load_private_key(&fixture(file), None).unwrap_err();
            assert_eq!(error.kind, ErrorKind::PassphraseRequired, "{file}");
            let error = load_private_key(&fixture(file), Some("nope")).unwrap_err();
            assert_eq!(error.kind, ErrorKind::PassphraseRequired, "{file}");
        }
    }

    #[test]
    fn public_key_file_is_rejected_with_hint() {
        let error = load_private_key(&fixture("ed25519.pub"), None).unwrap_err();
        assert!(error.message.contains("public key"), "{}", error.message);
    }

    #[test]
    fn encrypted_pkcs8_keys_with_pbkdf2_or_scrypt() {
        for file in ["pkcs8-ecdsa-enc", "pkcs8-ecdsa-scrypt-enc"] {
            let key = load_private_key(&fixture(file), Some(PASSPHRASE))
                .unwrap_or_else(|error| panic!("{file}: {}", error.message));
            assert_eq!(public_of(&key), expected_public("pkcs8-ecdsa"), "{file}");
        }
    }

    /// Decodes on a thread of its own, so that without its limit a test fails after seconds
    /// instead of deriving a key for hours.
    fn refused_in_time(text: String) -> AppError {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            sender
                .send(decode_private_key(text.as_bytes(), Some(PASSPHRASE)))
                .ok()
        });
        match receiver.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Ok(_)) => panic!("the key was decoded"),
            Ok(Err(error)) => error,
            Err(_) => panic!("the key was still being derived from its passphrase after 10s"),
        }
    }

    fn assert_refused_for_its_cost(error: &AppError, what: &str) {
        assert_eq!(
            error.kind,
            ErrorKind::AuthFailed,
            "{what}: {}",
            error.message
        );
        assert!(
            error.message.contains("than Poros allows"),
            "{what}: {}",
            error.message
        );
    }

    #[test]
    fn ppk_argon2_settings_beyond_any_real_key_are_refused_before_deriving() {
        let original = std::fs::read_to_string(fixture("ed25519-v3-enc.ppk")).unwrap();
        for (setting, crafted) in [
            ("Argon2-Memory: 1024", "Argon2-Memory: 4294967295"),
            ("Argon2-Passes: 2", "Argon2-Passes: 4294967295"),
        ] {
            assert!(original.contains(setting), "{setting}");
            let error = refused_in_time(original.replace(setting, crafted));
            assert_refused_for_its_cost(&error, crafted);
        }
    }

    /// The contents of a key file that sits between its begin and end lines.
    fn pem_body(text: &str) -> Vec<u8> {
        let encoded: String = text
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        STANDARD.decode(encoded).unwrap()
    }

    fn pem(label: &str, contents: &[u8]) -> String {
        let encoded = STANDARD.encode(contents);
        format!("-----BEGIN {label}-----\n{encoded}\n-----END {label}-----\n")
    }

    #[test]
    fn openssh_bcrypt_rounds_beyond_any_real_key_are_refused_before_deriving() {
        let original = std::fs::read_to_string(fixture("openssh-ed25519-enc")).unwrap();
        let mut contents = pem_body(&original);
        // After the format name, the cipher name `aes256-ctr` and the KDF name `bcrypt`, the
        // KDF options hold a 16 byte salt and then the rounds.
        let rounds = "openssh-key-v1\0".len() + (4 + 10) + (4 + 6) + 4 + (4 + 16);
        assert_eq!(contents[rounds..rounds + 4], 24u32.to_be_bytes());
        contents[rounds..rounds + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        let error = refused_in_time(pem("OPENSSH PRIVATE KEY", &contents));
        assert_refused_for_its_cost(&error, "bcrypt rounds");
    }

    /// An encrypted PKCS#8 fixture with its key derivation settings changed.
    fn pkcs8_key_with(file: &str, change: impl FnOnce(&mut pbes2::Kdf)) -> String {
        let original = std::fs::read_to_string(fixture(file)).unwrap();
        let mut key = EncryptedPrivateKeyInfoOwned::from_der(&pem_body(&original)).unwrap();
        let EncryptionScheme::Pbes2(parameters) = &mut key.encryption_algorithm else {
            panic!("{file} is not encrypted with PBES2");
        };
        change(&mut parameters.kdf);
        pem("ENCRYPTED PRIVATE KEY", &key.to_der().unwrap())
    }

    #[test]
    fn pkcs8_derivation_settings_beyond_any_real_key_are_refused_before_deriving() {
        let scrypt = |change: fn(&mut pbes2::ScryptParams)| {
            pkcs8_key_with("pkcs8-ecdsa-scrypt-enc", |kdf| {
                let pbes2::Kdf::Scrypt(parameters) = kdf else {
                    panic!("not scrypt");
                };
                change(parameters);
            })
        };
        let pbkdf2 = pkcs8_key_with("pkcs8-ecdsa-enc", |kdf| {
            let pbes2::Kdf::Pbkdf2(parameters) = kdf else {
                panic!("not PBKDF2");
            };
            parameters.iteration_count = u32::MAX;
        });
        for (what, text) in [
            (
                "scrypt memory",
                scrypt(|parameters| parameters.cost_parameter = 1 << 40),
            ),
            (
                "scrypt lanes",
                scrypt(|parameters| parameters.parallelization = u16::MAX),
            ),
            ("PBKDF2 iterations", pbkdf2),
        ] {
            assert_refused_for_its_cost(&refused_in_time(text), what);
        }
    }

    #[test]
    fn files_far_larger_than_a_key_are_refused() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("padded");
        let key = std::fs::read_to_string(fixture("openssh-ed25519")).unwrap();
        // The decoder skips whatever comes before the key, so only the size gives it away.
        std::fs::write(&path, format!("{}{key}", "padding\n".repeat(200_000))).unwrap();
        let error = load_private_key(&path, None).unwrap_err();
        assert_eq!(error.kind, ErrorKind::AuthFailed, "{}", error.message);
        assert!(error.message.contains("too large"), "{}", error.message);
    }
}
