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

use dbm_library::paths;
use dbm_mpv::prop::{self, Names};
use dbm_mpv::{Command, Events, Mpv, Options};

use crate::playback::commands;

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
pub fn configure(options: &Options<'_>) {
    if let Err(e) = options.set(prop::WATCH_LATER_DIRECTORY, paths::watch_later_dir()) {
        log::info!("watch-later directory: {e}");
    }
    if let Err(e) = options.set(prop::SAVE_POSITION_ON_QUIT, true) {
        log::info!("save-position-on-quit: {e}");
    }
    // `start` is the position; the rest makes it a resume. The subtitles'
    // speed goes with their delay, since a sync can set both, and one without
    // the other is worse than neither.
    let recorded = Names::default()
        .with(prop::START)
        .with(prop::VID)
        .with(prop::AID)
        .with(prop::SID)
        .with(prop::VOLUME)
        .with(prop::SUB_DELAY)
        .with(prop::SUB_SPEED);
    if let Err(e) = options.set(prop::WATCH_LATER_OPTIONS, recorded) {
        log::info!("watch-later-options: {e}");
    }
    // The other half of "per file": mpv carries a setting changed during one
    // file into the next, so a delay that fixed one episode would land on the
    // next as well. With this each file starts untimed, and its own timing
    // comes back from the entry above.
    let per_file = Names::default().with(prop::SUB_DELAY).with(prop::SUB_SPEED);
    if let Err(e) = options.set(prop::RESET_ON_NEXT_FILE, per_file) {
        log::info!("reset-on-next-file: {e}");
    }
    // A run nobody is watching (see `run_mode`) plays silently, through a
    // zero-gain filter at the end of the audio chain rather than by muting or
    // turning it down, which several exercises press and read back.
    if crate::run_mode::unattended() {
        match options.set(prop::AF, "lavfi=[volume=0]") {
            Ok(()) => eprintln!("dbm: unattended - playing silently"),
            Err(e) => eprintln!("dbm: unattended, but could not silence the audio: {e}"),
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
    if let Err(e) = mpv.run(
        commands::Reply::Ignored.id(),
        Command::WriteWatchLaterConfig,
    ) {
        log::info!("watch-later checkpoint: {e}");
    }
}

/// Write the playing file's entry as the player quits, and wait until mpv has.
///
/// Queued behind the last things asked for, so a seek made just before
/// quitting is the place saved. Taking mpv down doesn't save anything by
/// itself. Every event that arrives while it waits is dropped, so this is for
/// once the event loop has returned.
pub fn save_before_quitting(events: &mut Events) {
    if OPENING.load(Ordering::Relaxed) {
        return;
    }
    let saved = events.run_and_wait(Command::WriteWatchLaterConfig, QUIT_SAVE_WAIT);
    if let Err(e) = saved {
        log::info!("watch-later on quit: {e}");
    }
}

/// How long quitting waits for the save. It takes milliseconds; this is for
/// an mpv that has stopped answering, which shouldn't hold the quit for ever.
const QUIT_SAVE_WAIT: Duration = Duration::from_secs(5);

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
    dbm_library::paths::vendored(&[name]).unwrap_or_else(|| PathBuf::from(name))
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
    let path = dbm_library::paths::subtitle_mask();
    let loaded = std::fs::write(&path, include_str!("../../shaders/subtitle_mask.glsl"))
        .map_err(|e| e.to_string())
        .and_then(|()| {
            mpv.run(commands::Reply::Ignored.id(), Command::AppendShader(&path))
                .map_err(|e| e.to_string())
        });
    if let Err(e) = loaded {
        log::warn!(
            "no subtitle mask ({e}); a subtitle on the picture's edge will light the border"
        );
    }
}
