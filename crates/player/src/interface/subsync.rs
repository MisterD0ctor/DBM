//! What a subtitle sync's answer does: the timing it found played, the row
//! that asked set to offer the old one back, and a line saying what came of
//! it.
//!
//! The sync row starts the work (see `actions`), and the answer arrives here
//! through the player's completions.

use dbm_lang::{fill, tr};
use dbm_library::subsync::{Failure, Outcome};
use dbm_mpv::Mpv;
use slint::ComponentHandle;

use super::{format, say, warn};
use crate::MainWindow;
use crate::playback::commands;
use crate::playback::state::PlayerState;
use crate::{Subtitles, SyncState};

/// Play the subtitles the way a sync found, and say what came of it.
///
/// Every outcome is said: the row has read "Listening" for seconds, and
/// the two answers that change nothing on screen (already in step, and no
/// answer) would otherwise look like a button that did nothing.
pub fn answer(ui: &MainWindow, mpv: &Mpv, player: &PlayerState, outcome: Outcome) {
    ui.global::<Subtitles>().set_sync_state(SyncState::Idle);
    // For the film and the track it listened to, or for nothing: the
    // timing belongs to that pair and would be wrong for any other.
    if outcome.path.is_none() || player.path != outcome.path || player.sid != outcome.sid {
        return;
    }
    let fit = match outcome.result {
        Ok(fit) => fit,
        Err(why) => {
            warn(ui, unsynced(why).into());
            return;
        }
    };
    let was = (
        player.sub_delay,
        if player.sub_speed > 0.0 {
            player.sub_speed
        } else {
            1.0
        },
    );
    let moved = fit.delay - was.0;
    let same_speed = (fit.speed - was.1).abs() < 1e-4;
    if same_speed && moved.abs() < 0.05 {
        say(ui, tr("Subtitles already in sync").into());
        return;
    }
    commands::set_sub_delay(mpv, fit.delay);
    commands::set_sub_speed(mpv, fit.speed);
    // Saved now, since the timing is this file's alone, and a crash
    // before the next checkpoint would lose it.
    crate::playback::session::checkpoint(mpv);
    // What stood before, for the row to offer back.
    ui.global::<Subtitles>().set_sync_was_delay(was.0 as f32);
    ui.global::<Subtitles>().set_sync_was_speed(was.1 as f32);
    ui.global::<Subtitles>().set_sync_state(SyncState::Undoable);
    say(
        ui,
        if same_speed {
            // Two sentences rather than one with a word swapped in, since
            // the word doesn't sit in the same place in every language.
            fill(
                if moved < 0.0 {
                    tr("Subtitles moved {} s earlier")
                } else {
                    tr("Subtitles moved {} s later")
                },
                &[&format::decimal(&format!("{:.1}", moved.abs()))],
            )
        } else {
            // No figure, since at another speed the subtitles have moved
            // by a different amount at every point in the film.
            tr("Subtitles matched to the film's speed").into()
        },
    );
}

/// Why a sync changed nothing, for the person who asked for it.
fn unsynced(why: Failure) -> &'static str {
    match why {
        Failure::NoSubtitles => tr("No subtitles to sync"),
        Failure::Pictures => tr("Only text subtitles can be synced"),
        Failure::NoFfmpeg => tr("Syncing subtitles needs ffmpeg"),
        Failure::Unread => tr("Could not read these subtitles"),
        Failure::Silent => tr("Could not read this film's audio"),
        Failure::Unsure => tr("Could not match these subtitles to the speech"),
    }
}
