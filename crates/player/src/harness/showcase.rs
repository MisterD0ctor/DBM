//! Holding one surface on screen, so the window can be photographed.

use std::time::Duration;

use slint::{ComponentHandle, Model};

use super::drive::{arrow, click, hover, wheel};
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
                // The end pill, which sits in the middle of the window, under
                // the pointer: its highlight against its glass.
                "end-hover" => {
                    let size = ui.window().size().to_logical(ui.window().scale_factor());
                    (size.width / 2.0, size.height / 2.0)
                }
                // The angle at the top of a list turned away from the file
                // playing, for the circle that comes up under the pointer.
                "current-hover" => (
                    (ui.get_panel_left() + ui.get_panel_right()) / 2.0,
                    ui.get_playlist_list_top() + 17.0,
                ),
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

            // Reaching the end puts the keyboard's ring on the pill; `end`
            // takes it off again, for the pill as it is with nothing on it.
            if surface == "end" && tick > 6 {
                ui.global::<crate::Chrome>().set_focus_visible(false);
            }

            // The flash is a moment, not a surface: the only way to hold one
            // still long enough to look at is to keep raising it.
            if surface == "flash" {
                ui.global::<crate::Flash>()
                    .invoke_show(ui.global::<crate::Flash>().get_delay(), 0.5);
            }

            // The playlist turned away from the file playing, which is the
            // only time the angle pointing back to it is drawn: by a whole
            // list either way, or by enough to leave that row's lower edge
            // halfway into the 28px soft edge at the top — half a list, and
            // the difference between a 34px row and the edge, halved — which
            // is halfway through the angle's fade. The file has to be
            // mid-season for the list to open centred on it.
            if tick == 6 {
                let list = ui.get_playlist_list_h();
                let by = match surface.as_str() {
                    "current-above" | "current-back" | "current-hover" => -list,
                    "current-below" => list,
                    "current-half" => -(list / 2.0 + (34.0 - 28.0) / 2.0),
                    _ => 0.0,
                };
                if by != 0.0 {
                    let (x, y) = (
                        ui.get_panel_right() - 150.0,
                        ui.get_playlist_list_top() + 40.0,
                    );
                    wheel(&ui, x, y, by);
                }
            }
            // And the angle pressed, which should bring the row back to the
            // middle — and play nothing: a press that reached the row under
            // it would start another episode, and the title would say so.
            if surface == "current-back" && tick == 10 {
                let x = (ui.get_panel_left() + ui.get_panel_right()) / 2.0;
                let y = ui.get_playlist_list_top() + 17.0;
                click(&ui, x, y);
                // The glide, sampled: where the list is every 20ms for the
                // length of it and a little after.
                for i in 0..=20u64 {
                    let weak = ui.as_weak();
                    slint::Timer::single_shot(Duration::from_millis(i * 20), move || {
                        if let Some(ui) = weak.upgrade() {
                            eprintln!(
                                "dbm: glide {:>3}ms offset {:.1}",
                                i * 20,
                                ui.get_playlist_scroll()
                            );
                        }
                    });
                }
            }

            // The language page walked with the keyboard to its last row,
            // which on a small window is past the bottom of what the panel
            // shows: the list has to follow the ring down.
            if surface == "language-walk" && (6..=14).contains(&tick) {
                arrow(&ui, slint::platform::Key::DownArrow);
            }

            // Autoplay into a file with a place saved in it: the file before
            // it is sent to its last seconds and left to play out. The next
            // one should start from the top with the place offered in the
            // corner for ten seconds; `autoplay-take` takes the offer. Seed
            // the next file's watch-later entry first. Prints what happens.
            if surface.starts_with("autoplay") {
                if tick == 4 {
                    ui.invoke_seek_fraction(0.995);
                }
                if surface == "autoplay-take" && tick == 26 {
                    ui.invoke_take_resume_offer();
                }
                if tick.is_multiple_of(2) && (14..=60).contains(&tick) {
                    eprintln!(
                        "dbm: autoplay t{tick:<2} {} at {} offer={:?} corner={:?}",
                        ui.get_media_title(),
                        ui.get_elapsed(),
                        ui.get_resume_offer(),
                        ui.get_credits_label(),
                    );
                }
            }

            // Arriving at a file with a place saved in it, by the Next
            // button: held there, and then answered from the beginning. Seed
            // the next file's watch-later entry first. Prints what happens.
            if surface.starts_with("arrival") {
                if tick == 6 && surface != "arrival" {
                    ui.invoke_playlist_step(1);
                }
                if tick == 12 && surface == "arrival-answer" {
                    ui.invoke_arrival_answer(true);
                }
                if tick.is_multiple_of(2) && (6..=16).contains(&tick) {
                    eprintln!(
                        "dbm: arrival t{tick:<2} arriving={} from={:?} paused={} at {}",
                        ui.get_arriving(),
                        ui.get_arrival_from(),
                        ui.get_paused(),
                        ui.get_elapsed(),
                    );
                }
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
                "playlist" | "drill" | "scroll-bar" | "current-above" | "current-below"
                | "current-half" | "current-back" | "current-hover" => {
                    ui.invoke_open_playlist(true)
                }
                // The list of parts rather than the page the panel opens on.
                "seasons" => {
                    ui.invoke_open_playlist(true);
                    ui.set_playlist_page(-1);
                }
                "files" => ui.invoke_open_files(true),
                // Run to the end, with autoplay off, so the pill stays; `end`
                // with the pointer away from it, to subtract.
                "end" | "end-hover" => ui.invoke_seek_fraction(1.0),
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
                "language" | "language-walk" => ui.invoke_open_settings_page(6),
                _ => {}
            }
            eprintln!("dbm: showcase ready ({surface})");
        },
    );
    timer
}
