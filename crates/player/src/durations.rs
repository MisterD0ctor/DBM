//! How long each file is, and how far into it you got.
//!
//! mpv knows a file's duration only once it has opened it, which is too late
//! for a playlist that wants to say how long the other nine episodes are. So
//! the duration of everything played is kept in `durations.json` — the same
//! file, in the same place, as the Tauri build's, so a library's worth of
//! accumulated lengths survives the rewrite.
//!
//! The position comes from mpv's own watch-later files rather than a copy of
//! our own. That matters: the number a row shows is then *the* resume point,
//! the one mpv will actually use, and not something that can quietly drift
//! from it. The price is having to find those files, which mpv names by the
//! MD5 of the path it was given — hence the one dependency this module has.
//!
//! Everything here touches the disk, so everything here runs on the worker.

use std::collections::HashMap;
use std::path::PathBuf;

use md5::Digest;

/// The cache, as the Tauri build wrote it: a flat object of path to seconds.
pub type Cache = HashMap<String, f64>;

static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn cache_file() -> PathBuf {
    crate::paths::app_data_dir().join("durations.json")
}

/// **Blocking.** Worker thread only.
///
/// A missing or unreadable file is an empty cache, not an error: the first
/// run of the player has no cache, and neither does one whose file somebody
/// deleted. Both mean the same thing — nothing is known yet.
pub fn load() -> Cache {
    std::fs::read_to_string(cache_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remember one file's length.
///
/// **Blocking.** Worker thread only.
///
/// Re-reads before writing rather than holding the map in memory, because two
/// things write here — every file that finishes loading — and the file is a
/// few tens of kilobytes at worst. Reading first also means a cache edited by
/// hand, or written by the other build, is merged rather than clobbered.
pub fn record(path: &str, seconds: f64) {
    if seconds <= 0.0 || path.is_empty() {
        return;
    }
    // Two threads write here now: the worker, as files play, and the probe,
    // as a folder is scanned. Each rewrites the whole file from what it just
    // read, so without this one of two overlapping writes is simply lost.
    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut cache = load();
    if cache.get(path).copied() == Some(seconds) {
        return;
    }
    cache.insert(path.to_string(), seconds);
    let file = cache_file();
    let Ok(json) = serde_json::to_string(&cache) else {
        return;
    };
    if let Err(e) = std::fs::write(&file, json) {
        eprintln!("dbm: could not write {}: {e}", file.display());
    }
}

/// What is known about one playlist entry.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Progress {
    /// Length in seconds, or zero when this file has never been played here.
    pub seconds: f64,
    /// How far the resume point is through it, 0..1. Zero when there is no
    /// saved position, or no duration to measure it against.
    pub fraction: f32,
    /// The resume point itself, in seconds, zero when there is none.
    ///
    /// Kept because `fraction` cannot be worked out without a length, and a
    /// length can now arrive after this was read — from a probe of a file
    /// that has a resume point but was never played long enough here for mpv
    /// to report how long it is.
    pub start: f64,
    /// Watched to its end, or into its closing credits, and not started
    /// again since.
    ///
    /// A record of the player's own, because nothing mpv keeps can say it:
    /// mpv deletes a position once a file is watched through, and a scan
    /// gives every file a length, so a finished episode and one never started
    /// look exactly alike on disk.
    pub finished: bool,
}

/// Look up every entry in a playlist at once.
///
/// **Blocking.** Worker thread only — it reads one small file per entry.
///
/// One pass over the cache for the whole list, because the caller already has
/// the whole list and reading the cache ten times would be ten times the work
/// for the same answer.
pub fn of(paths: &[String]) -> Vec<Progress> {
    let cache = load();
    let finished = load_finished();
    let watch_later = crate::paths::watch_later_dir();
    paths
        .iter()
        .map(|path| {
            let seconds = cache.get(path).copied().unwrap_or(0.0);
            let start = resume_point(&watch_later, path).unwrap_or(0.0);
            Progress {
                seconds,
                start,
                fraction: if seconds > 0.0 && start > 0.0 {
                    (start / seconds).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                },
                finished: is_finished(&finished, path, start),
            }
        })
        .collect()
}

/// Where mpv would resume this file, in seconds.
///
/// mpv names each watch-later file by the uppercase MD5 of the path it was
/// opened with, and writes `start=<seconds>` inside. Both halves are mpv's
/// convention rather than ours, which is the point — this reads what mpv will
/// read.
pub fn resume_point(dir: &std::path::Path, path: &str) -> Option<f64> {
    let digest = md5::Md5::digest(path.as_bytes());
    let name: String = digest.iter().map(|b| format!("{b:02X}")).collect();
    let contents = std::fs::read_to_string(dir.join(name)).ok()?;
    contents
        .lines()
        .find_map(|line| line.strip_prefix("start="))
        .and_then(|value| value.trim().parse().ok())
}

/// The name mpv gives this path's watch-later file.
///
/// Exposed for the harness: whether our hashing agrees with mpv's is the one
/// thing in this module that cannot be checked by reading the code, since it
/// depends on the exact string mpv hashed.
pub fn watch_later_name(path: &str) -> String {
    md5::Md5::digest(path.as_bytes())
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

// ---------------------------------------------------------------------------
// Watched to the end
// ---------------------------------------------------------------------------

/// Where each finished file's ending began, in seconds: the start of its
/// credits, or its length when it simply ran out.
pub type Finished = HashMap<String, f64>;

static FINISHED_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// mpv checkpoints the position every ten seconds, so a file left in its
/// credits can hold a resume point a little before the moment they were
/// noticed. This much short of the ending is still the same viewing.
const CHECKPOINT_SLACK: f64 = 15.0;

fn finished_file() -> PathBuf {
    crate::paths::app_data_dir().join("finished.json")
}

/// **Blocking.** Worker thread only.
///
/// Missing or unreadable is empty, for the same reason as the durations:
/// nothing has been finished here yet.
pub fn load_finished() -> Finished {
    std::fs::read_to_string(finished_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remember that a file reached its ending, and where that ending began.
///
/// **Blocking.** Worker thread only. Re-read before writing, as `record` is.
pub fn record_finished(path: &str, at: f64) {
    if path.is_empty() {
        return;
    }
    let _guard = FINISHED_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut finished = load_finished();
    if finished.get(path).copied() == Some(at) {
        return;
    }
    finished.insert(path.to_string(), at);
    let file = finished_file();
    let Ok(json) = serde_json::to_string(&finished) else {
        return;
    };
    if let Err(e) = std::fs::write(&file, json) {
        eprintln!("dbm: could not write {}: {e}", file.display());
    }
}

/// Whether a file counts as finished, given where mpv would resume it now.
///
/// A resume point short of the ending is a viewing started again since, and
/// the row should say how far into that one you are rather than that it was
/// once seen through.
pub fn is_finished(finished: &Finished, path: &str, start: f64) -> bool {
    finished
        .get(path)
        .is_some_and(|&at| start <= 0.0 || start >= at - CHECKPOINT_SLACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_mpvs_own_spelling() {
        // Upper-case hex of the MD5 of the path, no separators, no extension:
        // mpv's `mp_get_playback_resume_config_filename`. The vector is the
        // well-known MD5 of "abc", so a broken digest shows up here rather
        // than as an empty playlist row.
        assert_eq!(watch_later_name("abc"), "900150983CD24FB0D6963F7D28E17F72");
    }

    /// Drives the whole lookup against a real directory: a cache file, a
    /// watch-later file named the way mpv names it, and the three cases a
    /// playlist actually contains — watched part-way, known but never
    /// started, and never seen at all.
    #[test]
    fn a_row_knows_its_length_and_how_far_in_it_is() {
        let dir = std::env::temp_dir().join(format!("dbm-durations-{}", std::process::id()));
        let app = dir.join("Death by MPV");
        std::fs::create_dir_all(app.join("watch_later")).unwrap();
        // `app_data_dir` is where both files live, and it is read from the
        // environment — which is also how the harness keeps test runs out of
        // the real one.
        std::env::set_var("APPDATA", &dir);

        std::fs::write(
            app.join("durations.json"),
            r#"{"C:/films/seen.mkv":1200.0,"C:/films/fresh.mkv":600.0}"#,
        )
        .unwrap();
        std::fs::write(
            app.join("watch_later")
                .join(watch_later_name("C:/films/seen.mkv")),
            "start=300.0\nvid=1\n",
        )
        .unwrap();

        let found = of(&[
            "C:/films/seen.mkv".to_string(),
            "C:/films/fresh.mkv".to_string(),
            "C:/films/never.mkv".to_string(),
        ]);
        assert_eq!(found[0].seconds, 1200.0);
        assert!((found[0].fraction - 0.25).abs() < 1e-6);
        // Known length, no saved position: a full-length row with no fill.
        assert_eq!(found[1].seconds, 600.0);
        assert_eq!(found[1].fraction, 0.0);
        // Never played here, so nothing is known and nothing is shown.
        assert_eq!(found[2], Progress::default());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_viewing_started_again_is_not_finished() {
        let finished = Finished::from([("C:/films/seen.mkv".to_string(), 1200.0)]);
        // mpv deleted the position at the end.
        assert!(is_finished(&finished, "C:/films/seen.mkv", 0.0));
        // Left in the credits, checkpointed a few seconds before they were
        // noticed.
        assert!(is_finished(&finished, "C:/films/seen.mkv", 1190.0));
        // Watching it again, a quarter of the way in.
        assert!(!is_finished(&finished, "C:/films/seen.mkv", 300.0));
        assert!(!is_finished(&finished, "C:/films/fresh.mkv", 0.0));
    }
}

// ---------------------------------------------------------------------------
// Noticing a duration worth keeping
// ---------------------------------------------------------------------------

/// Writes down how long the playing file is, once mpv has worked it out.
///
/// Lives on the frame path and does nothing there but compare two values: the
/// write itself goes to the worker, because it reads and rewrites a file.
#[derive(Default)]
pub struct Recorder {
    /// What was last handed to the worker, so the same fact is not written
    /// twice — which for a file playing at 60 frames a second would be a
    /// rewrite per frame.
    last: Option<(String, f64)>,
    /// The file playing, its length, and whether it was within a moment of
    /// its end. Autoplay moves straight past the end of a file and mpv never
    /// reports reaching it, so this is how that file is still counted.
    playing: Option<(String, f64, bool)>,
    /// The last file written down as finished, so it is written once.
    finished: Option<String>,
}

/// How near the end a file autoplay moved on from must have been: a frame or
/// two short of the last one, not a film abandoned in its final minute.
const NEAR_END: f64 = 2.0;

impl Recorder {
    pub fn poll(&mut self, worker: &crate::worker::Worker, player: &crate::state::PlayerState) {
        self.watch_ending(worker, player);
        let Some(path) = player.path.as_deref() else {
            return;
        };
        let seconds = player.duration;
        // Zero until the file is open and demuxed, which is most of the first
        // moment of every file.
        if seconds <= 0.0 {
            return;
        }
        if self
            .last
            .as_ref()
            .is_some_and(|(p, s)| p == path && *s == seconds)
        {
            return;
        }
        self.last = Some((path.to_string(), seconds));
        let path = path.to_string();
        worker.run(move |_mpv| record(&path, seconds));
    }

    /// Notices a file reaching its ending: its credits, its last frame, or
    /// its last moments before autoplay took the next one.
    fn watch_ending(&mut self, worker: &crate::worker::Worker, player: &crate::state::PlayerState) {
        if self.playing.as_ref().map(|p| p.0.as_str()) != player.path.as_deref() {
            if let Some((path, duration, true)) = self.playing.take() {
                self.finish(worker, path, duration);
            }
            self.playing = player.path.clone().map(|p| (p, 0.0, false));
        }
        let Some(path) = player.path.as_deref() else {
            return;
        };
        // Both halves of the file's own numbers, and the second half of it:
        // for a frame after a new file opens, the flags and the position can
        // still be the last one's.
        let into_it = player.duration > 0.0 && player.time_pos >= player.duration * 0.5;
        if let Some(playing) = self.playing.as_mut() {
            playing.1 = player.duration;
            playing.2 = into_it && player.time_pos >= player.duration - NEAR_END;
        }
        let ended = player.credits_rolling()
            || (player.eof_reached && player.time_pos >= player.duration - NEAR_END);
        if into_it && ended && self.finished.as_deref() != Some(path) {
            self.finish(worker, path.to_string(), player.time_pos);
        }
    }

    fn finish(&mut self, worker: &crate::worker::Worker, path: String, at: f64) {
        if self.finished.as_deref() == Some(path.as_str()) {
            return;
        }
        self.finished = Some(path.clone());
        worker.run(move |_mpv| record_finished(&path, at));
    }
}
