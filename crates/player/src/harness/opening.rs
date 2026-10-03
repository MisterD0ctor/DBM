//! Opening things: a path, the file dialog, a drop, and the playlist scan.

use std::time::Duration;

use slint::ComponentHandle;

use super::playlist_rows;
use crate::MainWindow;

/// Open a path the way the dialog does, and say what came of it.
///
/// Everything after the path is chosen is shared with the command line, so
/// this covers the part of opening that can actually break: the scan, the
/// `loadlist`, and the seek to the right entry once it lands.
pub(super) fn open_test(ui: &MainWindow, path: String) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1500),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => {
                    eprintln!("dbm: opening {path}");
                    ui.invoke_open_path(path.as_str().into());
                    // Whether the interface goes on saying so. With a film
                    // already playing, the flag used to be cleared on the
                    // very next frame, because a file existed — so these
                    // read false at once and "Opening …" never reached the
                    // screen. Now they should read true until mpv loads the
                    // new file, then false.
                    for ms in [30u64, 150, 600, 1400] {
                        let weak = weak.clone();
                        slint::Timer::single_shot(Duration::from_millis(ms), move || {
                            if let Some(ui) = weak.upgrade() {
                                eprintln!("dbm: +{ms:>4}ms opening={}", ui.get_opening());
                            }
                        });
                    }
                }
                // Two ticks: the scan is on the worker and the `loadlist`
                // that follows is asynchronous again.
                2 => report_open(&ui, "after open"),
                3 => eprintln!("dbm: --- open test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
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
        ui.get_media_title(),
        !ui.get_paused(),
        entries.len(),
        entries.join(", "),
    );
}

/// Put a real dialog on screen and leave it there.
///
/// There is nothing to assert from here — a native dialog cannot be driven
/// by a timer — but the trace alongside it answers the only question that
/// matters: whether the player keeps drawing while it is up.
pub(super) fn dialog_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(2000),
        move || {
            if let Some(ui) = weak.upgrade() {
                eprintln!("dbm: --- opening a file dialog; draws should continue ---");
                ui.invoke_open_file();
            }
        },
    );
    timer
}

/// Drop a file on the window without a mouse.
///
/// Everything from the `WM_DROPFILES` message inward is the real path: the
/// same block the shell builds, posted to the same window, read by the same
/// handler. What it cannot cover is the drag itself — whether Explorer's drop
/// lands on our target or on the one winit registered — so a real drag is
/// still worth doing once by hand.
#[cfg(windows)]
pub(super) fn drop_test(ui: &MainWindow, path: String) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_drop(&ui, "before"),
                1 => {
                    if let Err(e) = crate::platform::dropped::post_test_drop(&ui, &path) {
                        eprintln!("dbm: drop test: {e}");
                    }
                }
                2 | 3 => report_drop(&ui, "after the drop"),
                4 => eprintln!("dbm: --- drop test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

#[cfg(windows)]
fn report_drop(ui: &MainWindow, label: &str) {
    eprintln!(
        "dbm: drop {label:<16} title={:?} playlist={} entries",
        ui.get_media_title(),
        playlist_rows(ui).len(),
    );
}

/// Watch the playlist describe files nobody has played.
///
/// Reported early and often, because *when* is the point: rows that are right
/// eventually would also be the old behaviour, given long enough to play
/// every episode.
pub(super) fn scan_test(ui: &MainWindow) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(400),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 | 1 | 2 | 4 | 8 => report_rows(&ui, step * 400),
                9 => eprintln!("dbm: --- scan test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
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
