//! Walking the interface with the keyboard alone.

use std::time::Duration;

use slint::ComponentHandle;

use super::drive::chord;
use crate::playback::mpv::Mpv;
use crate::MainWindow;

/// Walk the keyboard into a panel and down it, with real key events.
///
/// The ring, the wrap and the scroll-to-follow are all driven from the same
/// dispatch a person's keyboard reaches, so this presses `P` and then Down
/// rather than assigning to the property: assigning would prove the wash
/// draws and nothing about whether the keys arrive.
///
/// Twelve rows down because nine fit: the interesting part is the three past
/// the bottom edge, where the list has to move for the ring to stay visible.
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

/// The shortcuts page under the keys that read it.
///
/// Every row on that page is a statement, so the ring has only the way back
/// to stand on and the arrows have nowhere to go. That was the bug: the ring
/// wrapped on its one row and swallowed them, and Home, PgUp and PgDn went
/// past it into the film — a reference page that restarted the film when you
/// asked to go to the top of it.
///
/// `time-pos` is the evidence, not the list: the list is built inside the
/// conditional the drill slides, so nothing out here can read where it
/// scrolled to. What can be proved is that the film did not move and the
/// page did not close, which is the whole of what went wrong.
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
                    // The page number stays private to the interface —
                    // exposing it to be read here would make this test the
                    // reason a property is public, which is how UI-as-storage
                    // starts. `?` opens the panel on page 4 or not at all.
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
                    // Playing, so the two will differ — the check is that
                    // they differ by about the time this test took, rather
                    // than by everything back to zero.
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
