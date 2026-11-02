//! Where the player's own lines go: to stderr as `dbm: …`, and to a file
//! beside the settings, since a Windows release has no console to print to.
//!
//! `DBM_TRACE` turns on the `debug` lines, which say what each part of the
//! player is doing as it does it. Its libraries' warnings go to the file.
//! The harness's reports and the per-frame instrumentation stay on stderr
//! alone: they are output for whoever ran them, not a record of the run.

use std::fs::File;
use std::io::{LineWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

/// How much of one run the file keeps. The previous run's is kept beside it.
const MOST: u64 = 4 * 1024 * 1024;

/// Send `log`'s lines here for the rest of the run. Call before anything
/// logs, which is first thing in `main`.
pub fn install() {
    let level = if crate::diagnostics::trace() {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let file = open(&file());
    if log::set_boxed_logger(Box::new(Logger {
        file: file.map(|file| Mutex::new(Kept { file, written: 0 })),
        started: Instant::now(),
    }))
    .is_ok()
    {
        log::set_max_level(level);
    }
}

/// Where this run's lines are kept. A run nobody is watching keeps to its
/// own, as it does for the settings.
fn file() -> PathBuf {
    let name = if crate::run_mode::unattended() {
        "dbm.test.log"
    } else {
        "dbm.log"
    };
    dbm_library::paths::app_data_dir().join(name)
}

/// The file for this run, with the last run's moved aside, or `None` where
/// there is nowhere to write it, which costs only the file.
fn open(path: &Path) -> Option<LineWriter<File>> {
    let _ = std::fs::rename(path, path.with_extension("previous.log"));
    File::create(path).ok().map(LineWriter::new)
}

struct Kept {
    /// Line-buffered, so a crash loses at most the line being written.
    file: LineWriter<File>,
    written: u64,
}

struct Logger {
    file: Option<Mutex<Kept>>,
    started: Instant,
}

/// Whether a line is the player's own rather than one of its libraries':
/// from this crate or one of the `dbm_` crates under it.
fn own(metadata: &log::Metadata) -> bool {
    metadata.target().starts_with("dbm_")
}

impl log::Log for Logger {
    /// The libraries' warnings and errors, but not their chatter, which
    /// `DBM_TRACE` is not asking about.
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
            && (own(metadata) || metadata.level() <= log::Level::Warn)
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // A library's line goes to the file alone, under its own name, so
        // nothing on stderr says `dbm:` that the player didn't.
        let own = own(record.metadata());
        if own {
            eprintln!("dbm: {}", record.args());
        }
        let Some(kept) = &self.file else {
            return;
        };
        let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
        if kept.written >= MOST {
            return;
        }
        let at = self.started.elapsed().as_secs_f64();
        let level = record.level();
        let line = if own {
            format!("{at:9.3} {level:<5} {}\n", record.args())
        } else {
            format!(
                "{at:9.3} {level:<5} [{}] {}\n",
                record.target(),
                record.args()
            )
        };
        if kept.file.write_all(line.as_bytes()).is_ok() {
            kept.written += line.len() as u64;
        }
    }

    fn flush(&self) {
        if let Some(kept) = &self.file {
            let _ = kept
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .file
                .flush();
        }
    }
}
