//! Remembering where you were.
//!
//! mpv's `watch-later` writes a small file per title with the position and
//! the selected tracks, and restores it when the file opens again. The work
//! here is pointing it at our own directory, choosing what it records, and
//! writing often enough that an unclean exit doesn't lose the evening.
//!
//! mpv writes on a clean quit by itself, but not on a crash, a kill or a
//! power cut, so this also writes periodically while something plays. The
//! command is async and mpv only touches the disk when the position has
//! moved, so doing it often costs next to nothing.

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
/// `watch-later-options` matters most: by default mpv restores only the
/// position, so a file would come back with the wrong audio track and the
/// subtitles off. Recording the tracks and the subtitle timing too makes
/// reopening resume rather than merely seek.
pub fn configure(mpv: &Mpv) {
    let dir = paths::watch_later_dir();
    if let Err(e) = mpv.set_option("watch-later-directory", &dir.to_string_lossy()) {
        eprintln!("dbm: watch-later directory: {e}");
    }
    for (key, value) in [
        ("save-position-on-quit", "yes"),
        // `start` is the position; the rest makes it a resume. The subtitles'
        // speed goes with their delay, since a sync can set both, and one
        // without the other is worse than neither.
        (
            "watch-later-options",
            "start,vid,aid,sid,volume,sub-delay,sub-speed",
        ),
        // The other half of "per file": mpv carries a setting changed during
        // one file into the next, so a delay that fixed one episode would
        // land on the next as well. With this each file starts untimed, and
        // its own timing comes back from the entry above.
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
/// For a change that mustn't be lost to leaving the file in the next ten
/// seconds: a subtitle sync, made once and on purpose (see `configure`).
///
/// Async, so this costs the UI thread a queue push. mpv skips the write when
/// there is nothing loaded.
pub fn checkpoint(mpv: &Mpv) {
    if let Err(e) = mpv.command_async(0, &["write-watch-later-config"]) {
        eprintln!("dbm: watch-later checkpoint: {e}");
    }
}
