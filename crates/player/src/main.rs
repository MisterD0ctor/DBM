//! Death by MPV — mpv's render API drawing into Slint's own GL context.
//!
//! Copyright (C) 2026 MisterD0ctor. This program is free software: you can
//! redistribute it and/or modify it under the terms of the GNU General Public
//! License as published by the Free Software Foundation, either version 3 of
//! the License, or (at your option) any later version. It is distributed in
//! the hope that it will be useful, but WITHOUT ANY WARRANTY; without even
//! the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
//! PURPOSE. See the GNU General Public License for more details: the full
//! text is in LICENSE, and what else ships with this player, under what
//! terms, is in THIRD-PARTY.md.
//!
//! The architecture rests on the video arriving as a texture inside Slint's
//! scene graph rather than in a sibling window the UI can't sample: the
//! ambient border and the glass are both shader passes over that texture.
//!
//! What it stands on, in crates of their own, each using only those below it
//! in this table, so none can reach back up into the interface:
//!
//! | crate         | owns                                                      |
//! |---------------|-----------------------------------------------------------|
//! | `dbm_library` | files on disk: playlists, lengths, previews, subtitles    |
//! | `dbm_gpu`     | the passes drawn over the video: border, blur, glass      |
//! | `dbm_mpv`     | libmpv: typed properties and commands, and `MediaPath`   |
//! | `dbm_naming`  | what files, tracks and chapters are called, from names    |
//! | `dbm_lang`    | the language it speaks, and the words in each             |
//!
//! Layout of this crate:
//!
//! | module        | owns                                                      |
//! |---------------|-----------------------------------------------------------|
//! | `app`         | the handles the rest share, in one value                  |
//! | `driver`      | the frame: Slint's rendering notifier, and all it runs    |
//! | `playback`    | mpv: its mirrored state, commands, resuming               |
//! | `interface`   | the Rust side of `app.slint`: callbacks and properties    |
//! | `platform`    | what each OS has to be asked for in its own way           |
//! | `worker`      | a thread for anything that would block the frame path     |
//! | `settings`    | what the person has set, kept between runs                |
//! | `logging`     | where the player's own lines go                           |
//! | `run_mode`    | whether a person is watching, or the harness is driving   |
//! | `diagnostics` | opt-in instrumentation                                    |
//! | `harness`     | automated exercises, in a build with `--features harness` |
//!
//! Each folder's `mod.rs` says what is in it.
//!
//! Run with a video path: `cargo run -p dbm-player -- some/video.mkv`

// Otherwise Windows opens a console beside the window. A debug build keeps
// it, to read the log as it is written while working on the player.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod diagnostics;
mod driver;
#[cfg(feature = "harness")]
mod harness;
mod interface;
mod logging;
mod platform;
mod playback;
mod run_mode;
mod settings;
mod worker;

use std::rc::Rc;
use std::sync::Arc;

use slint::ComponentHandle;

use crate::app::{App, Shared};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging::install();
    // `args_os`, since `args` panics on a name that isn't valid Unicode, which
    // a file manager on Linux can hand over.
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    let ui = MainWindow::new()?;

    // Wayland pairs a window with its desktop entry by this id, or the dock
    // shows a generic icon and no name. It is the Flatpak's id, which also
    // names the desktop file and the MPRIS bus, so the string lives in
    // `mpris`. Set after `MainWindow::new` brings up the platform and before
    // `show` creates the native window, which reads it.
    #[cfg(target_os = "linux")]
    slint::set_xdg_app_id(platform::media_keys::mpris::APP_ID)?;

    // Set once, since the answer is a `cfg`, so the interface only mentions
    // dropping where something catches it.
    ui.set_drop_supported(platform::dropped::SUPPORTED);

    // SAFETY: each place `libmpv_path` looks holds a libmpv, or the one the
    // person named in `DBM_LIBMPV`.
    let mpv = unsafe {
        dbm_mpv::Mpv::new(
            &playback::session::libmpv_path(),
            playback::session::configure,
        )
    };
    let mpv = Arc::new(mpv?);
    playback::session::mask_subtitles(&mpv);
    playback::state::observe(&mpv);

    let worker = Rc::new(worker::Worker::spawn(mpv.clone(), ui.as_weak()));
    dbm_library::real_paths::move_once();
    let settings = Rc::new(settings::Store::default());
    let app = App::new(Shared {
        mpv: mpv.clone(),
        worker: worker.clone(),
        settings: settings.clone(),
        activity: interface::chrome::Activity::new(),
        scrubber: playback::scrub::Scrubber::new(),
        preview: dbm_library::preview::Preview::default(),
        // Watches for audio devices coming and going.
        audio: playback::audio::Watchdog::new({
            let worker = worker.clone();
            move |remembered| {
                worker.submit(move |mpv| {
                    playback::audio::reopen(mpv, remembered).map(worker::Completion::Notice)
                });
            }
        }),
        subline: interface::subline::Subline::new(settings.clone()),
        search: interface::find::Search::default(),
        arrival: playback::arrival::Arrival::default(),
        retrace: playback::retrace::Retrace::default(),
        scale: interface::scale::Scale::default(),
    });

    // Loads what was saved, then writes changes back once they settle, so the
    // defaults in `GlassParams` are only what a reset goes back to.
    let saver = settings::Persister::install(settings, worker);
    interface::actions::wire(&ui, &app);
    // Timers stop when their handle drops, so this and the others below are
    // held until the event loop returns.
    let idle_timer = interface::chrome::install(&ui, app.activity.clone());

    let mut driver = driver::Driver::new(
        ui.as_weak(),
        app.clone(),
        file,
        // On Windows this registers itself from the frame path: the window
        // handle it needs does not exist yet and will not until the loop has
        // turned. On Linux there is no handle in it, so the bus name is
        // claimed here and now — see `mpris`.
        platform::media_keys::Controls::new(mpv.clone()),
    );
    ui.window()
        .set_rendering_notifier(move |state, api| driver.on(state, api))?;

    // Checkpoint the resume position while playing, for an end less tidy than
    // the quit below.
    let checkpoint = playback::session::checkpoint_periodically(mpv.clone());

    ui.show()?;
    #[cfg(feature = "harness")]
    let harnesses = harness::install(&ui, &app);

    ui.run()?;

    // First, so the place is on disk before anything slow.
    // SAFETY: the event loop has returned, so nothing drains mpv's events.
    unsafe { playback::session::save_before_quitting(&mpv) };
    saver.flush();

    // The window is gone by now, so a slow answer holds up nothing anyone
    // can see; OpenSubtitles asks for the token back when an app is done.
    dbm_library::opensubtitles::account::sign_out_before_quitting();

    #[cfg(feature = "harness")]
    drop(harnesses);
    drop(idle_timer);
    drop(checkpoint);
    drop(saver);
    // The file's lines are written on a thread of their own, which ends
    // with the process.
    log::logger().flush();
    // A harness run unwatched learns how it went from the exit code.
    #[cfg(feature = "harness")]
    if harness::script::failed() {
        return Err("a harness check failed".into());
    }
    Ok(())
}
