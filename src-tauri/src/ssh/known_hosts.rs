//! Reads `~/.ssh/known_hosts` but writes only the app's own file, which is consulted
//! first and is authoritative for the hosts it lists.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hmac::{Hmac, Mac};
use russh::keys::{Algorithm, HashAlg, PublicKey};
use sha1::Sha1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    Trusted,
    Unknown,
    /// Also returned for a `@revoked` key, and for a key of a type the host is not on record
    /// with when it is on record with others.
    Changed,
}

/// What one known_hosts file says about a host's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Record {
    NotListed,
    Trusted,
    /// Revoked, or another key of the same type is listed.
    Changed,
    /// The host is listed, but only with keys of other types.
    OtherKeyTypes,
}

#[derive(Debug, Clone)]
pub struct KnownHosts {
    app_file: PathBuf,
    system_files: Vec<PathBuf>,
    /// Keys trusted until Poros closes, shared by every clone.
    trusted_until_exit: Arc<Mutex<Vec<(String, PublicKey)>>>,
}

impl KnownHosts {
    pub fn new(app_file: PathBuf, system_files: Vec<PathBuf>) -> Self {
        Self {
            app_file,
            system_files,
            trusted_until_exit: Arc::default(),
        }
    }

    pub fn with_defaults(app_file: PathBuf) -> Self {
        let system = dirs::home_dir()
            .map(|home| vec![home.join(".ssh").join("known_hosts")])
            .unwrap_or_default();
        Self::new(app_file, system)
    }

    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        let host_port = host_pattern(host, port);
        let trusted_for_now =
            self.trusted_until_exit
                .lock()
                .unwrap()
                .iter()
                .any(|(trusted_host, trusted_key)| {
                    *trusted_host == host_port && trusted_key.key_data() == key.key_data()
                });
        if trusted_for_now {
            return HostKeyStatus::Trusted;
        }
        let mut listed_with_other_key_types = false;
        for file in self.files() {
            match check_file(file, &host_port, key) {
                Record::Trusted => return HostKeyStatus::Trusted,
                Record::Changed => return HostKeyStatus::Changed,
                Record::OtherKeyTypes => listed_with_other_key_types = true,
                Record::NotListed => {}
            }
        }
        // The handshake asks for the recorded key types first, so a server that presents
        // another type no longer has the recorded keys or is not the server on record.
        if listed_with_other_key_types {
            HostKeyStatus::Changed
        } else {
            HostKeyStatus::Unknown
        }
    }

    /// `defaults` reordered so the key types on record for the host come first, keeping
    /// their order otherwise. Without this the server would present the key type the client
    /// likes best, and a host on record with another type would look new.
    pub fn preferred_algorithms(
        &self,
        host: &str,
        port: u16,
        defaults: &[Algorithm],
    ) -> Vec<Algorithm> {
        let recorded = self.recorded_algorithms(host, port);
        let (on_record, others): (Vec<Algorithm>, Vec<Algorithm>) =
            defaults.iter().cloned().partition(|candidate| {
                recorded
                    .iter()
                    .any(|algorithm| same_key_type(algorithm, candidate))
            });
        on_record.into_iter().chain(others).collect()
    }

    fn recorded_algorithms(&self, host: &str, port: u16) -> Vec<Algorithm> {
        let host_port = host_pattern(host, port);
        let mut recorded: Vec<Algorithm> = self
            .trusted_until_exit
            .lock()
            .unwrap()
            .iter()
            .filter(|(trusted_host, _)| *trusted_host == host_port)
            .map(|(_, key)| key.algorithm())
            .collect();
        for file in self.files() {
            let Ok(contents) = fs::read_to_string(file) else {
                continue;
            };
            recorded.extend(
                contents
                    .lines()
                    .filter_map(parse_line)
                    .filter(|entry| entry.marker == Marker::None && entry.matches_host(&host_port))
                    .map(|entry| entry.key.algorithm()),
            );
        }
        recorded
    }

    fn files(&self) -> impl Iterator<Item = &PathBuf> {
        std::iter::once(&self.app_file).chain(&self.system_files)
    }

    /// Trusts a key without saving it. Used for jump hosts the user accepted once, which
    /// every later connection through them has to pass again.
    pub fn trust_until_exit(&self, host: &str, port: u16, key: &PublicKey) {
        self.trusted_until_exit
            .lock()
            .unwrap()
            .push((host_pattern(host, port), key.clone()));
    }

    /// Replaces any recorded key of the same algorithm for the host.
    pub fn trust(&self, host: &str, port: u16, key: &PublicKey) -> io::Result<()> {
        let host_port = host_pattern(host, port);
        let existing = fs::read_to_string(&self.app_file).unwrap_or_default();
        let mut kept: Vec<&str> = existing
            .lines()
            .filter(|line| match parse_line(line) {
                Some(entry) => {
                    !(entry.matches_host(&host_port)
                        && entry.key.key_data().algorithm() == key.key_data().algorithm())
                }
                None => true,
            })
            .collect();
        let openssh = key
            .to_openssh()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let mut parts = openssh.split_whitespace();
        let record = format!(
            "{host_port} {} {}",
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default()
        );
        kept.push(&record);

        if let Some(parent) = self.app_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.app_file.with_extension("tmp");
        {
            let mut file = fs::File::create(&temporary)?;
            for line in &kept {
                writeln!(file, "{line}")?;
            }
            file.sync_all()?;
        }
        fs::rename(&temporary, &self.app_file)
    }
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Every RSA signature algorithm verifies with the same RSA key.
fn same_key_type(recorded: &Algorithm, candidate: &Algorithm) -> bool {
    match (recorded, candidate) {
        (Algorithm::Rsa { .. }, Algorithm::Rsa { .. }) => true,
        _ => recorded == candidate,
    }
}

/// OpenSSH writes non-default ports as `[host]:port`.
fn host_pattern(host: &str, port: u16) -> String {
    let host = host.to_ascii_lowercase();
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

fn check_file(path: &Path, host_port: &str, key: &PublicKey) -> Record {
    let Ok(contents) = fs::read_to_string(path) else {
        return Record::NotListed;
    };
    let mut record = Record::NotListed;
    for entry in contents.lines().filter_map(parse_line) {
        if !entry.matches_host(host_port) {
            continue;
        }
        let same_key = entry.key.key_data() == key.key_data();
        match entry.marker {
            Marker::Revoked if same_key => return Record::Changed,
            Marker::Revoked | Marker::CertAuthority => continue,
            Marker::None => {}
        }
        if same_key {
            return Record::Trusted;
        }
        if entry.key.key_data().algorithm() == key.key_data().algorithm() {
            record = Record::Changed;
        } else if record == Record::NotListed {
            record = Record::OtherKeyTypes;
        }
    }
    record
}

#[derive(Debug, PartialEq, Eq)]
enum Marker {
    None,
    Revoked,
    CertAuthority,
}

struct Entry<'a> {
    marker: Marker,
    hosts: &'a str,
    key: PublicKey,
}

impl Entry<'_> {
    fn matches_host(&self, host_port: &str) -> bool {
        let mut matched = false;
        for pattern in self.hosts.split(',') {
            if let Some(negated) = pattern.strip_prefix('!') {
                if glob_match(&negated.to_ascii_lowercase(), host_port) {
                    return false;
                }
            } else if pattern.starts_with("|1|") {
                matched |= hashed_match(pattern, host_port);
            } else {
                matched |= glob_match(&pattern.to_ascii_lowercase(), host_port);
            }
        }
        matched
    }
}

fn parse_line(line: &str) -> Option<Entry<'_>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut fields = line.split_whitespace();
    let mut first = fields.next()?;
    let marker = match first {
        "@revoked" => Marker::Revoked,
        "@cert-authority" => Marker::CertAuthority,
        m if m.starts_with('@') => return None,
        _ => Marker::None,
    };
    if marker != Marker::None {
        first = fields.next()?;
    }
    let _algorithm = fields.next()?;
    let key = russh::keys::parse_public_key_base64(fields.next()?).ok()?;
    Some(Entry {
        marker,
        hosts: first,
        key,
    })
}

/// `|1|base64(salt)|base64(hmac_sha1(salt, host))`, as written by `HashKnownHosts yes`.
fn hashed_match(pattern: &str, host_port: &str) -> bool {
    let mut parts = pattern.split('|').skip(2);
    let (Some(salt), Some(hash)) = (parts.next(), parts.next()) else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (BASE64.decode(salt), BASE64.decode(hash)) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(host_port.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pattern_index, mut text_index) = (0, 0);
    // On mismatch, retry from the last `*` with it absorbing one more character.
    let mut backtrack: Option<(usize, usize)> = None;
    while text_index < text.len() {
        match pattern.get(pattern_index) {
            Some('*') => {
                backtrack = Some((pattern_index, text_index));
                pattern_index += 1;
            }
            Some(&expected) if expected == '?' || expected == text[text_index] => {
                pattern_index += 1;
                text_index += 1;
            }
            _ => match backtrack {
                Some((star_index, absorbed_until)) => {
                    pattern_index = star_index + 1;
                    text_index = absorbed_until + 1;
                    backtrack = Some((star_index, absorbed_until + 1));
                }
                None => return false,
            },
        }
    }
    pattern[pattern_index..]
        .iter()
        .all(|&remaining| remaining == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
    const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";

    fn key(b64: &str) -> PublicKey {
        russh::keys::parse_public_key_base64(b64).unwrap()
    }

    fn store(app: &str, system: &str) -> (tempfile::TempDir, KnownHosts) {
        let temp_dir = tempfile::tempdir().unwrap();
        let app_file = temp_dir.path().join("app_known_hosts");
        let system_file = temp_dir.path().join("known_hosts");
        fs::write(&app_file, app).unwrap();
        fs::write(&system_file, system).unwrap();
        let known_hosts = KnownHosts::new(app_file, vec![system_file]);
        (temp_dir, known_hosts)
    }

    #[test]
    fn keys_trusted_until_exit_are_shared_and_not_saved() {
        let (_dir, known_hosts) = store("", "");
        let clone = known_hosts.clone();
        clone.trust_until_exit("bastion", 2222, &key(KEY_A));
        assert_eq!(
            known_hosts.check("bastion", 2222, &key(KEY_A)),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            known_hosts.check("bastion", 22, &key(KEY_A)),
            HostKeyStatus::Unknown
        );
        assert_eq!(
            known_hosts.check("bastion", 2222, &key(KEY_B)),
            HostKeyStatus::Unknown
        );
        assert_eq!(fs::read_to_string(&known_hosts.app_file).unwrap(), "");
    }

    #[test]
    fn unknown_when_nothing_recorded() {
        let (_dir, known_hosts) = store("", "");
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_A)),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn trusted_and_changed_from_system_file() {
        let (_dir, known_hosts) = store("", &format!("example.com ssh-ed25519 {KEY_A}\n"));
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_A)),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            known_hosts.check("EXAMPLE.com", 22, &key(KEY_A)),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_B)),
            HostKeyStatus::Changed
        );
        assert_eq!(
            known_hosts.check("example.com", 2222, &key(KEY_A)),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn matches_hashed_entries_written_by_openssh() {
        // Produced by `ssh-keygen -H` for example.com and [example.com]:2222.
        let hashed = format!(
            "|1|8fxdNxJC4ug7TvJLO7OFyjeiwlA=|C6auzEsTkwfTHMojSmP26uYRtuE= ssh-ed25519 {KEY_A}\n\
             |1|l17BdQJFBuoThJQtEtB+nJ+BtS4=|CI/ykWdqjIKDD8u9ALJBzTob4fM= ssh-ed25519 {KEY_A}\n"
        );
        let (_dir, known_hosts) = store("", &hashed);
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_A)),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            known_hosts.check("example.com", 2222, &key(KEY_A)),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            known_hosts.check("other.com", 22, &key(KEY_A)),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn wildcards_negation_and_markers() {
        let lines = format!(
            "# comment\n\
             *.example.com,!bad.example.com ssh-ed25519 {KEY_A}\n\
             @revoked * ssh-ed25519 {KEY_B}\n\
             garbage line\n"
        );
        let (_dir, known_hosts) = store("", &lines);
        assert_eq!(
            known_hosts.check("a.example.com", 22, &key(KEY_A)),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            known_hosts.check("bad.example.com", 22, &key(KEY_A)),
            HostKeyStatus::Unknown
        );
        assert_eq!(
            known_hosts.check("a.example.com", 22, &key(KEY_B)),
            HostKeyStatus::Changed
        );
    }

    fn fixture_key(public_key_file: &str) -> &str {
        public_key_file.split_whitespace().nth(1).unwrap()
    }

    #[test]
    fn a_host_on_record_with_other_key_types_is_changed_not_new() {
        let ecdsa = fixture_key(include_str!("../../tests/fixtures/keys/ecdsa256.pub"));
        let (_dir, known_hosts) = store("", &format!("example.com ecdsa-sha2-nistp256 {ecdsa}\n"));
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_A)),
            HostKeyStatus::Changed
        );

        let (_dir, known_hosts) = store(
            &format!("[example.com]:2222 ecdsa-sha2-nistp256 {ecdsa}\n"),
            "",
        );
        assert_eq!(
            known_hosts.check("example.com", 2222, &key(KEY_A)),
            HostKeyStatus::Changed
        );

        // A key the system file vouches for still passes when the app file lists another type.
        let (_dir, known_hosts) = store(
            &format!("example.com ecdsa-sha2-nistp256 {ecdsa}\n"),
            &format!("example.com ssh-ed25519 {KEY_A}\n"),
        );
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_A)),
            HostKeyStatus::Trusted
        );

        // Revoked keys and certificate authorities do not put a host on record.
        let (_dir, known_hosts) = store(
            "",
            &format!(
                "@revoked example.com ecdsa-sha2-nistp256 {ecdsa}\n\
                 @cert-authority example.com ecdsa-sha2-nistp256 {ecdsa}\n"
            ),
        );
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_A)),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn key_types_on_record_are_asked_for_first() {
        let defaults = russh::Preferred::default().key.into_owned();
        let ecdsa = fixture_key(include_str!("../../tests/fixtures/keys/ecdsa256.pub"));
        let rsa = fixture_key(include_str!("../../tests/fixtures/keys/rsa.pub"));
        let (_dir, known_hosts) = store(
            &format!("[ecdsa.example]:2222 ecdsa-sha2-nistp256 {ecdsa}\n"),
            &format!("rsa.example ssh-rsa {rsa}\n@revoked new.example ssh-rsa {rsa}\n"),
        );

        let ecdsa_first = known_hosts.preferred_algorithms("ecdsa.example", 2222, &defaults);
        let nist_p256 = Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP256,
        };
        assert_eq!(ecdsa_first[0], nist_p256);
        let rest: Vec<Algorithm> = defaults
            .iter()
            .filter(|algorithm| **algorithm != nist_p256)
            .cloned()
            .collect();
        assert_eq!(ecdsa_first[1..], rest[..]);

        let rsa_first = known_hosts.preferred_algorithms("rsa.example", 22, &defaults);
        let rsa_count = defaults
            .iter()
            .filter(|algorithm| matches!(algorithm, Algorithm::Rsa { .. }))
            .count();
        assert!(rsa_count > 1);
        assert!(rsa_first[..rsa_count]
            .iter()
            .all(|algorithm| matches!(algorithm, Algorithm::Rsa { .. })));
        assert_eq!(rsa_first.len(), defaults.len());

        assert_eq!(
            known_hosts.preferred_algorithms("new.example", 22, &defaults),
            defaults
        );

        known_hosts.trust_until_exit("bastion", 22, &key(ecdsa));
        assert_eq!(
            known_hosts.preferred_algorithms("bastion", 22, &defaults)[0],
            nist_p256
        );
    }

    #[test]
    fn app_store_overrides_system_file_after_trust() {
        let (_dir, known_hosts) = store("", &format!("example.com ssh-ed25519 {KEY_A}\n"));
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_B)),
            HostKeyStatus::Changed
        );
        known_hosts.trust("example.com", 22, &key(KEY_B)).unwrap();
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_B)),
            HostKeyStatus::Trusted
        );
        // The old key is now the changed one, since the app store is authoritative.
        assert_eq!(
            known_hosts.check("example.com", 22, &key(KEY_A)),
            HostKeyStatus::Changed
        );
    }

    #[test]
    fn trust_replaces_previous_key_for_same_host() {
        let (_dir, known_hosts) = store("", "");
        known_hosts.trust("h", 2222, &key(KEY_A)).unwrap();
        known_hosts.trust("h", 2222, &key(KEY_B)).unwrap();
        known_hosts.trust("other", 22, &key(KEY_A)).unwrap();
        let contents = fs::read_to_string(&known_hosts.app_file).unwrap();
        assert_eq!(contents.lines().count(), 2, "{contents}");
        assert!(contents.contains(&format!("[h]:2222 ssh-ed25519 {KEY_B}")));
        assert!(contents.contains(&format!("other ssh-ed25519 {KEY_A}")));
    }

    #[test]
    fn fingerprint_matches_ssh_keygen() {
        assert_eq!(
            fingerprint(&key(KEY_A)),
            "SHA256:T7SvZ2cslqpPj6nKzitCBHHlpVF3r3MvLwmFL0fk0IE"
        );
    }

    #[test]
    fn glob() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("10.0.0.?", "10.0.0.5"));
        assert!(!glob_match("10.0.0.?", "10.0.0.55"));
        assert!(glob_match("*.example.com", "a.b.example.com"));
        assert!(!glob_match("*.example.com", "example.com"));
    }
}
