//! When scheduled tasks run. Pure, so it is tested with fixed clocks and time zones.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const MINUTES_PER_DAY: u32 = 24 * 60;
/// Longest repeat interval: four weeks.
pub const MAX_INTERVAL_MINUTES: u32 = 28 * MINUTES_PER_DAY;
/// Longest folder pattern a task may keep.
pub const MAX_PATTERN_CHARS: usize = 200;
/// Monday first, as weekdays are numbered. English whatever the system's language, so the
/// folders a pattern names sort and match the same everywhere.
const WEEKDAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
/// What a folder pattern may hold between braces. Mirrored in `src/lib/schedule.ts`.
pub const PATTERN_PLACEHOLDERS: [&str; 9] = [
    "date", "time", "year", "month", "day", "hour", "minute", "weekday", "week",
];
/// Characters no folder a pattern names may hold: they end a path part, or are refused by
/// Windows and by most cloud drives.
const FORBIDDEN_IN_FOLDER_NAMES: [char; 8] = ['\\', ':', '*', '?', '"', '<', '>', '|'];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Trigger {
    /// Seconds since the Unix epoch.
    #[serde(rename_all = "camelCase")]
    Once { at: i64 },
    /// From `start` (seconds since the Unix epoch), then every `minutes`.
    #[serde(rename_all = "camelCase")]
    Every { minutes: u32, start: i64 },
    /// At a local time of day on the chosen weekdays, Monday being 0.
    #[serde(rename_all = "camelCase")]
    Daily {
        minute_of_day: u32,
        weekdays: Vec<u8>,
    },
}

impl Trigger {
    pub fn validate(&self) -> AppResult<()> {
        match self {
            Self::Once { .. } => Ok(()),
            Self::Every { minutes, .. } => {
                if (1..=MAX_INTERVAL_MINUTES).contains(minutes) {
                    Ok(())
                } else {
                    Err(AppError::invalid("Repeat every 1 minute to 4 weeks"))
                }
            }
            Self::Daily {
                minute_of_day,
                weekdays,
            } => {
                if *minute_of_day >= MINUTES_PER_DAY {
                    Err(AppError::invalid("Pick a time of day"))
                } else if weekdays.is_empty() || weekdays.iter().any(|day| *day > 6) {
                    Err(AppError::invalid("Pick at least one day of the week"))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// The first time strictly after `after` that the task should run.
    pub fn next_after<Tz: TimeZone>(&self, after: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let zone = after.timezone();
        match self {
            Self::Once { at } => {
                let at = zone.timestamp_opt(*at, 0).single()?;
                (at > *after).then_some(at)
            }
            Self::Every { minutes, start } => {
                let start = zone.timestamp_opt(*start, 0).single()?;
                if start > *after {
                    return Some(start);
                }
                let period = i64::from((*minutes).max(1)) * 60;
                let periods = (after.timestamp() - start.timestamp()) / period + 1;
                zone.timestamp_opt(start.timestamp() + periods * period, 0)
                    .single()
            }
            Self::Daily {
                minute_of_day,
                weekdays,
            } => {
                let time = NaiveTime::from_hms_opt(minute_of_day / 60, minute_of_day % 60, 0)?;
                let today = after.date_naive();
                // Eight days reach the same weekday again when today's time has passed.
                (0..=7).find_map(|offset| {
                    let date = today + Duration::days(offset);
                    let weekday = date.weekday().num_days_from_monday() as u8;
                    if !weekdays.contains(&weekday) {
                        return None;
                    }
                    let candidate = match zone.from_local_datetime(&date.and_time(time)) {
                        LocalResult::Single(moment) => moment,
                        // Clocks went back: the first of the two.
                        LocalResult::Ambiguous(earliest, _) => earliest,
                        // Clocks went forward past this time: run when the gap ends.
                        LocalResult::None => zone
                            .from_local_datetime(&(date.and_time(time) + Duration::hours(1)))
                            .earliest()?,
                    };
                    (candidate > *after).then_some(candidate)
                })
            }
        }
    }
}

impl Trigger {
    /// When the task runs, in words: `Every day at 02:00`, `Every 30 minutes`. Mirrors
    /// `describeTrigger` in `src/lib/schedule.ts`. A single time is shown in `zone`.
    pub fn describe<Tz: TimeZone>(&self, zone: &Tz) -> String {
        match self {
            Self::Once { at } => match zone.timestamp_opt(*at, 0).single() {
                Some(moment) => format!("Once, {}", date_and_time(&moment)),
                None => "Once".to_string(),
            },
            Self::Every { minutes, .. } => {
                let (amount, unit) = [(MINUTES_PER_DAY, "day"), (60, "hour")]
                    .into_iter()
                    .find(|(per_unit, _)| minutes % per_unit == 0 && *minutes >= *per_unit)
                    .map_or((*minutes, "minute"), |(per_unit, unit)| {
                        (minutes / per_unit, unit)
                    });
                if amount == 1 {
                    format!("Every {unit}")
                } else {
                    format!("Every {amount} {unit}s")
                }
            }
            Self::Daily {
                minute_of_day,
                weekdays,
            } => {
                let mut days: Vec<u8> = weekdays.iter().copied().filter(|day| *day < 7).collect();
                days.sort_unstable();
                days.dedup();
                let days = match days.as_slice() {
                    [0, 1, 2, 3, 4, 5, 6] => "Every day".to_string(),
                    [0, 1, 2, 3, 4] => "Weekdays".to_string(),
                    [5, 6] => "Weekends".to_string(),
                    days => days
                        .iter()
                        .map(|day| WEEKDAY_NAMES[usize::from(*day)])
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                format!(
                    "{days} at {:02}:{:02}",
                    minute_of_day / 60,
                    minute_of_day % 60
                )
            }
        }
    }
}

/// `2026-11-23 02:00`, as the app shows times.
pub fn date_and_time<Tz: TimeZone>(moment: &DateTime<Tz>) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        moment.year(),
        moment.month(),
        moment.day(),
        moment.hour(),
        moment.minute()
    )
}

/// The folder a pattern such as `backup-{date}` names for a run at `now`, in the local time of
/// `now`: `{date}` is `2026-11-23`, `{time}` `0200`, `{year}`, `{month}`, `{day}`, `{hour}` and
/// `{minute}` their digits, `{weekday}` `Mon` to `Sun` and `{week}` the ISO week. A pattern
/// may name folders inside folders with `/`. An empty pattern names no folder.
pub fn expand_folder_pattern<Tz: TimeZone>(pattern: &str, now: &DateTime<Tz>) -> AppResult<String> {
    let pattern = pattern.trim().trim_matches('/');
    if pattern.chars().count() > MAX_PATTERN_CHARS {
        return Err(AppError::invalid(format!(
            "The folder pattern is longer than {MAX_PATTERN_CHARS} characters"
        )));
    }
    let mut expanded = String::with_capacity(pattern.len() + 8);
    let mut rest = pattern;
    while let Some(open) = rest.find(['{', '}']) {
        expanded.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = match (rest.as_bytes()[open], after.find(['{', '}'])) {
            (b'{', Some(close)) if after.as_bytes()[close] == b'}' => close,
            _ => return Err(AppError::invalid(
                "The folder pattern has a brace without its partner; write placeholders as {date}",
            )),
        };
        let placeholder = &after[..close];
        let value = match placeholder {
            "date" => format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day()),
            "time" => format!("{:02}{:02}", now.hour(), now.minute()),
            "year" => format!("{:04}", now.year()),
            "month" => format!("{:02}", now.month()),
            "day" => format!("{:02}", now.day()),
            "hour" => format!("{:02}", now.hour()),
            "minute" => format!("{:02}", now.minute()),
            "weekday" => WEEKDAY_NAMES[now.weekday().num_days_from_monday() as usize].to_string(),
            "week" => format!("{:02}", now.iso_week().week()),
            unknown => {
                return Err(AppError::invalid(format!(
                    "The folder pattern has {{{unknown}}}, which is not a placeholder; use {}",
                    PATTERN_PLACEHOLDERS
                        .iter()
                        .map(|name| format!("{{{name}}}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )))
            }
        };
        expanded.push_str(&value);
        rest = &after[close + 1..];
    }
    expanded.push_str(rest);
    for part in expanded.split('/') {
        let plain = !part.is_empty()
            && part != "."
            && part != ".."
            && part.trim() == part
            && !part.ends_with('.')
            && !part.chars().any(|character| {
                character.is_control() || FORBIDDEN_IN_FOLDER_NAMES.contains(&character)
            });
        if !plain && !expanded.is_empty() {
            return Err(AppError::invalid(format!(
                "The folder pattern names \"{part}\", which cannot be a folder name"
            )));
        }
    }
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, NaiveDate, Utc};

    fn utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn once_runs_only_in_the_future() {
        let at = utc(2026, 10, 8, 12, 0);
        let trigger = Trigger::Once { at: at.timestamp() };
        assert_eq!(trigger.next_after(&utc(2026, 10, 8, 11, 0)), Some(at));
        assert_eq!(trigger.next_after(&at), None);
    }

    #[test]
    fn every_counts_from_the_start_without_drifting() {
        let start = utc(2026, 10, 8, 9, 0);
        let trigger = Trigger::Every {
            minutes: 90,
            start: start.timestamp(),
        };
        assert_eq!(trigger.next_after(&utc(2026, 10, 8, 8, 0)), Some(start));
        assert_eq!(trigger.next_after(&start), Some(utc(2026, 10, 8, 10, 30)));
        // Late by a while: the next slot on the original grid, not now plus 90 minutes.
        assert_eq!(
            trigger.next_after(&utc(2026, 10, 8, 13, 5)),
            Some(utc(2026, 10, 8, 13, 30))
        );
    }

    #[test]
    fn daily_picks_the_next_chosen_weekday() {
        // 2026-10-08 is a Thursday (3).
        let trigger = Trigger::Daily {
            minute_of_day: 2 * 60 + 30,
            weekdays: vec![0, 3],
        };
        assert_eq!(
            trigger.next_after(&utc(2026, 10, 8, 1, 0)),
            Some(utc(2026, 10, 8, 2, 30))
        );
        assert_eq!(
            trigger.next_after(&utc(2026, 10, 8, 2, 30)),
            Some(utc(2026, 10, 12, 2, 30))
        );
        let thursdays = Trigger::Daily {
            minute_of_day: 0,
            weekdays: vec![3],
        };
        assert_eq!(
            thursdays.next_after(&utc(2026, 10, 8, 0, 1)),
            Some(utc(2026, 10, 15, 0, 0))
        );
    }

    #[test]
    fn daily_uses_the_local_time_of_the_zone() {
        let berlin_summer = FixedOffset::east_opt(2 * 3600).unwrap();
        let after = berlin_summer
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 10, 8)
                    .unwrap()
                    .and_hms_opt(20, 0, 0)
                    .unwrap(),
            )
            .unwrap();
        let trigger = Trigger::Daily {
            minute_of_day: 21 * 60,
            weekdays: (0..7).collect(),
        };
        let next = trigger.next_after(&after).unwrap();
        assert_eq!(next.timestamp() - after.timestamp(), 3600);
    }

    #[test]
    fn invalid_triggers_are_refused() {
        assert!(Trigger::Every {
            minutes: 0,
            start: 0
        }
        .validate()
        .is_err());
        assert!(Trigger::Daily {
            minute_of_day: 60,
            weekdays: vec![]
        }
        .validate()
        .is_err());
        assert!(Trigger::Daily {
            minute_of_day: MINUTES_PER_DAY,
            weekdays: vec![1]
        }
        .validate()
        .is_err());
        assert!(Trigger::Daily {
            minute_of_day: 60,
            weekdays: vec![6]
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn describes_when_a_task_runs() {
        let every = |minutes| Trigger::Every { minutes, start: 0 };
        assert_eq!(every(60).describe(&Utc), "Every hour");
        assert_eq!(every(30).describe(&Utc), "Every 30 minutes");
        assert_eq!(every(90).describe(&Utc), "Every 90 minutes");
        assert_eq!(every(2880).describe(&Utc), "Every 2 days");
        let daily = |minute_of_day, weekdays: &[u8]| Trigger::Daily {
            minute_of_day,
            weekdays: weekdays.to_vec(),
        };
        assert_eq!(
            daily(150, &[0, 1, 2, 3, 4, 5, 6]).describe(&Utc),
            "Every day at 02:30"
        );
        assert_eq!(
            daily(0, &[4, 0, 2, 1, 3]).describe(&Utc),
            "Weekdays at 00:00"
        );
        assert_eq!(daily(600, &[6, 2]).describe(&Utc), "Wed, Sun at 10:00");
        let once = Trigger::Once {
            at: utc(2026, 10, 8, 14, 30).timestamp(),
        };
        assert_eq!(once.describe(&Utc), "Once, 2026-10-08 14:30");
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        assert_eq!(once.describe(&tokyo), "Once, 2026-10-08 23:30");
    }

    #[test]
    fn folder_patterns_take_the_date_and_time_of_the_run() {
        // 2026-11-23 is a Monday in ISO week 48.
        let now = utc(2026, 11, 23, 2, 5);
        let expand = |pattern: &str| expand_folder_pattern(pattern, &now).unwrap();
        assert_eq!(expand("backup-{date}"), "backup-2026-11-23");
        assert_eq!(expand(" /{year}/{month}/site-{day}/ "), "2026/11/site-23");
        assert_eq!(expand("{date}_{time}"), "2026-11-23_0205");
        assert_eq!(expand("{hour}h{minute}"), "02h05");
        assert_eq!(expand("weekly/{weekday}"), "weekly/Mon");
        assert_eq!(expand("week-{week}"), "week-48");
        assert_eq!(expand("plain folder"), "plain folder");
        assert_eq!(expand(""), "");
        assert_eq!(expand(" / "), "");
    }

    #[test]
    fn folder_patterns_use_the_local_date_of_their_zone() {
        // Half past eleven in the evening in New York is already the next day in UTC.
        let new_york = FixedOffset::west_opt(5 * 3600).unwrap();
        let evening = new_york
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 12, 31)
                    .unwrap()
                    .and_hms_opt(23, 30, 0)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            expand_folder_pattern("{date}-{weekday}", &evening).unwrap(),
            "2026-12-31-Thu"
        );
        assert_eq!(
            expand_folder_pattern("{date}", &evening.with_timezone(&Utc)).unwrap(),
            "2027-01-01"
        );
        // A year that starts on a Thursday, as 2026 does, has 53 ISO weeks.
        assert_eq!(expand_folder_pattern("{week}", &evening).unwrap(), "53");
    }

    #[test]
    fn folder_patterns_that_cannot_name_a_folder_are_refused() {
        let now = utc(2026, 11, 23, 2, 5);
        let refused = |pattern: &str| expand_folder_pattern(pattern, &now).unwrap_err().message;
        assert!(refused("backup-{dat}").contains("{dat}"));
        assert!(refused("backup-{dat}").contains("{date}"));
        assert!(refused("backup-{date").contains("brace"));
        assert!(refused("backup-date}").contains("brace"));
        assert!(refused("{{date}}").contains("brace"));
        assert!(refused("../outside").contains("\"..\""));
        assert!(refused("a/../../b").contains("cannot be a folder name"));
        assert!(refused("a//b").contains("cannot be a folder name"));
        assert!(refused("C:\\backups").contains("cannot be a folder name"));
        assert!(refused("what?").contains("cannot be a folder name"));
        assert!(refused("line\nbreak").contains("cannot be a folder name"));
        assert!(refused("trailing./x").contains("cannot be a folder name"));
        assert!(refused(&"x".repeat(MAX_PATTERN_CHARS + 1)).contains("longer"));
    }

    #[test]
    fn serializes_with_a_type_tag() {
        let json = serde_json::to_value(Trigger::Daily {
            minute_of_day: 90,
            weekdays: vec![1],
        })
        .unwrap();
        assert_eq!(json["type"], "daily");
        assert_eq!(json["minuteOfDay"], 90);
    }
}
