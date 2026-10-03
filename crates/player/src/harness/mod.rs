//! Automated exercises for paths a keyboard and mouse would normally drive.
//!
//! These exist because the interesting failures were all in states that are
//! awkward to reach by hand and impossible to reach from a test that does not
//! own a window:
//!
//! * `DBM_INPUT_TEST=1` — drives the action callbacks in sequence and prints
//!   the mirrored state after each, so the whole chain can be checked without a
//!   keyboard.
//! * `DBM_SCRUB_TEST=1` — drags the timeline from 10% to 90% faster than mpv
//!   can seek, then lets go. Reports whether playback paused for the drag and
//!   resumed after it; pair it with `DBM_TRACE=1` for how many updates the
//!   scrubber coalesced into how many seeks.
//! * `DBM_REACH_TEST=1` — opens the playlist with P and walks twelve rows down
//!   with the arrow keys, reporting the ring's row and the list's scroll at
//!   each. `=glass` walks into the glass page and steps a slider with Left
//!   instead; `=up` opens the playlist without a key, so the first Up has to
//!   both show the ring and land on the bottom row.
//! * `DBM_RESIZE_TEST=1` — sweeps the window width like a drag. Note this does
//!   *not* enter Windows' modal resize loop, which is why it once reported
//!   healthy while the real thing froze.
//! * `DBM_MODAL_TEST=1` — posts `WM_SYSCOMMAND`/`SC_SIZE` to enter that modal
//!   loop for real.
//! * `DBM_TOGGLE_TEST=1` — flips each effect switch and autoplay in turn, then
//!   drives a slider and resets its section. Pair it with `DBM_PROBE=1`, which
//!   takes a fresh pixel reading each time an effect switch moves. Point
//!   `APPDATA` at a scratch directory when running it: a reset is saved like
//!   any other change.
//! * `DBM_PARAM_TEST=<0..1>` — opens the settings and sets the glass tint to
//!   that value through the same callback a slider drag uses. Anything that is
//!   not a number opens the panel and touches nothing, which is how a value
//!   loaded from disk is looked at.
//! * `DBM_OPEN_TEST=<path>` — opens that path the way the dialog does, and
//!   reports the playlist it produced.
//! * `DBM_DIALOG_TEST=1` — puts a real file dialog on screen. Pair it with
//!   `DBM_TRACE=1`: the point is that draws carry on while it is open, which is
//!   the whole reason the dialog does not run on this thread.
//! * `DBM_SHOWCASE=tip` and `tip-edge` rest the pointer on a button in the
//!   right-hand pill instead of in the corner, which is the only way the hover
//!   label is ever on screen.
//! * `DBM_SHOWCASE=drill` — flips the playlist between its list of parts and a
//!   part's page on every tick, so a capture lands inside the 220ms move rather
//!   than on whichever end it settled at. The same trick as `flash`: a movement
//!   cannot be held still, so it is started again instead.
//! * `DBM_SHOWCASE=<surface>` — holds one surface open with the chrome awake,
//!   so the window can be photographed. The only way to actually look at this
//!   interface: everything else here reports numbers.
//! * `DBM_WINDOW=<w>x<h>` — opens at that size instead of 1280×720. Not a test
//!   of its own: it goes with any of the others, and it exists because the
//!   layout has two widths worth looking at that the default is neither of —
//!   1060, where the transport gives way leftwards rather than crowd the right
//!   pill, and the 900×480 floor. Pair it with `DBM_CAPTURE`.
//! * `DBM_FADE_TEST=1` — samples how the bar fades out when the chrome goes
//!   idle. Nothing asks for frames once the interface is gone, so the question
//!   is whether the animation still gets any.
//! * `DBM_PAUSED_RESIZE_TEST=1` — pauses, then resizes, then leaves it alone.
//!   Pair with `DBM_TRACE=1` and `DBM_PROBE=1`: nothing redraws a paused frame,
//!   so anything drawn from state that arrives late stays wrong.
//! * `DBM_PREVIEW_TEST=1` — hovers along the timeline and reports what the seek
//!   preview would show at each point.
//! * `DBM_SCROLL_TEST=1` — checks how the tracks and playlist panels divide
//!   their height between lists, and that a wheel over one actually moves it.
//! * `DBM_KEY_TEST=1` — dispatches real key events into the window, for
//!   shortcuts whose effect is otherwise off-screen.
//! * `DBM_READ_TEST=1` — opens the shortcuts page and presses the keys that
//!   read it, with a film playing. The page is the only list here with no row
//!   the ring can stand on, so its keys scroll rather than walk — and before
//!   they did, every one of them fell through to the film: Home restarted it,
//!   PgUp and PgDn jumped its chapters, with the page that documents those keys
//!   open on top of them. `time-pos` either side is the whole of the check; the
//!   scroll itself is inside a conditional and has nothing to read.
//! * `DBM_SCAN_TEST=1` — prints every playlist row's name and length as the
//!   background scan fills them in. Point `APPDATA` at an empty directory to
//!   watch it do a first scan, and run it again to see a second one come from
//!   the cache all at once.
//! * `DBM_CURSOR_TEST=1` — waits out the idle clock with a film playing and
//!   asks Windows whether a cursor is on screen, then moves the pointer and
//!   asks again. The interface's own `pointer-hidden` is not evidence: the
//!   binding this replaced set that property correctly for months and the
//!   pointer never went anywhere.
//! * `DBM_PANEL_TEST=1` — opens each panel in turn and counts the glass rects
//!   the UI publishes. All three share a corner, so two open at once would
//!   stack.
//! * `DBM_SUBS_TEST=1` — steps subtitle timing, size and position, reads each
//!   back out of mpv, checks the clamp and the reset. Point `APPDATA` at a
//!   scratch directory: size and position are saved preferences.
//! * `DBM_AUDIO_TEST=1` — stages the Bluetooth failure: drops the audio track
//!   the way mpv does when a device dies, then feeds the watchdog a device
//!   appearing, and reads `aid` back to see whether anything came of it. Then
//!   the case that must *not* recover — audio the user turned off — because a
//!   watchdog that overrides a choice is worse than one that sleeps. It also
//!   prints every real device list, which is the only way to confirm the
//!   payload parses on a given mpv build. Needs a file with an audio track, or
//!   every branch declines for want of one.
//! * `DBM_FLASH_TEST=1` — presses the keys that raise the action flash and
//!   reports what the interface published for each: a flash only exists for
//!   about a second, so the one thing worth checking is that it was there, in
//!   the right place, and gone again afterwards.
//! * `DBM_PROGRESS_TEST=1` — plays, asks mpv to write its resume file, then
//!   looks for that file where the playlist rows look for it. The length and
//!   progress on each row come from two files on disk, and a wrong guess about
//!   either is silent.
//! * `DBM_DROP_TEST=<path>` — posts that path at the window as a file drop,
//!   exactly as the shell would, and reports what the player made of it.
//!   Windows only.
//! * `DBM_END_TEST=1` — turns autoplay off and seeks to the last moment of the
//!   file, so the end-of-playback button appears for real, then holds it there
//!   to be photographed. `DBM_END_TEST=enter` presses Enter instead, which the
//!   ring the pill took must answer. Point `APPDATA` at a scratch directory:
//!   autoplay is a saved preference.
//! * `DBM_CREDITS_TEST=1` — gives the file a credits chapter over its last two
//!   minutes and plays into it, so the credits pill comes up for real. It
//!   reports as the bar arrives and again after it has gone, which the pill
//!   must not, then presses Enter - which the pill's ring must answer - and
//!   reports the file that follows. Needs an episode, and a later file beside
//!   it. `DBM_CREDITS_TEST=own` keeps the file's own chapters and plays into
//!   the last of them.
//! * `DBM_SMTC_TEST=1` — presses the real media keys and reads the OS media
//!   session back, so both directions of the overlay are checked against
//!   Windows rather than against our own copy of what it was told. Windows
//!   only.
//! * `DBM_MPRIS_TEST=1` — the same exercise on the Linux side, as a client on
//!   the session bus: it presses each transport method on the player's own
//!   MPRIS object and reads the properties back off the bus, which is the only
//!   way to see what a desktop widget would see rather than what we believe we
//!   published. It also names every other player registered, because a media
//!   key goes to one of them. Not Windows.
//!
//! Each returns a `Timer` that must be kept alive for the duration of the
//! run; dropping it stops the exercise.
//!
//! On a desktop, run them through `scripts/headless.sh`. A window the harness
//! opens takes focus as it maps and sits under whatever the hand was about to
//! click, and on Wayland nothing the window asks for can stop that; the script
//! gives the run a headless compositor of its own, which nobody can see.

mod drive;
mod keys;
mod layout;
mod media_keys;
mod opening;
mod platform;
mod playback;
mod settings;
mod showcase;

use slint::{Model, ModelRc};

use crate::{MainWindow, TrackItem};

/// Handles for everything armed here. Kept in one value so `main` does not
/// have to name each timer just to hold it open.
pub struct Harnesses {
    _timers: Vec<slint::Timer>,
}

pub fn install(ui: &MainWindow, app: &crate::app::App) -> Harnesses {
    let (mpv, audio) = (&app.mpv, &app.audio);
    let mut timers = Vec::new();
    // First, so a showcase or a test that follows is already at the size
    // being asked about rather than resizing under itself. It still has to
    // wait for a frame: asked for here, before the window exists, the height
    // took and the width did not.
    if let Some(size) = std::env::var("DBM_WINDOW").ok().and_then(|s| {
        let (w, h) = s.split_once(['x', 'X'])?;
        Some(slint::LogicalSize::new(
            w.trim().parse().ok()?,
            h.trim().parse().ok()?,
        ))
    }) {
        timers.push(layout::window_size(ui, size));
    }
    if std::env::var_os("DBM_INPUT_TEST").is_some() {
        timers.push(playback::input_test(ui));
    }
    if std::env::var_os("DBM_RESIZE_TEST").is_some() {
        timers.push(layout::resize_test(ui));
    }
    if std::env::var_os("DBM_MODAL_TEST").is_some() {
        timers.push(layout::modal_test(ui));
    }
    if std::env::var_os("DBM_PARAM_TEST").is_some() {
        timers.push(settings::param_test(ui));
    }
    if std::env::var_os("DBM_TOGGLE_TEST").is_some() {
        timers.push(settings::toggle_test(ui, mpv.clone()));
    }
    if let Some(path) = std::env::var_os("DBM_OPEN_TEST") {
        timers.push(opening::open_test(ui, path.to_string_lossy().into_owned()));
    }
    if std::env::var_os("DBM_DIALOG_TEST").is_some() {
        timers.push(opening::dialog_test(ui));
    }
    if let Some(surface) = std::env::var_os("DBM_SHOWCASE") {
        timers.push(showcase::showcase(
            ui,
            surface.to_string_lossy().into_owned(),
        ));
    }
    if std::env::var_os("DBM_FADE_TEST").is_some() {
        timers.push(playback::fade_test(ui));
    }
    if std::env::var_os("DBM_PAUSED_RESIZE_TEST").is_some() {
        timers.push(layout::paused_resize_test(ui));
    }
    if std::env::var_os("DBM_PREVIEW_TEST").is_some() {
        timers.push(playback::preview_test(ui));
    }
    if std::env::var_os("DBM_SCROLL_TEST").is_some() {
        timers.push(layout::scroll_test(ui));
    }
    if std::env::var_os("DBM_KEY_TEST").is_some() {
        timers.push(keys::key_test(ui));
    }
    if std::env::var_os("DBM_READ_TEST").is_some() {
        timers.push(keys::read_test(ui, mpv.clone()));
    }
    if std::env::var_os("DBM_REACH_TEST").is_some() {
        timers.push(keys::reach_test(ui));
    }
    if std::env::var_os("DBM_CURSOR_TEST").is_some() {
        timers.push(platform::cursor_test(ui));
    }
    if std::env::var_os("DBM_PANEL_TEST").is_some() {
        timers.push(layout::panel_test(ui));
    }
    if std::env::var_os("DBM_SUBS_TEST").is_some() {
        timers.push(settings::subs_test(ui, mpv.clone()));
    }
    if std::env::var_os("DBM_AUDIO_TEST").is_some() {
        timers.push(platform::audio_test(mpv.clone(), audio.clone()));
    }
    #[cfg(windows)]
    if std::env::var_os("DBM_SMTC_TEST").is_some() {
        timers.push(media_keys::smtc_test(ui, mpv.clone()));
    }
    #[cfg(not(windows))]
    if std::env::var_os("DBM_MPRIS_TEST").is_some() {
        timers.push(media_keys::mpris_test(ui, mpv.clone()));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("DBM_DROP_TEST") {
        timers.push(opening::drop_test(ui, path.to_string_lossy().into_owned()));
    }
    if std::env::var_os("DBM_FLASH_TEST").is_some() {
        timers.push(playback::flash_test(ui));
    }
    if std::env::var_os("DBM_PROGRESS_TEST").is_some() {
        timers.push(playback::progress_test(ui, mpv.clone()));
    }
    if std::env::var_os("DBM_SCAN_TEST").is_some() {
        timers.push(opening::scan_test(ui));
    }
    if std::env::var_os("DBM_END_TEST").is_some() {
        timers.push(playback::end_test(ui));
    }
    if std::env::var_os("DBM_SCRUB_TEST").is_some() {
        timers.push(playback::scrub_test(ui));
    }
    if std::env::var_os("DBM_CREDITS_TEST").is_some() {
        timers.push(playback::credits_test(ui, mpv.clone()));
    }
    Harnesses { _timers: timers }
}

// ---------------------------------------------------------------------------
// Reading the interface back, for the exercises' reports
// ---------------------------------------------------------------------------

/// The label of the selected row in a track list, or `none`.
fn selected_label(model: &ModelRc<TrackItem>) -> String {
    model
        .iter()
        .find(|t| t.selected)
        .map(|t| t.label.to_string())
        .unwrap_or_else(|| "none".into())
}

/// Every file of the list mpv is playing, flattened back out of the parts the
/// panel shows it in. Files in seasons beside the list are left out: they are
/// shown, not queued.
fn playlist_rows(ui: &MainWindow) -> Vec<crate::PlaylistItem> {
    ui.get_playlist_groups()
        .iter()
        .flat_map(|group| group.entries.iter().collect::<Vec<_>>())
        .filter(|entry| entry.index >= 0)
        .collect()
}

/// Enough of a label to tell rows apart in one line of output.
fn short(label: &slint::SharedString) -> String {
    label.chars().take(24).collect()
}
