//! Bulk transfer benchmark against a real SSH server. Run it with
//! `cargo run --release --example transfer_benchmark`.
//!
//! It uses the server and account the end-to-end tests use, at `POROS_BENCH_HOST` (default
//! 127.0.0.1), and moves 3,600 files of incompressible data, 1.4 GiB in all, up and back down
//! with 1, 4 and 8 workers. Every other transfer setting keeps its default.
//!
//! `POROS_BENCH_WORKERS` takes other worker counts (`4,8`), `POROS_BENCH_RUNS` repeats each
//! transfer and reports the median, and `POROS_BENCH_CHECKS` picks the integrity checks:
//! `default` (all on), `off`, `checksums` or `flush` (only that one on). With
//! `POROS_BENCH_DROP_CACHES=1`, run as root on Linux, the page cache is dropped before each
//! transfer.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use poros_lib::events::Events;
use poros_lib::protocol::Protocol;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobState, TransferItem, TransferManager,
};

/// The window label sessions belong to.
const OWNER: &str = "main";
const KIBIBYTE: usize = 1024;
const MEBIBYTE: usize = 1024 * 1024;
const DEFAULT_WORKER_COUNTS: &str = "1,4,8";
const FOLDERS: usize = 36;

fn profile() -> ConnectProfile {
    let port = std::env::var("POROS_TEST_SSH_PORT")
        .expect("set POROS_TEST_SSH_PORT to the test server's port")
        .parse()
        .expect("POROS_TEST_SSH_PORT is not a port number");
    ConnectProfile {
        protocol: Protocol::Sftp,
        host: std::env::var("POROS_BENCH_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port,
        username: std::env::var("POROS_TEST_SSH_USER").unwrap_or_else(|_| "poros".into()),
        auth: AuthMethod::Password {
            password: std::env::var("POROS_TEST_SSH_PASSWORD")
                .unwrap_or_else(|_| "poros-pass".into()),
        },
        initial_path: None,
        timeout_secs: Some(10),
        keepalive_secs: None,
        compression: false,
        receive_buffer_kib: None,
        send_buffer_kib: None,
        saved_connection_id: None,
        ftp_active: false,
        bypass_proxy: false,
        jump_connection_id: None,
        rsync_path: None,
        s3: Default::default(),
        route: Route::default(),
    }
}

async fn connect(manager: &SessionManager) -> SessionInfo {
    match manager.connect(profile(), None, OWNER).await {
        Ok(info) => info,
        Err(error) => {
            let host_key = error
                .host_key
                .unwrap_or_else(|| panic!("could not connect: {}", error.message));
            let approval = HostKeyApproval {
                fingerprint: host_key.fingerprint,
                remember: true,
            };
            manager
                .connect(profile(), Some(approval), OWNER)
                .await
                .unwrap()
        }
    }
}

fn random_bytes(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    let mut bytes = Vec::with_capacity(length + 8);
    while bytes.len() < length {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.extend_from_slice(&state.to_le_bytes());
    }
    bytes.truncate(length);
    bytes
}

/// 3,000 files of 16 to 256 KiB, 560 of 1 MiB and 40 of 12 MiB, spread over 36 folders.
/// Returns the file count and total size in bytes.
fn write_workload(root: &Path) -> (usize, u64) {
    let mut sizes: Vec<usize> = (0..3000)
        .map(|index| 16 * KIBIBYTE + (index * 7919) % (240 * KIBIBYTE))
        .collect();
    sizes.extend([MEBIBYTE; 560]);
    sizes.extend([12 * MEBIBYTE; 40]);
    for (index, size) in sizes.iter().enumerate() {
        let folder = root.join(format!("folder-{:02}", index % FOLDERS));
        std::fs::create_dir_all(&folder).unwrap();
        let contents = random_bytes(*size, index as u64 + 1);
        std::fs::write(folder.join(format!("file-{index:04}.bin")), contents).unwrap();
    }
    (sizes.len(), sizes.iter().map(|size| *size as u64).sum())
}

/// The transfer settings for `POROS_BENCH_CHECKS`.
fn checked_settings(workers: u32) -> TransferSettings {
    let defaults = TransferSettings {
        workers,
        exists_action: ExistsAction::Overwrite,
        ..TransferSettings::default()
    };
    let off = TransferSettings {
        verify_checksums: false,
        read_back: false,
        verify_resume: false,
        check_metadata: false,
        check_published: false,
        flush_to_disk: false,
        ..defaults.clone()
    };
    match std::env::var("POROS_BENCH_CHECKS").as_deref() {
        Ok("off") => off,
        Ok("checksums") => TransferSettings {
            verify_checksums: true,
            read_back: true,
            ..off
        },
        Ok("flush") => TransferSettings {
            flush_to_disk: true,
            ..off
        },
        Ok("default") | Err(_) => defaults,
        Ok(other) => panic!("POROS_BENCH_CHECKS is default, off, checksums or flush, not {other}"),
    }
}

fn drop_caches() {
    if std::env::var("POROS_BENCH_DROP_CACHES").as_deref() == Ok("1") {
        let _ = std::process::Command::new("sync").status();
        std::fs::write("/proc/sys/vm/drop_caches", "3").expect("could not drop the page cache");
    }
}

/// Queues one folder and returns how long the whole batch took.
async fn run(
    manager: &Arc<SessionManager>,
    session: &SessionInfo,
    workers: u32,
    direction: Direction,
    source: TransferItem,
    target: &str,
) -> Duration {
    let transfers = TransferManager::new(
        manager.clone(),
        Events::default(),
        checked_settings(workers),
    );
    drop_caches();
    let started = Instant::now();
    transfers
        .enqueue(EnqueueRequest {
            session_id: session.id.clone(),
            source_session_id: None,
            direction,
            target_directory: target.to_string(),
            items: vec![source],
        })
        .await
        .unwrap();
    loop {
        let list = transfers.list();
        let counts = &list.stats.counts;
        if counts.queued == 0 && counts.running == 0 {
            let elapsed = started.elapsed();
            let unfinished = list
                .jobs
                .iter()
                .filter(|job| job.state != JobState::Done)
                .count();
            assert_eq!(unfinished, 0, "{unfinished} transfers did not finish");
            return elapsed;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn report(direction: &str, workers: u32, files: usize, bytes: u64, runs: Vec<Duration>) {
    let each: Vec<String> = runs
        .iter()
        .map(|run| format!("{:.2}", run.as_secs_f64()))
        .collect();
    let elapsed = median(runs);
    let seconds = elapsed.as_secs_f64();
    let megabytes_per_second = bytes as f64 / seconds / 1e6;
    println!(
        "{direction:<8} {workers} workers  {files} files  {:.2} GiB  {seconds:6.2} s  {megabytes_per_second:6.1} MB/s  {:5.0} Mbit/s  runs: {}",
        bytes as f64 / (1u64 << 30) as f64,
        megabytes_per_second * 8.0,
        each.join(" "),
    );
}

#[tokio::main]
async fn main() {
    let known_hosts = tempfile::tempdir().unwrap();
    let manager = Arc::new(SessionManager::new(
        known_hosts.path().join("known_hosts"),
        Events::default(),
    ));
    let session = connect(&manager).await;
    let browsing = manager.get(&session.id).await.unwrap();
    let fs = browsing.sftp().unwrap();

    let local = tempfile::tempdir().unwrap();
    let source = local.path().join("workload");
    let (files, bytes) = write_workload(&source);
    let upload_item = TransferItem {
        path: source.to_string_lossy().into_owned(),
        name: "workload".into(),
        is_dir: true,
        size: 0,
    };

    let worker_counts: Vec<u32> = std::env::var("POROS_BENCH_WORKERS")
        .unwrap_or_else(|_| DEFAULT_WORKER_COUNTS.into())
        .split(',')
        .map(|count| {
            count
                .trim()
                .parse()
                .expect("POROS_BENCH_WORKERS lists numbers")
        })
        .collect();
    let repeats: usize = std::env::var("POROS_BENCH_RUNS").ok().map_or(1, |runs| {
        runs.parse().expect("POROS_BENCH_RUNS is a number")
    });
    for workers in worker_counts {
        let mut uploads = Vec::new();
        let mut downloads = Vec::new();
        for _ in 0..repeats {
            let scratch = format!("poros-benchmark-{}", std::process::id());
            let remote = fs.make_dir("~", &scratch).await.unwrap();
            uploads.push(
                run(
                    &manager,
                    &session,
                    workers,
                    Direction::Upload,
                    upload_item.clone(),
                    &remote,
                )
                .await,
            );

            let downloaded = local.path().join("downloaded");
            std::fs::create_dir_all(&downloaded).unwrap();
            let download_item = TransferItem {
                path: format!("{remote}/workload"),
                name: "workload".into(),
                is_dir: true,
                size: 0,
            };
            downloads.push(
                run(
                    &manager,
                    &session,
                    workers,
                    Direction::Download,
                    download_item,
                    &downloaded.to_string_lossy(),
                )
                .await,
            );

            fs.delete(std::slice::from_ref(&remote)).await.unwrap();
            std::fs::remove_dir_all(&downloaded).unwrap();
        }
        report("upload", workers, files, bytes, uploads);
        report("download", workers, files, bytes, downloads);
    }
    manager.disconnect(&session.id).await.unwrap();
}
