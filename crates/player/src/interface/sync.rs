//! Carrying what mpv says out to the interface.
//!
//! One direction only: mpv is the source of truth, this carries its values
//! outward. Anything travelling the other way is an action, and lives in
//! `actions`/`commands`.
//!
//! Everything here is called from the frame path, so the rule that matters
//! is: **never make a blocking property read on this path.**
//! `mpv_get_property_string` waits on mpv's core lock, and during a resize
//! that lock is contended enough to cost hundreds of milliseconds a frame.
//! Scalars are observed and arrive on the event queue; the list-shaped
//! properties are read on the worker, and only when a count or selection
//! actually moves.

use slint::{ModelRc, VecModel};

use crate::interface::format;
use crate::library::durations::Progress;
use crate::playback::state::PlayerState;
use crate::playback::tracks::{self, Chapter, PlaylistEntry, Track, TrackKind};
use crate::worker::{Completion, Worker};
use crate::{MainWindow, TrackItem};

/// Push the scalar values the UI binds to.
pub fn push_scalars(ui: &MainWindow, player: &PlayerState) {
    ui.set_media_title(player.display_title().into());
    ui.set_media_episode(player.display_episode().into());
    ui.set_elapsed(format::time(player.time_pos).into());
    ui.set_total(format::time(player.duration).into());
    ui.set_progress(player.progress());
    ui.set_paused(player.paused);
    ui.set_muted(player.muted);
    ui.set_volume(player.volume as f32);
    // mpv's `sub-visibility` can be on with no subtitle track selected, which
    // shows nothing, so the interface is told whether subtitles are actually
    // showing, for the subtitles menu's Off row.
    ui.set_subs_visible(
        player.sub_visibility && tracks::selected(&player.tracks, TrackKind::Sub).is_some(),
    );
    ui.set_panscan(player.panscan > 0.5);
    ui.set_sub_delay_text(format::delay(player.sub_delay).into());
    ui.set_sub_scale_text(format::scale(player.sub_scale).into());
    // Whether anything is loaded, derived from mpv every frame, since a file
    // can arrive by the dialog, a drop or the command line.
    ui.set_has_file(player.path.is_some());
    // A finished file, and whether another follows it. mpv only reports
    // `eof-reached` while holding the last frame, which is when the interface
    // has something to offer.
    ui.set_at_end(player.eof_reached && player.path.is_some());
    ui.set_at_last(player.playlist_pos + 1 >= player.playlist_count);
    // The other end, so previous can say there is nothing before this.
    ui.set_at_first(player.playlist_pos <= 0);
    // Raw, beside the formatted text, so undoing a reset can put the number
    // back without parsing it out of "+0.30 s".
    ui.set_sub_delay(player.sub_delay as f32);
    ui.set_sub_speed(if player.sub_speed > 0.0 {
        player.sub_speed as f32
    } else {
        1.0
    });
    ui.set_speed(if player.speed > 0.0 {
        player.speed as f32
    } else {
        1.0
    });
    ui.set_speed_text(format::speed(player.speed).into());
    ui.set_duration(player.duration as f32);
    ui.set_chapter_label(player.chapter_label().into());
    ui.set_credits_rolling(player.credits_rolling());
    ui.set_intro_rolling(player.intro_rolling());
    ui.set_intro_end(player.intro().map_or(0.0, |(_, end)| end as f32));
    // Where the dialogs open: beside this file, or on the shelf above its
    // folder.
    ui.set_current_path(player.path.clone().unwrap_or_default().into());
    // Searching needs a key the player was built with, and a film to look
    // for.
    ui.set_find_offered(crate::library::opensubtitles::available() && player.path.is_some());
}

/// Track, chapter and playlist contents, read together because they are
/// invalidated together and a single job means a single round trip.
pub struct Lists {
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// One per playlist entry, in the same order: how long it is and how far
    /// in the resume point sits. Read here rather than in a job of its own,
    /// because the lookup needs the playlist's paths, which are read here.
    pub progress: Vec<Progress>,
    /// The file's chapter marks. Invalidated with the tracks, by a new file.
    pub chapters: Vec<Chapter>,
    /// The generation this was read for, so a stale result can be discarded
    /// if the lists moved again while the job was running.
    pub generation: u64,
}

/// Requests list reads and applies the results.
///
/// The reads are ~80ms of blocking property calls, so they happen on the
/// worker. Like the scrubber, this keeps one request in flight and converges
/// on the newest state rather than replaying every intermediate one.
#[derive(Default)]
pub struct ListSync {
    in_flight: bool,
    requested: u64,
    /// Ask again even though nothing mpv knows about has changed.
    ///
    /// The progress on each playlist row comes from files mpv writes without
    /// an event (the playing file's every ten seconds), so the lists are read
    /// again when the playlist is opened, the moment it matters.
    forced: bool,
}

impl ListSync {
    /// Read again on the next poll, whatever the generation says.
    pub fn refresh(&mut self) {
        self.forced = true;
    }

    /// Whether what the player holds was read since the lists last moved:
    /// nothing asked for and not yet answered, nothing waiting to be asked.
    /// Until then the chapters may still be the last file's.
    pub fn settled(&self, player: &PlayerState) -> bool {
        !self.in_flight && self.requested == player.tracks_generation
    }

    /// Kick off a read if the lists moved since the last one.
    pub fn poll(&mut self, worker: &Worker, player: &PlayerState) {
        let generation = player.tracks_generation;
        if self.in_flight || (self.requested == generation && !self.forced) {
            return;
        }
        self.forced = false;
        self.requested = generation;
        self.in_flight = worker.submit(move |mpv| {
            let playlist = tracks::read_playlist(mpv);
            let paths: Vec<String> = playlist.iter().map(|e| e.filename.clone()).collect();
            Some(Completion::Lists(Lists {
                tracks: tracks::read_tracks(mpv),
                chapters: tracks::read_chapters(mpv),
                progress: crate::library::durations::of(&paths),
                playlist,
                generation,
            }))
        });
    }

    /// Take delivery of a finished read. Returns whether anything in it was
    /// new, which is the caller's cue to have the playlist laid out again.
    pub fn apply(&mut self, ui: &MainWindow, player: &mut PlayerState, lists: Lists) -> bool {
        self.in_flight = false;
        let moved = player.apply_lists(lists);
        if moved {
            push_lists(ui, player);
        }
        moved
    }
}

/// Hand the tracks and chapters to the UI as models. The playlist goes by
/// way of the worker (see `playlist_panel`).
pub fn push_lists(ui: &MainWindow, player: &PlayerState) {
    ui.set_sub_tracks(track_model(player, TrackKind::Sub));
    ui.set_audio_tracks(track_model(player, TrackKind::Audio));
    ui.set_sub_current(selected_row(player, TrackKind::Sub));
    ui.set_audio_current(selected_row(player, TrackKind::Audio));
    ui.set_chapter_marks(ModelRc::new(VecModel::from(
        player
            .chapters
            .iter()
            .map(|c| c.time as f32)
            .collect::<Vec<_>>(),
    )));
    ui.set_chapter_label(player.chapter_label().into());
    // The chapter's name arrives with the list, a moment after its number.
    ui.set_credits_rolling(player.credits_rolling());
    ui.set_intro_rolling(player.intro_rolling());
    ui.set_intro_end(player.intro().map_or(0.0, |(_, end)| end as f32));
}

/// Which row of a track list is the selected one, or -1.
///
/// Worked out here because the interface can't search a model, and needs the
/// answer to open the tracks panel on the track in use.
fn selected_row(player: &PlayerState, kind: TrackKind) -> i32 {
    tracks::labelled(&player.tracks, kind)
        .iter()
        .position(|(t, _)| t.selected)
        .map_or(-1, |row| row as i32)
}

fn track_model(player: &PlayerState, kind: TrackKind) -> ModelRc<TrackItem> {
    ModelRc::new(VecModel::from(
        tracks::labelled(&player.tracks, kind)
            .into_iter()
            .map(|(t, label)| TrackItem {
                id: t.id as i32,
                label: label.into(),
                selected: t.selected,
                external: t.external,
            })
            .collect::<Vec<_>>(),
    ))
}
