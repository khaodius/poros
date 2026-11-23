//! `vault.cryptomator`: a signed token (JWT) at the top of a vault that names its format and
//! ciphers and where its masterkey is kept. The vault's own keys sign it, so a config that
//! verifies belongs to the masterkey that was just opened.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::hmac;
use serde::{Deserialize, Serialize};

use super::masterkey::Masterkey;
use crate::error::{AppError, AppResult};

pub const CONFIG_FILE: &str = "vault.cryptomator";
pub const MASTERKEY_FILE: &str = "masterkey.cryptomator";
/// The vault format of Cryptomator 1.6 and later.
const FORMAT: u64 = 8;
pub const DEFAULT_SHORTENING_THRESHOLD: usize = 220;
const MASTERKEY_FILE_SCHEME: &str = "masterkeyfile:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CipherCombo {
    /// AES-SIV for names and AES-GCM for contents: what Cryptomator 1.7 and later create.
    #[serde(rename = "SIV_GCM")]
    SivGcm,
    /// AES-SIV for names, AES-CTR with HMAC-SHA256 for contents: vaults made by earlier
    /// releases keep it.
    #[serde(rename = "SIV_CTRMAC")]
    SivCtrMac,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultConfig {
    pub id: String,
    pub cipher_combo: CipherCombo,
    /// Encrypted names longer than this, extension included, are stored shortened.
    pub shortening_threshold: usize,
}

#[derive(Serialize, Deserialize)]
struct Header {
    kid: String,
    alg: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    typ: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    #[serde(default)]
    jti: String,
    format: u64,
    cipher_combo: String,
    #[serde(default = "default_shortening_threshold")]
    shortening_threshold: usize,
}

fn default_shortening_threshold() -> usize {
    DEFAULT_SHORTENING_THRESHOLD
}

/// A config read from a vault, not yet checked against the vault's keys.
pub struct SignedConfig {
    header: Header,
    /// The header and claims as signed: everything before the last dot.
    signed: String,
    claims: Vec<u8>,
    signature: Vec<u8>,
}

impl SignedConfig {
    pub fn parse(token: &[u8]) -> AppResult<Self> {
        let token = std::str::from_utf8(token)
            .map_err(|_| damaged("it is not text"))?
            .trim();
        let mut parts = token.split('.');
        let (Some(header), Some(claims), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(damaged("it is not a signed token"));
        };
        let decode = |part: &str| {
            URL_SAFE_NO_PAD
                .decode(part.trim_end_matches('='))
                .map_err(|_| damaged("it is not a signed token"))
        };
        let header: Header = serde_json::from_slice(&decode(header)?)
            .map_err(|error| damaged(&format!("its header cannot be read ({error})")))?;
        Ok(Self {
            header,
            signed: token[..token.rfind('.').unwrap_or(token.len())].to_string(),
            claims: decode(claims)?,
            signature: decode(signature)?,
        })
    }

    /// The masterkey file's path inside the vault's folder.
    pub fn masterkey_file(&self) -> AppResult<&str> {
        let key_id = self.header.kid.as_str();
        let Some(file) = key_id.strip_prefix(MASTERKEY_FILE_SCHEME) else {
            return Err(AppError::unsupported(if key_id.starts_with("hub+") {
                "This vault's key is kept by Cryptomator Hub, which Poros cannot open".to_string()
            } else {
                format!("This vault keeps its key in a way Poros cannot open ({key_id})")
            }));
        };
        let stays_inside = !file.is_empty()
            && !file.starts_with('/')
            && !file.contains('\\')
            && file
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !stays_inside {
            return Err(damaged("it names a masterkey file outside the vault"));
        }
        Ok(file)
    }

    /// The config, once its signature proves it was written with `masterkey`.
    pub fn verify(&self, masterkey: &Masterkey) -> AppResult<VaultConfig> {
        let algorithm = match self.header.alg.as_str() {
            "HS256" => hmac::HMAC_SHA256,
            "HS384" => hmac::HMAC_SHA384,
            "HS512" => hmac::HMAC_SHA512,
            other => {
                return Err(AppError::unsupported(format!(
                    "This vault's configuration is signed with {other}, which Poros cannot check"
                )))
            }
        };
        let key = hmac::Key::new(algorithm, &masterkey.raw());
        hmac::verify(&key, self.signed.as_bytes(), &self.signature).map_err(|_| {
            AppError::integrity(
                "vault.cryptomator was not signed with this vault's key; it is damaged or was \
                 replaced",
            )
        })?;
        let claims: Claims = serde_json::from_slice(&self.claims)
            .map_err(|error| damaged(&format!("its contents cannot be read ({error})")))?;
        if claims.format != FORMAT {
            return Err(AppError::unsupported(format!(
                "This vault uses format {}; Poros opens format {FORMAT}, which Cryptomator 1.6 \
                 and later use",
                claims.format
            )));
        }
        let cipher_combo = match claims.cipher_combo.as_str() {
            "SIV_GCM" => CipherCombo::SivGcm,
            "SIV_CTRMAC" => CipherCombo::SivCtrMac,
            other => {
                return Err(AppError::unsupported(format!(
                    "This vault is encrypted with {other}, which Poros cannot open"
                )))
            }
        };
        Ok(VaultConfig {
            id: claims.jti,
            cipher_combo,
            shortening_threshold: claims.shortening_threshold,
        })
    }
}

impl VaultConfig {
    /// The config of a new vault, as Cryptomator creates them.
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            cipher_combo: CipherCombo::SivGcm,
            shortening_threshold: DEFAULT_SHORTENING_THRESHOLD,
        }
    }

    /// The contents of `vault.cryptomator` for this config, signed with `masterkey`.
    pub fn sign(&self, masterkey: &Masterkey) -> String {
        let header = Header {
            kid: format!("{MASTERKEY_FILE_SCHEME}{MASTERKEY_FILE}"),
            alg: "HS256".to_string(),
            typ: Some("JWT".to_string()),
        };
        let claims = Claims {
            jti: self.id.clone(),
            format: FORMAT,
            cipher_combo: match self.cipher_combo {
                CipherCombo::SivGcm => "SIV_GCM",
                CipherCombo::SivCtrMac => "SIV_CTRMAC",
            }
            .to_string(),
            shortening_threshold: self.shortening_threshold,
        };
        let signed = format!("{}.{}", encoded(&header), encoded(&claims));
        let key = hmac::Key::new(hmac::HMAC_SHA256, &masterkey.raw());
        let signature = hmac::sign(&key, signed.as_bytes());
        format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature))
    }
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self::new()
    }
}

fn encoded(value: &impl Serialize) -> String {
    let json = serde_json::to_vec(value).expect("a struct of strings and numbers encodes");
    URL_SAFE_NO_PAD.encode(json)
}

fn damaged(problem: &str) -> AppError {
    AppError::integrity(format!("vault.cryptomator is damaged: {problem}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn token_with(header: serde_json::Value, claims: serde_json::Value, key: &Masterkey) -> String {
        let signed = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let algorithm = match header["alg"].as_str().unwrap() {
            "HS384" => hmac::HMAC_SHA384,
            "HS512" => hmac::HMAC_SHA512,
            _ => hmac::HMAC_SHA256,
        };
        let signature = hmac::sign(&hmac::Key::new(algorithm, &key.raw()), signed.as_bytes());
        format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature))
    }

    fn header(algorithm: &str) -> serde_json::Value {
        serde_json::json!({
            "kid": "masterkeyfile:masterkey.cryptomator", "alg": algorithm, "typ": "JWT"
        })
    }

    #[test]
    fn a_signed_config_verifies_with_its_key_only() {
        let masterkey = Masterkey::generate().unwrap();
        let config = VaultConfig::new();
        let token = config.sign(&masterkey);

        let parsed = SignedConfig::parse(token.as_bytes()).unwrap();
        assert_eq!(parsed.masterkey_file().unwrap(), "masterkey.cryptomator");
        assert_eq!(parsed.verify(&masterkey).unwrap(), config);

        let other = Masterkey::generate().unwrap();
        assert_eq!(
            parsed.verify(&other).unwrap_err().kind,
            ErrorKind::Integrity
        );
    }

    #[test]
    fn reads_every_signature_cryptomator_accepts() {
        let masterkey = Masterkey::generate().unwrap();
        for algorithm in ["HS256", "HS384", "HS512"] {
            let claims = serde_json::json!({
                "jti": "id", "format": 8, "cipherCombo": "SIV_CTRMAC", "shorteningThreshold": 100
            });
            let token = token_with(header(algorithm), claims, &masterkey);
            let config = SignedConfig::parse(token.as_bytes())
                .unwrap()
                .verify(&masterkey)
                .unwrap();
            assert_eq!(config.cipher_combo, CipherCombo::SivCtrMac);
            assert_eq!(config.shortening_threshold, 100);
        }
    }

    #[test]
    fn a_tampered_config_is_refused() {
        let masterkey = Masterkey::generate().unwrap();
        let token = VaultConfig::new().sign(&masterkey);
        let mut parts: Vec<String> = token.split('.').map(str::to_string).collect();
        let claims = serde_json::json!({
            "jti": "id", "format": 8, "cipherCombo": "SIV_GCM", "shorteningThreshold": 1
        });
        parts[1] = URL_SAFE_NO_PAD.encode(claims.to_string());
        let error = SignedConfig::parse(parts.join(".").as_bytes())
            .unwrap()
            .verify(&masterkey)
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Integrity);
    }

    #[test]
    fn other_formats_and_ciphers_are_unsupported() {
        let masterkey = Masterkey::generate().unwrap();
        let verify = |claims: serde_json::Value| {
            SignedConfig::parse(token_with(header("HS256"), claims, &masterkey).as_bytes())
                .unwrap()
                .verify(&masterkey)
                .unwrap_err()
        };
        let newer = verify(serde_json::json!({ "format": 9, "cipherCombo": "SIV_GCM" }));
        assert_eq!(newer.kind, ErrorKind::Unsupported);
        assert!(newer.message.contains("format 9"), "{}", newer.message);
        let cipher = verify(serde_json::json!({ "format": 8, "cipherCombo": "XCHACHA" }));
        assert_eq!(cipher.kind, ErrorKind::Unsupported);
    }

    #[test]
    fn the_threshold_defaults_when_absent() {
        let masterkey = Masterkey::generate().unwrap();
        let claims = serde_json::json!({ "format": 8, "cipherCombo": "SIV_GCM" });
        let token = token_with(header("HS256"), claims, &masterkey);
        let config = SignedConfig::parse(token.as_bytes())
            .unwrap()
            .verify(&masterkey)
            .unwrap();
        assert_eq!(config.shortening_threshold, DEFAULT_SHORTENING_THRESHOLD);
    }

    #[test]
    fn only_a_masterkey_file_inside_the_vault_is_accepted() {
        let masterkey = Masterkey::generate().unwrap();
        let key_file = |key_id: &str| {
            let header = serde_json::json!({ "kid": key_id, "alg": "HS256" });
            let claims = serde_json::json!({ "format": 8, "cipherCombo": "SIV_GCM" });
            let token = token_with(header, claims, &masterkey);
            let parsed = SignedConfig::parse(token.as_bytes()).unwrap();
            parsed.masterkey_file().map(str::to_string)
        };
        assert_eq!(
            key_file("masterkeyfile:keys/main.cryptomator").unwrap(),
            "keys/main.cryptomator"
        );
        assert_eq!(
            key_file("masterkeyfile:../other").unwrap_err().kind,
            ErrorKind::Integrity
        );
        assert_eq!(
            key_file("masterkeyfile:/etc/passwd").unwrap_err().kind,
            ErrorKind::Integrity
        );
        assert_eq!(
            key_file("masterkeyfile:").unwrap_err().kind,
            ErrorKind::Integrity
        );
        let hub = key_file("hub+https://hub.example/vaults/1").unwrap_err();
        assert_eq!(hub.kind, ErrorKind::Unsupported);
        assert!(hub.message.contains("Hub"), "{}", hub.message);
    }

    #[test]
    fn text_that_is_not_a_token_is_damaged() {
        for text in ["", "one.two", "a.b.c.d", "!!!.###.$$$"] {
            let error = SignedConfig::parse(text.as_bytes()).err().unwrap();
            assert_eq!(error.kind, ErrorKind::Integrity, "{text}");
        }
    }
}
