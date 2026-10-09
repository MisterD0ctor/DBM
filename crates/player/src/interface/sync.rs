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

use crate::interface::{format, playlist_panel};
use crate::library::durations::Progress;
use crate::playback::state::PlayerState;
use crate::playback::tracks::{self, Chapter, PlaylistEntry, Track, TrackKind};
use crate::worker::{Completion, Worker};
use crate::{MainWindow, TrackItem};

/// Push the scalar values the UI binds to.
pub fn push_scalars(ui: &MainWindow, player: &PlayerState) {
    ui.set_media_title(player.display_title().into());
    ui.set_elapsed(format::time(player.time_pos).into());
    ui.set_total(format::time(player.duration).into());
    ui.set_progress(player.progress());
    ui.set_paused(player.paused);
    ui.set_muted(player.muted);
    ui.set_volume(player.volume as f32);
    // mpv's `sub-visibility` is only half of it: it can be on with no
    // subtitle track selected, which puts nothing on screen. The interface
    // asks whether subtitles are showing, and answers the tracks menu's Off
    // row and the bar's own glyph with it — both of which were claiming
    // subtitles were on for a file mpv had chosen none for.
    ui.set_subs_visible(
        player.sub_visibility && tracks::selected(&player.tracks, TrackKind::Sub).is_some(),
    );
    ui.set_panscan(player.panscan > 0.5);
    ui.set_sub_delay_text(format::delay(player.sub_delay).into());
    ui.set_sub_scale_text(format::scale(player.sub_scale).into());
    // Whether anything is loaded at all.
    //
    // Derived from mpv every frame rather than set once at startup. It used to
    // be written exactly once, in `main`, from the command line — so a file
    // opened through the dialog, the folder button or a drop never touched it,
    // and the interface spent the whole film insisting "No file open" while
    // showing a play glyph over a running picture. The end-of-playback button
    // is gated on this too, so it never appeared for those files either.
    ui.set_has_file(player.path.is_some());
    // A finished file, and whether there is another one after it. mpv only
    // reports `eof-reached` while it is holding the last frame open, which is
    // exactly when the interface has something to offer.
    ui.set_at_end(player.eof_reached && player.path.is_some());
    ui.set_at_last(player.playlist_pos + 1 >= player.playlist_count);
    // The other end, so previous can say there is nothing before this.
    ui.set_at_first(player.playlist_pos <= 0);
    // Raw, beside the formatted text: undoing a reset has to put back the
    // number, and parsing it out of "+0.30 s" would be reading our own label.
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
    // Where the dialogs open: beside this file, or on the shelf above its
    // folder.
    ui.set_current_path(player.path.clone().unwrap_or_default().into());
}

/// Track, chapter and playlist contents, read together because they are
/// invalidated together and a single job means a single round trip.
pub struct Lists {
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// One per playlist entry, in the same order: how long it is and how far
    /// into it the resume point sits. Read here rather than in a job of its
    /// own because it is answered *from* the playlist — the paths have to be
    /// known before the lookup can start, and they are known right here.
    pub progress: Vec<Progress>,
    /// The file's chapter marks. Invalidated with the tracks, by a new file.
    pub chapters: Vec<Chapter>,
    /// The generation this was read for, so a stale result can be discarded
    /// if the lists moved again while the job was running.
    pub generation: u64,
}

/// Requests list reads and applies the results.
///
/// The reads themselves are ~80ms of blocking property calls, so they happen
/// on the worker. This keeps one request in flight and remembers what it was
/// for, which is the same shape as the scrubber: coalesce, and always
/// converge on the newest state rather than replaying every intermediate one.
#[derive(Default)]
pub struct ListSync {
    in_flight: bool,
    requested: u64,
    /// Ask again even though nothing mpv knows about has changed.
    ///
    /// The progress on each playlist row comes from files on disk that mpv
    /// writes without telling anyone — the resume position of the file
    /// playing now is rewritten every ten seconds. Nothing in the event
    /// stream marks that, so the one moment it matters is used instead: when
    /// somebody opens the playlist to look at it.
    forced: bool,
}

impl ListSync {
    /// Read again on the next poll, whatever the generation says.
    pub fn refresh(&mut self) {
        self.forced = true;
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

    /// Take delivery of a finished read.
    pub fn apply(&mut self, ui: &MainWindow, player: &mut PlayerState, lists: Lists) {
        self.in_flight = false;
        if player.apply_lists(lists) {
            push_lists(ui, player);
        }
    }
}

/// Hand the current lists to the UI as models.
fn push_lists(ui: &MainWindow, player: &PlayerState) {
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
    playlist_panel::push(ui, player);
}

/// Which row of a track list is the selected one, or -1.
///
/// Worked out here because the interface cannot search a model, and it needs
/// the answer to open the tracks panel on the track in use rather than at the
/// top of a list of fifteen.
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
            })
            .collect::<Vec<_>>(),
    ))
}
