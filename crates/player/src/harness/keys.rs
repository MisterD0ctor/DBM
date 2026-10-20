//! Walking the interface with the keyboard alone.

use std::time::Duration;

use slint::{ComponentHandle, Model};

use super::drive::chord;
use crate::playback::mpv::Mpv;
use crate::{MainWindow, Motion};

/// Walk the keyboard into a panel and down it, with real key events.
///
/// Presses `P` and then Down rather than assigning to the property, which
/// would prove the wash draws and nothing about whether the keys arrive.
/// Twelve rows down because nine fit, and the three past the bottom edge are
/// where the list has to follow the ring.
pub(super) fn reach_test(ui: &MainWindow) -> slint::Timer {
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
            // The accessibility page's switch, pressed with Enter rather than
            // clicked: the keys reach it through `focus-activate-settings`,
            // which a click never touches. Twice, so the run leaves it on.
            if mode == "accessibility" {
                let down = slint::SharedString::from(Key::DownArrow);
                let enter = slint::SharedString::from(Key::Return);
                match step {
                    0 => {
                        eprintln!("dbm: pressing S for the settings");
                        chord(&ui, &[], "s");
                    }
                    // S lands the ring on the first row, so three Downs reach
                    // Accessibility; Enter opens it; Down onto the switch.
                    1..=3 | 5 => {
                        chord(&ui, &[], down.as_str());
                        eprintln!("dbm: down -> row {}", ui.get_focus_row());
                    }
                    4 => {
                        chord(&ui, &[], enter.as_str());
                        eprintln!("dbm: enter, opening the accessibility page");
                    }
                    6 | 7 => {
                        chord(&ui, &[], enter.as_str());
                        eprintln!(
                            "dbm: enter on row {} -> animations {}, surface {}ms",
                            ui.get_focus_row(),
                            ui.global::<Motion>().get_enabled(),
                            ui.global::<Motion>().get_surface()
                        );
                    }
                    8 => eprintln!("dbm: --- reach test done ---"),
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

                // S puts the ring on the first row, Liquid glass, so Enter
                // opens it directly; a Down first would walk into the ambient
                // border's page.
                (true, 1) => {
                    chord(&ui, &[], slint::SharedString::from(Key::Return).as_str());
                    eprintln!("dbm: enter, opening the glass page");
                }
                // Down onto the first slider.
                (true, 2) => {
                    chord(&ui, &[], slint::SharedString::from(Key::DownArrow).as_str());
                    eprintln!("dbm: down -> row {}", ui.get_focus_row());
                }
                // Named and read back, so a walk that lands on the wrong page
                // says so rather than reporting a row number that fits any.
                (true, 3..=10) => {
                    chord(&ui, &[], slint::SharedString::from(Key::LeftArrow).as_str());
                    let row = ui.get_focus_row();
                    let slider = (row as usize)
                        .checked_sub(1)
                        .and_then(|i| ui.get_glass_params().row_data(i))
                        .map(|p| format!("{} {}", p.label, p.readout))
                        .unwrap_or_default();
                    eprintln!("dbm: left on row {row} -> glass {slider}");
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
pub(super) fn key_test(ui: &MainWindow) -> slint::Timer {
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
                // C, twice, on a file whose subtitles aren't showing: the first
                // press must turn them on, not turn off a `sub-visibility` that
                // already shows nothing.
                //
                // Before the dialogs, since a file chooser keeps the keyboard.
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

/// The shortcuts page under the keys that read it.
///
/// Its rows are statements, so the ring has only the way back to stand on,
/// and the keys must scroll the page rather than wrap the ring or reach the
/// film. `time-pos` is the evidence rather than the list, which is built
/// inside the drill's conditional and can't be read from here: the film must
/// not move and the page must not close.
pub(super) fn read_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    use slint::platform::Key;

    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    let mut before = String::new();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(900),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let pos = || mpv.get_property("time-pos").unwrap_or_else(|| "-".into());
            let key = |k: Key| chord(&ui, &[], &slint::SharedString::from(k));
            match step {
                // Straight there, by the key the page itself documents.
                0 => {
                    eprintln!("dbm: pressing ?");
                    chord(&ui, &[], "?");
                }
                1 => {
                    before = pos();
                    // The page number stays private to the interface rather
                    // than made public for a test. `?` opens the panel on the
                    // shortcuts page or not at all.
                    eprintln!(
                        "dbm: settings-open={} time-pos={before}",
                        ui.get_settings_open()
                    );
                    eprintln!("dbm: pressing End");
                    key(Key::End);
                }
                2 => {
                    eprintln!("dbm: pressing Home");
                    key(Key::Home);
                }
                3 => {
                    eprintln!("dbm: pressing PageDown");
                    key(Key::PageDown);
                }
                4 => {
                    eprintln!("dbm: pressing PageUp");
                    key(Key::PageUp);
                }
                5 => {
                    eprintln!("dbm: pressing Down");
                    key(Key::DownArrow);
                }
                6 => {
                    // Playing, so the two differ, but only by about the time
                    // this test took, not back to zero.
                    eprintln!(
                        "dbm: settings-open={} time-pos={} (was {before})",
                        ui.get_settings_open(),
                        pos()
                    );
                    eprintln!("dbm: --- read test done ---");
                }
                _ => {}
            }
            step += 1;
        },
    );
    timer
}
