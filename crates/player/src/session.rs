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
