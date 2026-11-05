//! Saying why a file wouldn't play.
//!
//! What is kept is mpv's reason, and the sentence is made each time it is
//! shown, so a playlist row that says it again after the language changes
//! says it in the new one.

use dbm_lang::{fill, marked, tr};
use dbm_mpv::MediaPath;

use crate::interface::format;

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

/// The reason first, because the capsule elides at the bar's width and the
/// reason is the part worth reading. Then the name the playlist shows, not
/// the path, whose end, where the file's name is, would be cut. `path` is
/// `None` for a file that failed before mpv said which it was.
pub fn could_not_play(reason: &str, path: Option<&MediaPath>) -> String {
    let reason = format::sentence(said(reason));
    match path {
        Some(path) => fill(
            tr("{} — could not play {}"),
            &[&reason, &dbm_naming::titled(path.as_str(), None)],
        ),
        None => fill(tr("{} — could not play that file"), &[&reason]),
    }
}

/// mpv's description in the language being spoken, where it is one of
/// [`REASONS`].
fn said(reason: &str) -> &str {
    REASONS
        .iter()
        .find(|known| **known == reason)
        .map_or(reason, |known| tr(known))
}
