//! Automated exercises for paths a keyboard and mouse would normally drive.
//!
//! For states that are awkward to reach by hand and impossible from a test
//! that doesn't own a window:
//!
//! * `DBM_INPUT_TEST=1` — drives the action callbacks in sequence and prints
//!   the mirrored state after each, so the whole chain can be checked without a
//!   keyboard.
//! * `DBM_SCRUB_TEST=1` — drags the timeline from 10% to 90% faster than mpv
//!   can seek, then lets go. Reports whether playback paused for the drag and
//!   resumed after it; pair it with `DBM_TRACE=1` for how many updates the
//!   scrubber coalesced into how many seeks. `=slow` drags it at ten pixels a
//!   second instead, stopping three times, and reports how far the picture
//!   is from the pointer at each rest, then once more after a fast drag.
//! * `DBM_REACH_TEST=1` — opens the playlist with P and walks twelve rows down
//!   with the arrow keys, reporting the ring's row and the list's scroll at
//!   each. `=glass` walks into the glass page and steps a slider with Left
//!   instead; `=up` opens the playlist without a key, so the first Up has to
//!   both show the ring and land on the bottom row; `=accessibility` walks to
//!   the accessibility page and presses Enter on its switch twice.
//! * `DBM_RESIZE_TEST=1` — sweeps the window width like a drag. It does *not*
//!   enter Windows' modal resize loop, so it can pass while a real drag
//!   freezes; that is `DBM_MODAL_TEST`.
//! * `DBM_MODAL_TEST=1` — posts `WM_SYSCOMMAND`/`SC_SIZE` to enter that modal
//!   loop for real.
//! * `DBM_TOGGLE_TEST=1` — flips each effect switch, autoplay and the
//!   animations switch in turn, then drives a slider and resets its section.
//!   Pair it with `DBM_PROBE=1`, which takes a fresh pixel reading each time
//!   an effect switch moves. Point `APPDATA` at a scratch directory when
//!   running it: a reset is saved like any other change.
//! * `DBM_PARAM_TEST=<0..1>` — opens the settings and sets the glass tint to
//!   that value through the same callback a slider drag uses. Anything that is
//!   not a number opens the panel and touches nothing, which is how a value
//!   loaded from disk is looked at.
//! * `DBM_OPEN_TEST=<path>` — opens that path the way the dialog does, and
//!   reports the playlist it produced.
//! * `DBM_DIALOG_TEST=1` — puts a real file dialog on screen. Pair it with
//!   `DBM_TRACE=1` to see that draws carry on while it is open, which is why
//!   the dialog doesn't run on this thread.
//! * `DBM_SHOWCASE=tip` and `tip-edge` rest the pointer on a button in the
//!   right-hand pill instead of in the corner, so the hover label shows.
//!   `tip-volume` rests it on the volume track, whose label is the level.
//! * `DBM_SHOWCASE=window-max` presses the window's own maximize button.
//! * `DBM_SHOWCASE=muted` mutes, for the volume track with its fill gone.
//! * `DBM_SHOWCASE=find-offer`, `sign-in` and `account`, with
//!   `DBM_FIND_FAKE=spent`: the search's page offering to sign in once a
//!   download is refused, the sign-in with a name and a password typed, and
//!   the account's page once signed in.
//! * `DBM_SHOWCASE=scroll-bar` opens the playlist and rests the pointer on its
//!   scroll bar, to draw the bar at its grown width.
//! * `DBM_SHOWCASE=current-above`, `current-below` and `current-half` open the
//!   playlist and turn the wheel until the file playing is past the top or
//!   the bottom, or its lower edge is halfway into the top's soft edge, so
//!   the angle pointing back to it is drawn. Open a file from the middle of a
//!   season.
//!   `current-back` turns it away and then presses the angle, which should
//!   carry that row back to the middle and play nothing; it prints where the
//!   list is every 20ms of the glide.
//!   `current-hover` rests the pointer on the angle, for its circle.
//! * `DBM_SHOWCASE=arrival` opens a file with a place saved in it, which is
//!   held there with the choice of going on or starting over; `arrival-step`
//!   gets there by the Next button instead, and `arrival-answer` then answers
//!   from the beginning. Each prints what it sees. Seed the watch-later
//!   entries in a scratch `APPDATA` first.
//! * `DBM_SHOWCASE=autoplay` sends the file to its last seconds and lets
//!   autoplay carry on into the next, which should start from the top with
//!   its saved place offered in the corner for ten seconds; `autoplay-take`
//!   takes the offer. Each prints what it sees. Seed the next file's
//!   watch-later entry in a scratch `APPDATA` first.
//! * `DBM_SHOWCASE=language-walk` opens the language page and walks the ring
//!   down to its last row, which on a small window (`DBM_WINDOW=960x540`) is
//!   past the bottom of what the panel shows, so the list has to follow it.
//! * `DBM_SHOWCASE=drill` — flips the playlist between its list of parts and a
//!   part's page on every tick, so a capture lands inside the 220ms move rather
//!   than at either end. Like `flash`, a movement can't be held, so it is
//!   restarted instead.
//! * `DBM_SHOWCASE=ring-bar` walks the ring with Tab to the settings button,
//!   for the disc of glass under it; `ring-row` and `ring-slider` put it on a
//!   playlist row and on a glass slider.
//! * `DBM_SHOWCASE=armed` opens the subtitles menu and presses Delete once on
//!   the first subtitle loaded from its own file, so the row asks for the
//!   second press. Open a film with a downloaded subtitle.
//! * `DBM_SHOWCASE=sideways` presses Left and Right in the settings and the
//!   playlist and prints the page each left behind and the film's position,
//!   which neither should move. Open a file from the middle of a season.
//! * `DBM_SHOWCASE=<surface>` — holds one surface open with the chrome awake,
//!   so the window can be captured. Everything else here reports numbers.
//! * `DBM_WINDOW=<w>x<h>` — opens at that size instead of 1280×720, with any
//!   of the others. The widths worth looking at are 1060, where the transport
//!   starts giving way to the right pill, and the 960×540 floor. Pair it with
//!   `DBM_CAPTURE`.
//! * `DBM_FADE_TEST=1` — samples how the bar fades out when the chrome goes
//!   idle. Nothing asks for frames once the interface is gone, so the question
//!   is whether the animation still gets any.
//! * `DBM_PAUSED_RESIZE_TEST=1` — pauses, then resizes, then leaves it alone.
//!   Pair with `DBM_TRACE=1` and `DBM_PROBE=1`: nothing redraws a paused frame,
//!   so anything drawn from state that arrives late stays wrong.
//! * `DBM_PREVIEW_TEST=1` — hovers along the timeline and reports what the seek
//!   preview would show at each point.
//! * `DBM_SCROLL_TEST=1` — checks how tall the tracks and playlist panels
//!   come out against their lists, that a wheel over one actually moves it,
//!   and that its scroll bar can be pressed and dragged.
//! * `DBM_WHEEL_TEST=1` — turns the wheel over the picture, which should do
//!   nothing, and then over the volume track, which should move the level and
//!   say it in the hover label without the wait a name gets.
//! * `DBM_TIMING_TEST=1` — sets a subtitle delay and speed on the file that
//!   opened, steps to the next and back, and reports both at each: the next
//!   should start with neither and the first should get its own back.
//! * `DBM_KEY_TEST=1` — dispatches real key events into the window, for
//!   shortcuts whose effect is otherwise off-screen.
//! * `DBM_READ_TEST=1` — opens the shortcuts page and presses the keys that
//!   read it, with a film playing. They should scroll the page, not reach the
//!   film, so `time-pos` either side is the check; the scroll itself is inside
//!   a conditional and has nothing to read.
//! * `DBM_SCAN_TEST=1` — prints every playlist row's name and length as the
//!   background scan fills them in. Point `APPDATA` at an empty directory to
//!   watch it do a first scan, and run it again to see a second one come from
//!   the cache all at once.
//! * `DBM_CURSOR_TEST=1` — waits out the idle clock with a film playing and
//!   asks Windows whether a cursor is on screen, then moves the pointer and
//!   asks again, since the interface's own `pointer-hidden` doesn't prove the
//!   pointer went.
//! * `DBM_PANEL_TEST=1` — opens each panel in turn and counts the glass rects
//!   the UI publishes. All three share a corner, so two open at once would
//!   stack.
//! * `DBM_SUBS_TEST=1` — steps subtitle timing, size and position, reads each
//!   back out of mpv, checks the clamp and the reset. Point `APPDATA` at a
//!   scratch directory: size and position are saved preferences.
//! * `DBM_SYNC_TEST=<sid or file>` — picks that subtitle track, or loads that
//!   subtitle file, walks the keyboard to the sync row and presses it, waits
//!   out the listening, and reports the delay and speed it settled on with
//!   the line mpv shows at `DBM_SYNC_AT` seconds (19 if unset). Then takes
//!   the sync back and reports again, unless `DBM_SYNC_KEEP` is set.
//! * `DBM_FIND_TEST=1` — opens the subtitles menu and its search,
//!   takes the first result with the keyboard, removes it again with Delete,
//!   and reports the menu and the line on screen at each step. Pair it with
//!   `DBM_FIND_FAKE=1`, which answers the search from a fixed list and writes
//!   the download on the spot, so it needs no key and no connection.
//! * `DBM_SIGN_IN_TEST=1` — with `DBM_FIND_FAKE=spent`, has a download refused
//!   for want of an account, signs in from the keyboard (a wrong password,
//!   then the right one), sees the download go through, and signs out. On the
//!   way it checks a copy from the password field takes nothing out of it.
//! * `DBM_AUDIO_TEST=1` — stages the Bluetooth failure: drops the audio track
//!   the way mpv does when a device dies, then feeds the watchdog a device
//!   appearing, and reads `aid` back to see whether anything came of it. Then
//!   the case that must *not* recover, audio the user turned off. It also
//!   prints every real device list, to confirm the payload parses on a given
//!   mpv build. Needs a file with an audio track.
//! * `DBM_FLASH_TEST=1` — presses the keys that raise the action flash and
//!   reports what the interface published for each: that it was there, in the
//!   right place, and gone again afterwards.
//! * `DBM_PROGRESS_TEST=1` — plays, asks mpv to write its resume file, then
//!   looks for that file where the playlist rows look for it, since a wrong
//!   guess about where it is fails silently.
//! * `DBM_DROP_TEST=<path>` — posts that path at the window as a file drop,
//!   exactly as the shell would, and reports what the player made of it.
//!   Windows only.
//! * `DBM_END_TEST=1` — turns autoplay off and seeks to the last moment of the
//!   file, so the end-of-playback button appears for real, then holds it there
//!   to be captured. `DBM_END_TEST=enter` presses Enter instead, which the
//!   pill's ring must answer. Point `APPDATA` at a scratch directory, since
//!   autoplay is a saved preference.
//! * `DBM_CREDITS_TEST=1` — gives the file a credits chapter over its last two
//!   minutes and plays into it, so the credits pill comes up for real. It
//!   reports as the bar arrives and again after it has gone, which the pill
//!   must outlast, then presses Enter, which the pill's ring must answer, and
//!   reports the file that follows. Needs an episode, and a later file beside
//!   it. `DBM_CREDITS_TEST=own` keeps the file's own chapters and plays into
//!   the last of them. `=heard` gives it nothing and plays into its last
//!   twenty seconds, for a file with no chapters whose credits have to be
//!   found by listening; point `APPDATA` at a scratch directory to watch it
//!   listen, since what it hears is kept.
//! * `DBM_INTRO_TEST=1` — waits for the episode's title sequence to be found,
//!   by its chapters or by listening, and seeks into it so the pill offers to
//!   skip it. Reports as it arrives and after the bar has gone, then presses
//!   Enter and reports where that landed, and what the pill says, which must
//!   not change while it fades out. Needs an episode with another of its season
//!   beside it; point `APPDATA` at a scratch directory to watch it listen,
//!   since what it hears is kept. `=hold` pauses inside the sequence instead
//!   of pressing anything, to be captured.
//! * `DBM_SMTC_TEST=1` — presses the real media keys and reads the OS media
//!   session back, checking both directions against Windows rather than our
//!   own copy of what it was told. Windows only.
//! * `DBM_MPRIS_TEST=1` — the same exercise on the Linux side, as a client on
//!   the session bus: it presses each transport method on the player's MPRIS
//!   object and reads the properties back off the bus, as a desktop widget
//!   would see them. It also names every other player registered, since a
//!   media key goes to one of them. Not Windows.
//!
//! Each returns a `Timer` that must be kept alive for the duration of the
//! run; dropping it stops the exercise.
//!
//! On a desktop, run them through `scripts/headless.sh`: a harness window takes
//! focus as it maps, and on Wayland nothing it asks can stop that, so the
//! script gives the run a headless compositor of its own. Every run plays its
//! film silently (see [`configure`]).

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

use crate::playback::mpv::Mpv;
use crate::{MainWindow, TrackItem};

/// Whether any exercise is armed: a `DBM_*_TEST` switch of any kind.
///
/// Any at all, rather than a list of the ones that seem to matter, which goes
/// wrong: `DBM_KEY_TEST` presses B, and B is the ambience switch.
pub fn armed() -> bool {
    std::env::vars_os().any(|(k, _)| {
        let k = k.to_string_lossy();
        k.starts_with("DBM_") && k.ends_with("_TEST")
    })
}

/// Whether the player is being driven or photographed rather than watched:
/// an exercise, a showcase or a capture. Each of them plays a film, and
/// nobody is listening to it.
pub fn unattended() -> bool {
    armed()
        || std::env::var_os("DBM_SHOWCASE").is_some()
        || std::env::var_os("DBM_CAPTURE").is_some()
}

/// Options for a run nobody is watching. Before `mpv_initialize`, like the
/// rest of mpv's configuration.
///
/// The film plays silently, through a zero-gain filter at the end of the
/// audio chain rather than by muting or turning it down, which several
/// exercises press and read back. The levels, the selected track and the
/// device list all stay as they would be.
pub fn configure(mpv: &Mpv) {
    if !unattended() {
        return;
    }
    match mpv.set_option("af", "lavfi=[volume=0]") {
        Ok(()) => eprintln!("dbm: unattended - playing silently"),
        Err(e) => eprintln!("dbm: unattended, but could not silence the audio: {e}"),
    }
}

/// Handles for everything armed here. Kept in one value so `main` does not
/// have to name each timer just to hold it open.
pub struct Harnesses {
    _timers: Vec<slint::Timer>,
}

pub fn install(ui: &MainWindow, app: &crate::app::App) -> Harnesses {
    let (mpv, audio) = (&app.mpv, &app.audio);
    let mut timers = Vec::new();
    // First, so what follows starts at the size asked about. It waits for a
    // frame, because asked for before the window exists, the height took and
    // the width didn't.
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
    if std::env::var_os("DBM_WHEEL_TEST").is_some() {
        timers.push(playback::wheel_test(ui));
    }
    if std::env::var_os("DBM_TIMING_TEST").is_some() {
        timers.push(playback::timing_test(ui, mpv.clone()));
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
    if std::env::var_os("DBM_SYNC_TEST").is_some() {
        timers.push(settings::sync_test(ui, mpv.clone()));
    }
    if std::env::var_os("DBM_SIGN_IN_TEST").is_some() {
        timers.push(settings::sign_in_test(ui));
    }
    if std::env::var_os("DBM_FIND_TEST").is_some() {
        timers.push(settings::find_test(ui, mpv.clone()));
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
    match std::env::var("DBM_SCRUB_TEST").as_deref() {
        Ok("slow") => timers.push(playback::slow_scrub_test(ui)),
        Ok(_) => timers.push(playback::scrub_test(ui)),
        Err(_) => {}
    }
    if std::env::var_os("DBM_INTRO_TEST").is_some() {
        timers.push(playback::intro_test(ui));
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
