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
//! The architecture rests on one thing: the video arrives as a texture inside
//! Slint's scene graph rather than in a sibling window the UI can never
//! sample. Everything else follows from that — the ambient border and the
//! glass are both shader passes over that texture, which is why the forked
//! mpv `//!HOOK BORDER` stage is no longer needed.
//!
//! Layout of the crate:
//!
//! | module        | owns                                                      |
//! |---------------|-----------------------------------------------------------|
//! | `app`         | the handles the rest share, in one value                  |
//! | `driver`      | the frame: Slint's rendering notifier, and all it runs    |
//! | `playback`    | mpv: the binding, its mirrored state, commands, resuming  |
//! | `library`     | files on disk: playlists, names, lengths, preview sheets  |
//! | `interface`   | the Rust side of `app.slint`: callbacks and properties    |
//! | `gpu`         | the passes drawn over the video: border, blur, glass      |
//! | `platform`    | what each OS has to be asked for in its own way           |
//! | `worker`      | a thread for anything that would block the frame path     |
//! | `settings`    | what the person has set, kept between runs                |
//! | `paths`       | where the app keeps its own files                         |
//! | `diagnostics` | opt-in instrumentation                                    |
//! | `harness`     | opt-in automated exercises                                |
//!
//! Each folder's `mod.rs` says what is in it.
//!
//! Run with a video path: `cargo run -p dbm-player -- some/video.mkv`

mod app;
mod diagnostics;
mod driver;
mod gpu;
mod harness;
mod interface;
mod library;
mod paths;
mod platform;
mod playback;
mod settings;
mod worker;

use std::rc::Rc;
use std::sync::Arc;

use slint::ComponentHandle;

use crate::app::App;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = std::env::args().nth(1);

    let ui = MainWindow::new()?;

    // Wayland pairs a window with its desktop entry by this id; without it the
    // dock shows a generic icon and no name. It is the Flatpak's id, the
    // desktop file is named after it and the MPRIS bus name is built from it,
    // which is why the string itself lives in `mpris`. Here and not earlier:
    // it needs the platform `MainWindow::new` brings up, and is read when the
    // native window is created at `show`.
    #[cfg(target_os = "linux")]
    slint::set_xdg_app_id(platform::media_keys::mpris::APP_ID)?;

    // Set once: the answer is a `cfg`, not something that can change under a
    // running window. The interface has two lines that offer a drop and must
    // not offer one where nothing would catch it.
    ui.set_drop_supported(platform::dropped::SUPPORTED);

    let mpv = Arc::new(playback::mpv::Mpv::new(playback::session::configure)?);
    for (name, format) in playback::state::OBSERVED {
        if let Err(e) = mpv.observe(name, *format) {
            eprintln!("dbm: cannot observe {name}: {e}");
        }
    }

    let worker = Rc::new(worker::Worker::spawn(mpv.clone(), ui.as_weak()));
    let settings = Rc::new(settings::Store::default());
    let app = App {
        mpv: mpv.clone(),
        worker: worker.clone(),
        settings: settings.clone(),
        activity: interface::chrome::Activity::new(),
        scrubber: Rc::new(playback::scrub::Scrubber::new()),
        preview: Rc::new(library::preview::Preview::default()),
        // Watches for audio devices coming and going.
        audio: playback::audio::Watchdog::new(worker.clone()),
        subline: Rc::new(interface::subline::Subline::new(settings.clone())),
    };
    // Observing the device list is what starts mpv's hotplug monitor, so this
    // has to be armed whether or not anything ever disappears.
    playback::audio::observe(&mpv);

    // Loads what was saved, then writes changes back once they settle. The
    // defaults in `GlassParams` are now the reset target rather than the
    // source of truth.
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

    // Checkpoint the resume position while playing. mpv writes on a clean
    // quit by itself; this is what covers everything less tidy.
    let checkpoint = playback::session::checkpoint_periodically(mpv.clone());

    ui.show()?;
    let harnesses = harness::install(&ui, &app);

    ui.run()?;

    drop(harnesses);
    drop(idle_timer);
    drop(checkpoint);
    drop(saver);
    Ok(())
}
