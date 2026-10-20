//! What a file says about itself before anyone plays it.
//!
//! mpv learns a file's length and its embedded title only by opening it, so
//! this asks ffmpeg, which describes a file without playing it, for the rest
//! of the playlist.
//!
//! `ffmpeg -i` with no output reads the container header and exits, in about
//! 40ms for a local file, so a season takes under a second and a folder of
//! two hundred episodes fills in over a few seconds.
//!
//! Its own thread rather than the worker, as the preview's is, so a large
//! folder doesn't hold the track menu empty. One file at a time in playlist
//! order, so the rows on screen when the panel opens arrive first.
//!
//! Results are kept, so a folder is scanned once: lengths in `durations.json`
//! beside those recorded as files play, and titles in `titles.json`, where an
//! empty string means "looked, and there is none", so untitled files aren't
//! asked again every launch.
//!
//! A generation counter stops a superseded scan between files, as it stops a
//! superseded atlas. A file ffmpeg is already reading is finished first,
//! since a probe is short.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::library::cache::PathMap;
use crate::library::ffmpeg;
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

/// How long a scan gathers what it has read before handing it over.
const BATCH: Duration = Duration::from_secs(1);

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
    let lengths = crate::library::durations::load();
    let titles = TITLES.load();

    // Everything already known goes back in one delivery before any ffmpeg
    // starts, so a folder opened again is complete as soon as its list is.
    let (known, unknown): (Vec<String>, Vec<String>) = paths
        .into_iter()
        .partition(|path| lengths.get(path).is_some_and(|s| *s > 0.0) && titles.contains_key(path));
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
    // Without ffmpeg the rows stay as mpv alone can fill them.
    let Some(ffmpeg) = ffmpeg::path() else {
        return;
    };
    ffmpeg::announce(&ffmpeg);

    // Handed over a second's worth at a time rather than file by file, since
    // each delivery means rows to rewrite in the interface.
    let mut batch: Vec<Found> = Vec::new();
    let mut handed = Instant::now();
    for path in unknown {
        if !current() {
            return;
        }
        let Some(found) = probe(&ffmpeg, &path) else {
            continue;
        };
        if found.seconds > 0.0 {
            crate::library::durations::record(&path, found.seconds);
        }
        record_title(&path, found.title.as_deref());
        batch.push(found);
        if handed.elapsed() >= BATCH {
            if !reporter.send(Completion::Probed(std::mem::take(&mut batch))) {
                return;
            }
            handed = Instant::now();
        }
    }
    if !batch.is_empty() && current() {
        reporter.send(Completion::Probed(batch));
    }
}

/// Ask ffmpeg about one file. **Blocking**: spawns a process and waits.
///
/// `None` for anything that isn't a file on disk, since a stream URL would
/// mean a network request, and mpv reports those once they play.
fn probe(ffmpeg: &Path, path: &str) -> Option<Found> {
    if !Path::new(path).is_file() {
        return None;
    }
    let report = ffmpeg::describe(ffmpeg, Path::new(path))?;
    Some(Found {
        path: path.to_string(),
        seconds: ffmpeg::parse_duration(&report).unwrap_or(0.0),
        title: parse_title(&report),
    })
}

/// The container's `title` tag, from ffmpeg's description of a file.
///
/// Only the container's, or a subtitle stream titled `English` would name the
/// episode. ffmpeg prints the container block before `Duration:` and every
/// stream block after it.
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
///
/// One scan writes at a time in practice, but a superseded one can still be
/// finishing its last file as the next starts.
///
/// Versioned as caches are (see `cache`). 1: the container's `title` tag.
static TITLES: PathMap<String> = PathMap::cache("titles.json", 1);

/// **Blocking.**
fn record_title(path: &str, title: Option<&str>) {
    TITLES.set(path, title.unwrap_or("").to_string());
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
