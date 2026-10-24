//! Holding one surface on screen, so the window can be captured.

use std::time::Duration;

use slint::platform::Key;
use slint::{ComponentHandle, Model};

use super::drive::{arrow, click, hover, wheel};
use crate::MainWindow;

/// Hold one surface open, with the chrome kept awake, for a capture: the
/// interface hides after three still seconds, and half of it only exists
/// while the pointer is somewhere particular.
pub(super) fn showcase(ui: &MainWindow, surface: String) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut tick = 0u32;
    let mut armed = false;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(250),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            tick += 1;

            // Any pointer event resets the idle clock. Most surfaces keep the
            // pointer clear of the controls; the preview needs it on the
            // track, and a tip needs it resting on a button. The coordinates
            // are the right group's buttons at the default size, aimed for a
            // capture rather than geometry anything depends on.
            let (x, y) = match surface.as_str() {
                "preview" => (
                    ui.get_scrub_x() + ui.get_scrub_w() * 0.62,
                    ui.get_timeline_y() + 14.0,
                ),
                // The middle of the track, whose preview reaches the end pill
                // in a short window.
                "end-preview" => (
                    ui.get_scrub_x() + ui.get_scrub_w() * 0.5,
                    ui.get_timeline_y() + 14.0,
                ),
                // The settings gear.
                "tip" | "tip-idle" | "tip-open" => (1132.0, 664.0),
                // The last button there is, to see the clamp hold a tip
                // inside the margin rather than off the window.
                "tip-edge" => (1224.0, 664.0),
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

            // A move between two pages is held the same way, by restarting
            // it: flipping every tick against a 220ms drill keeps the panel
            // mid-move most of the time.
            if surface == "drill" && tick > 4 {
                let page = ui.get_playlist_page();
                ui.set_playlist_page(if page < 0 {
                    ui.get_playlist_current_group().max(0)
                } else {
                    -1
                });
            }

            // The pill moving aside for a panel opened over it.
            if tick == 8 {
                match surface.as_str() {
                    "end-playlist" => ui.invoke_open_playlist(true),
                    "end-settings" => ui.invoke_open_settings_page(0),
                    _ => {}
                }
            }

            // Reaching the end puts the keyboard's ring on the pill; `end`
            // takes it off again, for the pill as it is with nothing on it.
            if surface == "end" && tick > 6 {
                ui.global::<crate::Chrome>().set_focus_visible(false);
            }

            // The flash is a moment, not a surface, so it is held by raising
            // it again and again.
            if surface == "flash" {
                ui.global::<crate::Flash>()
                    .invoke_show(ui.global::<crate::Flash>().get_delay(), 0.5);
            }

            // The playlist turned away from the file playing, so the angle
            // back to it is drawn: by a whole list either way, or just far
            // enough to leave the row's lower edge halfway into the 28px soft
            // edge at the top, which is halfway through the angle's fade. The
            // file has to be mid-season for the list to open centred on it.
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
            // middle and play nothing; a press that reached the row under it
            // would start another episode, and the title would say so.
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

            // The window's maximize button pressed, for the window at the
            // monitor's size and the button's glyph turned to restore.
            if surface == "window-max" && tick == 6 {
                click(&ui, 1219.0, 27.0);
            }

            // The language page walked with the keyboard to its last row,
            // which on a small window is past the bottom of what the panel
            // shows: the list has to follow the ring down.
            if surface == "language-walk" && (6..=14).contains(&tick) {
                arrow(&ui, slint::platform::Key::DownArrow);
            }

            // The keyboard's ring held on the bar's settings button, for the
            // disc of glass it stands on: one Tab puts it on the first button
            // and ten more walk it there.
            if surface == "ring-bar" && (4..=14).contains(&tick) {
                arrow(&ui, Key::Tab);
            }
            // And on a row: the playlist's fourth, and the glass page's third
            // slider, whose name sits in the measured column.
            if (surface == "ring-row" || surface == "ring-slider") && (6..=9).contains(&tick) {
                arrow(&ui, Key::DownArrow);
            }

            // A track loaded from its own file, pressed once to remove it:
            // the ring is walked down to the first such subtitle and Delete
            // pressed, and never again, so the row stays armed. Open a film
            // with a downloaded subtitle.
            if surface == "armed" && tick >= 6 && !armed {
                let row = ui.get_focus_row();
                let external = (row as usize)
                    .checked_sub(1)
                    .and_then(|i| ui.get_sub_tracks().row_data(i))
                    .is_some_and(|t| t.external);
                if ui.global::<crate::Chrome>().get_focus_visible() && external {
                    arrow(&ui, Key::Delete);
                    armed = true;
                    eprintln!("dbm: armed row {row}");
                } else {
                    arrow(&ui, Key::DownArrow);
                }
            }

            // Left and right inside the panels: into a settings page and back
            // out, nothing past the front page, nothing on a file's row, back
            // to the list of parts and into a part again. Open a film in the
            // middle of a season with seasons beside it; prints what each key
            // left behind, and whether the film moved.
            if surface == "sideways" {
                match tick {
                    4 => ui.invoke_open_settings_page(0),
                    5 | 15 | 16 | 20 => arrow(&ui, Key::DownArrow),
                    6 | 12 | 18 | 22 => arrow(&ui, Key::RightArrow),
                    8 | 10 | 19 => arrow(&ui, Key::LeftArrow),
                    14 => {
                        ui.invoke_open_settings(false);
                        ui.invoke_open_playlist(true);
                    }
                    _ => {}
                }
                if (5..=23).contains(&tick) {
                    eprintln!(
                        "dbm: sideways t{tick:<2} settings={}:{} playlist={}:{} row={} at {} {}",
                        ui.get_settings_open(),
                        ui.get_settings_page(),
                        ui.get_playlist_open(),
                        ui.get_playlist_page(),
                        ui.get_focus_row(),
                        ui.get_elapsed(),
                        ui.get_media_title(),
                    );
                }
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
                "audio" => ui.invoke_open_audio(true),
                // The menu with a subtitle track chosen, so exactly one row
                // is lit.
                "subs-on" => {
                    ui.invoke_open_menu(true);
                    if let Some(first) = ui.get_sub_tracks().row_data(0) {
                        ui.invoke_select_subtitle(first.id);
                    }
                }
                // A track chosen and then turned off, where mpv still calls
                // the track selected but only Off should be lit. Choosing
                // first matters, since a file whose subtitles were never on
                // has nothing selected to contradict Off.
                "subs-off" => {
                    ui.invoke_open_menu(true);
                    if let Some(first) = ui.get_sub_tracks().row_data(0) {
                        ui.invoke_select_subtitle(first.id);
                    }
                    ui.invoke_select_subtitle(-1);
                }
                "playlist" | "drill" | "scroll-bar" | "current-above" | "current-below"
                | "current-half" | "current-back" | "current-hover" | "ring-row" => {
                    ui.invoke_open_playlist(true)
                }
                "armed" => ui.invoke_open_menu(true),
                "ring-slider" => ui.invoke_open_settings_page(1),
                // The list of parts rather than the page the panel opens on.
                "seasons" => {
                    ui.invoke_open_playlist(true);
                    ui.set_playlist_page(-1);
                }
                "files" => ui.invoke_open_files(true),
                // Run to the end, with autoplay off, so the pill stays; `end`
                // with the pointer away from it, to subtract; `end-ring` with
                // the keyboard's ring left on it; the last three with
                // something in its way.
                "end" | "end-hover" | "end-ring" | "end-playlist" | "end-settings"
                | "end-preview" => ui.invoke_seek_fraction(1.0),
                // The fatal pane, having been asked to copy itself, so the
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
                // The graphics failures need a driver that doesn't work, which
                // a test can't arrange, so the pane is set by hand. Its wording
                // is what matters, as the only account of the failure anyone
                // sees.
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
                // Held rather than provoked, since a local open clears it
                // within a few frames; the slow case needs a network share.
                "opening" => {
                    ui.set_opening_name("Game of Thrones Season 7".into());
                    ui.set_opening(true);
                }
                // A panel open under the pointer resting on the button that
                // opened it. Tips stand down for any panel, so the right
                // capture shows no tip.
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
