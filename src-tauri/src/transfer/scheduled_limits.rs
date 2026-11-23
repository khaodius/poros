//! Keeps the queue's speed limits in step with the transfer settings and their schedule: the
//! rate of the two limiters, which directions wait, and what the windows are told about it.
//! A changed rate reaches running transfers through their limiter, as one changed in the
//! settings always has, so nothing restarts when a rule begins or ends.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone};
use serde::Serialize;

use super::limit_schedule::{clock_time, LimitSchedule, Limits, PauseEnds, RateLimit, Standing};
use super::queue::Held;
use super::Shared;
use crate::events::LogLevel;

/// The schedule is looked at again at least this often, which notices a changed clock or time
/// zone and a wake from sleep, as the tick of the task scheduler does.
const RECHECK_SECS: i64 = 30;

/// Works out what the schedule puts in force at the time a clock shows, in its time zone.
type StandingAtNow = dyn Fn(&LimitSchedule, Limits) -> Standing + Send + Sync;

/// Where the schedule reads the time. The app reads the system's clock; tests set their own.
pub struct LimitClock {
    timestamp: Box<dyn Fn() -> i64 + Send + Sync>,
    standing: Box<StandingAtNow>,
}

impl LimitClock {
    pub fn system() -> Self {
        Self::reading(Local::now)
    }

    /// A clock showing the time `now` returns, in the time zone it returns it in.
    pub fn reading<Zone, Now>(now: Now) -> Self
    where
        Zone: TimeZone + 'static,
        Now: Fn() -> DateTime<Zone> + Send + Sync + 'static,
    {
        let now = Arc::new(now);
        let read = now.clone();
        Self {
            timestamp: Box::new(move || read().timestamp()),
            standing: Box::new(move |schedule, ordinary| schedule.standing(ordinary, &now())),
        }
    }
}

/// The limits in force and what set them, as the windows show it. Mirrored in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitStatus {
    pub upload: RateLimit,
    pub download: RateLimit,
    /// The rule of the schedule that applies now. While the schedule is ignored, the rule
    /// that would.
    pub rule: Option<ActiveRule>,
    /// When the schedule next changes what is in force, which is also when it stops being
    /// ignored, in seconds since the Unix epoch.
    pub until: Option<i64>,
    /// The user set the schedule aside until then.
    pub ignored: bool,
    /// When transfers waiting for the schedule continue, which can be later than `until` when
    /// one pause follows another.
    pub pause_ends: PauseEnds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRule {
    /// Its place in the list of rules.
    pub index: usize,
    pub name: String,
}

/// The schedule set aside by the user. It is never saved, so a restart follows the schedule.
#[derive(Debug, Clone, Copy)]
struct Ignored {
    /// `None` when the schedule changes nothing within the coming week; it then stays ignored
    /// until it is taken up again or its rules change.
    until: Option<DateTime<FixedOffset>>,
}

struct State {
    clock: LimitClock,
    /// The schedule as last applied, to tell when the settings change it.
    schedule: LimitSchedule,
    ignored: Option<Ignored>,
    status: LimitStatus,
    /// When the status was worked out, by the clock.
    checked_at: i64,
    /// When the schedule itself next changes what is in force.
    next_change: Option<i64>,
}

pub(super) struct ScheduledLimits {
    state: Mutex<State>,
}

impl ScheduledLimits {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                clock: LimitClock::system(),
                schedule: LimitSchedule::default(),
                ignored: None,
                status: LimitStatus::default(),
                checked_at: 0,
                next_change: None,
            }),
        }
    }

    pub fn status(&self) -> LimitStatus {
        self.state.lock().unwrap().status.clone()
    }
}

fn ordinary_limits(shared: &Shared) -> Limits {
    let settings = shared.settings.read().unwrap();
    Limits::ordinary(settings.upload_limit_kib, settings.download_limit_kib)
}

/// Works out what is in force now and has the limiters and the queue follow it. The queue is
/// locked while the state is, never the other way round.
pub(super) fn apply(shared: &Shared) {
    let schedule = shared.settings.read().unwrap().limit_schedule.clone();
    let ordinary = ordinary_limits(shared);
    let mut state = shared.scheduled_limits.state.lock().unwrap();
    if state.schedule != schedule {
        // "Until the next change" was said of the rules as they were.
        state.ignored = None;
        state.schedule = schedule;
    }
    let standing = (state.clock.standing)(&state.schedule, ordinary);
    let ignoring_ended = state.ignored.is_some_and(|ignored| {
        ignored
            .until
            .is_some_and(|until| standing.at >= until.timestamp())
    });
    if ignoring_ended {
        state.ignored = None;
    }

    let limits = match state.ignored {
        Some(_) => ordinary,
        None => standing.in_force.limits,
    };
    let until = match state.ignored {
        Some(ignored) => ignored.until,
        None => standing.next_change,
    };
    let status = LimitStatus {
        upload: limits.upload,
        download: limits.download,
        rule: standing.in_force.rule.map(|index| ActiveRule {
            index,
            name: state.schedule.rules[index].name.clone(),
        }),
        until: until.map(|until| until.timestamp()),
        ignored: state.ignored.is_some(),
        pause_ends: match state.ignored {
            Some(_) => PauseEnds::default(),
            None => standing.pause_ends,
        },
    };
    let news = announcement(
        &state.status,
        &status,
        &state.schedule,
        until,
        &standing.local,
    );
    state.status = status;
    state.checked_at = standing.at;
    state.next_change = standing.next_change.map(|change| change.timestamp());

    let (upload_rate, download_rate) = limiter_rates(limits, ordinary);
    shared.upload_limiter.set_rate(upload_rate);
    shared.download_limiter.set_rate(download_rate);
    let held_changed = shared.queue.lock().unwrap().set_held(Held {
        upload: limits.upload.is_paused(),
        download: limits.download.is_paused(),
    });
    drop(state);

    if held_changed {
        shared.work.notify_waiters();
    }
    if let Some(news) = news {
        shared.events.log(LogLevel::Info, None, news);
    }
}

/// The bytes per second of the upload and download limiters, zero for none. Transfers the other
/// way may read or write a little in a paused direction, as an upload read back to check it
/// does. That goes at the ordinary limit; the direction's own transfers are held in the queue.
fn limiter_rates(limits: Limits, ordinary: Limits) -> (u64, u64) {
    let rate = |limit: RateLimit, ordinary: RateLimit| {
        if limit.is_paused() {
            ordinary.bytes_per_second()
        } else {
            limit.bytes_per_second()
        }
    };
    (
        rate(limits.upload, ordinary.upload),
        rate(limits.download, ordinary.download),
    )
}

/// Called a few times a second. Applies the schedule again once its next change is due, the
/// clock went back, or it has not been looked at for a while.
pub(super) fn follow(shared: &Shared) {
    let due = {
        let state = shared.scheduled_limits.state.lock().unwrap();
        state.schedule.enabled && {
            let now = (state.clock.timestamp)();
            now < state.checked_at
                || now - state.checked_at >= RECHECK_SECS
                || state.next_change.is_some_and(|change| now >= change)
        }
    };
    if due {
        apply(shared);
    }
}

/// Sets the schedule aside until it next changes what is in force, or takes it up again.
pub(super) fn set_ignored(shared: &Shared, ignored: bool) {
    let ordinary = ordinary_limits(shared);
    {
        let mut state = shared.scheduled_limits.state.lock().unwrap();
        state.ignored = if ignored {
            let standing = (state.clock.standing)(&state.schedule, ordinary);
            // With no rule in force there is nothing to set aside.
            standing.in_force.rule.map(|_| Ignored {
                until: standing.next_change,
            })
        } else {
            None
        };
    }
    apply(shared);
}

pub(super) fn set_clock(shared: &Shared, clock: LimitClock) {
    shared.scheduled_limits.state.lock().unwrap().clock = clock;
    apply(shared);
}

/// A line for the log when the schedule changes what is in force; `None` when it did not.
fn announcement(
    before: &LimitStatus,
    now: &LimitStatus,
    schedule: &LimitSchedule,
    until: Option<DateTime<FixedOffset>>,
    local: &NaiveDateTime,
) -> Option<String> {
    // The ordinary limits changing in the settings is no news of the schedule.
    let schedule_part = |status: &LimitStatus| {
        let set_by_rule = status.rule.is_some() && !status.ignored;
        (
            status.ignored,
            status.rule.clone(),
            set_by_rule.then_some((status.upload, status.download)),
        )
    };
    if schedule_part(before) == schedule_part(now) {
        return None;
    }
    let until = until
        .map(|until| format!(" until {}", clock_time(&until.naive_local(), local)))
        .unwrap_or_default();
    if now.ignored {
        return Some(format!("Ignoring the speed limit schedule{until}"));
    }
    Some(match &now.rule {
        Some(rule) => format!(
            "Speed limit schedule: \"{}\" applies{until}, uploads {}, downloads {}",
            schedule.rules[rule.index].label(rule.index),
            now.upload.describe(),
            now.download.describe(),
        ),
        None => "Speed limit schedule: no rule applies now, so the ordinary limits do".into(),
    })
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::events::Events;
    use crate::session::SessionManager;
    use crate::settings::TransferSettings;
    use crate::transfer::limit_schedule::LimitRule;
    use crate::transfer::TransferManager;

    const KIBIBYTE: u64 = 1024;
    const WORK_HOURS_APPLY: &str = "Speed limit schedule: \"Work hours\" applies until 18:00, uploads 200 KiB/s, downloads 500 KiB/s";
    const NO_RULE_APPLIES: &str =
        "Speed limit schedule: no rule applies now, so the ordinary limits do";

    struct Fixture {
        _folder: tempfile::TempDir,
        manager: TransferManager,
        events: Events,
        clock: Arc<Mutex<DateTime<FixedOffset>>>,
    }

    /// Monday 23 November 2026 at the given time, an hour ahead of UTC.
    fn monday(hour: u32, minute: u32, second: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, 23, hour, minute, second)
            .unwrap()
    }

    fn rule(
        name: &str,
        days: &[u8],
        hours: (u32, u32),
        limits: (RateLimit, RateLimit),
    ) -> LimitRule {
        LimitRule {
            id: name.to_lowercase(),
            name: name.into(),
            days: days.to_vec(),
            start_minute: hours.0 * 60,
            end_minute: hours.1 * 60,
            upload: limits.0,
            download: limits.1,
        }
    }

    /// Work hours on weekdays, then an hour every evening with uploads paused.
    fn settings() -> TransferSettings {
        TransferSettings {
            upload_limit_kib: 0,
            download_limit_kib: 2000,
            limit_schedule: LimitSchedule {
                enabled: true,
                rules: vec![
                    rule(
                        "Work hours",
                        &[0, 1, 2, 3, 4],
                        (9, 18),
                        (RateLimit::limited(200), RateLimit::limited(500)),
                    ),
                    rule(
                        "",
                        &[0, 1, 2, 3, 4, 5, 6],
                        (20, 21),
                        (RateLimit::PAUSED, RateLimit::UNLIMITED),
                    ),
                ],
            },
            ..TransferSettings::default()
        }
    }

    impl Fixture {
        fn new(now: DateTime<FixedOffset>) -> Self {
            let folder = tempfile::tempdir().unwrap();
            let events = Events::keeping_startup_log();
            let sessions = Arc::new(SessionManager::new(
                folder.path().join("known_hosts"),
                events.clone(),
            ));
            // The schedule arrives with the settings once the clock is the test's own, so
            // nothing is worked out from the time of the test run.
            let manager =
                TransferManager::new(sessions, events.clone(), TransferSettings::default());
            let clock = Arc::new(Mutex::new(now));
            let shown = clock.clone();
            manager.read_time_from(LimitClock::reading(move || *shown.lock().unwrap()));
            manager.configure(settings());
            Self {
                _folder: folder,
                manager,
                events,
                clock,
            }
        }

        /// What the schedule wrote to the log so far.
        fn log(&self) -> Vec<String> {
            self.events
                .take_startup_log()
                .into_iter()
                .map(|record| record.message)
                .collect()
        }

        /// Moves the clock and lets the manager's tick see it.
        fn at(&self, now: DateTime<FixedOffset>) -> LimitStatus {
            *self.clock.lock().unwrap() = now;
            follow(&self.manager.shared);
            self.manager.list().stats.limits
        }

        /// The rates the limiters are given for what is in force now.
        fn rates(&self) -> (u64, u64) {
            let status = self.manager.list().stats.limits;
            let in_force = Limits {
                upload: status.upload,
                download: status.download,
            };
            limiter_rates(in_force, ordinary_limits(&self.manager.shared))
        }

        fn held(&self) -> Held {
            self.manager.shared.queue.lock().unwrap().held()
        }
    }

    #[test]
    fn limits_follow_the_schedule_as_the_clock_moves() {
        let fixture = Fixture::new(monday(8, 59, 50));
        let status = fixture.manager.list().stats.limits;
        assert_eq!(status.rule, None);
        assert_eq!(status.upload, RateLimit::UNLIMITED);
        assert_eq!(status.download, RateLimit::limited(2000));
        assert_eq!(status.until, Some(monday(9, 0, 0).timestamp()));
        assert_eq!(fixture.rates(), (0, 2000 * KIBIBYTE));

        let status = fixture.at(monday(9, 0, 0));
        assert_eq!(
            status.rule,
            Some(ActiveRule {
                index: 0,
                name: "Work hours".into()
            })
        );
        assert_eq!(status.upload, RateLimit::limited(200));
        assert_eq!(status.download, RateLimit::limited(500));
        assert_eq!(status.until, Some(monday(18, 0, 0).timestamp()));
        assert!(!status.ignored);
        assert_eq!(fixture.rates(), (200 * KIBIBYTE, 500 * KIBIBYTE));
        assert_eq!(fixture.held(), Held::default());

        // Nothing is due a few seconds later, so the tick leaves it alone.
        let status = fixture.at(monday(9, 0, 5));
        assert_eq!(status.until, Some(monday(18, 0, 0).timestamp()));

        let status = fixture.at(monday(18, 0, 0));
        assert_eq!(status.rule, None);
        assert_eq!(fixture.rates(), (0, 2000 * KIBIBYTE));

        // Uploads pause. What else needs the upload direction goes at its ordinary limit.
        let status = fixture.at(monday(20, 0, 0));
        assert_eq!(status.upload, RateLimit::PAUSED);
        assert_eq!(status.download, RateLimit::UNLIMITED);
        assert_eq!(status.until, Some(monday(21, 0, 0).timestamp()));
        let nine = Some(monday(21, 0, 0).timestamp());
        assert_eq!(
            status.pause_ends,
            PauseEnds {
                upload: nine,
                download: None,
                relay: nine,
            }
        );
        assert_eq!(fixture.rates(), (0, 0));
        assert_eq!(
            fixture.held(),
            Held {
                upload: true,
                download: false
            }
        );
        let status = fixture.at(monday(21, 0, 0));
        assert_eq!(status.upload, RateLimit::UNLIMITED);
        assert_eq!(fixture.held(), Held::default());

        assert_eq!(
            fixture.log(),
            [
                WORK_HOURS_APPLY,
                NO_RULE_APPLIES,
                "Speed limit schedule: \"Rule 2\" applies until 21:00, uploads paused, downloads unlimited",
                NO_RULE_APPLIES,
            ]
        );
    }

    #[test]
    fn a_clock_set_back_or_far_ahead_is_noticed() {
        let fixture = Fixture::new(monday(9, 30, 0));
        assert!(fixture.manager.list().stats.limits.rule.is_some());
        // Back before the rule began.
        let status = fixture.at(monday(8, 0, 0));
        assert_eq!(status.rule, None);
        assert_eq!(status.until, Some(monday(9, 0, 0).timestamp()));
        // Asleep through the working day and the evening: awake on Tuesday morning.
        let status = fixture.at(monday(8, 30, 0) + Duration::days(1));
        assert_eq!(status.rule, None);
        assert_eq!(
            status.until,
            Some((monday(9, 0, 0) + Duration::days(1)).timestamp())
        );
        // A time zone changed under a rule: the same moment shows as another time of day.
        let status = fixture.at(monday(9, 30, 0) + Duration::days(1));
        assert!(status.rule.is_some());
        // In Honolulu it is still Monday, half past ten at night.
        let honolulu = FixedOffset::west_opt(10 * 3600).unwrap();
        let moved = (monday(9, 30, 10) + Duration::days(1)).with_timezone(&honolulu);
        *fixture.clock.lock().unwrap() = moved;
        follow(&fixture.manager.shared);
        // Within the half minute between two looks, nothing is due yet.
        assert!(fixture.manager.list().stats.limits.rule.is_some());
        let status = fixture.at(moved + Duration::seconds(RECHECK_SECS));
        assert_eq!(status.rule, None);
    }

    #[test]
    fn ignoring_the_schedule_lasts_until_its_next_change() {
        let fixture = Fixture::new(monday(10, 0, 0));
        let manager = &fixture.manager;
        manager.ignore_limit_schedule(true);
        let status = manager.list().stats.limits;
        assert!(status.ignored);
        assert_eq!(status.rule.as_ref().map(|rule| rule.index), Some(0));
        assert_eq!(status.upload, RateLimit::UNLIMITED);
        assert_eq!(status.download, RateLimit::limited(2000));
        assert_eq!(status.until, Some(monday(18, 0, 0).timestamp()));
        assert_eq!(fixture.rates(), (0, 2000 * KIBIBYTE));

        // Other settings changing leaves it ignored; so does time passing within the rule.
        manager.configure(TransferSettings {
            workers: 2,
            download_limit_kib: 3000,
            ..settings()
        });
        let status = fixture.at(monday(17, 59, 59));
        assert!(status.ignored);
        assert_eq!(status.download, RateLimit::limited(3000));

        // The rule ends, and with it the ignoring: the evening's pause applies as usual.
        let status = fixture.at(monday(18, 0, 0));
        assert!(!status.ignored);
        assert_eq!(status.rule, None);
        let status = fixture.at(monday(20, 0, 0));
        assert!(!status.ignored);
        assert_eq!(status.upload, RateLimit::PAUSED);
        assert!(fixture.held().upload);

        // Ignoring a pause lets its transfers go, and following it again holds them.
        manager.ignore_limit_schedule(true);
        let status = manager.list().stats.limits;
        assert!(status.ignored);
        assert_eq!(status.pause_ends, PauseEnds::default());
        assert!(!fixture.held().upload);
        manager.ignore_limit_schedule(false);
        assert!(!manager.list().stats.limits.ignored);
        assert!(fixture.held().upload);

        // Changed rules are a change too.
        manager.ignore_limit_schedule(true);
        let mut edited = settings();
        edited.limit_schedule.rules[1].end_minute = 22 * 60;
        manager.configure(edited);
        let status = manager.list().stats.limits;
        assert!(!status.ignored);
        assert_eq!(status.until, Some(monday(22, 0, 0).timestamp()));

        // With no rule in force there is nothing to ignore.
        fixture.at(monday(23, 0, 0));
        manager.ignore_limit_schedule(true);
        assert!(!manager.list().stats.limits.ignored);

        let evening_pause = |until: &str| {
            format!(
                "Speed limit schedule: \"Rule 2\" applies until {until}, uploads paused, downloads unlimited"
            )
        };
        assert_eq!(
            fixture.log(),
            [
                WORK_HOURS_APPLY.to_string(),
                "Ignoring the speed limit schedule until 18:00".into(),
                NO_RULE_APPLIES.into(),
                evening_pause("21:00"),
                "Ignoring the speed limit schedule until 21:00".into(),
                evening_pause("21:00"),
                "Ignoring the speed limit schedule until 21:00".into(),
                evening_pause("22:00"),
                NO_RULE_APPLIES.into(),
            ]
        );
    }

    #[test]
    fn a_schedule_switched_off_leaves_the_ordinary_limits() {
        let fixture = Fixture::new(monday(10, 0, 0));
        let mut off = settings();
        off.limit_schedule.enabled = false;
        off.upload_limit_kib = 64;
        fixture.manager.configure(off);
        let status = fixture.at(monday(10, 0, 40));
        assert_eq!(
            status,
            LimitStatus {
                upload: RateLimit::limited(64),
                download: RateLimit::limited(2000),
                rule: None,
                until: None,
                ignored: false,
                pause_ends: PauseEnds::default(),
            }
        );
        assert_eq!(fixture.rates(), (64 * KIBIBYTE, 2000 * KIBIBYTE));
    }

    #[test]
    fn a_paused_direction_keeps_its_ordinary_rate_for_what_else_needs_it() {
        let ordinary = Limits::ordinary(300, 0);
        let paused = Limits {
            upload: RateLimit::PAUSED,
            download: RateLimit::limited(500),
        };
        assert_eq!(
            limiter_rates(paused, ordinary),
            (300 * KIBIBYTE, 500 * KIBIBYTE)
        );
        let lifted = Limits {
            upload: RateLimit::UNLIMITED,
            download: RateLimit::PAUSED,
        };
        assert_eq!(limiter_rates(lifted, ordinary), (0, 0));
    }

    /// How long `acquire` takes to let `bytes` through in chunks the size of an SFTP request.
    async fn time_to_send(limiter: &crate::transfer::limiter::RateLimiter, bytes: u64) -> f64 {
        const CHUNK: u64 = 32 * KIBIBYTE;
        let started = std::time::Instant::now();
        for _ in 0..bytes / CHUNK {
            limiter.acquire(CHUNK).await;
        }
        started.elapsed().as_secs_f64()
    }

    #[tokio::test]
    async fn the_limiter_of_a_running_transfer_follows_each_boundary() {
        let fixture = Fixture::new(monday(8, 59, 59));
        let limiter = &fixture.manager.shared.upload_limiter;
        // Before nine nothing limits uploads: eight MiB go at once.
        let free = time_to_send(limiter, 8 * 1024 * KIBIBYTE).await;
        assert!(free < 0.25, "{free} s unlimited");

        // The working day begins while the transfer goes on: 200 KiB/s, so 384 KiB take up to
        // two seconds, less what the bucket held when the limit came (at most 256 KiB).
        fixture.at(monday(9, 0, 0));
        let limited = time_to_send(limiter, 384 * KIBIBYTE).await;
        assert!((0.5..=2.5).contains(&limited), "{limited} s at 200 KiB/s");

        // And ends: the same transfer is free again from its next request on.
        fixture.at(monday(18, 0, 0));
        let lifted = time_to_send(limiter, 8 * 1024 * KIBIBYTE).await;
        assert!(lifted < 0.4, "{lifted} s once lifted");
    }
}
