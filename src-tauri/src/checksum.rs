//! File digests for checking transfers and comparing contents: computed here, or on the server
//! by its `sha256sum`, `sha1sum` or `md5sum` command, which OpenSSH shells and SFTPGo both offer.

use std::io::Read;
use std::path::Path;

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};

const READ_CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha256,
    Sha1,
    Md5,
}

impl Algorithm {
    /// Strongest first, the order a server's commands are tried in.
    pub const PREFERRED: [Algorithm; 3] = [Self::Sha256, Self::Sha1, Self::Md5];

    pub fn command(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256sum",
            Self::Sha1 => "sha1sum",
            Self::Md5 => "md5sum",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sha256 => "SHA-256",
            Self::Sha1 => "SHA-1",
            Self::Md5 => "MD5",
        }
    }

    pub fn hasher(self) -> Hasher {
        match self {
            Self::Sha256 => Hasher::Sha256(Sha256::new()),
            Self::Sha1 => Hasher::Sha1(Sha1::new()),
            Self::Md5 => Hasher::Md5(Md5::new()),
        }
    }

    fn hex_len(self) -> usize {
        match self {
            Self::Sha256 => 64,
            Self::Sha1 => 40,
            Self::Md5 => 32,
        }
    }
}

pub enum Hasher {
    Sha256(Sha256),
    Sha1(Sha1),
    Md5(Md5),
}

impl Hasher {
    pub fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha256(digest) => digest.update(data),
            Self::Sha1(digest) => digest.update(data),
            Self::Md5(digest) => digest.update(data),
        }
    }

    pub fn finish(self) -> Vec<u8> {
        match self {
            Self::Sha256(digest) => digest.finalize().to_vec(),
            Self::Sha1(digest) => digest.finalize().to_vec(),
            Self::Md5(digest) => digest.finalize().to_vec(),
        }
    }
}

/// Blocking; callers run it on a blocking thread.
pub fn hash_file(path: &Path, algorithm: Algorithm) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = algorithm.hasher();
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finish())
}

/// Lines of `<hex>  <name>` as the commands print them. Names with a backslash or line break
/// are escaped, and the line then starts with a backslash.
pub fn parse_output(output: &str, algorithm: Algorithm) -> Vec<(String, Vec<u8>)> {
    let hex_len = algorithm.hex_len();
    output
        .lines()
        .filter_map(|line| {
            let (escaped, line) = match line.strip_prefix('\\') {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let (hex, name) = line.split_at_checked(hex_len)?;
            let name = name
                .strip_prefix("  ")
                .or_else(|| name.strip_prefix(" *"))?;
            let digest = parse_hex(hex)?;
            let name = if escaped {
                unescape(name)
            } else {
                name.to_string()
            };
            Some((name, digest))
        })
        .collect()
}

fn parse_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(hex.get(index..index + 2)?, 16).ok())
        .collect()
}

fn unescape(name: &str) -> String {
    let mut plain = String::with_capacity(name.len());
    let mut characters = name.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            plain.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => plain.push('\n'),
            Some('r') => plain.push('\r'),
            Some(other) => plain.push(other),
            None => plain.push('\\'),
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_command_output() {
        let output = "d41d8cd98f00b204e9800998ecf8427e  /srv/empty\n\
                      \\0cc175b9c0f1b6a831c399e269772661  /srv/odd\\nname\n\
                      md5sum: /srv/locked: Permission denied\n";
        let parsed = parse_output(output, Algorithm::Md5);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "/srv/empty");
        assert_eq!(parsed[0].1[0], 0xd4);
        assert_eq!(parsed[1].0, "/srv/odd\nname");

        let sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  -\n";
        let parsed = parse_output(sha256, Algorithm::Sha256);
        assert_eq!(parsed[0].0, "-");
        assert_eq!(parsed[0].1, Algorithm::Sha256.hasher().finish());
        assert!(parse_output(sha256, Algorithm::Sha1).is_empty());
    }

    #[test]
    fn hashes_files_like_the_commands() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("abc");
        std::fs::write(&path, b"abc").unwrap();
        let expected = [
            (
                Algorithm::Sha256,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (Algorithm::Sha1, "a9993e364706816aba3e25717850c26c9cd0d89d"),
            (Algorithm::Md5, "900150983cd24fb0d6963f7d28e17f72"),
        ];
        for (algorithm, hex) in expected {
            assert_eq!(
                hash_file(&path, algorithm).unwrap(),
                parse_hex(hex).unwrap()
            );
        }
    }
}
