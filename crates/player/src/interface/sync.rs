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

use dbm_mpv::MediaPath;
use dbm_mpv::tracks::{self, TrackKind};
use slint::ComponentHandle;
use slint::{ModelRc, SharedString, VecModel};

use crate::interface::format;
use crate::playback::labels;
use crate::playback::state::{Lists, PlayerState};
use crate::worker::{Completion, Worker};
use crate::{Find, MainWindow, Playback, Subtitles, TrackItem, Tracks};

/// Push the scalar values the UI binds to.
pub fn push_scalars(ui: &MainWindow, player: &PlayerState) {
    let playback = ui.global::<Playback>();
    let subtitles = ui.global::<Subtitles>();
    playback.set_title(player.display_title().into());
    playback.set_episode(player.display_episode().into());
    playback.set_elapsed(format::time(player.time_pos).into());
    playback.set_total(format::time(player.duration).into());
    playback.set_remaining(format::remaining(player.duration - player.time_pos).into());
    playback.set_progress(player.progress());
    playback.set_paused(player.paused);
    playback.set_muted(player.muted);
    playback.set_volume(player.volume as f32);
    playback.set_panscan(player.panscan > 0.5);
    subtitles.set_delay_text(format::delay(player.sub_delay).into());
    subtitles.set_delay_widest(format::delay_widest().into());
    subtitles.set_scale_text(format::scale(player.sub_scale).into());
    // Whether anything is loaded, derived from mpv every frame, since a file
    // can arrive by the dialog, a drop or the command line.
    playback.set_has_file(player.path.is_some());
    // A finished file, and whether another follows it. mpv only reports
    // `eof-reached` while holding the last frame, which is when the interface
    // has something to offer.
    playback.set_at_end(player.eof_reached && player.path.is_some());
    playback.set_at_last(player.at_last());
    // The other end, so previous can say there is nothing before this.
    playback.set_at_first(player.at_first());
    // Raw, beside the formatted text, so undoing a reset can put the number
    // back without parsing it out of "+0.30 s".
    subtitles.set_delay(player.sub_delay as f32);
    subtitles.set_speed(if player.sub_speed > 0.0 {
        player.sub_speed as f32
    } else {
        1.0
    });
    playback.set_speed(if player.speed > 0.0 {
        player.speed as f32
    } else {
        1.0
    });
    playback.set_speed_text(format::speed(player.speed).into());
    playback.set_duration(player.duration as f32);
    push_what_the_lists_decide(ui, player);
    // Where the dialogs open: beside this file, or on the shelf above its
    // folder.
    playback.set_path(player.path.as_ref().map_or("", MediaPath::as_str).into());
    // Searching needs a key the player was built with, and a film to look
    // for.
    ui.global::<Find>()
        .set_offered(dbm_library::opensubtitles::available() && player.path.is_some());
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
            let paths: Vec<MediaPath> = playlist.iter().map(|e| e.filename.clone()).collect();
            Some(Completion::Lists(Lists {
                tracks: tracks::read_tracks(mpv),
                chapters: tracks::read_chapters(mpv),
                progress: dbm_library::durations::of(&paths),
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
    let tracks = ui.global::<Tracks>();
    tracks.set_subtitles(track_model(player, TrackKind::Sub));
    tracks.set_audio(track_model(player, TrackKind::Audio));
    tracks.set_subtitle_names(track_names(player, TrackKind::Sub, false));
    tracks.set_subtitle_removable_names(track_names(player, TrackKind::Sub, true));
    tracks.set_audio_names(track_names(player, TrackKind::Audio, false));
    tracks.set_audio_removable_names(track_names(player, TrackKind::Audio, true));
    tracks.set_subtitle_current(selected_row(player, TrackKind::Sub));
    tracks.set_audio_current(selected_row(player, TrackKind::Audio));
    ui.global::<Playback>()
        .set_chapter_marks(ModelRc::new(VecModel::from(
            player
                .chapters
                .iter()
                .map(|c| c.time as f32)
                .collect::<Vec<_>>(),
        )));
    push_what_the_lists_decide(ui, player);
}

/// What hangs on a scalar and on a list both, so it is pushed with either:
/// whether subtitles show hangs on the selection, and the chapter's name on
/// the chapter list, which arrives a moment after the chapter's number. A
/// paused film reports nothing else that would push them.
fn push_what_the_lists_decide(ui: &MainWindow, player: &PlayerState) {
    let playback = ui.global::<Playback>();
    ui.global::<Subtitles>().set_visible(subs_showing(player));
    playback.set_chapter_label(player.chapter_label().into());
    playback.set_credits_rolling(player.credits_rolling());
    playback.set_intro_rolling(player.intro_rolling());
    playback.set_intro_end(player.intro().map_or(0.0, |(_, end)| end as f32));
    playback.set_after_credits(player.after_credits().map_or(0.0, |at| at as f32));
}

/// Whether subtitles are actually showing, for the subtitles menu's Off row
/// and the pages that tune a line. mpv's `sub-visibility` can be on with no
/// subtitle track selected, which shows nothing.
fn subs_showing(player: &PlayerState) -> bool {
    player.sub_visibility && tracks::selected(&player.tracks, TrackKind::Sub).is_some()
}

/// Which row of a track list is the selected one, or -1.
///
/// Worked out here because the interface can't search a model, and needs the
/// answer to open the tracks panel on the track in use.
fn selected_row(player: &PlayerState, kind: TrackKind) -> i32 {
    labels::tracks(&player.tracks, kind)
        .iter()
        .position(|(t, _)| t.selected)
        .map_or(-1, |row| row as i32)
}

/// The names of the tracks carried in the film, or of those loaded from their
/// own files, one to a line, for the measures that size their panel.
fn track_names(player: &PlayerState, kind: TrackKind, external: bool) -> SharedString {
    labels::tracks(&player.tracks, kind)
        .into_iter()
        .filter(|(t, _)| t.external == external)
        .map(|(_, label)| label)
        .collect::<Vec<_>>()
        .join("\n")
        .into()
}

fn track_model(player: &PlayerState, kind: TrackKind) -> ModelRc<TrackItem> {
    ModelRc::new(VecModel::from(
        labels::tracks(&player.tracks, kind)
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
