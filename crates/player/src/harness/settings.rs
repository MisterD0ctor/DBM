//! The settings pages: sliders, switches, resets, and the subtitle steppers.

use std::time::Duration;

use slint::{ComponentHandle, Model};

use crate::playback::mpv::Mpv;
use crate::{MainWindow, Motion};

/// Open the settings panel and drive one parameter to its maximum, so the
/// probe frame lands with a value nothing else would have produced.
pub(super) fn param_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(600),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.set_settings_open(true);
            // The glass tint amount, found by key: this was a bare index once,
            // and it went on setting whatever the registry put there after
            // three rows left the glass page. Tint runs 0..1, so the value
            // from the variable is also the slider's fraction. A non-numeric
            // value opens the panel without touching anything, which is how a
            // loaded-from-disk value can be probed.
            let tint_row = crate::settings::REGISTRY
                .iter()
                .position(|p| p.key == crate::settings::Key::Tint)
                .expect("the registry has a tint row");
            match std::env::var("DBM_PARAM_TEST")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                Some(tint) => {
                    ui.invoke_set_param(tint_row as i32, tint);
                    eprintln!("dbm: settings opened, tint set to {tint}");
                }
                None => eprintln!("dbm: settings opened, values untouched"),
            }
        },
    );
    timer
}

/// What mpv holds against what the panel is showing.
///
/// Both, because either alone would miss half of what can go wrong: mpv
/// alone would not catch a readout that never updates, and the readout alone
/// would not catch a value that never left the process.
fn report_subs(ui: &MainWindow, mpv: &Mpv, label: &str) {
    // Blocking reads, acceptable only because this is a one-shot diagnostic
    // step rather than the frame path.
    // The two positions agree only while nothing lifts the line: the panel
    // shows where the person put it, mpv holds where the bar lets it sit. The
    // subtitles page asks for no lift, so on it they should match.
    eprintln!(
        "dbm: {label:<20} mpv delay={:?} scale={:?} pos={:?} | panel {} {} {} \
         | ceiling {:.1} lift {:.2}",
        mpv.get_property("sub-delay"),
        mpv.get_property("sub-scale"),
        mpv.get_property("sub-pos"),
        ui.get_sub_delay_text(),
        ui.get_sub_scale_text(),
        ui.get_sub_pos_text(),
        ui.get_subtitle_ceiling(),
        ui.get_subtitle_lift(),
    );
}

/// Step each subtitle adjustment, push one past its limit, then reset.
///
/// The clamp is the part worth driving: it lives in `commands` so that the
/// value recorded for saving is the one mpv was actually given, and a drift
/// between those two would only show up as a setting that comes back wrong
/// on the next run.
pub(super) fn subs_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
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

/// Every motion token, as the interface reads it.
fn report_motion(ui: &MainWindow, label: &str) {
    let motion = ui.global::<Motion>();
    eprintln!(
        "dbm: {label:<18} highlight={}ms track-grow={}ms surface={}ms switch={}ms \
         title-width={}ms drill={}ms",
        motion.get_highlight(),
        motion.get_track_grow(),
        motion.get_surface(),
        motion.get_switch(),
        motion.get_title_width(),
        motion.get_drill(),
    );
}

/// Walk the effect switches, autoplay and the animations switch, leaving each
/// one back where it started so a run does not quietly rewrite the saved
/// settings.
///
/// The interesting part is not the property afterwards — that only says the
/// click landed — but what the probe reads out of the framebuffer between the
/// steps, and what mpv says `keep-open` became.
pub(super) fn toggle_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
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
                // Animations, the way the switch turns them off: `Motion`
                // first, then the callback that saves it. What to read is the
                // tokens every `animate` in the interface takes its duration
                // from — all of them zero with the switch off, and back to
                // their lengths with it on.
                7 => {
                    ui.global::<Motion>().set_enabled(false);
                    ui.invoke_set_animations(false);
                    report_motion(&ui, "animations off");
                }
                8 => {
                    ui.global::<Motion>().set_enabled(true);
                    ui.invoke_set_animations(true);
                    report_motion(&ui, "animations on");
                }
                // Reset. Drive one slider somewhere it would never be by
                // default, then put the section back and read the row again.
                // The row is the thing to check: the value reaching the
                // pipeline is already covered by `DBM_PARAM_TEST`, and what
                // broke before was the model, which lost the drag when it
                // was rebuilt instead of updated in place.
                //
                // Tint found by key, as `param_test` finds it. This was the
                // bare index 7, which stopped being tint when three rows left
                // the glass page and has driven the ambient border's edge blur
                // ever since — a row this report never prints.
                9 => {
                    let tint = crate::settings::REGISTRY
                        .iter()
                        .position(|p| p.key == crate::settings::Key::Tint)
                        .expect("the registry has a tint row");
                    ui.invoke_set_param(tint as i32, 1.0);
                    eprintln!("dbm: tint driven to max");
                }
                10 => report_row(&ui, "after tint max"),
                11 => {
                    ui.invoke_reset_section(0);
                    eprintln!("dbm: glass section reset");
                }
                12 => report_row(&ui, "after reset"),
                13 => eprintln!("dbm: --- toggle test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}
