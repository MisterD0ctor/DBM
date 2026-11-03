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
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
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
    let file = open(&file()).and_then(|file| {
        let (lines, heard) = mpsc::channel();
        std::thread::Builder::new()
            .name("dbm-log".into())
            .spawn(move || write(file, &heard))
            .ok()?;
        Some(lines)
    });
    if log::set_boxed_logger(Box::new(Logger {
        file,
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

/// What the file's thread is handed.
enum Line {
    Text(String),
    /// Answered once everything handed over before it is written.
    Flush(SyncSender<()>),
}

/// The file's thread. Lines are written here rather than where they are
/// logged, which is often the frame path, where a slow or network home folder
/// would hold up the picture. Line-buffered, so a crash loses at most the
/// lines not yet handed over.
fn write(mut file: LineWriter<File>, lines: &Receiver<Line>) {
    let mut written = 0;
    for line in lines {
        match line {
            Line::Text(text) if written < MOST => {
                if file.write_all(text.as_bytes()).is_ok() {
                    written += text.len() as u64;
                }
            }
            Line::Text(_) => {}
            Line::Flush(done) => {
                let _ = file.flush();
                let _ = done.send(());
            }
        }
    }
}

struct Logger {
    file: Option<Sender<Line>>,
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
        let Some(file) = &self.file else {
            return;
        };
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
        let _ = file.send(Line::Text(line));
    }

    /// Waits until everything logged so far is in the file, as the end of
    /// a run must before the process takes the thread down with it.
    fn flush(&self) {
        let Some(file) = &self.file else {
            return;
        };
        let (done, written) = mpsc::sync_channel(1);
        if file.send(Line::Flush(done)).is_ok() {
            let _ = written.recv();
        }
    }
}
