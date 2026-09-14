//! Asking the desktop for a path.
//!
//! A native file dialog runs its own modal message loop, so calling one from
//! the UI thread stops the player dead for as long as it is open — no frames,
//! no events, a black window until the user picks something. So the dialog
//! gets its own thread and the answer comes back through the event loop.
//!
//! Deliberately *not* the worker thread. The worker serialises jobs, and a
//! dialog sitting open for a minute would hold up every track-list read
//! behind it. A dialog is not work to be queued; it is a conversation with
//! the user that happens to take a while.
//!
//! What comes back is a path and nothing more. Turning that into a playlist
//! is the same job the command line already goes through — see
//! [`crate::playlist::prepare`] — which is why this module knows nothing
//! about mpv, playlists or the worker.

use crate::MainWindow;

/// What the user is being asked for.
#[derive(Clone, Copy)]
pub enum Want {
    /// One video. Its siblings come along as the playlist.
    File,
    /// A directory, scanned recursively.
    Folder,
}

/// Open a dialog and hand whatever is chosen to `open-path` on the UI thread.
///
/// Returns immediately. A cancelled dialog is silent: it is not an error, and
/// there is nothing to report.
///
/// `start` is where the dialog opens, when there is somewhere better than
/// wherever the system last left it: beside the file playing, for a file, and
/// the folder above its season, for a folder — which at the end of a season
/// is the shelf the next one is on.
pub fn pick(want: Want, start: Option<std::path::PathBuf>, ui: slint::Weak<MainWindow>) {
    // A fresh thread per invocation rather than a kept one: dialogs are rare,
    // and a thread that exists only while one is open cannot be a place where
    // state accumulates.
    eprintln!(
        "dbm: file dialog requested ({})",
        match want {
            Want::File => "file",
            Want::Folder => "folder",
        }
    );
    std::thread::spawn(move || {
        let Some(path) = choose(want, start) else {
            eprintln!("dbm: file dialog cancelled");
            return;
        };
        let path = path.to_string_lossy().into_owned();
        // Back to the UI thread, which owns the worker the scan has to go to.
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui.upgrade() {
                ui.invoke_open_path(path.into());
            }
        });
    });
}

fn choose(want: Want, start: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    match want {
        Want::File => dialog
            .set_title("Open video")
            .add_filter("Video", crate::playlist::VIDEO_EXTENSIONS)
            .add_filter("All files", &["*"])
            .pick_file(),
        Want::Folder => dialog.set_title("Open folder").pick_folder(),
    }
}
