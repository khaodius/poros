//! Speed limits by time of day. The rules are kept with the transfer settings; what they add
//! up to at a local date and time is worked out here by pure functions, so it is tested with
//! fixed clocks and time zones. Mirrored in `src/lib/limitSchedule.ts` as far as the week view
//! in the settings needs, and both are tested against
//! `tests/fixtures/limit-schedule-cases.json`.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDateTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

use crate::automation::schedule::MINUTES_PER_DAY;

pub const MAX_RULES: usize = 50;
/// The most a rule limits to, as the ordinary limits in the settings take.
pub const MAX_LIMIT_KIB: u32 = 10 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 60;
const MAX_ID_CHARS: usize = 64;
const DAYS_PER_WEEK: u8 = 7;
/// How far ahead the next change is looked for: every weekday comes round, and a day more.
const SEARCH_DAYS: i64 = 8;
const WEEKDAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitMode {
    Limited,
    /// No transfer starts or continues in that direction.
    Paused,
    /// Also what a mode this version does not know reads as.
    #[default]
    #[serde(other)]
    Unlimited,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RateLimit {
    pub mode: LimitMode,
    /// KiB per second. A rule keeps the number while its mode is another, so switching back
    /// to a limit finds it again.
    pub kib: u32,
}

impl RateLimit {
    pub const UNLIMITED: Self = Self {
        mode: LimitMode::Unlimited,
        kib: 0,
    };
    pub const PAUSED: Self = Self {
        mode: LimitMode::Paused,
        kib: 0,
    };

    /// Zero is unlimited, as in the ordinary limits.
    pub fn limited(kib: u32) -> Self {
        if kib == 0 {
            Self::UNLIMITED
        } else {
            Self {
                mode: LimitMode::Limited,
                kib,
            }
        }
    }

    pub fn is_paused(self) -> bool {
        self.mode == LimitMode::Paused
    }

    /// Zero where there is no limit to keep to.
    pub fn bytes_per_second(self) -> u64 {
        match self.mode {
            LimitMode::Limited => u64::from(self.kib) * 1024,
            LimitMode::Unlimited | LimitMode::Paused => 0,
        }
    }

    fn sanitize(&mut self) {
        self.kib = self.kib.min(MAX_LIMIT_KIB);
        if self.mode == LimitMode::Limited && self.kib == 0 {
            self.mode = LimitMode::Unlimited;
        }
    }

    /// As it applies: only a limit carries a number.
    fn in_force(self) -> Self {
        match self.mode {
            LimitMode::Limited => Self::limited(self.kib),
            LimitMode::Unlimited => Self::UNLIMITED,
            LimitMode::Paused => Self::PAUSED,
        }
    }

    /// `500 KiB/s`, `unlimited` or `paused`.
    pub fn describe(self) -> String {
        match self.mode {
            LimitMode::Limited => format!("{} KiB/s", self.kib),
            LimitMode::Unlimited => "unlimited".into(),
            LimitMode::Paused => "paused".into(),
        }
    }
}

/// The limit of each direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    pub upload: RateLimit,
    pub download: RateLimit,
}

impl Limits {
    /// The two limits of the transfer settings, in KiB per second with zero for unlimited.
    pub fn ordinary(upload_kib: u32, download_kib: u32) -> Self {
        Self {
            upload: RateLimit::limited(upload_kib),
            download: RateLimit::limited(download_kib),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LimitRule {
    /// Tells the rules apart while they are edited and reordered.
    pub id: String,
    pub name: String,
    /// The weekdays the rule starts on, Monday being 0.
    pub days: Vec<u8>,
    /// Minutes after midnight, local time.
    pub start_minute: u32,
    /// At or before the start, the rule runs past midnight into the next day; the same as the
    /// start, for a whole day.
    pub end_minute: u32,
    pub upload: RateLimit,
    pub download: RateLimit,
}

impl LimitRule {
    /// The weekday is the one the clock shows, which for the part of a rule after midnight
    /// is the day after the one it started on.
    fn covers(&self, weekday: u8, minute: u32) -> bool {
        let starts_today = self.days.contains(&weekday);
        if self.start_minute < self.end_minute {
            return starts_today && (self.start_minute..self.end_minute).contains(&minute);
        }
        let started_yesterday = self
            .days
            .contains(&((weekday + DAYS_PER_WEEK - 1) % DAYS_PER_WEEK));
        (starts_today && minute >= self.start_minute)
            || (started_yesterday && minute < self.end_minute)
    }

    /// The rule's name, or its place in the list when it has none.
    pub fn label(&self, index: usize) -> String {
        if self.name.is_empty() {
            format!("Rule {}", index + 1)
        } else {
            self.name.clone()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LimitSchedule {
    pub enabled: bool,
    /// Where several cover the same time, the one latest in the list applies.
    pub rules: Vec<LimitRule>,
}

/// What applies at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InForce {
    /// The rule that applies, by its place in the list; `None` leaves the ordinary limits.
    pub rule: Option<usize>,
    pub limits: Limits,
}

/// When the pause in force ends for the transfers of each direction, in seconds since the Unix
/// epoch. `None` where nothing is paused, or the pause lasts beyond the coming week.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PauseEnds {
    pub upload: Option<i64>,
    pub download: Option<i64>,
    /// A copy between two servers waits until neither direction is paused.
    pub relay: Option<i64>,
}

/// What applies now and for how long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// When this was worked out, in seconds since the Unix epoch.
    pub at: i64,
    /// The local time then.
    pub local: NaiveDateTime,
    pub in_force: InForce,
    /// When something else comes into force, as the clock will show it then.
    pub next_change: Option<DateTime<FixedOffset>>,
    pub pause_ends: PauseEnds,
}

impl LimitSchedule {
    /// Makes stored rules safe to use: known days, times within a day, limits within range,
    /// one-line names and an id of its own for each.
    pub fn sanitize(&mut self) {
        self.rules.truncate(MAX_RULES);
        let mut seen: Vec<String> = Vec::with_capacity(self.rules.len());
        for (index, rule) in self.rules.iter_mut().enumerate() {
            rule.name = rule
                .name
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .filter(|character| !character.is_control())
                .collect::<String>()
                .trim()
                .chars()
                .take(MAX_NAME_CHARS)
                .collect();
            rule.name.truncate(rule.name.trim_end().len());
            rule.days.retain(|day| *day < DAYS_PER_WEEK);
            rule.days.sort_unstable();
            rule.days.dedup();
            rule.start_minute = rule.start_minute.min(MINUTES_PER_DAY - 1);
            rule.end_minute = rule.end_minute.min(MINUTES_PER_DAY - 1);
            rule.upload.sanitize();
            rule.download.sanitize();
            let mut id: String = rule.id.trim().chars().take(MAX_ID_CHARS).collect();
            id.truncate(id.trim_end().len());
            if id.is_empty() {
                id = format!("rule-{}", index + 1);
            }
            let base = id.clone();
            let mut number = index + 1;
            while seen.contains(&id) {
                let suffix = format!("-{number}");
                id = base
                    .chars()
                    .take(MAX_ID_CHARS - suffix.len())
                    .chain(suffix.chars())
                    .collect();
                number += 1;
            }
            seen.push(id.clone());
            rule.id = id;
        }
    }

    /// What applies when the clock shows `local`. With no rule covering that time, or with the
    /// schedule switched off, the ordinary limits do.
    pub fn limits_at(&self, ordinary: Limits, local: NaiveDateTime) -> InForce {
        let weekday = local.weekday().num_days_from_monday() as u8;
        let minute = minute_of_day(&local);
        let rule = if self.enabled {
            self.rules
                .iter()
                .rposition(|rule| rule.covers(weekday, minute))
        } else {
            None
        };
        let limits = match rule {
            Some(index) => Limits {
                upload: self.rules[index].upload.in_force(),
                download: self.rules[index].download.in_force(),
            },
            None => ordinary,
        };
        InForce { rule, limits }
    }

    /// The first moment after `now` at which another rule, or none, comes into force. `None`
    /// when nothing changes within the coming week.
    pub fn next_change_after<Tz: TimeZone>(
        &self,
        ordinary: Limits,
        now: &DateTime<Tz>,
    ) -> Option<DateTime<Tz>> {
        if !self.enabled {
            return None;
        }
        let current = self.limits_at(ordinary, now.naive_local());
        self.next_moment_after(ordinary, now, |in_force| *in_force != current)
    }

    /// When transfers that need the paused ones of these directions can go again: the first
    /// moment after `now` at which none of them is paused. `None` when that is beyond the
    /// coming week, and `now` itself when none of them is paused now.
    pub fn pause_end_after<Tz: TimeZone>(
        &self,
        ordinary: Limits,
        now: &DateTime<Tz>,
        upload: bool,
        download: bool,
    ) -> Option<DateTime<Tz>> {
        let paused = |in_force: &InForce| {
            (upload && in_force.limits.upload.is_paused())
                || (download && in_force.limits.download.is_paused())
        };
        if !paused(&self.limits_at(ordinary, now.naive_local())) {
            return Some(now.clone());
        }
        self.next_moment_after(ordinary, now, |in_force| !paused(in_force))
    }

    /// The first moment after `now` at which what is in force is `wanted`, within the coming
    /// week.
    fn next_moment_after<Tz: TimeZone>(
        &self,
        ordinary: Limits,
        now: &DateTime<Tz>,
        wanted: impl Fn(&InForce) -> bool,
    ) -> Option<DateTime<Tz>> {
        if !self.enabled {
            return None;
        }
        // Rules start and end only when the clock shows one of these times.
        let mut times_of_day: Vec<u32> = self
            .rules
            .iter()
            .filter(|rule| !rule.days.is_empty())
            .flat_map(|rule| [rule.start_minute, rule.end_minute])
            .collect();
        times_of_day.sort_unstable();
        times_of_day.dedup();

        let zone = now.timezone();
        let moment_at = |minute: i64| zone.timestamp_opt(minute * 60, 0).single();
        let mut minute = now.timestamp().div_euclid(60);
        let last_minute = minute + SEARCH_DAYS * i64::from(MINUTES_PER_DAY);
        let mut local = moment_at(minute)?.naive_local();
        while minute < last_minute {
            let time_of_day = minute_of_day(&local);
            // Until the clock next shows one of those times: a minute at least, a day at most.
            let step = times_of_day
                .iter()
                .map(|time| (time + MINUTES_PER_DAY - time_of_day - 1) % MINUTES_PER_DAY + 1)
                .min()?;
            let step = i64::from(step);
            let reached = moment_at(minute + step)?.naive_local();
            if reached == local + Duration::minutes(step) {
                minute += step;
                local = reached;
                if wanted(&self.limits_at(ordinary, local)) {
                    return moment_at(minute);
                }
                continue;
            }
            // The clocks change on the way there, so times of day are skipped or come twice:
            // every minute of it is looked at.
            for _ in 0..step {
                minute += 1;
                local = moment_at(minute)?.naive_local();
                if wanted(&self.limits_at(ordinary, local)) {
                    return moment_at(minute);
                }
            }
        }
        None
    }

    pub fn standing<Tz: TimeZone>(&self, ordinary: Limits, now: &DateTime<Tz>) -> Standing {
        let in_force = self.limits_at(ordinary, now.naive_local());
        let (upload, download) = (
            in_force.limits.upload.is_paused(),
            in_force.limits.download.is_paused(),
        );
        let pause_end = |held: bool, upload: bool, download: bool| {
            held.then(|| self.pause_end_after(ordinary, now, upload, download))
                .flatten()
                .map(|end| end.timestamp())
        };
        Standing {
            at: now.timestamp(),
            local: now.naive_local(),
            in_force,
            next_change: self
                .next_change_after(ordinary, now)
                .map(|change| change.fixed_offset()),
            pause_ends: PauseEnds {
                upload: pause_end(upload, true, false),
                download: pause_end(download, false, true),
                relay: pause_end(upload || download, true, true),
            },
        }
    }

    /// Has every rule leave uploads at this many KiB per second (zero is unlimited), for a run
    /// that was given its own upload limit.
    pub fn pin_upload(&mut self, kib: u32) {
        for rule in &mut self.rules {
            rule.upload = RateLimit::limited(kib);
        }
    }

    /// The same for downloads.
    pub fn pin_download(&mut self, kib: u32) {
        for rule in &mut self.rules {
            rule.download = RateLimit::limited(kib);
        }
    }
}

fn minute_of_day(local: &NaiveDateTime) -> u32 {
    local.hour() * 60 + local.minute()
}

/// A time as a clock shows it, `18:00`, with the weekday when it is not on the day of `today`.
pub fn clock_time(moment: &NaiveDateTime, today: &NaiveDateTime) -> String {
    let time = format!("{:02}:{:02}", moment.hour(), moment.minute());
    if moment.date() == today.date() {
        time
    } else {
        let weekday = moment.weekday().num_days_from_monday() as usize;
        format!("{} {time}", WEEKDAY_NAMES[weekday])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{LocalResult, NaiveDate, NaiveTime, Offset, Utc};

    use super::*;

    const EVERY_DAY: [u8; 7] = [0, 1, 2, 3, 4, 5, 6];

    fn rule(
        name: &str,
        days: &[u8],
        start: (u32, u32),
        end: (u32, u32),
        upload: RateLimit,
        download: RateLimit,
    ) -> LimitRule {
        LimitRule {
            id: name.to_lowercase(),
            name: name.into(),
            days: days.to_vec(),
            start_minute: start.0 * 60 + start.1,
            end_minute: end.0 * 60 + end.1,
            upload,
            download,
        }
    }

    fn schedule(rules: Vec<LimitRule>) -> LimitSchedule {
        LimitSchedule {
            enabled: true,
            rules,
        }
    }

    fn local(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
            .unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    #[derive(Deserialize)]
    struct Table {
        schedules: HashMap<String, LimitSchedule>,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        why: String,
        schedule: String,
        ordinary: [u32; 2],
        at: String,
        rule: Option<usize>,
        upload: Expected,
        download: Expected,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Expected {
        Kib(u32),
        Mode(LimitMode),
    }

    impl Expected {
        fn limit(&self) -> RateLimit {
            match self {
                Self::Kib(kib) => RateLimit::limited(*kib),
                Self::Mode(LimitMode::Paused) => RateLimit::PAUSED,
                Self::Mode(_) => RateLimit::UNLIMITED,
            }
        }
    }

    #[test]
    fn the_table_of_cases_shared_with_the_week_view() {
        let table: Table = serde_json::from_str(include_str!(
            "../../tests/fixtures/limit-schedule-cases.json"
        ))
        .unwrap();
        assert!(table.cases.len() > 40);
        for case in &table.cases {
            let mut schedule = table.schedules[&case.schedule].clone();
            let stored = schedule.clone();
            schedule.sanitize();
            assert_eq!(
                schedule, stored,
                "{} is stored as it is used",
                case.schedule
            );
            let ordinary = Limits::ordinary(case.ordinary[0], case.ordinary[1]);
            let in_force = schedule.limits_at(ordinary, local(&case.at));
            let expected = InForce {
                rule: case.rule,
                limits: Limits {
                    upload: case.upload.limit(),
                    download: case.download.limit(),
                },
            };
            assert_eq!(
                in_force, expected,
                "{} at {}: {}",
                case.schedule, case.at, case.why
            );
        }
    }

    fn office() -> LimitSchedule {
        schedule(vec![
            rule(
                "Night",
                &EVERY_DAY,
                (22, 0),
                (6, 0),
                RateLimit::UNLIMITED,
                RateLimit::UNLIMITED,
            ),
            rule(
                "Work hours",
                &[0, 1, 2, 3, 4],
                (9, 0),
                (18, 0),
                RateLimit::limited(200),
                RateLimit::limited(500),
            ),
            rule(
                "Backup window",
                &EVERY_DAY,
                (2, 0),
                (2, 30),
                RateLimit::PAUSED,
                RateLimit::PAUSED,
            ),
        ])
    }

    fn utc(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn the_next_change_is_the_next_start_or_end_that_matters() {
        let office = office();
        let ordinary = Limits::ordinary(0, 2000);
        let next = |now: DateTime<Utc>| office.next_change_after(ordinary, &now);
        // Monday morning: work starts at nine.
        assert_eq!(next(utc(23, 7, 30)), Some(utc(23, 9, 0)));
        // A change exactly now is not the next one.
        assert_eq!(next(utc(23, 9, 0)), Some(utc(23, 18, 0)));
        assert_eq!(
            next(utc(23, 17, 59) + Duration::seconds(59)),
            Some(utc(23, 18, 0))
        );
        assert_eq!(next(utc(23, 18, 0)), Some(utc(23, 22, 0)));
        // The night is interrupted by the backup window, then goes on behind it.
        assert_eq!(next(utc(23, 23, 0)), Some(utc(24, 2, 0)));
        assert_eq!(next(utc(24, 2, 10)), Some(utc(24, 2, 30)));
        assert_eq!(next(utc(24, 2, 30)), Some(utc(24, 6, 0)));
        // Friday evening to Monday morning, past a weekend of nights.
        assert_eq!(next(utc(27, 18, 0)), Some(utc(27, 22, 0)));
        assert_eq!(next(utc(28, 6, 0)), Some(utc(28, 22, 0)));
        assert_eq!(next(utc(30, 6, 0)), Some(utc(30, 9, 0)));
    }

    #[test]
    fn rules_that_change_nothing_are_not_a_change() {
        let ordinary = Limits::ordinary(0, 0);
        let always = schedule(vec![rule(
            "Always",
            &EVERY_DAY,
            (0, 0),
            (0, 0),
            RateLimit::limited(100),
            RateLimit::PAUSED,
        )]);
        assert_eq!(always.next_change_after(ordinary, &utc(23, 12, 0)), None);
        assert_eq!(
            always.standing(ordinary, &utc(23, 12, 0)).in_force.rule,
            Some(0)
        );

        let never = schedule(vec![rule(
            "Never",
            &[],
            (9, 0),
            (18, 0),
            RateLimit::PAUSED,
            RateLimit::PAUSED,
        )]);
        assert_eq!(never.next_change_after(ordinary, &utc(23, 8, 0)), None);
        assert_eq!(
            schedule(Vec::new()).next_change_after(ordinary, &utc(23, 8, 0)),
            None
        );

        let off = LimitSchedule {
            enabled: false,
            ..office()
        };
        assert_eq!(off.next_change_after(ordinary, &utc(23, 8, 0)), None);

        // Thursdays only: the next change is most of a week away.
        let weekly = schedule(vec![rule(
            "Thursday",
            &[3],
            (9, 0),
            (10, 0),
            RateLimit::PAUSED,
            RateLimit::PAUSED,
        )]);
        assert_eq!(
            weekly.next_change_after(ordinary, &utc(26, 10, 0)),
            Some(utc(26, 9, 0) + Duration::days(7))
        );
    }

    #[test]
    fn a_pause_ends_once_no_rule_pauses_that_direction() {
        let chained = schedule(vec![
            rule(
                "Backup",
                &EVERY_DAY,
                (2, 0),
                (2, 30),
                RateLimit::PAUSED,
                RateLimit::PAUSED,
            ),
            rule(
                "Maintenance",
                &EVERY_DAY,
                (2, 30),
                (3, 0),
                RateLimit::PAUSED,
                RateLimit::limited(100),
            ),
            rule(
                "Upload window",
                &EVERY_DAY,
                (3, 0),
                (3, 15),
                RateLimit::limited(50),
                RateLimit::PAUSED,
            ),
        ]);
        let ordinary = Limits::ordinary(0, 0);
        let at = |hour, minute| chained.standing(ordinary, &utc(23, hour, minute));
        let time = |hour, minute| Some(utc(23, hour, minute).timestamp());

        // The next change is into another pause of uploads, so they wait for longer than that.
        let backup = at(2, 10);
        assert_eq!(backup.next_change, Some(utc(23, 2, 30).fixed_offset()));
        assert_eq!(
            backup.pause_ends,
            PauseEnds {
                upload: time(3, 0),
                download: time(2, 30),
                // Each direction is paused in turn until a quarter past three.
                relay: time(3, 15),
            }
        );
        assert_eq!(
            at(2, 40).pause_ends,
            PauseEnds {
                upload: time(3, 0),
                download: None,
                relay: time(3, 15),
            }
        );
        assert_eq!(
            at(3, 5).pause_ends,
            PauseEnds {
                upload: None,
                download: time(3, 15),
                relay: time(3, 15),
            }
        );
        assert_eq!(at(4, 0).pause_ends, PauseEnds::default());

        // A pause with no end in the coming week has none to show.
        let always = schedule(vec![rule(
            "Always",
            &EVERY_DAY,
            (0, 0),
            (0, 0),
            RateLimit::PAUSED,
            RateLimit::UNLIMITED,
        )]);
        let standing = always.standing(ordinary, &utc(23, 12, 0));
        assert!(standing.in_force.limits.upload.is_paused());
        assert_eq!(standing.pause_ends, PauseEnds::default());
    }

    #[test]
    fn rules_follow_the_local_time_of_the_zone() {
        let office = office();
        let ordinary = Limits::ordinary(0, 0);
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        // Monday 08:30 in Tokyo is still Sunday evening in UTC.
        let now = tokyo.with_ymd_and_hms(2026, 11, 23, 8, 30, 0).unwrap();
        let standing = office.standing(ordinary, &now);
        assert_eq!(standing.in_force.rule, None);
        assert_eq!(standing.at, now.timestamp());
        let change = standing.next_change.unwrap();
        assert_eq!(change.timestamp() - now.timestamp(), 30 * 60);
        assert_eq!(clock_time(&change.naive_local(), &standing.local), "09:00");

        // A zone a quarter of an hour off the others.
        let kathmandu = FixedOffset::east_opt(5 * 3600 + 45 * 60).unwrap();
        let now = kathmandu
            .with_ymd_and_hms(2026, 11, 27, 17, 59, 30)
            .unwrap();
        let standing = office.standing(ordinary, &now);
        assert_eq!(standing.in_force.rule, Some(1));
        assert_eq!(
            standing.next_change.unwrap().timestamp(),
            now.timestamp() + 30
        );
    }

    /// A zone whose clocks change once, as a zone with daylight saving time does twice a year.
    #[derive(Debug, Clone, Copy)]
    struct ChangingZone {
        before: FixedOffset,
        after: FixedOffset,
        /// In UTC.
        changes_at: NaiveDateTime,
    }

    #[derive(Debug, Clone, Copy)]
    struct ChangingOffset {
        zone: ChangingZone,
        offset: FixedOffset,
    }

    impl Offset for ChangingOffset {
        fn fix(&self) -> FixedOffset {
            self.offset
        }
    }

    impl ChangingZone {
        fn offset(&self, offset: FixedOffset) -> ChangingOffset {
            ChangingOffset {
                zone: *self,
                offset,
            }
        }

        /// Europe/Berlin on 29 March 2026: 02:00 becomes 03:00.
        fn berlin_spring() -> Self {
            Self {
                before: FixedOffset::east_opt(3600).unwrap(),
                after: FixedOffset::east_opt(2 * 3600).unwrap(),
                changes_at: local("2026-03-29T01:00:00"),
            }
        }

        /// Europe/Berlin on 25 October 2026: 03:00 becomes 02:00.
        fn berlin_autumn() -> Self {
            Self {
                before: FixedOffset::east_opt(2 * 3600).unwrap(),
                after: FixedOffset::east_opt(3600).unwrap(),
                changes_at: local("2026-10-25T01:00:00"),
            }
        }
    }

    impl TimeZone for ChangingZone {
        type Offset = ChangingOffset;

        fn from_offset(offset: &ChangingOffset) -> Self {
            offset.zone
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<ChangingOffset> {
            self.offset_from_local_datetime(&local.and_time(NaiveTime::MIN))
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<ChangingOffset> {
            let shown_before = *local - self.before < self.changes_at;
            let shown_after = *local - self.after >= self.changes_at;
            match (shown_before, shown_after) {
                (true, true) => {
                    LocalResult::Ambiguous(self.offset(self.before), self.offset(self.after))
                }
                (true, false) => LocalResult::Single(self.offset(self.before)),
                (false, true) => LocalResult::Single(self.offset(self.after)),
                (false, false) => LocalResult::None,
            }
        }

        fn offset_from_utc_date(&self, utc: &NaiveDate) -> ChangingOffset {
            self.offset_from_utc_datetime(&utc.and_time(NaiveTime::MIN))
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> ChangingOffset {
            self.offset(if *utc < self.changes_at {
                self.before
            } else {
                self.after
            })
        }
    }

    fn clock_changes() -> LimitSchedule {
        schedule(vec![
            rule(
                "Starts in the skipped hour",
                &EVERY_DAY,
                (2, 30),
                (4, 0),
                RateLimit::limited(100),
                RateLimit::limited(100),
            ),
            rule(
                "Ends in the skipped hour",
                &EVERY_DAY,
                (1, 30),
                (2, 15),
                RateLimit::PAUSED,
                RateLimit::PAUSED,
            ),
        ])
    }

    /// Walks from change to change, giving the local time and rule after each.
    fn changes<Tz: TimeZone>(
        schedule: &LimitSchedule,
        from: &DateTime<Tz>,
        count: usize,
    ) -> Vec<(String, i64, Option<usize>)> {
        let ordinary = Limits::ordinary(0, 0);
        let mut now = from.clone();
        let mut seen = Vec::new();
        for _ in 0..count {
            let Some(change) = schedule.next_change_after(ordinary, &now) else {
                break;
            };
            let local = change.naive_local();
            seen.push((
                format!("{:02}:{:02}", local.hour(), local.minute()),
                change.timestamp() - from.timestamp(),
                schedule.limits_at(ordinary, local).rule,
            ));
            now = change;
        }
        seen
    }

    #[test]
    fn an_hour_the_clocks_skip_cuts_rules_short_and_starts_them_late() {
        let zone = ChangingZone::berlin_spring();
        let schedule = clock_changes();
        // 01:00 local, an hour before the clocks go from 02:00 to 03:00.
        let from = zone
            .from_local_datetime(&local("2026-03-29T01:00:00"))
            .unwrap();
        assert_eq!(
            changes(&schedule, &from, 3),
            vec![
                ("01:30".into(), 30 * 60, Some(1)),
                // The rule that ends at 02:15 ends when 02:00 never comes, and the one that
                // starts at 02:30 starts as the clock jumps past it.
                ("03:00".into(), 60 * 60, Some(0)),
                ("04:00".into(), 2 * 60 * 60, None),
            ]
        );
    }

    #[test]
    fn an_hour_the_clocks_repeat_runs_its_rules_twice() {
        let zone = ChangingZone::berlin_autumn();
        let schedule = clock_changes();
        // 01:00 local, two hours before the clocks go from 03:00 back to 02:00.
        let from = zone
            .from_local_datetime(&local("2026-10-25T01:00:00"))
            .unwrap();
        assert_eq!(
            changes(&schedule, &from, 7),
            vec![
                ("01:30".into(), 30 * 60, Some(1)),
                ("02:15".into(), 75 * 60, None),
                ("02:30".into(), 90 * 60, Some(0)),
                // 03:00 becomes 02:00: the paused rule is back for its last quarter of an hour.
                ("02:00".into(), 120 * 60, Some(1)),
                ("02:15".into(), 135 * 60, None),
                ("02:30".into(), 150 * 60, Some(0)),
                ("04:00".into(), 240 * 60, None),
            ]
        );
        // Both times the clock shows 02:10, the same rule is in force.
        for seconds in [70 * 60, 130 * 60] {
            let moment = from + Duration::seconds(seconds);
            assert_eq!(moment.naive_local(), local("2026-10-25T02:10:00"));
            let standing = schedule.standing(Limits::ordinary(0, 0), &moment);
            assert_eq!(standing.in_force.rule, Some(1));
            assert_eq!(
                standing.next_change.unwrap().timestamp() - moment.timestamp(),
                5 * 60
            );
        }
    }

    #[test]
    fn a_day_with_a_clock_change_is_found_from_a_week_before() {
        let zone = ChangingZone::berlin_spring();
        // Sundays only, in the hour the clocks skip on that Sunday.
        let schedule = schedule(vec![rule(
            "Sunday night",
            &[6],
            (2, 15),
            (2, 45),
            RateLimit::PAUSED,
            RateLimit::PAUSED,
        )]);
        let ordinary = Limits::ordinary(0, 0);
        // The Sunday before, after that week's turn: this Sunday's never comes.
        let from = zone
            .from_local_datetime(&local("2026-03-22T04:00:00"))
            .unwrap();
        assert_eq!(schedule.next_change_after(ordinary, &from), None);
        // A rule that only starts in the skipped hour starts as it ends.
        let longer = LimitSchedule {
            rules: vec![LimitRule {
                end_minute: 3 * 60 + 30,
                ..schedule.rules[0].clone()
            }],
            ..schedule
        };
        let change = longer.next_change_after(ordinary, &from).unwrap();
        assert_eq!(change.naive_local(), local("2026-03-29T03:00:00"));
    }

    #[test]
    fn stored_rules_are_made_safe() {
        let mut stored: LimitSchedule = serde_json::from_value(serde_json::json!({
            "enabled": true,
            "rules": [
                {
                    "id": " work ",
                    "name": "  Work hours \nrm -rf",
                    "days": [4, 0, 9, 4, 255],
                    "startMinute": 540,
                    "endMinute": 5000,
                    "upload": { "mode": "limited", "kib": 0 },
                    "download": { "mode": "limited", "kib": 4_000_000_000u32 }
                },
                {
                    "id": "work",
                    "name": "\u{1b}[2JBackup\t",
                    "upload": { "mode": "stopped", "kib": 7 },
                    "download": { "mode": "paused" }
                },
                { "name": "x".repeat(200), "days": [6] },
                {}
            ]
        }))
        .unwrap();
        stored.sanitize();
        let [first, second, third, fourth] = &stored.rules[..] else {
            panic!("four rules");
        };
        assert_eq!(first.id, "work");
        assert_eq!(first.name, "Work hours");
        assert_eq!(first.days, vec![0, 4]);
        assert_eq!((first.start_minute, first.end_minute), (540, 1439));
        assert_eq!(
            first.upload,
            RateLimit {
                mode: LimitMode::Unlimited,
                kib: 0
            }
        );
        assert_eq!(first.download, RateLimit::limited(MAX_LIMIT_KIB));
        // A second rule with the same id gets its own; a mode from a later version is no limit.
        assert_eq!(second.id, "work-2");
        assert_eq!(second.name, "[2JBackup");
        assert_eq!(
            second.upload,
            RateLimit {
                mode: LimitMode::Unlimited,
                kib: 7
            }
        );
        assert_eq!(second.download, RateLimit::PAUSED);
        assert_eq!(third.id, "rule-3");
        assert_eq!(third.name.chars().count(), MAX_NAME_CHARS);
        assert_eq!(fourth.id, "rule-4");
        assert!(fourth.days.is_empty());

        let again = stored.clone();
        stored.sanitize();
        assert_eq!(stored, again);

        let mut many = schedule(vec![LimitRule::default(); MAX_RULES + 5]);
        many.sanitize();
        assert_eq!(many.rules.len(), MAX_RULES);
    }

    #[test]
    fn every_rule_gets_an_id_no_other_rule_has_and_keeps_it() {
        let longest = "x".repeat(MAX_ID_CHARS);
        let cut_at_a_space = format!("{} y", "y".repeat(MAX_ID_CHARS - 1));
        let ids: [&str; 8] = [
            "a",
            "a-3",
            "a",
            "rule-5",
            "",
            &longest,
            &longest,
            &cut_at_a_space,
        ];
        let mut stored = schedule(
            ids.iter()
                .map(|id| LimitRule {
                    id: id.to_string(),
                    name: format!("{} z", "n".repeat(MAX_NAME_CHARS - 1)),
                    ..LimitRule::default()
                })
                .collect(),
        );
        stored.sanitize();
        let sanitized: Vec<&str> = stored.rules.iter().map(|rule| rule.id.as_str()).collect();
        let longest_again = format!("{}-7", "x".repeat(MAX_ID_CHARS - 2));
        let without_the_space = "y".repeat(MAX_ID_CHARS - 1);
        assert_eq!(
            sanitized,
            [
                "a",
                "a-3",
                "a-4",
                "rule-5",
                "rule-5-5",
                longest.as_str(),
                longest_again.as_str(),
                without_the_space.as_str(),
            ]
        );
        // A name cut just after a space loses the space, so the next pass leaves it alone.
        assert_eq!(stored.rules[0].name, "n".repeat(MAX_NAME_CHARS - 1));

        let again = stored.clone();
        stored.sanitize();
        assert_eq!(stored, again);
    }

    #[test]
    fn a_limit_given_for_one_run_takes_that_direction_out_of_the_schedule() {
        let mut office = office();
        office.pin_download(0);
        let ordinary = Limits::ordinary(0, 0);
        let working = office.limits_at(ordinary, local("2026-11-23T10:00:00"));
        assert_eq!(working.limits.upload, RateLimit::limited(200));
        assert_eq!(working.limits.download, RateLimit::UNLIMITED);
        office.pin_upload(64);
        let backup = office.limits_at(ordinary, local("2026-11-23T02:10:00"));
        assert_eq!(backup.limits.upload, RateLimit::limited(64));
        assert_eq!(backup.limits.download, RateLimit::UNLIMITED);
    }

    #[test]
    fn times_are_shown_as_the_clock_does() {
        let today = local("2026-11-23T10:00:00");
        assert_eq!(clock_time(&local("2026-11-23T18:00:00"), &today), "18:00");
        assert_eq!(
            clock_time(&local("2026-11-24T06:05:00"), &today),
            "Tue 06:05"
        );
        assert_eq!(
            clock_time(&local("2026-11-29T00:00:00"), &today),
            "Sun 00:00"
        );
    }
}
