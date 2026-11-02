use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;

use crate::MainWindow;
use crate::app::App;
use crate::playback::commands;

/// A click on the video is held this long so a double-click can cancel it,
/// rather than pausing and unpausing on the way to fullscreen.
const DOUBLE_CLICK_GRACE: Duration = Duration::from_millis(250);

/// Click to pause, double-click for fullscreen.
///
/// The pause is deferred until it is clear no second click is coming. Both
/// handlers bump a token; the deferred action runs only if it still owns the
/// value it was scheduled with.
pub(super) fn wire_video_clicks(ui: &MainWindow, app: &App) {
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
        // Not activity: the interface puts the chrome away after it, as it
        // does for F (`Chrome.rest`).
        let (weak, token) = (ui.as_weak(), token.clone());
        ui.on_video_double_clicked(move || {
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
pub(super) fn wire_window(ui: &MainWindow) {
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

/// Set the window state and the mirrored flag together, so the toolbar icon
/// cannot disagree with the window.
pub(super) fn set_fullscreen(ui: &MainWindow, on: bool) {
    ui.window().set_fullscreen(on);
    ui.set_fullscreen(on);
}
