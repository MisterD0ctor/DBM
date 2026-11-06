//! Saying why a file wouldn't play.
//!
//! What is kept is mpv's reason, and the sentence is made each time it is
//! shown, so a playlist row that says it again after the language changes
//! says it in the new one.

use dbm_lang::{fill, tr};
use dbm_mpv::{ErrorCode, MediaPath};

use crate::interface::format;

/// The reason first, because the capsule elides at the bar's width and the
/// reason is the part worth reading. Then the name the playlist shows, not
/// the path, whose end, where the file's name is, would be cut. `path` is
/// `None` for a file that failed before mpv said which it was.
pub fn could_not_play(failure: ErrorCode, path: Option<&MediaPath>) -> String {
    let reason = format::sentence(&said(failure));
    match path {
        Some(path) => fill(
            tr("{} — could not play {}"),
            &[&reason, &dbm_naming::titled(path.as_str(), None)],
        ),
        None => fill(tr("{} — could not play that file"), &[&reason]),
    }
}

/// mpv's reason in the language being spoken. One a file can't stop for has
/// no translation, and is shown in mpv's English.
fn said(failure: ErrorCode) -> String {
    match failure {
        ErrorCode::LoadingFailed => tr("loading failed"),
        ErrorCode::UnknownFormat => tr("unrecognized file format"),
        ErrorCode::NothingToPlay => tr("no audio or video data played"),
        ErrorCode::AoInitFailed => tr("audio output initialization failed"),
        ErrorCode::VoInitFailed => tr("video output initialization failed"),
        ErrorCode::Unsupported => tr("not supported"),
        ErrorCode::NotImplemented => tr("operation not implemented"),
        ErrorCode::Generic => tr("something happened"),
        ErrorCode::Other(_) => tr("unknown error"),
        other => return other.to_string(),
    }
    .to_owned()
}
