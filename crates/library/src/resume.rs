//! The film left unfinished most recently, for the empty window to offer.
//!
//! Worked out from what mpv and the player already keep rather than a record
//! of its own, so it can't disagree with what mpv will do. The duration cache
//! lists the paths ever played here, and mpv keeps a watch-later file only
//! while a path has a position worth resuming. So the newest watch-later file
//! among those paths is the last thing left unfinished, and its `start=` is
//! where.

use dbm_mpv::MediaPath;

use crate::durations::Progress;
use crate::paths;

/// The file you stopped part-way through most recently.
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

/// Find it.
///
/// **Blocking.** Worker thread only: it stats a file per entry in the
/// duration cache, reads one watch-later file and lists that file's folder.
pub fn last_watched() -> Option<Resume> {
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
    candidates.sort_by_key(|(modified, ..)| std::cmp::Reverse(*modified));

    // Newest first, and the first that still stands up: a watch-later file
    // can outlive its video, mpv writes some with no position, and one left
    // in its credits has a position but isn't unfinished.
    let finished = crate::durations::load_finished();
    candidates.into_iter().find_map(|(_, path, seconds)| {
        let start = crate::durations::resume_point(&dir, &path)?;
        if start <= 0.0
            || crate::durations::is_finished(&finished, &path, start)
            || !path.as_path().is_file()
        {
            return None;
        }
        let fraction = (start / seconds).clamp(0.0, 1.0) as f32;
        crate::playlist::notice_folder_of(path.as_path());
        let (show, title) = dbm_naming::described(path.as_str(), None);
        Some(Resume {
            seconds_left: (seconds - start).max(0.0),
            show,
            title,
            still: crate::preview::still(&path, fraction),
            path,
            fraction,
        })
    })
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
        if progress.start <= 0.0 {
            Self::Nothing
        } else if progress.finished
            || progress.seconds > 0.0
                && progress.seconds - progress.start
                    <= (progress.seconds * ENDING_SHARE).max(ENDING_LEAST)
            || progress.start < WORTH_ASKING
        {
            Self::Over
        } else {
            Self::Place(progress.start)
        }
    }
}
