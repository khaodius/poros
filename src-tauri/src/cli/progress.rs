//! Progress on standard error: a line that rewrites itself on a terminal, plain lines now and
//! then otherwise, or one JSON object per line for programs.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::args::ProgressMode;
use crate::format::{format_duration, format_size};

/// Plain lines, when standard error is not a terminal, come this far apart.
const TEXT_EVERY: Duration = Duration::from_secs(5);
const JSON_EVERY: Duration = Duration::from_secs(1);
/// The speed averages over this long.
const SPEED_WINDOW: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub files_done: u64,
    /// As far as known; folders still being listed add to it.
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub failed: u64,
}

pub struct Reporter {
    mode: ProgressMode,
    terminal: bool,
    started: Instant,
    last_shown: Option<Instant>,
    samples: Vec<(Instant, u64)>,
    drawn: bool,
}

impl Reporter {
    pub fn new(mode: ProgressMode) -> Self {
        Self {
            mode,
            terminal: std::io::stderr().is_terminal(),
            started: Instant::now(),
            last_shown: None,
            samples: Vec::new(),
            drawn: false,
        }
    }

    pub fn update(&mut self, snapshot: Snapshot) {
        let now = Instant::now();
        self.samples.push((now, snapshot.bytes_done));
        self.samples
            .retain(|(when, _)| now.duration_since(*when) <= SPEED_WINDOW);
        let interval = match self.mode {
            ProgressMode::None | ProgressMode::Auto => return,
            ProgressMode::Text if self.terminal => Duration::from_millis(200),
            ProgressMode::Text => TEXT_EVERY,
            ProgressMode::Json => JSON_EVERY,
        };
        let nothing_known = snapshot.files_total == 0 && self.mode == ProgressMode::Text;
        if nothing_known
            || self
                .last_shown
                .is_some_and(|shown| now.duration_since(shown) < interval)
        {
            return;
        }
        self.last_shown = Some(now);
        self.show(snapshot, now);
    }

    /// Clears the line on a terminal, so the result prints on a clean one.
    pub fn finish(&mut self) {
        if self.drawn && self.terminal {
            let mut stderr = std::io::stderr().lock();
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
        }
        self.drawn = false;
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn speed(&self) -> u64 {
        match (self.samples.first(), self.samples.last()) {
            (Some((first_time, first_bytes)), Some((last_time, last_bytes)))
                if last_time > first_time =>
            {
                let seconds = last_time.duration_since(*first_time).as_secs_f64();
                (last_bytes.saturating_sub(*first_bytes) as f64 / seconds) as u64
            }
            _ => 0,
        }
    }

    fn show(&mut self, snapshot: Snapshot, now: Instant) {
        let speed = self.speed();
        let mut stderr = std::io::stderr().lock();
        if self.mode == ProgressMode::Json {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Line {
                r#type: &'static str,
                #[serde(flatten)]
                snapshot: Snapshot,
                bytes_per_second: u64,
                elapsed_millis: u64,
            }
            let line = Line {
                r#type: "progress",
                snapshot,
                bytes_per_second: speed,
                elapsed_millis: now.duration_since(self.started).as_millis() as u64,
            };
            if let Ok(text) = serde_json::to_string(&line) {
                let _ = writeln!(stderr, "{text}");
            }
            return;
        }
        let mut line = format!(
            "{}/{} files  {} of {}",
            snapshot.files_done,
            snapshot.files_total,
            format_size(snapshot.bytes_done),
            format_size(snapshot.bytes_total)
        );
        if speed > 0 {
            line.push_str(&format!("  {}/s", format_size(speed)));
            let remaining = snapshot.bytes_total.saturating_sub(snapshot.bytes_done);
            if remaining > 0 {
                let seconds = remaining / speed.max(1);
                line.push_str(&format!(
                    "  {} left",
                    format_duration(Duration::from_secs(seconds))
                ));
            }
        }
        if snapshot.failed > 0 {
            line.push_str(&format!("  {} failed", snapshot.failed));
        }
        if self.terminal {
            let _ = write!(stderr, "\r\x1b[2K{line}");
            self.drawn = true;
        } else {
            let _ = writeln!(stderr, "{line}");
        }
        let _ = stderr.flush();
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_comes_from_the_recent_samples() {
        let mut reporter = Reporter::new(ProgressMode::None);
        let start = Instant::now();
        reporter.samples = vec![(start, 0), (start + Duration::from_secs(2), 4_000_000)];
        assert_eq!(reporter.speed(), 2_000_000);
        reporter.samples.truncate(1);
        assert_eq!(reporter.speed(), 0);
    }
}
