//! Moving data between mpv, the mirrored state, and the UI.
//!
//! One direction only: mpv is the source of truth, this carries its values
//! outward. Anything travelling the other way is an action, and lives in
//! `actions`/`commands`.
//!
//! Everything here is called once per frame from the render driver, so the
//! rule that matters is: **never make a blocking property read on this
//! path.** `mpv_get_property_string` waits on mpv's core lock, and during a
//! resize that lock is contended enough to cost hundreds of milliseconds a
//! frame. Scalars are observed and arrive on the event queue; the list-shaped
//! properties are read only when a count or selection actually moves.

use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::mpv::{Event, Mpv};
use crate::worker::{Completion, Lists, Worker};
use crate::pipeline::{GlassPanel, MAX_PANELS};
use crate::state::{self, PlayerState};
use crate::tracks::{self, TrackKind};
use crate::settings::{self, Section, Store};
use crate::{MainWindow, ParamItem, PlaylistGroup, PlaylistItem, TrackItem};

/// Drain mpv's event queue into the mirrored state.
///
/// Returns whether anything the UI shows changed. Draining here rather than
/// on a thread of its own keeps `mpv_wait_event` single-threaded — it is not
/// safe to call concurrently — and means events still flow while Windows owns
/// the loop during a resize drag, since the modal hook keeps frames coming.
pub fn drain_events(
    mpv: &Mpv,
    player: &mut PlayerState,
    replies: &mut Vec<u64>,
    notices: &mut Vec<String>,
    audio: &Rc<crate::audio::Watchdog>,
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
            let reason = capitalised(reason);
            notices.push(match &path {
                Some(path) => format!(
                    "{reason} — could not play {}",
                    crate::naming::titled(path, None)
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

/// mpv's reasons are lower-case fragments — "unrecognized file format" —
/// and here one starts a sentence.
fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Push the scalar values the UI binds to.
pub fn push_scalars(ui: &MainWindow, player: &PlayerState) {
    ui.set_media_title(player.display_title().into());
    ui.set_elapsed(state::format_time(player.time_pos).into());
    ui.set_total(state::format_time(player.duration).into());
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
        player.sub_visibility
            && tracks::selected(&player.tracks, TrackKind::Sub).is_some(),
    );
    ui.set_panscan(player.panscan > 0.5);
    ui.set_sub_delay_text(format_delay(player.sub_delay).into());
    ui.set_sub_scale_text(format!("{:.2}×", player.sub_scale).into());
    ui.set_sub_pos_text(format!("{:.0}%", player.sub_pos).into());
    // Whether anything is loaded at all.
    //
    // Derived from mpv every frame rather than set once at startup. It used to
    // be written exactly once, in `main`, from the command line — so a file
    // opened through the dialog, the folder button or a drop never touched it,
    // and the interface spent the whole film insisting "No file open" while
    // showing a play glyph over a running picture. The end-of-playback button
    // is gated on this too, so it never appeared for those files either.
    ui.set_has_file(player.path.is_some());
    // And the open is over the moment mpv admits to a file. Cleared from the
    // same place `has_file` is derived, so the two cannot disagree: the bar
    // arriving and the "Opening…" line leaving are one event.
    if player.path.is_some() {
        ui.set_opening(false);
    }
    // A finished file, and whether there is another one after it. mpv only
    // reports `eof-reached` while it is holding the last frame open, which is
    // exactly when the interface has something to offer.
    ui.set_at_end(player.eof_reached && player.path.is_some());
    ui.set_at_last(player.playlist_pos + 1 >= player.playlist_count);
    // The other end, so previous can say there is nothing before this.
    ui.set_at_first(player.playlist_pos <= 0);
    ui.set_playlist_count(player.playlist_count as i32);
    // Raw, beside the formatted text: undoing a reset has to put back the
    // number, and parsing it out of "+0.30 s" would be reading our own label.
    ui.set_sub_delay(player.sub_delay as f32);
    ui.set_speed(if player.speed > 0.0 { player.speed as f32 } else { 1.0 });
    ui.set_speed_text(state::format_speed(player.speed).into());
    ui.set_duration(player.duration as f32);
    ui.set_chapter_label(player.chapter_label().into());
    ui.set_credits_rolling(player.credits_rolling());
    // Where the dialogs open: beside this file, or on the shelf above its
    // folder.
    ui.set_current_path(player.path.clone().unwrap_or_default().into());
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
                progress: crate::durations::of(&paths),
                playlist,
                generation,
            }))
        });
    }

    /// Take delivery of a finished read.
    pub fn apply(
        &mut self,
        ui: &MainWindow,
        player: &mut PlayerState,
        lists: Lists,
    ) {
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
        player.chapters.iter().map(|c| c.time as f32).collect::<Vec<_>>(),
    )));
    ui.set_chapter_label(player.chapter_label().into());
    // The chapter's name arrives with the list, a moment after its number.
    ui.set_credits_rolling(player.credits_rolling());
    push_playlist(ui, player);
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

/// Hand the playlist alone to the UI.
///
/// Separate so a scan result, which changes nothing about the tracks, does
/// not rebuild the subtitle and audio menus as well — and so a row the
/// pointer is resting on is recreated no more often than it has to be.
pub fn push_playlist(ui: &MainWindow, player: &PlayerState) {
    use slint::Model as _;

    let items: Vec<crate::shelf::Item> = player
        .playlist
        .iter()
        .enumerate()
        .map(|(row, e)| crate::shelf::Item {
            path: &e.filename,
            title: player.known_title(row),
            index: e.index,
            current: e.current,
            progress: player.known_progress(row),
            failed: player.failed.contains(&e.filename),
        })
        .collect();
    let shelf = crate::shelf::arrange(&items, &player.beside);

    ui.set_playlist_named(shelf.heading.is_some());
    ui.set_playlist_heading(match shelf.title() {
        Some(title) => title.into(),
        None => slint::SharedString::from("PLAYLIST"),
    });
    // Where the panel opens: the part holding the file playing, on its row.
    let current = shelf.groups.iter().position(|g| g.current().is_some());
    ui.set_playlist_current_group(current.map_or(-1, |g| g as i32));
    ui.set_playlist_current(
        current
            .and_then(|g| shelf.groups[g].current())
            .map_or(-1, |row| row as i32),
    );
    ui.set_playlist_drills(shelf.groups.iter().any(|g| !g.leaf));

    // The page on show is kept by its name, not its place. Seasons found
    // beside this one arrive a moment after it and go in around it, and a
    // page held by number would turn into another season under the reader.
    let shown = usize::try_from(ui.get_playlist_page())
        .ok()
        .and_then(|page| ui.get_playlist_groups().row_data(page))
        .map(|group| group.label);
    let groups: Vec<PlaylistGroup> = shelf.groups.iter().map(group_item).collect();
    if let Some(label) = shown {
        ui.set_playlist_page(
            groups
                .iter()
                .position(|g| g.label == label)
                .map_or(-1, |page| page as i32),
        );
    }
    ui.set_playlist_groups(ModelRc::new(VecModel::from(groups)));
}

/// One part of the list, as the panel draws it.
fn group_item(group: &crate::shelf::Group) -> PlaylistGroup {
    let count = group.entries.len();
    PlaylistGroup {
        label: group.label.as_str().into(),
        // A heading shouts the interface's own words and never the user's:
        // SEASON 7, but a show's name in its own case.
        heading: (if group.named {
            group.label.clone()
        } else {
            group.label.to_uppercase()
        })
        .into(),
        titular: group.named,
        detail: if count == 1 {
            "1 episode".into()
        } else {
            format!("{count} episodes").into()
        },
        current: group.current().is_some(),
        leaf: group.leaf,
        open_row: group.open_row() as i32,
        entries: ModelRc::new(VecModel::from(
            group.entries.iter().map(entry_item).collect::<Vec<_>>(),
        )),
    }
}

fn entry_item(entry: &crate::shelf::Entry) -> PlaylistItem {
    PlaylistItem {
        index: entry.index.map_or(-1, |i| i as i32),
        path: entry.path.as_str().into(),
        label: entry.label.as_str().into(),
        current: entry.current,
        // A file never played here has no length to show and no progress to
        // draw; an empty string and a zero say so, and the row leaves both
        // out rather than printing "0:00".
        length: if entry.progress.seconds > 0.0 {
            state::format_time(entry.progress.seconds).into()
        } else {
            Default::default()
        },
        progress: entry.progress.fraction,
        finished: entry.progress.finished,
        failed: entry.failed,
    }
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

/// Read the glass panel geometry out of Slint's own layout.
///
/// Same process, same frame, read synchronously — so the glass cannot lag the
/// widget it belongs to. Slint works in logical pixels and the pipeline in
/// physical ones, hence the scale factor.
pub fn collect_panels(ui: &MainWindow, out: &mut Vec<GlassPanel>) {
    let dpi = ui.window().scale_factor();
    out.clear();
    let rects = ui.get_glass_rects();
    for i in 0..rects.row_count() {
        let Some(g) = rects.row_data(i) else { continue };
        // A hidden panel publishes a zero-sized rect rather than dropping out
        // of the array, which keeps the UI side a plain literal. Skip those.
        // A panel mid-fade still needs glass; one faded out entirely is as
        // absent as one that was never opened.
        if g.width <= 0.0 || g.height <= 0.0 || g.opacity <= 0.004 {
            continue;
        }
        // The cap counts panels that are really on screen, not how far into
        // the declaration list we have read. Capping the read instead meant
        // that once the list grew past `MAX_PANELS` — which it did the moment
        // the timeline became two pills — whatever was declared last silently
        // got no glass, however few panels were actually open. Every entry
        // past the live ones is a placeholder for something closed.
        if out.len() == MAX_PANELS {
            warn_overflow();
            return;
        }
        out.push(GlassPanel {
            rect: [
                g.x * dpi,
                g.y * dpi,
                (g.x + g.width) * dpi,
                (g.y + g.height) * dpi,
            ],
            radius: g.radius * dpi,
            tint_alpha: g.tint,
            opacity: g.opacity,
        });
    }
}

/// Said once, not once a frame. Losing glass off the end of the list is quiet
/// enough that it wants saying at all, and repeating it sixty times a second
/// would bury everything else.
fn warn_overflow() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        eprintln!("dbm: more than {MAX_PANELS} glass panels on screen; the rest get none");
    });
}

/// Rebuild the settings rows from the registry and the current values.
///
/// Called when a value changes rather than every frame: the model is a fresh
/// allocation and the panel is usually closed.
/// The slider rows, split by the page they appear on.
///
/// Two models rather than one filtered in the UI: each page binds to its own
/// list, and a row carries its registry index so the callback needs no
/// arithmetic to map back.
pub struct ParamModels {
    glass: Rc<VecModel<ParamItem>>,
    border: Rc<VecModel<ParamItem>>,
}

impl ParamModels {
    pub fn build(ui: &MainWindow, store: &Store) -> Self {
        let models = Self {
            glass: section_model(store, Section::Glass),
            border: section_model(store, Section::Border),
        };
        ui.set_glass_params(ModelRc::from(models.glass.clone()));
        ui.set_border_params(ModelRc::from(models.border.clone()));
        models
    }

    /// Refresh one row in place.
    ///
    /// Deliberately not a rebuild: replacing a model makes the repeater tear
    /// down and recreate every row, destroying the `TouchArea` the pointer is
    /// holding — which is what once made the sliders click-only.
    pub fn update(&self, store: &Store, index: usize) {
        let Some(param) = settings::REGISTRY.get(index) else {
            return;
        };
        let model = self.model_for(param.section);
        if let Some(row) = rows_of(param.section).position(|i| i == index) {
            model.set_row_data(row, param_row(store, index));
        }
    }

    /// Rebuild both pages. For a reset, where every row moved at once and no
    /// drag is in progress.
    pub fn refresh_all(&self, store: &Store) {
        for (section, model) in [
            (Section::Glass, &self.glass),
            (Section::Border, &self.border),
        ] {
            for (row, index) in rows_of(section).enumerate() {
                model.set_row_data(row, param_row(store, index));
            }
        }
    }

    fn model_for(&self, section: Section) -> &VecModel<ParamItem> {
        match section {
            Section::Glass => &self.glass,
            Section::Border => &self.border,
        }
    }
}

fn rows_of(section: Section) -> impl Iterator<Item = usize> {
    (0..settings::REGISTRY.len()).filter(move |i| settings::REGISTRY[*i].section == section)
}

fn section_model(store: &Store, section: Section) -> Rc<VecModel<ParamItem>> {
    Rc::new(VecModel::from(
        rows_of(section)
            .map(|i| param_row(store, i))
            .collect::<Vec<_>>(),
    ))
}

fn param_row(store: &Store, index: usize) -> ParamItem {
    let param = &settings::REGISTRY[index];
    let value = store.value(index);
    let span = (param.max - param.min).max(f32::EPSILON);
    ParamItem {
        index: index as i32,
        label: param.label.into(),
        fraction: ((value - param.min) / span).clamp(0.0, 1.0),
        readout: format_value(value, param.min, param.max).into(),
    }
}

/// Signed, because the sign is the whole point: a delay says whether the
/// subtitles are running early or late. Zero is written without one, so the
/// untouched case does not read as a setting someone made.
fn format_delay(seconds: f64) -> String {
    if seconds.abs() < 0.005 {
        "0.00 s".into()
    } else {
        format!("{seconds:+.2} s")
    }
}

/// Enough precision to tune by, without a column of noise.
///
/// From the parameter's range rather than its current value, which is what
/// the value alone cannot tell you: 5.000 is three meaningless digits on a
/// blur that runs to 40, and 0.010 is the whole story on an edge blur that
/// runs to 0.1. Same number, opposite needs.
fn format_value(value: f32, min: f32, max: f32) -> String {
    let span = (max - min).abs();
    let decimals = if span >= 100.0 {
        0
    } else if span >= 10.0 {
        1
    } else if span >= 1.0 {
        2
    } else {
        3
    };
    format!("{value:.decimals$}")
}
