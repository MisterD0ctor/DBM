//! Connecting UI intentions to player commands.
//!
//! Every callback the UI declares is bound here and nowhere else, one file
//! per area of the interface, so the set of things it can ask for is a few
//! readable lists. The handlers themselves stay thin: decide *what*, hand off
//! to `commands` for *how*.
//!
//! All of this runs on the UI thread, so an `Rc` is enough.

use std::cell::Cell;
use std::rc::Rc;

use dbm_mpv::{Direction, MediaPath};
use slint::ComponentHandle;

use crate::app::App;
use crate::interface::squircle;
use crate::playback::commands;
use crate::settings::Before;
use crate::{MainWindow, Motion, Playback, Shapes, Subtitles};

/// What the last reset replaced, so its page can offer it back. One slot: a
/// page shows its undo only until it is left, and only one page is ever open.
type Undo = Rc<Cell<Option<Before>>>;

/// Bind a callback that is a single command.
///
/// Not counted as activity here: the interface counts the input that asked,
/// and a key whose own answer is enough, like a seek's timeline or a track's
/// flash, wakes no bar (`key-wake`).
macro_rules! on {
    ($ui:expr, $app:expr, $setter:ident, |$m:ident $(, $arg:ident)*| $body:expr) => {{
        let mpv = $app.mpv.clone();
        $ui.$setter(move |$($arg),*| {
            let $m = &*mpv;
            $body
        });
    }};
}

/// Bind a callback that needs more of the player than mpv: the closure is
/// handed its own clone of the `App`, as `app`.
macro_rules! on_app {
    ($ui:expr, $app:expr, $setter:ident, |$a:ident $(, $arg:ident)*| $body:expr) => {{
        let $a = $app.clone();
        $ui.$setter(move |$($arg),*| $body);
    }};
}

mod opening;
mod settings;
mod subtitles;
mod timeline;
mod tracks;
mod transport;
mod window;

pub use window::follow_fullscreen;

pub fn wire(ui: &MainWindow, app: &App) {
    push_constants(ui);
    // First, since some of what the wiring hands the interface is words.
    settings::restore_language(ui, app);
    ui.global::<Shapes>().on_squircle(squircle::image);
    let undo: Undo = Rc::new(Cell::new(None));

    transport::wire_transport(ui, app);
    timeline::wire_timeline(ui, app);
    tracks::wire_tracks(ui, app);
    settings::wire_settings(ui, app, &undo);
    subtitles::wire_subtitles(ui, app, &undo);
    opening::wire_opening(ui, app);
    window::wire_video_clicks(ui, app);
    window::wire_window(ui);
    settings::restore_preferences(ui, app);
    watch_motion(ui);
}

/// Tell `Motion` whether the desktop asks for less motion, now and whenever
/// that changes.
fn watch_motion(ui: &MainWindow) {
    let weak = ui.as_weak();
    crate::platform::motion::watch(move |still| {
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.global::<Motion>().set_system_still(still);
        });
    });
}

/// A step the interface asks for, which it says as -1 or +1.
fn way(step: crate::Way) -> Direction {
    match step {
        crate::Way::Forward => Direction::Forward,
        crate::Way::Back => Direction::Back,
    }
}

/// Which kind of track a menu or a key means.
fn pick(kind: crate::Pick) -> commands::Pick {
    match kind {
        crate::Pick::Subtitles => commands::Pick::Subtitles,
        crate::Pick::Audio => commands::Pick::Audio,
    }
}

/// The file playing, as the interface was last told it, for a handler that
/// has only the window to ask. `None` with nothing open.
fn playing(ui: &MainWindow) -> Option<MediaPath> {
    let path = ui.global::<Playback>().get_path();
    (!path.is_empty()).then(|| MediaPath::from(path.as_str()))
}

/// Values `commands` owns, handed to the UI so they are defined once.
fn push_constants(ui: &MainWindow) {
    let playback = ui.global::<Playback>();
    let subtitles = ui.global::<Subtitles>();
    playback.set_volume_max(commands::VOLUME_MAX as f32);
    playback.set_seek_step(commands::SEEK_STEP as f32);
    playback.set_volume_step(commands::VOLUME_STEP as f32);
    subtitles.set_delay_step(commands::DELAY_STEP as f32);
    playback.set_scroll_seek_step(commands::SCROLL_SEEK_STEP as f32);
    playback.set_fine_seek_step(commands::FINE_SEEK_STEP as f32);
    subtitles.set_scale_step(commands::SUB_SCALE_STEP as f32);
    subtitles.set_pos_step(commands::SUB_POS_STEP as f32);
}
