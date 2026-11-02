//! The window's size, and how the panels divide the room in it.

use std::time::Duration;

use slint::{ComponentHandle, Model};

use super::drive::{click, drag, wheel};
use super::script::{self, Beat, Script, sleep};
use super::selected_label;
use crate::platform::modal_loop;
use crate::{MainWindow, Playback, Settings, Tracks};

pub(super) fn resize_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(8));
        for tick in 1..=401u32 {
            beat.next().await;
            // Stop after a while and hold the last size, so the settling path
            // is exercised too.
            if tick > 400 {
                continue;
            }
            // Pause partway in, since resizing while paused is where a
            // reallocated target that wasn't re-rendered would show black.
            if tick == 60 {
                ui.global::<Playback>().invoke_toggle_pause();
                eprintln!("dbm: paused mid-resize-sweep");
            }
            // A triangle wave in big steps, width and height in turn. Width
            // exercises the target allocation; height moves the letterbox,
            // where a video rect a frame behind points above the picture and
            // darkens the border. Big steps, because a rect one frame behind
            // is wrong by however much changed in that frame, and a dragged
            // corner moves tens of pixels a frame.
            let phase = tick % 40;
            let (w, h) = if phase < 20 {
                let d = if phase < 10 { phase } else { 20 - phase };
                (960 + d * 45, 700)
            } else {
                let p = phase - 20;
                let d = if p < 10 { p } else { 20 - p };
                (1200, 560 + d * 45)
            };
            ui.window().set_size(slint::PhysicalSize::new(w, h));
            // How far the settings panel, if open, trails its size, read a
            // frame or so after it was set, where an eased resize would trail
            // by tens of pixels.
            if tick % 10 == 5 {
                eprintln!("dbm: resize lag {:.1}px", ui.get_resize_lag());
            }
        }
    })
}

/// Open at a size other than the one `main` asks for.
///
/// For the sizes worth looking at besides the default (1060 wide, where the
/// transport stops being centred, and the 960×540 floor), which a compositor
/// that places windows itself won't let you set by hand.
///
/// Repeated rather than fired once, because the size doesn't always take on
/// the first frame and nothing says when it has. It stops once the window
/// agrees, so nothing fights a real drag.
pub(super) fn window_size(ui: &MainWindow, size: slint::LogicalSize) -> Script {
    script::run(ui, move |ui| async move {
        let window = ui.window();
        for tries in 0.. {
            sleep(Duration::from_millis(100)).await;
            let now = window.size().to_logical(window.scale_factor());
            if (now.width - size.width).abs() < 1.0 && (now.height - size.height).abs() < 1.0 {
                eprintln!("dbm: window {}x{}", now.width, now.height);
                return;
            }
            if tries == 20 {
                eprintln!(
                    "dbm: window stayed {}x{}, asked for {}x{}",
                    now.width, now.height, size.width, size.height
                );
                return;
            }
            window.set_size(size);
        }
    })
}

/// Pause, resize, and then stop touching it.
///
/// The moment that matters is after the resize. mpv reports the new video
/// rect a frame or two late; playing, the next frame redraws the border from
/// it, but paused there is no next frame, so the rect itself has to trigger
/// the redraw.
pub(super) fn paused_resize_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1200));
        let settings = ui.global::<Settings>();
        beat.next().await;
        ui.global::<Playback>().invoke_toggle_pause();
        eprintln!("dbm: paused");
        beat.rest(2).await;
        // Twice the height, so the letterbox bars are a different
        // size afterwards and a stale rect cannot pass for a fresh
        // one.
        eprintln!("dbm: --- resizing while paused ---");
        ui.window().set_size(slint::LogicalSize::new(1000.0, 900.0));
        beat.rest(2).await;
        eprintln!("dbm: --- settled; the border must match ---");
        beat.next().await;
        // Flipping the switch makes the probe take a fresh pixel
        // reading of the letterbox with the new geometry.
        settings.set_ambience_on(false);
        settings.invoke_set_ambience(false);
        beat.next().await;
        settings.set_ambience_on(true);
        settings.invoke_set_ambience(true);
        beat.rest(2).await;
        eprintln!("dbm: --- paused resize test done ---");
    })
}

/// A tooltip, then another long after it went, then one just after. The
/// second should wait `Motion.tip-delay` as the first did, and only the third
/// come up at once, the bar being read. Reports the tip's glass a tenth of a
/// second apart from each arrival.
pub(super) fn tip_test(ui: &MainWindow) -> Script {
    // The settings and fullscreen buttons at the default size, and the
    // picture clear of the bar.
    let (gear, fullscreen, film) = ((1132.0, 664.0), (1224.0, 664.0), (640.0, 300.0));
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(100));
        for tick in 1..=61u32 {
            beat.next().await;
            let (at, label) = match tick {
                10 => (Some(film), ""),
                11 => (Some(gear), "first"),
                25 | 55 => (Some(film), ""),
                40 => (Some(fullscreen), "long after"),
                57 => (Some(gear), "just after"),
                _ => (None, ""),
            };
            if let Some((x, y)) = at {
                super::drive::hover(&ui, x, y);
                if !label.is_empty() {
                    eprintln!("dbm: tip onto a button, {label}");
                }
            }
            if [(11..=18), (40..=47), (57..=61)]
                .iter()
                .any(|r| r.contains(&tick))
            {
                let tip = ui.get_glass_rects().row_data(6).unwrap_or_default();
                eprintln!(
                    "dbm: tip t{tick} a={:.2}",
                    if tip.width > 0.0 { tip.opacity } else { 0.0 }
                );
            }
        }
    })
}

/// Opens one panel six seconds in, with nothing having asked for it before,
/// then closes and opens it again every two seconds, `DBM_OPEN_CYCLES` times
/// in all (twice if unset), saying when, for `DBM_FRAME_LOG` to show what the
/// first opening costs against the rest. The pointer moves over the picture
/// half a second before each, as a hand reaching for the button would, so
/// the bar is already up and only the panel arrives.
pub(super) fn first_open_test(ui: &MainWindow, panel: String) -> Script {
    let cycles: u32 = std::env::var("DBM_OPEN_CYCLES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(2);
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(250));
        // Until the window closes.
        let mut tick = 0u32;
        loop {
            beat.next().await;
            tick += 1;
            // Two moves well apart, which the twitch filter lets through.
            if tick >= 22 && tick % 4 == 2 {
                super::drive::hover(&ui, 560.0, 300.0);
                super::drive::hover(&ui, 640.0, 340.0);
            }
            let Some(step) = tick.checked_sub(24).filter(|t| t % 4 == 0) else {
                continue;
            };
            if step / 8 >= cycles {
                continue;
            }
            let open = step % 8 == 0;
            eprintln!(
                "dbm: --- {} {panel} ---",
                if open { "opening" } else { "closing" }
            );
            match panel.as_str() {
                "playlist" => ui.invoke_open_playlist(open),
                "menu" => ui.invoke_open_menu(open),
                "audio" => ui.invoke_open_audio(open),
                "settings" => ui.invoke_open_settings(open),
                "files" => ui.invoke_open_files(open),
                _ => {}
            }
        }
    })
}

/// How tall each tracks panel came out, against what its list wanted.
fn report_lists(ui: &MainWindow, label: &str) {
    eprintln!(
        "dbm: {label:<18} subs panel={:.0} list {:.0}/{:.0} | audio panel={:.0} list {:.0}/{:.0}",
        ui.get_subs_h(),
        ui.get_subs_list_h(),
        ui.get_subs_natural(),
        ui.get_audio_h(),
        ui.get_audio_list_h(),
        ui.get_audio_natural(),
    );
}

/// Check the panels' heights against their caps, and that the lists move.
///
/// A list sized correctly but stuck at the top looks the same as one that
/// works, so the wheel is turned over each that can scroll.
pub(super) fn scroll_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1000));
        beat.next().await;
        // Short enough that a long list runs into the panel's cap.
        ui.window().set_size(slint::LogicalSize::new(1280.0, 540.0));
        ui.invoke_open_menu(true);
        beat.next().await;
        report_lists(&ui, "tracks, short");
        beat.next().await;
        // Into the subtitle list: below the panel top, the
        // padding and the heading.
        let x = ui.window().size().width as f32 / ui.window().scale_factor() - 100.0;
        let y = ui.get_timeline_y() - ui.get_subs_h() + 60.0;
        eprintln!("dbm: wheel over the subtitle list at ({x:.0}, {y:.0})");
        wheel(&ui, x, y, -120.0);
        beat.next().await;
        eprintln!("dbm: subs scrolled to {:.0}", ui.get_subs_scroll());
        beat.next().await;
        ui.window().set_size(slint::LogicalSize::new(1280.0, 720.0));
        ui.invoke_open_playlist(true);
        beat.next().await;
        eprintln!(
            "dbm: playlist panel={:.0} list {:.0}/{:.0}",
            ui.get_playlist_h(),
            ui.get_playlist_list_h(),
            ui.get_playlist_natural(),
        );
        beat.next().await;
        let x = ui.get_playlist_anchor();
        let y = ui.get_timeline_y() - ui.get_playlist_h() + 60.0;
        eprintln!("dbm: wheel over the playlist at ({x:.0}, {y:.0})");
        wheel(&ui, x, y, -240.0);
        beat.next().await;
        eprintln!("dbm: playlist scrolled to {:.0}", ui.get_playlist_scroll());
        beat.next().await;
        // The scroll bar, which is a control and not only a hint. The
        // wheel has left the list at its end, so a press on the rail
        // near the head of it should bring the thumb, and the list,
        // back to the start.
        let (x, top) = playlist_bar(&ui);
        eprintln!("dbm: pressing the rail at ({x:.0}, {:.0})", top + 8.0);
        click(&ui, x, top + 8.0);
        beat.next().await;
        eprintln!(
            "dbm: rail pressed at the head: {:.0}",
            ui.get_playlist_scroll()
        );
        beat.next().await;
        // Then the thumb, taken where it is and moved down 30px: the
        // list should go as far as that is of the thumb's run.
        let (x, top) = playlist_bar(&ui);
        drag(&ui, (x, top + 8.0), (x, top + 38.0));
        beat.next().await;
        let (shown, held) = (ui.get_playlist_list_h(), ui.get_playlist_natural());
        let rail = shown - 4.0;
        let run = rail - (rail * shown / held).max(24.0);
        eprintln!(
            "dbm: thumb dragged down 30: {:.0}, expected {:.0}",
            ui.get_playlist_scroll(),
            (shown - held) * 30.0 / run,
        );
        beat.next().await;
        let (x, top) = playlist_bar(&ui);
        let shown = ui.get_playlist_list_h();
        drag(&ui, (x, top + 38.0), (x, top + 38.0 + shown));
        beat.next().await;
        eprintln!(
            "dbm: thumb dragged past the foot: {:.0}, the end is {:.0}",
            ui.get_playlist_scroll(),
            ui.get_playlist_list_h() - ui.get_playlist_natural(),
        );
        beat.next().await;
        // Back to the subtitles menu and click the first real
        // subtitle row: the one below "Off", inside the scrolling list.
        ui.invoke_open_menu(true);
        beat.next().await;
        let top = ui.get_timeline_y() - ui.get_subs_h();
        let y = top + 14.0 + 28.0 + 34.0 + 17.0;
        let x = ui.get_subs_anchor();
        eprintln!("dbm: clicking a subtitle row at ({x:.0}, {y:.0})");
        click(&ui, x, y);
        beat.next().await;
        eprintln!(
            "dbm: subtitle now {:?}",
            selected_label(&ui.global::<Tracks>().get_subtitles())
        );
        beat.next().await;
        eprintln!("dbm: --- scroll test done ---");
    })
}

/// Where the playlist's scroll bar is: its centre line, which runs 5.5px in
/// from the panel's right edge, and the top of the list it belongs to.
fn playlist_bar(ui: &MainWindow) -> (f32, f32) {
    (ui.get_panel_right() - 5.5, ui.get_playlist_list_top())
}

/// Open each panel in turn and count what reaches the pipeline.
///
/// The count is what to watch rather than the booleans: the panels share a
/// corner, so two open at once would stack glass on glass.
pub(super) fn panel_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(900));
        beat.next().await;
        report_panels(&ui, "nothing open");
        beat.next().await;
        ui.invoke_open_menu(true);
        beat.next().await;
        report_panels(&ui, "subtitles");
        beat.next().await;
        ui.invoke_open_audio(true);
        beat.next().await;
        report_panels(&ui, "audio over subtitles");
        beat.next().await;
        ui.invoke_open_playlist(true);
        beat.next().await;
        report_panels(&ui, "playlist over audio");
        beat.next().await;
        ui.invoke_open_settings(true);
        beat.next().await;
        report_panels(&ui, "settings over playlist");
        beat.next().await;
        ui.invoke_open_files(true);
        beat.next().await;
        report_panels(&ui, "files over settings");
        beat.next().await;
        ui.invoke_open_menu(true);
        beat.next().await;
        report_panels(&ui, "subtitles over files");
        beat.next().await;
        // Each settings page sizes the panel from its own row
        // count, so walk them and see what the pipeline is told.
        ui.invoke_open_settings_page(0);
        beat.next().await;
        report_panels(&ui, "settings list");
        beat.next().await;
        ui.invoke_open_settings_page(1);
        beat.next().await;
        report_panels(&ui, "settings glass");
        beat.next().await;
        ui.invoke_open_settings_page(2);
        beat.next().await;
        report_panels(&ui, "settings ambience");
        beat.next().await;
        ui.invoke_open_settings_page(3);
        beat.next().await;
        report_panels(&ui, "settings accessibility");
        beat.next().await;
        ui.invoke_open_settings_page(4);
        beat.next().await;
        report_panels(&ui, "settings shortcuts");
        beat.next().await;
        ui.invoke_open_settings_page(5);
        beat.next().await;
        report_panels(&ui, "settings language");
        beat.next().await;
        ui.invoke_open_settings(false);
        beat.next().await;
        report_panels(&ui, "closed again");
        beat.next().await;
        eprintln!("dbm: --- panel test done ---");
    })
}

/// Which panels believe they are open, and how many rects that produces. A
/// closed panel publishes a zero-sized rect, which the Rust side drops.
fn report_panels(ui: &MainWindow, label: &str) {
    let drawn: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| format!("x{}..{} h{}", r.x, r.x + r.width, r.height))
        .collect();
    eprintln!(
        "dbm: {label:<22} subtitles={} audio={} playlist={} settings={} files={} | {} rect(s) {}",
        ui.get_menu_open(),
        ui.get_audio_open(),
        ui.get_playlist_open(),
        ui.get_settings_open(),
        ui.get_files_open(),
        drawn.len(),
        drawn.join(" "),
    );
}

pub(super) fn modal_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        sleep(Duration::from_millis(3000)).await;
        eprintln!("dbm: --- entering modal size loop ---");
        modal_loop::enter_modal_size_loop_for_test(ui.window());
    })
}
