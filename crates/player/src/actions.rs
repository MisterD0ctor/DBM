//! Connecting UI intentions to player commands.
//!
//! Every callback the UI declares is bound here and nowhere else, so the set
//! of things the interface can ask for is one readable list. The handlers
//! themselves stay thin: decide *what*, hand off to `commands` for *how*.
//!
//! All of this runs on the UI thread, so an `Rc` is sufficient — no
//! synchronisation, no channels.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::ComponentHandle;

use crate::chrome::Activity;
use crate::commands;
use crate::mpv::Mpv;
use crate::scrub::Scrubber;
use crate::settings::Store;
use crate::worker::Worker;
use crate::sync;
use crate::MainWindow;

/// A click on the video is held this long so a double-click can cancel it.
/// Without the delay, double-clicking to go fullscreen pauses and unpauses on
/// the way there.
const DOUBLE_CLICK_GRACE: Duration = Duration::from_millis(250);

pub fn wire(
    ui: &MainWindow,
    mpv: &Arc<Mpv>,
    activity: &Activity,
    scrubber: &Rc<Scrubber>,
    worker: &Rc<Worker>,
    params: &Rc<Store>,
    preview: &Rc<crate::preview::Preview>,
    audio: &Rc<crate::audio::Watchdog>,
) {
    push_constants(ui);

    // Every action also counts as activity, so working the toolbar keeps the
    // chrome on screen even if the pointer never moves.
    macro_rules! on {
        ($setter:ident, |$m:ident $(, $arg:ident)*| $body:expr) => {{
            let $m = mpv.clone();
            let seen = activity.clone();
            ui.$setter(move |$($arg),*| {
                seen.bump();
                let $m = &*$m;
                $body
            });
        }};
    }

    on!(on_toggle_pause, |m| commands::toggle_pause(m));
    on!(on_toggle_mute, |m| commands::toggle_mute(m));
    on!(on_toggle_panscan, |m| commands::toggle_panscan(m));
    // Reads the track list before deciding, so it goes to the worker
    // rather than blocking a keypress behind mpv's core lock.
    {
        let (worker, seen) = (worker.clone(), activity.clone());
        ui.on_toggle_subtitles(move || {
            seen.bump();
            worker.run(commands::toggle_subtitles);
        });
    }
    on!(on_restart, |m| commands::restart(m));
    // What the end of a file offers: this one again, or the next one.
    on!(on_replay, |m| commands::replay(m));
    on!(on_play_next, |m| commands::advance(m, 1));
    on!(on_seek_relative, |m, secs| commands::seek_relative(m, secs as f64));
    // Release: land exactly, and drop any coalesced drag update, which is
    // now stale by definition.
    {
        let (mpv, scrubber, seen) = (mpv.clone(), scrubber.clone(), activity.clone());
        ui.on_seek_fraction(move |f| {
            seen.bump();
            scrubber.release(&mpv, f);
        });
    }
    {
        let (mpv, scrubber, seen, weak) =
            (mpv.clone(), scrubber.clone(), activity.clone(), ui.as_weak());
        ui.on_seek_scrub(move |f| {
            seen.bump();
            // The mirrored state is the cheapest source of the current
            // playback state, and it only matters on the first update of a
            // drag.
            let paused = weak.upgrade().is_some_and(|ui| ui.get_paused());
            scrubber.drag_to(&mpv, f, paused);
        });
    }
    // Delivery of a finished atlas, from the thread that built it.
    {
        let (preview, weak) = (preview.clone(), ui.as_weak());
        ui.on_preview_ready(move |path, tile_w, tile_h, grid| {
            let Some(ui) = weak.upgrade() else { return };
            let path = std::path::PathBuf::from(path.as_str());
            match slint::Image::load_from_path(&path) {
                Ok(image) => {
                    eprintln!("dbm: preview atlas {grid}x{grid} tiles of {tile_w}x{tile_h}");
                    ui.set_preview_sprite(image);
                    ui.set_preview_tile_w(tile_w);
                    ui.set_preview_tile_h(tile_h);
                    preview.set_sprite(Some(crate::preview::Sprite {
                        path,
                        tile_w: tile_w as u32,
                        tile_h: tile_h as u32,
                        grid: grid as u32,
                    }));
                    // The pointer may already be on the timeline, in which
                    // case it is showing a timestamp and an empty frame.
                    let (_, x, y) = preview.again();
                    ui.set_preview_clip_x(x);
                    ui.set_preview_clip_y(y);
                }
                Err(e) => eprintln!("dbm: preview atlas unreadable: {e:?}"),
            }
        });
    }

    // Answered from what is already known — the duration and the atlas —
    // because this fires on every pointer move over the timeline and nothing
    // on that path may touch mpv or the disk.
    {
        let (preview, weak) = (preview.clone(), ui.as_weak());
        ui.on_preview_at(move |fraction| {
            let Some(ui) = weak.upgrade() else { return };
            let (time, x, y) = preview.answer(fraction);
            ui.set_preview_time(time.into());
            ui.set_preview_clip_x(x);
            ui.set_preview_clip_y(y);
        });
    }
    on!(on_nudge_volume, |m, d| commands::nudge_volume(m, d as f64));
    on!(on_set_volume, |m, v| commands::set_volume(m, v as f64));
    on!(on_nudge_sub_delay, |m, d| commands::nudge_sub_delay(m, d as f64));
    on!(on_playlist_step, |m, d| commands::playlist_step(m, d));
    on!(on_play_index, |m, i| commands::playlist_play(m, i as i64));
    // -1 is the menu's "Off" row rather than a real track id.
    on!(on_select_subtitle, |m, id| commands::set_subtitle_track(
        m,
        (id >= 0).then_some(id as i64)
    ));
    // The one selection the watchdog has to hear about: choosing "Off" is
    // the only silence it must not undo when a device reappears.
    {
        let (mpv, seen, audio) = (mpv.clone(), activity.clone(), audio.clone());
        ui.on_select_audio(move |id| {
            seen.bump();
            let id = (id >= 0).then_some(id as i64);
            audio.note_user_selection(id);
            commands::set_audio_track(&mpv, id);
        });
    }

    // Sliders report a normalised position; the registry owns the range.
    // The model is rebuilt here so the readout follows the drag.
    {
        let models = Rc::new(sync::ParamModels::build(ui, params));
        {
            let (params, seen, models) = (params.clone(), activity.clone(), models.clone());
            ui.on_set_param(move |index, fraction| {
                seen.bump();
                let index = index as usize;
                let Some(param) = crate::settings::REGISTRY.get(index) else {
                    return;
                };
                params.set(
                    index,
                    param.min + fraction.clamp(0.0, 1.0) * (param.max - param.min),
                );
                models.update(&params, index);
            });
        }
        {
            let (params, seen, models) = (params.clone(), activity.clone(), models.clone());
            ui.on_reset_section(move |section| {
                seen.bump();
                if let Some(section) = crate::settings::Section::from_index(section) {
                    params.reset(section);
                    // Every row moved and no drag can be in progress, so a
                    // full refresh is both safe and simplest.
                    models.refresh_all(&params);
                }
            });
        }
    }

    // Effect toggles. The tuning behind an effect survives switching it off,
    // which is why these are separate from the sliders rather than a zero
    // value on one of them.
    {
        let (params, seen) = (params.clone(), activity.clone());
        ui.on_set_ambience(move |on| {
            seen.bump();
            params.set_ambience(on);
        });
    }
    {
        let (params, seen, mpv) = (params.clone(), activity.clone(), mpv.clone());
        ui.on_set_autoplay(move |on| {
            seen.bump();
            params.set_autoplay(on);
            commands::set_autoplay(&mpv, on);
        });
    }

    // Opening. The dialog runs on its own thread and comes back through
    // `open-path`, which is also where a future drag-and-drop would arrive:
    // by the time anything reaches here it is just a path.
    {
        let (seen, weak) = (activity.clone(), ui.as_weak());
        ui.on_open_file(move || {
            seen.bump();
            crate::dialog::pick(crate::dialog::Want::File, weak.clone());
        });
    }
    {
        let (seen, weak) = (activity.clone(), ui.as_weak());
        ui.on_open_folder(move || {
            seen.bump();
            crate::dialog::pick(crate::dialog::Want::Folder, weak.clone());
        });
    }
    {
        let (worker, seen) = (worker.clone(), activity.clone());
        let weak = ui.as_weak();
        ui.on_open_path(move |path| {
            seen.bump();
            let path = std::path::PathBuf::from(path.as_str());
            // Say so before handing it over. Everything past this point is on
            // another thread — a `read_dir` that the playlist module warns can
            // take seconds on a network share, an m3u write, a `loadlist`, and
            // then mpv's own open — and for all of it the interface used to be
            // indistinguishable from one that had ignored the click.
            //
            // Set here rather than in the dialog handlers so a drop and the
            // command line get it too: this is the one funnel they share.
            if let Some(ui) = weak.upgrade() {
                ui.set_opening_name(opening_name(&path).into());
                ui.set_opening(true);
            }
            // Same job the command line goes through, scan and all.
            worker.submit(move |_mpv| {
                Some(crate::worker::Completion::Opened(crate::playlist::prepare(
                    &path,
                )))
            });
        });
    }

    // Subtitle size and placement. Nudged from the stored value rather than
    // from the mirrored one: the store is what gets saved, and driving both
    // from the same number is what keeps them from drifting apart.
    {
        let (params, seen, mpv) = (params.clone(), activity.clone(), mpv.clone());
        ui.on_nudge_sub_scale(move |delta| {
            seen.bump();
            let sent = commands::set_sub_scale(&mpv, params.sub_scale() as f64 + delta as f64);
            params.set_sub_scale(sent as f32);
        });
    }
    {
        let (params, seen, mpv) = (params.clone(), activity.clone(), mpv.clone());
        ui.on_nudge_sub_pos(move |delta| {
            seen.bump();
            let sent = commands::set_sub_pos(&mpv, params.sub_pos() as f64 + delta as f64);
            params.set_sub_pos(sent as f32);
        });
    }
    {
        let (params, seen, mpv) = (params.clone(), activity.clone(), mpv.clone());
        ui.on_reset_subtitles(move || {
            seen.bump();
            // One row rather than three, matching the settings pages. The
            // delay goes back to zero too even though it is not saved here -
            // "reset" on a page means the whole page.
            commands::set_sub_delay(&mpv, 0.0);
            params.set_sub_scale(commands::set_sub_scale(&mpv, commands::SUB_SCALE_DEFAULT) as f32);
            params.set_sub_pos(commands::set_sub_pos(&mpv, commands::SUB_POS_DEFAULT) as f32);
        });
    }

    ui.set_ambience_on(params.ambience_on());
    ui.set_autoplay(params.autoplay());
    commands::set_autoplay(mpv, params.autoplay());
    commands::set_sub_scale(mpv, params.sub_scale() as f64);
    commands::set_sub_pos(mpv, params.sub_pos() as f64);

    wire_video_clicks(ui, mpv, activity);
    wire_fullscreen(ui);
}

/// Values `commands` owns, handed to the UI so they are defined once.
fn push_constants(ui: &MainWindow) {
    ui.set_volume_max(commands::VOLUME_MAX as f32);
    ui.set_seek_step(commands::SEEK_STEP as f32);
    ui.set_volume_step(commands::VOLUME_STEP as f32);
    ui.set_delay_step(commands::DELAY_STEP as f32);
    ui.set_scroll_seek_step(commands::SCROLL_SEEK_STEP as f32);
    ui.set_sub_scale_step(commands::SUB_SCALE_STEP as f32);
    ui.set_sub_pos_step(commands::SUB_POS_STEP as f32);
}

/// Click to pause, double-click for fullscreen.
///
/// The pause is deferred until it is clear no second click is coming. Both
/// handlers bump a token; the deferred action runs only if it still owns the
/// value it was scheduled with.
fn wire_video_clicks(ui: &MainWindow, mpv: &Arc<Mpv>, activity: &Activity) {
    let token = Rc::new(Cell::new(0u64));

    {
        let (mpv, token, seen) = (mpv.clone(), token.clone(), activity.clone());
        ui.on_video_clicked(move || {
            seen.bump();
            let mine = token.get().wrapping_add(1);
            token.set(mine);
            let (token, mpv) = (token.clone(), mpv.clone());
            slint::Timer::single_shot(DOUBLE_CLICK_GRACE, move || {
                if token.get() == mine {
                    commands::toggle_pause(&mpv);
                }
            });
        });
    }

    {
        let (weak, token, seen) = (ui.as_weak(), token.clone(), activity.clone());
        ui.on_video_double_clicked(move || {
            seen.bump();
            // Invalidate whatever single click is pending.
            token.set(token.get().wrapping_add(1));
            if let Some(ui) = weak.upgrade() {
                set_fullscreen(&ui, !ui.window().is_fullscreen());
            }
        });
    }
}

/// Fullscreen is the window's business, not mpv's — with `vo=libmpv` there is
/// no mpv-owned window to put into a fullscreen state.
fn wire_fullscreen(ui: &MainWindow) {
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

/// Set the window state and the mirrored flag together, so the toolbar icon
/// cannot disagree with the window.
fn set_fullscreen(ui: &MainWindow, on: bool) {
    ui.window().set_fullscreen(on);
    ui.set_fullscreen(on);
}

/// What to call the thing being opened while it is being opened.
///
/// The file's or folder's own name, not the path: the line it goes into is one
/// row wide, and the part that identifies it to the person who just picked it
/// is the end. No extension — `naming` strips it everywhere else too, and
/// "Opening S02E01.mkv…" reads like a file manager rather than a player.
fn opening_name(path: &std::path::Path) -> String {
    let name = if path.is_dir() {
        path.file_name().map(std::ffi::OsStr::to_string_lossy)
    } else {
        path.file_stem().map(std::ffi::OsStr::to_string_lossy)
    };
    name.map(std::borrow::Cow::into_owned)
        .unwrap_or_else(|| path.display().to_string())
}
