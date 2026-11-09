//! The Rust side of the interface.
//!
//! `app.slint` declares what can be pressed and what is shown; this is where
//! each press becomes a command and each piece of state reaches a property.
//!
//! * `actions` binds every callback the interface declares, and `find` and
//!   `subsync` take the answers to the two that start work off the frame:
//!   a search for subtitles and a sync.
//! * `sync` pushes the mirrored state and the track lists out to it, and
//!   `playlist_panel` and `sliders` build the models its two big panels show.
//! * `glass` reads each surface's rect back out of the layout for the pipeline,
//!   and `fades` the ends of each scrolling list. `squircle` draws the glass's
//!   corner for the shapes Slint puts on it.
//! * `chrome` is the idle clock the bar and the pointer hide on.
//! * `warm` draws each panel once, unseen, so its first opening is smooth.
//! * `scale` sizes the whole interface at the multiple the settings choose.
//! * `subline` places the subtitle line, which the person and the bar both get
//!   a say in.
//! * `format` writes every figure the interface shows, and `unplayable` why
//!   a file wouldn't play.

pub mod actions;
pub mod chrome;
pub mod fades;
pub mod find;
pub mod format;
pub mod glass;
pub mod heads;
pub mod playlist_panel;
pub mod scale;
pub mod sliders;
pub mod squircle;
pub mod subline;
pub mod subsync;
pub mod sync;
pub mod unplayable;
pub mod warm;

use slint::ComponentHandle;

use crate::MainWindow;

/// Put one sentence on screen, in the action flash's capsule, since a packaged
/// build has no console, and nobody reads its log as it happens.
pub fn say(ui: &MainWindow, text: String) {
    ui.global::<crate::Flash>().invoke_notice(text.into());
}

/// Say that something did not happen: the same sentence, with the warning
/// mark before it. For a failure, never for news.
pub fn warn(ui: &MainWindow, text: String) {
    ui.global::<crate::Flash>().invoke_warn(text.into());
}
