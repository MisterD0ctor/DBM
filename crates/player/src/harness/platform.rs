//! What the operating system is asked for: the pointer, and the audio device.

use std::time::Duration;

use slint::ComponentHandle;

use super::drive::hover;
use crate::playback::mpv::Mpv;
use crate::MainWindow;

/// Watch the pointer disappear and come back.
///
/// Timed around the three seconds the chrome waits, with the film left
/// playing: the run starts with a real move, so the clock starts from a known
/// point rather than from whenever the window last saw the hand.
///
/// Both halves matter and the second one more. A pointer that hides is only
/// half a feature — one that does not come back the moment the hand moves is
/// worse than one that never left.
pub(super) fn cursor_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => {
                    // The real one, not a dispatched event. `ShowCursor`
                    // keeps its display count on this thread's input queue,
                    // so it governs the pointer only while the pointer is
                    // over this window — and a test that asked the system
                    // about a pointer sitting on some other window would be
                    // reading an answer about somebody else's cursor.
                    park_pointer(&ui);
                    hover(&ui, 40.0, 40.0);
                    report_cursor(&ui, "hand just moved");
                }
                2 => report_cursor(&ui, "a second later"),
                // Past HIDE_AFTER, plus a poll for the clock to notice.
                8 => report_cursor(&ui, "chrome faded"),
                9 => {
                    hover(&ui, 60.0, 80.0);
                    eprintln!("dbm: cursor  --- the hand moves ---");
                }
                10 => report_cursor(&ui, "hand moved again"),
                11 => eprintln!("dbm: --- cursor test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Put the physical pointer in the middle of the window.
#[cfg(windows)]
fn park_pointer(ui: &MainWindow) {
    let at = ui.window().position();
    let size = ui.window().size();
    let (x, y) = (at.x + size.width as i32 / 2, at.y + size.height as i32 / 2);
    let moved = unsafe { windows::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y) };
    eprintln!(
        "dbm: cursor  parked at {x},{y} ({})",
        if moved.is_ok() { "ok" } else { "refused" }
    );
}

#[cfg(not(windows))]
fn park_pointer(_ui: &MainWindow) {}

fn report_cursor(ui: &MainWindow, when: &str) {
    let asked = ui.get_pointer_hidden();
    let on_screen = match crate::platform::cursor::on_screen() {
        Some(true) => "on screen",
        Some(false) => "hidden",
        None => "not answerable on this platform",
    };
    eprintln!(
        "dbm: cursor  {when:<18} interface wants it {:<8} | system says {on_screen}",
        if asked { "hidden" } else { "shown" }
    );
}

/// Stage the failure the audio watchdog exists for.
///
/// The real one needs a Bluetooth device to walk out of range, so the two
/// halves are faked separately and each honestly: mpv really does drop the
/// audio track, which is what leaves the player silent, and the watchdog
/// really is handed a device list with something in it that was not there
/// before. Everything between — the diff, the settle, the rebuild on the
/// worker — is the code under test.
///
/// The property writes here block the UI thread. That is fine in a harness and
/// would not be anywhere else.
pub(super) fn audio_test(
    mpv: std::sync::Arc<Mpv>,
    audio: std::rc::Rc<crate::playback::audio::Watchdog>,
) -> slint::Timer {
    const BASELINE: &str = r#"[{"name":"auto","description":"Autoselect device"}]"#;
    const PLUS_ONE: &str = r#"[{"name":"auto","description":"Autoselect device"},
        {"name":"wasapi/harness","description":"A device that just turned up"}]"#;
    const PLUS_TWO: &str = r#"[{"name":"auto","description":"Autoselect device"},
        {"name":"wasapi/harness","description":"A device that just turned up"},
        {"name":"wasapi/harness-2","description":"And another"}]"#;

    let timer = slint::Timer::default();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(900),
        move || {
            match step {
                0 => report_audio(&mpv, "start"),
                // Take the baseline, so the next list reads as an arrival.
                1 => audio.on_event(&device_list(BASELINE)),
                // What mpv does by itself when the device under the output
                // disappears: the track goes, playback does not.
                2 => {
                    if let Err(e) = mpv.set_property("aid", "no") {
                        eprintln!("dbm: audio test: could not drop the track: {e}");
                    }
                }
                3 => report_audio(&mpv, "device lost"),
                4 => audio.on_event(&device_list(PLUS_ONE)),
                // The settle is 1200ms and the rebuild is a worker round
                // trip, so the result lands somewhere in these two steps.
                5 | 6 => {}
                7 => report_audio(&mpv, "after recovery"),
                // Second half: silence somebody asked for is not a fault.
                // The interface's own "Off" row, then another device turning
                // up, which must leave it alone.
                8 => {
                    audio.note_user_selection(None);
                    if let Err(e) = mpv.set_property("aid", "no") {
                        eprintln!("dbm: audio test: could not turn audio off: {e}");
                    }
                }
                9 => report_audio(&mpv, "user chose off"),
                10 => audio.on_event(&device_list(PLUS_TWO)),
                11 | 12 => {}
                13 => report_audio(&mpv, "still off"),
                14 => eprintln!("dbm: --- audio test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// An `audio-device-list` change as mpv would report it.
fn device_list(payload: &str) -> crate::playback::mpv::Event {
    crate::playback::mpv::Event::Property {
        name: "audio-device-list".into(),
        value: crate::playback::mpv::Value::Str(payload.into()),
    }
}

fn report_audio(mpv: &Mpv, label: &str) {
    let read = |name: &str| mpv.get_property(name).unwrap_or_else(|| "-".into());
    eprintln!(
        "dbm: audio {label:<16} aid={} device={} tracks={}",
        read("aid"),
        read("audio-device"),
        read("track-list/count"),
    );
}
