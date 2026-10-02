//! The first step of every frame: draining mpv's event queue.
//!
//! Draining here rather than on a thread of its own keeps `mpv_wait_event`
//! single-threaded — it is not safe to call concurrently — and means events
//! still flow while Windows owns the loop during a resize drag, since the
//! modal hook keeps frames coming.

use std::rc::Rc;

use crate::interface::format;
use crate::playback::audio::Watchdog;
use crate::playback::mpv::{Event, Mpv};
use crate::playback::state::PlayerState;

/// Drain mpv's event queue into the mirrored state.
///
/// Returns whether anything the UI shows changed. Command replies are
/// collected into `replies` for the driver to route, and a file that would
/// not play becomes a sentence in `notices`.
pub fn drain(
    mpv: &Mpv,
    player: &mut PlayerState,
    replies: &mut Vec<u64>,
    notices: &mut Vec<String>,
    audio: &Rc<Watchdog>,
) -> bool {
    let mut dirty = false;
    replies.clear();
    notices.clear();
    while let Some(event) = mpv.poll_event() {
        // Command completions are not state; they belong to whoever issued
        // the command, so they are collected for the driver to route.
        if let Event::CommandReply { id } = event {
            replies.push(id);
            continue;
        }
        // A file that stopped because it could not be played. mpv knows why
        // and says so in its own words, which are better than any wording
        // invented here — "Unrecognized file format" beats "playback error".
        //
        // Named by the file's own name, not its path. A path fills the
        // capsule with the drive and folders, and elision takes the end —
        // exactly the part that says which file in a season it was.
        //
        // Reason first. The capsule elides at the width of the bar, and with
        // a scene release's name in front it was the reason — the one part
        // worth reading — that fell off the end. The name is the one the
        // playlist shows, not the file's, and the path is remembered so the
        // row can go on saying so after the notice has gone.
        if let Event::EndFile {
            failure: Some(reason),
        } = &event
        {
            let path = player.path.clone().or_else(|| player.filename.clone());
            let reason = format::sentence(reason);
            notices.push(match &path {
                Some(path) => format!(
                    "{reason} — could not play {}",
                    crate::library::naming::titled(path, None)
                ),
                None => format!("{reason} — could not play that file"),
            });
            if let Some(path) = path {
                player.failed.insert(path);
            }
        }
        // Not everything mpv reports is state the interface shows. The
        // watchdog reads the same stream for the device list and the audio
        // track, neither of which belongs in `PlayerState`.
        audio.on_event(&event);
        dirty |= player.apply(&event);
    }
    dirty
}
