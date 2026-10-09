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

use crate::paths;
use crate::playback::mpv::Mpv;

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
        // `start` is the position; the rest is what makes it a resume. The
        // subtitles' speed goes with their delay: a sync can set both, and
        // one restored without the other is worse than neither.
        (
            "watch-later-options",
            "start,vid,aid,sid,volume,sub-delay,sub-speed",
        ),
        // The other half of "per file". mpv keeps a setting changed during
        // one file for every file after it, so a delay that fixed one
        // episode's subtitles was put on the next episode's as well — and a
        // speed with it, which no row shows. With this each file starts
        // untimed and leaves as it started; its own timing comes back from
        // the entry above when it is opened again.
        ("reset-on-next-file", "sub-delay,sub-speed"),
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
        checkpoint(&mpv)
    });
    timer
}

/// Write the playing file's entry now, rather than at the next tick.
///
/// For a change that should not be lost to leaving the file in the next ten
/// seconds: a subtitle sync, which is made once and on purpose, and which
/// the file no longer takes with it to the next — see `configure`.
///
/// Async, so this costs the UI thread a queue push. mpv skips the write when
/// there is nothing loaded.
pub fn checkpoint(mpv: &Mpv) {
    if let Err(e) = mpv.command_async(0, &["write-watch-later-config"]) {
        eprintln!("dbm: watch-later checkpoint: {e}");
    }
}
