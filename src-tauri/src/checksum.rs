//! File digests for checking transfers and comparing contents: computed here, or on the server
//! by its `sha256sum`, `sha512sum`, `sha1sum` or `md5sum` command, which OpenSSH shells and
//! SFTPGo both offer.

use std::io::Read;
use std::ops::Range;
use std::path::Path;

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

const READ_CHUNK: usize = 256 * 1024;
const MEBIBYTE: u64 = 1024 * 1024;
/// Parts are at least this long, so a file of a few MiB is hashed whole.
const MIN_PART: u64 = 4 * MEBIBYTE;
/// At most this many parts per file, each hashed by its own process on the server.
const MAX_PARTS: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha256,
    Sha512,
    Sha1,
    Md5,
}

impl Algorithm {
    /// The ones a finished transfer is checked with, in the order a server's commands are tried.
    /// SHA-1 and MD5 catch accidental damage but can be forged, so they never prove a copy.
    pub const STRONG: [Algorithm; 2] = [Self::Sha256, Self::Sha512];

    pub fn command(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256sum",
            Self::Sha512 => "sha512sum",
            Self::Sha1 => "sha1sum",
            Self::Md5 => "md5sum",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
            Self::Sha1 => "SHA-1",
            Self::Md5 => "MD5",
        }
    }

    pub fn hasher(self) -> Hasher {
        match self {
            Self::Sha256 => Hasher::Sha256(Sha256::new()),
            Self::Sha512 => Hasher::Sha512(Box::new(Sha512::new())),
            Self::Sha1 => Hasher::Sha1(Sha1::new()),
            Self::Md5 => Hasher::Md5(Md5::new()),
        }
    }

    pub fn hex_len(self) -> usize {
        match self {
            Self::Sha256 => 64,
            Self::Sha512 => 128,
            Self::Sha1 => 40,
            Self::Md5 => 32,
        }
    }
}

pub enum Hasher {
    Sha256(Sha256),
    Sha512(Box<Sha512>),
    Sha1(Sha1),
    Md5(Md5),
}

impl Hasher {
    pub fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha256(digest) => digest.update(data),
            Self::Sha512(digest) => digest.update(data),
            Self::Sha1(digest) => digest.update(data),
            Self::Md5(digest) => digest.update(data),
        }
    }

    pub fn finish(self) -> Vec<u8> {
        match self {
            Self::Sha256(digest) => digest.finalize().to_vec(),
            Self::Sha512(digest) => digest.finalize().to_vec(),
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

/// How a file is cut for a SHA-256 digest of each part, so a mismatch tells which parts to
/// send again. Every part is `size` bytes but the last. A file no longer than one part has a
/// single digest of the whole, as `sha256sum` gives, and so does an empty one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts {
    pub size: u64,
}

impl Parts {
    pub const WHOLE: Parts = Parts { size: u64::MAX };

    /// At least 4 MiB each, in whole MiB, and no more than 64 of them.
    pub fn for_length(length: u64) -> Self {
        let size = length.div_ceil(MAX_PARTS).div_ceil(MEBIBYTE) * MEBIBYTE;
        Self {
            size: size.max(MIN_PART),
        }
    }

    pub fn count(self, length: u64) -> usize {
        if length <= self.size {
            1
        } else {
            length.div_ceil(self.size) as usize
        }
    }

    pub fn range(self, index: usize, length: u64) -> Range<u64> {
        let start = (index as u64).saturating_mul(self.size).min(length);
        start..start.saturating_add(self.size).min(length)
    }
}

/// SHA-256 digests of each part of a stream, fed in order.
pub struct PartHasher {
    parts: Parts,
    filled: u64,
    current: Sha256,
    digests: Vec<Vec<u8>>,
}

impl PartHasher {
    pub fn new(parts: Parts) -> Self {
        Self {
            parts,
            filled: 0,
            current: Sha256::new(),
            digests: Vec::new(),
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let room = self.parts.size - self.filled;
            let taken = (data.len() as u64).min(room) as usize;
            self.current.update(&data[..taken]);
            self.filled += taken as u64;
            data = &data[taken..];
            if self.filled == self.parts.size {
                self.digests.push(self.current.finalize_reset().to_vec());
                self.filled = 0;
            }
        }
    }

    pub fn finish(mut self) -> Vec<Vec<u8>> {
        if self.filled > 0 || self.digests.is_empty() {
            self.digests.push(self.current.finalize().to_vec());
        }
        self.digests
    }
}

/// Digests of each part of the first `length` bytes of a file, or of all of it when it is
/// shorter. Blocking.
pub fn hash_file_parts(path: &Path, length: u64, parts: Parts) -> std::io::Result<Vec<Vec<u8>>> {
    let file = std::fs::File::open(path)?;
    let mut limited = file.take(length);
    let mut hasher = PartHasher::new(parts);
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        let read = limited.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finish())
}

pub fn to_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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

pub fn parse_hex(hex: &str) -> Option<Vec<u8>> {
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
            (
                Algorithm::Sha512,
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
                 2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
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

    #[test]
    fn parts_match_what_split_and_sha256sum_print() {
        let mebibyte = 1024 * 1024;
        assert_eq!(Parts::for_length(0).size, 4 * mebibyte);
        assert_eq!(Parts::for_length(100 * mebibyte).size, 4 * mebibyte);
        assert_eq!(Parts::for_length(1024 * mebibyte).size, 16 * mebibyte);
        assert_eq!(Parts::for_length(1000 * mebibyte + 1).size, 16 * mebibyte);

        let parts = Parts { size: 2 };
        assert_eq!(parts.count(0), 1);
        assert_eq!(parts.count(2), 1);
        assert_eq!(parts.count(3), 2);
        assert_eq!(parts.count(4), 2);
        assert_eq!(parts.range(1, 3), 2..3);
        assert_eq!(Parts::WHOLE.count(u64::MAX - 1), 1);

        let sha256 = |data: &[u8]| Sha256::digest(data).to_vec();
        let digests = |data: &[u8]| {
            let mut hasher = PartHasher::new(parts);
            for byte in data {
                hasher.update(std::slice::from_ref(byte));
            }
            hasher.finish()
        };
        assert_eq!(digests(b""), vec![sha256(b"")]);
        assert_eq!(digests(b"ab"), vec![sha256(b"ab")]);
        assert_eq!(digests(b"abc"), vec![sha256(b"ab"), sha256(b"c")]);
        assert_eq!(digests(b"abcd"), vec![sha256(b"ab"), sha256(b"cd")]);

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("abcde");
        std::fs::write(&path, b"abcde").unwrap();
        assert_eq!(
            hash_file_parts(&path, 3, parts).unwrap(),
            vec![sha256(b"ab"), sha256(b"c")]
        );
        assert_eq!(
            hash_file_parts(&path, u64::MAX, Parts::WHOLE).unwrap(),
            vec![sha256(b"abcde")]
        );
        assert_eq!(to_hex(&[0x0f, 0xa0]), "0fa0");
    }
}
