use slint::ComponentHandle;

use crate::MainWindow;
use crate::app::App;
use crate::playback::commands;
use crate::worker::Completion;

/// Subtitle and audio tracks.
pub(super) fn wire_tracks(ui: &MainWindow, app: &App) {
    // Reads the track list before deciding, so it goes to the worker rather
    // than blocking a keypress behind mpv's core lock.
    // Not activity, for `on!`'s reason: C answers with a flash.
    on_app!(ui, app, on_toggle_subtitles, |app| {
        app.worker.run(commands::toggle_subtitles);
    });
    // -1 is the menu's "Off" row rather than a real track id.
    on!(ui, app, on_select_subtitle, |m, id| {
        commands::set_subtitle_track(m, (id >= 0).then_some(i64::from(id)));
    });
    // The one selection the watchdog has to hear about: choosing "Off" is
    // the only silence it must not undo when a device reappears.
    on_app!(ui, app, on_select_audio, |app, id| {
        app.activity.bump();
        let id = (id >= 0).then_some(i64::from(id));
        app.audio.note_user_selection(id);
        commands::set_audio_track(&app.mpv, id);
    });
    on!(ui, app, on_cycle_track, |m, subtitles, direction| {
        commands::cycle_track(m, subtitles, direction);
    });
    // A track out, and a fetched one forgotten with it so it doesn't come
    // back next time the film opens. Reads the track list, so on the worker.
    on_app!(ui, app, on_remove_track, |app, id, subtitle| {
        app.activity.bump();
        app.worker.run(move |mpv| {
            let Some(file) = commands::remove_track(mpv, i64::from(id), subtitle) else {
                return;
            };
            if let Some(path) = mpv.get_property("path") {
                dbm_library::opensubtitles::forget(&path, &file);
            }
        });
    });
    wire_finding(ui, app);
}

/// Subtitles from OpenSubtitles: the search the menu's last subtitle row
/// starts, and the result a row of its answer fetches. Both are network
/// round trips on threads of their own; the answers come back as
/// completions, which is where the rows are filled and the file loaded.
pub(super) fn wire_finding(ui: &MainWindow, app: &App) {
    use dbm_library::opensubtitles;
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_find_subtitles(move || {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            let path = ui.get_current_path().to_string();
            if path.is_empty() {
                return;
            }
            app.found.borrow_mut().clear();
            ui.set_find_hits(Default::default());
            ui.set_find_state(1);
            opensubtitles::search(
                path,
                search_languages(),
                app.worker.reporter(Completion::Found),
            );
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_fetch_subtitle(move |row| {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            let hit = usize::try_from(row)
                .ok()
                .and_then(|row| app.found.borrow().get(row).cloned());
            let path = ui.get_current_path().to_string();
            let Some(hit) = hit.filter(|_| !path.is_empty()) else {
                return;
            };
            ui.set_find_fetching(row);
            opensubtitles::fetch(path, hit, app.worker.reporter(Completion::Fetched));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_sign_in(move |name, password| {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            // The player's one copy of the password, overwritten once it is
            // sent. Slint's is cleared from the field as this returns.
            let password = zeroize::Zeroizing::new(password.as_str().to_owned());
            let name = name.trim().to_string();
            if name.is_empty() || password.is_empty() {
                return;
            }
            ui.set_signing_in(true);
            ui.set_sign_in_said(Default::default());
            opensubtitles::account::sign_in(
                name,
                password,
                app.worker.reporter(Completion::SignedIn),
            );
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        ui.on_sign_out(move || {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            opensubtitles::account::sign_out();
            ui.set_find_account(Default::default());
            ui.set_find_left(-1);
        });
    }
}

/// The languages a search asks for: the one the player speaks, and English
/// beside it, which more films have than any other. In OpenSubtitles' own
/// codes, which split Portuguese in two where the player has one.
fn search_languages() -> Vec<String> {
    let mut codes: Vec<String> = match dbm_lang::spoken() {
        "pt" => vec!["pt-br".into(), "pt-pt".into()],
        code => vec![code.to_string()],
    };
    codes.push("en".into());
    codes.sort();
    codes.dedup();
    codes
}
