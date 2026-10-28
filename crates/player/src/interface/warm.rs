//! Drawing each panel once, unseen, before anyone opens it.
//!
//! The first time Slint draws a panel it finds fonts for the scripts in it
//! and renders its glyphs, which on this machine costs the subtitles menu's
//! first opening 18ms of its first frame and the playlist's 12ms, against 1
//! to 3ms every time after (`DBM_FIRST_OPEN` with `DBM_FRAME_LOG`). Drawn
//! once at no opacity while nothing on screen moves, where a slow frame shows
//! nowhere, a panel opens the first time as it does every time after.

use std::time::Duration;

use slint::ComponentHandle;

use crate::MainWindow;

/// The panels, numbered as `MainWindow.warming` names them.
pub const MENU: i32 = 0;
pub const AUDIO: i32 = 1;
pub const PLAYLIST: i32 = 2;
pub const SETTINGS: i32 = 3;
pub const FILES: i32 = 4;
pub const ALL: [i32; 5] = [MENU, AUDIO, PLAYLIST, SETTINGS, FILES];

/// The panels waiting to be drawn unseen.
#[derive(Default)]
pub struct WarmUp {
    due: Vec<i32>,
}

impl WarmUp {
    /// Have this panel drawn unseen soon, as it is now: at the start, and
    /// whenever what it holds has been replaced with words it may not have
    /// drawn yet.
    pub fn ask(&mut self, panel: i32) {
        if !self.due.contains(&panel) {
            self.due.push(panel);
        }
    }

    /// From the frame path: draw the next panel due in this frame, unless one
    /// is open, or something on screen is moving for the slow frame to show in.
    pub fn frame(&mut self, ui: &MainWindow) {
        if self.due.is_empty()
            || ui.get_menu_open()
            || ui.get_audio_open()
            || ui.get_playlist_open()
            || ui.get_settings_open()
            || ui.get_files_open()
            || ui.window().has_active_animations()
        {
            return;
        }
        let panel = self.due.remove(0);
        if crate::diagnostics::trace() {
            eprintln!("dbm: drawing panel {panel} unseen");
        }
        ui.set_warming(panel);
        // Taken back once this frame is drawn, so the panel, unseen as it is,
        // is never left there to catch a click.
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_warming(-1);
            }
        });
    }
}
