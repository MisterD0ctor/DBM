//! Remembering where you were.
//!
//! mpv already knows how to do this — `watch-later` writes a small file per
//! title recording the position and which tracks were selected, and restores
//! it when the same file is opened again. The work here is pointing it at our
//! own directory, choosing what to record, and making sure the file is
//! written often enough that an unclean exit does not lose the evening.
//!
//! mpv writes on a clean quit by itself. That covers closing the window; it
//! does not cover a crash, a kill, or a power cut, so this also writes
//! periodically while something is playing. The command is async and mpv
//! only touches the disk when the position has actually moved, so the cost
//! of doing it often is close to nothing.

use std::sync::Arc;
use std::time::Duration;

use crate::mpv::Mpv;
use crate::paths;

/// How often to checkpoint the position of the playing file.
///
/// Ten seconds is the most anyone should lose to a crash, and is far longer
/// than the interval at which mpv would consider the position unchanged.
const CHECKPOINT: Duration = Duration::from_secs(10);

/// Options that must be set before `mpv_initialize`.
///
/// `watch-later-options` is the interesting one: by default mpv restores only
/// the position, so a file reopened would come back at the right timestamp
/// with the wrong audio track and the subtitles off. Recording the track
/// selection and the subtitle timing alongside means reopening genuinely
/// resumes rather than merely seeking.
pub fn configure(mpv: &Mpv) {
    let dir = paths::watch_later_dir();
    if let Err(e) = mpv.set_option("watch-later-directory", &dir.to_string_lossy()) {
        eprintln!("dbm: watch-later directory: {e}");
    }
    for (key, value) in [
        ("save-position-on-quit", "yes"),
        // `start` is the position; the rest is what makes it a resume.
        ("watch-later-options", "start,vid,aid,sid,volume,sub-delay"),
    ] {
        if let Err(e) = mpv.set_option(key, value) {
            eprintln!("dbm: {key}: {e}");
        }
    }
}

/// The file you stopped part-way through most recently.
pub struct Resume {
    pub path: String,
    /// Named the way the bar would name it.
    pub title: String,
    /// How far in the resume point sits, 0..1.
    pub fraction: f32,
    /// A frame from about there, when a seek-preview atlas was ever built.
    pub still: Option<crate::preview::Still>,
}

/// Find it.
///
/// **Blocking.** Worker thread only: it stats a file per entry in the
/// duration cache and reads one watch-later file.
///
/// Worked out from two things mpv and the player already keep, rather than
/// from a record of its own. The duration cache is the list of paths that
/// were ever played here, and mpv's watch-later file for a path exists only
/// while it has a position worth resuming — it deletes it when a file is
/// watched to the end. So the newest watch-later file among those paths is
/// the last thing left unfinished, and its `start=` is where. Nothing here
/// can disagree with what mpv will actually do on opening it.
pub fn last_watched() -> Option<Resume> {
    let dir = paths::watch_later_dir();
    let mut candidates: Vec<(std::time::SystemTime, String, f64)> =
        crate::durations::load()
            .into_iter()
            .filter(|(_, seconds)| *seconds > 0.0)
            .filter_map(|(path, seconds)| {
                let file = dir.join(crate::durations::watch_later_name(&path));
                let modified = std::fs::metadata(file).ok()?.modified().ok()?;
                Some((modified, path, seconds))
            })
            .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    // Newest first, and the first that still stands up: a watch-later file
    // can outlive the video it describes, and mpv also writes entries that
    // carry no position at all.
    candidates.into_iter().find_map(|(_, path, seconds)| {
        let start = crate::durations::resume_point(&dir, &path)?;
        if start <= 0.0 || !std::path::Path::new(&path).is_file() {
            return None;
        }
        let fraction = (start / seconds).clamp(0.0, 1.0) as f32;
        Some(Resume {
            title: crate::naming::titled(&path, None),
            still: crate::preview::still(std::path::Path::new(&path), fraction),
            path,
            fraction,
        })
    })
}

/// Start checkpointing. The returned timer must be kept alive.
#[must_use]
pub fn checkpoint_periodically(mpv: Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, CHECKPOINT, move || {
        // Async, so this costs the UI thread a queue push. mpv skips the
        // write when there is nothing loaded.
        if let Err(e) = mpv.command_async(0, &["write-watch-later-config"]) {
            eprintln!("dbm: watch-later checkpoint: {e}");
        }
    });
    timer
}
