//! The two keys of a vault and the `masterkey.cryptomator` file that keeps them, each wrapped
//! (RFC 3394) with a key that scrypt derives from the vault's password.

use aes_kw::{KeyInit, KwAes256};
use base64::engine::general_purpose::STANDARD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::{alphabet, Engine};
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult, ErrorKind};

pub const KEY_LENGTH: usize = 32;
const WRAPPED_KEY_LENGTH: usize = KEY_LENGTH + 8;
const SALT_LENGTH: usize = 8;
const SCRYPT_COST: u64 = 1 << 15;
const SCRYPT_BLOCK_SIZE: u32 = 8;
/// What Cryptomator writes as the file's version since the vault's format moved to
/// `vault.cryptomator`.
const FILE_VERSION: u32 = 999;
/// A masterkey file whose scrypt parameters need more memory than this is refused, so a
/// planted file cannot exhaust the computer's memory.
const MAX_SCRYPT_MEMORY: u64 = 1 << 30;

/// Reads base64 with or without its padding.
const LENIENT_BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

pub struct Masterkey {
    encryption: Zeroizing<[u8; KEY_LENGTH]>,
    mac: Zeroizing<[u8; KEY_LENGTH]>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MasterkeyFile {
    #[serde(default)]
    version: u32,
    scrypt_salt: String,
    scrypt_cost_param: u64,
    scrypt_block_size: u32,
    primary_master_key: String,
    hmac_master_key: String,
    #[serde(default)]
    version_mac: String,
}

impl Masterkey {
    pub fn generate() -> AppResult<Self> {
        let mut encryption = Zeroizing::new([0u8; KEY_LENGTH]);
        let mut mac = Zeroizing::new([0u8; KEY_LENGTH]);
        fill_random(encryption.as_mut())?;
        fill_random(mac.as_mut())?;
        Ok(Self { encryption, mac })
    }

    pub fn encryption_key(&self) -> &[u8; KEY_LENGTH] {
        &self.encryption
    }

    pub fn mac_key(&self) -> &[u8; KEY_LENGTH] {
        &self.mac
    }

    /// Both keys, encryption key first: what signs `vault.cryptomator`.
    pub fn raw(&self) -> Zeroizing<Vec<u8>> {
        let mut raw = Zeroizing::new(Vec::with_capacity(KEY_LENGTH * 2));
        raw.extend_from_slice(self.encryption.as_ref());
        raw.extend_from_slice(self.mac.as_ref());
        raw
    }

    /// Opens the contents of a `masterkey.cryptomator` file. Takes as long as scrypt does, so
    /// it belongs on a blocking thread.
    ///
    /// The Cryptomator apps compose the password as it is typed (NFC), but their library takes
    /// it as it comes, so a vault made by another program may be locked with the same
    /// characters in another form. Each form of the password is tried.
    pub fn unlock(file: &[u8], password: &str) -> AppResult<Self> {
        let file: MasterkeyFile = serde_json::from_slice(file)
            .map_err(|error| damaged(format!("it is not a masterkey file ({error})")))?;
        let salt = decode(&file.scrypt_salt, "scryptSalt")?;
        let wrapped_encryption = decode(&file.primary_master_key, "primaryMasterKey")?;
        let wrapped_mac = decode(&file.hmac_master_key, "hmacMasterKey")?;
        let mut forms: Vec<Zeroizing<String>> = Vec::with_capacity(3);
        for form in [
            password.nfc().collect(),
            password.to_string(),
            password.nfd().collect(),
        ] {
            let form = Zeroizing::new(form);
            if !forms.contains(&form) {
                forms.push(form);
            }
        }
        let mut refused = None;
        for form in forms {
            let wrapping_key =
                derive_wrapping_key(&form, &salt, file.scrypt_cost_param, file.scrypt_block_size)?;
            let wrap = key_wrap(&wrapping_key);
            match unwrap(&wrap, &wrapped_encryption) {
                Ok(encryption) => {
                    return Ok(Self {
                        encryption,
                        mac: unwrap(&wrap, &wrapped_mac)?,
                    })
                }
                Err(error) if error.kind == ErrorKind::WrongPassword => refused = Some(error),
                Err(error) => return Err(error),
            }
        }
        Err(refused.unwrap_or_else(wrong_password))
    }

    /// The contents of a `masterkey.cryptomator` file that `password` opens. Takes as long as
    /// scrypt does.
    pub fn lock(&self, password: &str) -> AppResult<Vec<u8>> {
        let mut salt = [0u8; SALT_LENGTH];
        fill_random(&mut salt)?;
        let composed: Zeroizing<String> = Zeroizing::new(password.nfc().collect());
        let wrapping_key = derive_wrapping_key(&composed, &salt, SCRYPT_COST, SCRYPT_BLOCK_SIZE)?;
        let wrap = key_wrap(&wrapping_key);
        let wrapped = |key: &[u8; KEY_LENGTH]| -> AppResult<String> {
            let mut buffer = [0u8; WRAPPED_KEY_LENGTH];
            wrap.wrap_key(key, &mut buffer)
                .map_err(|error| AppError::new(ErrorKind::Io, error.to_string()))?;
            Ok(STANDARD.encode(buffer))
        };
        let version_mac = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, self.mac.as_ref()),
            &FILE_VERSION.to_be_bytes(),
        );
        let file = MasterkeyFile {
            version: FILE_VERSION,
            scrypt_salt: STANDARD.encode(salt),
            scrypt_cost_param: SCRYPT_COST,
            scrypt_block_size: SCRYPT_BLOCK_SIZE,
            primary_master_key: wrapped(&self.encryption)?,
            hmac_master_key: wrapped(&self.mac)?,
            version_mac: STANDARD.encode(version_mac),
        };
        serde_json::to_vec_pretty(&file)
            .map_err(|error| AppError::new(ErrorKind::Io, error.to_string()))
    }
}

pub(super) fn fill_random(bytes: &mut [u8]) -> AppResult<()> {
    SystemRandom::new()
        .fill(bytes)
        .map_err(|_| AppError::new(ErrorKind::Io, "The system's random number generator failed"))
}

fn damaged(problem: String) -> AppError {
    AppError::integrity(format!("The vault's masterkey file is damaged: {problem}"))
}

fn decode(encoded: &str, field: &str) -> AppResult<Vec<u8>> {
    LENIENT_BASE64
        .decode(encoded.trim())
        .map_err(|error| damaged(format!("{field} is not base64 ({error})")))
}

fn derive_wrapping_key(
    password: &str,
    salt: &[u8],
    cost: u64,
    block_size: u32,
) -> AppResult<Zeroizing<[u8; KEY_LENGTH]>> {
    if cost < 2 || !cost.is_power_of_two() || block_size == 0 {
        return Err(damaged(format!(
            "scrypt cost {cost} with block size {block_size} is not valid"
        )));
    }
    let memory = cost
        .saturating_mul(u64::from(block_size))
        .saturating_mul(128);
    if memory > MAX_SCRYPT_MEMORY {
        return Err(AppError::unsupported(format!(
            "Opening this vault takes {} MiB of memory to derive its key, more than Poros allows",
            memory / (1024 * 1024)
        )));
    }
    let parameters = scrypt::Params::new(cost.trailing_zeros() as u8, block_size, 1)
        .map_err(|error| damaged(format!("its scrypt parameters are not valid ({error})")))?;
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    scrypt::scrypt(password.as_bytes(), salt, &parameters, key.as_mut())
        .map_err(|error| AppError::new(ErrorKind::Io, error.to_string()))?;
    Ok(key)
}

fn key_wrap(wrapping_key: &[u8; KEY_LENGTH]) -> KwAes256 {
    KwAes256::new(wrapping_key.into())
}

/// A key that does not unwrap means the password is wrong: the wrapping carries its own check.
fn unwrap(wrap: &KwAes256, wrapped: &[u8]) -> AppResult<Zeroizing<[u8; KEY_LENGTH]>> {
    if wrapped.len() != WRAPPED_KEY_LENGTH {
        return Err(damaged(format!(
            "a wrapped key is {} bytes long instead of {WRAPPED_KEY_LENGTH}",
            wrapped.len()
        )));
    }
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    wrap.unwrap_key(wrapped, key.as_mut())
        .map_err(|_| wrong_password())?;
    Ok(key)
}

fn wrong_password() -> AppError {
    AppError::new(ErrorKind::WrongPassword, "Wrong password")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locked_key_opens_with_its_password_only() {
        let masterkey = Masterkey::generate().unwrap();
        let file = masterkey.lock("correct horse").unwrap();

        let opened = Masterkey::unlock(&file, "correct horse").unwrap();
        assert_eq!(opened.encryption_key(), masterkey.encryption_key());
        assert_eq!(opened.mac_key(), masterkey.mac_key());

        let refused = Masterkey::unlock(&file, "correct horse ").err().unwrap();
        assert_eq!(refused.kind, ErrorKind::WrongPassword);
    }

    #[test]
    fn the_file_has_the_fields_cryptomator_writes() {
        let file = Masterkey::generate().unwrap().lock("password").unwrap();
        let json: serde_json::Value = serde_json::from_slice(&file).unwrap();
        assert_eq!(json["version"], 999);
        assert_eq!(json["scryptCostParam"], 32768);
        assert_eq!(json["scryptBlockSize"], 8);
        let length = |field: &str| {
            STANDARD
                .decode(json[field].as_str().unwrap())
                .unwrap()
                .len()
        };
        assert_eq!(length("scryptSalt"), 8);
        assert_eq!(length("primaryMasterKey"), 40);
        assert_eq!(length("hmacMasterKey"), 40);
        assert_eq!(length("versionMac"), 32);
    }

    #[test]
    fn composed_and_decomposed_passwords_are_the_same_password() {
        let masterkey = Masterkey::generate().unwrap();
        let file = masterkey.lock("pa\u{0308}ss").unwrap();
        let opened = Masterkey::unlock(&file, "p\u{00e4}ss").unwrap();
        assert_eq!(opened.encryption_key(), masterkey.encryption_key());
    }

    #[test]
    fn refuses_parameters_that_would_exhaust_memory() {
        let file = Masterkey::generate().unwrap().lock("password").unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&file).unwrap();
        json["scryptCostParam"] = serde_json::json!(1u64 << 30);
        let error = Masterkey::unlock(&serde_json::to_vec(&json).unwrap(), "password")
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Unsupported);

        json["scryptCostParam"] = serde_json::json!(1000);
        let error = Masterkey::unlock(&serde_json::to_vec(&json).unwrap(), "password")
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Integrity);
    }

    #[test]
    fn a_file_that_is_not_json_is_damaged() {
        let error = Masterkey::unlock(b"not json", "password").err().unwrap();
        assert_eq!(error.kind, ErrorKind::Integrity);
    }
}
