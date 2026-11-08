//! The films left unfinished, for the empty window to offer.
//!
//! Worked out from what mpv and the player already keep rather than a record
//! of its own, so it can't disagree with what mpv will do. The duration cache
//! lists the paths ever played here, and mpv keeps a watch-later file only
//! while a path has a position worth resuming. So the watch-later files among
//! those paths are the things left unfinished, newest first, and their
//! `start=` is where each one was left.

use std::collections::HashSet;

use dbm_mpv::MediaPath;

use crate::durations::Progress;
use crate::paths;

/// One file you stopped part-way through.
pub struct Resume {
    pub path: MediaPath,
    /// The show it is an episode of, if it is one. Kept apart from the title
    /// so the way in can give it a line of its own, leaving the episode room
    /// for its number.
    pub show: Option<String>,
    /// Named the way the bar would name it, less the show.
    pub title: String,
    /// How far in the resume point sits, 0..1.
    pub fraction: f32,
    /// How much of it is left, in seconds.
    pub seconds_left: f64,
    /// A frame from about there, when a seek-preview atlas was ever built.
    pub still: Option<crate::preview::Still>,
}

/// How many the way in offers at once: as many rows as leave it clear of the
/// window's edges at its smallest size.
pub const OFFERED: usize = 4;

/// The films you stopped part-way through, the one you left most recently
/// first, up to `most` of them, and no more than one episode of a show.
///
/// Three rows for one series say less than the same three rows holding
/// three shows, so a show is offered once, by the episode left most
/// recently: the one watching would carry on from. A film belongs to no
/// show, so every film is offered.
///
/// **Blocking.** Worker thread only: it stats a file per entry in the
/// duration cache, reads one watch-later file per candidate and lists the
/// folders they are in, and cuts a frame from each file's preview atlas.
pub fn unfinished(most: usize) -> Vec<Resume> {
    let dir = paths::watch_later_dir();
    let mut candidates: Vec<(std::time::SystemTime, MediaPath, f64)> = crate::durations::load()
        .into_iter()
        .filter(|(_, seconds)| *seconds > 0.0)
        .filter_map(|(path, seconds)| {
            let file = dir.join(crate::durations::watch_later_name(&path));
            let modified = std::fs::metadata(file).ok()?.modified().ok()?;
            Some((modified, path, seconds))
        })
        .collect();
    // Newest first, and the path breaking a tie, so two files saved in the
    // same instant never swap places between reads.
    candidates.sort_by(|(a, path, _), (b, other, _)| b.cmp(a).then_with(|| path.cmp(other)));

    // A watch-later file can outlive its video, mpv writes some with no
    // position, and one left in its credits has a position but isn't
    // unfinished, so a candidate is offered only if it still stands up.
    let finished = crate::durations::load_finished();
    let mut shows: HashSet<String> = HashSet::new();
    candidates
        .into_iter()
        .filter_map(|(_, path, seconds)| {
            let start = crate::durations::resume_point(&dir, &path)?;
            if start <= 0.0 || !path.as_path().is_file() {
                return None;
            }
            let progress = Progress {
                seconds: Some(seconds),
                start: Some(start),
                finished: crate::durations::is_finished(&finished, &path, Some(start)),
            };
            // A place that would start from the top anyway is not an offer to
            // carry on, so it is not offered.
            let Saved::Place(_) = Saved::of(&progress) else {
                return None;
            };
            let fraction = (start / seconds).clamp(0.0, 1.0) as f32;
            crate::playlist::notice_folder_of(path.as_path());
            let described = dbm_naming::described(path.as_str(), None);
            // The newest of a show comes first, so the offer for a show is
            // the first one met and the rest are passed over.
            if let Some(show) = described.show.as_deref()
                && !shows.insert(show.to_lowercase())
            {
                return None;
            }
            Some(Resume {
                seconds_left: (seconds - start).max(0.0),
                show: described.show,
                title: described.rest,
                still: crate::preview::still(&path, fraction),
                path,
                fraction,
            })
        })
        .take(most)
        .collect()
}

/// The one you stopped most recently, for a caller that has room for one.
///
/// **Blocking.** Worker thread only; see [`unfinished`].
pub fn last_watched() -> Option<Resume> {
    unfinished(1).pop()
}

/// A place saved this early is a file opened and left again rather than
/// one being watched, and it starts over rather than ask about seconds.
const WORTH_ASKING: f64 = 60.0;
/// Left this close to its end, a file has been watched to it in all but
/// name (its credits never heard, or its last checkpoint a little short of
/// them), and starts over as a finished one does. The last twentieth, but
/// never less than its last minute, since an eleven-minute episode's credits
/// run longer than a twentieth of it.
const ENDING_SHARE: f64 = 0.05;
const ENDING_LEAST: f64 = 60.0;

/// What a file's saved place says to do on arriving at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Saved {
    /// Nothing saved, and mpv starts at the top by itself.
    Nothing,
    /// Watched through, or all but, or barely begun: from the top.
    Over,
    /// A place worth going back to, this many seconds in.
    Place(f64),
}

impl Saved {
    pub fn of(progress: &Progress) -> Self {
        let Some(start) = progress.start else {
            return Self::Nothing;
        };
        let almost_over = progress
            .seconds
            .is_some_and(|seconds| seconds - start <= (seconds * ENDING_SHARE).max(ENDING_LEAST));
        if progress.finished || almost_over || start < WORTH_ASKING {
            Self::Over
        } else {
            Self::Place(start)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};

    /// A temp folder holding five films: three left part-way through, at
    /// different times (`newest`, `older`, `oldest`); one opened and left
    /// again in its first minute; and one watched to its end. Each has the
    /// watch-later file mpv would have written, or would have deleted.
    fn seed() -> (std::path::PathBuf, Vec<String>) {
        let dir = std::env::temp_dir().join(format!("dbm-shelf-{}", std::process::id()));
        let app = dir.join("Death by MPV");
        fs::create_dir_all(app.join("watch_later")).unwrap();
        let media = dir.join("media");
        fs::create_dir_all(&media).unwrap();
        crate::paths::set_test_base(&dir);

        let names = ["newest", "older", "oldest", "barely started", "finished"];
        let paths: Vec<String> = names
            .iter()
            .map(|name| media.join(format!("{name}.mkv")).display().to_string())
            .collect();
        for path in &paths {
            fs::write(path, b"").unwrap();
        }

        let lengths: Vec<String> = paths
            .iter()
            .map(|path| format!("{path:?}: 3600.0"))
            .collect();
        fs::write(
            app.join("durations.json"),
            format!("{{{}}}", lengths.join(",")),
        )
        .unwrap();

        // Where each was left, in seconds. The fourth is under the minute a
        // place needs before it is worth offering; the fifth was watched to
        // the end, so mpv has no watch-later file for it at all.
        let starts = [1800.0, 1200.0, 900.0, 20.0];
        let now = SystemTime::now();
        for (i, (path, start)) in paths.iter().zip(starts).enumerate() {
            let file = app
                .join("watch_later")
                .join(crate::durations::watch_later_name(&MediaPath::for_test(
                    path,
                )));
            fs::write(&file, format!("start={start}\n")).unwrap();
            let age = Duration::from_secs(60 * i as u64);
            let handle = fs::File::options().write(true).open(&file).unwrap();
            handle
                .set_times(fs::FileTimes::new().set_modified(now - age))
                .unwrap();
        }
        fs::write(
            app.join("finished.json"),
            format!("{{{:?}: 1800.0}}", paths[4]),
        )
        .unwrap();

        (dir, paths)
    }

    #[test]
    fn what_was_left_unfinished_comes_newest_first() {
        let (dir, paths) = seed();
        let found = unfinished(10);
        let order: Vec<&str> = found.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            order,
            [paths[0].as_str(), paths[1].as_str(), paths[2].as_str()]
        );
        // Newest first, and each says how much is left.
        assert!((found[0].fraction - 0.5).abs() < 1e-6);
        assert!((found[0].seconds_left - 1800.0).abs() < 1e-6);
        // Only as many as were asked for.
        assert_eq!(unfinished(2).len(), 2);
        assert!(unfinished(0).is_empty());
        assert_eq!(
            last_watched().map(|r| r.path),
            Some(MediaPath::for_test(&paths[0]))
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_show_is_offered_once_by_its_latest_episode() {
        let dir = std::env::temp_dir().join(format!("dbm-shelf-show-{}", std::process::id()));
        let app = dir.join("Death by MPV");
        fs::create_dir_all(app.join("watch_later")).unwrap();
        let media = dir.join("media");
        fs::create_dir_all(&media).unwrap();
        crate::paths::set_test_base(&dir);

        // Two episodes of one show, its second season left more recently,
        // and a film left more recently still.
        let left = [
            ("Show S01E01 Pilot.mkv", 1500.0, 2),
            ("Show S02E05 Return.mkv", 1200.0, 1),
            ("Film (2001).mkv", 1800.0, 0),
        ];
        let now = SystemTime::now();
        let mut durations = Vec::new();
        for (name, start, age) in left {
            let path = media.join(name).display().to_string();
            fs::write(&path, b"").unwrap();
            durations.push(format!("{path:?}: 3600.0"));
            let file = app
                .join("watch_later")
                .join(crate::durations::watch_later_name(&MediaPath::for_test(
                    &path,
                )));
            fs::write(&file, format!("start={start}\n")).unwrap();
            let handle = fs::File::options().write(true).open(&file).unwrap();
            handle
                .set_times(
                    fs::FileTimes::new().set_modified(now - Duration::from_secs(60 * age as u64)),
                )
                .unwrap();
        }
        fs::write(
            app.join("durations.json"),
            format!("{{{}}}", durations.join(",")),
        )
        .unwrap();

        let found = unfinished(10);
        assert_eq!(found.len(), 2);
        assert!(found[0].path.as_str().ends_with("Film (2001).mkv"));
        assert_eq!(found[0].show, None);
        // The later episode stands for the show, and the earlier one is
        // passed over even though it was left part-way through too.
        assert!(found[1].path.as_str().ends_with("Show S02E05 Return.mkv"));
        assert_eq!(found[1].show.as_deref(), Some("Show"));
        assert!(found.iter().all(|r| !r.path.as_str().contains("S01E01")));
        fs::remove_dir_all(&dir).ok();
    }
}
