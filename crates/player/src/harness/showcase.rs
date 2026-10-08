//! Holding one surface on screen, so the window can be photographed.

use std::time::Duration;

use slint::{ComponentHandle, Model};

use super::drive::hover;
use crate::MainWindow;

/// Hold one surface open, with the chrome kept awake, for a photograph.
///
/// The interface hides itself after three seconds of stillness and half of it
/// only exists while a pointer is somewhere particular, so a screenshot of the
/// running player is otherwise a picture of a video with nothing on it.
pub(super) fn showcase(ui: &MainWindow, surface: String) -> slint::Timer {
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
                // The playlist's scroll bar, which thickens under a pointer.
                "scroll-bar" => (
                    ui.get_panel_right() - 5.5,
                    ui.get_playlist_list_top() + 60.0,
                ),
                // The volume track, whose label is a reading, not a name.
                "tip-volume" => (ui.get_vol_cx(), ui.get_vol_cy()),
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

            // A move between two pages is a moment too, and the same answer
            // works: keep starting it. Flipping every tick against a 220ms
            // drill leaves the panel in the middle of one for most of the
            // time, so a capture at any moment lands inside the movement
            // rather than on whichever end it happened to settle at.
            if surface == "drill" && tick > 4 {
                let page = ui.get_playlist_page();
                ui.set_playlist_page(if page < 0 {
                    ui.get_playlist_current_group().max(0)
                } else {
                    -1
                });
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
                "playlist" | "drill" | "scroll-bar" => ui.invoke_open_playlist(true),
                // The list of parts rather than the page the panel opens on.
                "seasons" => {
                    ui.invoke_open_playlist(true);
                    ui.set_playlist_page(-1);
                }
                "files" => ui.invoke_open_files(true),
                // The three graphics failures all need a driver that does
                // not work, which is not something a test can arrange. The
                // wording is the part worth looking at anyway: it is the only
                // account of the failure that ever reaches anyone, since the
                // console it used to print to does not exist in a packaged
                // build.
                // The same pane, having been asked to copy itself, so the
                // clipboard can be read back from outside.
                "fatal-copy" => {
                    ui.set_fatal("The player cannot show video on this computer.".into());
                    ui.set_fatal_detail(
                        "The graphics driver would not build the player's shaders.".into(),
                    );
                    ui.set_fatal_note(
                        "If the driver is current, this is a bug in the player.".into(),
                    );
                    ui.invoke_copy_details();
                }
                // A caption-only flash — the speed, as `]` raises it.
                "figure" => {
                    ui.global::<crate::Flash>().invoke_show_figure(1);
                }
                "fatal" => {
                    ui.set_fatal("The player cannot show video on this computer.".into());
                    ui.set_fatal_detail(
                        "The graphics driver would not build the player's shaders. \
                         compiling glass.frag: 0(213) : error C1503: undefined \
                         variable \"backdrop\""
                            .into(),
                    );
                    ui.set_fatal_note(
                        "If the driver is current, this is a bug in the player.".into(),
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
                "accessibility" => ui.invoke_open_settings_page(4),
                "shortcuts" => ui.invoke_open_settings_page(5),
                _ => {}
            }
            eprintln!("dbm: showcase ready ({surface})");
        },
    );
    timer
}
