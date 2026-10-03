//! The film left unfinished most recently, for the empty window to offer.
//!
//! Worked out from two things mpv and the player already keep, rather than
//! from a record of its own. The duration cache is the list of paths that
//! were ever played here, and mpv's watch-later file for a path exists only
//! while it has a position worth resuming — it deletes it when a file is
//! watched to the end. So the newest watch-later file among those paths is
//! the last thing left unfinished, and its `start=` is where. Nothing here
//! can disagree with what mpv will actually do on opening it.

use crate::paths;

/// The file you stopped part-way through most recently.
pub struct Resume {
    pub path: String,
    /// The show it is an episode of, if it is one. Kept apart from the title
    /// so the way in can give it a line of its own: sharing one with the
    /// episode, a show's name took the room and the episode lost its number.
    pub show: Option<String>,
    /// Named the way the bar would name it, less the show.
    pub title: String,
    /// How far in the resume point sits, 0..1.
    pub fraction: f32,
    /// How much of it is left, in seconds.
    pub seconds_left: f64,
    /// A frame from about there, when a seek-preview atlas was ever built.
    pub still: Option<crate::library::preview::Still>,
}

/// Find it.
///
/// **Blocking.** Worker thread only: it stats a file per entry in the
/// duration cache and reads one watch-later file.
pub fn last_watched() -> Option<Resume> {
    let dir = paths::watch_later_dir();
    let mut candidates: Vec<(std::time::SystemTime, String, f64)> =
        crate::library::durations::load()
            .into_iter()
            .filter(|(_, seconds)| *seconds > 0.0)
            .filter_map(|(path, seconds)| {
                let file = dir.join(crate::library::durations::watch_later_name(&path));
                let modified = std::fs::metadata(file).ok()?.modified().ok()?;
                Some((modified, path, seconds))
            })
            .collect();
    candidates.sort_by_key(|(modified, ..)| std::cmp::Reverse(*modified));

    // Newest first, and the first that still stands up: a watch-later file
    // can outlive the video it describes, and mpv also writes entries that
    // carry no position at all. Nor one watched into its credits and left
    // there: mpv still holds a position for it, but it is not unfinished.
    let finished = crate::library::durations::load_finished();
    candidates.into_iter().find_map(|(_, path, seconds)| {
        let start = crate::library::durations::resume_point(&dir, &path)?;
        if start <= 0.0
            || crate::library::durations::is_finished(&finished, &path, start)
            || !std::path::Path::new(&path).is_file()
        {
            return None;
        }
        let fraction = (start / seconds).clamp(0.0, 1.0) as f32;
        let (show, title) = crate::library::naming::described(&path, None);
        Some(Resume {
            seconds_left: (seconds - start).max(0.0),
            show,
            title,
            still: crate::library::preview::still(std::path::Path::new(&path), fraction),
            path,
            fraction,
        })
    })
}
