//! The window's size, and how the panels divide the room in it.

use std::time::Duration;

use slint::{ComponentHandle, Model};

use super::drive::{click, wheel};
use super::selected_label;
use crate::platform::modal_loop;
use crate::MainWindow;

pub(super) fn resize_test(ui: &MainWindow) -> slint::Timer {
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

/// Open at a size other than the one `main` asks for.
///
/// Every width worth looking at is one the default is not: 1060, where the
/// transport stops being centred rather than crowd the right pill, and the
/// 900×480 floor. Neither can be photographed by hand on a compositor that
/// will not be told where to put a window.
///
/// Repeated rather than fired once, because the size does not always take on
/// the first frame and there is nothing to wait for that says when it has.
/// It stops asking once the window agrees, so nothing fights a real drag.
pub(super) fn window_size(ui: &MainWindow, size: slint::LogicalSize) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut tries = 0u32;
    let mut done = false;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            if done {
                return;
            }
            let Some(ui) = weak.upgrade() else { return };
            let now = ui.window().size().to_logical(ui.window().scale_factor());
            if (now.width - size.width).abs() < 1.0 && (now.height - size.height).abs() < 1.0 {
                done = true;
                eprintln!("dbm: window {}x{}", now.width, now.height);
                return;
            }
            tries += 1;
            if tries > 20 {
                done = true;
                eprintln!(
                    "dbm: window stayed {}x{}, asked for {}x{}",
                    now.width, now.height, size.width, size.height
                );
                return;
            }
            ui.window().set_size(size);
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
pub(super) fn paused_resize_test(ui: &MainWindow) -> slint::Timer {
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
                    ui.window().set_size(slint::LogicalSize::new(1000.0, 900.0));
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

/// Check the height split and that the lists actually move.
///
/// The split has four cases in it — both lists fitting, either one borrowing
/// from the other, and both being capped — so it is worth reading back rather
/// than trusting. The wheel is the other half: a list sized correctly but
/// stuck at the top would look identical from here.
pub(super) fn scroll_test(ui: &MainWindow) -> slint::Timer {
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
                    ui.window().set_size(slint::LogicalSize::new(1280.0, 520.0));
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
                    ui.window().set_size(slint::LogicalSize::new(1280.0, 720.0));
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

/// Open each panel in turn and count what reaches the pipeline.
///
/// The count is the thing to watch rather than the three booleans: the
/// panels all sit in the same corner now, so a second one opening would not
/// just be untidy, it would stack a second sheet of glass on the first.
pub(super) fn panel_test(ui: &MainWindow) -> slint::Timer {
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
                20 => report_panels(&ui, "settings accessibility"),
                21 => ui.invoke_open_settings_page(5),
                22 => report_panels(&ui, "settings shortcuts"),
                23 => ui.invoke_open_settings(false),
                24 => report_panels(&ui, "closed again"),
                25 => eprintln!("dbm: --- panel test done ---"),
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

pub(super) fn modal_test(ui: &MainWindow) -> slint::Timer {
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
