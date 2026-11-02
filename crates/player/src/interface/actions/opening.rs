use slint::ComponentHandle;

use crate::app::App;
use crate::platform::dialog;
use crate::{MainWindow, Playback, Playlist};

/// Opening a file or a folder.
///
/// The dialog runs on its own thread and comes back through `open-path`,
/// which is also where a dropped file and the command line arrive: by the
/// time anything reaches here it is just a path.
pub(super) fn wire_opening(ui: &MainWindow, app: &App) {
    let playlist = ui.global::<Playlist>();
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playlist.on_open_file(move || {
            app.activity.bump();
            let start = weak
                .upgrade()
                .and_then(|ui| near(&ui))
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
            dialog::pick(dialog::Want::File, start, weak.clone());
        });
    }
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playlist.on_open_folder(move || {
            app.activity.bump();
            // The shelf above this file's folder: at the end of a season,
            // that is where the next one is.
            let start = weak
                .upgrade()
                .and_then(|ui| near(&ui))
                .and_then(|p| p.parent()?.parent().map(std::path::Path::to_path_buf));
            dialog::pick(dialog::Want::Folder, start, weak.clone());
        });
    }
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        playlist.on_open_path(move |path, ask| {
            app.activity.bump();
            let path = std::path::PathBuf::from(path.as_str());
            // Said before handing it over, since everything past here is on
            // another thread (a `read_dir` that can take seconds on a network
            // share, an m3u write, a `loadlist`, mpv's own open), and would
            // otherwise look like an ignored click. Set here, the one funnel
            // they share, so a drop and the command line get it too.
            if let Some(ui) = weak.upgrade() {
                ui.global::<Playlist>()
                    .set_opening_name(opening_name(&path).into());
                ui.global::<Playlist>().set_opening(true);
            }
            // Same job the command line goes through, scan and all.
            app.worker.submit(move |_mpv| {
                Some(crate::worker::Completion::Opened(
                    dbm_library::playlist::prepare(&crate::platform::real_place(&path))
                        .map(|prepared| dbm_library::playlist::Prepared { ask, ..prepared }),
                ))
            });
        });
    }
}

/// The file the dialogs should open near: the one playing, or failing that
/// the one the way in offers to continue.
fn near(ui: &MainWindow) -> Option<std::path::PathBuf> {
    let playback = ui.global::<Playback>();
    [playback.get_path(), playback.get_resume_path()]
        .into_iter()
        .find(|p| !p.is_empty())
        .map(|p| std::path::PathBuf::from(p.as_str()))
}

/// What to call the thing being opened while it is being opened.
///
/// The file's or folder's own name, not the path, since the line is one row
/// wide and the end is what identifies it. No extension, as everywhere else
/// in `naming`.
///
/// Decided from the name alone, without asking the disk whether it is a
/// folder, because this runs on the UI thread at a click, on a path that may
/// be on a share. So only a video extension is stripped, and a folder called
/// `Show.S01.1080p` keeps all of its name.
fn opening_name(path: &std::path::Path) -> String {
    let name = if dbm_library::playlist::is_video_file(path) {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::opening_name;
    use std::path::Path;

    #[test]
    fn an_opening_is_named_without_touching_the_disk() {
        assert_eq!(
            opening_name(Path::new("/films/Show/S02E01 - Pilot.mkv")),
            "S02E01 - Pilot"
        );
        assert_eq!(
            opening_name(Path::new("/films/Show.S01.1080p")),
            "Show.S01.1080p"
        );
        assert_eq!(opening_name(Path::new("/films/Season 2")), "Season 2");
    }
}
