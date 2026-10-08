//! The date formats FTP servers and the cloud APIs use, converted to and from seconds since
//! the Unix epoch. Always UTC.

use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: i64 = 86_400;

/// Days from 1970-01-01 to a date in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_from_march + 2) / 5 + 1) as u32;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub fn unix_seconds(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<i64> {
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second <= 60;
    valid.then(|| {
        days_from_civil(year, month, day) * SECONDS_PER_DAY
            + i64::from(hour) * 3600
            + i64::from(minute) * 60
            + i64::from(second)
    })
}

fn number(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `2024-03-01T12:34:56Z`, with optional fractional seconds and a `+hh:mm` or `-hh:mm` offset.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.len() < 19 || !text.is_char_boundary(19) {
        return None;
    }
    let (date_time, zone) = text.split_at(19);
    let bytes = date_time.as_bytes();
    let separators_valid = bytes[4] == b'-'
        && bytes[7] == b'-'
        && matches!(bytes[10], b'T' | b't' | b' ')
        && bytes[13] == b':'
        && bytes[16] == b':';
    if !separators_valid {
        return None;
    }
    let seconds = unix_seconds(
        i64::from(number(&date_time[0..4])?),
        number(&date_time[5..7])?,
        number(&date_time[8..10])?,
        number(&date_time[11..13])?,
        number(&date_time[14..16])?,
        number(&date_time[17..19])?,
    )?;
    let zone = match zone.strip_prefix('.') {
        Some(fraction) => fraction.trim_start_matches(|character: char| character.is_ascii_digit()),
        None => zone,
    };
    let offset = match zone {
        "Z" | "z" | "" => 0,
        _ => {
            let sign = match zone.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (hours, minutes) = zone[1..].split_once(':')?;
            sign * (i64::from(number(hours)?) * 3600 + i64::from(number(minutes)?) * 60)
        }
    };
    Some(seconds - offset)
}

pub fn format_rfc3339(seconds: i64) -> String {
    let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_PER_DAY));
    let time_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3600,
        time_of_day % 3600 / 60,
        time_of_day % 60
    )
}

/// `YYYYMMDDHHMMSS`, with optional fractional seconds, as in `MDTM` replies and MLSD facts.
pub fn parse_ftp_time(text: &str) -> Option<i64> {
    let digits = text.trim().split('.').next()?;
    if digits.len() != 14 || !digits.is_ascii() {
        return None;
    }
    unix_seconds(
        i64::from(number(&digits[0..4])?),
        number(&digits[4..6])?,
        number(&digits[6..8])?,
        number(&digits[8..10])?,
        number(&digits[10..12])?,
        number(&digits[12..14])?,
    )
}

pub fn format_ftp_time(seconds: i64) -> String {
    let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_PER_DAY));
    let time_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    format!(
        "{year:04}{month:02}{day:02}{:02}{:02}{:02}",
        time_of_day / 3600,
        time_of_day % 3600 / 60,
        time_of_day % 60
    )
}

/// `Jan` is 1. English abbreviations, as `ls -l` style listings use.
pub fn month_number(abbreviation: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = abbreviation.get(..3)?.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|month| *month == lower)
        .map(|index| index as u32 + 1)
}

pub fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

pub fn year_of(seconds: i64) -> i64 {
    civil_from_days(seconds.div_euclid(SECONDS_PER_DAY)).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_calendar_dates() {
        assert_eq!(unix_seconds(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(unix_seconds(2000, 3, 1, 0, 0, 0), Some(951_868_800));
        assert_eq!(unix_seconds(2024, 2, 29, 23, 59, 59), Some(1_709_251_199));
        for seconds in [0, 951_868_800, 1_709_251_199, -86_400, 4_102_444_800] {
            assert_eq!(parse_rfc3339(&format_rfc3339(seconds)), Some(seconds));
            assert_eq!(parse_ftp_time(&format_ftp_time(seconds)), Some(seconds));
        }
        assert_eq!(unix_seconds(2024, 13, 1, 0, 0, 0), None);
    }

    #[test]
    fn parses_rfc3339_variants() {
        assert_eq!(parse_rfc3339("2024-02-29T23:59:59Z"), Some(1_709_251_199));
        assert_eq!(
            parse_rfc3339("2024-02-29T23:59:59.123Z"),
            Some(1_709_251_199)
        );
        assert_eq!(
            parse_rfc3339("2024-03-01T01:59:59+02:00"),
            Some(1_709_251_199)
        );
        assert_eq!(
            parse_rfc3339("2024-02-29T22:59:59.5-01:00"),
            Some(1_709_251_199)
        );
        assert_eq!(parse_rfc3339("2024-02-29"), None);
        assert_eq!(parse_rfc3339("2024/02/29T23:59:59Z"), None);
    }

    #[test]
    fn parses_ftp_times() {
        assert_eq!(parse_ftp_time("20240229235959"), Some(1_709_251_199));
        assert_eq!(parse_ftp_time("20240229235959.250"), Some(1_709_251_199));
        assert_eq!(parse_ftp_time("2024022923595"), None);
        assert_eq!(format_ftp_time(1_709_251_199), "20240229235959");
    }

    #[test]
    fn months() {
        assert_eq!(month_number("Jan"), Some(1));
        assert_eq!(month_number("DEC"), Some(12));
        assert_eq!(month_number("Foo"), None);
        assert_eq!(year_of(1_709_251_199), 2024);
    }
}
