//! What the operating system is asked for: the pointer, and the audio device.

use std::time::Duration;

use dbm_mpv::Reader;
use dbm_mpv::prop::{self, TrackChoice};

use super::drive::hover;
use super::script::{self, Beat, Script};
use crate::MainWindow;

/// Watch the pointer disappear and come back.
///
/// Timed around the three seconds the chrome waits, with the film left
/// playing: the run starts with a real move, so the clock starts from a known
/// point rather than from whenever the window last saw the hand.
///
/// The second half matters more: a pointer that doesn't come back the
/// moment the hand moves is worse than one that never left.
pub(super) fn cursor_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(500));
        beat.next().await;
        // The real pointer, not a dispatched event: `ShowCursor`
        // only governs it while it is over this window, so asking
        // about one elsewhere would read another app's cursor.
        park_pointer(&ui);
        hover(&ui, 40.0, 40.0);
        report_cursor(&ui, "hand just moved");
        beat.rest(2).await;
        report_cursor(&ui, "a second later");
        beat.rest(6).await;
        // Past HIDE_AFTER, plus a poll for the clock to notice.
        report_cursor(&ui, "chrome faded");
        beat.next().await;
        hover(&ui, 60.0, 80.0);
        eprintln!("dbm: cursor  --- the hand moves ---");
        beat.next().await;
        report_cursor(&ui, "hand moved again");
        beat.next().await;
        eprintln!("dbm: --- cursor test done ---");
    })
}

/// Put the physical pointer in the middle of the window.
#[cfg(windows)]
fn park_pointer(ui: &MainWindow) {
    use slint::ComponentHandle;

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
/// halves are staged separately: mpv really drops the audio track, and the
/// watchdog really is handed a device list with something new in it.
/// Everything between, the diff, the settle and the rebuild on the worker, is
/// the code under test. The property writes block the UI thread, which is
/// fine only in a harness.
pub(super) fn audio_test(
    ui: &MainWindow,
    mpv: Reader,
    audio: std::rc::Rc<crate::playback::audio::Watchdog>,
) -> Script {
    const BASELINE: &str = r#"[{"name":"auto","description":"Autoselect device"}]"#;
    const PLUS_ONE: &str = r#"[{"name":"auto","description":"Autoselect device"},
        {"name":"wasapi/harness","description":"A device that just turned up"}]"#;
    const PLUS_TWO: &str = r#"[{"name":"auto","description":"Autoselect device"},
        {"name":"wasapi/harness","description":"A device that just turned up"},
        {"name":"wasapi/harness-2","description":"And another"}]"#;

    script::run(ui, move |_| async move {
        let mut beat = Beat::new(Duration::from_millis(900));
        beat.next().await;
        report_audio(&mpv, "start");
        beat.next().await;
        // Take the baseline, so the next list reads as an arrival.
        audio.on_event(&device_list(BASELINE));
        beat.next().await;
        // What mpv does by itself when the device under the output
        // disappears: the track goes, playback does not.
        if let Err(e) = mpv.set(prop::AID, TrackChoice::Off) {
            eprintln!("dbm: audio test: could not drop the track: {e}");
        }
        beat.next().await;
        report_audio(&mpv, "device lost");
        beat.next().await;
        audio.on_event(&device_list(PLUS_ONE));
        // The settle is 1200ms and the rebuild is a worker round
        // trip, so the result lands somewhere in these two steps.
        beat.rest(3).await;
        report_audio(&mpv, "after recovery");
        beat.next().await;
        // Second half: silence somebody asked for is not a fault. The
        // "Off" row, then another device turning up, which must leave
        // it alone.
        audio.note_user_selection(None);
        if let Err(e) = mpv.set(prop::AID, TrackChoice::Off) {
            eprintln!("dbm: audio test: could not turn audio off: {e}");
        }
        beat.next().await;
        report_audio(&mpv, "user chose off");
        beat.next().await;
        audio.on_event(&device_list(PLUS_TWO));
        beat.rest(3).await;
        report_audio(&mpv, "still off");
        beat.next().await;
        eprintln!("dbm: --- audio test done ---");
    })
}

/// An `audio-device-list` change as mpv would report it.
fn device_list(payload: &str) -> dbm_mpv::Event {
    dbm_mpv::Event::Property {
        id: crate::playback::state::Prop::AudioDeviceList.id(),
        value: Some(dbm_mpv::Value::Str(payload.into())),
    }
}

fn report_audio(mpv: &Reader, label: &str) {
    eprintln!(
        "dbm: audio {label:<16} aid={} device={} tracks={}",
        super::shown(mpv.get(prop::AID)),
        super::shown(mpv.get(prop::AUDIO_DEVICE)),
        super::shown(mpv.get(prop::TRACK_COUNT)),
    );
}
