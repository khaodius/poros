//! Log lines on standard error. Without `-v` only warnings show, and those logged while
//! connecting wait until the connection succeeds: a failure reaches the user as the command's
//! result, and logging it as well would say everything twice.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

static HOLDING: AtomicBool = AtomicBool::new(false);
static HELD: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct StderrLogger {
    verbosity: u8,
    terminal: bool,
}

pub fn install(verbosity: u8, quiet: bool) {
    if quiet {
        return;
    }
    let logger = Box::leak(Box::new(StderrLogger {
        verbosity,
        terminal: std::io::stderr().is_terminal(),
    }));
    let level = match verbosity {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}

/// Keeps warnings back until `release` or `silence`. With `-v` everything shows at once.
pub fn hold() {
    HOLDING.store(true, Ordering::SeqCst);
}

pub fn release() {
    HOLDING.store(false, Ordering::SeqCst);
    for line in HELD.lock().unwrap().drain(..) {
        eprintln!("{line}");
    }
}

/// Drops what was held and keeps holding, for when the run is ending anyway: the connection
/// failed, or is being closed, which logs news of its own end.
pub fn silence() {
    HOLDING.store(true, Ordering::SeqCst);
    HELD.lock().unwrap().clear();
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let ours = metadata.target().starts_with("poros");
        match self.verbosity {
            0 => metadata.level() == Level::Warn && ours,
            1 => metadata.level() <= Level::Info && ours,
            2 => metadata.level() <= Level::Debug && ours,
            _ => true,
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let prefix = match record.level() {
            Level::Error => "error: ",
            Level::Warn => "warning: ",
            Level::Info => "",
            Level::Debug | Level::Trace => "debug: ",
        };
        let line = format!("poros-cli: {prefix}{}", record.args());
        if self.verbosity == 0 && HOLDING.load(Ordering::SeqCst) {
            HELD.lock().unwrap().push(line);
            return;
        }
        // Clears a progress line first, which is redrawn on its next update.
        let clear = if self.terminal { "\r\x1b[2K" } else { "" };
        eprintln!("{clear}{line}");
    }

    fn flush(&self) {}
}
