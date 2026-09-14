//! Everything the UI can ask the player to do.
//!
//! Deliberately stateless. Each action is expressed as a command mpv resolves
//! against its own state — `cycle pause`, `add volume 2`,
//! `seek 50 absolute-percent` — rather than "read the current value, compute
//! the new one, write it back". mpv stays the single source of truth, and a
//! toggle can never disagree with the copy of state we happen to be mirroring
//! at the time.
//!
//! Failures are logged rather than propagated: a rejected seek should not
//! take the player down with it.

use crate::mpv::Mpv;

/// Arrow-key seek, in seconds. Matches the Tauri build.
pub const SEEK_STEP: f64 = 10.0;
/// Arrow-key volume step.
pub const VOLUME_STEP: f64 = 2.0;
/// Subtitle timing nudge — mpv's own step for `z` / `Z`.
pub const DELAY_STEP: f64 = 0.1;
/// Highest volume mpv will accept here, and the span of the slider.
pub const VOLUME_MAX: f64 = 200.0;
/// One wheel notch over the timeline.
pub const SCROLL_SEEK_STEP: f64 = 1.0;
/// Shift with an arrow: the small seek, for a line of dialogue missed.
pub const FINE_SEEK_STEP: f64 = 1.0;

/// The speeds `[` and `]` step through.
///
/// A ladder rather than mpv's own multiply-by-1.1, which lands on 1.21 and
/// 1.331 — numbers nobody asked for and nobody can get back from exactly.
/// Stepping from wherever the speed stands, so a speed set some other way
/// joins the ladder at the next rung rather than being snapped to one.
const SPEEDS: &[f64] = &[0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0];

/// One rung up or down from `current`. At either end there is no rung, and
/// nothing is sent.
pub fn step_speed(mpv: &Mpv, current: f64, direction: i32) {
    let next = if direction > 0 {
        SPEEDS.iter().copied().find(|s| *s > current + 1e-3)
    } else {
        SPEEDS.iter().rev().copied().find(|s| *s < current - 1e-3)
    };
    if let Some(speed) = next {
        set_prop(mpv, "speed", &fmt(speed));
    }
}

pub fn reset_speed(mpv: &Mpv) {
    set_prop(mpv, "speed", "1");
}

/// One frame. Forward pauses on arrival, as mpv's own does; back always
/// seeks, so it is slower on a long GOP and there is nothing to be done
/// about that from here.
pub fn frame_step(mpv: &Mpv, direction: i32) {
    run(mpv, &[if direction >= 0 { "frame-step" } else { "frame-back-step" }]);
}

/// The previous or next chapter. mpv's own semantics: back from a little way
/// into a chapter returns to its start before it goes to the one before.
pub fn chapter_step(mpv: &Mpv, direction: i32) {
    run(mpv, &["add", "chapter", if direction >= 0 { "1" } else { "-1" }]);
}

/// The next subtitle or audio track, wrapping through "none".
///
/// Subtitles are made visible with it: cycling onto a track that then stays
/// hidden because they were switched off earlier would be a key that
/// appeared to do nothing.
pub fn cycle_track(mpv: &Mpv, subtitles: bool, direction: i32) {
    let way = if direction >= 0 { "up" } else { "down" };
    if subtitles {
        run(mpv, &["cycle", "sub", way]);
        set_prop(mpv, "sub-visibility", "yes");
    } else {
        run(mpv, &["cycle", "audio", way]);
    }
}

/// Reply ids for async commands. Only the scrubber cares which of its own
/// requests came back; everything else fires and forgets.
pub const REPLY_IGNORED: u64 = 0;
pub const REPLY_SCRUB: u64 = 1;
pub const REPLY_LOADLIST: u64 = 2;

/// Queue a command. Never blocks.
///
/// Measured before this was async: one exact seek held the UI thread for
/// ~195ms, so a drag froze the interface for a fifth of a second per mouse
/// move. Nothing on an interactive path may wait on mpv core.
fn run(mpv: &Mpv, args: &[&str]) {
    if let Err(e) = mpv.command_async(REPLY_IGNORED, args) {
        eprintln!("dbm: mpv command {args:?} failed: {e}");
    }
}

/// Set the paused state outright. `toggle_pause` is preferred where the
/// intent really is a toggle; this is for code that has decided which state
/// it wants, such as the scrubber restoring playback.
pub fn set_pause(mpv: &Mpv, paused: bool) {
    set_prop(mpv, "pause", if paused { "yes" } else { "no" });
}

pub fn toggle_pause(mpv: &Mpv) {
    run(mpv, &["cycle", "pause"]);
}

/// Toggle pan-and-scan: fill the window by cropping, rather than
/// letterboxing to fit.
///
/// `cycle-values` rather than reading the current value and negating it, so
/// this stays stateless like the rest — mpv walks the list itself.
pub fn toggle_panscan(mpv: &Mpv) {
    run(mpv, &["cycle-values", "panscan", "1.0", "0.0"]);
}

pub fn toggle_mute(mpv: &Mpv) {
    run(mpv, &["cycle", "mute"]);
}

/// Toggle caption visibility.
///
/// The one action that cannot be a plain `cycle`. Turning captions *on* has
/// to make sure a subtitle track is actually selected first: `sub-visibility`
/// on its own renders nothing when `sid` is unset, which is where a file
/// lands whenever mpv picked no subtitle track (none matching `slang`, none
/// default) or watch-later restored `sid=no` from a session where they were
/// off.
///
/// Turning them off deliberately leaves `sid` alone, so it stays the
/// "previously active" track for the next press.
///
/// Reads the track list, so this **blocks** and must run on the worker.
/// Reading at the moment of the decision is guaranteed fresh in a way a
/// mirrored copy is not, which is worth a thread hop.
pub fn toggle_subtitles(mpv: &Mpv) {
    let tracks = crate::tracks::read_tracks(mpv);
    let subs = crate::tracks::of_kind(&tracks, crate::tracks::TrackKind::Sub);
    if subs.is_empty() {
        return;
    }

    // Showing, not merely visible. `sub-visibility` can be on for a file mpv
    // selected no subtitle track for, which puts nothing on screen — and
    // turning that off spent the press without changing anything anybody
    // could see. The next press did the work, so the key took two goes the
    // first time it was used on such a file.
    let showing = mpv.get_bool("sub-visibility")
        && crate::tracks::selected(&tracks, crate::tracks::TrackKind::Sub).is_some();
    if showing {
        set_prop(mpv, "sub-visibility", "no");
        return;
    }
    // Only trust the current selection if it names a track this file has.
    if crate::tracks::selected(&tracks, crate::tracks::TrackKind::Sub).is_none() {
        set_prop(mpv, "sid", &subs[0].id.to_string());
    }
    set_prop(mpv, "sub-visibility", "yes");
}

/// Select a subtitle track, or `None` to turn them off.
pub fn set_subtitle_track(mpv: &Mpv, id: Option<i64>) {
    match id {
        Some(id) => {
            set_prop(mpv, "sid", &id.to_string());
            set_prop(mpv, "sub-visibility", "yes");
        }
        // Leaves `sid` intact so the track stays the one a later toggle
        // brings back, matching the tracks menu's own "Off" entry.
        None => set_prop(mpv, "sub-visibility", "no"),
    }
}

pub fn set_audio_track(mpv: &Mpv, id: Option<i64>) {
    let value = id.map(|i| i.to_string()).unwrap_or_else(|| "no".into());
    set_prop(mpv, "aid", &value);
}

/// Load a scanned playlist and begin at one of its entries.
///
/// The entry is chosen *before* the list is loaded, rather than selected
/// after it. Selecting afterwards costs the first episode its resume
/// position: `loadlist` starts playing entry 0 the moment it lands, and a
/// file mpv opens and leaves a moment later is a file mpv considers stopped
/// at the beginning — so it deletes that file's watch-later entry on the way
/// out. Opening episode five of a folder quietly wiped episode one, every
/// time, and nothing said so.
///
/// Measured, not assumed: staging resume positions for a folder and opening
/// the second episode left the first one's file gone and the rest untouched.
///
/// `playlist-start` is mpv's own answer to this and applies to the next
/// playlist it loads. Completion still arrives as [`REPLY_LOADLIST`], since
/// there is other work waiting on the list existing.
pub fn load_list(mpv: &Mpv, m3u: &std::path::Path, start: usize) {
    set_prop(mpv, "playlist-start", &start.to_string());
    let args = ["loadlist", &*m3u.to_string_lossy(), "replace"];
    if let Err(e) = mpv.command_async(REPLY_LOADLIST, &args) {
        eprintln!("dbm: loadlist failed: {e}");
    }
}

pub fn playlist_play(mpv: &Mpv, index: i64) {
    run(mpv, &["playlist-play-index", &index.to_string()]);
}

/// Set a property without waiting. Expressed as mpv `set` command so it can
/// go through the same async path; `mpv_set_property_async` would need a
/// typed payload for no benefit here.
fn set_prop(mpv: &Mpv, name: &str, value: &str) {
    run(mpv, &["set", name, value]);
}

pub fn seek_relative(mpv: &Mpv, seconds: f64) {
    run(mpv, &["seek", &fmt(seconds), "relative+exact"]);
}

/// Seek to a position given as 0..1 of the file, landing exactly there.
/// Expressed as a percentage so the duration never crosses into this layer.
///
/// Exact seeks decode from the preceding keyframe forward, which is why they
/// are slow. Use [`seek_scrub`] while a drag is in progress and save this for
/// where the user lets go.
pub fn seek_fraction(mpv: &Mpv, fraction: f32) {
    run(mpv, &["seek", &fmt(percent_of(fraction)), "absolute-percent+exact"]);
}

/// Seek for a drag in progress: keyframe-accurate, so mpv can satisfy it
/// without decoding up to the target. Issued with a distinct reply id so the
/// scrubber can tell its own completions apart.
pub fn seek_scrub(mpv: &Mpv, fraction: f32) {
    let args = ["seek", &fmt(percent_of(fraction)), "absolute-percent"];
    if let Err(e) = mpv.command_async(REPLY_SCRUB, &args) {
        eprintln!("dbm: scrub seek failed: {e}");
    }
}

fn percent_of(fraction: f32) -> f64 {
    f64::from(fraction.clamp(0.0, 1.0)) * 100.0
}

/// Set the volume outright, for the slider. Clamped to mpv own ceiling so
/// a drag to the far end cannot ask for something it will reject.
/// Put the volume at a level outright — what the bar does under a hand.
///
/// Unmutes. A hand that places the level has said what it wants to hear, and
/// mute would answer with a number nobody can hear: the bar draws itself
/// empty while muted, so the fill would not even move. Clicking a control and
/// having nothing happen at all is how a control stops being believed.
///
/// Only here, not in `nudge_volume`. Placing a level is a statement; a nudge
/// is two units on a wheel that also turns over the video, nowhere near the
/// mute button, and mute is the one setting people reach for because the room
/// demands it. A wheel brushed by a sleeve should not fill a room with sound.
pub fn set_volume(mpv: &Mpv, value: f64) {
    // Written rather than read and tested, like everything else here: mpv
    // owns the state, setting a property to what it already holds costs
    // nothing, and a read would block the gesture asking for it.
    set_prop(mpv, "mute", "no");
    set_prop(mpv, "volume", &fmt(value.clamp(0.0, VOLUME_MAX)));
}

pub fn nudge_volume(mpv: &Mpv, delta: f64) {
    run(mpv, &["add", "volume", &fmt(delta)]);
}

pub fn nudge_sub_delay(mpv: &Mpv, delta: f64) {
    run(mpv, &["add", "sub-delay", &fmt(delta)]);
}

/// Subtitle size, as a multiple of the default.
pub const SUB_SCALE_MIN: f64 = 0.3;
pub const SUB_SCALE_MAX: f64 = 3.0;
pub const SUB_SCALE_STEP: f64 = 0.05;
pub const SUB_SCALE_DEFAULT: f64 = 1.0;

/// Vertical placement, in mpv's own units: 0 is the top of the frame and 100
/// the bottom. Past 100 the line moves down into the letterbox, which is
/// exactly what the bar is usually in the way of, so the range runs to mpv's
/// full 150 rather than stopping at the picture.
pub const SUB_POS_MIN: f64 = 0.0;
pub const SUB_POS_MAX: f64 = 150.0;
pub const SUB_POS_STEP: f64 = 2.0;
pub const SUB_POS_DEFAULT: f64 = 100.0;

/// These two return what they actually set.
///
/// The clamp has to happen somewhere, and here is the only place that knows
/// both the range and that mpv is about to be told. Returning the value lets
/// the caller record the same number that was sent, so a saved setting can
/// never drift from what the player is doing.
pub fn set_sub_scale(mpv: &Mpv, value: f64) -> f64 {
    let value = value.clamp(SUB_SCALE_MIN, SUB_SCALE_MAX);
    set_prop(mpv, "sub-scale", &fmt(value));
    value
}

pub fn set_sub_pos(mpv: &Mpv, value: f64) -> f64 {
    let value = value.clamp(SUB_POS_MIN, SUB_POS_MAX);
    set_prop(mpv, "sub-pos", &fmt(value));
    value
}

/// Timing is per file rather than a preference — a delay that fixes one set
/// of subtitles is wrong for the next — so mpv's own watch-later file keeps
/// it, and there is nothing here to remember.
pub fn set_sub_delay(mpv: &Mpv, seconds: f64) {
    set_prop(mpv, "sub-delay", &fmt(seconds));
}

/// Whether reaching the end of a file advances to the next.
///
/// mpv expresses this through `keep-open`: `yes` holds the window open only
/// at the end of the playlist, so files flow into one another; `always`
/// stops at the end of every file. There is no separate autoplay flag.
pub fn set_autoplay(mpv: &Mpv, on: bool) {
    set_prop(mpv, "keep-open", if on { "yes" } else { "always" });
}

/// Play this file again from the start.
///
/// Two commands rather than one because a finished file is paused: seeking
/// back to zero without unpausing lands on the first frame and stays there.
pub fn replay(mpv: &Mpv) {
    restart(mpv);
    set_pause(mpv, false);
}

/// Move through the playlist and play what is there — the end-of-file
/// button, the media keys on a headset, and anything else that means "go on"
/// rather than merely "select".
///
/// The unpause is queued behind the step rather than timed after it: mpv
/// executes queued commands in order, so by the time the property is written
/// the new file is the current one. The Tauri build slept 100ms here.
pub fn advance(mpv: &Mpv, delta: i32) {
    playlist_step(mpv, delta);
    set_pause(mpv, false);
}

pub fn playlist_step(mpv: &Mpv, delta: i32) {
    run(
        mpv,
        if delta >= 0 {
            &["playlist-next"]
        } else {
            &["playlist-prev"]
        },
    );
}

pub fn restart(mpv: &Mpv) {
    run(mpv, &["seek", "0", "absolute+exact"]);
}

/// mpv parses these as plain numbers, so avoid scientific notation and the
/// locale-dependent formatting a naive `to_string` could produce.
fn fmt(v: f64) -> String {
    format!("{v:.4}")
}
