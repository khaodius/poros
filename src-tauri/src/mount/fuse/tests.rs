//! The drive as the kernel and ordinary programs see it, over a server held in memory. These
//! tests mount for real, so they pass without doing anything where FUSE is not available.

use std::io::{Read, Write};
use std::os::fd::IntoRawFd;
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use super::super::backend::{Limits, SharedBackend};
use super::super::memory::MemoryFs;
use super::super::spool;
use super::super::vfs::{FsOptions, MountFs};
use super::{device, FuseMount};

struct Drive {
    server: Arc<MemoryFs>,
    core: Arc<MountFs>,
    mount: FuseMount,
    at: PathBuf,
    ended: Arc<AtomicBool>,
    /// Holds the mount point and the spool folder.
    folder: tempfile::TempDir,
}

fn core_for(server: &Arc<MemoryFs>, folder: &Path, read_only: bool) -> Arc<MountFs> {
    let mut options = FsOptions::new("memory", folder.join("spool"));
    options.read_only = read_only;
    let backend = Arc::new(SharedBackend::new(server.clone(), Limits::SHARED));
    MountFs::new(backend, "/", options)
}

/// A drive showing `server`, or `None` where this computer cannot mount.
fn mount_with(server: Arc<MemoryFs>, read_only: bool) -> Option<Drive> {
    if let Some(missing) = device::missing() {
        eprintln!("Skipped: {}", missing.problem);
        return None;
    }
    let folder = tempfile::tempdir().unwrap();
    let at = folder.path().join("drive");
    let core = core_for(&server, folder.path(), read_only);
    let ended = Arc::new(AtomicBool::new(false));
    let mount = FuseMount::start(
        core.clone(),
        &at,
        "memory test",
        Box::new({
            let ended = ended.clone();
            move || ended.store(true, Ordering::Relaxed)
        }),
    )
    .unwrap();
    Some(Drive {
        server,
        core,
        mount,
        at,
        ended,
        folder,
    })
}

fn mount(server: Arc<MemoryFs>) -> Option<Drive> {
    mount_with(server, false)
}

/// Runs file operations the way another program would: off the threads that serve the drive.
async fn program<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work).await.unwrap()
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

fn errno(error: &std::io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn programs_list_stat_and_read_files() {
    let server = MemoryFs::new();
    let big = pattern(3 * 1024 * 1024 + 4321, 7);
    server.put_file("/docs/notes.txt", "hello there");
    server.put_file("/big.bin", big.clone());
    let Some(drive) = mount(server) else {
        return;
    };
    let at = drive.at.clone();
    program(move || {
        let mut names: Vec<String> = std::fs::read_dir(&at)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["big.bin", "docs"]);

        let listed = std::fs::metadata(at.join("big.bin")).unwrap();
        assert!(listed.is_file());
        assert_eq!(listed.len(), big.len() as u64);
        assert_eq!(listed.permissions().mode() & 0o777, 0o644);
        assert!(std::fs::metadata(at.join("docs")).unwrap().is_dir());
        assert_eq!(
            std::fs::read_to_string(at.join("docs/notes.txt")).unwrap(),
            "hello there"
        );
        assert_eq!(std::fs::read(at.join("big.bin")).unwrap(), big);

        let file = std::fs::File::open(at.join("big.bin")).unwrap();
        let mut part = vec![0; 5000];
        file.read_exact_at(&mut part, 2 * 1024 * 1024 - 100)
            .unwrap();
        assert_eq!(
            part,
            big[2 * 1024 * 1024 - 100..2 * 1024 * 1024 + 4900].to_vec()
        );

        let missing = std::fs::metadata(at.join("missing")).unwrap_err();
        assert_eq!(errno(&missing), libc::ENOENT);
        let not_a_folder = std::fs::read_dir(at.join("big.bin")).unwrap_err();
        assert_eq!(errno(&not_a_folder), libc::ENOTDIR);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn what_programs_write_is_on_the_server_when_they_close() {
    let Some(drive) = mount(MemoryFs::new()) else {
        return;
    };
    let at = drive.at.clone();
    let server = drive.server.clone();
    program(move || {
        std::fs::write(at.join("new.txt"), "one").unwrap();
        assert_eq!(server.file("/new.txt").unwrap(), "one");

        let mut appended = std::fs::OpenOptions::new()
            .append(true)
            .open(at.join("new.txt"))
            .unwrap();
        appended.write_all(b" two").unwrap();
        drop(appended);
        assert_eq!(server.file("/new.txt").unwrap(), "one two");

        let shortened = std::fs::OpenOptions::new()
            .write(true)
            .open(at.join("new.txt"))
            .unwrap();
        shortened.set_len(3).unwrap();
        shortened.write_all_at(b"O", 0).unwrap();
        shortened.sync_all().unwrap();
        assert_eq!(server.file("/new.txt").unwrap(), "One");
        drop(shortened);

        // The way editors save: write beside the file, then rename over it.
        std::fs::write(at.join(".new.txt.swp"), "saved by rename").unwrap();
        std::fs::rename(at.join(".new.txt.swp"), at.join("new.txt")).unwrap();
        assert_eq!(server.file("/new.txt").unwrap(), "saved by rename");
        assert!(!server.exists("/.new.txt.swp"));
        assert_eq!(
            std::fs::read_to_string(at.join("new.txt")).unwrap(),
            "saved by rename"
        );

        let large = pattern(5 * 1024 * 1024 + 99, 3);
        std::fs::write(at.join("large.bin"), &large).unwrap();
        assert_eq!(server.file("/large.bin").unwrap(), large);
        assert_eq!(std::fs::read(at.join("large.bin")).unwrap(), large);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folders_moves_times_and_modes_reach_the_server() {
    let Some(drive) = mount(MemoryFs::new()) else {
        return;
    };
    let at = drive.at.clone();
    let server = drive.server.clone();
    program(move || {
        std::fs::create_dir(at.join("work")).unwrap();
        assert!(server.exists("/work"));
        let again = std::fs::create_dir(at.join("work")).unwrap_err();
        assert_eq!(errno(&again), libc::EEXIST);
        std::fs::write(at.join("work/draft.txt"), "draft").unwrap();

        let full = std::fs::remove_dir(at.join("work")).unwrap_err();
        assert_eq!(errno(&full), libc::ENOTEMPTY);

        std::fs::create_dir(at.join("done")).unwrap();
        std::fs::rename(at.join("work/draft.txt"), at.join("done/final.txt")).unwrap();
        assert!(!server.exists("/work/draft.txt"));
        assert_eq!(server.file("/done/final.txt").unwrap(), "draft");

        std::fs::set_permissions(
            at.join("done/final.txt"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert_eq!(server.permissions("/done/final.txt"), Some(0o600));
        let when = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        std::fs::File::options()
            .write(true)
            .open(at.join("done/final.txt"))
            .unwrap()
            .set_modified(when)
            .unwrap();
        assert_eq!(server.modified("/done/final.txt"), Some(1_600_000_000));
        let seen = std::fs::metadata(at.join("done/final.txt")).unwrap();
        assert_eq!(seen.modified().unwrap(), when);
        assert_eq!(seen.permissions().mode() & 0o777, 0o600);

        std::fs::remove_dir(at.join("work")).unwrap();
        std::fs::remove_dir_all(at.join("done")).unwrap();
        assert!(!server.exists("/work"));
        assert!(!server.exists("/done"));
        assert_eq!(std::fs::read_dir(&at).unwrap().count(), 0);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_upload_fails_the_close_and_the_file_is_kept() {
    let Some(drive) = mount(MemoryFs::new()) else {
        return;
    };
    let at = drive.at.clone();
    let server = drive.server.clone();
    program(move || {
        let mut file = std::fs::File::create(at.join("report.txt")).unwrap();
        file.write_all(b"quarterly numbers").unwrap();
        // More than this test needs: a close in any program started meanwhile, which
        // inherits the descriptor until it runs, asks for the upload too.
        server.failing_uploads.store(1000, Ordering::Relaxed);
        let synced = file.sync_all().unwrap_err();
        assert_eq!(errno(&synced), libc::EIO);
        // SAFETY: the descriptor was just taken from an open file and is closed once.
        let closed = unsafe { libc::close(file.into_raw_fd()) };
        assert_eq!(closed, -1, "close reports the failed upload");
        assert_eq!(errno(&std::io::Error::last_os_error()), libc::EIO);
        assert_eq!(server.file("/report.txt").unwrap(), "");
    })
    .await;

    let unsent = spool::unsent_files(&drive.folder.path().join("spool"));
    assert_eq!(unsent.len(), 1);
    assert_eq!(unsent[0].note.remote_path, "/report.txt");
    assert_eq!(
        std::fs::read(&unsent[0].path).unwrap(),
        b"quarterly numbers"
    );
    let pending = drive.core.pending_uploads();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].error.is_some());

    // The drive still shows what was written, and sending it again works.
    drive.server.failing_uploads.store(0, Ordering::Relaxed);
    let at = drive.at.clone();
    let server = drive.server.clone();
    program(move || {
        assert_eq!(
            std::fs::read_to_string(at.join("report.txt")).unwrap(),
            "quarterly numbers"
        );
        let file = std::fs::File::options()
            .write(true)
            .open(at.join("report.txt"))
            .unwrap();
        file.sync_all().unwrap();
        assert_eq!(server.file("/report.txt").unwrap(), "quarterly numbers");
    })
    .await;
    assert!(drive.core.pending_uploads().is_empty());
    assert!(spool::unsent_files(&drive.folder.path().join("spool")).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_program_waiting_on_a_slow_server_can_be_interrupted() {
    let server = MemoryFs::new();
    server.put_file("/slow.bin", pattern(256 * 1024, 5));
    server.put_file("/quick.txt", "still here");
    server.read_delay_millis.store(60_000, Ordering::Relaxed);
    let Some(drive) = mount(server) else {
        return;
    };
    let at = drive.at.clone();
    let waited = program(move || {
        let mut reader = std::process::Command::new("cat")
            .arg(at.join("slow.bin"))
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(700));
        assert!(
            reader.try_wait().unwrap().is_none(),
            "the read is still waiting on the server"
        );
        let killed = Instant::now();
        reader.kill().unwrap();
        reader.wait().unwrap();
        killed.elapsed()
    })
    .await;
    assert!(
        waited < Duration::from_secs(5),
        "the program went away {waited:?} after it was killed"
    );
    // The drive goes on working for everyone else.
    drive.server.read_delay_millis.store(0, Ordering::Relaxed);
    let at = drive.at.clone();
    let read = program(move || std::fs::read_to_string(at.join("quick.txt")).unwrap()).await;
    assert_eq!(read, "still here");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn several_programs_work_on_a_drive_at_once() {
    let server = MemoryFs::new();
    for index in 0..6 {
        server.put_file(
            &format!("/in/{index}.bin"),
            pattern(1024 * 1024 + index * 1000, index as u64),
        );
    }
    server.read_delay_millis.store(3, Ordering::Relaxed);
    let Some(drive) = mount(server) else {
        return;
    };
    let mut programs = Vec::new();
    for index in 0..6usize {
        let at = drive.at.clone();
        programs.push(tokio::task::spawn_blocking(move || {
            let read = std::fs::read(at.join(format!("in/{index}.bin"))).unwrap();
            assert_eq!(read, pattern(1024 * 1024 + index * 1000, index as u64));
            std::fs::create_dir_all(at.join("out")).unwrap();
            std::fs::write(at.join(format!("out/{index}.bin")), &read).unwrap();
            std::fs::read_dir(at.join("in")).unwrap().count()
        }));
    }
    for program in programs {
        assert_eq!(program.await.unwrap(), 6);
    }
    for index in 0..6usize {
        assert_eq!(
            drive.server.file(&format!("/out/{index}.bin")).unwrap(),
            pattern(1024 * 1024 + index * 1000, index as u64)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_only_drive_refuses_every_change() {
    let server = MemoryFs::new();
    server.put_file("/keep.txt", "as it is");
    let Some(drive) = mount_with(server, true) else {
        return;
    };
    let at = drive.at.clone();
    program(move || {
        assert_eq!(
            std::fs::read_to_string(at.join("keep.txt")).unwrap(),
            "as it is"
        );
        for refused in [
            std::fs::write(at.join("keep.txt"), "changed").unwrap_err(),
            std::fs::write(at.join("new.txt"), "new").unwrap_err(),
            std::fs::remove_file(at.join("keep.txt")).unwrap_err(),
            std::fs::create_dir(at.join("folder")).unwrap_err(),
            std::fs::rename(at.join("keep.txt"), at.join("moved.txt")).unwrap_err(),
        ] {
            assert_eq!(errno(&refused), libc::EROFS);
        }
    })
    .await;
    assert_eq!(drive.server.file("/keep.txt").unwrap(), "as it is");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unmounting_waits_for_open_files_unless_forced() {
    let server = MemoryFs::new();
    server.put_file("/open.txt", "held open");
    server.put_file("/other.txt", "not held");
    let Some(drive) = mount(server) else {
        return;
    };
    let at = drive.at.clone();
    let mut held = program(move || std::fs::File::open(at.join("open.txt")).unwrap()).await;

    let Drive {
        mount, at, folder, ..
    } = drive;
    let mount = Arc::new(mount);
    let refused = program({
        let mount = mount.clone();
        move || mount.unmount(false)
    })
    .await;
    assert!(refused.is_err());
    let still_there = program({
        let at = at.clone();
        move || std::fs::read_to_string(at.join("other.txt")).unwrap()
    })
    .await;
    assert_eq!(still_there, "not held");

    program({
        let mount = mount.clone();
        move || mount.unmount(true)
    })
    .await
    .unwrap();
    assert!(!device::mounted()
        .iter()
        .any(|entry| entry.mount_point == at));
    assert!(!at.exists(), "the folder made for the drive goes with it");
    let mut contents = String::new();
    assert!(
        program(move || held.read_to_string(&mut contents))
            .await
            .is_err(),
        "a file left open on a drive unmounted by force fails"
    );
    drop(folder);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_drive_unmounted_from_outside_is_noticed() {
    let Some(drive) = mount(MemoryFs::new()) else {
        return;
    };
    let at = drive.at.clone();
    program(move || device::unmount(&at, false).unwrap()).await;
    let waiting = Instant::now();
    while !drive.ended.load(Ordering::Relaxed) {
        assert!(waiting.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let Drive { mount, .. } = drive;
    program(move || mount.unmount(false)).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_drive_left_by_a_crash_is_cleared_at_the_next_mount() {
    if device::missing().is_some() {
        return;
    }
    let folder = tempfile::tempdir().unwrap();
    let at = folder.path().join("left behind");
    std::fs::create_dir(&at).unwrap();
    // What a crash leaves: the folder still mounted, with nothing answering.
    drop(device::mount(&at, "crashed", false).unwrap());
    let dead = std::fs::metadata(&at).unwrap_err();
    assert_eq!(errno(&dead), libc::ENOTCONN);

    let server = MemoryFs::new();
    server.put_file("/back.txt", "working again");
    let core = core_for(&server, folder.path(), false);
    let mount = FuseMount::start(core, &at, "memory test", Box::new(|| {})).unwrap();
    let read = program({
        let at = at.clone();
        move || std::fs::read_to_string(at.join("back.txt")).unwrap()
    })
    .await;
    assert_eq!(read, "working again");
    program(move || mount.unmount(false)).await.unwrap();
    assert!(at.is_dir(), "a folder that was there before stays");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changes_made_behind_the_drive_show_up_when_listings_expire() {
    let server = MemoryFs::new();
    server.put_file("/first.txt", "1");
    let Some(drive) = mount(server) else {
        return;
    };
    let at = drive.at.clone();
    let count = program({
        let at = at.clone();
        move || std::fs::read_dir(&at).unwrap().count()
    })
    .await;
    assert_eq!(count, 1);
    drive.server.put_file("/second.txt", "2");
    drive.core.forget_cached();
    let read = program(move || std::fs::read_to_string(at.join("second.txt")).unwrap()).await;
    assert_eq!(read, "2");
}
