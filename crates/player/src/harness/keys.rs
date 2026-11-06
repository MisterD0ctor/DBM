//! Walking the interface with the keyboard alone.

use std::time::Duration;

use dbm_mpv::Reader;
use slint::{ComponentHandle, Model};

use super::drive::chord;
use super::script::{self, Beat, Script};
use crate::{MainWindow, Motion, Playback, Settings, Subtitles};

/// Walk the keyboard into a panel and down it, with real key events.
///
/// Presses `P` and then Down rather than assigning to the property, which
/// would prove the wash draws and nothing about whether the keys arrive.
/// Twelve rows down because nine fit, and the three past the bottom edge are
/// where the list has to follow the ring.
pub(super) fn reach_test(ui: &MainWindow) -> Script {
    use slint::SharedString;
    use slint::platform::Key;

    // `DBM_REACH_TEST=glass` walks into a settings page instead, where the
    // rows are sliders and the interesting keys are left and right. `=up`
    // opens the playlist the way the folder button does, with no key at all,
    // so the ring starts hidden and the first Up has to both show it and land
    // at the bottom.
    let mode = std::env::var("DBM_REACH_TEST").unwrap_or_default();
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(400));
        let up = SharedString::from(Key::UpArrow);
        let down = SharedString::from(Key::DownArrow);
        let left = SharedString::from(Key::LeftArrow);
        let enter = SharedString::from(Key::Return);
        beat.next().await;
        match mode.as_str() {
            "up" => {
                // Not a key: this is what the folder button does.
                ui.invoke_open_playlist(true);
                eprintln!("dbm: playlist opened without a key");
                for _ in 0..2 {
                    beat.next().await;
                    chord(&ui, &[], up.as_str());
                    eprintln!("dbm: up -> row {}", ui.get_focus_row());
                }
                beat.next().await;
            }
            // The accessibility page's switch, pressed with Enter rather than
            // clicked: the keys reach it through `focus-activate-settings`,
            // which a click never touches. Twice, so the run leaves it on.
            "accessibility" => {
                eprintln!("dbm: pressing S for the settings");
                chord(&ui, &[], "s");
                // S lands the ring on the first row, Video, so four Downs
                // reach Accessibility; Enter opens it; two Downs past the
                // interface's size onto Animations.
                for _ in 0..4 {
                    beat.next().await;
                    chord(&ui, &[], down.as_str());
                    eprintln!("dbm: down -> row {}", ui.get_focus_row());
                }
                beat.next().await;
                chord(&ui, &[], enter.as_str());
                eprintln!("dbm: enter, opening the accessibility page");
                for _ in 0..2 {
                    beat.next().await;
                    chord(&ui, &[], down.as_str());
                    eprintln!("dbm: down -> row {}", ui.get_focus_row());
                }
                for _ in 0..2 {
                    beat.next().await;
                    chord(&ui, &[], enter.as_str());
                    eprintln!(
                        "dbm: enter on row {} -> animations {}, surface {}ms",
                        ui.get_focus_row(),
                        ui.global::<Motion>().get_enabled(),
                        ui.global::<Motion>().get_surface()
                    );
                }
                beat.next().await;
            }
            "glass" => {
                eprintln!("dbm: pressing S for the settings");
                chord(&ui, &[], "s");
                // S puts the ring on the first row, Video, so two Downs reach
                // Liquid glass, and a third would walk into the ambient
                // border's page.
                for _ in 0..2 {
                    beat.next().await;
                    chord(&ui, &[], down.as_str());
                }
                beat.next().await;
                chord(&ui, &[], enter.as_str());
                eprintln!("dbm: enter, opening the glass page");
                // Down onto the first slider.
                beat.next().await;
                chord(&ui, &[], down.as_str());
                eprintln!("dbm: down -> row {}", ui.get_focus_row());
                // Named and read back, so a walk that lands on the wrong page
                // says so rather than reporting a row number that fits any.
                for _ in 0..8 {
                    beat.next().await;
                    chord(&ui, &[], left.as_str());
                    let row = ui.get_focus_row();
                    let slider = (row as usize)
                        .checked_sub(1)
                        .and_then(|i| ui.global::<Settings>().get_glass_params().row_data(i))
                        .map(|p| format!("{} {}", p.label, p.readout))
                        .unwrap_or_default();
                    eprintln!("dbm: left on row {row} -> glass {slider}");
                }
                beat.rest(3).await;
            }
            _ => {
                eprintln!("dbm: pressing P for the playlist");
                chord(&ui, &[], "p");
                for at in 1..=12 {
                    beat.next().await;
                    chord(&ui, &[], down.as_str());
                    eprintln!(
                        "dbm: down {at} -> row {}, scrolled {}",
                        ui.get_focus_row(),
                        ui.get_playlist_scroll()
                    );
                }
                beat.next().await;
            }
        }
        eprintln!("dbm: --- reach test done ---");
    })
}

/// Drive the shortcuts that have nowhere visible to report.
///
/// Ctrl+O puts a native dialog on screen, so there is no property to read
/// afterwards — `dialog` traces the request instead, and that line is the
/// evidence the chord arrived.
pub(super) fn key_test(ui: &MainWindow) -> Script {
    use slint::platform::Key;

    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1200));
        beat.next().await;
        // C, twice, on a file whose subtitles aren't showing: the first
        // press must turn them on, not turn off a `sub-visibility` that
        // already shows nothing.
        //
        // Before the dialogs, since a file chooser keeps the keyboard.
        eprintln!(
            "dbm: subs showing {}",
            ui.global::<Subtitles>().get_visible()
        );
        eprintln!("dbm: pressing C");
        chord(&ui, &[], "c");
        beat.next().await;
        eprintln!(
            "dbm: subs showing {}",
            ui.global::<Subtitles>().get_visible()
        );
        eprintln!("dbm: pressing C again");
        chord(&ui, &[], "c");
        beat.next().await;
        eprintln!(
            "dbm: subs showing {}",
            ui.global::<Subtitles>().get_visible()
        );
        eprintln!("dbm: pressing B (ambience)");
        chord(&ui, &[], "b");
        beat.next().await;
        eprintln!(
            "dbm: ambience now {}",
            ui.global::<Settings>().get_ambience_on()
        );
        beat.next().await;
        eprintln!("dbm: pressing Ctrl+O");
        chord(&ui, &[Key::Control], "o");
        beat.next().await;
        eprintln!("dbm: pressing Ctrl+Shift+O");
        chord(&ui, &[Key::Control, Key::Shift], "O");
        beat.rest(2).await;
        eprintln!("dbm: --- key test done ---");
    })
}

/// The shortcuts page under the keys that read it.
///
/// Its rows are statements, so the ring has only the way back to stand on,
/// and the keys must scroll the page rather than wrap the ring or reach the
/// film. `time-pos` is the evidence rather than the list, which is built
/// inside the drill's conditional and can't be read from here: the film must
/// not move and the page must not close.
pub(super) fn read_test(ui: &MainWindow, mpv: Reader) -> Script {
    use slint::platform::Key;

    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(900));
        let pos = || super::shown(mpv.get(dbm_mpv::prop::TIME_POS));
        let key = |k: Key| chord(&ui, &[], &slint::SharedString::from(k));
        beat.next().await;
        // Straight there, by the key the page itself documents.
        eprintln!("dbm: pressing ?");
        chord(&ui, &[], "?");
        beat.next().await;
        let before = pos();
        // The page number stays private to the interface rather
        // than made public for a test. `?` opens the panel on the
        // shortcuts page or not at all.
        eprintln!(
            "dbm: settings-open={} time-pos={before}",
            ui.get_settings_open()
        );
        eprintln!("dbm: pressing End");
        key(Key::End);
        beat.next().await;
        eprintln!("dbm: pressing Home");
        key(Key::Home);
        beat.next().await;
        eprintln!("dbm: pressing PageDown");
        key(Key::PageDown);
        beat.next().await;
        eprintln!("dbm: pressing PageUp");
        key(Key::PageUp);
        beat.next().await;
        eprintln!("dbm: pressing Down");
        key(Key::DownArrow);
        beat.next().await;
        // Playing, so the two differ, but only by about the time
        // this test took, not back to zero.
        eprintln!(
            "dbm: settings-open={} time-pos={} (was {before})",
            ui.get_settings_open(),
            pos()
        );
        eprintln!("dbm: --- read test done ---");
    })
}

/// Let the chrome go to sleep over a playing film, press one key or move the
/// pointer, and report what came up, a line a tick: the bar's glass (whole
/// is 94px tall, the timeline alone 46px, what is loaded alone 64px and
/// narrower), the window's buttons, the flash's words, and how far the
/// subtitles were lifted.
///
/// No pointer event is sent but the mode's own. The key goes in five seconds
/// after the start, by when the bar has slept.
pub(super) fn wake_test(ui: &MainWindow) -> Script {
    use slint::platform::Key;

    let mode = std::env::var("DBM_WAKE_TEST").unwrap_or_default();
    const PRESS: u32 = 20;
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(250));
        let playback = ui.global::<Playback>();
        for tick in 1..=PRESS + 21 {
            beat.next().await;
            let key = |text: Key| chord(&ui, &[], slint::SharedString::from(text).as_str());
            let size = ui.window().size().to_logical(ui.window().scale_factor());
            // Two moves, far enough apart to be a hand rather than a twitch.
            let reach = |x: f32, y: f32| {
                super::drive::hover(&ui, x, y - 40.0);
                super::drive::hover(&ui, x, y);
            };
            match (mode.as_str(), tick) {
                // The bar is held by a pause, then let go by a frame step.
                ("pause-step", t) if t == PRESS - 8 => chord(&ui, &[], " "),
                ("pause-step", PRESS) => chord(&ui, &[], "."),
                // Held by a pause, then played on, or by a panel, then closed.
                ("space-play", t) if t == PRESS - 8 => chord(&ui, &[], " "),
                ("space-play", PRESS) => chord(&ui, &[], " "),
                ("escape-panel", t) if t == PRESS - 8 => chord(&ui, &[], "s"),
                ("escape-panel", PRESS) => key(Key::Escape),
                // A knock, then a hand: the first should bring up nothing.
                ("twitch", PRESS) => {
                    super::drive::hover(&ui, size.width / 2.0, size.height / 2.0);
                    super::drive::hover(&ui, size.width / 2.0 + 4.0, size.height / 2.0 + 3.0);
                }
                ("twitch", t) if t == PRESS + 8 => reach(size.width / 2.0, size.height / 2.0),
                // Onto the play button and left there, past the clock.
                // Hover is only found on a move, so one more once the bar is
                // under the pointer.
                ("hover", PRESS) => reach(size.width / 2.0, ui.get_timeline_y() + 30.0 + 26.0),
                ("hover", t) if t == PRESS + 2 => {
                    super::drive::hover(
                        &ui,
                        size.width / 2.0 + 2.0,
                        ui.get_timeline_y() + 30.0 + 26.0,
                    );
                }
                // What is loaded is up alone when the pointer comes low: the
                // bar should grow from it.
                ("next-move", t) if t == PRESS + 8 => reach(size.width / 2.0, size.height * 0.75),
                ("leave" | "overshoot" | "return", PRESS) => {
                    reach(size.width / 2.0, size.height * 0.75);
                }
                ("leave" | "overshoot" | "return", t) if t == PRESS + 4 => {
                    super::drive::leave(&ui);
                }
                // Back over the edge a moment later, or after the chrome has
                // gone, by the few pixels of finding the edge to resize from.
                ("overshoot", t) if t == PRESS + 5 => {
                    super::drive::hover(&ui, size.width - 4.0, size.height * 0.75);
                    super::drive::hover(&ui, size.width - 6.0, size.height * 0.75 + 1.0);
                }
                ("return", t) if t == PRESS + 10 => {
                    super::drive::hover(&ui, size.width - 4.0, size.height * 0.75);
                    super::drive::hover(&ui, size.width - 6.0, size.height * 0.75 + 1.0);
                }
                // Subtitles on, into a stretch with lines, while the open's
                // bar is still up, for the line to rise onto the timeline.
                ("seek-subs", 2) => {
                    playback.invoke_seek_fraction(0.3);
                    chord(&ui, &[], "c");
                }
                (_, PRESS) => {
                    eprintln!("dbm: pressing {mode}");
                    match mode.as_str() {
                        "seek" | "seek-move" | "seek-subs" => key(Key::RightArrow),
                        "fine" => chord(
                            &ui,
                            &[Key::Shift],
                            slint::SharedString::from(Key::RightArrow).as_str(),
                        ),
                        "volume" => key(Key::UpArrow),
                        "mute" => chord(&ui, &[], "m"),
                        "subs" => chord(&ui, &[Key::Shift], "C"),
                        "speed" => chord(&ui, &[Key::Shift], ">"),
                        "step" => chord(&ui, &[], "."),
                        "panscan" => chord(&ui, &[], "t"),
                        "fullscreen" => chord(&ui, &[], "f"),
                        "tab" => key(Key::Tab),
                        "space" => chord(&ui, &[], " "),
                        "next" | "next-move" => chord(&ui, &[Key::Shift], "N"),
                        "double-click" => {
                            let size = ui.window().size().to_logical(ui.window().scale_factor());
                            let (x, y) = (size.width / 2.0, size.height / 2.0);
                            super::drive::click(&ui, x, y);
                            super::drive::click(&ui, x, y);
                        }
                        _ => {}
                    }
                }
                // The timeline is up alone when the pointer moves: the bar
                // should grow from it rather than appear beside it.
                ("seek-move", t) if t == PRESS + 2 => super::drive::hover(&ui, 40.0, 40.0),
                _ => {}
            }
            if tick + 2 < PRESS || tick > PRESS + 20 {
                if tick == PRESS + 21 {
                    eprintln!("dbm: --- wake test done ---");
                }
                continue;
            }
            let rects = ui.get_glass_rects();
            let rect = |i: usize| rects.row_data(i).unwrap_or_default();
            let (bar, window, words) = (rect(0), rect(1), rect(5));
            let chrome = ui.global::<crate::Chrome>();
            eprintln!(
                "dbm: wake t={:+.2}s bar y={:.0} w={:.0} h={:.0} a={:.2} | window a={:.2} | flash w={:.0} a={:.2} | lift={:.2} floor={:.3} | idle={} pointer-hidden={} fullscreen={} paused={} panel={}",
                (tick as f32 - PRESS as f32) * 0.25,
                bar.y,
                bar.width,
                if bar.width > 0.0 { bar.height } else { 0.0 },
                if bar.width > 0.0 { bar.opacity } else { 0.0 },
                if window.width > 0.0 {
                    window.opacity
                } else {
                    0.0
                },
                words.width,
                if words.width > 0.0 {
                    words.opacity
                } else {
                    0.0
                },
                ui.get_subtitle_lift(),
                ui.get_subtitle_floor(),
                chrome.get_idle(),
                ui.get_pointer_hidden(),
                ui.get_fullscreen(),
                playback.get_paused(),
                ui.get_settings_open(),
            );
        }
    })
}
