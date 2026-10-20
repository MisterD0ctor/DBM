//! Asking the desktop for a path.
//!
//! A native file dialog runs its own modal message loop, so on the UI thread
//! it would stop the player dead while open. So the dialog gets its own
//! thread, and the answer comes back through the event loop.
//!
//! Not the worker, which serialises jobs: a dialog open for a minute would
//! hold up every track-list read behind it.
//!
//! What comes back is only a path, which becomes a playlist the way the
//! command line's does (see [`crate::library::playlist::prepare`]), so this
//! module knows nothing about mpv, playlists or the worker.

use crate::lang::tr;
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
/// Returns immediately. A cancelled dialog is silent, since it isn't an error.
///
/// `start` is where the dialog opens, when there is somewhere better than
/// where the system last left it: beside the file playing for a file, and the
/// folder above its season for a folder, where the next season is.
pub fn pick(want: Want, start: Option<std::path::PathBuf>, ui: slint::Weak<MainWindow>) {
    // A fresh thread per dialog rather than a kept one, since dialogs are rare
    // and a short-lived thread can't accumulate state.
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
                ui.invoke_open_path(path.into(), true);
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
            .set_title(tr("Open video"))
            .add_filter(tr("Video"), crate::library::playlist::VIDEO_EXTENSIONS)
            .add_filter(tr("All files"), &["*"])
            .pick_file(),
        Want::Folder => dialog.set_title(tr("Open folder")).pick_folder(),
    }
}
