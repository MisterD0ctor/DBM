use dbm_mpv::tracks::TrackId;
use dbm_naming::Language;
use slint::ComponentHandle;

use super::{pick, way};
use crate::app::App;
use crate::playback::commands;
use crate::worker::Completion;
use crate::{Find, FindState, MainWindow, Subtitles, Tracks};

/// Subtitle and audio tracks.
pub(super) fn wire_tracks(ui: &MainWindow, app: &App) {
    let tracks = ui.global::<Tracks>();
    // Reads the track list before deciding, so it goes to the worker rather
    // than blocking a keypress behind mpv's core lock.
    // Not activity, for `on!`'s reason: C answers with a flash.
    on_app!(ui.global::<Subtitles>(), app, on_toggle, |app| {
        app.worker.run(commands::toggle_subtitles);
    });
    on!(tracks, app, on_select_subtitle, |m, id| {
        commands::set_subtitle_track(m, chosen(id));
    });
    // The one selection the watchdog has to hear about: choosing "Off" is
    // the only silence it must not undo when a device reappears.
    on_app!(tracks, app, on_select_audio, |app, id| {
        app.activity.bump();
        let id = chosen(id);
        app.audio.note_user_selection(id);
        commands::set_audio_track(&app.mpv, id);
    });
    on!(tracks, app, on_cycle, |m, kind, direction| {
        commands::cycle_track(m, pick(kind), way(direction));
    });
    // A track out, and a fetched one forgotten with it so it doesn't come
    // back next time the film opens. Reads the track list, so on the worker.
    on_app!(tracks, app, on_remove, |app, id, kind| {
        app.activity.bump();
        app.worker.run(move |mpv| {
            let Some(file) = commands::remove_track(mpv, TrackId(i64::from(id)), pick(kind)) else {
                return;
            };
            if let Some(path) = mpv.get_or_none(dbm_mpv::prop::PATH) {
                dbm_library::opensubtitles::forget(&path, &file);
            }
        });
    });
    wire_finding(ui, app);
}

/// The track a menu row names by its id, or `None` for -1, the "Off" row.
fn chosen(id: i32) -> Option<TrackId> {
    (id >= 0).then_some(TrackId(i64::from(id)))
}

/// Subtitles from OpenSubtitles: the search the menu's last subtitle row
/// starts, and the result a row of its answer fetches. Both are network
/// round trips on threads of their own; the answers come back as
/// completions, which is where the rows are filled and the file loaded.
pub(super) fn wire_finding(ui: &MainWindow, app: &App) {
    use dbm_library::opensubtitles;
    let find = ui.global::<Find>();
    {
        let app = app.clone();
        let weak = ui.as_weak();
        find.on_search(move || {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            let Some(path) = app.playing.get() else {
                return;
            };
            app.search.clear();
            let find = ui.global::<Find>();
            find.set_hits(Default::default());
            find.set_state(FindState::Searching);
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
        find.on_fetch(move |row| {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            let Some((path, hit)) = app.playing.get().zip(app.search.hit(row)) else {
                return;
            };
            ui.global::<Find>().set_fetching(row);
            opensubtitles::fetch(path, hit, app.worker.reporter(Completion::Fetched));
        });
    }
    {
        let app = app.clone();
        let weak = ui.as_weak();
        find.on_sign_in(move |name, password| {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            // The player's one copy of the password, overwritten once it is
            // sent. Slint's is cleared from the field as this returns.
            let password = zeroize::Zeroizing::new(password.as_str().to_owned());
            let name = name.trim().to_string();
            if name.is_empty() || password.is_empty() {
                return;
            }
            let find = ui.global::<Find>();
            find.set_signing_in(true);
            find.set_sign_in_said(Default::default());
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
        find.on_sign_out(move || {
            app.activity.bump();
            let Some(ui) = weak.upgrade() else { return };
            opensubtitles::account::sign_out();
            let find = ui.global::<Find>();
            find.set_account(Default::default());
            find.set_left(-1);
        });
    }
}

/// The languages a search asks for: the one the player speaks, and English
/// beside it, which more films have than any other. In OpenSubtitles' own
/// codes, which split Portuguese in two where the player has one.
fn search_languages() -> Vec<Language> {
    let mut codes: Vec<Language> = match dbm_lang::spoken() {
        "pt" => ["pt-br", "pt-pt"]
            .into_iter()
            .filter_map(Language::parse)
            .collect(),
        code => Language::parse(code).into_iter().collect(),
    };
    codes.extend(Language::parse("en"));
    codes.sort_by(|a, b| a.tag().cmp(b.tag()));
    codes.dedup_by(|a, b| a.tag() == b.tag());
    codes
}
