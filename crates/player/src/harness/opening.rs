//! Opening things: a path, the file dialog, a drop, and the playlist scan.

use std::time::Duration;

use slint::ComponentHandle;

use super::playlist_rows;
use super::script::{self, Beat, Script, sleep};
use crate::{MainWindow, Playback, Playlist};

/// Open a path the way the dialog does, and say what came of it.
///
/// Everything after the path is chosen is shared with the command line, so
/// this covers the part of opening that can actually break: the scan, the
/// `loadlist`, and the seek to the right entry once it lands.
pub(super) fn open_test(ui: &MainWindow, path: String) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1500));
        beat.next().await;
        eprintln!("dbm: opening {path}");
        ui.global::<Playlist>()
            .invoke_open_path(path.as_str().into(), true);
        // Whether the interface goes on saying so: true until mpv
        // has loaded the new file, then false, even with a film
        // already playing.
        for ms in [30u64, 150, 600, 1400] {
            let weak = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(ms), move || {
                if let Some(ui) = weak.upgrade() {
                    eprintln!(
                        "dbm: +{ms:>4}ms opening={}",
                        ui.global::<Playlist>().get_opening()
                    );
                }
            });
        }
        beat.rest(2).await;
        // Two ticks: the scan is on the worker and the `loadlist`
        // that follows is asynchronous again.
        report_open(&ui, "after open");
        beat.next().await;
        eprintln!("dbm: --- open test done ---");
    })
}

fn report_open(ui: &MainWindow, label: &str) {
    let entries: Vec<String> = playlist_rows(ui)
        .iter()
        .map(|e| {
            let mark = if e.current { "*" } else { " " };
            format!("{mark}{}", e.label)
        })
        .collect();
    eprintln!(
        "dbm: {label:<12} title={:?} playing={} | {} entries: {}",
        ui.global::<Playback>().get_title(),
        !ui.global::<Playback>().get_paused(),
        entries.len(),
        entries.join(", "),
    );
}

/// Put a real dialog on screen and leave it there.
///
/// Nothing to assert, since a timer can't drive a native dialog, but the
/// trace shows whether the player keeps drawing while it is up.
pub(super) fn dialog_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        sleep(Duration::from_millis(2000)).await;
        eprintln!("dbm: --- opening a file dialog; draws should continue ---");
        ui.global::<Playlist>().invoke_open_file();
    })
}

/// Drop a file on the window without a mouse.
///
/// Everything from the `WM_DROPFILES` message inward is the real path. It
/// can't cover the drag itself, whether Explorer's drop lands on our target
/// or winit's, so a real drag is still worth doing once by hand.
#[cfg(windows)]
pub(super) fn drop_test(ui: &MainWindow, path: String) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1200));
        beat.next().await;
        report_drop(&ui, "before");
        beat.next().await;
        if let Err(e) = crate::platform::dropped::post_test_drop(&ui, &path) {
            eprintln!("dbm: drop test: {e}");
        }
        beat.next().await;
        report_drop(&ui, "after the drop");
        beat.next().await;
        report_drop(&ui, "after the drop");
        beat.next().await;
        eprintln!("dbm: --- drop test done ---");
    })
}

#[cfg(windows)]
fn report_drop(ui: &MainWindow, label: &str) {
    eprintln!(
        "dbm: drop {label:<16} title={:?} playlist={} entries",
        ui.global::<Playback>().get_title(),
        playlist_rows(ui).len(),
    );
}

/// Watch the playlist describe files nobody has played.
///
/// Reported early and often, because when the rows fill in is the point,
/// not only that they do.
pub(super) fn scan_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(400));
        // The rows at doubling waits, as the scan fills them in.
        for at in 0..=8 {
            beat.next().await;
            if matches!(at, 0 | 1 | 2 | 4 | 8) {
                report_rows(&ui, at * 400);
            }
        }
        beat.next().await;
        eprintln!("dbm: --- scan test done ---");
    })
}

fn report_rows(ui: &MainWindow, ms: usize) {
    let rows = playlist_rows(ui);
    let timed = rows.iter().filter(|r| !r.length.is_empty()).count();
    eprintln!(
        "dbm: scan  at {ms:>4}ms  {timed} of {} rows have a length",
        rows.len()
    );
    for row in rows.iter() {
        eprintln!(
            "dbm: scan      {:<44} {}",
            row.label.as_str(),
            row.length.as_str()
        );
    }
}
