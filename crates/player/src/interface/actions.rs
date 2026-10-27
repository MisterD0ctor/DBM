//! Connecting UI intentions to player commands.
//!
//! Every callback the UI declares is bound here and nowhere else, so the set
//! of things the interface can ask for is one readable list. The handlers
//! themselves stay thin: decide *what*, hand off to `commands` for *how*.
//!
//! All of this runs on the UI thread, so an `Rc` is enough.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;

use crate::app::App;
use crate::interface::{sliders, squircle};
use crate::platform::dialog;
use crate::playback::commands;
use crate::settings::{Before, Section, REGISTRY};
use crate::{MainWindow, Motion, Shapes, Tip};

/// A click on the video is held this long so a double-click can cancel it,
/// rather than pausing and unpausing on the way to fullscreen.
const DOUBLE_CLICK_GRACE: Duration = Duration::from_millis(250);

/// What the last reset replaced, so its page can offer it back. One slot: a
/// page shows its undo only until it is left, and only one page is ever open.
type Undo = Rc<Cell<Option<Before>>>;

/// Bind a callback that is a single command.
///
/// Every action also counts as activity, so working the toolbar keeps the
/// chrome on screen even if the pointer never moves.
macro_rules! on {
    ($ui:expr, $app:expr, $setter:ident, |$m:ident $(, $arg:ident)*| $body:expr) => {{
        let app = $app.clone();
        $ui.$setter(move |$($arg),*| {
            app.activity.bump();
            let $m = &*app.mpv;
            $body
        });
    }};
}

pub fn wire(ui: &MainWindow, app: &App) {
    push_constants(ui);
    // First, since some of what the wiring hands the interface is words.
    restore_language(ui, app);
    ui.global::<Shapes>().on_squircle(squircle::image);
    let undo: Undo = Rc::new(Cell::new(None));

    wire_transport(ui, app);
    wire_timeline(ui, app);
    wire_tracks(ui, app);
    wire_settings(ui, app, &undo);
    wire_subtitles(ui, app, &undo);
    wire_opening(ui, app);
    wire_video_clicks(ui, app);
    wire_window(ui);
    restore_preferences(ui, app);
}

/// Values `commands` owns, handed to the UI so they are defined once.
fn push_constants(ui: &MainWindow) {
    ui.set_volume_max(commands::VOLUME_MAX as f32);
    ui.set_seek_step(commands::SEEK_STEP as f32);
    ui.set_volume_step(commands::VOLUME_STEP as f32);
    ui.set_delay_step(commands::DELAY_STEP as f32);
    ui.set_scroll_seek_step(commands::SCROLL_SEEK_STEP as f32);
    ui.set_fine_seek_step(commands::FINE_SEEK_STEP as f32);
    ui.set_sub_scale_step(commands::SUB_SCALE_STEP as f32);
    ui.set_sub_pos_step(commands::SUB_POS_STEP as f32);
}

/// The bar's own controls and what lies just beyond them: pause, mute,
/// panscan, the end of a file, the playlist, speed, frames and chapters.
fn wire_transport(ui: &MainWindow, app: &App) {
    on!(ui, app, on_toggle_pause, |m| commands::toggle_pause(m));
    on!(ui, app, on_toggle_mute, |m| commands::toggle_mute(m));
    on!(ui, app, on_toggle_panscan, |m| commands::toggle_panscan(m));
    on!(ui, app, on_restart, |m| commands::restart(m));
    // What the end of a file offers: this one again, or the next one.
    on!(ui, app, on_replay, |m| commands::replay(m));
    // Every way of asking for another file on purpose goes through
    // `arrival`, which holds one with a place saved in it for the choice.
    {
        let app = app.clone();
        ui.on_play_next(move || {
            app.activity.bump();
            app.arrival.step(&app.mpv, 1, Some(true));
        });
    }
    on!(ui, app, on_nudge_volume, |m, d| commands::nudge_volume(
        m, d as f64
    ));
    on!(ui, app, on_set_volume, |m, v| commands::set_volume(
        m, v as f64
    ));
    {
        let app = app.clone();
        ui.on_playlist_step(move |delta| {
            app.activity.bump();
            app.arrival.step(&app.mpv, delta, None);
        });
    }
    {
        let app = app.clone();
        ui.on_play_index(move |index| {
            app.activity.bump();
            app.arrival.play_index(&app.mpv, i64::from(index), None);
        });
    }
    // The corner pill's offer, for a file autoplay started over.
    {
        let app = app.clone();
        ui.on_take_resume_offer(move || {
            app.activity.bump();
            app.arrival.take_offer(&app.mpv);
        });
    }
    // The centre pill's answer, for a file held on its saved place.
    {
        let app = app.clone();
        ui.on_arrival_answer(move |from_the_start| {
            app.activity.bump();
            app.arrival.answer(&app.mpv, from_the_start);
        });
    }

    // Speed steps from the value the interface is showing, so the ladder is
    // decided here and mpv is only told where to land.
    on!(ui, app, on_step_speed, |m, current, direction| {
        commands::step_speed(m, current as f64, direction)
    });
    on!(ui, app, on_reset_speed, |m| commands::reset_speed(m));
    on!(ui, app, on_frame_step, |m, direction| commands::frame_step(
        m, direction
    ));
    on!(ui, app, on_chapter_step, |m, direction| {
        commands::chapter_step(m, direction)
    });

    ui.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}

/// Seeking, scrubbing, and the preview over the timeline.
fn wire_timeline(ui: &MainWindow, app: &App) {
    on!(
        ui,
        app,
        on_seek_relative,
        |m, secs| commands::seek_relative(m, secs as f64)
    );
    // Past the title sequence, to where it was found to end, or past the
    // credits, to the scene after them.
    on!(ui, app, on_skip_to, |m, to| commands::seek_to(
        m,
        f64::from(to)
    ));
    // Release: land exactly, and drop any coalesced drag update, which is
    // now stale by definition.
    {
        let app = app.clone();
        ui.on_seek_fraction(move |f| {
            app.activity.bump();
            app.scrubber.release(&app.mpv, f);
        });
    }
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        ui.on_seek_scrub(move |f| {
            app.activity.bump();
            // From the mirrored state, the cheapest source: whether it is
            // paused, which matters on a drag's first update, and the film's
            // length, which turns distance along the track into speed.
            let (paused, duration) = weak
                .upgrade()
                .map_or((false, 0.0), |ui| (ui.get_paused(), ui.get_duration()));
            app.scrubber
                .drag_to(&app.mpv, f, paused, f64::from(duration));
        });
    }
    {
        let app = app.clone();
        ui.on_seek_held(move |f| {
            app.activity.bump();
            app.scrubber.held_at(&app.mpv, f);
        });
    }
    // Answered from what is already known (the duration and the atlas),
    // because this fires on every pointer move over the timeline, where
    // nothing may touch mpv or the disk.
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        ui.on_preview_at(move |fraction| {
            let Some(ui) = weak.upgrade() else { return };
            let (time, x, y) = app.preview.answer(fraction);
            ui.set_preview_time(time.into());
            ui.set_preview_clip_x(x);
            ui.set_preview_clip_y(y);
        });
    }
}

/// Subtitle and audio tracks.
fn wire_tracks(ui: &MainWindow, app: &App) {
    // Reads the track list before deciding, so it goes to the worker rather
    // than blocking a keypress behind mpv's core lock.
    {
        let app = app.clone();
        ui.on_toggle_subtitles(move || {
            app.activity.bump();
            app.worker.run(commands::toggle_subtitles);
        });
    }
    // -1 is the menu's "Off" row rather than a real track id.
    on!(ui, app, on_select_subtitle, |m, id| {
        commands::set_subtitle_track(m, (id >= 0).then_some(id as i64))
    });
    // The one selection the watchdog has to hear about: choosing "Off" is
    // the only silence it must not undo when a device reappears.
    {
        let app = app.clone();
        ui.on_select_audio(move |id| {
            app.activity.bump();
            let id = (id >= 0).then_some(id as i64);
            app.audio.note_user_selection(id);
            commands::set_audio_track(&app.mpv, id);
        });
    }
    on!(ui, app, on_cycle_track, |m, subtitles, direction| {
        commands::cycle_track(m, subtitles, direction)
    });
    // A track out, and a fetched one forgotten with it so it doesn't come
    // back next time the film opens. Reads the track list, so on the worker.
    {
        let app = app.clone();
        ui.on_remove_track(move |id, subtitle| {
            app.activity.bump();
            app.worker.run(move |mpv| {
                let Some(file) = commands::remove_track(mpv, id as i64, subtitle) else {
                    return;
                };
                if let Some(path) = mpv.get_property("path") {
                    crate::library::opensubtitles::forget(&path, &file);
                }
            });
        });
    }
    wire_finding(ui, app);
}

/// Subtitles from OpenSubtitles: the search the menu's last subtitle row
/// starts, and the result a row of its answer fetches. Both are network
/// round trips on threads of their own; the answers come back as
/// completions, which is where the rows are filled and the file loaded.
fn wire_finding(ui: &MainWindow, app: &App) {
    use crate::library::opensubtitles;
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
            opensubtitles::search(path, search_languages(), app.worker.reporter());
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
            opensubtitles::fetch(path, hit, app.worker.reporter());
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
            opensubtitles::account::sign_in(name, password, app.worker.reporter());
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
    let mut codes: Vec<String> = match crate::lang::spoken() {
        "pt" => vec!["pt-br".into(), "pt-pt".into()],
        code => vec![code.to_string()],
    };
    codes.push("en".into());
    codes.sort();
    codes.dedup();
    codes
}

/// The glass and ambient border pages: sliders, resets and their undo, and
/// the two switches beside them.
fn wire_settings(ui: &MainWindow, app: &App, undo: &Undo) {
    // Sliders report a normalised position; the registry owns the range.
    // The model is updated here so the readout follows the drag.
    let models = Rc::new(sliders::ParamModels::build(ui, &app.settings));
    {
        let (app, models) = (app.clone(), models.clone());
        ui.on_set_param(move |index, fraction| {
            app.activity.bump();
            let index = index as usize;
            let Some(param) = REGISTRY.get(index) else {
                return;
            };
            app.settings.set(
                index,
                param.min + fraction.clamp(0.0, 1.0) * (param.max - param.min),
            );
            models.update(&app.settings, index);
        });
    }
    {
        let (app, models, undo) = (app.clone(), models.clone(), undo.clone());
        ui.on_reset_section(move |section| {
            app.activity.bump();
            if let Some(section) = Section::from_index(section) {
                undo.set(Some(app.settings.before()));
                app.settings.reset(section);
                // Every row moved and no drag can be in progress, so a full
                // refresh is both safe and simplest.
                models.refresh_all(&app.settings);
            }
        });
    }
    // Resets as the panels number them: 1 glass, 2 ambient border, and from
    // the subtitles menu 4 their size and position. 3, their timing, is put
    // back by the menu itself, since it was never a setting.
    {
        let (app, models, undo) = (app.clone(), models.clone(), undo.clone());
        ui.on_undo_reset(move |page| {
            app.activity.bump();
            let Some(was) = undo.take() else {
                return;
            };
            match page {
                1 => app.settings.restore(Section::Glass, &was),
                2 => app.settings.restore(Section::Border, &was),
                4 => {
                    app.settings
                        .set_sub_scale(
                            commands::set_sub_scale(&app.mpv, was.sub_scale as f64) as f32
                        );
                    app.subline.set(&app.mpv, was.sub_pos as f64);
                }
                _ => {}
            }
            models.refresh_all(&app.settings);
        });
    }

    // Effect toggles, separate from the sliders rather than a zero on one,
    // so the tuning survives switching an effect off.
    {
        let app = app.clone();
        ui.on_set_ambience(move |on| {
            app.activity.bump();
            app.settings.set_ambience(on);
        });
    }
    {
        let app = app.clone();
        ui.on_set_autoplay(move |on| {
            app.activity.bump();
            app.settings.set_autoplay(on);
            commands::set_autoplay(&app.mpv, on);
        });
    }
    // `Motion.enabled` is already set on the Slint side, so this saves the
    // choice, which the driver also hands the pipeline for the backdrop's
    // fade.
    {
        let app = app.clone();
        ui.on_set_animations(move |on| {
            app.activity.bump();
            app.settings.set_animations(on);
        });
    }
    // Saved, and handed the pipeline by the driver with the material.
    {
        let app = app.clone();
        ui.on_set_legible_glass(move |on| {
            app.activity.bump();
            app.settings.set_legible_glass(on);
        });
    }
    // The tooltips' two switches, likewise already acted on by `Tip`.
    {
        let app = app.clone();
        ui.on_set_tips(move |on| {
            app.activity.bump();
            app.settings.set_tips(on);
        });
    }
    {
        let app = app.clone();
        ui.on_set_tip_keys(move |on| {
            app.activity.bump();
            app.settings.set_tip_keys(on);
        });
    }

    // The rows of the language page, as it numbers them: the system's, then
    // English, then each language built in. The row is already lit by the
    // time this runs; what is left is speaking it and saving it.
    {
        let (app, models, weak) = (app.clone(), models.clone(), ui.as_weak());
        ui.on_choose_language(move |row| {
            app.activity.bump();
            let code = language_of(row);
            app.settings.set_language(&code);
            crate::lang::speak(&code);
            if let Some(ui) = weak.upgrade() {
                models.say_again(&ui, &app.settings);
            }
        });
    }
}

/// The subtitles page: timing, size and placement, and lining the timing up
/// with the film's speech.
///
/// Size and placement are nudged from the stored value rather than the
/// mirrored one, because the store is what gets saved, and one source keeps
/// them from drifting apart.
fn wire_subtitles(ui: &MainWindow, app: &App, undo: &Undo) {
    on!(ui, app, on_nudge_sub_delay, |m, d| {
        commands::nudge_sub_delay(m, d as f64)
    });
    {
        let app = app.clone();
        ui.on_nudge_sub_scale(move |delta| {
            app.activity.bump();
            let sent =
                commands::set_sub_scale(&app.mpv, app.settings.sub_scale() as f64 + delta as f64);
            app.settings.set_sub_scale(sent as f32);
        });
    }
    {
        let app = app.clone();
        ui.on_nudge_sub_pos(move |delta| {
            app.activity.bump();
            app.subline
                .set(&app.mpv, app.settings.sub_pos() as f64 + delta as f64);
        });
    }
    {
        let (app, undo) = (app.clone(), undo.clone());
        ui.on_reset_subtitles(move || {
            app.activity.bump();
            undo.set(Some(app.settings.before()));
            app.settings
                .set_sub_scale(
                    commands::set_sub_scale(&app.mpv, commands::SUB_SCALE_DEFAULT) as f32,
                );
            app.subline.set(&app.mpv, commands::SUB_POS_DEFAULT);
        });
    }
    // Listening takes seconds and runs on a thread of its own; the answer
    // comes back as a completion, which is where the timing is applied.
    {
        let app = app.clone();
        ui.on_sync_subtitles(move || {
            app.activity.bump();
            crate::library::subsync::spawn(app.mpv.clone(), app.worker.reporter());
        });
    }
    {
        let app = app.clone();
        ui.on_set_timing(move |delay, speed| {
            app.activity.bump();
            commands::set_sub_delay(&app.mpv, delay as f64);
            commands::set_sub_speed(&app.mpv, speed as f64);
            // Saved at once, as a sync is.
            crate::playback::session::checkpoint(&app.mpv);
        });
    }
}

/// Opening a file or a folder.
///
/// The dialog runs on its own thread and comes back through `open-path`,
/// which is also where a dropped file and the command line arrive: by the
/// time anything reaches here it is just a path.
fn wire_opening(ui: &MainWindow, app: &App) {
    {
        let (app, weak) = (app.clone(), ui.as_weak());
        ui.on_open_file(move || {
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
        ui.on_open_folder(move || {
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
        ui.on_open_path(move |path, ask| {
            app.activity.bump();
            let path = std::path::PathBuf::from(path.as_str());
            // Said before handing it over, since everything past here is on
            // another thread (a `read_dir` that can take seconds on a network
            // share, an m3u write, a `loadlist`, mpv's own open), and would
            // otherwise look like an ignored click. Set here, the one funnel
            // they share, so a drop and the command line get it too.
            if let Some(ui) = weak.upgrade() {
                ui.set_opening_name(opening_name(&path).into());
                ui.set_opening(true);
            }
            // Same job the command line goes through, scan and all.
            app.worker.submit(move |_mpv| {
                Some(crate::worker::Completion::Opened(
                    crate::library::playlist::prepare(&path)
                        .map(|prepared| crate::library::playlist::Prepared { ask, ..prepared }),
                ))
            });
        });
    }
}

/// Click to pause, double-click for fullscreen.
///
/// The pause is deferred until it is clear no second click is coming. Both
/// handlers bump a token; the deferred action runs only if it still owns the
/// value it was scheduled with.
fn wire_video_clicks(ui: &MainWindow, app: &App) {
    let token = Rc::new(Cell::new(0u64));

    {
        let (app, token) = (app.clone(), token.clone());
        ui.on_video_clicked(move || {
            app.activity.bump();
            let mine = token.get().wrapping_add(1);
            token.set(mine);
            let (token, mpv) = (token.clone(), app.mpv.clone());
            slint::Timer::single_shot(DOUBLE_CLICK_GRACE, move || {
                if token.get() == mine {
                    commands::toggle_pause(&mpv);
                }
            });
        });
    }

    {
        let (app, weak, token) = (app.clone(), ui.as_weak(), token.clone());
        ui.on_video_double_clicked(move || {
            app.activity.bump();
            // Invalidate whatever single click is pending.
            token.set(token.get().wrapping_add(1));
            if let Some(ui) = weak.upgrade() {
                set_fullscreen(&ui, !ui.window().is_fullscreen());
            }
        });
    }
}

/// Minimizing, from the window's own button, and fullscreen, which is the
/// window's business rather than mpv's, since with `vo=libmpv` there is no
/// mpv-owned window. Maximizing is the interface's, through `maximized`.
fn wire_window(ui: &MainWindow) {
    {
        let weak = ui.as_weak();
        ui.on_minimize(move || {
            if let Some(ui) = weak.upgrade() {
                crate::platform::window::minimize(ui.window());
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_toggle_fullscreen(move || {
            if let Some(ui) = weak.upgrade() {
                set_fullscreen(&ui, !ui.window().is_fullscreen());
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_leave_fullscreen(move || {
            if let Some(ui) = weak.upgrade() {
                set_fullscreen(&ui, false);
            }
        });
    }
}

/// The language a row of the language page stands for, by code: nothing for
/// the system's, which is the first row, and English for the second.
fn language_of(row: i32) -> String {
    match row {
        0 => String::new(),
        1 => "en".into(),
        n => crate::lang::codes()
            .nth((n - 2).max(0) as usize)
            .unwrap_or_default()
            .to_string(),
    }
}

/// The row that stands for a saved choice. A language this build no longer
/// has is the system's row, which is also how it is spoken.
fn row_of(code: &str) -> i32 {
    match code {
        "" => 0,
        "en" => 1,
        code => crate::lang::codes()
            .position(|known| known == code)
            .map_or(0, |place| place as i32 + 2),
    }
}

/// Put what was saved where it acts: the switches on the settings page, and
/// the preferences mpv has to be told about.
fn restore_preferences(ui: &MainWindow, app: &App) {
    let settings = &app.settings;
    ui.set_ambience_on(settings.ambience_on());
    ui.set_autoplay(settings.autoplay());
    ui.global::<Motion>().set_enabled(settings.animations());
    ui.set_legible_glass(settings.legible_glass());
    let tip = ui.global::<Tip>();
    tip.set_enabled(settings.tips());
    tip.set_with_keys(settings.tip_keys());
    commands::set_autoplay(&app.mpv, settings.autoplay());
    commands::set_sub_scale(&app.mpv, settings.sub_scale() as f64);
    app.subline.set(&app.mpv, settings.sub_pos() as f64);
}

/// Speak the language that was saved, and list the ones it can be changed to.
fn restore_language(ui: &MainWindow, app: &App) {
    // Each language by its own name for itself, which is the one name for it
    // that its speakers can read whatever the interface is in.
    ui.set_language_names(slint::ModelRc::new(slint::VecModel::from(
        crate::lang::codes()
            .map(|code| crate::library::naming::language_name(code).into())
            .collect::<Vec<slint::SharedString>>(),
    )));
    let language = app.settings.language();
    ui.set_language_choice(row_of(&language));
    crate::lang::speak(&language);
}

/// Set the window state and the mirrored flag together, so the toolbar icon
/// cannot disagree with the window.
fn set_fullscreen(ui: &MainWindow, on: bool) {
    ui.window().set_fullscreen(on);
    ui.set_fullscreen(on);
}

/// The file the dialogs should open near: the one playing, or failing that
/// the one the way in offers to continue.
fn near(ui: &MainWindow) -> Option<std::path::PathBuf> {
    [ui.get_current_path(), ui.get_resume_path()]
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
    let name = if crate::library::playlist::is_video_file(path) {
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
