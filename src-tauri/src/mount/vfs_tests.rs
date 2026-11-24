//! The mounted drive's file system against a server held in memory.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::backend::{Limits, SharedBackend};
use super::blocks::BLOCK_SIZE;
use super::memory::MemoryFs;
use super::names::NameStyle;
use super::spool;
use super::vfs::{AttributeChanges, FsOptions, MountFs, NodeKind, OpenMode, VfsError, ROOT_NODE};
use crate::protocol::RemoteFileSystem;

struct Drive {
    server: Arc<MemoryFs>,
    fs: Arc<MountFs>,
    /// Holds the spool folder for as long as the drive is used.
    folder: tempfile::TempDir,
}

impl Drive {
    fn spool_folder(&self) -> std::path::PathBuf {
        self.folder.path().join("spool")
    }

    fn spooled_files(&self) -> usize {
        std::fs::read_dir(self.spool_folder())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().extension().is_some_and(|kind| kind == "spool"))
                    .count()
            })
            .unwrap_or(0)
    }
}

fn drive_with(server: Arc<MemoryFs>, adjust: impl FnOnce(&mut FsOptions)) -> Drive {
    drive_showing(server, "/", adjust)
}

/// A drive showing the folder `root` of `server`.
fn drive_showing(server: Arc<MemoryFs>, root: &str, adjust: impl FnOnce(&mut FsOptions)) -> Drive {
    let folder = tempfile::tempdir().unwrap();
    let mut options = FsOptions::new("memory", folder.path().join("spool"));
    options.style = NameStyle::Unix;
    adjust(&mut options);
    let backend = Arc::new(SharedBackend::new(server.clone(), Limits::SHARED));
    Drive {
        fs: MountFs::new(backend, root, options),
        server,
        folder,
    }
}

/// The names the server holds in `folder`, in order.
async fn server_names(server: &MemoryFs, folder: &str) -> Vec<String> {
    let mut names: Vec<String> = server
        .list_dir(folder)
        .await
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    names
}

fn drive() -> Drive {
    drive_with(MemoryFs::new(), |_| {})
}

fn pattern(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

async fn node(drive: &Drive, path: &str) -> u64 {
    let mut current = ROOT_NODE;
    for part in path.split('/').filter(|part| !part.is_empty()) {
        current = drive.fs.lookup(current, part).await.unwrap().node;
    }
    current
}

async fn read_whole(drive: &Drive, path: &str, step: u32) -> Vec<u8> {
    let id = node(drive, path).await;
    let handle = drive.fs.open(id, OpenMode::READ).await.unwrap();
    let mut contents = Vec::new();
    loop {
        let data = drive
            .fs
            .read(handle, contents.len() as u64, step)
            .await
            .unwrap();
        if data.is_empty() {
            break;
        }
        contents.extend_from_slice(&data);
    }
    drive.fs.release(handle);
    contents
}

/// Creates or replaces a file the way a program saving it does.
async fn save(drive: &Drive, folder: u64, name: &str, contents: &[u8]) {
    let (_, handle) = drive
        .fs
        .create(folder, name, OpenMode::WRITE_TRUNCATE, false, Some(0o644))
        .await
        .unwrap();
    drive.fs.write(handle, 0, contents).await.unwrap();
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
}

async fn until(what: &str, mut holds: impl FnMut() -> bool) {
    for _ in 0..400 {
        if holds() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting until {what}");
}

#[tokio::test]
async fn one_listing_answers_every_lookup_in_its_folder() {
    let drive = drive();
    drive.server.put_file("/docs/a.txt", "alpha");
    drive.server.put_file("/docs/b.txt", "beta!");
    drive.server.put_folder("/docs/sub");

    let docs = node(&drive, "/docs").await;
    let names: Vec<(String, NodeKind, u64)> = drive
        .fs
        .read_folder(docs)
        .await
        .unwrap()
        .into_iter()
        .map(|(name, attributes)| (name, attributes.kind, attributes.size))
        .collect();
    assert_eq!(
        names,
        vec![
            ("a.txt".to_string(), NodeKind::File, 5),
            ("b.txt".to_string(), NodeKind::File, 5),
            ("sub".to_string(), NodeKind::Folder, 0),
        ]
    );
    let listed = drive.server.counters.listings.load(Ordering::Relaxed);
    let file = drive.fs.lookup(docs, "a.txt").await.unwrap();
    assert_eq!((file.kind, file.size), (NodeKind::File, 5));
    assert_eq!(
        drive.fs.lookup(docs, "missing").await,
        Err(VfsError::NotFound)
    );
    assert_eq!(drive.fs.attributes(file.node).await.unwrap().size, 5);
    assert_eq!(
        drive.server.counters.listings.load(Ordering::Relaxed),
        listed,
        "lookups and attributes come from the cached listing"
    );
    // The entry a folder listing names is the node a lookup returns.
    let listed_node = drive.fs.read_folder(docs).await.unwrap()[0].1.node;
    assert_eq!(listed_node, file.node);
}

#[tokio::test]
async fn reads_at_offsets_and_across_blocks() {
    let drive = drive();
    let contents = pattern(3 * BLOCK_SIZE as usize + 4321, 1);
    drive.server.put_file("/big.bin", contents.clone());
    let id = node(&drive, "/big.bin").await;
    let handle = drive.fs.open(id, OpenMode::READ).await.unwrap();

    let across = BLOCK_SIZE - 100;
    let data = drive.fs.read(handle, across, 5000).await.unwrap();
    assert_eq!(data[..], contents[across as usize..across as usize + 5000]);

    let tail = contents.len() as u64 - 10;
    let data = drive.fs.read(handle, tail, 4096).await.unwrap();
    assert_eq!(data[..], contents[tail as usize..]);

    assert!(drive
        .fs
        .read(handle, contents.len() as u64, 10)
        .await
        .unwrap()
        .is_empty());
    assert!(drive
        .fs
        .read(handle, contents.len() as u64 + 99_999, 10)
        .await
        .unwrap()
        .is_empty());

    let spanning = drive
        .fs
        .read(handle, 10, 2 * BLOCK_SIZE as u32 + 50)
        .await
        .unwrap();
    assert_eq!(
        spanning[..],
        contents[10..10 + 2 * BLOCK_SIZE as usize + 50]
    );
    drive.fs.release(handle);
}

#[tokio::test]
async fn a_file_read_in_order_is_streamed_over_one_request() {
    let drive = drive();
    let contents = pattern(6 * BLOCK_SIZE as usize + 17, 2);
    drive.server.put_file("/video.mkv", contents.clone());
    assert_eq!(read_whole(&drive, "/video.mkv", 128 * 1024).await, contents);
    assert_eq!(
        drive.server.counters.read_requests.load(Ordering::Relaxed),
        1,
        "one stream carries the whole file"
    );
    // Read again, the file comes from the cached blocks.
    assert_eq!(
        read_whole(&drive, "/video.mkv", 1024 * 1024).await,
        contents
    );
    assert_eq!(
        drive.server.counters.read_requests.load(Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn random_reads_fetch_only_the_blocks_they_touch() {
    let drive = drive_with(MemoryFs::new(), |options| options.read_ahead_blocks = 2);
    let contents = pattern(8 * BLOCK_SIZE as usize, 3);
    drive.server.put_file("/disk.img", contents.clone());
    let id = node(&drive, "/disk.img").await;
    let handle = drive.fs.open(id, OpenMode::READ).await.unwrap();

    let offset = 5 * BLOCK_SIZE + 10;
    let data = drive.fs.read(handle, offset, 100).await.unwrap();
    assert_eq!(data[..], contents[offset as usize..offset as usize + 100]);
    assert_eq!(
        drive.server.counters.bytes_read.load(Ordering::Relaxed),
        BLOCK_SIZE
    );
    // The same block again costs nothing.
    drive.fs.read(handle, offset + 5000, 100).await.unwrap();
    assert_eq!(
        drive.server.counters.read_requests.load(Ordering::Relaxed),
        1
    );
    // Back to an earlier block: another single block.
    let data = drive.fs.read(handle, 2 * BLOCK_SIZE, 64).await.unwrap();
    assert_eq!(data[..], contents[2 * BLOCK_SIZE as usize..][..64]);
    assert_eq!(
        drive.server.counters.bytes_read.load(Ordering::Relaxed),
        2 * BLOCK_SIZE
    );
    drive.fs.release(handle);
}

#[tokio::test]
async fn the_block_cache_stays_within_its_budget() {
    let budget = 3 * BLOCK_SIZE as usize;
    let drive = drive_with(MemoryFs::new(), |options| {
        options.memory_budget = budget;
        options.read_ahead_blocks = 1;
    });
    let contents = pattern(10 * BLOCK_SIZE as usize, 4);
    drive.server.put_file("/large.iso", contents.clone());
    assert_eq!(read_whole(&drive, "/large.iso", 256 * 1024).await, contents);
    // What is no longer cached is fetched again and still correct.
    let id = node(&drive, "/large.iso").await;
    let handle = drive.fs.open(id, OpenMode::READ).await.unwrap();
    let data = drive.fs.read(handle, 100, 1000).await.unwrap();
    assert_eq!(data[..], contents[100..1100]);
    drive.fs.release(handle);
}

#[tokio::test]
async fn a_new_file_is_written_read_back_and_uploaded_on_flush() {
    let drive = drive();
    let (created, handle) = drive
        .fs
        .create(
            ROOT_NODE,
            "notes.txt",
            OpenMode::READ_WRITE,
            true,
            Some(0o600),
        )
        .await
        .unwrap();
    assert_eq!(created.size, 0);
    assert!(
        drive.server.exists("/notes.txt"),
        "the name is taken at once"
    );

    // Written out of order, as a program is free to.
    drive.fs.write(handle, 6, b"world").await.unwrap();
    drive.fs.write(handle, 0, b"hello ").await.unwrap();
    assert_eq!(drive.fs.attributes(created.node).await.unwrap().size, 11);
    assert_eq!(
        drive.fs.read(handle, 0, 100).await.unwrap()[..],
        b"hello world"[..]
    );
    assert_eq!(drive.server.file("/notes.txt").unwrap().len(), 0);
    assert_eq!(drive.fs.pending_uploads().len(), 1);

    drive.fs.flush(handle).await.unwrap();
    assert_eq!(
        drive.server.file("/notes.txt").unwrap()[..],
        b"hello world"[..]
    );
    assert_eq!(drive.server.permissions("/notes.txt"), Some(0o600));
    assert!(drive.fs.pending_uploads().is_empty());

    // A second flush with nothing new uploads nothing.
    let uploads = drive.server.counters.uploads.load(Ordering::Relaxed);
    drive.fs.flush(handle).await.unwrap();
    assert_eq!(
        drive.server.counters.uploads.load(Ordering::Relaxed),
        uploads
    );
    drive.fs.release(handle);
    until("the spool is removed", || drive.spooled_files() == 0).await;
    assert_eq!(
        drive.fs.lookup(ROOT_NODE, "notes.txt").await.unwrap().size,
        11
    );
    assert_eq!(
        drive
            .fs
            .create(ROOT_NODE, "notes.txt", OpenMode::READ_WRITE, true, None)
            .await
            .map(|_| ()),
        Err(VfsError::AlreadyExists)
    );
}

#[tokio::test]
async fn a_file_opened_for_update_is_fetched_at_its_first_write() {
    let drive = drive();
    let original = pattern(2 * BLOCK_SIZE as usize + 100, 5);
    drive.server.put_file("/data.bin", original.clone());
    let id = node(&drive, "/data.bin").await;
    let handle = drive.fs.open(id, OpenMode::READ_WRITE).await.unwrap();
    assert_eq!(
        drive.spooled_files(),
        0,
        "nothing is fetched to open a file"
    );

    let middle = BLOCK_SIZE as usize + 7;
    drive
        .fs
        .write(handle, middle as u64, b"PATCH")
        .await
        .unwrap();
    assert_eq!(drive.spooled_files(), 1);
    let mut expected = original.clone();
    expected[middle..middle + 5].copy_from_slice(b"PATCH");
    let read = drive.fs.read(handle, middle as u64 - 3, 11).await.unwrap();
    assert_eq!(read[..], expected[middle - 3..middle + 8]);
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(drive.server.file("/data.bin").unwrap()[..], expected[..]);

    let handle = drive
        .fs
        .open(
            id,
            OpenMode {
                write: true,
                append: true,
                ..OpenMode::default()
            },
        )
        .await
        .unwrap();
    // Appending ignores the offset the kernel passes.
    drive.fs.write(handle, 0, b"-tail").await.unwrap();
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    expected.extend_from_slice(b"-tail");
    assert_eq!(drive.server.file("/data.bin").unwrap()[..], expected[..]);
    assert_eq!(read_whole(&drive, "/data.bin", 300_000).await, expected);
}

#[tokio::test]
async fn truncating_with_and_without_an_open_file() {
    let drive = drive();
    drive.server.put_file("/log.txt", "0123456789abcdef");
    let id = node(&drive, "/log.txt").await;
    let requests = drive.server.counters.read_requests.load(Ordering::Relaxed);

    // Opening with truncation fetches nothing and replaces everything.
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    assert_eq!(drive.fs.attributes(id).await.unwrap().size, 0);
    drive.fs.write(handle, 0, b"new").await.unwrap();
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(drive.server.file("/log.txt").unwrap()[..], b"new"[..]);
    assert_eq!(
        drive.server.counters.read_requests.load(Ordering::Relaxed),
        requests
    );

    // With nothing open, a new size is sent at once.
    drive.server.put_file("/log.txt", "0123456789abcdef");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let drive_fs = &drive.fs;
    drive_fs.forget_cached();
    let shrunk = drive_fs
        .set_attributes(
            id,
            AttributeChanges {
                size: Some(4),
                ..AttributeChanges::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(shrunk.size, 4);
    assert_eq!(drive.server.file("/log.txt").unwrap()[..], b"0123"[..]);
    let grown = drive_fs
        .set_attributes(
            id,
            AttributeChanges {
                size: Some(6),
                ..AttributeChanges::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(grown.size, 6);
    assert_eq!(drive.server.file("/log.txt").unwrap()[..], b"0123\0\0"[..]);
    until("the spool is removed", || drive.spooled_files() == 0).await;

    // Truncating an open file waits for its close.
    let handle = drive.fs.open(id, OpenMode::READ_WRITE).await.unwrap();
    drive_fs
        .set_attributes(
            id,
            AttributeChanges {
                size: Some(2),
                ..AttributeChanges::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(drive.server.file("/log.txt").unwrap().len(), 6);
    assert_eq!(drive.fs.read(handle, 0, 10).await.unwrap()[..], b"01"[..]);
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(drive.server.file("/log.txt").unwrap()[..], b"01"[..]);
}

#[tokio::test]
async fn saving_by_rename_replaces_the_original() {
    for moves_between_folders in [true, false] {
        let drive = drive();
        drive
            .server
            .moves_between_folders
            .store(moves_between_folders, Ordering::Relaxed);
        drive.server.put_file("/docs/report.txt", "first draft");
        let docs = node(&drive, "/docs").await;
        let original = drive.fs.lookup(docs, "report.txt").await.unwrap();

        save(&drive, docs, ".report.txt.swp", b"second draft, longer").await;
        drive
            .fs
            .rename(docs, ".report.txt.swp", docs, "report.txt", true)
            .await
            .unwrap();

        assert_eq!(
            drive.server.file("/docs/report.txt").unwrap()[..],
            b"second draft, longer"[..]
        );
        assert!(!drive.server.exists("/docs/.report.txt.swp"));
        let names: Vec<String> = drive
            .fs
            .read_folder(docs)
            .await
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, vec!["report.txt".to_string()]);
        let renamed = drive.fs.lookup(docs, "report.txt").await.unwrap();
        assert_eq!(renamed.size, 20);
        assert_ne!(
            renamed.node, original.node,
            "the replaced file is another node"
        );
        assert_eq!(
            read_whole(&drive, "/docs/report.txt", 4096).await,
            b"second draft, longer"
        );
        assert_eq!(
            drive
                .fs
                .rename(docs, "report.txt", docs, "report.txt", false)
                .await,
            Ok(())
        );
    }
}

#[tokio::test]
async fn moving_between_folders_or_saying_the_server_cannot() {
    let drive = drive();
    drive.server.put_file("/in/tree/leaf.txt", "leaf");
    drive.server.put_file("/in/file.txt", "file");
    drive.server.put_folder("/out");
    let source = node(&drive, "/in").await;
    let target = node(&drive, "/out").await;
    let leaf = node(&drive, "/in/tree/leaf.txt").await;

    drive
        .fs
        .rename(source, "tree", target, "moved", false)
        .await
        .unwrap();
    assert!(drive.server.exists("/out/moved/leaf.txt"));
    assert!(!drive.server.exists("/in/tree"));
    // The node of a file inside the moved folder follows it.
    assert_eq!(node(&drive, "/out/moved/leaf.txt").await, leaf);
    assert_eq!(
        read_whole(&drive, "/out/moved/leaf.txt", 100).await,
        b"leaf"
    );
    assert_eq!(
        drive.fs.lookup(source, "tree").await,
        Err(VfsError::NotFound)
    );

    drive.server.put_file("/out/taken.txt", "taken");
    drive.fs.forget_cached();
    assert_eq!(
        drive
            .fs
            .rename(source, "file.txt", target, "taken.txt", false)
            .await,
        Err(VfsError::AlreadyExists)
    );
    assert_eq!(
        drive
            .fs
            .rename(source, "file.txt", target, "moved", true)
            .await,
        Err(VfsError::IsAFolder)
    );

    drive
        .server
        .moves_between_folders
        .store(false, Ordering::Relaxed);
    assert_eq!(
        drive
            .fs
            .rename(source, "file.txt", target, "file.txt", false)
            .await,
        Err(VfsError::CrossFolder),
        "programs then copy and delete"
    );
    drive
        .fs
        .rename(source, "file.txt", source, "renamed.txt", false)
        .await
        .unwrap();
    assert!(drive.server.exists("/in/renamed.txt"));
}

#[tokio::test]
async fn folders_are_made_and_removed_only_when_empty() {
    let drive = drive();
    let made = drive.fs.make_folder(ROOT_NODE, "project").await.unwrap();
    assert_eq!(made.kind, NodeKind::Folder);
    assert!(drive.server.exists("/project"));
    assert_eq!(
        drive.fs.make_folder(ROOT_NODE, "project").await,
        Err(VfsError::AlreadyExists)
    );
    save(&drive, made.node, "main.rs", b"fn main() {}").await;

    assert_eq!(
        drive.fs.remove_folder(ROOT_NODE, "project").await,
        Err(VfsError::NotEmpty)
    );
    assert_eq!(
        drive.fs.remove_file(ROOT_NODE, "project").await,
        Err(VfsError::IsAFolder)
    );
    assert_eq!(
        drive.fs.remove_folder(made.node, "main.rs").await,
        Err(VfsError::NotAFolder)
    );
    drive.fs.remove_file(made.node, "main.rs").await.unwrap();
    assert!(!drive.server.exists("/project/main.rs"));
    assert_eq!(
        drive.fs.lookup(made.node, "main.rs").await,
        Err(VfsError::NotFound)
    );
    drive.fs.remove_folder(ROOT_NODE, "project").await.unwrap();
    assert!(!drive.server.exists("/project"));
    assert_eq!(
        drive.fs.attributes(made.node).await,
        Err(VfsError::NotFound)
    );
    assert_eq!(
        drive.fs.remove_file(ROOT_NODE, "never-there").await,
        Err(VfsError::NotFound)
    );
}

#[tokio::test]
async fn handles_on_one_file_share_what_is_written() {
    let drive = drive();
    drive.server.put_file("/shared.db", vec![b'.'; 64]);
    let id = node(&drive, "/shared.db").await;
    let writer = drive.fs.open(id, OpenMode::READ_WRITE).await.unwrap();
    let reader = drive.fs.open(id, OpenMode::READ).await.unwrap();

    assert_eq!(drive.fs.read(reader, 0, 4).await.unwrap()[..], b"...."[..]);
    drive.fs.write(writer, 0, b"HEAD").await.unwrap();
    assert_eq!(
        drive.fs.read(reader, 0, 6).await.unwrap()[..],
        b"HEAD.."[..],
        "a reader sees what another handle wrote, before any upload"
    );

    // Two programs writing different parts at once.
    let mut tasks = Vec::new();
    for part in 0..8u64 {
        let fs = drive.fs.clone();
        tasks.push(tokio::spawn(async move {
            let handle = fs.open(id, OpenMode::READ_WRITE).await.unwrap();
            let byte = b'a' + part as u8;
            fs.write(handle, 8 + part * 4, &[byte; 4]).await.unwrap();
            fs.flush(handle).await.unwrap();
            fs.release(handle);
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    drive.fs.flush(writer).await.unwrap();
    drive.fs.release(writer);
    drive.fs.release(reader);

    let mut expected = [b'.'; 64];
    expected[..4].copy_from_slice(b"HEAD");
    for part in 0..8usize {
        expected[8 + part * 4..12 + part * 4].fill(b'a' + part as u8);
    }
    assert_eq!(drive.server.file("/shared.db").unwrap()[..], expected[..]);
    assert_eq!(drive.fs.open_paths(), Vec::<String>::new());
    until("the spool is removed", || drive.spooled_files() == 0).await;
}

#[tokio::test]
async fn a_failed_upload_reaches_the_program_and_the_data_is_kept() {
    let drive = drive();
    drive.server.put_file("/thesis.tex", "old");
    let id = node(&drive, "/thesis.tex").await;
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    drive.fs.write(handle, 0, b"new chapter").await.unwrap();

    drive.server.failing_uploads.store(1, Ordering::Relaxed);
    let failed = drive.fs.flush(handle).await;
    assert_eq!(
        failed,
        Err(VfsError::Io("The server refused the upload".into())),
        "close() reports the failure"
    );
    assert_eq!(drive.server.file("/thesis.tex").unwrap()[..], b"old"[..]);
    let pending = drive.fs.pending_uploads();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].path, "/thesis.tex");
    assert_eq!(
        pending[0].error.as_deref(),
        Some("The server refused the upload")
    );
    // Kept on disk with a note at once, so even a crash now would not lose it.
    let unsent = spool::unsent_files(&drive.spool_folder());
    assert_eq!(unsent.len(), 1);
    assert_eq!(unsent[0].note.remote_path, "/thesis.tex");
    assert_eq!(std::fs::read(&unsent[0].path).unwrap(), b"new chapter");

    drive.fs.release(handle);
    // The drive still shows what was written, not what the server holds.
    assert_eq!(drive.fs.attributes(id).await.unwrap().size, 11);
    assert_eq!(read_whole(&drive, "/thesis.tex", 100).await, b"new chapter");

    until("the upload is retried after the close", || {
        drive.server.file("/thesis.tex").unwrap()[..] == b"new chapter"[..]
    })
    .await;
    until("nothing is pending", || {
        drive.fs.pending_uploads().is_empty()
    })
    .await;
    assert!(spool::unsent_files(&drive.spool_folder()).is_empty());
    until("the spool is removed", || drive.spooled_files() == 0).await;
}

#[tokio::test]
async fn an_upload_failing_after_the_close_is_reported_kept_and_retried() {
    let drive = drive();
    let reported: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let sink = reported.clone();
    drive
        .fs
        .report_upload_failures(Arc::new(move |path, reason| {
            sink.lock()
                .unwrap()
                .push((path.to_string(), reason.to_string()));
        }));
    let (_, handle) = drive
        .fs
        .create(
            ROOT_NODE,
            "photo.raw",
            OpenMode::WRITE_TRUNCATE,
            false,
            None,
        )
        .await
        .unwrap();
    drive.fs.write(handle, 0, &pattern(5000, 6)).await.unwrap();
    drive.server.offline.store(true, Ordering::Relaxed);
    // No flush: the program was killed, so only the release arrives.
    drive.fs.release(handle);
    until("the failure is reported", || {
        !reported.lock().unwrap().is_empty()
    })
    .await;
    assert_eq!(reported.lock().unwrap()[0].0, "/photo.raw");
    assert_eq!(drive.fs.pending_uploads().len(), 1);
    assert_eq!(spool::unsent_files(&drive.spool_folder()).len(), 1);

    drive.server.offline.store(false, Ordering::Relaxed);
    drive.fs.retry_unsent();
    until("the retry got through", || {
        drive
            .server
            .file("/photo.raw")
            .is_some_and(|data| data[..] == pattern(5000, 6)[..])
    })
    .await;
    until("nothing is pending", || {
        drive.fs.pending_uploads().is_empty()
    })
    .await;
    assert!(spool::unsent_files(&drive.spool_folder()).is_empty());
}

#[tokio::test]
async fn an_upload_the_server_cut_short_is_an_error() {
    let drive = drive();
    drive.server.short_uploads.store(true, Ordering::Relaxed);
    let (_, handle) = drive
        .fs
        .create(ROOT_NODE, "cut.bin", OpenMode::WRITE_TRUNCATE, false, None)
        .await
        .unwrap();
    drive.fs.write(handle, 0, &[7; 2048]).await.unwrap();
    let Err(VfsError::Io(reason)) = drive.fs.flush(handle).await else {
        panic!("the size check must fail");
    };
    assert!(reason.contains("The server holds"), "{reason}");
    drive.server.short_uploads.store(false, Ordering::Relaxed);
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(drive.server.file("/cut.bin").unwrap().len(), 2048);
}

#[tokio::test]
async fn unsent_files_are_kept_when_the_drive_goes_away() {
    let drive = drive();
    let (_, handle) = drive
        .fs
        .create(ROOT_NODE, "draft.md", OpenMode::WRITE_TRUNCATE, false, None)
        .await
        .unwrap();
    drive.fs.write(handle, 0, b"unsaved words").await.unwrap();
    assert_eq!(drive.fs.open_paths(), vec!["/draft.md".to_string()]);
    assert_eq!(drive.fs.keep_unsent(), vec!["/draft.md".to_string()]);
    let spool_folder = drive.spool_folder();
    let Drive { fs, server, folder } = drive;
    drop(fs);
    drop(server);
    let unsent = spool::unsent_files(&spool_folder);
    assert_eq!(unsent.len(), 1);
    assert_eq!(unsent[0].note.connection, "memory");
    assert_eq!(std::fs::read(&unsent[0].path).unwrap(), b"unsaved words");
    drop(folder);
}

#[tokio::test]
async fn a_drive_that_kept_nothing_leaves_no_spool_folder() {
    let drive = drive();
    let (_, handle) = drive
        .fs
        .create(ROOT_NODE, "sent.txt", OpenMode::WRITE_TRUNCATE, false, None)
        .await
        .unwrap();
    drive.fs.write(handle, 0, b"sent in time").await.unwrap();
    assert!(drive.spool_folder().is_dir());
    drive.fs.release(handle);
    until("the file is uploaded", || drive.spooled_files() == 0).await;
    assert_eq!(
        drive.server.file("/sent.txt").unwrap()[..],
        b"sent in time"[..]
    );
    let spool_folder = drive.spool_folder();
    let Drive { fs, server, folder } = drive;
    drop(fs);
    until("the spool folder is removed", || !spool_folder.exists()).await;
    drop(server);
    drop(folder);
}

#[tokio::test]
async fn unsent_changes_carry_a_note_from_the_first_write_on() {
    let drive = drive();
    drive.server.put_folder("/docs");
    let (_, handle) = drive
        .fs
        .create(ROOT_NODE, "draft.md", OpenMode::WRITE_TRUNCATE, false, None)
        .await
        .unwrap();
    assert!(
        spool::unsent_files(&drive.spool_folder()).is_empty(),
        "an empty new file holds nothing to lose"
    );
    drive.fs.write(handle, 0, b"words").await.unwrap();
    // What a crash now would leave behind says where it belongs.
    let unsent = spool::unsent_files(&drive.spool_folder());
    assert_eq!(unsent.len(), 1);
    assert_eq!(unsent[0].note.remote_path, "/draft.md");

    let docs = node(&drive, "/docs").await;
    drive
        .fs
        .rename(ROOT_NODE, "draft.md", docs, "final.md", false)
        .await
        .unwrap();
    // Sent before it moved, so what moved on the server is whole.
    assert_eq!(
        drive.server.file("/docs/final.md").unwrap()[..],
        b"words"[..]
    );
    assert!(spool::unsent_files(&drive.spool_folder()).is_empty());
    drive.fs.write(handle, 5, b" more").await.unwrap();
    let unsent = spool::unsent_files(&drive.spool_folder());
    assert_eq!(unsent[0].note.remote_path, "/docs/final.md");

    drive.fs.flush(handle).await.unwrap();
    assert!(spool::unsent_files(&drive.spool_folder()).is_empty());
    drive.fs.release(handle);
    until("the spool is removed", || drive.spooled_files() == 0).await;
}

#[tokio::test]
async fn a_file_removed_with_unsent_changes_leaves_nothing_behind() {
    let drive = drive();
    let (_, handle) = drive
        .fs
        .create(
            ROOT_NODE,
            "scratch.tmp",
            OpenMode::WRITE_TRUNCATE,
            false,
            None,
        )
        .await
        .unwrap();
    drive.fs.write(handle, 0, b"temporary").await.unwrap();
    drive
        .fs
        .remove_file(ROOT_NODE, "scratch.tmp")
        .await
        .unwrap();
    drive.fs.release(handle);
    until("the spool is removed", || drive.spooled_files() == 0).await;
    assert!(spool::unsent_files(&drive.spool_folder()).is_empty());
    assert!(!drive.server.exists("/scratch.tmp"));
}

#[tokio::test]
async fn a_read_only_drive_refuses_every_change() {
    let server = MemoryFs::new();
    server.put_file("/keep.txt", "keep");
    server.put_folder("/folder");
    let drive = drive_with(server, |options| options.read_only = true);
    let id = node(&drive, "/keep.txt").await;
    assert_eq!(
        drive.fs.open(id, OpenMode::READ_WRITE).await,
        Err(VfsError::ReadOnly)
    );
    assert_eq!(
        drive
            .fs
            .create(ROOT_NODE, "new", OpenMode::WRITE_TRUNCATE, false, None)
            .await
            .map(|_| ()),
        Err(VfsError::ReadOnly)
    );
    assert_eq!(
        drive.fs.make_folder(ROOT_NODE, "new").await,
        Err(VfsError::ReadOnly)
    );
    assert_eq!(
        drive.fs.remove_file(ROOT_NODE, "keep.txt").await,
        Err(VfsError::ReadOnly)
    );
    assert_eq!(
        drive.fs.remove_folder(ROOT_NODE, "folder").await,
        Err(VfsError::ReadOnly)
    );
    assert_eq!(
        drive
            .fs
            .rename(ROOT_NODE, "keep.txt", ROOT_NODE, "moved", true)
            .await,
        Err(VfsError::ReadOnly)
    );
    let touch = AttributeChanges {
        modified: Some(1),
        ..AttributeChanges::default()
    };
    assert_eq!(
        drive.fs.set_attributes(id, touch).await,
        Err(VfsError::ReadOnly)
    );
    assert_eq!(read_whole(&drive, "/keep.txt", 100).await, b"keep");
    assert!(drive.server.exists("/keep.txt"));
}

#[tokio::test]
async fn changes_made_elsewhere_show_once_the_listing_is_read_again() {
    let drive = drive_with(MemoryFs::new(), |options| {
        options.metadata_lifetime = Duration::from_millis(80);
    });
    drive.server.put_file("/feed.xml", "version one");
    assert_eq!(read_whole(&drive, "/feed.xml", 100).await, b"version one");

    drive.server.put_file("/feed.xml", "version two, longer");
    drive.server.put_file("/added.txt", "x");
    // Within the lifetime the cached answer stands.
    assert_eq!(
        drive.fs.lookup(ROOT_NODE, "feed.xml").await.unwrap().size,
        11
    );
    assert_eq!(
        drive.fs.lookup(ROOT_NODE, "added.txt").await,
        Err(VfsError::NotFound)
    );

    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(
        drive.fs.lookup(ROOT_NODE, "feed.xml").await.unwrap().size,
        19
    );
    assert!(drive.fs.lookup(ROOT_NODE, "added.txt").await.is_ok());
    assert_eq!(
        read_whole(&drive, "/feed.xml", 100).await,
        b"version two, longer",
        "blocks of the old version are not served"
    );
}

#[tokio::test]
async fn changes_through_the_drive_show_at_once() {
    let drive = drive_with(MemoryFs::new(), |options| {
        options.metadata_lifetime = Duration::from_secs(3600);
    });
    drive.server.put_file("/page.html", "<p>one</p>");
    assert_eq!(read_whole(&drive, "/page.html", 100).await, b"<p>one</p>");
    let listed = drive.server.counters.listings.load(Ordering::Relaxed);

    save(&drive, ROOT_NODE, "page.html", b"<p>two, with more</p>").await;
    save(&drive, ROOT_NODE, "style.css", b"p{}").await;
    drive.fs.make_folder(ROOT_NODE, "img").await.unwrap();
    assert_eq!(
        read_whole(&drive, "/page.html", 100).await,
        b"<p>two, with more</p>"
    );
    let names: Vec<(String, u64)> = drive
        .fs
        .read_folder(ROOT_NODE)
        .await
        .unwrap()
        .into_iter()
        .map(|(name, attributes)| (name, attributes.size))
        .collect();
    assert_eq!(
        names,
        vec![
            ("img".to_string(), 0),
            ("page.html".to_string(), 21),
            ("style.css".to_string(), 3),
        ]
    );
    drive.fs.remove_file(ROOT_NODE, "style.css").await.unwrap();
    assert_eq!(
        drive.fs.lookup(ROOT_NODE, "style.css").await,
        Err(VfsError::NotFound)
    );
    assert_eq!(
        drive.server.counters.listings.load(Ordering::Relaxed),
        listed,
        "the cached listing was updated in place, not read again"
    );
}

#[tokio::test]
async fn a_server_without_ranged_reads_is_read_through_a_local_copy() {
    let drive = drive();
    drive.server.ranged_reads.store(false, Ordering::Relaxed);
    let contents = pattern(2 * BLOCK_SIZE as usize + 5, 7);
    drive.server.put_file("/old-ftp.bin", contents.clone());
    let id = node(&drive, "/old-ftp.bin").await;
    let handle = drive.fs.open(id, OpenMode::READ).await.unwrap();
    let offset = BLOCK_SIZE as usize + 3;
    let data = drive.fs.read(handle, offset as u64, 500).await.unwrap();
    assert_eq!(data[..], contents[offset..offset + 500]);
    assert_eq!(
        drive.server.counters.read_requests.load(Ordering::Relaxed),
        1
    );
    drive.fs.release(handle);
    until("the local copy is removed", || drive.spooled_files() == 0).await;
    assert_eq!(drive.server.counters.uploads.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn times_and_permissions_reach_the_server() {
    let drive = drive();
    drive.server.put_file("/script.sh", "#!/bin/sh\n");
    let id = node(&drive, "/script.sh").await;
    let changed = drive
        .fs
        .set_attributes(
            id,
            AttributeChanges {
                modified: Some(1_600_000_000),
                permissions: Some(0o100755),
                ..AttributeChanges::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(changed.modified, Some(1_600_000_000));
    assert_eq!(changed.permissions, Some(0o755));
    assert_eq!(drive.server.modified("/script.sh"), Some(1_600_000_000));
    assert_eq!(drive.server.permissions("/script.sh"), Some(0o755));

    // A time set while the file is still being written goes with the upload, as `cp -p`
    // sets it before closing.
    let (_, handle) = drive
        .fs
        .create(
            ROOT_NODE,
            "copy.sh",
            OpenMode::WRITE_TRUNCATE,
            false,
            Some(0o700),
        )
        .await
        .unwrap();
    drive.fs.write(handle, 0, b"#!/bin/sh\n").await.unwrap();
    let copy = node(&drive, "/copy.sh").await;
    drive
        .fs
        .set_attributes(
            copy,
            AttributeChanges {
                modified: Some(1_500_000_000),
                ..AttributeChanges::default()
            },
        )
        .await
        .unwrap();
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(drive.server.modified("/copy.sh"), Some(1_500_000_000));
    assert_eq!(drive.server.permissions("/copy.sh"), Some(0o700));
    assert_eq!(
        drive.fs.attributes(copy).await.unwrap().modified,
        Some(1_500_000_000)
    );
}

#[tokio::test]
async fn names_the_local_system_refuses_are_shown_under_stand_ins() {
    let server = MemoryFs::new();
    server.put_file("/a:b.txt", "colon");
    server.put_file("/CON", "device");
    server.put_file("/Readme", "upper");
    server.put_file("/readme", "lower");
    let drive = drive_with(server, |options| options.style = NameStyle::Windows);
    let names: Vec<String> = drive
        .fs
        .read_folder(ROOT_NODE)
        .await
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        vec![
            "CO\u{F04E}".to_string(),
            "Readme".to_string(),
            "a\u{F03A}b.txt".to_string(),
            "readme (2)".to_string(),
        ]
    );
    assert_eq!(read_whole(&drive, "/a\u{F03A}b.txt", 100).await, b"colon");
    assert_eq!(read_whole(&drive, "/README (2)", 100).await, b"lower");
    assert_eq!(read_whole(&drive, "/README", 100).await, b"upper");

    save(&drive, ROOT_NODE, "x\u{F03F}y", b"question").await;
    assert_eq!(drive.server.file("/x?y").unwrap()[..], b"question"[..]);
    // Saving over a name that differs only by case writes the file that is there.
    save(&drive, ROOT_NODE, "README (2)", b"lower, again").await;
    assert_eq!(
        drive.server.file("/readme").unwrap()[..],
        b"lower, again"[..]
    );
    assert_eq!(drive.server.file("/Readme").unwrap()[..], b"upper"[..]);
    drive
        .fs
        .rename(ROOT_NODE, "x\u{F03F}y", ROOT_NODE, "plain", false)
        .await
        .unwrap();
    assert!(drive.server.exists("/plain"));
    assert!(!drive.server.exists("/x?y"));
}

#[tokio::test]
async fn a_lost_connection_fails_requests_and_the_drive_works_again_after() {
    let drive = drive_with(MemoryFs::new(), |options| {
        options.metadata_lifetime = Duration::from_millis(1);
    });
    drive.server.put_file("/file.txt", "contents");
    let id = node(&drive, "/file.txt").await;
    drive.server.offline.store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(matches!(
        drive.fs.lookup(ROOT_NODE, "file.txt").await,
        Err(VfsError::Io(_))
    ));
    assert!(matches!(
        drive.fs.make_folder(ROOT_NODE, "new").await,
        Err(VfsError::Io(_))
    ));
    drive.server.offline.store(false, Ordering::Relaxed);
    assert_eq!(
        drive.fs.lookup(ROOT_NODE, "file.txt").await.unwrap().node,
        id
    );
    assert_eq!(read_whole(&drive, "/file.txt", 100).await, b"contents");
}

#[tokio::test]
async fn a_file_removed_while_open_keeps_its_contents_until_closed() {
    let drive = drive();
    let (created, handle) = drive
        .fs
        .create(ROOT_NODE, "scratch.tmp", OpenMode::READ_WRITE, true, None)
        .await
        .unwrap();
    drive.fs.write(handle, 0, b"temporary").await.unwrap();
    drive
        .fs
        .remove_file(ROOT_NODE, "scratch.tmp")
        .await
        .unwrap();
    assert!(!drive.server.exists("/scratch.tmp"));
    assert_eq!(
        drive.fs.read(handle, 0, 100).await.unwrap()[..],
        b"temporary"[..]
    );
    assert_eq!(drive.fs.attributes(created.node).await.unwrap().size, 9);
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert!(
        !drive.server.exists("/scratch.tmp"),
        "nothing brings it back"
    );
    assert!(drive.fs.pending_uploads().is_empty());
    until("the spool is removed", || drive.spooled_files() == 0).await;
}

#[tokio::test]
async fn forgotten_nodes_are_let_go() {
    let drive = drive();
    drive.server.put_file("/one.txt", "1");
    let first = drive.fs.lookup(ROOT_NODE, "one.txt").await.unwrap();
    drive.fs.lookup(ROOT_NODE, "one.txt").await.unwrap();
    drive.fs.forget(first.node, 1);
    assert!(drive.fs.attributes(first.node).await.is_ok());
    drive.fs.forget(first.node, 1);
    assert_eq!(
        drive.fs.attributes(first.node).await,
        Err(VfsError::NotFound)
    );
    // Looked up again, the path gets the same id.
    assert_eq!(
        drive.fs.lookup(ROOT_NODE, "one.txt").await.unwrap().node,
        first.node
    );
}

/// The folders a drive reported changes in, each with the path below it that changed.
type Reported = Vec<(String, Option<String>)>;

#[tokio::test]
async fn every_change_on_the_server_is_reported_with_its_folder() {
    let server = MemoryFs::new();
    server.put_file("/read.txt", "read");
    let drive = drive_with(server, |_| {});
    let reported: Arc<Mutex<Reported>> = Arc::default();
    {
        let reported = reported.clone();
        drive
            .fs
            .report_changes(Arc::new(move |folder: &str, below: Option<&str>| {
                reported
                    .lock()
                    .unwrap()
                    .push((folder.to_string(), below.map(str::to_string)));
            }));
    }
    let take = || std::mem::take(&mut *reported.lock().unwrap());
    let changes = |pairs: &[(&str, Option<&str>)]| -> Reported {
        pairs
            .iter()
            .map(|(folder, below)| (folder.to_string(), below.map(str::to_string)))
            .collect()
    };

    let folder = drive.fs.make_folder(ROOT_NODE, "docs").await.unwrap().node;
    assert_eq!(take(), changes(&[("/", Some("/docs"))]));

    save(&drive, folder, "draft.txt", b"words").await;
    // Made empty, then sent with its contents.
    assert_eq!(take(), changes(&[("/docs", None), ("/docs", None)]));

    drive
        .fs
        .rename(folder, "draft.txt", ROOT_NODE, "final.txt", false)
        .await
        .unwrap();
    assert_eq!(
        take(),
        changes(&[("/docs", Some("/docs/draft.txt")), ("/", None)])
    );

    drive.fs.remove_file(ROOT_NODE, "final.txt").await.unwrap();
    drive.fs.remove_folder(ROOT_NODE, "docs").await.unwrap();
    assert_eq!(
        take(),
        changes(&[("/", Some("/final.txt")), ("/", Some("/docs"))])
    );

    // Reading changes nothing.
    let read = node(&drive, "/read.txt").await;
    let handle = drive.fs.open(read, OpenMode::READ).await.unwrap();
    drive.fs.read(handle, 0, 100).await.unwrap();
    drive.fs.release(handle);
    drive.fs.read_folder(ROOT_NODE).await.unwrap();
    assert!(take().is_empty());
}

#[tokio::test]
async fn links_are_read_from_the_listing_where_it_says_where_they_lead() {
    for in_listings in [true, false] {
        let drive = drive();
        drive
            .server
            .links_in_listings
            .store(in_listings, Ordering::Relaxed);
        drive.server.put_file("/docs/a.txt", "alpha");
        drive.server.put_file("/docs/b.txt", "beta");
        drive.server.put_link("/docs/one", "a.txt");
        drive.server.put_link("/docs/two", "/docs/b.txt");
        let target = |name: &'static str| {
            let drive = &drive;
            async move {
                let id = node(drive, &format!("/docs/{name}")).await;
                drive.fs.read_link(id).await
            }
        };
        assert_eq!(target("one").await, Ok("a.txt".to_string()));
        assert_eq!(target("two").await, Ok("b.txt".to_string()));
        let asked = drive.server.counters.link_reads.load(Ordering::Relaxed);
        assert_eq!(asked, if in_listings { 0 } else { 2 }, "{in_listings}");

        // A link moved over another leads where the moved one does.
        let docs = node(&drive, "/docs").await;
        drive
            .fs
            .rename(docs, "two", docs, "one", true)
            .await
            .unwrap();
        assert_eq!(
            target("one").await,
            Ok("b.txt".to_string()),
            "{in_listings}"
        );
    }
}

#[tokio::test]
async fn links_are_shown_as_links_and_removed_alone() {
    let server = MemoryFs::new();
    server.put_file("/outside/keep.txt", "keep");
    server.put_file("/tree/docs/a.txt", "alpha");
    server.put_link("/tree/dir/out", "../../outside");
    server.put_link("/tree/dir/loop", ".");
    server.put_link("/tree/dir/note", "/tree/docs/a.txt");
    server.put_link("/tree/dir/up", "../docs");
    // Inside the root by its letters, outside once `loop` is followed.
    server.put_link("/tree/dir/sneaky", "loop/../../../outside");
    server.put_link("/tree/dir/gone", "missing");
    let drive = drive_showing(server, "/tree", |_| {});

    let dir = node(&drive, "/dir").await;
    let listed: Vec<(String, NodeKind)> = drive
        .fs
        .read_folder(dir)
        .await
        .unwrap()
        .into_iter()
        .map(|(name, attributes)| (name, attributes.kind))
        .collect();
    let links = ["gone", "loop", "note", "out", "sneaky", "up"];
    assert_eq!(
        listed,
        links
            .iter()
            .map(|name| (name.to_string(), NodeKind::Link))
            .collect::<Vec<_>>()
    );

    let target = |name: &'static str| {
        let drive = &drive;
        async move {
            let link = drive.fs.lookup(dir, name).await.unwrap().node;
            drive.fs.read_link(link).await
        }
    };
    assert_eq!(target("loop").await, Ok(".".to_string()));
    assert_eq!(target("note").await, Ok("../docs/a.txt".to_string()));
    assert_eq!(target("up").await, Ok("../docs".to_string()));
    assert_eq!(target("gone").await, Ok("missing".to_string()));
    assert_eq!(target("out").await, Err(VfsError::PermissionDenied));
    assert_eq!(target("sneaky").await, Err(VfsError::PermissionDenied));
    let file = node(&drive, "/docs/a.txt").await;
    assert_eq!(drive.fs.read_link(file).await, Err(VfsError::InvalidName));

    let out = drive.fs.lookup(dir, "out").await.unwrap().node;
    assert_eq!(
        drive.fs.open(out, OpenMode::READ).await,
        Err(VfsError::Unsupported)
    );
    assert_eq!(
        drive.fs.lookup(out, "keep.txt").await,
        Err(VfsError::NotAFolder),
        "nothing is looked up through a link"
    );
    let note = drive.fs.lookup(dir, "note").await.unwrap().node;
    let unchanged = drive
        .fs
        .set_attributes(
            note,
            AttributeChanges {
                permissions: Some(0o600),
                ..AttributeChanges::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(unchanged.kind, NodeKind::Link);
    assert_eq!(drive.server.permissions("/tree/docs/a.txt"), Some(0o644));

    assert_eq!(
        drive.fs.remove_folder(dir, "out").await,
        Err(VfsError::NotAFolder)
    );
    drive.fs.remove_file(dir, "out").await.unwrap();
    drive.fs.remove_file(dir, "loop").await.unwrap();
    assert!(!drive.server.exists("/tree/dir/out"));
    assert_eq!(
        drive.server.file("/outside/keep.txt").unwrap()[..],
        b"keep"[..]
    );
    assert!(drive.server.exists("/tree/dir"));

    drive
        .fs
        .rename(dir, "note", ROOT_NODE, "moved", false)
        .await
        .unwrap();
    let moved = node(&drive, "/moved").await;
    assert_eq!(
        drive.fs.read_link(moved).await,
        Ok("docs/a.txt".to_string()),
        "the server's target stays, and reads from where the link is now"
    );
    assert_eq!(
        drive.server.file("/tree/docs/a.txt").unwrap()[..],
        b"alpha"[..]
    );
}

#[tokio::test]
async fn an_upload_is_written_beside_the_file_and_moved_into_its_place() {
    for moves_between_folders in [true, false] {
        let drive = drive();
        drive
            .server
            .moves_between_folders
            .store(moves_between_folders, Ordering::Relaxed);
        drive.server.put_file("/docs/log.txt", "the whole original");
        drive
            .server
            .set_attributes("/docs/log.txt", None, Some(0o640))
            .await
            .unwrap();
        let id = node(&drive, "/docs/log.txt").await;
        let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
        drive.fs.write(handle, 0, b"new").await.unwrap();

        // Refused, then cut short: either way the original stays whole, and nothing is left
        // beside it.
        drive.server.failing_uploads.store(1, Ordering::Relaxed);
        assert!(drive.fs.flush(handle).await.is_err());
        drive.server.short_uploads.store(true, Ordering::Relaxed);
        assert!(drive.fs.flush(handle).await.is_err());
        drive.server.short_uploads.store(false, Ordering::Relaxed);
        assert_eq!(
            drive.server.file("/docs/log.txt").unwrap()[..],
            b"the whole original"[..]
        );
        assert_eq!(server_names(&drive.server, "/docs").await, ["log.txt"]);

        drive.fs.flush(handle).await.unwrap();
        drive.fs.release(handle);
        assert_eq!(drive.server.file("/docs/log.txt").unwrap()[..], b"new"[..]);
        assert_eq!(drive.server.permissions("/docs/log.txt"), Some(0o640));
        assert_eq!(server_names(&drive.server, "/docs").await, ["log.txt"]);
    }
}

#[tokio::test]
async fn a_file_nobody_may_write_is_not_replaced() {
    let drive = drive();
    drive.server.put_file("/locked.txt", "the original");
    drive
        .server
        .set_attributes("/locked.txt", None, Some(0o444))
        .await
        .unwrap();
    let id = node(&drive, "/locked.txt").await;
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    drive.fs.write(handle, 0, b"the replacement").await.unwrap();
    let Err(VfsError::Io(reason)) = drive.fs.flush(handle).await else {
        panic!("the upload must be refused");
    };
    assert!(reason.contains("read-only on the server"), "{reason}");
    assert_eq!(
        drive.server.file("/locked.txt").unwrap()[..],
        b"the original"[..]
    );
    drive
        .server
        .set_attributes("/locked.txt", None, Some(0o644))
        .await
        .unwrap();
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(
        drive.server.file("/locked.txt").unwrap()[..],
        b"the replacement"[..]
    );
}

#[tokio::test]
async fn a_drive_that_goes_stops_its_uploads_and_leaves_nothing_beside_the_file() {
    let drive = drive();
    drive.server.put_file("/video.mp4", "the original");
    let id = node(&drive, "/video.mp4").await;
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    let contents = pattern(4 * 1024 * 1024, 8);
    drive.fs.write(handle, 0, &contents).await.unwrap();
    drive.server.write_delay_millis.store(40, Ordering::Relaxed);
    // Closed without a flush: the upload runs in the background.
    drive.fs.release(handle);
    until("a piece of the upload is on the server", || {
        drive
            .fs
            .pending_uploads()
            .iter()
            .any(|pending| pending.sent > 0)
    })
    .await;
    assert_eq!(
        server_names(&drive.server, "/").await.len(),
        2,
        "written beside the file"
    );

    assert_eq!(drive.fs.keep_unsent(), vec!["/video.mp4".to_string()]);
    drive.fs.stop_uploads();
    drive.fs.uploads_ended(Duration::from_secs(5)).await;
    assert!(drive
        .fs
        .pending_uploads()
        .iter()
        .all(|pending| !pending.uploading));
    assert_eq!(server_names(&drive.server, "/").await, ["video.mp4"]);
    assert_eq!(
        drive.server.file("/video.mp4").unwrap()[..],
        b"the original"[..]
    );
    let unsent = spool::unsent_files(&drive.spool_folder());
    assert_eq!(unsent.len(), 1);
    assert!(std::fs::read(&unsent[0].path).unwrap() == contents);
}

#[tokio::test]
async fn an_upload_cut_off_with_the_connection_is_removed_before_the_next_attempt() {
    let drive = drive();
    drive.server.put_file("/video.mp4", "the original");
    let id = node(&drive, "/video.mp4").await;
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    let contents = pattern(2 * 1024 * 1024, 11);
    drive.fs.write(handle, 0, &contents).await.unwrap();
    drive.server.write_delay_millis.store(20, Ordering::Relaxed);

    // Twice the connection goes halfway through: what was sent stays beside the file, as
    // nothing can remove it then, but one attempt's leftover at most.
    for _ in 0..2 {
        let flushing = {
            let fs = drive.fs.clone();
            tokio::spawn(async move { fs.flush(handle).await })
        };
        until("a piece of this attempt is on the server", || {
            drive
                .fs
                .pending_uploads()
                .iter()
                .any(|pending| pending.uploading && pending.sent > 0)
        })
        .await;
        drive.server.offline.store(true, Ordering::Relaxed);
        assert!(flushing.await.unwrap().is_err());
        drive.server.offline.store(false, Ordering::Relaxed);
        assert_eq!(drive.fs.left_beside().len(), 1);
        assert_eq!(server_names(&drive.server, "/").await.len(), 2);
        assert_eq!(
            drive.server.file("/video.mp4").unwrap()[..],
            b"the original"[..]
        );
    }

    // A server that does not answer in time keeps it remembered too.
    drive
        .server
        .delete_delay_millis
        .store(500, Ordering::Relaxed);
    drive.fs.tidy_staged(Duration::from_millis(50)).await;
    assert_eq!(drive.fs.left_beside().len(), 1);
    drive.server.delete_delay_millis.store(0, Ordering::Relaxed);

    // Without a connection it stays remembered; with one, a reconnect or the drive going
    // removes it, and so does the next attempt.
    drive.server.offline.store(true, Ordering::Relaxed);
    drive.fs.tidy_staged(Duration::from_secs(5)).await;
    assert_eq!(drive.fs.left_beside().len(), 1);
    drive.server.offline.store(false, Ordering::Relaxed);
    drive.fs.tidy_staged(Duration::from_secs(5)).await;
    assert!(drive.fs.left_beside().is_empty());
    assert_eq!(server_names(&drive.server, "/").await, ["video.mp4"]);

    drive.server.write_delay_millis.store(0, Ordering::Relaxed);
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(server_names(&drive.server, "/").await, ["video.mp4"]);
    assert!(drive.server.file("/video.mp4").unwrap()[..] == contents[..]);
}

#[tokio::test]
async fn an_original_that_cannot_be_put_back_is_named_and_both_copies_stay() {
    let drive = drive();
    drive.server.put_file("/docs/log.txt", "the original");
    let id = node(&drive, "/docs/log.txt").await;
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    drive.fs.write(handle, 0, b"the new log").await.unwrap();
    // The upload may not take the file's place, so the old file steps aside, and then may
    // not come back either.
    drive.server.refuse_moves_onto("/docs/log.txt");
    let Err(VfsError::Io(reason)) = drive.fs.flush(handle).await else {
        panic!("the upload must fail");
    };
    let names = server_names(&drive.server, "/docs").await;
    assert_eq!(names.len(), 2, "{names:?}");
    let old = names.iter().find(|name| name.ends_with(".old")).unwrap();
    let new = names.iter().find(|name| !name.ends_with(".old")).unwrap();
    assert!(reason.contains(old.as_str()), "{reason}");
    assert!(reason.contains(new.as_str()), "{reason}");
    assert_eq!(
        drive.server.file(&format!("/docs/{old}")).unwrap()[..],
        b"the original"[..]
    );
    assert_eq!(
        drive.server.file(&format!("/docs/{new}")).unwrap()[..],
        b"the new log"[..]
    );
    assert!(drive.fs.left_beside().is_empty(), "neither copy is removed");

    // Once the server lets it, the original goes back to its name, on a reconnect as before
    // the next attempt, and the next upload that succeeds removes the stranded copy.
    drive.server.allow_every_move();
    drive.fs.tidy_staged(Duration::from_secs(5)).await;
    assert_eq!(
        drive.server.file("/docs/log.txt").unwrap()[..],
        b"the original"[..]
    );
    assert!(!drive.server.exists(&format!("/docs/{old}")));
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(
        drive.server.file("/docs/log.txt").unwrap()[..],
        b"the new log"[..]
    );
    assert_eq!(server_names(&drive.server, "/docs").await, ["log.txt"]);
}

#[tokio::test]
async fn an_original_something_took_the_place_of_stays_aside() {
    let drive = drive();
    drive.server.put_file("/docs/log.txt", "the original");
    let id = node(&drive, "/docs/log.txt").await;
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    drive.fs.write(handle, 0, b"the new log").await.unwrap();
    drive.server.refuse_moves_onto("/docs/log.txt");
    assert!(drive.fs.flush(handle).await.is_err());
    let old = server_names(&drive.server, "/docs")
        .await
        .into_iter()
        .find(|name| name.ends_with(".old"))
        .unwrap();

    // Another client wrote the file meanwhile, so the original stays where it went.
    drive.server.allow_every_move();
    drive.server.put_file("/docs/log.txt", "written meanwhile");
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(
        drive.server.file("/docs/log.txt").unwrap()[..],
        b"the new log"[..]
    );
    assert_eq!(
        server_names(&drive.server, "/docs").await,
        [old.clone(), "log.txt".to_string()],
        "the stranded copy of the new contents is gone"
    );
    assert_eq!(
        drive.server.file(&format!("/docs/{old}")).unwrap()[..],
        b"the original"[..]
    );
}

#[tokio::test]
async fn a_folder_that_takes_no_new_files_has_its_files_written_in_place() {
    let drive = drive();
    drive.server.put_file("/drop/form.txt", "blank");
    drive
        .server
        .refused_new_files
        .store(true, Ordering::Relaxed);
    let id = node(&drive, "/drop/form.txt").await;
    let handle = drive.fs.open(id, OpenMode::WRITE_TRUNCATE).await.unwrap();
    drive.fs.write(handle, 0, b"filled in").await.unwrap();
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(
        drive.server.file("/drop/form.txt").unwrap()[..],
        b"filled in"[..]
    );
    assert_eq!(server_names(&drive.server, "/drop").await, ["form.txt"]);
}

#[tokio::test]
async fn a_rename_sends_what_is_unsent_first_and_fails_if_it_cannot() {
    let drive = drive();
    drive.server.put_file("/report.docx", "the good report");
    let (_, handle) = drive
        .fs
        .create(
            ROOT_NODE,
            "report.tmp",
            OpenMode::WRITE_TRUNCATE,
            false,
            None,
        )
        .await
        .unwrap();
    drive.fs.write(handle, 0, b"the new report").await.unwrap();
    drive.server.failing_uploads.store(2, Ordering::Relaxed);
    assert!(drive.fs.flush(handle).await.is_err());

    // Still open, as POSIX allows, and its contents only here.
    let refused = drive
        .fs
        .rename(ROOT_NODE, "report.tmp", ROOT_NODE, "report.docx", true)
        .await;
    assert_eq!(
        refused,
        Err(VfsError::Io("The server refused the upload".into()))
    );
    assert_eq!(
        drive.server.file("/report.docx").unwrap()[..],
        b"the good report"[..],
        "the good file is not replaced by an empty one"
    );
    assert!(drive.server.exists("/report.tmp"));

    drive
        .fs
        .rename(ROOT_NODE, "report.tmp", ROOT_NODE, "report.docx", true)
        .await
        .unwrap();
    assert_eq!(
        drive.server.file("/report.docx").unwrap()[..],
        b"the new report"[..]
    );
    assert!(!drive.server.exists("/report.tmp"));
    drive.fs.flush(handle).await.unwrap();
    drive.fs.release(handle);
    assert_eq!(
        drive.server.file("/report.docx").unwrap()[..],
        b"the new report"[..]
    );
}

#[tokio::test]
async fn moving_a_folder_waits_for_uploads_below_it() {
    let drive = drive();
    let folder = drive.fs.make_folder(ROOT_NODE, "album").await.unwrap().node;
    let (_, handle) = drive
        .fs
        .create(folder, "photo.raw", OpenMode::WRITE_TRUNCATE, false, None)
        .await
        .unwrap();
    drive.fs.write(handle, 0, &pattern(9000, 3)).await.unwrap();
    drive
        .server
        .upload_delay_millis
        .store(300, Ordering::Relaxed);
    // Closed without a flush: the upload runs in the background.
    drive.fs.release(handle);
    until("the upload runs", || {
        drive
            .fs
            .pending_uploads()
            .iter()
            .any(|pending| pending.uploading)
    })
    .await;
    drive
        .fs
        .rename(ROOT_NODE, "album", ROOT_NODE, "trip", false)
        .await
        .unwrap();
    assert_eq!(
        drive.server.file("/trip/photo.raw").unwrap()[..],
        pattern(9000, 3)[..]
    );
    assert!(!drive.server.exists("/album"));
    assert!(drive.fs.pending_uploads().is_empty());
}

#[tokio::test]
async fn a_reader_of_a_replaced_file_reads_what_replaced_it() {
    let drive = drive();
    drive.server.put_file("/doc.txt", "the old words");
    let reader = node(&drive, "/doc.txt").await;
    let handle = drive.fs.open(reader, OpenMode::READ).await.unwrap();
    assert_eq!(
        drive.fs.read(handle, 0, 7).await.unwrap()[..],
        b"the old"[..]
    );

    save(&drive, ROOT_NODE, ".doc.txt.new", b"the new words, longer").await;
    drive
        .fs
        .rename(ROOT_NODE, ".doc.txt.new", ROOT_NODE, "doc.txt", true)
        .await
        .unwrap();
    assert_eq!(
        drive.fs.read(handle, 0, 100).await.unwrap()[..],
        b"the new words, longer"[..]
    );
    assert_eq!(drive.fs.attributes(reader).await.unwrap().size, 21);
    drive.fs.release(handle);
    drive.fs.forget(reader, 1);
    assert_eq!(
        read_whole(&drive, "/doc.txt", 100).await,
        b"the new words, longer"
    );
}

#[tokio::test]
async fn trying_again_covers_an_attempt_under_way_and_open_files() {
    let drive = drive();
    let (_, handle) = drive
        .fs
        .create(ROOT_NODE, "held.txt", OpenMode::WRITE_TRUNCATE, false, None)
        .await
        .unwrap();
    drive.fs.write(handle, 0, b"held open").await.unwrap();
    drive
        .server
        .upload_delay_millis
        .store(300, Ordering::Relaxed);
    drive.server.failing_uploads.store(1, Ordering::Relaxed);
    let flushing = {
        let fs = drive.fs.clone();
        tokio::spawn(async move { fs.flush(handle).await })
    };
    until("the attempt runs", || {
        drive
            .fs
            .pending_uploads()
            .iter()
            .any(|pending| pending.uploading)
    })
    .await;
    // Asked while the attempt that is to fail still runs, and the file is open. The flush
    // hears of that failure, or of the retry's success if it came first.
    drive.fs.retry_unsent();
    let _ = flushing.await.unwrap();
    until("the retry got through", || {
        drive
            .server
            .file("/held.txt")
            .is_some_and(|data| data[..] == b"held open"[..])
    })
    .await;
    until("nothing is pending", || {
        drive.fs.pending_uploads().is_empty()
    })
    .await;
    drive.fs.release(handle);
}

#[tokio::test]
async fn a_file_changed_on_the_server_since_it_was_listed_is_not_edited() {
    for now in ["grown since the listing", "short"] {
        let drive = drive();
        drive.server.put_file("/notes.txt", "as listed");
        let id = node(&drive, "/notes.txt").await;
        // Behind the drive's back, within the listing's lifetime.
        drive.server.put_file("/notes.txt", now);
        let handle = drive.fs.open(id, OpenMode::READ_WRITE).await.unwrap();
        let Err(VfsError::Io(reason)) = drive.fs.write(handle, 0, b"edit").await else {
            panic!("the edit must fail");
        };
        assert!(reason.contains("The server sent"), "{reason}");
        drive.fs.release(handle);
        assert_eq!(
            drive.server.file("/notes.txt").unwrap()[..],
            now.as_bytes()[..]
        );
    }
}
