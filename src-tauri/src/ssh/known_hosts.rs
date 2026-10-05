//! Host key verification against OpenSSH-format known_hosts files.
//!
//! Poros reads the user's `~/.ssh/known_hosts` but only ever writes to its own file in
//! the app config dir, so accepting a key in Poros never edits the user's SSH setup.
//! The app file is consulted first and is authoritative for the hosts it lists.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use russh::keys::{HashAlg, PublicKey};
use sha1::Sha1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// A recorded key of the same algorithm matches.
    Trusted,
    /// No key of this algorithm is recorded for the host.
    Unknown,
    /// A different key of the same algorithm is recorded, or the key is `@revoked`.
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

    /// App store plus the user's OpenSSH known_hosts, when a home dir exists.
    pub fn with_defaults(app_file: PathBuf) -> Self {
        let system = dirs::home_dir()
            .map(|h| vec![h.join(".ssh").join("known_hosts")])
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

    /// Records `key` for the host in the app store, replacing any recorded key of the
    /// same algorithm for that host.
    pub fn trust(&self, host: &str, port: u16, key: &PublicKey) -> io::Result<()> {
        let host_port = host_pattern(host, port);
        let existing = fs::read_to_string(&self.app_file).unwrap_or_default();
        let mut kept: Vec<&str> = existing
            .lines()
            .filter(|line| match parse_line(line) {
                Some(entry) => !(entry.matches_host(&host_port)
                    && entry.key.key_data().algorithm() == key.key_data().algorithm()),
                None => true,
            })
            .collect();
        let openssh = key
            .to_openssh()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        // Drop the comment so the line is `host alg base64`.
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
        let tmp = self.app_file.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            for line in &kept {
                writeln!(f, "{line}")?;
            }
            f.sync_all()?;
        }
        fs::rename(&tmp, &self.app_file)
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
    let (Ok(salt), Ok(hash)) = (B64.decode(salt), B64.decode(hash)) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(host_port.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

/// OpenSSH host pattern matching: `*` matches any run, `?` a single character.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
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
        let sys_file = dir.path().join("known_hosts");
        fs::write(&app_file, app).unwrap();
        fs::write(&sys_file, system).unwrap();
        let kh = KnownHosts::new(app_file, vec![sys_file]);
        (dir, kh)
    }

    #[test]
    fn unknown_when_nothing_recorded() {
        let (_d, kh) = store("", "");
        assert_eq!(kh.check("example.com", 22, &key(KEY_A)), HostKeyStatus::Unknown);
    }

    #[test]
    fn trusted_and_changed_from_system_file() {
        let (_d, kh) = store("", &format!("example.com ssh-ed25519 {KEY_A}\n"));
        assert_eq!(kh.check("example.com", 22, &key(KEY_A)), HostKeyStatus::Trusted);
        assert_eq!(kh.check("EXAMPLE.com", 22, &key(KEY_A)), HostKeyStatus::Trusted);
        assert_eq!(kh.check("example.com", 22, &key(KEY_B)), HostKeyStatus::Changed);
        assert_eq!(kh.check("example.com", 2222, &key(KEY_A)), HostKeyStatus::Unknown);
    }

    #[test]
    fn matches_hashed_entries_written_by_openssh() {
        // Produced by `ssh-keygen -H` for example.com and [example.com]:2222.
        let hashed = format!(
            "|1|8fxdNxJC4ug7TvJLO7OFyjeiwlA=|C6auzEsTkwfTHMojSmP26uYRtuE= ssh-ed25519 {KEY_A}\n\
             |1|l17BdQJFBuoThJQtEtB+nJ+BtS4=|CI/ykWdqjIKDD8u9ALJBzTob4fM= ssh-ed25519 {KEY_A}\n"
        );
        let (_d, kh) = store("", &hashed);
        assert_eq!(kh.check("example.com", 22, &key(KEY_A)), HostKeyStatus::Trusted);
        assert_eq!(kh.check("example.com", 2222, &key(KEY_A)), HostKeyStatus::Trusted);
        assert_eq!(kh.check("other.com", 22, &key(KEY_A)), HostKeyStatus::Unknown);
    }

    #[test]
    fn wildcards_negation_and_markers() {
        let lines = format!(
            "# comment\n\
             *.example.com,!bad.example.com ssh-ed25519 {KEY_A}\n\
             @revoked * ssh-ed25519 {KEY_B}\n\
             garbage line\n"
        );
        let (_d, kh) = store("", &lines);
        assert_eq!(kh.check("a.example.com", 22, &key(KEY_A)), HostKeyStatus::Trusted);
        assert_eq!(kh.check("bad.example.com", 22, &key(KEY_A)), HostKeyStatus::Unknown);
        assert_eq!(kh.check("a.example.com", 22, &key(KEY_B)), HostKeyStatus::Changed);
    }

    #[test]
    fn app_store_overrides_system_file_after_trust() {
        let (_d, kh) = store("", &format!("example.com ssh-ed25519 {KEY_A}\n"));
        assert_eq!(kh.check("example.com", 22, &key(KEY_B)), HostKeyStatus::Changed);
        kh.trust("example.com", 22, &key(KEY_B)).unwrap();
        assert_eq!(kh.check("example.com", 22, &key(KEY_B)), HostKeyStatus::Trusted);
        // The old key is now the changed one, since the app store is authoritative.
        assert_eq!(kh.check("example.com", 22, &key(KEY_A)), HostKeyStatus::Changed);
    }

    #[test]
    fn trust_replaces_previous_key_for_same_host() {
        let (_d, kh) = store("", "");
        kh.trust("h", 2222, &key(KEY_A)).unwrap();
        kh.trust("h", 2222, &key(KEY_B)).unwrap();
        kh.trust("other", 22, &key(KEY_A)).unwrap();
        let contents = fs::read_to_string(&kh.app_file).unwrap();
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
