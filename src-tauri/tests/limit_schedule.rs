//! End-to-end tests of the speed limit schedule against real servers, with the clock the
//! schedule reads set by the test instead of waited on. The SFTP tests need
//! `POROS_TEST_SSH_PORT` (the server `sftp_server.rs` describes) and the FTP one
//! `POROS_TEST_FTP_PORT` (the one `ftp_server.rs` describes); each is skipped without it.

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset, TimeZone};
use poros_lib::events::Events;
use poros_lib::protocol::Protocol;
use poros_lib::session::{SessionInfo, SessionManager};
use poros_lib::settings::TransferSettings;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval, Route};
use poros_lib::transfer::{
    Direction, EnqueueRequest, ExistsAction, JobSnapshot, JobState, LimitClock, LimitRule,
    LimitSchedule, RateLimit, TransferItem, TransferList, TransferManager,
};

/// The window label sessions belong to.
const OWNER: &str = "main";
const MEBIBYTE: u64 = 1024 * 1024;
const LIMIT_KIB: u32 = 512;
const LIMIT_BYTES: f64 = 512.0 * 1024.0;
const EVERY_DAY: [u8; 7] = [0, 1, 2, 3, 4, 5, 6];
/// What the requests one worker keeps in flight hold, at the request size of the settings.
const IN_FLIGHT_BYTES: u64 = 4 * 32 * 1024;
/// More than a streamed download can have counted, but not yet have on disk, when it stops.
const UNWRITTEN_BYTES: u64 = 256 * 1024;

struct Server {
    protocol: Protocol,
    port: u16,
    user: String,
    password: String,
}

fn server(protocol: Protocol) -> Option<Server> {
    let (port, user, password) = match protocol {
        Protocol::Sftp => (
            "POROS_TEST_SSH_PORT",
            "POROS_TEST_SSH_USER",
            "POROS_TEST_SSH_PASSWORD",
        ),
        _ => (
            "POROS_TEST_FTP_PORT",
            "POROS_TEST_FTP_USER",
            "POROS_TEST_FTP_PASSWORD",
        ),
    };
    Some(Server {
        protocol,
        port: std::env::var(port).ok()?.parse().ok()?,
        user: std::env::var(user).unwrap_or_else(|_| "poros".into()),
        password: std::env::var(password).unwrap_or_else(|_| "poros-pass".into()),
    })
}

fn profile(server: &Server) -> ConnectProfile {
    ConnectProfile {
        protocol: server.protocol,
        host: "127.0.0.1".into(),
        port: server.port,
        username: server.user.clone(),
        auth: AuthMethod::Password {
            password: server.password.clone(),
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

/// A session to the test server, trusting its host key or certificate the first time.
async fn connect(server: &Server, sessions: &SessionManager) -> SessionInfo {
    match sessions.connect(profile(server), None, OWNER).await {
        Ok(session) => session,
        Err(error) => {
            let host_key = error
                .host_key
                .expect("first connect should ask about the host key");
            let approval = HostKeyApproval::of(&host_key, true);
            sessions
                .connect(profile(server), Some(approval), OWNER)
                .await
                .unwrap()
        }
    }
}

fn pattern(length: u64, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// Monday 23 November 2026 at the given time, an hour ahead of UTC.
fn monday(hour: u32, minute: u32, second: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(3600)
        .unwrap()
        .with_ymd_and_hms(2026, 11, 23, hour, minute, second)
        .unwrap()
}

fn rule(name: &str, hours: (u32, u32), upload: RateLimit, download: RateLimit) -> LimitRule {
    LimitRule {
        id: name.to_lowercase(),
        name: name.into(),
        days: EVERY_DAY.to_vec(),
        start_minute: hours.0 * 60,
        end_minute: hours.1 * 60,
        upload,
        download,
    }
}

/// Few requests are kept in flight, so the bytes counted as transferred are the ones that
/// just left: a worker pays the limiter for a request before it joins the ones in flight, and
/// the default of 64 would have it pay for 2 MiB before the first byte moves.
fn scheduled(rules: Vec<LimitRule>) -> TransferSettings {
    TransferSettings {
        exists_action: ExistsAction::Overwrite,
        retry_attempts: 0,
        requests_in_flight: 4,
        limit_schedule: LimitSchedule {
            enabled: true,
            rules,
        },
        ..TransferSettings::default()
    }
}

struct Fixture {
    _known_hosts: tempfile::TempDir,
    local: tempfile::TempDir,
    sessions: Arc<SessionManager>,
    session: SessionInfo,
    /// Absolute remote scratch folder, removed when the test ends.
    remote: String,
    /// The time the schedule is shown.
    clock: Arc<Mutex<DateTime<FixedOffset>>>,
}

impl Fixture {
    async fn new(server: &Server, name: &str) -> Self {
        let known_hosts = tempfile::tempdir().unwrap();
        let sessions = Arc::new(SessionManager::new(
            known_hosts.path().join("known_hosts"),
            Events::default(),
        ));
        let session = connect(server, &sessions).await;
        let files = sessions.get(&session.id).await.unwrap().files();
        let scratch = format!("poros-limit-schedule-{name}-{}", std::process::id());
        let remote = format!("{}/{scratch}", session.home.trim_end_matches('/'));
        let _ = files.delete(std::slice::from_ref(&remote)).await;
        files.make_dir(&session.home, &scratch).await.unwrap();
        Self {
            _known_hosts: known_hosts,
            local: tempfile::tempdir().unwrap(),
            sessions,
            session,
            remote,
            clock: Arc::new(Mutex::new(monday(0, 0, 0))),
        }
    }

    fn set_clock(&self, now: DateTime<FixedOffset>) {
        *self.clock.lock().unwrap() = now;
    }

    /// A queue that reads the time from the fixture's clock. The schedule arrives with the
    /// settings only once it does, so the time of the test run never counts.
    fn transfers(&self, settings: TransferSettings) -> TransferManager {
        let transfers = TransferManager::new(
            self.sessions.clone(),
            Events::default(),
            TransferSettings::default(),
        );
        let clock = self.clock.clone();
        transfers.read_time_from(LimitClock::reading(move || *clock.lock().unwrap()));
        transfers.configure(settings);
        transfers
    }

    /// Queues the upload of a new local file of `size` bytes into the scratch folder.
    async fn upload(&self, transfers: &TransferManager, name: &str, size: u64) {
        let source = self.local.path().join(name);
        std::fs::write(&source, pattern(size, size)).unwrap();
        transfers
            .enqueue(EnqueueRequest {
                session_id: self.session.id.clone(),
                source_session_id: None,
                direction: Direction::Upload,
                target_directory: self.remote.clone(),
                items: vec![TransferItem {
                    path: source.to_string_lossy().into_owned(),
                    name: name.into(),
                    is_dir: false,
                    size,
                }],
            })
            .await
            .unwrap();
    }

    /// Puts a file of `size` bytes in the scratch folder, with no schedule in the way.
    async fn put(&self, name: &str, size: u64) {
        let transfers = self.transfers(TransferSettings {
            exists_action: ExistsAction::Overwrite,
            ..TransferSettings::default()
        });
        self.upload(&transfers, name, size).await;
        let list = wait_until_done(&transfers).await;
        assert_eq!(
            list.jobs[0].state,
            JobState::Done,
            "{:?}",
            list.jobs[0].error
        );
    }

    /// Queues the download of a file of the scratch folder into a local folder of its own.
    async fn download(&self, transfers: &TransferManager, name: &str, size: u64) -> PathBuf {
        let downloads = self.local.path().join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        transfers
            .enqueue(EnqueueRequest {
                session_id: self.session.id.clone(),
                source_session_id: None,
                direction: Direction::Download,
                target_directory: downloads.to_string_lossy().into_owned(),
                items: vec![TransferItem {
                    path: format!("{}/{name}", self.remote),
                    name: name.into(),
                    is_dir: false,
                    size,
                }],
            })
            .await
            .unwrap();
        downloads.join(name)
    }

    async fn remote_size(&self, name: &str) -> Option<u64> {
        let files = self.sessions.get(&self.session.id).await.unwrap().files();
        let listing = files.list_dir(&self.remote).await.unwrap();
        listing
            .entries
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.size)
    }

    async fn close(&self) {
        let files = self.sessions.get(&self.session.id).await.unwrap().files();
        files
            .delete(std::slice::from_ref(&self.remote))
            .await
            .unwrap();
        self.sessions.disconnect(&self.session.id).await.unwrap();
    }
}

/// Runs a test with a fixture and removes its folder on the server afterwards, also when the
/// test fails.
async fn with_fixture<Body, Run>(protocol: Protocol, name: &str, body: Body)
where
    Body: FnOnce(Arc<Fixture>) -> Run,
    Run: Future<Output = ()> + Send + 'static,
{
    let Some(server) = server(protocol) else {
        return;
    };
    let fixture = Arc::new(Fixture::new(&server, name).await);
    let outcome = tokio::spawn(body(fixture.clone())).await;
    fixture.close().await;
    if let Err(failure) = outcome {
        std::panic::resume_unwind(failure.into_panic());
    }
}

/// The one job of the queue.
fn only_job(transfers: &TransferManager) -> JobSnapshot {
    let mut jobs = transfers.list().jobs;
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    jobs.remove(0)
}

/// Polls until `found` gives something.
async fn wait_for<Found>(what: &str, mut found: impl FnMut() -> Option<Found>) -> Found {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(found) = found() {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_until_done(transfers: &TransferManager) -> TransferList {
    wait_for("the transfer to finish", || {
        let list = transfers.list();
        let counts = &list.stats.counts;
        (counts.queued == 0 && counts.running == 0).then_some(list)
    })
    .await
}

#[tokio::test]
async fn a_rule_limits_a_running_upload_until_the_clock_passes_its_end() {
    with_fixture(Protocol::Sftp, "limited", |fixture| async move {
        fixture.set_clock(monday(10, 0, 0));
        let limit = RateLimit::limited(LIMIT_KIB);
        let transfers =
            fixture.transfers(scheduled(vec![rule("Work hours", (9, 18), limit, limit)]));
        let limits = transfers.list().stats.limits;
        assert_eq!(
            limits.rule.map(|rule| rule.name).as_deref(),
            Some("Work hours")
        );
        assert_eq!(limits.upload, limit);
        assert_eq!(limits.until, Some(monday(18, 0, 0).timestamp()));

        let size = 6 * MEBIBYTE;
        fixture.upload(&transfers, "limited.bin", size).await;
        let moving_from = wait_for("the upload to move", || {
            let transferred = only_job(&transfers).transferred;
            (transferred > 0).then_some(transferred)
        })
        .await;
        let measured_from = Instant::now();
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let job = only_job(&transfers);
        let limited_rate =
            (job.transferred - moving_from) as f64 / measured_from.elapsed().as_secs_f64();
        assert_eq!(job.state, JobState::Running);
        assert!(
            (0.5 * LIMIT_BYTES..=1.5 * LIMIT_BYTES).contains(&limited_rate),
            "{limited_rate} bytes per second under a limit of {LIMIT_BYTES}"
        );

        // The working day ends while the file is on its way: the rest goes at full speed, on
        // the connection it already has.
        fixture.set_clock(monday(18, 0, 0));
        let lifted_at = Instant::now();
        let left = size - job.transferred;
        assert!(left > 3 * MEBIBYTE, "{left} bytes left");
        wait_for("the rest of the upload", || {
            (only_job(&transfers).transferred == size).then_some(())
        })
        .await;
        let free_rate = left as f64 / lifted_at.elapsed().as_secs_f64();
        assert!(
            free_rate > 2.0 * LIMIT_BYTES,
            "{free_rate} bytes per second once the rule ended"
        );

        let list = wait_until_done(&transfers).await;
        assert_eq!(
            list.jobs[0].state,
            JobState::Done,
            "{:?}",
            list.jobs[0].error
        );
        assert_eq!(list.jobs[0].attempts, 0);
        assert_eq!(list.stats.limits.rule, None);
        assert_eq!(list.stats.limits.upload, RateLimit::UNLIMITED);
        assert_eq!(fixture.remote_size("limited.bin").await, Some(size));
    })
    .await;
}

#[tokio::test]
async fn a_paused_window_holds_an_upload_and_lets_it_continue_where_it_stopped() {
    with_fixture(Protocol::Sftp, "paused", |fixture| async move {
        fixture.set_clock(monday(11, 59, 30));
        let quiet_hour = rule(
            "Quiet hour",
            (12, 13),
            RateLimit::PAUSED,
            RateLimit::UNLIMITED,
        );
        // Slow at all times, so the file is still on its way when the window begins and what
        // is left of it afterwards takes long enough to watch.
        let settings = TransferSettings {
            upload_limit_kib: LIMIT_KIB,
            ..scheduled(vec![quiet_hour])
        };
        let transfers = fixture.transfers(settings);
        let size = 3 * MEBIBYTE;
        fixture.upload(&transfers, "paused.bin", size).await;
        let before_noon = wait_for("a good part of the upload", || {
            let transferred = only_job(&transfers).transferred;
            (transferred >= MEBIBYTE).then_some(transferred)
        })
        .await;

        fixture.set_clock(monday(12, 0, 0));
        let held = wait_for("the upload to wait for the schedule", || {
            let list = transfers.list();
            let waiting = list.jobs[0].state == JobState::Queued && list.stats.counts.running == 0;
            waiting.then_some(list)
        })
        .await;
        let limits = &held.stats.limits;
        assert!(limits.upload.is_paused());
        assert_eq!(limits.download, RateLimit::UNLIMITED);
        assert_eq!(
            limits.rule.as_ref().map(|rule| rule.name.as_str()),
            Some("Quiet hour")
        );
        assert_eq!(limits.until, Some(monday(13, 0, 0).timestamp()));
        assert_eq!(limits.pause_ends.upload, Some(monday(13, 0, 0).timestamp()));
        assert_eq!(limits.pause_ends.download, None);
        let id = held.jobs[0].id;
        let kept = held.jobs[0].transferred;
        // What was sent stays on the server, under the temporary name, while the window lasts.
        let partial = format!("{}/.paused.bin.poros-part", fixture.remote);
        let session = fixture.sessions.get(&fixture.session.id).await.unwrap();
        let sent = session
            .sftp()
            .unwrap()
            .read_range(&partial, 0, kept as u32)
            .await
            .unwrap();
        assert!(sent[..] == pattern(size, size)[..kept as usize]);
        assert!(
            (before_noon..size).contains(&kept),
            "{kept} bytes when held, after {before_noon} were seen moving"
        );

        // A queue that only waits for the schedule has not finished, and nothing moves.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let list = transfers.list();
        assert_eq!(list.stats.counts.queued, 1);
        assert_eq!(list.stats.counts.done + list.stats.counts.failed, 0);
        assert_eq!(list.jobs[0].transferred, kept);
        assert_eq!(list.stats.remaining_bytes, size - kept);

        // Paused by hand and continued, and the whole queue paused and run again: the window
        // still holds it.
        transfers.pause(&[id]);
        assert_eq!(only_job(&transfers).state, JobState::Paused);
        transfers.resume(&[id]);
        transfers.set_paused(true);
        transfers.set_paused(false);
        tokio::time::sleep(Duration::from_millis(700)).await;
        let job = only_job(&transfers);
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.transferred, kept);

        // The window ends: the upload goes on from what the server already has.
        fixture.set_clock(monday(13, 0, 0));
        let mut least_after_release = u64::MAX;
        let list = wait_for("the upload to finish", || {
            let list = transfers.list();
            let job = &list.jobs[0];
            if job.state == JobState::Running {
                least_after_release = least_after_release.min(job.transferred);
            }
            (list.stats.counts.queued == 0 && list.stats.counts.running == 0).then_some(list)
        })
        .await;
        assert_eq!(
            list.jobs[0].state,
            JobState::Done,
            "{:?}",
            list.jobs[0].error
        );
        assert_eq!(list.jobs[0].attempts, 0);
        // It continues from what the server has, short of at most the requests in flight.
        assert!(least_after_release < size);
        assert!(
            kept.saturating_sub(least_after_release) <= IN_FLIGHT_BYTES,
            "continued from {least_after_release} after being held at {kept}"
        );
        assert_eq!(list.stats.limits.rule, None);
        assert_eq!(list.stats.limits.upload, RateLimit::limited(LIMIT_KIB));
        assert_eq!(fixture.remote_size("paused.bin").await, Some(size));
    })
    .await;
}

#[tokio::test]
async fn ignoring_the_schedule_lets_a_held_upload_go() {
    with_fixture(Protocol::Sftp, "ignored", |fixture| async move {
        fixture.set_clock(monday(12, 30, 0));
        let quiet_hour = rule("Quiet hour", (12, 13), RateLimit::PAUSED, RateLimit::PAUSED);
        let transfers = fixture.transfers(scheduled(vec![quiet_hour]));
        let size = MEBIBYTE / 4;
        fixture.upload(&transfers, "ignored.bin", size).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let job = only_job(&transfers);
        assert_eq!((job.state, job.transferred), (JobState::Queued, 0));
        assert_eq!(fixture.remote_size("ignored.bin").await, None);

        transfers.ignore_limit_schedule(true);
        let list = wait_until_done(&transfers).await;
        assert_eq!(
            list.jobs[0].state,
            JobState::Done,
            "{:?}",
            list.jobs[0].error
        );
        let limits = &list.stats.limits;
        assert!(limits.ignored);
        assert_eq!(limits.upload, RateLimit::UNLIMITED);
        assert_eq!(limits.until, Some(monday(13, 0, 0).timestamp()));
        assert_eq!(fixture.remote_size("ignored.bin").await, Some(size));

        // The window's end is the next change, where the schedule is followed again.
        fixture.set_clock(monday(13, 0, 0));
        let limits = wait_for("the schedule to be followed again", || {
            let limits = transfers.list().stats.limits;
            (!limits.ignored).then_some(limits)
        })
        .await;
        assert_eq!(limits.rule, None);
    })
    .await;
}

/// The streamed path every protocol but SFTP takes, through a download from an FTP server.
#[tokio::test]
async fn a_download_over_ftp_is_slowed_then_held_and_continues() {
    with_fixture(Protocol::Ftp, "ftp", |fixture| async move {
        let size = 4 * MEBIBYTE;
        fixture.put("streamed.bin", size).await;
        fixture.set_clock(monday(11, 0, 0));
        let limit = RateLimit::limited(LIMIT_KIB);
        let transfers = fixture.transfers(scheduled(vec![
            rule("Work hours", (9, 12), RateLimit::UNLIMITED, limit),
            rule(
                "Quiet hour",
                (12, 13),
                RateLimit::UNLIMITED,
                RateLimit::PAUSED,
            ),
        ]));
        let target = fixture.download(&transfers, "streamed.bin", size).await;
        let moving_from = wait_for("the download to move", || {
            let transferred = only_job(&transfers).transferred;
            (transferred > 0).then_some(transferred)
        })
        .await;
        let measured_from = Instant::now();
        tokio::time::sleep(Duration::from_millis(2000)).await;
        let job = only_job(&transfers);
        let before_noon = job.transferred;
        let limited_rate =
            (before_noon - moving_from) as f64 / measured_from.elapsed().as_secs_f64();
        assert!(
            (0.5 * LIMIT_BYTES..=1.5 * LIMIT_BYTES).contains(&limited_rate),
            "{limited_rate} bytes per second under a limit of {LIMIT_BYTES}"
        );

        // Noon: downloads wait for the schedule, keeping what they have. How much that is
        // depends on how fast the runner went within the limit, so it is compared with what
        // was seen moving rather than with a fixed amount.
        fixture.set_clock(monday(12, 0, 0));
        let held = wait_for("the download to wait for the schedule", || {
            let list = transfers.list();
            let waiting = list.jobs[0].state == JobState::Queued && list.stats.counts.running == 0;
            waiting.then_some(list)
        })
        .await;
        assert!(held.stats.limits.download.is_paused());
        assert_eq!(
            held.stats.limits.pause_ends.download,
            Some(monday(13, 0, 0).timestamp())
        );
        let kept = held.jobs[0].transferred;
        assert!(
            (before_noon..size).contains(&kept),
            "{kept} bytes when held, after {before_noon} were seen moving"
        );
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(only_job(&transfers).transferred, kept);

        // One o'clock: no rule, no limit, and the download goes on from where it was.
        fixture.set_clock(monday(13, 0, 0));
        let mut least_after_release = u64::MAX;
        let list = wait_for("the download to finish", || {
            let list = transfers.list();
            let job = &list.jobs[0];
            if job.state == JobState::Running {
                least_after_release = least_after_release.min(job.transferred);
            }
            (list.stats.counts.queued == 0 && list.stats.counts.running == 0).then_some(list)
        })
        .await;
        assert_eq!(
            list.jobs[0].state,
            JobState::Done,
            "{:?}",
            list.jobs[0].error
        );
        assert_eq!(list.jobs[0].attempts, 0);
        // It continues from what the file holds, short of at most a write still on its way to
        // the disk when the hold stopped it. What was kept is more than that, so this tells a
        // continued download from one that started again.
        assert!(kept > UNWRITTEN_BYTES, "{kept} bytes when held");
        assert!(
            kept.saturating_sub(least_after_release) <= UNWRITTEN_BYTES,
            "continued from {least_after_release} after being held at {kept}"
        );
        assert!(std::fs::read(&target).unwrap() == pattern(size, size));
    })
    .await;
}
