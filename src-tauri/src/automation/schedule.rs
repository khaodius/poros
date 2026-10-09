//! When scheduled tasks run. Pure, so it is tested with fixed clocks and time zones.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const MINUTES_PER_DAY: u32 = 24 * 60;
/// Longest repeat interval: four weeks.
pub const MAX_INTERVAL_MINUTES: u32 = 28 * MINUTES_PER_DAY;

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
