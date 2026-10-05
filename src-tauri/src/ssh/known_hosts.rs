//! Reads `~/.ssh/known_hosts` but writes only the app's own file, which is consulted
//! first and is authoritative for the hosts it lists.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use hmac::{Hmac, Mac};
use russh::keys::{HashAlg, PublicKey};
use sha1::Sha1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    Trusted,
    Unknown,
    /// Also returned for a `@revoked` key.
    Changed,
}

#[derive(Debug, Clone)]
pub struct KnownHosts {
    app_file: PathBuf,
    system_files: Vec<PathBuf>,
}

impl KnownHosts {
    pub fn new(app_file: PathBuf, system_files: Vec<PathBuf>) -> Self {
        Self {
            app_file,
            system_files,
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
        for file in std::iter::once(&self.app_file).chain(&self.system_files) {
            match check_file(file, &host_port, key) {
                HostKeyStatus::Unknown => continue,
                decided => return decided,
            }
        }
        HostKeyStatus::Unknown
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

        if let Some(dir) = self.app_file.parent() {
            fs::create_dir_all(dir)?;
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

/// OpenSSH writes non-default ports as `[host]:port`.
fn host_pattern(host: &str, port: u16) -> String {
    let host = host.to_ascii_lowercase();
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

fn check_file(path: &Path, host_port: &str, key: &PublicKey) -> HostKeyStatus {
    let Ok(contents) = fs::read_to_string(path) else {
        return HostKeyStatus::Unknown;
    };
    let mut status = HostKeyStatus::Unknown;
    for entry in contents.lines().filter_map(parse_line) {
        if !entry.matches_host(host_port) {
            continue;
        }
        let same_key = entry.key.key_data() == key.key_data();
        match entry.marker {
            Marker::Revoked if same_key => return HostKeyStatus::Changed,
            Marker::Revoked | Marker::CertAuthority => continue,
            Marker::None => {}
        }
        if same_key {
            return HostKeyStatus::Trusted;
        }
        if entry.key.key_data().algorithm() == key.key_data().algorithm() {
            status = HostKeyStatus::Changed;
        }
    }
    status
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
        let dir = tempfile::tempdir().unwrap();
        let app_file = dir.path().join("app_known_hosts");
        let system_file = dir.path().join("known_hosts");
        fs::write(&app_file, app).unwrap();
        fs::write(&system_file, system).unwrap();
        let known_hosts = KnownHosts::new(app_file, vec![system_file]);
        (dir, known_hosts)
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
