//! Cryptomator vaults, checked against the official Cryptomator libraries in both directions.
//!
//! The vaults in `tests/fixtures/vaults` were made by Cryptomator's own `cryptofs` library,
//! through the `create` and `populate` commands of the harness in `tests/cryptomator-harness`,
//! and each `manifest.json` beside one is that harness's `dump`: what the library reads out of
//! the vault. The tests here read the same vaults and must find exactly that.
//!
//! The other direction needs Java, so it is skipped unless `POROS_TEST_CRYPTOMATOR_JAR` names
//! the harness built from `tests/cryptomator-harness`: vaults made and changed here are then
//! read by `cryptofs`, and vaults it changes are read back here.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use poros_lib::cryptomator::VaultFs;
use poros_lib::error::{AppError, AppResult, ErrorKind};
use poros_lib::model::{DirListing, EntryKind, FileEntry};
use poros_lib::protocol::{
    self, Capabilities, Protocol, ReadStream, RemoteFileSystem, RemoteStat, WriteRequest,
    WriteStream,
};
use poros_lib::remote_path;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const FIXTURE_PASSWORD: &str = "poros-test-password";
const CHUNK: u64 = 32 * 1024;

/// A folder on this computer, served the way the strictest servers serve theirs: a folder is
/// made only inside one that exists, a file is written only into a folder that exists, and what
/// is read arrives in pieces that line up with nothing.
struct DiskServer {
    folder: PathBuf,
    /// The size of the pieces a read arrives in.
    piece: usize,
}

impl DiskServer {
    fn serving(folder: &Path) -> Arc<dyn RemoteFileSystem> {
        Self::in_pieces(folder, 5003)
    }

    fn in_pieces(folder: &Path, piece: usize) -> Arc<dyn RemoteFileSystem> {
        Arc::new(Self {
            folder: folder.to_path_buf(),
            piece,
        })
    }

    fn on_disk(&self, path: &str) -> PathBuf {
        let mut on_disk = self.folder.clone();
        on_disk.extend(
            protocol::normalize(path)
                .split('/')
                .filter(|name| !name.is_empty()),
        );
        on_disk
    }
}

fn seconds(time: std::io::Result<SystemTime>) -> Option<i64> {
    let since_epoch = time.ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(since_epoch.as_secs() as i64)
}

/// A path that leads through a file is not reported as missing, as on some servers it is not.
fn failed(error: std::io::Error, path: &str) -> AppError {
    AppError::from(error).with_path(path)
}

#[async_trait]
impl RemoteFileSystem for DiskServer {
    fn protocol(&self) -> Protocol {
        Protocol::WebDav
    }

    fn home(&self) -> &str {
        "/"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            ranged_reads: true,
            resumable_writes: true,
        }
    }

    async fn canonicalize(&self, path: &str) -> AppResult<String> {
        Ok(protocol::normalize(path))
    }

    async fn list_dir(&self, path: &str) -> AppResult<DirListing> {
        let path = protocol::normalize(path);
        let mut entries = Vec::new();
        for child in std::fs::read_dir(self.on_disk(&path)).map_err(|error| failed(error, &path))? {
            let child = child?;
            let attributes = child.metadata()?;
            let name = child.file_name().to_string_lossy().into_owned();
            entries.push(FileEntry {
                path: remote_path::join(&path, &name),
                hidden: name.starts_with('.'),
                name,
                kind: match attributes.is_dir() {
                    true => EntryKind::Dir,
                    false => EntryKind::File,
                },
                link_target: None,
                size: attributes.len(),
                modified: seconds(attributes.modified()),
                permissions: None,
                owner: None,
                group: None,
            });
        }
        Ok(DirListing {
            parent: remote_path::parent(&path),
            path,
            entries,
        })
    }

    async fn make_dir(&self, parent: &str, name: &str) -> AppResult<String> {
        let path = remote_path::join(&protocol::normalize(parent), name);
        std::fs::create_dir(self.on_disk(&path)).map_err(|error| failed(error, &path))?;
        Ok(path)
    }

    async fn rename(&self, path: &str, new_name: &str) -> AppResult<String> {
        let path = protocol::normalize(path);
        let renamed = remote_path::join(&remote_path::parent(&path).unwrap_or_default(), new_name);
        if self.on_disk(&renamed).exists() {
            return Err(protocol::already_exists(&renamed));
        }
        std::fs::rename(self.on_disk(&path), self.on_disk(&renamed))
            .map_err(|error| failed(error, &path))?;
        Ok(renamed)
    }

    async fn delete(&self, paths: &[String]) -> AppResult<()> {
        for path in paths {
            let on_disk = self.on_disk(path);
            let removed = match std::fs::symlink_metadata(&on_disk) {
                Ok(attributes) if attributes.is_dir() => std::fs::remove_dir_all(&on_disk),
                Ok(_) => std::fs::remove_file(&on_disk),
                Err(error) => Err(error),
            };
            removed.map_err(|error| failed(error, path))?;
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> AppResult<Option<RemoteStat>> {
        match std::fs::metadata(self.on_disk(path)) {
            Ok(attributes) => Ok(Some(RemoteStat {
                is_dir: attributes.is_dir(),
                size: attributes.len(),
                modified: seconds(attributes.modified()),
                permissions: None,
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failed(error, path)),
        }
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<()> {
        let on_disk = self.on_disk(path);
        match std::fs::create_dir(&on_disk) {
            Err(error) if !on_disk.is_dir() => Err(failed(error, path)),
            _ => Ok(()),
        }
    }

    async fn set_attributes(
        &self,
        path: &str,
        modified: Option<i64>,
        _permissions: Option<u32>,
    ) -> AppResult<()> {
        if let Some(modified) = modified {
            let file = std::fs::File::options()
                .write(true)
                .open(self.on_disk(path))
                .map_err(|error| failed(error, path))?;
            file.set_modified(UNIX_EPOCH + Duration::from_secs(modified as u64))?;
        }
        Ok(())
    }

    async fn open_read(
        self: Arc<Self>,
        path: &str,
        range: Range<u64>,
    ) -> AppResult<Box<dyn ReadStream>> {
        let contents = std::fs::read(self.on_disk(path)).map_err(|error| failed(error, path))?;
        let end = (range.end.min(contents.len() as u64)) as usize;
        let start = (range.start as usize).min(end);
        Ok(Box::new(DiskRead {
            contents: Bytes::from(contents).slice(start..end),
            piece: self.piece,
        }))
    }

    async fn open_write(self: Arc<Self>, request: WriteRequest) -> AppResult<Box<dyn WriteStream>> {
        let on_disk = self.on_disk(&request.path);
        if on_disk.is_dir() {
            return Err(AppError::new(ErrorKind::Io, "Is a directory").with_path(request.path));
        }
        let kept = match request.offset {
            0 => Vec::new(),
            offset => {
                let mut kept = std::fs::read(&on_disk)?;
                kept.truncate(offset as usize);
                kept
            }
        };
        // Like a server, the file is there from the moment it is opened.
        std::fs::write(&on_disk, &kept).map_err(|error| failed(error, &request.path))?;
        Ok(Box::new(DiskWrite {
            on_disk,
            contents: kept,
            announced: request.offset + request.size,
            modified: request.modified,
        }))
    }
}

struct DiskRead {
    contents: Bytes,
    piece: usize,
}

#[async_trait]
impl ReadStream for DiskRead {
    async fn next_chunk(&mut self) -> AppResult<Option<Bytes>> {
        if self.contents.is_empty() {
            return Ok(None);
        }
        let length = self.piece.min(self.contents.len());
        Ok(Some(self.contents.split_to(length)))
    }

    async fn finish(self: Box<Self>) -> AppResult<()> {
        Ok(())
    }
}

struct DiskWrite {
    on_disk: PathBuf,
    contents: Vec<u8>,
    /// The size the writer said the file would have, which some servers hold it to.
    announced: u64,
    modified: Option<i64>,
}

#[async_trait]
impl WriteStream for DiskWrite {
    async fn write(&mut self, data: Bytes) -> AppResult<()> {
        self.contents.extend_from_slice(&data);
        Ok(())
    }

    async fn finish(self: Box<Self>) -> AppResult<()> {
        if self.contents.len() as u64 != self.announced {
            return Err(AppError::new(
                ErrorKind::Io,
                format!(
                    "{} bytes were announced and {} written",
                    self.announced,
                    self.contents.len()
                ),
            ));
        }
        std::fs::write(&self.on_disk, &self.contents)?;
        if let Some(modified) = self.modified {
            let file = std::fs::File::options().write(true).open(&self.on_disk)?;
            file.set_modified(UNIX_EPOCH + Duration::from_secs(modified as u64))?;
        }
        Ok(())
    }
}

/// One entry of a manifest written by the harness on Cryptomator's own library.
#[derive(Debug, Deserialize)]
struct Listed {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    sha256: String,
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/vaults")
        .join(name)
}

fn manifest(fixture: &Path) -> Vec<Listed> {
    let manifest = std::fs::read(fixture.join("manifest.json")).unwrap();
    serde_json::from_slice(&manifest).unwrap()
}

fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for child in std::fs::read_dir(from).unwrap() {
        let child = child.unwrap();
        let target = to.join(child.file_name());
        if child.file_type().unwrap().is_dir() {
            copy_folder(&child.path(), &target);
        } else {
            std::fs::copy(child.path(), target).unwrap();
        }
    }
}

/// A fixture vault copied to a folder of its own, so a test can damage or change it.
fn fixture_copy(name: &str) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    copy_folder(&fixture(name).join("vault"), &copy.path().join("vault"));
    copy
}

fn sha256(contents: &[u8]) -> String {
    Sha256::digest(contents)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Contents that differ from chunk to chunk and from file to file.
fn contents(length: usize, seed: u8) -> Vec<u8> {
    (0..length)
        .map(|position| (position as u32).wrapping_mul(2654435761).to_be_bytes()[0] ^ seed)
        .collect()
}

async fn read_range(files: &Arc<VaultFs>, path: &str, range: Range<u64>) -> AppResult<Vec<u8>> {
    let mut stream = files.clone().open_read(path, range).await?;
    let mut read = Vec::new();
    while let Some(chunk) = stream.next_chunk().await? {
        assert!(
            !chunk.is_empty(),
            "an empty piece would end a transfer early"
        );
        read.extend_from_slice(&chunk);
    }
    stream.finish().await?;
    Ok(read)
}

async fn read(files: &Arc<VaultFs>, path: &str) -> AppResult<Vec<u8>> {
    read_range(files, path, 0..u64::MAX).await
}

/// Writes a file in pieces of `piece` bytes, as a transfer would hand them over.
async fn write_in_pieces(
    files: &Arc<VaultFs>,
    path: &str,
    contents: &[u8],
    piece: usize,
) -> AppResult<()> {
    let mut stream = files
        .clone()
        .open_write(WriteRequest {
            path: path.to_string(),
            offset: 0,
            size: contents.len() as u64,
            modified: Some(1_700_000_000),
            permissions: Some(0o600),
        })
        .await?;
    for piece in contents.chunks(piece) {
        stream.write(Bytes::copy_from_slice(piece)).await?;
    }
    stream.finish().await
}

async fn write(files: &Arc<VaultFs>, path: &str, contents: &[u8]) -> AppResult<()> {
    write_in_pieces(files, path, contents, 100_000).await
}

/// Every entry below `folder`, by its path inside the vault, with the kind and size listed and,
/// for a file, the SHA-256 of what reads out of it.
async fn walk(files: &Arc<VaultFs>, folder: &str) -> BTreeMap<String, (String, u64, String)> {
    let root = files.root().to_string();
    let mut found = BTreeMap::new();
    let mut folders = vec![folder.to_string()];
    while let Some(folder) = folders.pop() {
        let listing = files.list_dir(&folder).await.unwrap();
        assert_eq!(listing.path, folder);
        for entry in listing.entries {
            assert_eq!(entry.path, remote_path::join(&folder, &entry.name));
            let inside = entry.path.strip_prefix(&root).unwrap().to_string();
            let listed = match entry.kind {
                EntryKind::Dir => match files.list_dir(&entry.path).await {
                    Ok(_) => {
                        folders.push(entry.path.clone());
                        ("dir".to_string(), 0, String::new())
                    }
                    // A link shows as a folder until it is opened.
                    Err(error) if error.kind == ErrorKind::Unsupported => {
                        ("symlink".to_string(), 0, String::new())
                    }
                    Err(error) => panic!("{}: {error:?}", entry.path),
                },
                EntryKind::Symlink => ("symlink".to_string(), 0, String::new()),
                _ => {
                    let read = read(files, &entry.path).await.unwrap();
                    assert_eq!(read.len() as u64, entry.size, "{}", entry.path);
                    ("file".to_string(), entry.size, sha256(&read))
                }
            };
            assert!(found.insert(inside, listed).is_none(), "listed twice");
        }
    }
    found
}

fn expected(manifest: &[Listed]) -> BTreeMap<String, (String, u64, String)> {
    manifest
        .iter()
        .map(|listed| {
            (
                listed.path.clone(),
                (listed.kind.clone(), listed.size, listed.sha256.clone()),
            )
        })
        .collect()
}

#[tokio::test]
async fn reads_what_cryptomator_wrote() {
    for name in ["gcm", "ctrmac"] {
        let fixture = fixture(name);
        let listed = manifest(&fixture);
        assert!(listed.len() > 50, "{name}: the fixture lists too little");
        for piece in [1 << 20, 5003, 61] {
            let server = DiskServer::in_pieces(&fixture, piece);
            let vault = VaultFs::unlock(server, "/vault", FIXTURE_PASSWORD.to_string())
                .await
                .unwrap();
            assert_eq!(vault.root(), "/vault");
            assert_eq!(vault.name(), "vault");
            assert_eq!(
                walk(&vault, "/vault").await,
                expected(&listed),
                "{name} read in pieces of {piece}"
            );
        }
    }
}

#[tokio::test]
async fn stat_agrees_with_cryptomator() {
    for name in ["gcm", "ctrmac"] {
        let fixture = fixture(name);
        let vault = VaultFs::unlock(
            DiskServer::serving(&fixture),
            "/vault",
            FIXTURE_PASSWORD.to_string(),
        )
        .await
        .unwrap();
        for listed in manifest(&fixture) {
            let path = format!("/vault{}", listed.path);
            let stat = vault.stat(&path).await.unwrap().unwrap();
            match listed.kind.as_str() {
                "file" => {
                    assert!(!stat.is_dir, "{path}");
                    assert_eq!(stat.size, listed.size, "{path}");
                    assert!(stat.modified.is_some(), "{path}");
                }
                _ => assert!(stat.is_dir, "{path}"),
            }
        }
        assert!(vault.stat("/vault").await.unwrap().unwrap().is_dir);
        assert_eq!(vault.stat("/vault/absent.txt").await.unwrap(), None);
        assert_eq!(vault.stat("/vault/absent/below.txt").await.unwrap(), None);
        assert_eq!(vault.stat("/vault/hello.txt/below").await.unwrap(), None);
    }
}

#[tokio::test]
async fn reads_any_part_of_a_file() {
    for name in ["gcm", "ctrmac"] {
        let fixture = fixture(name);
        let vault = VaultFs::unlock(
            DiskServer::serving(&fixture),
            "/vault",
            FIXTURE_PASSWORD.to_string(),
        )
        .await
        .unwrap();
        let whole = read(&vault, "/vault/large.bin").await.unwrap();
        let size = whole.len() as u64;
        assert_eq!(size, 250_000);
        let ranges = [
            0..1,
            0..CHUNK,
            0..CHUNK + 1,
            1..CHUNK,
            CHUNK - 1..CHUNK + 1,
            CHUNK..2 * CHUNK,
            CHUNK + 5..CHUNK + 6,
            3 * CHUNK - 1..5 * CHUNK + 7,
            7 * CHUNK..size,
            size - 1..size,
            100_000..size + 1000,
            0..size,
        ];
        for range in ranges {
            let part = read_range(&vault, "/vault/large.bin", range.clone())
                .await
                .unwrap();
            let end = range.end.min(size) as usize;
            assert_eq!(part, whole[range.start as usize..end], "{name} {range:?}");
        }
        for range in [size..size + 10, 5..5, 2 * size..3 * size] {
            let part = read_range(&vault, "/vault/large.bin", range.clone())
                .await
                .unwrap();
            assert!(part.is_empty(), "{name} {range:?}");
        }
        assert!(read(&vault, "/vault/empty.txt").await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn the_password_opens_the_vault_in_either_unicode_form() {
    let fixture = fixture("unicode-password");
    let hex = std::fs::read_to_string(fixture.join("password-bytes.hex")).unwrap();
    let typed: Vec<u8> = (0..hex.trim().len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex.trim()[at..at + 2], 16).unwrap())
        .collect();
    let decomposed = String::from_utf8(typed).unwrap();
    let composed = "p\u{e4}ssw\u{f6}rd-\u{5bc6}\u{7801}-\u{e9}".to_string();
    assert_ne!(decomposed, composed);
    for password in [decomposed, composed] {
        let vault = VaultFs::unlock(DiskServer::serving(&fixture), "/vault", password)
            .await
            .unwrap();
        assert!(vault.list_dir("/vault").await.unwrap().entries.is_empty());
    }
}

#[tokio::test]
async fn a_wrong_password_is_told_apart_from_a_damaged_vault() {
    let copy = fixture_copy("gcm");
    let server = DiskServer::serving(copy.path());
    let unlock = |password: &str| VaultFs::unlock(server.clone(), "/vault", password.to_string());

    let wrong = unlock("poros-test-passwore").await.err().unwrap();
    assert_eq!(wrong.kind, ErrorKind::WrongPassword);

    let not_a_vault = VaultFs::unlock(server.clone(), "/vault/d", FIXTURE_PASSWORD.to_string())
        .await
        .err()
        .unwrap();
    assert_eq!(not_a_vault.kind, ErrorKind::NotFound);

    // A config signed by another key is not this vault's, whatever it says.
    let config = copy.path().join("vault/vault.cryptomator");
    let mut token = std::fs::read_to_string(&config).unwrap();
    let last = token.pop().unwrap();
    token.push(if last == 'A' { 'B' } else { 'A' });
    std::fs::write(&config, token).unwrap();
    let forged = unlock(FIXTURE_PASSWORD).await.err().unwrap();
    assert_eq!(forged.kind, ErrorKind::Integrity);

    std::fs::remove_file(&config).unwrap();
    let older = unlock(FIXTURE_PASSWORD).await.err().unwrap();
    assert_eq!(older.kind, ErrorKind::Unsupported);
}

#[tokio::test]
async fn damage_to_a_file_is_noticed() {
    for name in ["gcm", "ctrmac"] {
        let copy = fixture_copy(name);
        let server = DiskServer::serving(copy.path());
        let vault = VaultFs::unlock(server.clone(), "/vault", FIXTURE_PASSWORD.to_string())
            .await
            .unwrap();
        let overhead = match name {
            "gcm" => (68, 28),
            _ => (88, 48),
        };
        let (header, chunk_overhead) = overhead;
        let stored_chunk = CHUNK as usize + chunk_overhead;

        // The stored file of large.bin is the only one of its size.
        let stored_size = header + 250_000 + 8 * chunk_overhead;
        let mut stored = None;
        let mut folders = vec![copy.path().join("vault/d")];
        while let Some(folder) = folders.pop() {
            for child in std::fs::read_dir(folder).unwrap() {
                let child = child.unwrap();
                let attributes = child.metadata().unwrap();
                if attributes.is_dir() {
                    folders.push(child.path());
                } else if attributes.len() == stored_size as u64 {
                    stored = Some(child.path());
                }
            }
        }
        let stored = stored.unwrap();
        let intact = std::fs::read(&stored).unwrap();
        let whole = read(&vault, "/vault/large.bin").await.unwrap();

        let damaged = |change: &dyn Fn(&mut Vec<u8>)| {
            let mut changed = intact.clone();
            change(&mut changed);
            std::fs::write(&stored, changed).unwrap();
        };
        let second_chunk = header + stored_chunk;

        // One bit, in the header, in a chunk's contents and in a chunk's check.
        for position in [
            3,
            header - 1,
            second_chunk + 100,
            second_chunk + stored_chunk - 1,
        ] {
            damaged(&|changed| changed[position] ^= 0x10);
            let error = read(&vault, "/vault/large.bin").await.err().unwrap();
            assert_eq!(error.kind, ErrorKind::Integrity, "{name} bit at {position}");
        }

        // Two chunks changing places.
        damaged(&|changed| {
            let (first, rest) = changed[header..].split_at_mut(stored_chunk);
            first.swap_with_slice(&mut rest[..stored_chunk]);
        });
        let error = read(&vault, "/vault/large.bin").await.err().unwrap();
        assert_eq!(error.kind, ErrorKind::Integrity, "{name} swapped chunks");
        // The chunks after them are still read on their own.
        let later = read_range(&vault, "/vault/large.bin", 2 * CHUNK..3 * CHUNK)
            .await
            .unwrap();
        assert_eq!(later, whole[2 * CHUNK as usize..3 * CHUNK as usize]);

        // A file cut off inside a chunk.
        damaged(&|changed| changed.truncate(second_chunk + 1000));
        let error = read(&vault, "/vault/large.bin").await.err().unwrap();
        assert_eq!(error.kind, ErrorKind::Integrity, "{name} cut short");

        // A file cut off inside its header.
        damaged(&|changed| changed.truncate(header - 1));
        for range in [0..10, 0..0, CHUNK..CHUNK + 10] {
            let error = read_range(&vault, "/vault/large.bin", range)
                .await
                .err()
                .unwrap();
            assert_eq!(error.kind, ErrorKind::Integrity, "{name} header cut short");
        }

        // Another file's contents under this file's name: contents are not tied to names, as
        // Cryptomator's own are not, but they read back whole or not at all.
        damaged(&|_| {});
        assert_eq!(read(&vault, "/vault/large.bin").await.unwrap(), whole);
    }
}

async fn new_vault(server: &Arc<dyn RemoteFileSystem>, password: &str) -> Arc<VaultFs> {
    server.ensure_dir("/storage").await.unwrap();
    VaultFs::create(server.clone(), "/storage", "My vault", password.to_string())
        .await
        .unwrap()
}

/// A name whose encrypted form is longer than a vault stores as a file name.
fn long_name(stem: &str) -> String {
    format!("{stem}-{}.txt", "long ".repeat(40))
}

#[tokio::test]
async fn a_new_vault_keeps_what_is_put_in_it() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "correct horse").await;
    let root = "/storage/My vault";
    assert_eq!(vault.root(), root);
    assert_eq!(vault.name(), "My vault");
    assert_eq!(vault.home(), root);
    assert_eq!(vault.protocol(), Protocol::Cryptomator);
    assert_eq!(vault.transport(), Protocol::WebDav);
    assert!(vault.list_dir(root).await.unwrap().entries.is_empty());
    assert_eq!(
        vault.list_dir(root).await.unwrap().parent.as_deref(),
        Some("/storage")
    );

    let sizes = [0, 1, 5, 32_767, 32_768, 32_769, 65_536, 100_000, 300_001];
    vault.make_dir(root, "Folder").await.unwrap();
    vault.ensure_dir(&format!("{root}/a/b/c")).await.unwrap();
    let mut written = BTreeMap::new();
    for (number, size) in sizes.into_iter().enumerate() {
        let path = match number % 3 {
            0 => format!("{root}/file-{size}.bin"),
            1 => format!("{root}/Folder/file-{size}.bin"),
            _ => format!("{root}/a/b/c/{}", long_name(&size.to_string())),
        };
        let cleartext = contents(size, number as u8);
        // Pieces of every awkward size: smaller than a chunk, a chunk, and several at once.
        let piece = [1000, 32_768, 100_000, 7][number % 4];
        write_in_pieces(&vault, &path, &cleartext, piece)
            .await
            .unwrap();
        written.insert(path, cleartext);
    }

    for (path, cleartext) in &written {
        assert_eq!(&read(&vault, path).await.unwrap(), cleartext, "{path}");
        let stat = vault.stat(path).await.unwrap().unwrap();
        assert_eq!(stat.size, cleartext.len() as u64, "{path}");
        assert_eq!(stat.modified, Some(1_700_000_000), "{path}");
        assert!(!stat.is_dir);
    }

    // The same vault opened afresh, as after a restart, shows the same.
    let reopened = VaultFs::unlock(server.clone(), root, "correct horse".to_string())
        .await
        .unwrap();
    let found = walk(&reopened, root).await;
    for (path, cleartext) in &written {
        let inside = path.strip_prefix(root).unwrap();
        let (kind, size, hash) = &found[inside];
        assert_eq!(
            (kind.as_str(), *size, hash),
            ("file", cleartext.len() as u64, &sha256(cleartext))
        );
    }
    for folder in ["/Folder", "/a", "/a/b", "/a/b/c"] {
        assert_eq!(found[folder].0, "dir");
    }
    assert_eq!(found.len(), written.len() + 4);

    // Writing a file again replaces it.
    let replaced = format!("{root}/file-0.bin");
    write(&vault, &replaced, b"no longer empty").await.unwrap();
    assert_eq!(read(&vault, &replaced).await.unwrap(), b"no longer empty");
    write(&vault, &replaced, b"").await.unwrap();
    assert!(read(&vault, &replaced).await.unwrap().is_empty());

    // Nothing on the server gives a name or a size away.
    let mut stored = vec![folder.path().join("storage/My vault")];
    let mut stored_names = Vec::new();
    while let Some(folder) = stored.pop() {
        for child in std::fs::read_dir(folder).unwrap() {
            let child = child.unwrap();
            stored_names.push(child.file_name().to_string_lossy().into_owned());
            if child.file_type().unwrap().is_dir() {
                stored.push(child.path());
            }
        }
    }
    assert!(stored_names.contains(&"vault.cryptomator".to_string()));
    assert!(stored_names.contains(&"masterkey.cryptomator".to_string()));
    assert!(stored_names.contains(&"dirid.c9r".to_string()));
    assert!(stored_names.iter().any(|name| name.ends_with(".c9s")));
    for name in &stored_names {
        assert!(!name.contains("file-") && !name.contains("Folder") && !name.contains("long"));
        assert!(
            name.len() <= 220 + 4,
            "{name} is longer than a vault stores"
        );
    }
}

#[tokio::test]
async fn folders_are_made_once_and_only_where_they_can_be() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "pw").await;
    let root = vault.root().to_string();

    assert_eq!(
        vault.make_dir(&root, "Photos").await.unwrap(),
        format!("{root}/Photos")
    );
    let again = vault.make_dir(&root, "Photos").await.err().unwrap();
    assert_eq!(again.kind, ErrorKind::AlreadyExists);
    vault.ensure_dir(&format!("{root}/Photos")).await.unwrap();

    write(&vault, &format!("{root}/notes.txt"), b"notes")
        .await
        .unwrap();
    let over_a_file = vault.make_dir(&root, "notes.txt").await.err().unwrap();
    assert_eq!(over_a_file.kind, ErrorKind::AlreadyExists);
    let through_a_file = vault
        .ensure_dir(&format!("{root}/notes.txt/below"))
        .await
        .err()
        .unwrap();
    assert_eq!(through_a_file.kind, ErrorKind::InvalidInput);
    let in_nothing = vault
        .make_dir(&format!("{root}/absent"), "below")
        .await
        .err()
        .unwrap();
    assert_eq!(in_nothing.kind, ErrorKind::NotFound);

    // A file does not go where a folder is, nor into a folder that is not there.
    let over_a_folder = write(&vault, &format!("{root}/Photos"), b"x")
        .await
        .err()
        .unwrap();
    assert_eq!(over_a_folder.kind, ErrorKind::InvalidInput);
    let into_nothing = write(&vault, &format!("{root}/absent/file.txt"), b"x")
        .await
        .err()
        .unwrap();
    assert_eq!(into_nothing.kind, ErrorKind::NotFound);
    let a_folder = read(&vault, &format!("{root}/Photos")).await.err().unwrap();
    assert_ne!(a_folder.kind, ErrorKind::Integrity);
    let absent = read(&vault, &format!("{root}/absent.txt"))
        .await
        .err()
        .unwrap();
    assert_eq!(absent.kind, ErrorKind::NotFound);
    assert_eq!(absent.path.as_deref(), Some(&*format!("{root}/absent.txt")));

    // A long folder name, with a file in it.
    let long_folder = long_name("album");
    vault.make_dir(&root, &long_folder).await.unwrap();
    let inside = format!("{root}/{long_folder}/inside.txt");
    write(&vault, &inside, b"inside").await.unwrap();
    assert_eq!(read(&vault, &inside).await.unwrap(), b"inside");
    let listed: Vec<(String, EntryKind)> = vault
        .list_dir(&root)
        .await
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| (entry.name, entry.kind))
        .collect();
    assert!(listed.contains(&(long_folder.clone(), EntryKind::Dir)));
    assert!(listed.contains(&("Photos".to_string(), EntryKind::Dir)));
    assert!(listed.contains(&("notes.txt".to_string(), EntryKind::File)));
    assert_eq!(listed.len(), 3);
}

#[tokio::test]
async fn workers_that_need_the_same_new_folder_make_it_once() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "pw").await;
    let root = vault.root().to_string();

    let mut workers = Vec::new();
    for worker in 0..8 {
        // Each worker reaches the vault through a connection of its own.
        let files = vault.through(DiskServer::serving(folder.path()));
        let root = root.clone();
        workers.push(tokio::spawn(async move {
            let folder = format!("{root}/upload/deep/er");
            files.ensure_dir(&folder).await.unwrap();
            let path = format!("{folder}/from-{worker}.txt");
            write(&files, &path, format!("worker {worker}").as_bytes())
                .await
                .unwrap();
        }));
    }
    for worker in workers {
        worker.await.unwrap();
    }

    let reopened = VaultFs::unlock(server, &root, "pw".to_string())
        .await
        .unwrap();
    let listing = reopened
        .list_dir(&format!("{root}/upload/deep/er"))
        .await
        .unwrap();
    assert_eq!(listing.entries.len(), 8);
    for worker in 0..8 {
        let path = format!("{root}/upload/deep/er/from-{worker}.txt");
        assert_eq!(
            read(&reopened, &path).await.unwrap(),
            format!("worker {worker}").as_bytes()
        );
    }
}

#[tokio::test]
async fn renames_keep_contents_whatever_the_length_of_the_names() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "pw").await;
    let root = vault.root().to_string();
    let cleartext = contents(70_000, 9);
    let short = "short.bin".to_string();
    let other_short = "other.bin".to_string();
    let long = long_name("first");
    let other_long = long_name("second");

    let mut name = short.clone();
    write(&vault, &format!("{root}/{name}"), &cleartext)
        .await
        .unwrap();
    for new_name in [&other_short, &long, &other_long, &short] {
        let renamed = vault
            .rename(&format!("{root}/{name}"), new_name)
            .await
            .unwrap();
        assert_eq!(renamed, format!("{root}/{new_name}"));
        assert_eq!(vault.stat(&format!("{root}/{name}")).await.unwrap(), None);
        assert_eq!(
            read(&vault, &renamed).await.unwrap(),
            cleartext,
            "{new_name}"
        );
        let names: Vec<String> = vault
            .list_dir(&root)
            .await
            .unwrap()
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec![new_name.clone()]);
        name = new_name.clone();
    }

    // A folder keeps what is in it under every name.
    vault
        .ensure_dir(&format!("{root}/Folder/below"))
        .await
        .unwrap();
    let kept = format!("below/{}", long_name("kept"));
    write(&vault, &format!("{root}/Folder/{kept}"), &cleartext)
        .await
        .unwrap();
    let mut name = "Folder".to_string();
    for new_name in [
        "Renamed",
        &long_name("folder"),
        &long_name("again"),
        "Folder",
    ] {
        // Listing first means the folder is remembered under the name it is about to lose.
        vault
            .list_dir(&format!("{root}/{name}/below"))
            .await
            .unwrap();
        vault
            .rename(&format!("{root}/{name}"), new_name)
            .await
            .unwrap();
        let gone = vault
            .list_dir(&format!("{root}/{name}"))
            .await
            .err()
            .unwrap();
        assert_eq!(gone.kind, ErrorKind::NotFound);
        let moved = format!("{root}/{new_name}/{kept}");
        assert_eq!(read(&vault, &moved).await.unwrap(), cleartext, "{new_name}");
        name = new_name.to_string();
    }

    // A name that is taken stays taken.
    write(&vault, &format!("{root}/taken.txt"), b"taken")
        .await
        .unwrap();
    for path in [format!("{root}/{short}"), format!("{root}/Folder")] {
        let refused = vault.rename(&path, "taken.txt").await.err().unwrap();
        assert_eq!(refused.kind, ErrorKind::AlreadyExists);
    }
    let onto_a_folder = vault
        .rename(&format!("{root}/taken.txt"), "Folder")
        .await
        .err()
        .unwrap();
    assert_eq!(onto_a_folder.kind, ErrorKind::AlreadyExists);
    assert_eq!(
        read(&vault, &format!("{root}/taken.txt")).await.unwrap(),
        b"taken"
    );
    let absent = vault
        .rename(&format!("{root}/absent"), "present")
        .await
        .err()
        .unwrap();
    assert_eq!(absent.kind, ErrorKind::NotFound);
    assert!(vault.rename(&root, "Other vault").await.is_err());
}

/// The number of files and folders the server holds below `folder`.
fn stored_below(folder: &Path) -> usize {
    let mut count = 0;
    for child in std::fs::read_dir(folder).unwrap() {
        let child = child.unwrap();
        count += 1;
        if child.file_type().unwrap().is_dir() {
            count += stored_below(&child.path());
        }
    }
    count
}

#[tokio::test]
async fn deleting_a_folder_leaves_nothing_of_it_on_the_server() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "pw").await;
    let root = vault.root().to_string();
    let on_disk = folder.path().join("storage/My vault/d");
    write(&vault, &format!("{root}/kept.txt"), b"kept")
        .await
        .unwrap();
    let files_before = stored_below(&on_disk);

    let long_folder = long_name("deep");
    for folder in [
        format!("{root}/tree/a/b"),
        format!("{root}/tree/c"),
        format!("{root}/tree/{long_folder}/d"),
    ] {
        vault.ensure_dir(&folder).await.unwrap();
        write(&vault, &format!("{folder}/file.txt"), b"contents")
            .await
            .unwrap();
        write(
            &vault,
            &format!("{folder}/{}", long_name("file")),
            b"contents",
        )
        .await
        .unwrap();
    }
    write(&vault, &format!("{root}/single.txt"), b"single")
        .await
        .unwrap();
    write(
        &vault,
        &format!("{root}/{}", long_name("single")),
        b"single",
    )
    .await
    .unwrap();

    let refused = vault
        .delete(std::slice::from_ref(&root))
        .await
        .err()
        .unwrap();
    assert_eq!(refused.kind, ErrorKind::InvalidInput);
    let outside = vault.delete(&["/storage".to_string()]).await.err().unwrap();
    assert_eq!(outside.kind, ErrorKind::InvalidInput);
    assert!(folder
        .path()
        .join("storage/My vault/vault.cryptomator")
        .exists());

    vault
        .delete(&[
            format!("{root}/tree"),
            // Inside a folder that is going anyway.
            format!("{root}/tree/a/b/file.txt"),
            format!("{root}/single.txt"),
            format!("{root}/{}", long_name("single")),
        ])
        .await
        .unwrap();
    let names: Vec<String> = vault
        .list_dir(&root)
        .await
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, vec!["kept.txt".to_string()]);
    assert_eq!(
        read(&vault, &format!("{root}/kept.txt")).await.unwrap(),
        b"kept"
    );

    // Only the emptied two-letter folders that group the storage may remain.
    let mut leftovers = Vec::new();
    for group in std::fs::read_dir(&on_disk).unwrap() {
        let group = group.unwrap().path();
        leftovers.extend(
            std::fs::read_dir(&group)
                .unwrap()
                .map(|child| child.unwrap().path()),
        );
    }
    assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    assert_eq!(stored_below(&leftovers[0]), files_before - 2);

    let absent = vault.delete(&[format!("{root}/tree")]).await.err().unwrap();
    assert_eq!(absent.kind, ErrorKind::NotFound);
    // A folder of the same name starts empty.
    vault.ensure_dir(&format!("{root}/tree/a")).await.unwrap();
    assert!(vault
        .list_dir(&format!("{root}/tree/a"))
        .await
        .unwrap()
        .entries
        .is_empty());
}

#[tokio::test]
async fn a_locked_vault_gives_nothing_away() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "pw").await;
    let root = vault.root().to_string();
    let path = format!("{root}/secret.txt");
    write(&vault, &path, &contents(100_000, 1)).await.unwrap();
    let worker = vault.through(DiskServer::serving(folder.path()));
    let mut under_way = vault.clone().open_read(&path, 0..u64::MAX).await.unwrap();

    assert!(!vault.is_locked());
    vault.lock();
    assert!(vault.is_locked() && worker.is_locked());

    for files in [&vault, &worker] {
        let locked = [
            files.list_dir(&root).await.err(),
            files.stat(&path).await.err(),
            files.make_dir(&root, "new").await.err(),
            files.ensure_dir(&format!("{root}/new")).await.err(),
            files.rename(&path, "renamed.txt").await.err(),
            files.delete(std::slice::from_ref(&path)).await.err(),
            read(files, &path).await.err(),
            write(files, &path, b"x").await.err(),
        ];
        for error in locked {
            assert_eq!(error.unwrap().kind, ErrorKind::SessionNotFound);
        }
    }
    // A read that was under way when the vault was locked runs to its end.
    let mut read_after = 0;
    while let Some(chunk) = under_way.next_chunk().await.unwrap() {
        read_after += chunk.len();
    }
    assert_eq!(read_after, 100_000);

    let reopened = VaultFs::unlock(server, &root, "pw".to_string())
        .await
        .unwrap();
    assert_eq!(read(&reopened, &path).await.unwrap(), contents(100_000, 1));
}

#[tokio::test]
async fn nothing_outside_the_vault_is_reached_through_it() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "pw").await;
    let root = vault.root().to_string();
    std::fs::write(folder.path().join("storage/beside.txt"), b"beside").unwrap();

    for outside in [
        "/",
        "/storage",
        "/storage/beside.txt",
        "/storage/My vault 2",
        "/storage/My vault/../beside.txt",
        "/storage/My vault/a/../../beside.txt",
    ] {
        assert_eq!(
            vault.list_dir(outside).await.err().unwrap().kind,
            ErrorKind::InvalidInput,
            "{outside}"
        );
        assert_eq!(
            vault.stat(outside).await.err().unwrap().kind,
            ErrorKind::InvalidInput,
            "{outside}"
        );
        assert_eq!(
            read(&vault, outside).await.err().unwrap().kind,
            ErrorKind::InvalidInput,
            "{outside}"
        );
        assert_eq!(
            write(&vault, outside, b"x").await.err().unwrap().kind,
            ErrorKind::InvalidInput,
            "{outside}"
        );
    }
    assert_eq!(
        std::fs::read(folder.path().join("storage/beside.txt")).unwrap(),
        b"beside"
    );
    // Paths are also taken relative to the vault's folder.
    write(&vault, "relative.txt", b"relative").await.unwrap();
    assert_eq!(
        read(&vault, &format!("{root}/./relative.txt"))
            .await
            .unwrap(),
        b"relative"
    );
    // The vault's own files do not show inside it.
    let names: Vec<String> = vault
        .list_dir(&root)
        .await
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, vec!["relative.txt".to_string()]);
}

#[tokio::test]
async fn names_are_stored_composed_as_cryptomator_stores_them() {
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let vault = new_vault(&server, "pw").await;
    let root = vault.root().to_string();
    let composed = "caf\u{e9}.txt";
    let decomposed = "cafe\u{301}.txt";

    write(&vault, &format!("{root}/{decomposed}"), b"coffee")
        .await
        .unwrap();
    let names: Vec<String> = vault
        .list_dir(&root)
        .await
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, vec![composed.to_string()]);
    for name in [composed, decomposed] {
        assert_eq!(
            read(&vault, &format!("{root}/{name}")).await.unwrap(),
            b"coffee"
        );
    }
}

/// The harness on Cryptomator's own library, when the test run names one.
struct Cryptomator {
    jar: PathBuf,
}

impl Cryptomator {
    fn configured() -> Option<Self> {
        let jar = PathBuf::from(std::env::var_os("POROS_TEST_CRYPTOMATOR_JAR")?);
        assert!(jar.is_file(), "{} is not a file", jar.display());
        Some(Self { jar })
    }

    fn run(&self, arguments: &[&str]) -> Vec<u8> {
        let java = std::env::var_os("POROS_TEST_JAVA").unwrap_or_else(|| "java".into());
        let output = std::process::Command::new(java)
            .arg("-jar")
            .arg(&self.jar)
            .args(arguments)
            // Java reads its arguments, a password among them, in the locale's character set.
            .env("LC_ALL", "C.UTF-8")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "the harness failed on {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    /// What Cryptomator reads out of the vault in `folder`.
    fn dump(&self, folder: &Path, password: &str) -> BTreeMap<String, (String, u64, String)> {
        let manifest = self.run(&["dump", folder.to_str().unwrap(), password]);
        expected(&serde_json::from_slice::<Vec<Listed>>(&manifest).unwrap())
    }
}

#[tokio::test]
async fn cryptomator_reads_what_poros_wrote() {
    let Some(cryptomator) = Cryptomator::configured() else {
        eprintln!("skipped: POROS_TEST_CRYPTOMATOR_JAR is not set");
        return;
    };
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let password = "p\u{e4}ss w\u{f6}rd \u{5bc6}\u{7801}";
    let vault = new_vault(&server, password).await;
    let root = vault.root().to_string();
    let on_disk = folder.path().join("storage/My vault");

    // An empty vault is one Cryptomator opens.
    assert!(cryptomator.dump(&on_disk, password).is_empty());

    let mut written: BTreeMap<String, (String, u64, String)> = BTreeMap::new();
    let mut folders = vec![
        "/Documents".to_string(),
        "/Documents/Reports/2024".to_string(),
        "/empty".to_string(),
        "/\u{e9}t\u{e9} \u{1f600}".to_string(),
        format!("/{}", long_name("folder")),
        format!("/{}/below", long_name("folder")),
    ];
    for folder in &folders {
        vault.ensure_dir(&format!("{root}{folder}")).await.unwrap();
    }
    folders.push("/Documents/Reports".to_string());
    for folder in folders {
        written.insert(folder, ("dir".to_string(), 0, String::new()));
    }
    let files = [
        ("/empty.txt".to_string(), 0),
        ("/one.bin".to_string(), 1),
        ("/chunk-minus-one.bin".to_string(), 32_767),
        ("/chunk.bin".to_string(), 32_768),
        ("/chunk-plus-one.bin".to_string(), 32_769),
        ("/Documents/large.bin".to_string(), 1_000_003),
        (
            "/Documents/Reports/2024/q1 & q2 (final).txt".to_string(),
            300,
        ),
        (
            "/\u{e9}t\u{e9} \u{1f600}/\u{65e5}\u{672c}\u{8a9e}.txt".to_string(),
            77,
        ),
        (format!("/{}", long_name("file")), 40_000),
        (
            format!("/{}/below/{}", long_name("folder"), long_name("deep")),
            5,
        ),
        ("/.hidden".to_string(), 12),
    ];
    for (number, (path, size)) in files.into_iter().enumerate() {
        let cleartext = contents(size, number as u8);
        let piece = [100_000, 4096, 33_000][number % 3];
        write_in_pieces(&vault, &format!("{root}{path}"), &cleartext, piece)
            .await
            .unwrap();
        written.insert(path, ("file".to_string(), size as u64, sha256(&cleartext)));
    }
    assert_eq!(cryptomator.dump(&on_disk, password), written);

    // Renamed and deleted here, the vault still reads the same in Cryptomator.
    let renamed = long_name("renamed");
    vault
        .rename(&format!("{root}/chunk.bin"), &renamed)
        .await
        .unwrap();
    let moved = written.remove("/chunk.bin").unwrap();
    written.insert(format!("/{renamed}"), moved);
    vault
        .rename(&format!("{root}/{}", long_name("file")), "short again.bin")
        .await
        .unwrap();
    let moved = written.remove(&format!("/{}", long_name("file"))).unwrap();
    written.insert("/short again.bin".to_string(), moved);
    vault
        .rename(&format!("{root}/Documents"), "Papers")
        .await
        .unwrap();
    for path in written.keys().cloned().collect::<Vec<_>>() {
        if let Some(below) = path.strip_prefix("/Documents") {
            let moved = written.remove(&path).unwrap();
            written.insert(format!("/Papers{below}"), moved);
        }
    }
    vault
        .delete(&[
            format!("{root}/{}", long_name("folder")),
            format!("{root}/one.bin"),
        ])
        .await
        .unwrap();
    written.retain(|path, _| path != "/one.bin" && !path.starts_with("/folder-"));
    assert_eq!(cryptomator.dump(&on_disk, password), written);
    assert_eq!(walk(&vault, &root).await, written);
}

#[tokio::test]
async fn files_added_to_a_cryptomator_vault_sit_beside_its_own() {
    for name in ["gcm", "ctrmac"] {
        let copy = fixture_copy(name);
        let server = DiskServer::serving(copy.path());
        let vault = VaultFs::unlock(server, "/vault", FIXTURE_PASSWORD.to_string())
            .await
            .unwrap();
        let mut listed = expected(&manifest(&fixture(name)));

        let long = long_name("added");
        vault
            .ensure_dir("/vault/Documents/Added/deeper")
            .await
            .unwrap();
        listed.insert(
            "/Documents/Added".to_string(),
            ("dir".to_string(), 0, String::new()),
        );
        listed.insert(
            "/Documents/Added/deeper".to_string(),
            ("dir".to_string(), 0, String::new()),
        );
        let added = [
            ("/added-empty.bin".to_string(), 0),
            ("/Documents/Added/chunk.bin".to_string(), 32_768),
            ("/Documents/Added/deeper/large.bin".to_string(), 200_001),
            (format!("/many/{long}"), 33_333),
        ];
        for (number, (path, size)) in added.into_iter().enumerate() {
            let cleartext = contents(size, number as u8);
            write_in_pieces(&vault, &format!("/vault{path}"), &cleartext, 50_000)
                .await
                .unwrap();
            listed.insert(path, ("file".to_string(), size as u64, sha256(&cleartext)));
        }
        // One of Cryptomator's own files replaced, one renamed, one folder removed.
        let replacement = contents(70_000, 77);
        write(&vault, "/vault/hello.txt", &replacement)
            .await
            .unwrap();
        listed.insert(
            "/hello.txt".to_string(),
            ("file".to_string(), 70_000, sha256(&replacement)),
        );
        vault.rename("/vault/large.bin", "big.bin").await.unwrap();
        let renamed = listed.remove("/large.bin").unwrap();
        listed.insert("/big.bin".to_string(), renamed);
        vault.delete(&["/vault/a b".to_string()]).await.unwrap();
        listed.retain(|path, _| !path.starts_with("/a b"));

        assert_eq!(walk(&vault, "/vault").await, listed, "{name}");
        if let Some(cryptomator) = Cryptomator::configured() {
            let read = cryptomator.dump(&copy.path().join("vault"), FIXTURE_PASSWORD);
            assert_eq!(read, listed, "{name} as Cryptomator reads it");
        }
    }
}

#[tokio::test]
async fn poros_reads_what_cryptomator_changes_in_its_vault() {
    let Some(cryptomator) = Cryptomator::configured() else {
        eprintln!("skipped: POROS_TEST_CRYPTOMATOR_JAR is not set");
        return;
    };
    let folder = tempfile::tempdir().unwrap();
    let server = DiskServer::serving(folder.path());
    let password = "shared between the two";
    let vault = new_vault(&server, password).await;
    let root = vault.root().to_string();
    let on_disk = folder.path().join("storage/My vault");
    let vault_folder = on_disk.to_str().unwrap();

    vault
        .ensure_dir(&format!("{root}/from poros/inner"))
        .await
        .unwrap();
    let cleartext = contents(123_456, 3);
    write(
        &vault,
        &format!("{root}/from poros/inner/data.bin"),
        &cleartext,
    )
    .await
    .unwrap();
    write(&vault, &format!("{root}/to delete.txt"), b"delete me")
        .await
        .unwrap();
    vault
        .ensure_dir(&format!("{root}/to delete/below"))
        .await
        .unwrap();
    write(
        &vault,
        &format!("{root}/to delete/below/file.txt"),
        b"delete me",
    )
    .await
    .unwrap();

    // Cryptomator adds to, moves within and deletes from the vault Poros made.
    let source = folder.path().join("source.bin");
    let added = contents(200_000, 4);
    std::fs::write(&source, &added).unwrap();
    let long = long_name("from cryptomator");
    cryptomator.run(&["mkdir", vault_folder, password, "/from cryptomator/deep"]);
    cryptomator.run(&[
        "put",
        vault_folder,
        password,
        "/from cryptomator/deep/added.bin",
        source.to_str().unwrap(),
    ]);
    cryptomator.run(&[
        "put",
        vault_folder,
        password,
        &format!("/from poros/{long}"),
        source.to_str().unwrap(),
    ]);
    cryptomator.run(&[
        "move",
        vault_folder,
        password,
        "/from poros/inner",
        "/from cryptomator/moved",
    ]);
    cryptomator.run(&["delete", vault_folder, password, "/to delete.txt"]);
    cryptomator.run(&["delete", vault_folder, password, "/to delete"]);
    cryptomator.run(&["symlink", vault_folder, password, "/link", "from poros"]);

    // The folder ids read before Cryptomator moved things are stale now, which a fresh look
    // at the vault, as any later session takes, does not have.
    let reopened = VaultFs::unlock(server.clone(), &root, password.to_string())
        .await
        .unwrap();
    let found = walk(&reopened, &root).await;
    assert_eq!(found, cryptomator.dump(&on_disk, password));
    assert_eq!(
        read(
            &reopened,
            &format!("{root}/from cryptomator/moved/data.bin")
        )
        .await
        .unwrap(),
        cleartext
    );
    assert_eq!(
        read(&reopened, &format!("{root}/from poros/{long}"))
            .await
            .unwrap(),
        added
    );
    assert_eq!(found["/link"].0, "symlink");
    assert!(!found.contains_key("/to delete.txt") && !found.contains_key("/to delete"));

    // The session that was open all along finds its way too: a folder it remembers, and that
    // is gone, is looked up again instead of being read from where it was.
    let stale = vault
        .list_dir(&format!("{root}/to delete/below"))
        .await
        .err()
        .unwrap();
    assert_eq!(stale.kind, ErrorKind::NotFound);
    assert_eq!(
        vault.stat(&format!("{root}/to delete")).await.unwrap(),
        None
    );
    assert_eq!(
        read(&vault, &format!("{root}/from cryptomator/deep/added.bin"))
            .await
            .unwrap(),
        added
    );

    // And Poros changes what Cryptomator wrote.
    reopened
        .rename(
            &format!("{root}/from cryptomator/deep/added.bin"),
            "renamed.bin",
        )
        .await
        .unwrap();
    reopened
        .delete(&[
            format!("{root}/from cryptomator/moved"),
            format!("{root}/link"),
        ])
        .await
        .unwrap();
    assert_eq!(
        walk(&reopened, &root).await,
        cryptomator.dump(&on_disk, password)
    );
}
