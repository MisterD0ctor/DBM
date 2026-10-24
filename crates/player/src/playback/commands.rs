//! Everything the UI can ask the player to do.
//!
//! Stateless: each action is a command mpv resolves against its own state
//! (`cycle pause`, `add volume 2`, `seek 50 absolute-percent`) rather than a
//! read, a computation and a write, so mpv stays the single source of truth
//! and a toggle can't disagree with our mirror of it.
//!
//! Failures are logged rather than propagated, since a rejected seek
//! shouldn't take the player down.

use crate::playback::mpv::Mpv;

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

/// The speeds `[` and `]` step through.
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

/// Play at a given speed, for a caller that arrived at a number of its own
/// rather than stepping the ladder.
// Only MPRIS asks for this; SMTC has no rate or position to set.
#[cfg_attr(windows, allow(dead_code))]
pub fn set_speed(mpv: &Mpv, speed: f64) {
    set_prop(mpv, "speed", &fmt(speed));
}

pub fn reset_speed(mpv: &Mpv) {
    set_prop(mpv, "speed", "1");
}

/// One frame. Forward pauses on arrival, as mpv's own does; back always
/// seeks, so it is slower on a long GOP.
pub fn frame_step(mpv: &Mpv, direction: i32) {
    run(
        mpv,
        &[if direction >= 0 {
            "frame-step"
        } else {
            "frame-back-step"
        }],
    );
}

/// The previous or next chapter. mpv's own semantics: back from a little way
/// into a chapter returns to its start before it goes to the one before.
pub fn chapter_step(mpv: &Mpv, direction: i32) {
    run(
        mpv,
        &["add", "chapter", if direction >= 0 { "1" } else { "-1" }],
    );
}

/// The next subtitle or audio track, wrapping through "none".
///
/// Subtitles are made visible with it, or cycling onto a track hidden by an
/// earlier switch-off would seem to do nothing.
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
/// Nothing on an interactive path may wait on mpv's core: a synchronous
/// exact seek holds the UI thread for about 200ms.
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
/// this stays stateless like the rest.
pub fn toggle_panscan(mpv: &Mpv) {
    run(mpv, &["cycle-values", "panscan", "1.0", "0.0"]);
}

pub fn toggle_mute(mpv: &Mpv) {
    run(mpv, &["cycle", "mute"]);
}

/// Toggle caption visibility.
///
/// The one action that can't be a plain `cycle`: turning captions on must
/// first make sure a subtitle track is selected, since `sub-visibility`
/// renders nothing with `sid` unset, as when mpv picked no track or
/// watch-later restored `sid=no`. Turning them off leaves `sid` alone, so the
/// next press brings that track back.
///
/// Reads the track list, so this **blocks** and must run on the worker,
/// where the read is fresh in a way a mirrored copy isn't.
pub fn toggle_subtitles(mpv: &Mpv) {
    let tracks = crate::playback::tracks::read_tracks(mpv);
    let subs = crate::playback::tracks::of_kind(&tracks, crate::playback::tracks::TrackKind::Sub);
    if subs.is_empty() {
        return;
    }

    // Showing, not merely visible, since `sub-visibility` can be on with no
    // track selected, where turning it off would change nothing visible.
    let showing = mpv.get_bool("sub-visibility")
        && crate::playback::tracks::selected(&tracks, crate::playback::tracks::TrackKind::Sub)
            .is_some();
    if showing {
        set_prop(mpv, "sub-visibility", "no");
        return;
    }
    // Only trust the current selection if it names a track this file has.
    if crate::playback::tracks::selected(&tracks, crate::playback::tracks::TrackKind::Sub).is_none()
    {
        set_prop(mpv, "sid", &subs[0].id.to_string());
    }
    set_prop(mpv, "sub-visibility", "yes");
}

/// Load a subtitle file into the film playing, named as fetched from
/// OpenSubtitles and in `language`. Selected and shown when `show` is set;
/// otherwise only added to the list, as the film's own tracks are.
pub fn add_subtitle(mpv: &Mpv, file: &std::path::Path, language: &str, show: bool) {
    let file = file.to_string_lossy();
    let flag = if show { "select" } else { "auto" };
    run(mpv, &["sub-add", &file, flag, "OpenSubtitles", language]);
    if show {
        set_prop(mpv, "sub-visibility", "yes");
    }
}

/// Take a track that came from a file of its own back out of the film.
/// Returns the file it came from, so a caller can forget it too.
///
/// **Blocking**: reads the track list. Worker thread only.
pub fn remove_track(mpv: &Mpv, id: i64, subtitle: bool) -> Option<String> {
    let count = mpv.get_f64("track-list/count").unwrap_or(0.0) as i64;
    let kind = if subtitle { "sub" } else { "audio" };
    let file = (0..count.max(0)).find_map(|i| {
        let here = |name: &str| format!("track-list/{i}/{name}");
        let matches = mpv.get_f64(&here("id")).map(|n| n as i64) == Some(id)
            && mpv.get_property(&here("type")).as_deref() == Some(kind)
            && mpv.get_bool(&here("external"));
        matches.then(|| {
            mpv.get_property(&here("external-filename"))
                .unwrap_or_default()
        })
    })?;
    let command = if subtitle {
        "sub-remove"
    } else {
        "audio-remove"
    };
    run(mpv, &[command, &id.to_string()]);
    Some(file)
}

/// Select a subtitle track, or `None` to turn them off.
pub fn set_subtitle_track(mpv: &Mpv, id: Option<i64>) {
    match id {
        Some(id) => {
            set_prop(mpv, "sid", &id.to_string());
            set_prop(mpv, "sub-visibility", "yes");
        }
        // Leaves `sid` intact so the track stays the one a later toggle
        // brings back, matching the subtitles menu's own "Off" entry.
        None => set_prop(mpv, "sub-visibility", "no"),
    }
}

pub fn set_audio_track(mpv: &Mpv, id: Option<i64>) {
    let value = id.map(|i| i.to_string()).unwrap_or_else(|| "no".into());
    set_prop(mpv, "aid", &value);
}

/// Load a scanned playlist and begin at one of its entries.
///
/// The entry is chosen before the list loads, with mpv's `playlist-start`,
/// rather than selected after: `loadlist` starts entry 0 at once, and a file
/// mpv opens and leaves a moment later counts as stopped at the beginning, so
/// mpv would delete its watch-later entry. Completion still arrives as
/// [`REPLY_LOADLIST`], since other work waits on the list.
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
/// are slow. Use [`seek_scrub`] while a drag is in progress, which knows when
/// one is worth it, and this for where the user lets go.
pub fn seek_fraction(mpv: &Mpv, fraction: f32) {
    run(
        mpv,
        &["seek", &fmt(percent_of(fraction)), "absolute-percent+exact"],
    );
}

/// Seek for a drag in progress: to the keyframe before the target unless
/// `exact`, so mpv needn't decode up to it. The scrubber asks for exactness
/// only while the hand is slow enough to see it. Issued with a distinct reply
/// id, so the scrubber can tell its own completions apart.
pub fn seek_scrub(mpv: &Mpv, fraction: f32, exact: bool) {
    let how = if exact {
        "absolute-percent+exact"
    } else {
        "absolute-percent"
    };
    let args = ["seek", &fmt(percent_of(fraction)), how];
    if let Err(e) = mpv.command_async(REPLY_SCRUB, &args) {
        eprintln!("dbm: scrub seek failed: {e}");
    }
}

fn percent_of(fraction: f32) -> f64 {
    f64::from(fraction.clamp(0.0, 1.0)) * 100.0
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
/// The clamp is here, the one place that knows both the range and that mpv
/// is about to be told, and returning the value lets the caller save the
/// number that was sent.
///
/// Placement is the exception: only `subline` sends it, saving where the
/// person put the line and sending where the bar lets it sit, which differ
/// while the bar is up.
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

/// Timing is per file rather than a preference, since a delay that fixes one
/// set of subtitles is wrong for the next, so mpv's watch-later file keeps it
/// (see `session::configure`).
pub fn set_sub_delay(mpv: &Mpv, seconds: f64) {
    set_prop(mpv, "sub-delay", &fmt(seconds));
}

/// The speed subtitles run at against the film: 1, unless they were timed
/// for a transfer at another frame rate (see `subsync`). Per file, like the
/// delay it goes with.
///
/// Six places rather than `fmt`'s four. The fourth place of a speed is
/// seven hundredths of a second by the end of a two-hour film, which is the
/// error this exists to remove.
pub fn set_sub_speed(mpv: &Mpv, speed: f64) {
    set_prop(mpv, "sub-speed", &format!("{speed:.6}"));
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

/// Move through the playlist and play what is there, for the end-of-file
/// button, a headset's media keys, and anything else that means "go on"
/// rather than "select".
///
/// The unpause is queued behind the step rather than timed after it, since
/// mpv runs queued commands in order.
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

/// Seek to a point, counted in seconds from the start. Exact, like every
/// seek that is not a drag in progress.
pub fn seek_to(mpv: &Mpv, seconds: f64) {
    run(mpv, &["seek", &fmt(seconds), "absolute+exact"]);
}

pub fn restart(mpv: &Mpv) {
    run(mpv, &["seek", "0", "absolute+exact"]);
}

/// mpv parses these as plain numbers, so avoid scientific notation and the
/// locale-dependent formatting a naive `to_string` could produce.
fn fmt(v: f64) -> String {
    format!("{v:.4}")
}
