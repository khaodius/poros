//! Checksums and signatures a publisher puts beside a file, checked when the file is
//! downloaded. A matching checksum shows the copy is the file the publisher listed; a good
//! signature shows the publisher made it. Checksum files are `SHA256SUMS`, `SHA512SUMS` and
//! the like, or `name.sha256` beside one file, in GNU or BSD form. Signatures are minisign's
//! `.minisig`, checked against the keys in Settings, and OpenPGP's `.asc` or `.sig`, checked
//! by this computer's `gpg` against its keyring.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::BytesMut;
use minisign_verify::{PublicKey, Signature};
use tokio::sync::OnceCell;

use super::worker::JobRef;
use crate::checksum::{hash_file, parse_hex, Algorithm};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::LogLevel;
use crate::protocol::RemoteFileSystem;
use crate::remote_path;
use crate::sftp::RemoteFs;

/// A folder's checksum files are read again after this long, in case they changed.
const FOLDER_LIFETIME: Duration = Duration::from_secs(60);
const MAX_SUMS_FILE: u32 = 4 * 1024 * 1024;
const MAX_SIDECAR: u32 = 64 * 1024;
/// Legacy minisign signatures cover the file itself, which then has to be read whole.
const MAX_LEGACY_SIGNED: u64 = 256 * 1024 * 1024;
const READ_CHUNK: usize = 256 * 1024;

/// Names of files that list checksums for the files of their folder.
const SUMS_FILES: [&str; 12] = [
    "SHA256SUMS",
    "SHA512SUMS",
    "SHA256SUMS.txt",
    "SHA512SUMS.txt",
    "sha256sums.txt",
    "sha512sums.txt",
    "sha256sum.txt",
    "sha512sum.txt",
    "CHECKSUMS",
    "CHECKSUMS.txt",
    "checksums.txt",
    "checksums.sha256",
];
/// Endings of a checksum file for the one file whose name comes before them.
const SIDECAR_SUFFIXES: [&str; 4] = [".sha256", ".sha256sum", ".sha512", ".sha512sum"];
const SIGNATURE_SUFFIXES: [&str; 3] = [".minisig", ".asc", ".sig"];

/// Where a download comes from, to list its folder and read the files beside it.
pub(super) enum Source<'a> {
    Sftp(&'a RemoteFs),
    Other(&'a Arc<dyn RemoteFileSystem>),
}

impl Source<'_> {
    async fn names(&self, folder: &str) -> AppResult<HashSet<String>> {
        let listing = match self {
            Self::Sftp(fs) => fs.list_dir(folder).await?,
            Self::Other(files) => files.list_dir(folder).await?,
        };
        Ok(listing
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect())
    }

    async fn read(&self, path: &str, limit: u32) -> AppResult<Vec<u8>> {
        match self {
            Self::Sftp(fs) => fs.read_range(path, 0, limit).await,
            Self::Other(files) => {
                let mut stream = Arc::clone(files)
                    .open_read(path, 0..u64::from(limit))
                    .await?;
                let mut data = BytesMut::new();
                while let Some(chunk) = stream.next_chunk().await? {
                    data.extend_from_slice(&chunk);
                    if data.len() >= limit as usize {
                        break;
                    }
                }
                stream.finish().await?;
                data.truncate(limit as usize);
                Ok(data.to_vec())
            }
        }
    }
}

/// What one folder publishes.
#[derive(Default)]
struct Folder {
    names: HashSet<String>,
    /// Digests listed in the folder's checksum files: file name to (algorithm, digest, list).
    listed: HashMap<String, Vec<Listed>>,
}

#[derive(Clone)]
struct Listed {
    algorithm: Algorithm,
    digest: Vec<u8>,
    /// The checksum file it came from.
    file: String,
}

/// A folder's published files, read once by whichever worker gets there first.
type FolderCell = Arc<OnceCell<Arc<Folder>>>;

/// Each folder's published files, with when they were first asked for.
#[derive(Default)]
pub struct FolderCache {
    folders: Mutex<HashMap<String, (Instant, FolderCell)>>,
}

impl FolderCache {
    fn cell(&self, folder: &str) -> FolderCell {
        let now = Instant::now();
        let mut folders = self.folders.lock().unwrap();
        folders.retain(|_, (read, _)| now.duration_since(*read) < FOLDER_LIFETIME);
        folders
            .entry(folder.to_string())
            .or_insert_with(|| (now, Arc::new(OnceCell::new())))
            .1
            .clone()
    }
}

async fn read_folder(source: &Source<'_>, folder: &str) -> AppResult<Folder> {
    let names = source.names(folder).await?;
    let mut listed: HashMap<String, Vec<Listed>> = HashMap::new();
    for sums in SUMS_FILES.iter().filter(|sums| names.contains(**sums)) {
        let path = remote_path::join(folder, sums);
        let Ok(contents) = source.read(&path, MAX_SUMS_FILE).await else {
            continue;
        };
        for (name, algorithm, digest) in parse_sums(&String::from_utf8_lossy(&contents)) {
            listed.entry(name).or_default().push(Listed {
                algorithm,
                digest,
                file: sums.to_string(),
            });
        }
    }
    Ok(Folder { names, listed })
}

/// Lines of `<hex>  <name>` or `<hex> *<name>` (GNU), or `SHA256 (<name>) = <hex>` (BSD). The
/// algorithm follows from the length of the hex digest, or the BSD tag.
fn parse_sums(text: &str) -> Vec<(String, Algorithm, Vec<u8>)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            parse_bsd_line(line).or_else(|| parse_gnu_line(line))
        })
        .collect()
}

fn parse_gnu_line(line: &str) -> Option<(String, Algorithm, Vec<u8>)> {
    let (hex, rest) = line.split_once(' ')?;
    let algorithm = algorithm_for_hex(hex)?;
    let name = rest.strip_prefix(['*', ' ']).unwrap_or(rest);
    let name = name.strip_prefix("./").unwrap_or(name);
    Some((name.to_string(), algorithm, parse_hex(hex)?))
}

fn parse_bsd_line(line: &str) -> Option<(String, Algorithm, Vec<u8>)> {
    let (tag, rest) = line.split_once(" (")?;
    let algorithm = match tag {
        "SHA256" => Algorithm::Sha256,
        "SHA512" => Algorithm::Sha512,
        _ => return None,
    };
    let (name, hex) = rest.rsplit_once(") = ")?;
    let name = name.strip_prefix("./").unwrap_or(name);
    (algorithm_for_hex(hex) == Some(algorithm))
        .then(|| Some((name.to_string(), algorithm, parse_hex(hex)?)))
        .flatten()
}

fn algorithm_for_hex(hex: &str) -> Option<Algorithm> {
    if !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }
    Algorithm::STRONG
        .into_iter()
        .find(|algorithm| algorithm.hex_len() == hex.len())
}

/// The digest in a checksum file for one file: its first line, which may name the file or be
/// the bare digest.
fn parse_sidecar(text: &str, name: &str) -> Option<(Algorithm, Vec<u8>)> {
    let line = text.lines().find(|line| !line.trim().is_empty())?.trim();
    if let Some(algorithm) = algorithm_for_hex(line) {
        return Some((algorithm, parse_hex(line)?));
    }
    let (listed, algorithm, digest) = parse_bsd_line(line).or_else(|| parse_gnu_line(line))?;
    let names_it = listed == name || listed.rsplit('/').next() == Some(name) || listed == "-";
    names_it.then_some((algorithm, digest))
}

/// Whether a name is itself one of the files this module reads, which is not checked.
fn is_published_file(name: &str) -> bool {
    SUMS_FILES.contains(&name)
        || SIDECAR_SUFFIXES
            .iter()
            .chain(SIGNATURE_SUFFIXES.iter())
            .any(|suffix| name.ends_with(suffix))
}

/// Checks a downloaded file, still under its temporary name at `local`, against what its
/// folder on the server publishes. A mismatch or a bad signature is an error that retrying
/// cannot fix; the rest is logged.
pub(super) async fn check(
    job: &JobRef<'_>,
    source: Source<'_>,
    remote: &str,
    local: &Path,
) -> AppResult<()> {
    let name = remote_path::file_name(remote).to_string();
    let Some(folder_path) = remote_path::parent(remote) else {
        return Ok(());
    };
    if is_published_file(&name) {
        return Ok(());
    }
    let target = job.shared.target(&job.spec.session_id)?;
    let folder = target
        .published
        .cell(&folder_path)
        .get_or_try_init(|| async { read_folder(&source, &folder_path).await.map(Arc::new) })
        .await?
        .clone();

    let mut expected = folder.listed.get(&name).cloned().unwrap_or_default();
    for suffix in SIDECAR_SUFFIXES {
        let sidecar = format!("{name}{suffix}");
        if !folder.names.contains(&sidecar) {
            continue;
        }
        let path = remote_path::join(&folder_path, &sidecar);
        let Ok(contents) = source.read(&path, MAX_SIDECAR).await else {
            continue;
        };
        if let Some((algorithm, digest)) = parse_sidecar(&String::from_utf8_lossy(&contents), &name)
        {
            expected.push(Listed {
                algorithm,
                digest,
                file: sidecar,
            });
        }
    }
    check_digests(job, &name, local, &expected).await?;

    let minisig = format!("{name}.minisig");
    if folder.names.contains(&minisig) {
        let path = remote_path::join(&folder_path, &minisig);
        if let Ok(contents) = source.read(&path, MAX_SIDECAR).await {
            let signature = String::from_utf8_lossy(&contents).into_owned();
            check_minisign(job, &name, local, &signature).await?;
        }
    }
    for suffix in [".asc", ".sig"] {
        let signature = format!("{name}{suffix}");
        if !folder.names.contains(&signature) {
            continue;
        }
        let path = remote_path::join(&folder_path, &signature);
        if let Ok(contents) = source.read(&path, MAX_SIDECAR).await {
            check_openpgp(job, &name, local, &contents).await?;
        }
    }
    Ok(())
}

fn untrusted(message: String) -> AppError {
    AppError::new(ErrorKind::Untrusted, message)
}

fn log(job: &JobRef<'_>, level: LogLevel, message: String) {
    job.shared
        .events
        .log(level, Some(&job.spec.session_id), message);
}

async fn check_digests(
    job: &JobRef<'_>,
    name: &str,
    local: &Path,
    expected: &[Listed],
) -> AppResult<()> {
    let mut computed: HashMap<&'static str, Vec<u8>> = HashMap::new();
    for listed in expected {
        let label = listed.algorithm.label();
        if !computed.contains_key(label) {
            let path = local.to_path_buf();
            let algorithm = listed.algorithm;
            let digest = tokio::task::spawn_blocking(move || hash_file(&path, algorithm))
                .await?
                .map_err(|error| AppError::from(error).with_path(local.to_string_lossy()))?;
            computed.insert(label, digest);
        }
        if computed[label] != listed.digest {
            return Err(untrusted(format!(
                "{name} does not match the {label} checksum published in {}, so it was not saved",
                listed.file
            )));
        }
        log(
            job,
            LogLevel::Info,
            format!(
                "{name} matches the {label} checksum published in {}",
                listed.file
            ),
        );
    }
    Ok(())
}

enum SignatureCheck {
    Good(String),
    Bad,
    /// Signed by a key that is not among the trusted ones.
    UnknownKey,
    /// The signature or a key could not be read.
    Unreadable(String),
}

async fn check_minisign(
    job: &JobRef<'_>,
    name: &str,
    local: &Path,
    signature: &str,
) -> AppResult<()> {
    let keys = job.settings.minisign_keys.clone();
    if keys.is_empty() {
        log(
            job,
            LogLevel::Info,
            format!("{name} has a minisign signature; add its publisher's public key in Settings > Transfers to check it"),
        );
        return Ok(());
    }
    let path = local.to_path_buf();
    let signature = signature.to_string();
    let checked =
        tokio::task::spawn_blocking(move || verify_minisign(&keys, &signature, &path)).await?;
    match checked {
        SignatureCheck::Good(comment) => {
            log(
                job,
                LogLevel::Info,
                format!("{name} has a good minisign signature ({comment})"),
            );
            Ok(())
        }
        SignatureCheck::Bad => Err(untrusted(format!(
            "The minisign signature of {name} does not match the file, so it was not saved"
        ))),
        SignatureCheck::UnknownKey => {
            log(
                job,
                LogLevel::Info,
                format!("{name} is signed with a minisign key that is not among the trusted keys in Settings > Transfers, so the signature was not checked"),
            );
            Ok(())
        }
        SignatureCheck::Unreadable(problem) => {
            log(
                job,
                LogLevel::Warn,
                format!("Could not check the minisign signature of {name}: {problem}"),
            );
            Ok(())
        }
    }
}

/// A key as pasted from a `minisign.pub` file: the base64 line, or both lines.
fn parse_minisign_key(text: &str) -> Option<PublicKey> {
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))?;
    PublicKey::from_base64(line).ok()
}

/// Blocking: reads the file.
fn verify_minisign(keys: &[String], signature: &str, path: &Path) -> SignatureCheck {
    let signature = match Signature::decode(signature) {
        Ok(signature) => signature,
        Err(error) => return SignatureCheck::Unreadable(error.to_string()),
    };
    let comment = signature.trusted_comment().to_string();
    for key in keys.iter().filter_map(|key| parse_minisign_key(key)) {
        let result =
            match key.verify_stream(&signature) {
                Ok(mut verifier) => stream_file(path, |data| verifier.update(data))
                    .map_err(|error| error.to_string())
                    .map(|()| verifier.finalize()),
                Err(minisign_verify::Error::UnexpectedKeyId) => continue,
                Err(minisign_verify::Error::UnsupportedLegacyMode) => {
                    match std::fs::metadata(path) {
                    Ok(metadata) if metadata.len() <= MAX_LEGACY_SIGNED => std::fs::read(path)
                        .map_err(|error| error.to_string())
                        .map(|data| key.verify(&data, &signature, true)),
                    _ => Err("the signature is in minisign's legacy form, which covers the whole file, and the file is too large to check that way".to_string()),
                }
                }
                Err(error) => Err(error.to_string()),
            };
        return match result {
            Ok(Ok(())) => SignatureCheck::Good(comment),
            Ok(Err(_)) => SignatureCheck::Bad,
            Err(problem) => SignatureCheck::Unreadable(problem),
        };
    }
    SignatureCheck::UnknownKey
}

fn stream_file(path: &Path, mut each: impl FnMut(&[u8])) -> std::io::Result<()> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        each(&buffer[..read]);
    }
}

async fn check_openpgp(
    job: &JobRef<'_>,
    name: &str,
    local: &Path,
    signature: &[u8],
) -> AppResult<()> {
    let folder = tempfile_folder()?;
    let signature_path = folder.join("signature");
    tokio::fs::write(&signature_path, signature).await?;
    let checked = run_gpg(&signature_path, local).await;
    let _ = tokio::fs::remove_dir_all(&folder).await;
    match checked {
        Some(SignatureCheck::Good(signer)) => {
            log(
                job,
                LogLevel::Info,
                format!("{name} has a good OpenPGP signature from {signer}"),
            );
            Ok(())
        }
        Some(SignatureCheck::Bad) => Err(untrusted(format!(
            "The OpenPGP signature of {name} does not match the file, or its key was revoked, so it was not saved"
        ))),
        Some(SignatureCheck::UnknownKey) => {
            log(
                job,
                LogLevel::Info,
                format!("{name} is signed with an OpenPGP key that is not in this computer's keyring, so the signature was not checked"),
            );
            Ok(())
        }
        Some(SignatureCheck::Unreadable(problem)) => {
            log(
                job,
                LogLevel::Warn,
                format!("Could not check the OpenPGP signature of {name}: {problem}"),
            );
            Ok(())
        }
        // No gpg here, or the file beside it is not an OpenPGP signature.
        None => Ok(()),
    }
}

/// A private folder for the signature while gpg reads it.
fn tempfile_folder() -> AppResult<PathBuf> {
    let folder =
        std::env::temp_dir().join(format!("poros-signature-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir(&folder)?;
    Ok(folder)
}

/// `None` when gpg is not installed or found no OpenPGP signature.
async fn run_gpg(signature: &Path, file: &Path) -> Option<SignatureCheck> {
    let mut process = tokio::process::Command::new("gpg");
    process
        .args(["--batch", "--no-tty", "--status-fd", "1", "--verify"])
        .arg(signature)
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    process.creation_flags(0x0800_0000);
    let output = process.output().await.ok()?;
    parse_gpg_status(&String::from_utf8_lossy(&output.stdout))
}

fn parse_gpg_status(status: &str) -> Option<SignatureCheck> {
    let mut found = None;
    for line in status.lines() {
        let Some(line) = line.strip_prefix("[GNUPG:] ") else {
            continue;
        };
        let (keyword, rest) = line.split_once(' ').unwrap_or((line, ""));
        let signer = || {
            rest.split_once(' ')
                .map_or(rest, |(_, signer)| signer)
                .to_string()
        };
        match keyword {
            "BADSIG" | "REVKEYSIG" => return Some(SignatureCheck::Bad),
            "GOODSIG" | "EXPKEYSIG" => found = Some(SignatureCheck::Good(signer())),
            "NO_PUBKEY" => found = found.or(Some(SignatureCheck::UnknownKey)),
            // The sixth field is the reason; 9 is a missing public key.
            "ERRSIG" if rest.split_whitespace().nth(5) == Some("9") => {
                found = found.or(Some(SignatureCheck::UnknownKey))
            }
            "ERRSIG" => {
                found = found.or(Some(SignatureCheck::Unreadable(
                    "gpg could not check it".into(),
                )))
            }
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn reads_gnu_and_bsd_checksum_lists() {
        let listing = format!(
            "{ABC_SHA256}  poros.tar.gz\r\n{ABC_SHA256} *./poros.deb\nSHA256 (poros.rpm) = {ABC_SHA256}\n\
             SHA1 (old.rpm) = a9993e364706816aba3e25717850c26c9cd0d89d\n\
             a9993e364706816aba3e25717850c26c9cd0d89d  weak.bin\nnot a line\n"
        );
        let parsed = parse_sums(&listing);
        let names: Vec<&str> = parsed.iter().map(|(name, _, _)| name.as_str()).collect();
        assert_eq!(names, ["poros.tar.gz", "poros.deb", "poros.rpm"]);
        assert!(parsed
            .iter()
            .all(|(_, algorithm, digest)| *algorithm == Algorithm::Sha256 && digest.len() == 32));
    }

    #[test]
    fn reads_a_checksum_file_for_one_file() {
        assert!(parse_sidecar(&format!("{ABC_SHA256}\n"), "a.bin").is_some());
        assert!(parse_sidecar(&format!("{ABC_SHA256}  a.bin\n"), "a.bin").is_some());
        assert!(parse_sidecar(&format!("{ABC_SHA256}  dist/a.bin\n"), "a.bin").is_some());
        assert!(parse_sidecar(&format!("{ABC_SHA256}  b.bin\n"), "a.bin").is_none());
        assert!(is_published_file("SHA256SUMS"));
        assert!(is_published_file("a.bin.minisig"));
        assert!(!is_published_file("a.bin"));
    }

    #[test]
    fn checks_minisign_signatures_against_trusted_keys() {
        // Made with minisign 0.11: a key, and signatures of "test" in the current (pre-hashed)
        // and the legacy form.
        let key = "untrusted comment: minisign public key 5AFDFB2B9F589E85\n\
                   RWSFnlifK/v9WrmhvEB4DeXXEaTspaCu+Nqjr78raegjsROt2CILH1Fu\n";
        let signature = "untrusted comment: signature from minisign secret key\n\
RUSFnlifK/v9WuHGh691QJojrJUl0dG7Wk8r3X8+z9q+ZS8uSqWXcbRabb3PS6Sg9ZeO+cun0nIm6b2t73Wjfr9Lktj34NIgRgs=\n\
trusted comment: file:test\n\
b8QN6w9nN4iHOGzT0ZtAWu0ompwKIV0AbzYpASmLBK/h/eoAYgEHHv1o8tnMS49ecYBqp8a2A9O1g2hbm/0qCg==\n";
        let legacy = "untrusted comment: signature from minisign secret key\n\
RWSFnlifK/v9WktXDSUqfqOsBhbfwHpxWvBHzFOZAluq0v/Q48Dq98sgqDva+59tWqVeyAcv5wI4anePV9Gmwpc7sgVwzgtF7wk=\n\
trusted comment: legacy\n\
W8zbbsQtdwmg+jUULrJW9P1CKBEVjzKJMO2as5sTj/J0DJoxRJoh7jKlLvqSPWvN8OJ2GgrIiCTGX5+us3YFAg==\n";
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("test");
        std::fs::write(&file, b"test").unwrap();
        let trusted = [key.to_string()];
        assert!(matches!(
            verify_minisign(&trusted, signature, &file),
            SignatureCheck::Good(comment) if comment == "file:test"
        ));
        assert!(matches!(
            verify_minisign(&trusted, legacy, &file),
            SignatureCheck::Good(_)
        ));
        std::fs::write(&file, b"tset").unwrap();
        assert!(matches!(
            verify_minisign(&trusted, signature, &file),
            SignatureCheck::Bad
        ));
        let other = ["RWS/niVkxpK9wVVEE3117xI8SWxoH03TKE/xFMTq0ujfzxTOYeXdWLYF".to_string()];
        assert!(matches!(
            verify_minisign(&other, signature, &file),
            SignatureCheck::UnknownKey
        ));
        assert!(matches!(
            verify_minisign(&trusted, "not a signature", &file),
            SignatureCheck::Unreadable(_)
        ));
    }

    #[test]
    fn reads_gpg_status_lines() {
        let good = "[GNUPG:] NEWSIG\n[GNUPG:] GOODSIG 0123456789ABCDEF Release Key <release@example.org>\n[GNUPG:] VALIDSIG ABC\n";
        assert!(matches!(
            parse_gpg_status(good),
            Some(SignatureCheck::Good(signer)) if signer == "Release Key <release@example.org>"
        ));
        let bad = "[GNUPG:] NEWSIG\n[GNUPG:] BADSIG 0123456789ABCDEF Release Key\n";
        assert!(matches!(parse_gpg_status(bad), Some(SignatureCheck::Bad)));
        let unknown = "[GNUPG:] NEWSIG\n[GNUPG:] ERRSIG 0123456789ABCDEF 22 10 00 1700000000 9 -\n";
        assert!(matches!(
            parse_gpg_status(unknown),
            Some(SignatureCheck::UnknownKey)
        ));
        assert!(parse_gpg_status("[GNUPG:] NODATA 1\n").is_none());
    }
}
