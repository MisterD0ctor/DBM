//! Remembering where you were.
//!
//! mpv's `watch-later` writes a small file per title with the position and
//! the selected tracks, and restores it when the file opens again. The work
//! here is pointing it at our own directory, choosing what it records, and
//! writing it at the moments that matter.
//!
//! mpv's own saving belongs to commands this player never sends (`quit`,
//! `stop`, `loadfile`), and it deletes a file's entry as it resumes from it.
//! So the player writes the entry itself: on leaving a file (see `commands`),
//! on quitting, and every ten seconds while something plays, for a crash, a
//! kill or a power cut. Each write is one small file, so doing it often costs
//! next to nothing.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::paths;
use crate::playback::commands;
use crate::playback::mpv::Mpv;

/// Whether a file is on its way in and hasn't loaded. mpv doesn't know its
/// position until it has, and an entry written meanwhile would have none and
/// replace the one it is resuming from, so nothing here writes until then.
static OPENING: AtomicBool = AtomicBool::new(false);

/// A file is about to open: one asked for (see `commands`), or one mpv has
/// started by itself.
pub fn opening() {
    OPENING.store(true, Ordering::Relaxed);
}

/// Nothing is on its way in any more: the file loaded, or the move to it went
/// nowhere.
pub fn settled() {
    OPENING.store(false, Ordering::Relaxed);
}

/// How often to checkpoint the position of the playing file.
///
/// Ten seconds is the most anyone should lose to a crash.
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
        log::info!("watch-later directory: {e}");
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
            log::info!("{key}: {e}");
        }
    }
}

/// Start checkpointing. The returned timer must be kept alive.
#[must_use]
pub fn checkpoint_periodically(mpv: Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, CHECKPOINT, move || {
        checkpoint(&mpv);
    });
    timer
}

/// Write the playing file's entry now, rather than at the next tick.
///
/// For what mustn't wait for it: leaving the file, and a subtitle sync, made
/// once and on purpose (see `configure`), which a crash would otherwise take.
///
/// Async, so this costs the UI thread a queue push. mpv skips the write when
/// nothing is playing.
pub fn checkpoint(mpv: &Mpv) {
    if OPENING.load(Ordering::Relaxed) {
        return;
    }
    if let Err(e) = mpv.command_async(0, &["write-watch-later-config"]) {
        log::info!("watch-later checkpoint: {e}");
    }
}

/// Write the playing file's entry as the player quits, and wait until mpv has.
///
/// Only once the event loop has returned, since it blocks (see
/// [`Mpv::command`]). Taking mpv down doesn't save anything by itself.
pub fn save_before_quitting(mpv: &Mpv) {
    if OPENING.load(Ordering::Relaxed) {
        return;
    }
    if let Err(e) = mpv.command(&["write-watch-later-config"]) {
        log::info!("watch-later on quit: {e}");
    }
}

/// Where to find libmpv.
///
/// `DBM_LIBMPV` wins, so a stock build can be dropped in without touching the
/// tree. Otherwise the copy this app ships with, and failing that the bare
/// name, which hands the search to the system loader.
pub fn libmpv_path() -> PathBuf {
    if let Some(p) = std::env::var_os("DBM_LIBMPV") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) {
        "libmpv-2.dll"
    } else {
        "libmpv.so.2"
    };
    crate::paths::vendored(&[name]).unwrap_or_else(|| PathBuf::from(name))
}

/// Have mpv mark which of the picture's pixels are a subtitle's, so the
/// ambient border can leave them out of the light it reads off the picture
/// (subtitle_mask.glsl). Without it the border works as it always did, and a
/// subtitle on the picture's edge lights it. After `mpv_initialize`, since
/// the shader is loaded by a command.
///
/// Written out each run, since mpv loads shaders only from files, and this
/// one has to match the border pass it was built with.
pub fn mask_subtitles(mpv: &Mpv) {
    let path = crate::paths::subtitle_mask();
    // A command rather than the option, which would split the path on the
    // list separator: as an argument of its own, any path is taken as it is.
    let loaded = std::fs::write(&path, include_str!("../../shaders/subtitle_mask.glsl"))
        .map_err(|e| e.to_string())
        .and_then(|()| {
            let path = path.to_string_lossy();
            let args = ["change-list", "glsl-shaders", "append", path.as_ref()];
            mpv.command_async(commands::REPLY_IGNORED, &args)
                .map_err(|e| e.to_string())
        });
    if let Err(e) = loaded {
        log::warn!(
            "no subtitle mask ({e}); a subtitle on the picture's edge will light the border"
        );
    }
}
