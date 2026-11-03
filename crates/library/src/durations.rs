//! How long each file is, and how far into it you got.
//!
//! mpv knows a file's duration only once it has opened it, too late for a
//! playlist that wants to say how long the other episodes are. So the length
//! of everything played is kept in `durations.json`, the same file and place
//! as the Tauri build's, so the lengths it gathered carry over.
//!
//! The position comes from mpv's own watch-later files rather than a copy, so
//! a row shows the resume point mpv will actually use. Finding them means the
//! MD5 of the path mpv was given, hence this module's one dependency.
//!
//! Everything here touches the disk, so everything here runs on the worker.

use std::collections::HashMap;

use dbm_mpv::MediaPath;
use md5::Digest;

use crate::cache::PathMap;

/// Each file's length, in seconds.
pub type Cache = HashMap<MediaPath, f64>;

/// Versioned as caches are (see `cache`). 1: the container's own duration.
pub(crate) static LENGTHS: PathMap<f64> = PathMap::cache("durations.json", 1);

/// Every length known. **Blocking.** Worker thread only.
pub fn load() -> Cache {
    LENGTHS.load()
}

/// Remember one file's length.
///
/// **Blocking.** Worker thread only. Two threads write here: the worker, as
/// files play, and the probe, as a folder is scanned.
pub fn record(path: &MediaPath, seconds: f64) {
    record_many([(path.clone(), seconds)]);
}

/// Remember several files' lengths in one write. **Blocking.** Worker thread
/// only.
pub fn record_many(lengths: impl IntoIterator<Item = (MediaPath, f64)>) {
    LENGTHS.set_many(lengths.into_iter().filter(|(_, seconds)| *seconds > 0.0));
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
    /// Kept because `fraction` needs a length, which can arrive after this
    /// was read, from a probe of a file never played long enough here for
    /// mpv to report its length.
    pub start: f64,
    /// Watched to its end, or into its closing credits, and not started
    /// again since.
    ///
    /// A record of the player's own, because mpv deletes a position once a
    /// file is watched through, and a scan gives every file a length, so a
    /// finished episode and one never started look alike on disk.
    pub finished: bool,
}

/// Look up every entry in a playlist at once.
///
/// **Blocking.** Worker thread only, since it reads one small file per entry.
/// One pass over the cache for the whole list, rather than one per entry.
pub fn of(paths: &[MediaPath]) -> Vec<Progress> {
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
/// opened with, and writes `start=<seconds>` inside. Both are mpv's
/// convention, so this reads what mpv will read.
pub fn resume_point(dir: &std::path::Path, path: &MediaPath) -> Option<f64> {
    let contents = std::fs::read_to_string(dir.join(watch_later_name(path))).ok()?;
    contents
        .lines()
        .find_map(|line| line.strip_prefix("start="))
        .and_then(|value| value.trim().parse().ok())
}

/// The name mpv gives this path's watch-later file.
///
/// Exposed for the harness, since whether our hashing agrees with mpv's
/// depends on the exact string mpv hashed, which reading the code can't
/// check.
pub fn watch_later_name(path: &MediaPath) -> String {
    md5::Md5::digest(path.as_str().as_bytes())
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

// ---------------------------------------------------------------------------
// Watched to the end
// ---------------------------------------------------------------------------

/// Where each finished file's ending began, in seconds: the start of its
/// credits, or its length when it simply ran out.
pub type Finished = HashMap<MediaPath, f64>;

pub(crate) static FINISHED: PathMap<f64> = PathMap::record("finished.json");

/// mpv checkpoints the position every ten seconds, so a file left in its
/// credits can hold a resume point a little before the moment they were
/// noticed. This much short of the ending is still the same viewing.
const CHECKPOINT_SLACK: f64 = 15.0;

/// Every file watched to its end. **Blocking.** Worker thread only.
pub fn load_finished() -> Finished {
    FINISHED.load()
}

/// Remember that a file reached its ending, and where that ending began.
///
/// **Blocking.** Worker thread only.
pub fn record_finished(path: &MediaPath, at: f64) {
    FINISHED.set(path, at);
}

/// Whether a file counts as finished, given where mpv would resume it now.
///
/// A resume point short of the ending is a viewing started again since, and
/// the row should say how far into that one you are.
pub fn is_finished(finished: &Finished, path: &MediaPath, start: f64) -> bool {
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
        assert_eq!(
            watch_later_name(&"abc".into()),
            "900150983CD24FB0D6963F7D28E17F72"
        );
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
        // `app_data_dir` is where both files live.
        crate::paths::set_test_base(&dir);

        std::fs::write(
            app.join("durations.json"),
            r#"{"C:/films/seen.mkv":1200.0,"C:/films/fresh.mkv":600.0}"#,
        )
        .unwrap();
        std::fs::write(
            app.join("watch_later")
                .join(watch_later_name(&"C:/films/seen.mkv".into())),
            "start=300.0\nvid=1\n",
        )
        .unwrap();

        let found = of(&[
            "C:/films/seen.mkv".into(),
            "C:/films/fresh.mkv".into(),
            "C:/films/never.mkv".into(),
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
        let seen = MediaPath::from("C:/films/seen.mkv");
        let finished = Finished::from([(seen.clone(), 1200.0)]);
        // mpv deleted the position at the end.
        assert!(is_finished(&finished, &seen, 0.0));
        // Left in the credits, checkpointed a few seconds before they were
        // noticed.
        assert!(is_finished(&finished, &seen, 1190.0));
        // Watching it again, a quarter of the way in.
        assert!(!is_finished(&finished, &seen, 300.0));
        assert!(!is_finished(&finished, &"C:/films/fresh.mkv".into(), 0.0));
    }
}
