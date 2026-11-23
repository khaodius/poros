//! `find`: the search the app's panes have, from the command line.

use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use serde_json::json;

use super::browse::render_entries;
use crate::cli::args::{FindArgs, FindKind};
use crate::cli::context::Context;
use crate::cli::exit::Exit;
use crate::cli::output::{plural, Reply};
use crate::error::{AppError, AppResult};
use crate::file_ops::{Failure, Location};
use crate::model::FileEntry;
use crate::search::{
    KindFilter, Outcome, PatternKind, SearchManager, SearchReport, SearchRequest, SearchSink,
};

/// The one search a run of the command line has going.
const SEARCH_ID: &str = "cli";

pub async fn find(context: &Context, arguments: &FindArgs) -> AppResult<Reply> {
    let now = chrono::Utc::now().timestamp();
    let min_size = arguments.min_size.as_deref().map(parse_size).transpose()?;
    let max_size = arguments.max_size.as_deref().map(parse_size).transpose()?;
    let modified_after = arguments
        .newer
        .as_deref()
        .map(|when| parse_time(when, now, false))
        .transpose()?;
    let modified_before = arguments
        .older
        .as_deref()
        .map(|when| parse_time(when, now, true))
        .transpose()?;
    let connected = context.connected().await?;
    let folder = connected.resolve(arguments.folder.as_deref().unwrap_or_default());
    let request = SearchRequest {
        search_id: SEARCH_ID.to_string(),
        location: Location::Remote {
            session_id: connected.info.id.clone(),
        },
        folder: folder.clone(),
        pattern: arguments.name.clone(),
        pattern_kind: if arguments.regex {
            PatternKind::Regex
        } else {
            PatternKind::Text
        },
        case_sensitive: arguments.case_sensitive,
        kind: match arguments.kind {
            None => KindFilter::Any,
            Some(FindKind::File) => KindFilter::Files,
            Some(FindKind::Folder) => KindFilter::Folders,
        },
        min_size,
        max_size,
        modified_after,
        modified_before,
        max_depth: arguments.max_depth,
        hidden: arguments.hidden,
        follow_links: arguments.follow,
        limit: Some(arguments.limit),
    };

    let found: Arc<Mutex<(Vec<FileEntry>, Vec<Failure>)>> = Arc::default();
    let collected = found.clone();
    let sink: SearchSink = Arc::new(move |report: SearchReport| {
        let mut collected = collected.lock().unwrap();
        collected.0.extend(report.entries);
        collected.1.extend(report.unreadable);
    });
    let searches = SearchManager::new(context.sessions.clone());
    let interrupt = context.interrupt.clone();
    let summary = tokio::select! {
        summary = searches.run(request, sink) => summary?,
        _ = interrupt.cancelled() => return Err(AppError::cancelled()),
    };

    let (mut entries, unreadable) = std::mem::take(&mut *found.lock().unwrap());
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let mut lines = Vec::new();
    if !entries.is_empty() {
        lines.push(render_entries(&entries, arguments.long, true));
    }
    let capped = summary.outcome == Outcome::Capped;
    if capped {
        log::warn!(
            "Stopped at the first {}; there may be more",
            plural(summary.found, "result")
        );
    }
    for failure in &unreadable {
        log::warn!("{}: {}", failure.path, failure.message);
    }
    let value = json!({
        "folder": folder,
        "engine": summary.engine,
        "entries": entries,
        "capped": capped,
        "foldersSearched": summary.folders,
        "unreadable": unreadable,
        "unreadableCount": summary.unreadable,
    });
    let reply = Reply::new(value, lines.join("\n"));
    Ok(if summary.unreadable > 0 {
        reply.with_exit(Exit::Partial)
    } else {
        reply
    })
}

/// Bytes, or a number with a unit in powers of 1024: `500K`, `1.5M`, `2G`, `1T`.
fn parse_size(text: &str) -> AppResult<u64> {
    let invalid = || {
        AppError::invalid(format!(
            "{text} is not a size: give bytes, or a unit as in 500K, 100M, 2G"
        ))
    };
    let trimmed = text.trim();
    let unit_start = trimmed
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(unit_start);
    let factor: u64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1 << 10,
        "m" | "mb" | "mib" => 1 << 20,
        "g" | "gb" | "gib" => 1 << 30,
        "t" | "tb" | "tib" => 1 << 40,
        _ => return Err(invalid()),
    };
    if let Ok(whole) = number.parse::<u64>() {
        return whole.checked_mul(factor).ok_or_else(invalid);
    }
    let fraction: f64 = number.parse().map_err(|_| invalid())?;
    let bytes = fraction * factor as f64;
    if !bytes.is_finite() || bytes < 0.0 || bytes >= u64::MAX as f64 {
        return Err(invalid());
    }
    Ok(bytes.round() as u64)
}

/// A moment as seconds since the epoch: an age before `now` (`30m`, `12h`, `7d`, `2w`), a
/// date in local time, or a date and time (local unless it names its offset). A date alone
/// stands for its first second, or for its last when it is `end_of_day`.
fn parse_time(text: &str, now: i64, end_of_day: bool) -> AppResult<i64> {
    let invalid = || {
        AppError::invalid(format!(
            "{text} is not a time: give a date as in 2026-11-01, a date and time as in \
             2026-11-01T08:30:00Z, or an age as in 30m, 12h, 7d"
        ))
    };
    let trimmed = text.trim();
    if let Some(age) = parse_age(trimmed) {
        return Ok(now.saturating_sub(age));
    }
    if let Ok(moment) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Ok(moment.timestamp());
    }
    let local = |moment: NaiveDateTime| {
        Local
            .from_local_datetime(&moment)
            .earliest()
            .map(|moment| moment.timestamp())
            .ok_or_else(invalid)
    };
    for format in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(moment) = NaiveDateTime::parse_from_str(trimmed, format) {
            return local(moment);
        }
    }
    let date = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").map_err(|_| invalid())?;
    let moment = if end_of_day {
        date.and_hms_opt(23, 59, 59)
    } else {
        date.and_hms_opt(0, 0, 0)
    };
    local(moment.ok_or_else(invalid)?)
}

/// Seconds in an age such as `90s`, `30m`, `12h`, `7d` or `2w`.
fn parse_age(text: &str) -> Option<i64> {
    let unit = text.chars().last()?;
    let seconds_each: i64 = match unit {
        's' => 1,
        'm' => 60,
        'h' => 60 * 60,
        'd' => 24 * 60 * 60,
        'w' => 7 * 24 * 60 * 60,
        _ => return None,
    };
    let count: i64 = text[..text.len() - unit.len_utf8()].parse().ok()?;
    (count >= 0)
        .then(|| count.checked_mul(seconds_each))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_take_units_in_powers_of_1024() {
        assert_eq!(parse_size("0").unwrap(), 0);
        assert_eq!(parse_size("1536").unwrap(), 1536);
        assert_eq!(parse_size("500K").unwrap(), 500 * 1024);
        assert_eq!(parse_size("100m").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_size("2G").unwrap(), 2 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1.5MiB").unwrap(), 1_572_864);
        assert_eq!(parse_size(" 3 kb ").unwrap(), 3 * 1024);
        assert_eq!(parse_size("1T").unwrap(), 1 << 40);
        for wrong in [
            "",
            "big",
            "-1",
            "10X",
            "1e3",
            "99999999999999999999T",
            "1.2.3M",
        ] {
            assert!(parse_size(wrong).is_err(), "{wrong:?}");
        }
    }

    #[test]
    fn ages_count_back_from_now() {
        let now = 1_700_000_000;
        assert_eq!(parse_time("90s", now, false).unwrap(), now - 90);
        assert_eq!(parse_time("30m", now, false).unwrap(), now - 1_800);
        assert_eq!(parse_time("12h", now, false).unwrap(), now - 43_200);
        assert_eq!(parse_time("7d", now, true).unwrap(), now - 604_800);
        assert_eq!(parse_time("2w", now, false).unwrap(), now - 1_209_600);
    }

    #[test]
    fn times_with_an_offset_are_exact() {
        assert_eq!(
            parse_time("2023-11-14T22:13:20Z", 0, false).unwrap(),
            1_700_000_000
        );
        assert_eq!(
            parse_time("2023-11-15T00:13:20+02:00", 0, false).unwrap(),
            1_700_000_000
        );
    }

    #[test]
    fn a_date_alone_is_a_whole_day_in_local_time() {
        let start = parse_time("2026-11-01", 0, false).unwrap();
        let end = parse_time("2026-11-01", 0, true).unwrap();
        assert_eq!(end - start, 24 * 60 * 60 - 1);
        let morning = parse_time("2026-11-01 08:30", 0, false).unwrap();
        assert_eq!(morning - start, 8 * 3600 + 30 * 60);
        assert_eq!(
            parse_time("2026-11-01T08:30:00", 0, false).unwrap(),
            morning
        );
    }

    #[test]
    fn anything_else_is_not_a_time() {
        for wrong in [
            "",
            "yesterday",
            "7x",
            "-3d",
            "2026-13-01",
            "01/11/2026",
            "d",
        ] {
            assert!(parse_time(wrong, 0, false).is_err(), "{wrong:?}");
        }
    }
}
