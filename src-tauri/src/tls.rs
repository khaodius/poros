//! TLS for FTPS and the cloud APIs: rustls on `ring`, verified against the operating system's
//! certificate store. An FTPS server whose certificate the system does not trust (self-signed,
//! expired, or issued for another name) can still be used once the user approves its
//! fingerprint, the way an unknown SSH host key is approved.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls_platform_verifier::{BuilderVerifierExt, Verifier};

use crate::error::{AppError, AppResult, ErrorKind, HostKeyInfo};
use crate::events::{Events, LogLevel};
use crate::ssh::HostKeyApproval;

pub const CERTIFICATE_ALGORITHM: &str = "TLS certificate";

pub fn crypto_provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn tls_error(error: rustls::Error) -> AppError {
    AppError::new(ErrorKind::Connection, format!("TLS setup failed: {error}"))
}

/// For HTTPS APIs, which must present a certificate the system trusts.
pub fn platform_config(alpn: &[&[u8]]) -> AppResult<ClientConfig> {
    let mut config = ClientConfig::builder_with_provider(crypto_provider())
        .with_safe_default_protocol_versions()
        .map_err(tls_error)?
        .with_platform_verifier()
        .map_err(tls_error)?
        .with_no_client_auth();
    config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
    Ok(config)
}

/// `SHA256:` and the unpadded base64 digest of the certificate, as SSH shows key fingerprints.
pub fn fingerprint(certificate: &CertificateDer<'_>) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, certificate.as_ref());
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest.as_ref())
    )
}

/// Certificates the user chose to trust, one `host:port fingerprint` per line in the app's
/// config folder.
#[derive(Debug, Clone)]
pub struct TrustedCertificates {
    file: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl TrustedCertificates {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            lock: Arc::new(Mutex::new(())),
        }
    }

    fn key(host: &str, port: u16) -> String {
        format!("{}:{port}", host.to_ascii_lowercase())
    }

    fn entries(&self) -> Vec<(String, String)> {
        std::fs::read_to_string(&self.file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let (host_port, fingerprint) = line.trim().split_once(' ')?;
                Some((host_port.to_string(), fingerprint.trim().to_string()))
            })
            .collect()
    }

    /// The fingerprint on record for the server, if any.
    pub fn get(&self, host: &str, port: u16) -> Option<String> {
        let _guard = self.lock.lock().unwrap();
        let key = Self::key(host, port);
        self.entries()
            .into_iter()
            .find(|(host_port, _)| *host_port == key)
            .map(|(_, fingerprint)| fingerprint)
    }

    pub fn trust(&self, host: &str, port: u16, fingerprint: &str) -> AppResult<()> {
        let _guard = self.lock.lock().unwrap();
        let key = Self::key(host, port);
        let mut text = String::new();
        for (host_port, existing) in self.entries() {
            if host_port != key {
                let _ = writeln!(text, "{host_port} {existing}");
            }
        }
        let _ = writeln!(text, "{key} {fingerprint}");
        crate::storage::write_text(&self.file, &text)
    }
}

/// The platform verifier, plus fingerprints the user approved. Signatures are always checked
/// by the platform verifier, so an approved certificate still proves the server holds its key.
struct ApprovingVerifier {
    platform: Verifier,
    host: String,
    port: u16,
    trusted: TrustedCertificates,
    approval: Option<HostKeyApproval>,
    outcome: Arc<Mutex<CertificateOutcome>>,
    session_id: String,
    events: Events,
}

impl std::fmt::Debug for ApprovingVerifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApprovingVerifier")
            .field("host", &self.host)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct CertificateOutcome {
    accepted: Option<String>,
    rejected: Option<AppError>,
}

impl ServerCertVerifier for ApprovingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fingerprint = fingerprint(end_entity);
        let platform = self.platform.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        let problem = match platform {
            Ok(verified) => {
                self.outcome.lock().unwrap().accepted = Some(fingerprint);
                return Ok(verified);
            }
            Err(error) => error,
        };
        let recorded = self.trusted.get(&self.host, self.port);
        let approved = self
            .approval
            .as_ref()
            .filter(|approval| approval.fingerprint == fingerprint);
        if recorded.as_deref() == Some(fingerprint.as_str()) || approved.is_some() {
            if approved.is_some_and(|approval| approval.remember) {
                if let Err(error) = self.trusted.trust(&self.host, self.port, &fingerprint) {
                    self.events.log(
                        LogLevel::Warn,
                        Some(&self.session_id),
                        format!("Could not save the certificate: {}", error.message),
                    );
                }
            }
            self.outcome.lock().unwrap().accepted = Some(fingerprint);
            return Ok(ServerCertVerified::assertion());
        }
        let kind = if recorded.is_some() {
            ErrorKind::HostKeyChanged
        } else {
            ErrorKind::HostKeyUnknown
        };
        let info = HostKeyInfo {
            host: self.host.clone(),
            port: self.port,
            algorithm: CERTIFICATE_ALGORITHM.to_string(),
            fingerprint,
            certificate_problem: Some(describe_problem(&problem)),
        };
        self.outcome.lock().unwrap().rejected = Some(AppError::host_key(kind, info));
        Err(problem)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform
            .verify_tls12_signature(message, certificate, signature)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.platform
            .verify_tls13_signature(message, certificate, signature)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.platform.supported_verify_schemes()
    }
}

fn describe_problem(error: &rustls::Error) -> String {
    use rustls::CertificateError;
    match error {
        rustls::Error::InvalidCertificate(problem) => match problem {
            CertificateError::UnknownIssuer => {
                "It is self-signed or issued by an authority this computer does not trust.".into()
            }
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                "It has expired.".into()
            }
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
                "It is not valid yet.".into()
            }
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                "It was issued for a different server name.".into()
            }
            CertificateError::Revoked => "It has been revoked.".into(),
            other => format!("It could not be verified ({other:?})."),
        },
        other => format!("It could not be verified ({other})."),
    }
}

/// A TLS setup for one FTPS server. Data connections reuse it, so they resume the control
/// connection's TLS session, which many servers require.
pub struct ServerTls {
    pub config: Arc<ClientConfig>,
    pub server_name: ServerName<'static>,
    outcome: Arc<Mutex<CertificateOutcome>>,
}

impl ServerTls {
    pub fn new(
        host: &str,
        port: u16,
        trusted: &TrustedCertificates,
        approval: Option<HostKeyApproval>,
        session_id: &str,
        events: &Events,
    ) -> AppResult<Self> {
        let provider = crypto_provider();
        let outcome = Arc::new(Mutex::new(CertificateOutcome::default()));
        let verifier = ApprovingVerifier {
            platform: Verifier::new(provider.clone()).map_err(tls_error)?,
            host: host.to_string(),
            port,
            trusted: trusted.clone(),
            approval,
            outcome: outcome.clone(),
            session_id: session_id.to_string(),
            events: events.clone(),
        };
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(tls_error)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|_| AppError::invalid(format!("{host} is not a valid server name for TLS")))?;
        Ok(Self {
            config: Arc::new(config),
            server_name,
            outcome,
        })
    }

    /// Why the handshake failed, when it was the certificate.
    pub fn rejection(&self) -> Option<AppError> {
        self.outcome.lock().unwrap().rejected.take()
    }

    /// The fingerprint of the certificate the server presented and was accepted.
    pub fn accepted_fingerprint(&self) -> Option<String> {
        self.outcome.lock().unwrap().accepted.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_certificates_replace_per_server() {
        let temp_dir = tempfile::tempdir().unwrap();
        let trusted = TrustedCertificates::new(temp_dir.path().join("trusted_certificates"));
        assert_eq!(trusted.get("ftp.example.com", 21), None);
        trusted.trust("FTP.example.com", 21, "SHA256:one").unwrap();
        trusted
            .trust("other.example.com", 990, "SHA256:two")
            .unwrap();
        trusted
            .trust("ftp.example.com", 21, "SHA256:three")
            .unwrap();
        assert_eq!(
            trusted.get("ftp.example.com", 21).as_deref(),
            Some("SHA256:three")
        );
        assert_eq!(
            trusted.get("other.example.com", 990).as_deref(),
            Some("SHA256:two")
        );
    }

    #[test]
    fn fingerprints_look_like_ssh_ones() {
        let certificate = CertificateDer::from(vec![1, 2, 3]);
        let printed = fingerprint(&certificate);
        assert!(printed.starts_with("SHA256:"));
        assert_eq!(printed.len(), "SHA256:".len() + 43);
    }
}
