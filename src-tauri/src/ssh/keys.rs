//! Private key loading for public key authentication.
//!
//! Accepts OpenSSH (`BEGIN OPENSSH PRIVATE KEY`), PEM/PKCS#1, PKCS#8 and PuTTY `.ppk`
//! v2 and v3 (Argon2i/d/id) files, encrypted or not, for RSA, Ed25519 and ECDSA
//! P-256/384/521 keys.
//!
//! Key files that went through Windows editors, mail clients or `git autocrlf` often
//! pick up a UTF-8 BOM, CRLF line endings or stripped trailing spaces (an empty PPK
//! `Comment: ` becomes `Comment:`). Other clients reject those files outright; we
//! normalize them before parsing. PPK MACs cover the decoded field values, not the
//! file's bytes, so normalization does not invalidate them.

use std::path::Path;

use russh::keys::{self, PrivateKey};

use crate::error::{AppError, AppResult, ErrorKind};

const PPK_PREFIX: &str = "PuTTY-User-Key-File-";

pub fn load_private_key(path: &Path, passphrase: Option<&str>) -> AppResult<PrivateKey> {
    let display = path.to_string_lossy().into_owned();
    let bytes = std::fs::read(path).map_err(|e| AppError::from(e).with_path(display.clone()))?;
    decode_private_key(&bytes, passphrase).map_err(|e| e.with_path(display))
}

pub fn decode_private_key(bytes: &[u8], passphrase: Option<&str>) -> AppResult<PrivateKey> {
    let passphrase = passphrase.filter(|p| !p.is_empty());
    let text = std::str::from_utf8(bytes)
        .map_err(|_| key_error("This file is not a private key (it is not text)"))?;
    let text = normalize(text);

    if text.starts_with(PPK_PREFIX) {
        return decode_ppk(&text, passphrase);
    }
    if text.starts_with("ssh-") || text.starts_with("ecdsa-") || text.starts_with("---- BEGIN SSH2 PUBLIC KEY") {
        return Err(key_error(
            "This is a public key. Select the matching private key file instead.",
        ));
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
        Err(e) => Err(key_error(format!("Could not read the private key: {e}"))),
    }
}

fn decode_ppk(text: &str, passphrase: Option<&str>) -> AppResult<PrivateKey> {
    let header = text.lines().next().unwrap_or_default();
    let (version, algorithm) = header[PPK_PREFIX.len()..]
        .split_once(':')
        .map(|(v, a)| (v.trim(), a.trim()))
        .ok_or_else(|| key_error("The PuTTY key file header is malformed"))?;
    match version {
        "2" | "3" => {}
        "1" => {
            return Err(key_error(
                "PuTTY key format version 1 is obsolete. Re-save the key with a current PuTTYgen.",
            ))
        }
        v => return Err(key_error(format!("Unsupported PuTTY key format version {v}"))),
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

    let encrypted = text
        .lines()
        .find_map(|l| l.strip_prefix("Encryption:"))
        .is_some_and(|v| v.trim() != "none");
    if encrypted && passphrase.is_none() {
        return Err(passphrase_required());
    }

    PrivateKey::from_ppk(text, passphrase.map(str::to_owned)).map_err(|e| {
        // `ssh_key`'s PPK error type is private; its MAC failure is the one case we need
        // to tell apart, since with encryption it means "wrong passphrase".
        let mac_failed = format!("{e:?}").contains("IncorrectMac");
        match (mac_failed, encrypted) {
            (true, true) => wrong_passphrase(),
            (true, false) => key_error("The PuTTY key file is corrupted (integrity check failed)"),
            _ => key_error(format!("Could not read the PuTTY key: {e}")),
        }
    })
}

/// Strips a BOM and leading blank lines, converts CRLF to LF, and restores the
/// `Key: value` separator PPK parsers require. The PPK comment is kept byte-exact
/// (it is covered by the MAC); other values and base64 lines are trimmed.
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
                out.push_str(if key.trim() == "Comment" { value } else { value.trim() });
            }
            // The PPK parser rejects blank lines, including a trailing one.
            _ if is_ppk && line.trim().is_empty() => continue,
            _ => out.push_str(line.trim_end()),
        }
        out.push('\n');
    }
    out
}

fn passphrase_required() -> AppError {
    AppError::new(
        ErrorKind::PassphraseRequired,
        "This key is protected by a passphrase. Enter it to continue.",
    )
}

fn wrong_passphrase() -> AppError {
    AppError::new(ErrorKind::PassphraseRequired, "The passphrase is incorrect.")
}

fn key_error(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::AuthFailed, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const PASS: &str = "poros-test";

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/keys").join(name)
    }

    /// The OpenSSH public key puttygen exported for the fixture, without the comment.
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
                    .unwrap_or_else(|e| panic!("{file}: {}", e.message));
                assert_eq!(public_of(&key), expected_public(name), "{file}");
            }
        }
    }

    #[test]
    fn ppk_v2_and_v3_encrypted_for_every_key_type() {
        for name in PPK_KEYS {
            for version in ["v2", "v3"] {
                let file = format!("{name}-{version}-enc.ppk");
                let key = load_private_key(&fixture(&file), Some(PASS))
                    .unwrap_or_else(|e| panic!("{file}: {}", e.message));
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
            let key = load_private_key(&fixture(file), Some(PASS))
                .unwrap_or_else(|e| panic!("{file}: {}", e.message));
            assert_eq!(public_of(&key), expected_public("ed25519"), "{file}");
        }
    }

    #[test]
    fn encrypted_ppk_without_passphrase_asks_for_one() {
        for file in ["rsa-v2-enc.ppk", "ed25519-v3-enc.ppk"] {
            let err = load_private_key(&fixture(file), None).unwrap_err();
            assert_eq!(err.kind, ErrorKind::PassphraseRequired, "{file}");
            let err = load_private_key(&fixture(file), Some("")).unwrap_err();
            assert_eq!(err.kind, ErrorKind::PassphraseRequired, "{file}");
        }
    }

    #[test]
    fn wrong_ppk_passphrase_is_reported_as_such() {
        for file in ["ecdsa256-v2-enc.ppk", "ed25519-v3-enc.ppk"] {
            let err = load_private_key(&fixture(file), Some("nope")).unwrap_err();
            assert_eq!(err.kind, ErrorKind::PassphraseRequired, "{file}");
            assert!(err.message.contains("incorrect"), "{file}: {}", err.message);
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
            let pass = file.contains("enc").then_some(PASS);
            let key = decode_private_key(mangled.as_bytes(), pass)
                .unwrap_or_else(|e| panic!("{file}: {}", e.message));
            assert_eq!(public_of(&key), expected_public(name), "{file}");
        }
    }

    #[test]
    fn tampered_ppk_is_reported_as_corrupted() {
        let original = std::fs::read_to_string(fixture("ed25519-v3-plain.ppk")).unwrap();
        let tampered = original.replace("Comment: poros-ed25519", "Comment: tampered");
        let err = decode_private_key(tampered.as_bytes(), None).unwrap_err();
        assert!(err.message.contains("corrupted"), "{}", err.message);
    }

    #[test]
    fn unsupported_ppk_types_get_clear_errors() {
        let err = load_private_key(&fixture("dsa-v3-plain.ppk"), None).unwrap_err();
        assert!(err.message.contains("DSA"), "{}", err.message);
        let err = load_private_key(&fixture("ed448-v3-plain.ppk"), None).unwrap_err();
        assert!(err.message.contains("Ed448"), "{}", err.message);
        let err = decode_private_key(b"PuTTY-User-Key-File-1: ssh-rsa\n", None).unwrap_err();
        assert!(err.message.contains("version 1"), "{}", err.message);
    }

    #[test]
    fn openssh_pem_and_pkcs8_keys() {
        for (file, pass) in [
            ("openssh-ed25519", None),
            ("openssh-ed25519-enc", Some(PASS)),
            ("pem-rsa", None),
            ("pem-rsa-enc", Some(PASS)),
            ("pkcs8-ecdsa", None),
        ] {
            let key = load_private_key(&fixture(file), pass)
                .unwrap_or_else(|e| panic!("{file}: {}", e.message));
            assert_eq!(public_of(&key), expected_public(file), "{file}");
        }
    }

    #[test]
    fn encrypted_openssh_keys_ask_for_passphrase() {
        for file in ["openssh-ed25519-enc", "pem-rsa-enc"] {
            let err = load_private_key(&fixture(file), None).unwrap_err();
            assert_eq!(err.kind, ErrorKind::PassphraseRequired, "{file}");
            let err = load_private_key(&fixture(file), Some("nope")).unwrap_err();
            assert_eq!(err.kind, ErrorKind::PassphraseRequired, "{file}");
        }
    }

    #[test]
    fn public_key_file_is_rejected_with_hint() {
        let err = load_private_key(&fixture("ed25519.pub"), None).unwrap_err();
        assert!(err.message.contains("public key"), "{}", err.message);
    }
}
