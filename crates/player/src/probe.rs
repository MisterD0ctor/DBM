//! What a file says about itself before anyone plays it.
//!
//! mpv learns a file's length and its embedded title only by opening it, so a
//! playlist used to know both for the one episode playing and for whatever
//! had been played before, and nothing for the rest: `S02E04` with no running
//! time, beside `S02E01 · The Chosen One  0:24:10`. This asks ffmpeg instead,
//! which will describe a file without playing it.
//!
//! `ffmpeg -i` with no output reads the container header and exits. It is
//! about 40ms for a local file, most of it process start, and it never
//! decodes a frame — so a season is under a second, and a folder of two
//! hundred episodes fills in over a few seconds while the rows it has not
//! reached yet stay as they were.
//!
//! Its own thread rather than the worker, for the reason the preview atlas
//! has one: the worker is what keeps the track lists current, and a scan of a
//! large folder behind it would hold the subtitle menu empty until it was
//! done. One file at a time, in playlist order, so the rows near the top —
//! the ones on screen when the panel opens — are the first to arrive.
//!
//! Results are kept, so a folder is scanned once. Lengths go into the same
//! `durations.json` the player already records into as files play, and titles
//! into `titles.json` beside it, where an empty string means "looked, and
//! there is none" — without that, every untitled file would be asked again on
//! every launch.
//!
//! A generation counter stops a superseded scan the same way it stops a
//! superseded atlas: opening another folder abandons the old one between
//! files. A file ffmpeg is already reading is finished first, since a probe
//! is short and cannot be interrupted anyway.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::worker::{Completion, Reporter, Worker};

/// One file, described.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: String,
    /// Zero when the container does not say.
    pub seconds: f64,
    /// The container's own title tag, if it has one.
    pub title: Option<String>,
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Keeps one scan running for whatever the playlist currently is.
#[derive(Default)]
pub struct Scan {
    /// The paths the running scan was started for, so a list read that
    /// changed only the tracks does not start the same scan over.
    requested: Vec<String>,
}

impl Scan {
    /// Describe every entry in this playlist that is not already known.
    ///
    /// Called when a list read lands, not per frame. Compares, and returns at
    /// once when the paths are the ones already being scanned.
    pub fn request(&mut self, paths: &[String], worker: &Worker) {
        if paths == self.requested.as_slice() {
            return;
        }
        self.requested = paths.to_vec();
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        if paths.is_empty() {
            return;
        }
        let paths = paths.to_vec();
        let reporter = worker.reporter();
        let spawned = std::thread::Builder::new()
            .name("dbm-probe".into())
            .spawn(move || scan(paths, generation, reporter));
        if let Err(e) = spawned {
            eprintln!("dbm: could not start the playlist scan: {e}");
        }
    }
}

fn scan(paths: Vec<String>, generation: u64, reporter: Reporter) {
    let current = || GENERATION.load(Ordering::SeqCst) == generation;
    let lengths = crate::durations::load();
    let titles = load_titles();

    // Everything already known goes back in one delivery, before any ffmpeg
    // starts: a folder opened for the second time is complete the moment its
    // list is, rather than filling in row by row out of a cache.
    let (known, unknown): (Vec<String>, Vec<String>) = paths.into_iter().partition(|path| {
        lengths.get(path).is_some_and(|s| *s > 0.0) && titles.contains_key(path)
    });
    let known: Vec<Found> = known
        .into_iter()
        .map(|path| Found {
            seconds: lengths.get(&path).copied().unwrap_or(0.0),
            title: titles.get(&path).filter(|t| !t.is_empty()).cloned(),
            path,
        })
        .collect();
    if !known.is_empty() && !reporter.send(Completion::Probed(known)) {
        return;
    }

    if unknown.is_empty() || !current() {
        return;
    }
    // No ffmpeg is the same answer the preview gets: the feature is not
    // there, and the rows stay as mpv alone can fill them.
    let Some(ffmpeg) = crate::preview::ffmpeg_path() else {
        return;
    };
    crate::preview::announce(&ffmpeg);

    for path in unknown {
        if !current() {
            return;
        }
        let Some(found) = probe(&ffmpeg, &path) else {
            continue;
        };
        if found.seconds > 0.0 {
            crate::durations::record(&path, found.seconds);
        }
        record_title(&path, found.title.as_deref());
        if !reporter.send(Completion::Probed(vec![found])) {
            return;
        }
    }
}

/// Ask ffmpeg about one file. **Blocking** — spawns a process and waits.
///
/// `None` for anything that is not a file on disk: a stream URL would be a
/// network request to find out how long something is, which is not this
/// module's business, and mpv reports those itself once they play.
fn probe(ffmpeg: &Path, path: &str) -> Option<Found> {
    if !Path::new(path).is_file() {
        return None;
    }
    let out = crate::preview::command(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(path)
        .output()
        .ok()?;
    let report = String::from_utf8_lossy(&out.stderr);
    Some(Found {
        path: path.to_string(),
        seconds: crate::preview::parse_duration(&report).unwrap_or(0.0),
        title: parse_title(&report),
    })
}

/// The container's `title` tag, from ffmpeg's description of a file.
///
/// Only the container's. Streams carry metadata blocks of their own, and a
/// subtitle stream titled `English` would otherwise name the episode. ffmpeg
/// prints the container block before `Duration:` and every stream block
/// after it, which is the whole of the distinction.
pub fn parse_title(report: &str) -> Option<String> {
    let header = report.split("Duration:").next()?;
    header
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("title")
                .then(|| value.trim().to_string())
        })
        .filter(|title| !title.is_empty())
}

// ---------------------------------------------------------------------------
// The title cache
// ---------------------------------------------------------------------------

/// Path to title. An empty string is a file that was asked and has none.
type Titles = HashMap<String, String>;

fn titles_file() -> PathBuf {
    crate::paths::app_data_dir().join("titles.json")
}

/// **Blocking.** Missing or unreadable is empty, as for the duration cache.
fn load_titles() -> Titles {
    std::fs::read_to_string(titles_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Serialises read-modify-write on `titles.json`. One scan writes at a time
/// in practice, but a superseded one can still be finishing its last file as
/// the next starts, and two rewrites of the same file lose one of them.
static TITLES_WRITE: Mutex<()> = Mutex::new(());

/// **Blocking.**
fn record_title(path: &str, title: Option<&str>) {
    let _guard = TITLES_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut titles = load_titles();
    let value = title.unwrap_or("").to_string();
    if titles.get(path) == Some(&value) {
        return;
    }
    titles.insert(path.to_string(), value);
    let file = titles_file();
    let Ok(json) = serde_json::to_string(&titles) else {
        return;
    };
    if let Err(e) = std::fs::write(&file, json) {
        eprintln!("dbm: could not write {}: {e}", file.display());
    }
}

#[cfg(test)]
mod tests {
    use super::parse_title;

    /// What ffmpeg printed for a real file, trimmed.
    const REPORT: &str = "\
Input #0, matroska,webm, from 'C:/films/Show - 03.mkv':
  Metadata:
    title           : The Chosen One
    ENCODER         : Lavf59.33.100
  Duration: 00:24:10.00, start: 0.000000, bitrate: 186 kb/s
  Stream #0:0: Video: h264, yuv420p, 1920x1080
  Stream #0:1(eng): Subtitle: ass
    Metadata:
      title           : English
";

    #[test]
    fn the_title_is_the_containers_not_a_streams() {
        assert_eq!(parse_title(REPORT).as_deref(), Some("The Chosen One"));
    }

    #[test]
    fn a_stream_title_alone_names_nothing() {
        let untitled = REPORT.replace("    title           : The Chosen One\n", "");
        assert_eq!(parse_title(&untitled), None);
    }

    #[test]
    fn a_colon_inside_the_title_stays_in_it() {
        let colon = REPORT.replace("The Chosen One", "Part 2: The Return");
        assert_eq!(parse_title(&colon).as_deref(), Some("Part 2: The Return"));
    }

    #[test]
    fn windows_line_endings_are_not_part_of_the_title() {
        let crlf = REPORT.replace('\n', "\r\n");
        assert_eq!(parse_title(&crlf).as_deref(), Some("The Chosen One"));
    }

    #[test]
    fn an_empty_tag_is_no_title() {
        let empty = REPORT.replace("The Chosen One", "");
        assert_eq!(parse_title(&empty), None);
    }
}
