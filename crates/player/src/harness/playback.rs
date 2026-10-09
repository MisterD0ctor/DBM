//! Exercises of playback: the transport, scrubbing, the fade, the seek
//! preview, the ends of a file, the flash, and resume progress.

use std::time::Duration;

use slint::{ComponentHandle, Model};

use super::drive::{arrow, chord, hover, wheel};
use super::{playlist_rows, selected_label, short};
use crate::playback::mpv::Mpv;
use crate::MainWindow;

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

pub(super) fn input_test(ui: &MainWindow) -> slint::Timer {
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

/// Drag the timeline the way a hand does: many small position updates in
/// quick succession. A single seek says nothing about scrubbing; the whole
/// question is what happens when they arrive faster than mpv can service
/// them.
pub(super) fn scrub_test(ui: &MainWindow) -> slint::Timer {
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

/// Watch the bar fade out, in the value the pipeline is actually given.
///
/// Every surface publishes how far it has faded in, so a fade is readable
/// without a screenshot: a run of values between 1 and 0 is an animation, and
/// a jump straight to nothing is a cut. Worth checking because the interface
/// only asks for frames while it is on screen — if Slint did not drive its
/// own animation, the fade out is precisely what would vanish.
pub(super) fn fade_test(ui: &MainWindow) -> slint::Timer {
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
                let steps: Vec<String> = seen.iter().map(|v| format!("{v:.2}")).collect();
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

/// Walk the pointer along the timeline and read the preview off the far end.
///
/// A real pointer move, not just the callback: the overlay only appears while
/// the track is hovered, and its rect is what tells the pipeline where to put
/// glass. So this checks both halves — that hovering raises it, and that the
/// timestamp and the tile under the pointer are the right ones.
pub(super) fn preview_test(ui: &MainWindow) -> slint::Timer {
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

/// Set the subtitles' timing on one file, go to the next, and come back.
///
/// Timing belongs to the file it was set on: the next one should start
/// untimed, and the first should have its own back when it is returned to.
/// Both halves, because either can fail alone — a timing that follows the
/// playlist is the first failing, and one that is simply dropped at the end
/// of the file would pass the first check and fail the second.
///
/// The watch-later file is written by hand before leaving. The player does
/// that every ten seconds while something plays, and waiting it out would
/// only be testing the clock.
pub(super) fn timing_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let report = |ui: &MainWindow, mpv: &Mpv, label: &str| {
        eprintln!(
            "dbm: timing {label:<20} {:?} delay={:+.2} speed={:.4}",
            short(&mpv.get_property("filename").unwrap_or_default().into()),
            ui.get_sub_delay(),
            ui.get_sub_speed(),
        );
    };
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(700),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                // Let the film open and its playlist arrive first.
                0..=3 => {}
                4 => report(&ui, &mpv, "as it opened"),
                5 => {
                    ui.invoke_nudge_sub_delay(-5.0);
                    let _ = mpv.set_property("sub-speed", "1.04");
                }
                6 => {
                    report(&ui, &mpv, "set on the first");
                    let _ = mpv.command_async(0, &["write-watch-later-config"]);
                }
                7 => ui.invoke_playlist_step(1),
                10 => report(&ui, &mpv, "on the next file"),
                11 => ui.invoke_playlist_step(-1),
                14 => report(&ui, &mpv, "back on the first"),
                15 => eprintln!("dbm: --- timing test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Turn the wheel over the picture, then over the volume track.
///
/// The wheel was the volume from anywhere in the window once, and is now the
/// volume only on its own track. Both halves are worth reading back: that the
/// picture no longer answers, and that the track still does — with the level
/// said in the hover label at once, not after the wait a name gets. The steps
/// are shorter than that wait so a label that is up by the next one cannot
/// have got there by waiting.
pub(super) fn wheel_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(400),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let (x, y) = (ui.get_vol_cx(), ui.get_vol_cy());
            match step {
                // Let the film start and the bar settle first.
                0..=4 => {}
                5 => report_wheel(&ui, "at rest"),
                6 => {
                    for _ in 0..3 {
                        wheel(&ui, 640.0, 300.0, 120.0);
                    }
                }
                7 => report_wheel(&ui, "three notches, picture"),
                8 => {
                    eprintln!("dbm: wheel over the volume track at ({x:.0}, {y:.0})");
                    wheel(&ui, x, y, 120.0);
                }
                9 => report_wheel(&ui, "one notch up, track"),
                10 => {
                    for _ in 0..3 {
                        wheel(&ui, x, y, -120.0);
                    }
                }
                11 => report_wheel(&ui, "three notches down"),
                12 => ui.invoke_toggle_mute(),
                13 => report_wheel(&ui, "muted"),
                14 => ui.invoke_toggle_mute(),
                15 => hover(&ui, 640.0, 300.0),
                16 => report_wheel(&ui, "pointer gone"),
                17 => eprintln!("dbm: --- wheel test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

fn report_wheel(ui: &MainWindow, label: &str) {
    let tip = ui.global::<crate::Tip>();
    let glass = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .count();
    eprintln!(
        "dbm: wheel {label:<24} vol={} muted={} tip={:?} {:?} at {:.0} | {glass} glass rect(s)",
        ui.get_volume(),
        ui.get_muted(),
        tip.get_label(),
        tip.get_keys(),
        tip.get_cx(),
    );
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
pub(super) fn end_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    // `DBM_END_TEST=enter` goes on past the pill with the one key the ring
    // put on it; anything else holds the pill up to be photographed.
    let enter = std::env::var("DBM_END_TEST").is_ok_and(|v| v == "enter");
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
                6 if enter => {
                    let key = slint::SharedString::from(slint::platform::Key::Return);
                    chord(&ui, &[], &key);
                }
                7 | 8 if enter => report_end(&ui, "after enter"),
                9 if enter => eprintln!("dbm: --- end-of-playback test done ---"),
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
        "dbm: end {label:<12} at-end={} at-last={} paused={} ring={}@{} t={}/{} {:?} | {} rect(s) {}",
        ui.get_at_end(),
        ui.get_at_last(),
        ui.get_paused(),
        ui.global::<crate::Chrome>().get_focus_visible(),
        ui.get_bar_focus(),
        ui.get_elapsed(),
        ui.get_total(),
        ui.get_current_path().rsplit(['\\', '/']).next().unwrap_or_default().to_string(),
        rects.len(),
        rects.join(" "),
    );
}

/// Play into a file's closing credits.
///
/// Few files carry chapters and fewer name their credits, so the chapters
/// are supplied: an ffmetadata file, which mpv takes through `chapters-file`,
/// naming the last two minutes `End Credits`. mpv reads it when a file opens,
/// so the file is opened again, the way the dialog would.
pub(super) fn credits_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    let mut credits_at = 0.0f64;
    let own = std::env::var("DBM_CREDITS_TEST").is_ok_and(|v| v == "own");
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1000),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                2 if own => {
                    let marks = ui.get_chapter_marks();
                    credits_at = marks
                        .row_count()
                        .checked_sub(1)
                        .and_then(|last| marks.row_data(last))
                        .map_or(0.0, f64::from);
                    eprintln!(
                        "dbm: credits: the file's own {} chapters, the last from {credits_at:.1}s",
                        marks.row_count()
                    );
                }
                2 => {
                    let duration = f64::from(ui.get_duration());
                    credits_at = (duration - 120.0).max(duration / 2.0);
                    let marks = std::env::temp_dir().join("dbm-credits-test.ffmeta");
                    let text = format!(
                        ";FFMETADATA1\n\
                         [CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND={c}\ntitle=Episode\n\
                         [CHAPTER]\nTIMEBASE=1/1000\nSTART={c}\nEND={d}\ntitle=End Credits\n",
                        c = (credits_at * 1000.0) as u64,
                        d = (duration * 1000.0) as u64,
                    );
                    if let Err(e) = std::fs::write(&marks, text) {
                        eprintln!("dbm: credits: cannot write chapters: {e}");
                        return;
                    }
                    if let Err(e) = mpv.set_property("chapters-file", &marks.to_string_lossy()) {
                        eprintln!("dbm: credits: mpv refused chapters-file: {e}");
                    }
                    eprintln!("dbm: credits from {credits_at:.0}s of {duration:.0}s");
                    ui.invoke_open_path(ui.get_current_path());
                }
                6 => {
                    let duration = f64::from(ui.get_duration()).max(1.0);
                    ui.invoke_seek_fraction(((credits_at + 5.0) / duration) as f32);
                }
                7 => report_credits(&ui, "arrived"),
                // Past `HIDE_AFTER`, with nothing touched since.
                12 => report_credits(&ui, "left alone"),
                // The ring came to the pill on its own, so Enter alone goes on.
                13 => {
                    let enter = slint::SharedString::from(slint::platform::Key::Return);
                    chord(&ui, &[], &enter);
                }
                // The next file, and the ring gone with the pill rather than
                // waiting on a stop that no longer exists.
                15 => report_credits(&ui, "after enter"),
                16 => eprintln!("dbm: --- credits test: holding ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

fn report_credits(ui: &MainWindow, label: &str) {
    let rects: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| format!("{}x{}@{},{}", r.width, r.height, r.x, r.y))
        .collect();
    eprintln!(
        "dbm: credits {label:<11} rolling={} chapter={:?} at-last={} idle={} ring={}@{} t={} {:?} | {} rect(s) {}",
        ui.get_credits_rolling(),
        ui.get_chapter_label(),
        ui.get_at_last(),
        ui.global::<crate::Chrome>().get_idle(),
        ui.global::<crate::Chrome>().get_focus_visible(),
        ui.get_bar_focus(),
        ui.get_elapsed(),
        ui.get_current_path().rsplit(['\\', '/']).next().unwrap_or_default().to_string(),
        rects.len(),
        rects.join(" "),
    );
}

/// Check that a playlist row's length and progress are real.
///
/// The part that cannot be checked by reading the code is whether our idea of
/// where mpv keeps a file's resume position agrees with mpv's. So this plays a
/// while, asks mpv to write that file, and then looks for it by the name we
/// would look for it under — and reports what the rows say before and after.
pub(super) fn progress_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
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
    let rows: Vec<String> = playlist_rows(ui)
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
    let name = crate::library::durations::watch_later_name(&path);
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

/// Raise the action flash from the keyboard and watch it come and go.
///
/// Real key events, because the flash is raised inside the key handlers: an
/// invoked callback would skip the exact statements under test. Each report
/// names the glass rect published for the ring, which is what the flash
/// actually is — the glyph is only what sits in it.
pub(super) fn flash_test(ui: &MainWindow) -> slint::Timer {
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
