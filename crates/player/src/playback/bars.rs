//! Showing a film without the black bars its frame carries, as found by
//! `dbm_library::bars`.
//!
//! mpv is handed the picture as `video-crop`, after which everything that
//! reads where the picture is — the ambient border, Fill the window, where
//! subtitles rest — finds the picture rather than the frame.

use dbm_library::bars::{Crop, Found};
use dbm_library::threads::ForFile;
use dbm_mpv::tracks::{self, TrackKind};
use dbm_mpv::{MediaPath, Mpv};

use super::commands;
use super::state::PlayerState;

#[derive(Default)]
pub struct Bars {
    /// The answer for the file playing, kept with its path.
    found: Option<ForFile<Option<Crop>>>,
    /// The crop mpv was handed, and which of `PlayerState::loads` it was for:
    /// mpv drops it when that file ends, even when the same file opens again.
    sent: Option<(u64, Crop)>,
}

impl Bars {
    /// Keep an answer for the file playing. A run reports the next file's as
    /// well, which is kept on disk for when it opens.
    pub fn take(&mut self, found: Found, playing: Option<&MediaPath>) {
        let path = found.path().clone();
        if let Some(crop) = found.if_playing(playing) {
            self.found = Some(ForFile::new(path, crop));
        }
    }

    /// Crop the file playing to its picture, or show its whole frame again,
    /// once it has opened and its tracks have been read.
    ///
    /// Not while a styled (ASS) subtitle track is selected: mpv lays the
    /// script over the cropped picture rather than the frame, which moves
    /// and squashes every sign placed against the frame.
    pub fn follow(&mut self, mpv: &Mpv, player: &PlayerState, tracks_read: bool) {
        let Some(path) = player.path.as_ref() else {
            return;
        };
        if player.duration.is_none() || !tracks_read {
            return;
        }
        let styled = tracks::selected(&player.tracks, TrackKind::Sub)
            .and_then(|track| track.codec.as_deref())
            .is_some_and(tracks::is_styled);
        let wanted = self
            .found
            .as_ref()
            .filter(|found| found.path() == path)
            .and_then(|found| *found.value())
            .filter(|_| !styled);
        let given = self
            .sent
            .filter(|(load, _)| *load == player.loads)
            .map(|(_, crop)| crop);
        if wanted == given {
            return;
        }
        commands::crop(mpv, wanted);
        self.sent = wanted.map(|crop| (player.loads, crop));
    }
}
