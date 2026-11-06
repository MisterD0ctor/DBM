//! Everything the UI can ask the player to do.
//!
//! Stateless: each action is a command mpv resolves against its own state
//! (`cycle pause`, `add volume 2`, `seek 50 absolute-percent`) rather than a
//! read, a computation and a write, so mpv stays the single source of truth
//! and a toggle can't disagree with our mirror of it.
//!
//! Failures are logged rather than propagated, since a rejected seek
//! shouldn't take the player down.

use dbm_library::bars::Crop;
use dbm_mpv::prop::{self, Cache, KeepOpen, Rect, TrackChoice};
use dbm_mpv::tracks::{self, TrackId, TrackKind};
use dbm_mpv::{Command, Direction, MediaPath, Mpv, Property, Reader, SeekTarget, ToMpv};
use dbm_naming::Language;

/// Arrow-key seek, in seconds.
pub const SEEK_STEP: f64 = 10.0;
/// Arrow-key volume step.
pub const VOLUME_STEP: f64 = 2.0;
/// Subtitle timing nudge, mpv's own step for `z` / `Z`.
pub const DELAY_STEP: f64 = 0.1;
/// Highest volume mpv will accept here, and the span of the slider.
pub const VOLUME_MAX: f64 = 200.0;
/// One wheel notch over the timeline.
pub const SCROLL_SEEK_STEP: f64 = 1.0;
/// Shift with an arrow: the small seek, for a line of dialogue missed.
pub const FINE_SEEK_STEP: f64 = 1.0;

/// The speeds `<` and `>` step through.
///
/// A ladder rather than mpv's own multiply-by-1.1, which lands on 1.21 and
/// 1.331, numbers nobody asked for or can get back from exactly. Steps go
/// from wherever the speed stands, so a speed set another way joins the
/// ladder at the next rung.
const SPEEDS: &[f64] = &[0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0];

/// The ends of that ladder, for a caller that has to advertise a range
/// rather than step through one (see `mpris`).
// Only MPRIS asks for this; SMTC has no rate or position to set.
#[cfg_attr(windows, allow(dead_code))]
pub fn speed_limits() -> (f64, f64) {
    (SPEEDS[0], SPEEDS[SPEEDS.len() - 1])
}

/// One rung up or down from `current`. At either end there is no rung, and
/// nothing is sent.
pub fn step_speed(mpv: &Mpv, current: f64, step: Direction) {
    let next = match step {
        Direction::Forward => SPEEDS.iter().copied().find(|s| *s > current + 1e-3),
        Direction::Back => SPEEDS.iter().rev().copied().find(|s| *s < current - 1e-3),
    };
    if let Some(speed) = next {
        set(mpv, prop::SPEED, speed);
    }
}

/// Play at a given speed, for a caller that arrived at a number of its own
/// rather than stepping the ladder.
// Only MPRIS asks for this; SMTC has no rate or position to set.
#[cfg_attr(windows, allow(dead_code))]
pub fn set_speed(mpv: &Mpv, speed: f64) {
    set(mpv, prop::SPEED, speed);
}

pub fn reset_speed(mpv: &Mpv) {
    set(mpv, prop::SPEED, 1.0);
}

/// One frame. Forward pauses on arrival, as mpv's own does; back always
/// seeks, so it is slower on a long GOP.
pub fn frame_step(mpv: &Mpv, step: Direction) {
    run(mpv, Command::FrameStep(step));
}

/// The previous or next chapter. mpv's own semantics: back from a little way
/// into a chapter returns to its start before it goes to the one before.
pub fn chapter_step(mpv: &Mpv, step: Direction) {
    run(mpv, Command::ChapterStep(step));
}

/// Which of the two kinds of track the person picks between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Subtitles,
    Audio,
}

impl Pick {
    fn kind(self) -> TrackKind {
        match self {
            Self::Subtitles => TrackKind::Sub,
            Self::Audio => TrackKind::Audio,
        }
    }
}

/// The next subtitle or audio track, wrapping through "none".
///
/// Subtitles are made visible with it, or cycling onto a track hidden by an
/// earlier switch-off would seem to do nothing.
pub fn cycle_track(mpv: &Mpv, pick: Pick, step: Direction) {
    match pick {
        Pick::Subtitles => {
            run(mpv, Command::CycleTrack(prop::SID, step));
            set(mpv, prop::SUB_VISIBILITY, true);
        }
        Pick::Audio => run(mpv, Command::CycleTrack(prop::AID, step)),
    }
}

/// What a queued command's answer is for. The scrubber and the list's load
/// wait on their own, and a move to another file is told apart for when it
/// fails (see `move_to`); everything else fires and forgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Ignored,
    Scrub,
    LoadList,
    Move,
}

impl Reply {
    const ALL: [Self; 4] = [Self::Ignored, Self::Scrub, Self::LoadList, Self::Move];

    /// The id mpv hands back with the answer.
    pub const fn id(self) -> u32 {
        self as u32
    }

    pub fn of(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|reply| reply.id() == id)
    }
}

/// Queue a command. Never blocks.
///
/// Nothing on an interactive path may wait on mpv's core: a synchronous
/// exact seek holds the UI thread for about 200ms.
fn run(mpv: &Mpv, command: Command<'_>) {
    if let Err(e) = mpv.run(Reply::Ignored.id(), command) {
        log::warn!("mpv command {command:?} failed: {e}");
    }
}

/// Queue a property change. Never blocks, and keeps its place among the
/// commands queued around it.
fn set<T: ToMpv>(mpv: &Mpv, property: Property<T>, value: impl Into<T>) {
    if let Err(e) = mpv.set(property, value) {
        log::warn!("mpv set {property:?} failed: {e}");
    }
}

/// Set the paused state outright. `toggle_pause` is preferred where the
/// intent really is a toggle; this is for code that has decided which state
/// it wants, such as the scrubber restoring playback.
pub fn set_pause(mpv: &Mpv, paused: bool) {
    set(mpv, prop::PAUSE, paused);
}

pub fn toggle_pause(mpv: &Mpv) {
    run(mpv, Command::Cycle(prop::PAUSE));
}

/// Toggle pan-and-scan: fill the window by cropping, rather than
/// letterboxing to fit.
///
/// `cycle-values` rather than reading the current value and negating it, so
/// this stays stateless like the rest.
pub fn toggle_panscan(mpv: &Mpv) {
    run(mpv, Command::CycleValues(prop::PANSCAN, &[1.0, 0.0]));
}

/// Show only `crop` of the playing file's frame, or the whole of it.
///
/// File-local, so mpv puts the whole frame back when the file ends and the
/// next one never opens with this one's crop.
pub fn crop(mpv: &Mpv, crop: Option<Crop>) {
    let rect = crop.map(|c| Rect {
        x: c.x,
        y: c.y,
        w: c.w,
        h: c.h,
    });
    set(mpv, prop::FILE_LOCAL_VIDEO_CROP, rect);
}

pub fn toggle_mute(mpv: &Mpv) {
    run(mpv, Command::Cycle(prop::MUTE));
}

/// Toggle caption visibility.
///
/// The one action that can't be a plain `cycle`: turning captions on must
/// first make sure a subtitle track is selected, since `sub-visibility`
/// renders nothing with `sid` unset, as when mpv picked no track or
/// watch-later restored `sid=no`. Turning them off leaves `sid` alone, so the
/// next press brings that track back.
///
/// Reads the track list, so this **blocks**, and is the worker's: the read is
/// fresh in a way a mirrored copy isn't.
pub fn toggle_subtitles(mpv: &Reader) {
    let tracks = mpv.get(prop::TRACK_LIST).unwrap_or_default();
    let subs = tracks::of_kind(&tracks, TrackKind::Sub);
    if subs.is_empty() {
        return;
    }

    // Showing, not merely visible, since `sub-visibility` can be on with no
    // track selected, where turning it off would change nothing visible.
    let showing = mpv.get(prop::SUB_VISIBILITY).unwrap_or(false)
        && tracks::selected(&tracks, TrackKind::Sub).is_some();
    if showing {
        set(mpv, prop::SUB_VISIBILITY, false);
        return;
    }
    // Only trust the current selection if it names a track this file has.
    if tracks::selected(&tracks, TrackKind::Sub).is_none() {
        set(mpv, prop::SID, TrackChoice::Id(subs[0].id));
    }
    set(mpv, prop::SUB_VISIBILITY, true);
}

/// Load a subtitle file into the film playing, named as fetched from
/// OpenSubtitles and in `language` where one is known. Selected and shown when
/// `show` is set; otherwise only added to the list, as the film's own tracks
/// are.
pub fn add_subtitle(mpv: &Mpv, file: &std::path::Path, language: Option<&Language>, show: bool) {
    run(
        mpv,
        Command::SubAdd {
            file,
            select: show,
            title: Some("OpenSubtitles"),
            lang: language.map(Language::tag),
        },
    );
    if show {
        set(mpv, prop::SUB_VISIBILITY, true);
    }
}

/// Take a track that came from a file of its own back out of the film.
/// Returns the file it came from, so a caller can forget it too, and `None`
/// for a track carried in the film, which stays.
///
/// **Blocking**: reads the track list.
pub fn remove_track(mpv: &Reader, id: TrackId, pick: Pick) -> Option<String> {
    let file = mpv
        .get(prop::TRACK_LIST)?
        .into_iter()
        .find(|track| track.id == id && track.kind == pick.kind())?
        .file?;
    let command = match pick {
        Pick::Subtitles => Command::SubRemove(id),
        Pick::Audio => Command::AudioRemove(id),
    };
    run(mpv, command);
    Some(file)
}

/// Select a subtitle track, or `None` to turn them off.
pub fn set_subtitle_track(mpv: &Mpv, id: Option<TrackId>) {
    match id {
        Some(id) => {
            set(mpv, prop::SID, TrackChoice::Id(id));
            set(mpv, prop::SUB_VISIBILITY, true);
        }
        // Leaves `sid` intact so the track stays the one a later toggle
        // brings back, matching the subtitles menu's own "Off" entry.
        None => set(mpv, prop::SUB_VISIBILITY, false),
    }
}

pub fn set_audio_track(mpv: &Mpv, id: Option<TrackId>) {
    set(mpv, prop::AID, id.map_or(TrackChoice::Off, TrackChoice::Id));
}

/// Load a scanned playlist and begin at one of its entries.
///
/// The entry is chosen before the list loads, with mpv's `playlist-start`,
/// rather than selected after: `loadlist` starts entry 0 at once, and a file
/// mpv opens and leaves a moment later counts as stopped at the beginning, so
/// mpv would delete its watch-later entry. Completion still arrives as
/// [`Reply::LoadList`], since other work waits on the list.
///
/// `first`, the entry it starts on, says where the list's files are, since
/// they all come from one folder. Returns whether the list went to mpv,
/// which then answers with that reply.
pub fn load_list(mpv: &Mpv, m3u: &std::path::Path, start: usize, first: &MediaPath) -> bool {
    save_before_leaving(mpv);
    set(mpv, prop::PLAYLIST_START, i64::try_from(start).unwrap_or(0));
    // The document portal is a FUSE mount, which mpv takes for a network
    // share: it reads 150 MB ahead and keeps 50 MB behind, in memory, of files
    // that are almost always on a local disk. Most of its paths are played from
    // where they really are, but not one whose real place is out of reach (see
    // `platform::documents`), which plays as a local file does.
    let portal = crate::platform::documents::is_portal(first.as_path());
    set(
        mpv,
        prop::CACHE,
        if portal { Cache::No } else { Cache::Auto },
    );
    if let Err(e) = mpv.run(Reply::LoadList.id(), Command::LoadList(m3u)) {
        log::warn!("loadlist failed: {e}");
        crate::playback::session::settled();
        return false;
    }
    true
}

pub fn playlist_play(mpv: &Mpv, index: usize) {
    move_to(mpv, Command::PlaylistPlayIndex(index));
}

/// Go to another entry in the playlist, saving the place in this one first.
///
/// One past either end goes nowhere, and mpv's refusal (`Reply::Move`) is
/// then the only sign that nothing is opening after all.
fn move_to(mpv: &Mpv, command: Command<'_>) {
    save_before_leaving(mpv);
    if let Err(e) = mpv.run(Reply::Move.id(), command) {
        log::warn!("mpv command {command:?} failed: {e}");
        crate::playback::session::settled();
    }
}

/// Save the playing file's place before going to another. mpv writes nothing
/// on its way out of a file, and deleted the entry it resumed from as the file
/// opened, so moving on would leave the last checkpoint's place, or none.
///
/// Queued ahead of the move, and mpv runs queued commands in order, so it is
/// still the old file's place. A move made while the last is still opening
/// saves nothing, since that file has no place to save yet.
fn save_before_leaving(mpv: &Mpv) {
    crate::playback::session::checkpoint(mpv);
    crate::playback::session::opening();
}

pub fn seek_relative(mpv: &Mpv, seconds: f64) {
    run(
        mpv,
        Command::Seek {
            to: SeekTarget::By(seconds),
            exact: true,
        },
    );
}

/// Seek to a position given as 0..1 of the file, landing exactly there.
/// Expressed as a percentage so the duration never crosses into this layer.
///
/// Exact seeks decode from the preceding keyframe forward, which is why they
/// are slow. Use [`seek_scrub`] while a drag is in progress, which knows when
/// one is worth it, and this for where the user lets go.
pub fn seek_fraction(mpv: &Mpv, fraction: f32) {
    run(
        mpv,
        Command::Seek {
            to: percent_of(fraction),
            exact: true,
        },
    );
}

/// Seek for a drag in progress: to the keyframe before the target unless
/// `exact`, so mpv needn't decode up to it. The scrubber asks for exactness
/// only while the hand is slow enough to see it. Issued with a distinct reply
/// id, so the scrubber can tell its own completions apart. Returns whether
/// the seek went out, and so whether that reply will come.
pub fn seek_scrub(mpv: &Mpv, fraction: f32, exact: bool) -> bool {
    let seek = Command::Seek {
        to: percent_of(fraction),
        exact,
    };
    if let Err(e) = mpv.run(Reply::Scrub.id(), seek) {
        log::warn!("scrub seek failed: {e}");
        return false;
    }
    true
}

fn percent_of(fraction: f32) -> SeekTarget {
    SeekTarget::Percent(f64::from(fraction.clamp(0.0, 1.0)) * 100.0)
}

/// Put the volume at a level outright, as the bar does under a hand. Clamped
/// to `VOLUME_MAX`, so a drag to the far end can't ask for what mpv rejects.
///
/// Unmutes, since a hand placing the level wants to hear it, and the bar is
/// drawn empty while muted, so nothing would visibly happen. Only here, not
/// in `nudge_volume`: a nudge is an arrow key or a wheel notch, and a key
/// leaned on shouldn't unmute a quiet room.
pub fn set_volume(mpv: &Mpv, value: f64) {
    // Written rather than read and tested, like everything here, since
    // setting what mpv already holds costs nothing and a read would block.
    set(mpv, prop::MUTE, false);
    set(mpv, prop::VOLUME, value.clamp(0.0, VOLUME_MAX));
}

pub fn nudge_volume(mpv: &Mpv, delta: f64) {
    run(mpv, Command::Add(prop::VOLUME, delta));
}

pub fn nudge_sub_delay(mpv: &Mpv, delta: f64) {
    run(mpv, Command::Add(prop::SUB_DELAY, delta));
}

/// Subtitle size, as a multiple of the default.
pub const SUB_SCALE_MIN: f64 = 0.3;
pub const SUB_SCALE_MAX: f64 = 3.0;
pub const SUB_SCALE_STEP: f64 = 0.05;
pub const SUB_SCALE_DEFAULT: f64 = 1.0;

/// The line's Position: 0 is the top of the picture and 100 its foot. Past
/// 100 the line moves down into the letterbox, which is exactly what the bar
/// is usually in the way of, and reaches the window's foot at 150. `subline`
/// turns it into mpv's `sub-pos`, whose range is the same.
pub const SUB_POS_MIN: f64 = 0.0;
pub const SUB_POS_MAX: f64 = 150.0;
pub const SUB_POS_STEP: f64 = 2.0;
pub const SUB_POS_DEFAULT: f64 = 100.0;

/// These two return what they actually set.
///
/// The clamp is here, the one place that knows both the range and that mpv
/// is about to be told, and returning the value lets the caller save the
/// number that was sent.
///
/// Placement is the exception: only `subline` sends it, saving where the
/// person put the line and sending where the bar lets it sit, which differ
/// while the bar is up.
pub fn set_sub_scale(mpv: &Mpv, value: f64) -> f64 {
    let value = value.clamp(SUB_SCALE_MIN, SUB_SCALE_MAX);
    set(mpv, prop::SUB_SCALE, value);
    value
}

/// How mpv draws the picture: each in its own terms, -100 to 100, and none
/// by default. A preference rather than a film's, as the subtitles' size is:
/// what a screen or a room needs, every film needs.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct PictureParams {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub gamma: f32,
}

impl PictureParams {
    /// Each with the mpv property it sets.
    pub fn properties(&self) -> [(Property<i64>, f32); 4] {
        [
            (prop::BRIGHTNESS, self.brightness),
            (prop::CONTRAST, self.contrast),
            (prop::SATURATION, self.saturation),
            (prop::GAMMA, self.gamma),
        ]
    }
}

/// How the picture is drawn: its brightness, contrast, saturation and gamma,
/// each a whole number from -100 to 100, as mpv takes them.
pub fn set_picture(mpv: &Mpv, picture: &PictureParams) {
    for (property, value) in picture.properties() {
        set(mpv, property, value.round().clamp(-100.0, 100.0) as i64);
    }
}

pub fn set_sub_pos(mpv: &Mpv, value: f64) -> f64 {
    let value = value.clamp(SUB_POS_MIN, SUB_POS_MAX);
    set(mpv, prop::SUB_POS, value);
    value
}

/// Whether mpv lays subtitles out against the whole window, letterbox and
/// all, or against the picture alone. Text and ASS subtitles have an option
/// each; bitmaps follow neither and stay in the picture.
pub fn set_sub_frame(mpv: &Mpv, window: bool) {
    set(mpv, prop::SUB_USE_MARGINS, window);
    set(mpv, prop::SUB_ASS_FORCE_MARGINS, window);
}

/// Timing is per file rather than a preference, since a delay that fixes one
/// set of subtitles is wrong for the next, so mpv's watch-later file keeps it
/// (see `session::configure`).
pub fn set_sub_delay(mpv: &Mpv, seconds: f64) {
    set(mpv, prop::SUB_DELAY, seconds);
}

/// The speed subtitles run at against the film: 1, unless they were timed
/// for a transfer at another frame rate (see `subsync`). Per file, like the
/// delay it goes with.
pub fn set_sub_speed(mpv: &Mpv, speed: f64) {
    set(mpv, prop::SUB_SPEED, speed);
}

/// Whether reaching the end of a file advances to the next.
///
/// mpv expresses this through `keep-open`: `yes` holds the window open only
/// at the end of the playlist, so files flow into one another; `always`
/// stops at the end of every file. There is no separate autoplay flag.
pub fn set_autoplay(mpv: &Mpv, on: bool) {
    set(
        mpv,
        prop::KEEP_OPEN,
        if on { KeepOpen::Yes } else { KeepOpen::Always },
    );
}

/// Play this file again from the start.
///
/// Two commands rather than one because a finished file is paused: seeking
/// back to zero without unpausing lands on the first frame and stays there.
pub fn replay(mpv: &Mpv) {
    restart(mpv);
    set_pause(mpv, false);
}

/// Move through the playlist and play what is there, for the end-of-file
/// button, a headset's media keys, and anything else that means "go on"
/// rather than "select".
///
/// The unpause is queued behind the step rather than timed after it, since
/// mpv runs queued commands in order.
pub fn advance(mpv: &Mpv, step: Direction) {
    playlist_step(mpv, step);
    set_pause(mpv, false);
}

pub fn playlist_step(mpv: &Mpv, step: Direction) {
    move_to(
        mpv,
        match step {
            Direction::Forward => Command::PlaylistNext,
            Direction::Back => Command::PlaylistPrev,
        },
    );
}

/// Seek to a point, counted in seconds from the start. Exact, like every
/// seek that is not a drag in progress.
pub fn seek_to(mpv: &Mpv, seconds: f64) {
    run(
        mpv,
        Command::Seek {
            to: SeekTarget::To(seconds),
            exact: true,
        },
    );
}

pub fn restart(mpv: &Mpv) {
    seek_to(mpv, 0.0);
}
