//! Automated exercises for paths a keyboard and mouse would normally drive.
//!
//! These exist because the interesting failures were all in states that are
//! awkward to reach by hand and impossible to reach from a test that does not
//! own a window:
//!
//! * `DBM_INPUT_TEST=1`  — drives the action callbacks in sequence and prints
//!                         the mirrored state after each, so the whole chain
//!                         can be checked without a keyboard.
//! * `DBM_RESIZE_TEST=1` — sweeps the window width like a drag. Note this
//!                         does *not* enter Windows' modal resize loop, which
//!                         is why it once reported healthy while the real
//!                         thing froze.
//! * `DBM_MODAL_TEST=1`  — posts `WM_SYSCOMMAND`/`SC_SIZE` to enter that
//!                         modal loop for real.
//! * `DBM_TOGGLE_TEST=1` — flips each effect switch and autoplay in turn,
//!                         then drives a slider and resets its section. Pair
//!                         it with `DBM_PROBE=1`, which takes a fresh pixel
//!                         reading each time an effect switch moves. Point
//!                         `APPDATA` at a scratch directory when running it:
//!                         a reset is saved like any other change.
//!
//! * `DBM_OPEN_TEST=<path>` — opens that path the way the dialog does, and
//!                         reports the playlist it produced.
//! * `DBM_DIALOG_TEST=1` — puts a real file dialog on screen. Pair it with
//!                         `DBM_TRACE=1`: the point is that draws carry on
//!                         while it is open, which is the whole reason the
//!                         dialog does not run on this thread.
//! * `DBM_SHOWCASE=tip` and `tip-edge` rest the pointer on a button in the
//!                         right-hand pill instead of in the corner, which
//!                         is the only way the hover label is ever on screen.
//! * `DBM_SHOWCASE=<surface>` — holds one surface open with the chrome
//!                         awake, so the window can be photographed. The
//!                         only way to actually look at this interface:
//!                         everything else here reports numbers.
//! * `DBM_FADE_TEST=1`   — samples how the bar fades out when the chrome
//!                         goes idle. Nothing asks for frames once the
//!                         interface is gone, so the question is whether the
//!                         animation still gets any.
//! * `DBM_PAUSED_RESIZE_TEST=1` — pauses, then resizes, then leaves it
//!                         alone. Pair with `DBM_TRACE=1` and `DBM_PROBE=1`:
//!                         nothing redraws a paused frame, so anything drawn
//!                         from state that arrives late stays wrong.
//! * `DBM_PREVIEW_TEST=1` — hovers along the timeline and reports what the
//!                         seek preview would show at each point.
//! * `DBM_SCROLL_TEST=1` — checks how the tracks and playlist panels divide
//!                         their height between lists, and that a wheel over
//!                         one actually moves it.
//! * `DBM_KEY_TEST=1`    — dispatches real key events into the window, for
//!                         shortcuts whose effect is otherwise off-screen.
//! * `DBM_CURSOR_TEST=1` — waits out the idle clock with a film playing and
//!                         asks Windows whether a cursor is on screen, then
//!                         moves the pointer and asks again. The interface's
//!                         own `pointer-hidden` is not evidence: the binding
//!                         this replaced set that property correctly for
//!                         months and the pointer never went anywhere.
//! * `DBM_PANEL_TEST=1`  — opens each panel in turn and counts the glass
//!                         rects the UI publishes. All three share a corner,
//!                         so two open at once would stack.
//! * `DBM_SUBS_TEST=1`   — steps subtitle timing, size and position, reads
//!                         each back out of mpv, checks the clamp and the
//!                         reset. Point `APPDATA` at a scratch directory:
//!                         size and position are saved preferences.
//! * `DBM_AUDIO_TEST=1` — stages the Bluetooth failure: drops the audio
//!                         track the way mpv does when a device dies, then
//!                         feeds the watchdog a device appearing, and reads
//!                         `aid` back to see whether anything came of it.
//!                         Then the case that must *not* recover — audio the
//!                         user turned off — because a watchdog that
//!                         overrides a choice is worse than one that sleeps.
//!                         It also prints every real device list, which is
//!                         the only way to confirm the payload parses on a
//!                         given mpv build. Needs a file with an audio
//!                         track, or every branch declines for want of one.
//!
//! * `DBM_FLASH_TEST=1`  — presses the keys that raise the action flash and
//!                         reports what the interface published for each: a
//!                         flash only exists for about a second, so the one
//!                         thing worth checking is that it was there, in the
//!                         right place, and gone again afterwards.
//! * `DBM_PROGRESS_TEST=1` — plays, asks mpv to write its resume file, then
//!                         looks for that file where the playlist rows look
//!                         for it. The length and progress on each row come
//!                         from two files on disk, and a wrong guess about
//!                         either is silent.
//! * `DBM_DROP_TEST=<path>` — posts that path at the window as a file drop,
//!                         exactly as the shell would, and reports what the
//!                         player made of it. Windows only.
//! * `DBM_END_TEST=1`   — turns autoplay off and seeks to the last moment of
//!                         the file, so the end-of-playback button appears
//!                         for real, then holds it there to be photographed.
//!                         Point `APPDATA` at a scratch directory: autoplay
//!                         is a saved preference.
//! * `DBM_SMTC_TEST=1`  — presses the real media keys and reads the OS media
//!                         session back, so both directions of the overlay
//!                         are checked against Windows rather than against
//!                         our own copy of what it was told. Windows only.
//!
//! Each returns a `Timer` that must be kept alive for the duration of the
//! run; dropping it stops the exercise.

use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc};

use crate::mpv::Mpv;
use crate::{modal_loop, MainWindow, TrackItem};

/// Handles for everything armed here. Kept in one value so `main` does not
/// have to name each timer just to hold it open.
pub struct Harnesses {
    _timers: Vec<slint::Timer>,
}

pub fn install(
    ui: &MainWindow,
    mpv: &std::sync::Arc<Mpv>,
    audio: &std::rc::Rc<crate::audio::Watchdog>,
) -> Harnesses {
    let mut timers = Vec::new();
    if std::env::var_os("DBM_INPUT_TEST").is_some() {
        timers.push(input_test(ui));
    }
    if std::env::var_os("DBM_RESIZE_TEST").is_some() {
        timers.push(resize_test(ui));
    }
    if std::env::var_os("DBM_MODAL_TEST").is_some() {
        timers.push(modal_test(ui));
    }
    if std::env::var_os("DBM_PARAM_TEST").is_some() {
        timers.push(param_test(ui));
    }
    if std::env::var_os("DBM_TOGGLE_TEST").is_some() {
        timers.push(toggle_test(ui, mpv.clone()));
    }
    if let Some(path) = std::env::var_os("DBM_OPEN_TEST") {
        timers.push(open_test(ui, path.to_string_lossy().into_owned()));
    }
    if std::env::var_os("DBM_DIALOG_TEST").is_some() {
        timers.push(dialog_test(ui));
    }
    if let Some(surface) = std::env::var_os("DBM_SHOWCASE") {
        timers.push(showcase(ui, surface.to_string_lossy().into_owned()));
    }
    if std::env::var_os("DBM_FADE_TEST").is_some() {
        timers.push(fade_test(ui));
    }
    if std::env::var_os("DBM_PAUSED_RESIZE_TEST").is_some() {
        timers.push(paused_resize_test(ui));
    }
    if std::env::var_os("DBM_PREVIEW_TEST").is_some() {
        timers.push(preview_test(ui));
    }
    if std::env::var_os("DBM_SCROLL_TEST").is_some() {
        timers.push(scroll_test(ui));
    }
    if std::env::var_os("DBM_KEY_TEST").is_some() {
        timers.push(key_test(ui));
    }
    if std::env::var_os("DBM_REACH_TEST").is_some() {
        timers.push(reach_test(ui));
    }
    if std::env::var_os("DBM_CURSOR_TEST").is_some() {
        timers.push(cursor_test(ui));
    }
    if std::env::var_os("DBM_PANEL_TEST").is_some() {
        timers.push(panel_test(ui));
    }
    if std::env::var_os("DBM_SUBS_TEST").is_some() {
        timers.push(subs_test(ui, mpv.clone()));
    }
    if std::env::var_os("DBM_AUDIO_TEST").is_some() {
        timers.push(audio_test(mpv.clone(), audio.clone()));
    }
    #[cfg(windows)]
    if std::env::var_os("DBM_SMTC_TEST").is_some() {
        timers.push(smtc_test(ui, mpv.clone()));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("DBM_DROP_TEST") {
        timers.push(drop_test(ui, path.to_string_lossy().into_owned()));
    }
    if std::env::var_os("DBM_FLASH_TEST").is_some() {
        timers.push(flash_test(ui));
    }
    if std::env::var_os("DBM_PROGRESS_TEST").is_some() {
        timers.push(progress_test(ui, mpv.clone()));
    }
    if std::env::var_os("DBM_END_TEST").is_some() {
        timers.push(end_test(ui));
    }
    if std::env::var_os("DBM_SCRUB_TEST").is_some() {
        timers.push(scrub_test(ui));
    }
    Harnesses { _timers: timers }
}

fn selected_label(model: &ModelRc<TrackItem>) -> String {
    model
        .iter()
        .find(|t| t.selected)
        .map(|t| t.label.to_string())
        .unwrap_or_else(|| "none".into())
}

fn report(ui: &MainWindow, label: &str) {
    eprintln!(
        "dbm: {label:<22} t={} paused={} vol={} muted={} subs={}/{} audio={}",
        ui.get_elapsed(),
        ui.get_paused(),
        ui.get_volume(),
        ui.get_muted(),
        ui.get_subs_visible(),
        selected_label(&ui.get_sub_tracks()),
        selected_label(&ui.get_audio_tracks()),
    );
}

fn input_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1500),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            // Alternating act / observe, so each report reflects a settled
            // state rather than one still in flight.
            match step {
                0 => report(&ui, "start"),
                1 => ui.invoke_toggle_pause(),
                2 => report(&ui, "after toggle-pause"),
                3 => ui.invoke_toggle_pause(),
                4 => report(&ui, "after toggle again"),
                5 => ui.invoke_seek_fraction(0.5),
                6 => report(&ui, "after seek to 50%"),
                7 => ui.invoke_nudge_volume(-15.0),
                8 => report(&ui, "after volume -15"),
                9 => ui.invoke_seek_relative(-3.0),
                10 => report(&ui, "after seek -3s"),
                11 => ui.invoke_toggle_mute(),
                12 => report(&ui, "after toggle-mute"),
                13 => ui.invoke_select_audio(2),
                14 => report(&ui, "after audio -> id 2"),
                15 => ui.invoke_select_subtitle(2),
                16 => report(&ui, "after sub -> id 2"),
                17 => ui.invoke_select_subtitle(-1),
                18 => report(&ui, "after subs off"),
                19 => ui.invoke_toggle_subtitles(),
                20 => report(&ui, "after captions toggle"),
                21 => ui.invoke_toggle_panscan(),
                22 => eprintln!("dbm: panscan now {}", ui.get_panscan()),
                23 => ui.invoke_toggle_panscan(),
                24 => eprintln!("dbm: panscan back to {}", ui.get_panscan()),
                // The mute from step 11 is still on — nothing since has
                // touched it, and the reports above say so at every step. Now
                // the bar is used. The level has to arrive *and* the mute has
                // to go: either alone is a volume nobody can hear.
                25 => report(&ui, "still muted, about to use the bar"),
                26 => ui.invoke_set_volume(120.0),
                27 => report(&ui, "after the bar was used while muted"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

fn resize_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut tick = 0u32;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(8),
        move || {
            // Stop after a while and hold the last size, so the settling
            // path gets exercised too — a drag that never ends would only
            // ever test the moving case.
            if tick > 400 {
                return;
            }
            let Some(ui) = weak.upgrade() else { return };
            tick += 1;
            // Pause partway in. Resizing while paused is the case that used
            // to show black: no new frame meant the reallocated target was
            // never re-rendered, and the border pass then grew a glow out of
            // the black it found.
            if tick == 60 {
                ui.invoke_toggle_pause();
                eprintln!("dbm: paused mid-resize-sweep");
            }
            // Triangle wave, so the size is genuinely moving every tick.
            //
            // Both dimensions, not just width. Widening makes the video
            // taller, so a video rect that lags by a frame lands inside the
            // picture and nothing looks wrong; growing the *height* makes the
            // letterbox taller, and a lagging rect then points above the top
            // of the picture — at nothing — which is what darkens the border.
            // Width and height in turn, in big steps.
            //
            // Width exercises the target allocation; height is what actually
            // moves the letterbox, since a 2.4:1 film in a window growing on
            // both axes keeps almost the same bars. And the steps are large
            // because how far the edge moves in *one frame* is the whole
            // question: a video rect one frame behind is wrong by however
            // much changed in that frame, so a gentle sweep is wrong by a
            // pixel and shows nothing. A dragged corner moves tens of pixels
            // a frame, and that is what this has to be.
            let phase = tick % 40;
            let (w, h) = if phase < 20 {
                let d = if phase < 10 { phase } else { 20 - phase };
                (900 + d * 45, 700)
            } else {
                let p = phase - 20;
                let d = if p < 10 { p } else { 20 - p };
                (1200, 560 + d * 45)
            };
            ui.window().set_size(slint::PhysicalSize::new(w, h));
        },
    );
    timer
}

/// Drag the timeline the way a hand does: many small position updates in
/// quick succession. A single seek says nothing about scrubbing; the whole
/// question is what happens when they arrive faster than mpv can service
/// them.
fn scrub_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut tick = 0u32;
    let mut started = std::time::Instant::now();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            // Let playback settle before dragging.
            if tick == 0 {
                started = std::time::Instant::now();
            }
            tick += 1;
            if tick < 120 {
                return;
            }
            let step = tick - 120;
            if step > 130 {
                return;
            }
            if step == 0 {
                started = std::time::Instant::now();
                eprintln!(
                    "dbm: --- scrub drag starting, paused={} ---",
                    ui.get_paused()
                );
            }
            if step == 60 {
                eprintln!("dbm: mid-drag paused={}", ui.get_paused());
            }
            // Sweep 10% -> 90%. Several updates per tick, because a real
            // drag delivers pointer moves faster than a 16ms timer and the
            // whole question is what happens when they outpace mpv.
            // Stop feeding drag updates once released, or the next tick
            // starts a fresh drag and pauses all over again.
            #[allow(unused_assignments)]
            let mut f = 0.0;
            if step < 120 {
                for sub in 0..4 {
                    f = 0.1 + 0.8 * ((step * 4 + sub) as f32 / 480.0);
                    ui.invoke_seek_scrub(f);
                }
            }
            if step == 120 {
                f = 0.9;
                // Release, so the coalescing report fires.
                ui.invoke_seek_fraction(f);
                eprintln!(
                    "dbm: --- scrub drag done in {:.0}ms ---",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            // A tick later, so the resume has round-tripped through mpv.
            if step == 122 {
                eprintln!("dbm: after release paused={}", ui.get_paused());
            }
        },
    );
    timer
}

/// Open the settings panel and drive one parameter to its maximum, so the
/// probe frame lands with a value nothing else would have produced.
fn param_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(600),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.set_settings_open(true);
            // Index 7 is the glass tint amount. The value comes from the
            // variable so the same run can be repeated at both extremes.
            // A non-numeric value opens the panel without touching
            // anything, which is how a loaded-from-disk value can be probed.
            match std::env::var("DBM_PARAM_TEST")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                Some(tint) => {
                    ui.invoke_set_param(7, tint);
                    eprintln!("dbm: settings opened, tint set to {tint}");
                }
                None => eprintln!("dbm: settings opened, values untouched"),
            }
        },
    );
    timer
}

/// Hold one surface open, with the chrome kept awake, for a photograph.
///
/// The interface hides itself after three seconds of stillness and half of it
/// only exists while a pointer is somewhere particular, so a screenshot of the
/// running player is otherwise a picture of a video with nothing on it.
fn showcase(ui: &MainWindow, surface: String) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut tick = 0u32;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(250),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            tick += 1;

            // Any pointer event resets the idle clock. For the preview the
            // pointer also has to be *on* the track, so that one parks there
            // and the rest stay clear of the controls.
            // A hover label needs the pointer to stop on a button and stay
            // there, which is the one thing the default corner hover cannot
            // do. The coordinates are the right-hand pill's buttons at the
            // window this harness opens; they are a photograph's aim, not
            // geometry anything depends on.
            let (x, y) = match surface.as_str() {
                "preview" => (
                    ui.get_scrub_x() + ui.get_scrub_w() * 0.62,
                    ui.get_timeline_y() + 14.0,
                ),
                // The settings gear, mid-pill.
                "tip" | "tip-idle" | "tip-open" => (1140.0, 672.0),
                // The last button there is, to see the clamp hold a tip
                // inside the margin rather than off the window.
                "tip-edge" => (1232.0, 672.0),
                _ => (40.0, 40.0),
            };
            // Everything else here keeps hovering so the chrome stays up.
            // This one stops, on purpose: a pointer resting on a button
            // raises no events, the idle clock runs out under a label that
            // is already up, and the bar it points at leaves without it.
            // Capture this one well past HIDE_AFTER.
            if !(surface == "tip-idle" && tick > 8) {
                hover(&ui, x, y);
            }

            // The flash is a moment, not a surface: the only way to hold one
            // still long enough to look at is to keep raising it.
            if surface == "flash" {
                ui.global::<crate::Flash>()
                    .invoke_show(ui.global::<crate::Flash>().get_delay(), 0.5);
            }

            if tick != 4 {
                return;
            }
            match surface.as_str() {
                "tracks" => ui.invoke_open_menu(true),
                // The same menu with a subtitle track chosen and then
                // turned off, which is where Off and the track mpv still
                // calls selected were both lit at once. Choosing first
                // matters: a file whose subtitles were never on has nothing
                // selected to contradict Off, so the state that was wrong is
                // not the state a file arrives in.
                // The positive case, for the same reason: a list that can
                // show two answers can also show none.
                "subs-on" => {
                    ui.invoke_open_menu(true);
                    if let Some(first) = ui.get_sub_tracks().row_data(0) {
                        ui.invoke_select_subtitle(first.id);
                    }
                }
                "subs-off" => {
                    ui.invoke_open_menu(true);
                    if let Some(first) = ui.get_sub_tracks().row_data(0) {
                        ui.invoke_select_subtitle(first.id);
                    }
                    ui.invoke_select_subtitle(-1);
                }
                "playlist" => ui.invoke_open_playlist(true),
                "files" => ui.invoke_open_files(true),
                // The three graphics failures all need a driver that does
                // not work, which is not something a test can arrange. The
                // wording is the part worth looking at anyway: it is the only
                // account of the failure that ever reaches anyone, since the
                // console it used to print to does not exist in a packaged
                // build.
                "fatal" => {
                    ui.set_fatal("The player cannot show video on this computer.".into());
                    ui.set_fatal_detail(
                        "The graphics driver would not build the player's shaders. \
                         compiling glass.frag: 0(213) : error C1503: undefined \
                         variable \"backdrop\""
                            .into(),
                    );
                }
                // Held rather than provoked. A real open clears this the
                // moment mpv reports a file, which on a local disk is too few
                // frames to photograph — and the case worth looking at is the
                // slow one, which needs a network share to reproduce.
                "opening" => {
                    ui.set_opening_name("Game of Thrones Season 7".into());
                    ui.set_opening(true);
                }
                // A panel open under the pointer that is resting on the
                // button which opened it. Tips stand down for any panel, so
                // the right photograph here is of nothing at all.
                "tip-open" => ui.invoke_open_settings_page(0),
                "settings" => ui.invoke_open_settings_page(0),
                "glass" => ui.invoke_open_settings_page(1),
                "ambience" => ui.invoke_open_settings_page(2),
                "subs" => ui.invoke_open_settings_page(3),
                "shortcuts" => ui.invoke_open_settings_page(4),
                _ => {}
            }
            eprintln!("dbm: showcase ready ({surface})");
        },
    );
    timer
}

/// Watch the bar fade out, in the value the pipeline is actually given.
///
/// Every surface publishes how far it has faded in, so a fade is readable
/// without a screenshot: a run of values between 1 and 0 is an animation, and
/// a jump straight to nothing is a cut. Worth checking because the interface
/// only asks for frames while it is on screen — if Slint did not drive its
/// own animation, the fade out is precisely what would vanish.
fn fade_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut seen: Vec<f32> = Vec::new();
    let mut settled = 0u32;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(bar) = ui.get_glass_rects().row_data(0) else {
                return;
            };
            let now = if bar.width > 0.0 { bar.opacity } else { 0.0 };
            if seen.last().is_none_or(|last| (last - now).abs() > 0.001) {
                seen.push(now);
            }
            if now > 0.0 {
                return;
            }
            settled += 1;
            if settled == 12 {
                let steps: Vec<String> =
                    seen.iter().map(|v| format!("{v:.2}")).collect();
                eprintln!(
                    "dbm: bar faded through {} value(s): {}",
                    seen.len(),
                    steps.join(" ")
                );
                eprintln!("dbm: --- fade test done ---");
            }
        },
    );
    timer
}

/// Pause, resize, and then stop touching it.
///
/// The interesting moment is the one *after* the resize. mpv reports the new
/// video rect asynchronously, so it lands a frame or two late; while playing
/// the next frame redraws the border from it and nobody is the wiser. Paused
/// there is no next frame, so whatever redrew the border last has to have
/// been triggered by the rect itself.
fn paused_resize_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => {
                    ui.invoke_toggle_pause();
                    eprintln!("dbm: paused");
                }
                // Twice the height, so the letterbox bars are a different
                // size afterwards and a stale rect cannot pass for a fresh
                // one.
                2 => {
                    eprintln!("dbm: --- resizing while paused ---");
                    ui.window()
                        .set_size(slint::LogicalSize::new(1000.0, 900.0));
                }
                4 => eprintln!("dbm: --- settled; the border must match ---"),
                // Flipping the switch is what makes the probe take a fresh
                // pixel reading, which is how the letterbox gets looked at
                // with the new geometry rather than merely reasoned about.
                5 => {
                    ui.set_ambience_on(false);
                    ui.invoke_set_ambience(false);
                }
                6 => {
                    ui.set_ambience_on(true);
                    ui.invoke_set_ambience(true);
                }
                8 => eprintln!("dbm: --- paused resize test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Walk the pointer along the timeline and read the preview off the far end.
///
/// A real pointer move, not just the callback: the overlay only appears while
/// the track is hovered, and its rect is what tells the pipeline where to put
/// glass. So this checks both halves — that hovering raises it, and that the
/// timestamp and the tile under the pointer are the right ones.
fn preview_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1100),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let fractions = [0.0f32, 0.25, 0.5, 0.99];
            // Report first, then move: a value read in the same tick as the
            // move that produced it is the one the *next* label describes.
            if step == 0 {
                report_preview(&ui, "before hovering");
            } else if step <= fractions.len() {
                report_preview(&ui, &format!("at {:.0}%", fractions[step - 1] * 100.0));
            }
            if step < fractions.len() {
                let x = ui.get_scrub_x() + ui.get_scrub_w() * fractions[step];
                let y = ui.get_timeline_y() + 14.0;
                hover(&ui, x, y);
            } else if step == fractions.len() + 1 {
                eprintln!("dbm: --- preview test done ---");
            }
            step += 1;
        },
    );
    timer
}

/// Watch the pointer disappear and come back.
///
/// Timed around the three seconds the chrome waits, with the film left
/// playing: the run starts with a real move, so the clock starts from a known
/// point rather than from whenever the window last saw the hand.
///
/// Both halves matter and the second one more. A pointer that hides is only
/// half a feature — one that does not come back the moment the hand moves is
/// worse than one that never left.
fn cursor_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => {
                    // The real one, not a dispatched event. `ShowCursor`
                    // keeps its display count on this thread's input queue,
                    // so it governs the pointer only while the pointer is
                    // over this window — and a test that asked the system
                    // about a pointer sitting on some other window would be
                    // reading an answer about somebody else's cursor.
                    park_pointer(&ui);
                    hover(&ui, 40.0, 40.0);
                    report_cursor(&ui, "hand just moved");
                }
                2 => report_cursor(&ui, "a second later"),
                // Past HIDE_AFTER, plus a poll for the clock to notice.
                8 => report_cursor(&ui, "chrome faded"),
                9 => {
                    hover(&ui, 60.0, 80.0);
                    eprintln!("dbm: cursor  --- the hand moves ---");
                }
                10 => report_cursor(&ui, "hand moved again"),
                11 => eprintln!("dbm: --- cursor test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Put the physical pointer in the middle of the window.
#[cfg(windows)]
fn park_pointer(ui: &MainWindow) {
    let at = ui.window().position();
    let size = ui.window().size();
    let (x, y) = (
        at.x + size.width as i32 / 2,
        at.y + size.height as i32 / 2,
    );
    let moved = unsafe { windows::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y) };
    eprintln!("dbm: cursor  parked at {x},{y} ({})", if moved.is_ok() { "ok" } else { "refused" });
}

#[cfg(not(windows))]
fn park_pointer(_ui: &MainWindow) {}

fn report_cursor(ui: &MainWindow, when: &str) {
    let asked = ui.get_pointer_hidden();
    let on_screen = match crate::cursor::on_screen() {
        Some(true) => "on screen",
        Some(false) => "hidden",
        None => "not answerable on this platform",
    };
    eprintln!(
        "dbm: cursor  {when:<18} interface wants it {:<8} | system says {on_screen}",
        if asked { "hidden" } else { "shown" }
    );
}

fn hover(ui: &MainWindow, x: f32, y: f32) {
    use slint::platform::WindowEvent;
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(x, y),
    });
}

fn report_preview(ui: &MainWindow, label: &str) {
    let overlay = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .count();
    eprintln!(
        "dbm: preview {label:<16} time={:?} tile=({},{}) of {}x{} | {overlay} glass rect(s)",
        ui.get_preview_time(),
        ui.get_preview_clip_x(),
        ui.get_preview_clip_y(),
        ui.get_preview_tile_w(),
        ui.get_preview_tile_h(),
    );
}

/// How a panel divided itself up, against what its lists wanted.
fn report_lists(ui: &MainWindow, label: &str) {
    eprintln!(
        "dbm: {label:<18} panel={:.0} | subs {:.0}/{:.0} audio {:.0}/{:.0}",
        ui.get_tracks_h(),
        ui.get_subs_list_h(),
        ui.get_subs_natural(),
        ui.get_audio_list_h(),
        ui.get_audio_natural(),
    );
}

/// Click a point, the way a mouse would.
///
/// A `Flickable` has to tell a drag from a tap, so a list that scrolls
/// correctly can still be one whose rows have stopped responding. Nothing
/// short of a real click finds that.
fn click(ui: &MainWindow, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let position = slint::LogicalPosition::new(x, y);
    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

/// Turn a wheel over a point, the way a mouse would.
fn wheel(ui: &MainWindow, x: f32, y: f32, delta: f32) {
    use slint::platform::WindowEvent;
    let position = slint::LogicalPosition::new(x, y);
    let window = ui.window();
    // The pointer has to be over the list for the list to be what scrolls.
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x: 0.0,
        delta_y: delta,
    });
}

/// Check the height split and that the lists actually move.
///
/// The split has four cases in it — both lists fitting, either one borrowing
/// from the other, and both being capped — so it is worth reading back rather
/// than trusting. The wheel is the other half: a list sized correctly but
/// stuck at the top would look identical from here.
fn scroll_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1000),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                // Short enough that the panel cannot hold both lists, which
                // is the only state where the split does anything.
                0 => {
                    ui.window()
                        .set_size(slint::LogicalSize::new(1280.0, 520.0));
                    ui.invoke_open_menu(true);
                }
                1 => report_lists(&ui, "tracks, short"),
                2 => {
                    // Into the subtitle list: below the panel top, the
                    // padding and the first heading.
                    let x = ui.window().size().width as f32 / ui.window().scale_factor() - 100.0;
                    let y = ui.get_timeline_y() - ui.get_tracks_h() + 60.0;
                    eprintln!("dbm: wheel over the subtitle list at ({x:.0}, {y:.0})");
                    wheel(&ui, x, y, -120.0);
                }
                3 => eprintln!("dbm: subs scrolled to {:.0}", ui.get_subs_scroll()),
                4 => {
                    ui.window()
                        .set_size(slint::LogicalSize::new(1280.0, 720.0));
                    ui.invoke_open_playlist(true);
                }
                5 => eprintln!(
                    "dbm: playlist panel={:.0} list {:.0}/{:.0}",
                    ui.get_playlist_h(),
                    ui.get_playlist_list_h(),
                    ui.get_playlist_natural(),
                ),
                6 => {
                    let x = ui.get_playlist_anchor();
                    let y = ui.get_timeline_y() - ui.get_playlist_h() + 60.0;
                    eprintln!("dbm: wheel over the playlist at ({x:.0}, {y:.0})");
                    wheel(&ui, x, y, -240.0);
                }
                7 => eprintln!("dbm: playlist scrolled to {:.0}", ui.get_playlist_scroll()),
                // Back to the tracks menu and click the first real subtitle
                // row: the one below "Off", inside the scrolling list.
                8 => ui.invoke_open_menu(true),
                9 => {
                    let top = ui.get_timeline_y() - ui.get_tracks_h();
                    let y = top + 14.0 + 28.0 + 34.0 + 17.0;
                    let x = ui.get_subs_anchor();
                    eprintln!("dbm: clicking a subtitle row at ({x:.0}, {y:.0})");
                    click(&ui, x, y);
                }
                10 => eprintln!(
                    "dbm: subtitle now {:?}",
                    selected_label(&ui.get_sub_tracks())
                ),
                11 => eprintln!("dbm: --- scroll test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Press a chord, the way a keyboard would.
///
/// Slint tracks modifier state from the modifier key's own press and release,
/// so a chord is four events rather than one with flags.
fn chord(ui: &MainWindow, modifiers: &[slint::platform::Key], text: &str) {
    use slint::platform::WindowEvent;
    let window = ui.window();
    for m in modifiers {
        window.dispatch_event(WindowEvent::KeyPressed { text: (*m).into() });
    }
    window.dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: text.into() });
    for m in modifiers.iter().rev() {
        window.dispatch_event(WindowEvent::KeyReleased { text: (*m).into() });
    }
}

/// Walk the keyboard into a panel and down it, with real key events.
///
/// The ring, the wrap and the scroll-to-follow are all driven from the same
/// dispatch a person's keyboard reaches, so this presses `P` and then Down
/// rather than assigning to the property: assigning would prove the wash
/// draws and nothing about whether the keys arrive.
///
/// Twelve rows down because nine fit: the interesting part is the three past
/// the bottom edge, where the list has to move for the ring to stay visible.
fn reach_test(ui: &MainWindow) -> slint::Timer {
    use slint::platform::Key;

    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(400),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            // `DBM_REACH_TEST=glass` walks into a settings page instead, where
            // the rows are sliders and the interesting keys are left and right.
            // `=up` opens the playlist the way the folder button does, with no
            // key at all, so the ring starts hidden and the first Up has to
            // both show it and land at the bottom.
            let mode = std::env::var("DBM_REACH_TEST").unwrap_or_default();
            if mode == "up" {
                match step {
                    0 => {
                        // Not a key: this is what the folder button does.
                        ui.invoke_open_playlist(true);
                        eprintln!("dbm: playlist opened without a key");
                    }
                    1 | 2 => {
                        chord(&ui, &[], slint::SharedString::from(Key::UpArrow).as_str());
                        eprintln!("dbm: up -> row {}", ui.get_focus_row());
                    }
                    3 => eprintln!("dbm: --- reach test done ---"),
                    _ => {}
                }
                step += 1;
                return;
            }
            let glass = mode == "glass";
            match (glass, step) {
                (false, 0) => {
                    eprintln!("dbm: pressing P for the playlist");
                    chord(&ui, &[], "p");
                }
                (false, 1..=12) => {
                    chord(&ui, &[], slint::SharedString::from(Key::DownArrow).as_str());
                    eprintln!(
                        "dbm: down {} -> row {}, scrolled {}",
                        step,
                        ui.get_focus_row(),
                        ui.get_playlist_scroll()
                    );
                }
                (true, 0) => {
                    eprintln!("dbm: pressing S for the settings");
                    chord(&ui, &[], "s");
                }

                // Down onto Liquid glass, Enter to open it, then Down again
                // onto the first slider.
                (true, 1) | (true, 3) => {
                    chord(&ui, &[], slint::SharedString::from(Key::DownArrow).as_str());
                    eprintln!("dbm: down -> row {}", ui.get_focus_row());
                }
                (true, 2) => {
                    chord(&ui, &[], slint::SharedString::from(Key::Return).as_str());
                    eprintln!("dbm: enter, opening the glass page");
                }
                (true, 4..=11) => {
                    chord(&ui, &[], slint::SharedString::from(Key::LeftArrow).as_str());
                    eprintln!("dbm: left on row {}", ui.get_focus_row());
                }
                (_, 13) => eprintln!("dbm: --- reach test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Drive the shortcuts that have nowhere visible to report.
///
/// Ctrl+O puts a native dialog on screen, so there is no property to read
/// afterwards — `dialog` traces the request instead, and that line is the
/// evidence the chord arrived.
fn key_test(ui: &MainWindow) -> slint::Timer {
    use slint::platform::Key;

    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                // C, twice, on a file whose subtitles are not showing. The
                // first press has to turn them on: it used to turn off a
                // `sub-visibility` that was already showing nothing, which
                // from the outside was a key that did nothing.
                //
                // Before the dialogs, not after. The two that follow put a
                // file chooser on screen and it keeps the keyboard, so
                // anything pressed after them is pressed at the dialog.
                0 => {
                    eprintln!("dbm: subs showing {}", ui.get_subs_visible());
                    eprintln!("dbm: pressing C");
                    chord(&ui, &[], "c");
                }
                1 => {
                    eprintln!("dbm: subs showing {}", ui.get_subs_visible());
                    eprintln!("dbm: pressing C again");
                    chord(&ui, &[], "c");
                }
                2 => {
                    eprintln!("dbm: subs showing {}", ui.get_subs_visible());
                    eprintln!("dbm: pressing B (ambience)");
                    chord(&ui, &[], "b");
                }
                3 => eprintln!("dbm: ambience now {}", ui.get_ambience_on()),
                4 => {
                    eprintln!("dbm: pressing Ctrl+O");
                    chord(&ui, &[Key::Control], "o");
                }
                5 => {
                    eprintln!("dbm: pressing Ctrl+Shift+O");
                    chord(&ui, &[Key::Control, Key::Shift], "O");
                }
                7 => eprintln!("dbm: --- key test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Open a path the way the dialog does, and say what came of it.
///
/// Everything after the path is chosen is shared with the command line, so
/// this covers the part of opening that can actually break: the scan, the
/// `loadlist`, and the seek to the right entry once it lands.
fn open_test(ui: &MainWindow, path: String) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1500),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => {
                    eprintln!("dbm: opening {path}");
                    ui.invoke_open_path(path.as_str().into());
                }
                // Two ticks: the scan is on the worker and the `loadlist`
                // that follows is asynchronous again.
                2 => report_open(&ui, "after open"),
                3 => eprintln!("dbm: --- open test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

fn report_open(ui: &MainWindow, label: &str) {
    let entries: Vec<String> = ui
        .get_playlist()
        .iter()
        .map(|e| {
            let mark = if e.current { "*" } else { " " };
            format!("{mark}{}", e.label)
        })
        .collect();
    eprintln!(
        "dbm: {label:<12} title={:?} playing={} | {} entries: {}",
        ui.get_media_title(),
        !ui.get_paused(),
        entries.len(),
        entries.join(", "),
    );
}

/// Put a real dialog on screen and leave it there.
///
/// There is nothing to assert from here — a native dialog cannot be driven
/// by a timer — but the trace alongside it answers the only question that
/// matters: whether the player keeps drawing while it is up.
fn dialog_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(2000),
        move || {
            if let Some(ui) = weak.upgrade() {
                eprintln!("dbm: --- opening a file dialog; draws should continue ---");
                ui.invoke_open_file();
            }
        },
    );
    timer
}

/// Open each panel in turn and count what reaches the pipeline.
///
/// The count is the thing to watch rather than the three booleans: the
/// panels all sit in the same corner now, so a second one opening would not
/// just be untidy, it would stack a second sheet of glass on the first.
fn panel_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(900),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_panels(&ui, "nothing open"),
                1 => ui.invoke_open_menu(true),
                2 => report_panels(&ui, "tracks"),
                3 => ui.invoke_open_playlist(true),
                4 => report_panels(&ui, "playlist over tracks"),
                5 => ui.invoke_open_settings(true),
                6 => report_panels(&ui, "settings over playlist"),
                7 => ui.invoke_open_files(true),
                8 => report_panels(&ui, "files over settings"),
                9 => ui.invoke_open_menu(true),
                10 => report_panels(&ui, "tracks over files"),
                // Each settings page sizes the panel from its own row
                // count, so walk them and see what the pipeline is told.
                11 => ui.invoke_open_settings_page(0),
                12 => report_panels(&ui, "settings list"),
                13 => ui.invoke_open_settings_page(1),
                14 => report_panels(&ui, "settings glass"),
                15 => ui.invoke_open_settings_page(2),
                16 => report_panels(&ui, "settings ambience"),
                17 => ui.invoke_open_settings_page(3),
                18 => report_panels(&ui, "settings subtitles"),
                19 => ui.invoke_open_settings_page(4),
                20 => report_panels(&ui, "settings shortcuts"),
                21 => ui.invoke_open_settings(false),
                22 => report_panels(&ui, "closed again"),
                23 => eprintln!("dbm: --- panel test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Which panels believe they are open, and how many rects that actually
/// produces — a zero-sized rect is how a closed panel is expressed, and the
/// Rust side drops those.
fn report_panels(ui: &MainWindow, label: &str) {
    let drawn: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| format!("x{}..{} h{}", r.x, r.x + r.width, r.height))
        .collect();
    eprintln!(
        "dbm: {label:<22} tracks={} playlist={} settings={} files={} | {} rect(s) {}",
        ui.get_menu_open(),
        ui.get_playlist_open(),
        ui.get_settings_open(),
        ui.get_files_open(),
        drawn.len(),
        drawn.join(" "),
    );
}

/// What mpv holds against what the panel is showing.
///
/// Both, because either alone would miss half of what can go wrong: mpv
/// alone would not catch a readout that never updates, and the readout alone
/// would not catch a value that never left the process.
fn report_subs(ui: &MainWindow, mpv: &Mpv, label: &str) {
    // Blocking reads, acceptable only because this is a one-shot diagnostic
    // step rather than the frame path.
    eprintln!(
        "dbm: {label:<20} mpv delay={:?} scale={:?} pos={:?} | panel {} {} {}",
        mpv.get_property("sub-delay"),
        mpv.get_property("sub-scale"),
        mpv.get_property("sub-pos"),
        ui.get_sub_delay_text(),
        ui.get_sub_scale_text(),
        ui.get_sub_pos_text(),
    );
}

/// Step each subtitle adjustment, push one past its limit, then reset.
///
/// The clamp is the part worth driving: it lives in `commands` so that the
/// value recorded for saving is the one mpv was actually given, and a drift
/// between those two would only show up as a setting that comes back wrong
/// on the next run.
fn subs_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(900),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_subs(&ui, &mpv, "start"),
                1 => {
                    for _ in 0..4 {
                        ui.invoke_nudge_sub_delay(ui.get_delay_step());
                    }
                }
                2 => report_subs(&ui, &mpv, "delay +4 steps"),
                3 => {
                    for _ in 0..3 {
                        ui.invoke_nudge_sub_scale(ui.get_sub_scale_step());
                    }
                }
                4 => report_subs(&ui, &mpv, "size +3 steps"),
                5 => {
                    for _ in 0..5 {
                        ui.invoke_nudge_sub_pos(-ui.get_sub_pos_step());
                    }
                }
                6 => report_subs(&ui, &mpv, "position -5 steps"),
                // Far past the ceiling in one go, which is what a held
                // button amounts to.
                7 => {
                    for _ in 0..80 {
                        ui.invoke_nudge_sub_scale(ui.get_sub_scale_step());
                    }
                }
                8 => report_subs(&ui, &mpv, "size held up"),
                9 => {
                    for _ in 0..120 {
                        ui.invoke_nudge_sub_pos(-ui.get_sub_pos_step());
                    }
                }
                10 => report_subs(&ui, &mpv, "position held down"),
                11 => ui.invoke_reset_subtitles(),
                12 => report_subs(&ui, &mpv, "after reset"),
                13 => eprintln!("dbm: --- subtitle test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// The glass slider rows, as the panel would show them. Named rather than
/// indexed in the output, so a registry reordering does not silently change
/// what the run is reporting on.
fn report_row(ui: &MainWindow, label: &str) {
    let rows: Vec<String> = ui
        .get_glass_params()
        .iter()
        .map(|p| format!("{}={}", p.label, p.readout))
        .collect();
    eprintln!("dbm: {label:<18} {}", rows.join(" "));
}

/// Walk the effect switches and autoplay, leaving each one back where it
/// started so a run does not quietly rewrite the saved settings.
///
/// The interesting part is not the property afterwards — that only says the
/// click landed — but what the probe reads out of the framebuffer between the
/// steps, and what mpv says `keep-open` became.
fn toggle_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            // A panel has to be on screen for there to be glass to sample.
            let flip = |name: &str, on: bool| eprintln!("dbm: {name} -> {on}");
            match step {
                0 => ui.set_settings_open(true),
                // The glass no longer has a switch, so the ambient border is
                // the only effect left to flip. What the probe reads between
                // these steps is still the point: glass is sampled out of the
                // framebuffer under the panel that is open.
                1 => {
                    ui.set_ambience_on(false);
                    ui.invoke_set_ambience(false);
                    flip("ambience", false);
                }
                2 => {
                    ui.set_ambience_on(true);
                    ui.invoke_set_ambience(true);
                    flip("ambience", true);
                }
                3 => {
                    ui.set_autoplay(false);
                    ui.invoke_set_autoplay(false);
                }
                // A blocking read, which is only acceptable here: this is a
                // one-shot diagnostic step, not the frame path. Autoplay has
                // no picture to check, so mpv itself is the witness.
                4 => eprintln!(
                    "dbm: autoplay off -> keep-open={:?}",
                    mpv.get_property("keep-open")
                ),
                5 => {
                    ui.set_autoplay(true);
                    ui.invoke_set_autoplay(true);
                }
                6 => eprintln!(
                    "dbm: autoplay on  -> keep-open={:?}",
                    mpv.get_property("keep-open")
                ),
                // Reset. Drive one slider somewhere it would never be by
                // default, then put the section back and read the row again.
                // The row is the thing to check: the value reaching the
                // pipeline is already covered by `DBM_PARAM_TEST`, and what
                // broke before was the model, which lost the drag when it
                // was rebuilt instead of updated in place.
                7 => {
                    ui.invoke_set_param(7, 1.0);
                    eprintln!("dbm: tint driven to max");
                }
                8 => report_row(&ui, "after tint max"),
                9 => {
                    ui.invoke_reset_section(0);
                    eprintln!("dbm: glass section reset");
                }
                10 => report_row(&ui, "after reset"),
                11 => eprintln!("dbm: --- toggle test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

fn modal_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(3000),
        move || {
            if let Some(ui) = weak.upgrade() {
                eprintln!("dbm: --- entering modal size loop ---");
                modal_loop::enter_modal_size_loop_for_test(ui.window());
            }
        },
    );
    timer
}

/// Stage the failure the audio watchdog exists for.
///
/// The real one needs a Bluetooth device to walk out of range, so the two
/// halves are faked separately and each honestly: mpv really does drop the
/// audio track, which is what leaves the player silent, and the watchdog
/// really is handed a device list with something in it that was not there
/// before. Everything between — the diff, the settle, the rebuild on the
/// worker — is the code under test.
///
/// The property writes here block the UI thread. That is fine in a harness and
/// would not be anywhere else.
fn audio_test(
    mpv: std::sync::Arc<Mpv>,
    audio: std::rc::Rc<crate::audio::Watchdog>,
) -> slint::Timer {
    const BASELINE: &str = r#"[{"name":"auto","description":"Autoselect device"}]"#;
    const PLUS_ONE: &str = r#"[{"name":"auto","description":"Autoselect device"},
        {"name":"wasapi/harness","description":"A device that just turned up"}]"#;
    const PLUS_TWO: &str = r#"[{"name":"auto","description":"Autoselect device"},
        {"name":"wasapi/harness","description":"A device that just turned up"},
        {"name":"wasapi/harness-2","description":"And another"}]"#;

    let timer = slint::Timer::default();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(900),
        move || {
            match step {
                0 => report_audio(&mpv, "start"),
                // Take the baseline, so the next list reads as an arrival.
                1 => audio.on_event(&device_list(BASELINE)),
                // What mpv does by itself when the device under the output
                // disappears: the track goes, playback does not.
                2 => {
                    if let Err(e) = mpv.set_property("aid", "no") {
                        eprintln!("dbm: audio test: could not drop the track: {e}");
                    }
                }
                3 => report_audio(&mpv, "device lost"),
                4 => audio.on_event(&device_list(PLUS_ONE)),
                // The settle is 1200ms and the rebuild is a worker round
                // trip, so the result lands somewhere in these two steps.
                5 | 6 => {}
                7 => report_audio(&mpv, "after recovery"),
                // Second half: silence somebody asked for is not a fault.
                // The interface's own "Off" row, then another device turning
                // up, which must leave it alone.
                8 => {
                    audio.note_user_selection(None);
                    if let Err(e) = mpv.set_property("aid", "no") {
                        eprintln!("dbm: audio test: could not turn audio off: {e}");
                    }
                }
                9 => report_audio(&mpv, "user chose off"),
                10 => audio.on_event(&device_list(PLUS_TWO)),
                11 | 12 => {}
                13 => report_audio(&mpv, "still off"),
                14 => eprintln!("dbm: --- audio test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// An `audio-device-list` change as mpv would report it.
fn device_list(payload: &str) -> crate::mpv::Event {
    crate::mpv::Event::Property {
        name: "audio-device-list".into(),
        value: crate::mpv::Value::Str(payload.into()),
    }
}

fn report_audio(mpv: &Mpv, label: &str) {
    let read = |name: &str| mpv.get_property(name).unwrap_or_else(|| "-".into());
    eprintln!(
        "dbm: audio {label:<16} aid={} device={} tracks={}",
        read("aid"),
        read("audio-device"),
        read("track-list/count"),
    );
}

/// Drive the OS media overlay from both ends.
///
/// The outbound half is read back out of Windows rather than out of our own
/// copy of it: `GlobalSystemMediaTransportControlsSessionManager` is the same
/// API the lock screen uses, so whatever it reports is what the machine
/// believes. The inbound half asks Windows to deliver a transport
/// button to this player's own session, which is the route a headset button
/// takes — Windows calling our `ButtonPressed` — aimed rather than broadcast.
/// Every report also names the session Windows has in front, because that is
/// who a real key press would have reached instead.
#[cfg(windows)]
fn smtc_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_smtc(&ui, &mpv, "start"),
                1 => ask(Button::Pause),
                2 => report_smtc(&ui, &mpv, "pause button"),
                3 => ask(Button::Play),
                4 => report_smtc(&ui, &mpv, "play button"),
                5 => ask(Button::Next),
                6 => report_smtc(&ui, &mpv, "next button"),
                7 => eprintln!("dbm: --- media overlay test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Ask Windows to deliver one transport button to *this* player's session.
///
/// A real media key is the same route — Windows decides which session gets it
/// and calls that application's `ButtonPressed` — with the aiming done for us.
/// `SendInput` with `VK_MEDIA_PLAY_PAUSE` was the first version of this and had
/// to be abandoned: the session Windows has in front is usually somebody
/// else's, so the press either went to another application's music or the test
/// declined to happen at all. Naming the session removes both problems and
/// covers strictly more of the path.
#[cfg(windows)]
fn ask(button: Button) {
    let Some(session) = our_session() else {
        eprintln!("dbm: smtc test: Windows has no session for this player");
        return;
    };
    let asked = match button {
        Button::Play => session.TryPlayAsync().and_then(|op| op.get()),
        Button::Pause => session.TryPauseAsync().and_then(|op| op.get()),
        Button::Next => session.TrySkipNextAsync().and_then(|op| op.get()),
    };
    match asked {
        // The bool says Windows accepted the request, not that anything came
        // of it — that is what the next report is for.
        Ok(true) => {}
        Ok(false) => eprintln!("dbm: smtc test: Windows refused {button:?}"),
        Err(e) => eprintln!("dbm: smtc test: {button:?} failed: {e}"),
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
enum Button {
    Play,
    Pause,
    Next,
}

/// This player's own entry among the media sessions Windows is tracking.
///
/// Found by the executable name, which is what an unpackaged application's
/// `SourceAppUserModelId` amounts to.
#[cfg(windows)]
fn our_session() -> Option<windows::Media::Control::GlobalSystemMediaTransportControlsSession> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;

    let manager = Manager::RequestAsync().ok()?.get().ok()?;
    manager.GetSessions().ok()?.into_iter().find(|session| {
        session
            .SourceAppUserModelId()
            .is_ok_and(|id| id.to_string_lossy().contains("dbm-player"))
    })
}

/// What the player thinks, and what Windows thinks, side by side. They are the
/// two ends of the same claim and only worth reading together.
#[cfg(windows)]
fn report_smtc(ui: &MainWindow, mpv: &Mpv, label: &str) {
    let (app, title, status) =
        current_session().unwrap_or_else(|| ("nothing".into(), "-".into(), "no session".into()));
    eprintln!(
        "dbm: smtc {label:<14} ours[paused={} entry={} {}] windows[{app} {status} {title:?}]",
        ui.get_paused(),
        mpv.get_property("playlist-pos").unwrap_or_else(|| "-".into()),
        ui.get_media_title(),
    );
}

/// The session Windows would send a media key to: who owns it, what it says is
/// playing, and whether it is playing. Not necessarily us — that is the point
/// of reporting it.
#[cfg(windows)]
fn current_session() -> Option<(String, String, String)> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;

    // Blocking waits, on the UI thread. Acceptable here and nowhere else.
    let manager = Manager::RequestAsync().ok()?.get().ok()?;
    let session = manager.GetCurrentSession().ok()?;
    let app = session
        .SourceAppUserModelId()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|_| "?".into());
    let title = session
        .TryGetMediaPropertiesAsync()
        .ok()
        .and_then(|op| op.get().ok())
        .and_then(|props| props.Title().ok())
        .map(|t| t.to_string_lossy())
        .unwrap_or_else(|| "?".into());
    let status = session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .map(|s| {
            match s.0 {
                0 => "closed",
                1 => "opened",
                2 => "changing",
                3 => "stopped",
                4 => "playing",
                5 => "paused",
                _ => "unknown",
            }
            .to_string()
        })
        .unwrap_or_else(|_| "?".into());
    Some((app, title, status))
}

/// Sit at the end of a file, where the only thing left on screen is the offer
/// of another one.
///
/// The end is reached honestly rather than by setting the flag: `at-end` comes
/// from mpv's own `eof-reached`, and anything that wrote it from this side
/// would be testing the harness instead of the player. So: autoplay off, so
/// mpv holds the last frame instead of moving on, then a seek to the final
/// moment and a wait.
///
/// It then stays there, deliberately, so the window can be photographed both
/// while the bar is still up and after it has gone.
fn end_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1000),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_end(&ui, "start"),
                // Both halves: the callback tells mpv, the property moves the
                // switch, exactly as clicking it does.
                1 => {
                    ui.invoke_set_autoplay(false);
                    ui.set_autoplay(false);
                }
                2 => ui.invoke_seek_fraction(0.999),
                3 | 4 => report_end(&ui, "seeking"),
                5 => report_end(&ui, "at the end"),
                6 => eprintln!("dbm: --- end-of-playback test: holding ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// What the player is showing at the end of a file, and whether the pipeline
/// was told to put glass under it.
fn report_end(ui: &MainWindow, label: &str) {
    let rects: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| format!("{}x{}@{},{}", r.width, r.height, r.x, r.y))
        .collect();
    eprintln!(
        "dbm: end {label:<12} at-end={} at-last={} paused={} t={}/{} | {} rect(s) {}",
        ui.get_at_end(),
        ui.get_at_last(),
        ui.get_paused(),
        ui.get_elapsed(),
        ui.get_total(),
        rects.len(),
        rects.join(" "),
    );
}

/// Drop a file on the window without a mouse.
///
/// Everything from the `WM_DROPFILES` message inward is the real path: the
/// same block the shell builds, posted to the same window, read by the same
/// handler. What it cannot cover is the drag itself — whether Explorer's drop
/// lands on our target or on the one winit registered — so a real drag is
/// still worth doing once by hand.
#[cfg(windows)]
fn drop_test(ui: &MainWindow, path: String) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_drop(&ui, "before"),
                1 => {
                    if let Err(e) = crate::dropped::post_test_drop(&ui, &path) {
                        eprintln!("dbm: drop test: {e}");
                    }
                }
                2 | 3 => report_drop(&ui, "after the drop"),
                4 => eprintln!("dbm: --- drop test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

#[cfg(windows)]
fn report_drop(ui: &MainWindow, label: &str) {
    eprintln!(
        "dbm: drop {label:<16} title={:?} playlist={} entries",
        ui.get_media_title(),
        ui.get_playlist().row_count(),
    );
}

/// Check that a playlist row's length and progress are real.
///
/// The part that cannot be checked by reading the code is whether our idea of
/// where mpv keeps a file's resume position agrees with mpv's. So this plays a
/// while, asks mpv to write that file, and then looks for it by the name we
/// would look for it under — and reports what the rows say before and after.
fn progress_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_progress(&ui, &mpv, "on load"),
                1 => ui.invoke_seek_fraction(0.3),
                // What the ten-second checkpoint does, without the wait.
                2 => {
                    if let Err(e) = mpv.command_async(0, &["write-watch-later-config"]) {
                        eprintln!("dbm: progress test: {e}");
                    }
                }
                3 => report_resume_file(&mpv),
                // Opening the panel is what asks for a fresh read.
                4 => ui.invoke_open_playlist(true),
                5 => report_progress(&ui, &mpv, "panel open"),
                6 => eprintln!("dbm: --- progress test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

fn report_progress(ui: &MainWindow, mpv: &Mpv, label: &str) {
    let rows: Vec<String> = ui
        .get_playlist()
        .iter()
        .map(|e| {
            format!(
                "{}{}[{} {:.0}%]",
                if e.current { "*" } else { "" },
                short(&e.label),
                if e.length.is_empty() { "?" } else { &e.length },
                e.progress * 100.0,
            )
        })
        .collect();
    eprintln!(
        "dbm: progress {label:<12} t={} | {}",
        mpv.get_property("time-pos").unwrap_or_else(|| "-".into()),
        rows.join(" "),
    );
}

/// Whether mpv's resume file for the playing path is where we look for it.
///
/// This is the whole question: mpv names it by the MD5 of the path it was
/// given, and if our copy of that rule is wrong every row shows an empty bar
/// and nothing anywhere reports an error.
fn report_resume_file(mpv: &Mpv) {
    let Some(path) = mpv.get_property("path") else {
        eprintln!("dbm: progress: nothing playing");
        return;
    };
    let name = crate::durations::watch_later_name(&path);
    let file = crate::paths::watch_later_dir().join(&name);
    let start = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("start=").map(str::to_owned))
        })
        .unwrap_or_else(|| "none".into());
    eprintln!(
        "dbm: progress resume   {name} exists={} start={start}",
        file.is_file(),
    );
}

/// Enough of a label to tell rows apart in one line of output.
fn short(label: &slint::SharedString) -> String {
    label.chars().take(24).collect()
}

/// Raise the action flash from the keyboard and watch it come and go.
///
/// Real key events, because the flash is raised inside the key handlers: an
/// invoked callback would skip the exact statements under test. Each report
/// names the glass rect published for the ring, which is what the flash
/// actually is — the glyph is only what sits in it.
fn flash_test(ui: &MainWindow) -> slint::Timer {
    use slint::platform::Key;

    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_flash(&ui, "before"),
                1 => arrow(&ui, Key::RightArrow),
                2 => report_flash(&ui, "seek forward"),
                3 => arrow(&ui, Key::LeftArrow),
                4 => report_flash(&ui, "seek back"),
                // Held keys are the common case: the dwell has to restart
                // rather than the second press landing on the first's fade.
                5 => {
                    for _ in 0..3 {
                        chord(&ui, &[], "z");
                    }
                }
                6 => report_flash(&ui, "delay, thrice"),
                // Long enough for the dwell and the fade both.
                7 | 8 => {}
                9 => report_flash(&ui, "after it leaves"),
                10 => eprintln!("dbm: --- flash test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// A special key as the character Slint encodes it in.
fn arrow(ui: &MainWindow, key: slint::platform::Key) {
    let text: slint::SharedString = key.into();
    chord(ui, &[], &text);
}

fn report_flash(ui: &MainWindow, label: &str) {
    // The flash publishes a ring, and a second rect for the figure when it
    // has one. Nothing else is open in this run, so anything clear of the
    // bar is the flash.
    let bar_top = ui.get_timeline_y();
    let rects: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0 && r.y + r.height < bar_top)
        .map(|r| format!("{}x{}@{:.0},{:.0}", r.width, r.height, r.x, r.y))
        .collect();
    eprintln!(
        "dbm: flash {label:<18} seq={} delay={} | {} rect(s) {}",
        ui.global::<crate::Flash>().get_seq(),
        ui.get_sub_delay_text(),
        rects.len(),
        rects.join(" "),
    );
}
