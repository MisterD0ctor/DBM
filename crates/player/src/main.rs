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
//! Slint's scene graph rather than in a sibling HWND the UI can never sample.
//! Everything else follows from that — the ambient border and the glass are
//! both shader passes over that texture, which is why the forked mpv
//! `//!HOOK BORDER` stage is no longer needed.
//!
//! Layout of the crate, roughly outermost-first:
//!
//! | module        | owns                                                  |
//! |---------------|-------------------------------------------------------|
//! | `mpv`         | libmpv FFI: properties, commands, events, render API  |
//! | `state`       | the mirrored copy of the properties the UI binds to   |
//! | `tracks`      | reading the list-shaped properties                    |
//! | `commands`    | actions expressed as mpv commands                     |
//! | `actions`     | binding UI callbacks to those commands                |
//! | `audio`       | putting audio back when its device comes and goes     |
//! | `sync`        | carrying values mpv → state → UI, once per frame      |
//! | `scrub`       | coalescing timeline drags into seeks mpv can keep up with |
//! | `chrome`      | the idle clock behind auto-hide                       |
//! | `render`      | driving the GPU pipeline from the rendering notifier  |
//! | `pipeline`    | the passes themselves: border, blur, glass            |
//! | `gfx`         | GL primitives the passes are built from               |
//! | `modal_loop`  | keeping frames coming while Windows owns the loop     |
//! | `diagnostics` | opt-in instrumentation                                |
//! | `naming`      | filenames and track titles into something readable    |
//! | `playlist`    | turning a path into a list of videos to play          |
//! | `shelf`       | the shape a playlist is shown in: seasons and shows   |
//! | `durations`   | how long each file is, and how far in you got         |
//! | `probe`       | what a file says about itself before it is played     |
//! | `preview`     | thumbnail atlases for the seek preview                |
//! | `dialog`      | asking the desktop for a path, off the UI thread      |
//! | `dropped`     | files dragged onto the window                         |
//! | `session`     | remembering the position and tracks of each file      |
//! | `settings`    | the registry of tunable material parameters           |
//! | `smtc`        | the OS media overlay and the buttons on a headset     |
//! | `awake`       | keeping the display on while something plays          |
//! | `paths`       | where the app keeps its own files                     |
//! | `worker`      | a thread for anything that would block the frame path |
//! | `harness`     | opt-in automated exercises                            |
//!
//! Run with a video path: `cargo run -p dbm-player -- some/video.mkv`

mod actions;
mod audio;
mod awake;
mod chrome;
mod commands;
mod cursor;
mod dialog;
mod diagnostics;
mod dropped;
mod durations;
mod gfx;
mod harness;
mod modal_loop;
mod naming;
mod mpv;
mod paths;
mod pipeline;
mod playlist;
mod preview;
mod probe;
mod render;
mod scrub;
mod session;
mod settings;
mod shelf;
mod smtc;
mod state;
mod sync;
mod tracks;
mod worker;

use std::rc::Rc;
use std::sync::Arc;

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = std::env::args().nth(1);

    let ui = MainWindow::new()?;

    // `Arc` for two reasons: the address of the `Mpv` must stay put, because
    // the render context keeps a raw pointer to the symbol table inside it;
    // and the worker thread needs a share of it.
    let mpv = Arc::new(mpv::Mpv::new(session::configure)?);
    for (name, format) in state::OBSERVED {
        if let Err(e) = mpv.observe(name, *format) {
            eprintln!("dbm: cannot observe {name}: {e}");
        }
    }

    // Anything that would block the frame path goes here — see `worker`.
    let worker = Rc::new(worker::Worker::spawn(mpv.clone(), ui.as_weak()));

    // Watches for audio devices coming and going. Observing the list is what
    // starts mpv's hotplug monitor, so this has to be armed whether or not
    // anything ever disappears.
    let audio = audio::Watchdog::new(worker.clone());
    audio::observe(&mpv);

    let activity = chrome::Activity::new();
    // Shared: `actions` feeds it drag positions, the render driver feeds it
    // the completions that let the next one go out.
    let scrubber = Rc::new(scrub::Scrubber::new());
    let params = Rc::new(settings::Store::default());
    // Shared: the render loop learns the duration and takes delivery of the
    // thumbnail atlas; `actions` answers the pointer from both.
    let preview = Rc::new(preview::Preview::default());
    // Loads what was saved, then writes changes back once they settle. The
    // defaults in `GlassParams` are now the reset target rather than the
    // source of truth.
    let saver = settings::Persister::install(params.clone(), worker.clone());
    actions::wire(
        &ui, &mpv, &activity, &scrubber, &worker, &params, &preview, &audio,
    );
    // Timers stop when their handle drops, so both of these are held until
    // the event loop returns.
    let idle_timer = chrome::install(&ui, activity);

    let mut driver = render::Driver::new(
        ui.as_weak(),
        mpv.clone(),
        file,
        scrubber,
        worker,
        params,
        preview,
        audio.clone(),
        // Registers itself from the frame path: the window handle it needs
        // does not exist yet, and will not until the loop has turned.
        smtc::Controls::new(mpv.clone()),
    );
    ui.window()
        .set_rendering_notifier(move |state, api| driver.on(state, api))?;

    // Must happen after the window exists but before the loop runs: see
    // `modal_loop` for why holding a window edge otherwise freezes the video.
    // Checkpoint the resume position while playing. mpv writes on a clean
    // quit by itself; this is what covers everything less tidy.
    let checkpoint = session::checkpoint_periodically(mpv.clone());

    ui.show()?;
    let harnesses = harness::install(&ui, &mpv, &audio);

    ui.run()?;

    drop(harnesses);
    drop(idle_timer);
    drop(checkpoint);
    drop(saver);
    Ok(())
}
