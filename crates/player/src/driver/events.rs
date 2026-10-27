//! The first step of every frame: draining mpv's event queue.
//!
//! Draining here rather than on a thread of its own keeps `mpv_wait_event`,
//! which isn't safe to call concurrently, on one thread, and events still
//! flow while Windows owns the loop during a resize drag, since the modal
//! hook keeps frames coming.

use std::rc::Rc;

use crate::interface::format;
use crate::lang::{fill, marked, tr};
use crate::playback::audio::Watchdog;
use crate::playback::mpv::{Event, Mpv};
use crate::playback::state::PlayerState;

/// mpv's own words for why a file stopped, as `mpv_error_string` gives them,
/// marked so each language can say them too. One mpv adds later is shown as
/// mpv says it.
const REASONS: [&str; 9] = [
    marked("loading failed"),
    marked("unrecognized file format"),
    marked("no audio or video data played"),
    marked("audio output initialization failed"),
    marked("video output initialization failed"),
    marked("not supported"),
    marked("operation not implemented"),
    marked("something happened"),
    marked("unknown error"),
];

/// mpv's description in the language being spoken, where it is one of
/// [`REASONS`].
fn said(reason: &str) -> &str {
    REASONS
        .iter()
        .find(|known| **known == reason)
        .map_or(reason, |known| tr(known))
}

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
        // A file that stopped because it couldn't be played, said in mpv's own
        // words, which beat anything invented here.
        //
        // Reason first, because the capsule elides at the bar's width and the
        // reason is the part worth reading. Then the name the playlist shows,
        // not the path, whose end, where the file's name is, would be cut.
        // The entry is remembered so its row keeps saying so after the notice
        // has gone.
        if let Event::EndFile {
            failure: Some(reason),
            entry,
            ..
        } = &event
        {
            // Which file it was is asked of the list, by the entry's id. The
            // player's own `path` is the file before this one, or nothing,
            // when a file fails before it has opened.
            let path = player
                .playlist
                .iter()
                .find(|e| e.id == *entry)
                .map(|e| e.filename.clone());
            let reason = format::sentence(said(reason));
            notices.push(match &path {
                Some(path) => fill(
                    tr("{} — could not play {}"),
                    &[&reason, &crate::library::naming::titled(path, None)],
                ),
                None => fill(tr("{} — could not play that file"), &[&reason]),
            });
            player.failed.insert(*entry);
        }
        // Not everything mpv reports is state the interface shows. The
        // watchdog reads the same stream for the device list and the audio
        // track, neither of which belongs in `PlayerState`.
        audio.on_event(&event);
        dirty |= player.apply(&event);
    }
    dirty
}
