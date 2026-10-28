//! The Rust side of the interface.
//!
//! `app.slint` declares what can be pressed and what is shown; this is where
//! each press becomes a command and each piece of state reaches a property.
//!
//! * `actions` binds every callback the interface declares.
//! * `sync` pushes the mirrored state and the track lists out to it, and
//!   `playlist_panel` and `sliders` build the models its two big panels show.
//! * `glass` reads each surface's rect back out of the layout for the pipeline,
//!   and `fades` the ends of each scrolling list. `squircle` draws the glass's
//!   corner for the shapes Slint puts on it.
//! * `chrome` is the idle clock the bar and the pointer hide on.
//! * `warm` draws each panel once, unseen, so its first opening is smooth.
//! * `subline` places the subtitle line, which the person and the bar both get
//!   a say in.
//! * `format` writes every figure the interface shows.

pub mod actions;
pub mod chrome;
pub mod fades;
pub mod format;
pub mod glass;
pub mod playlist_panel;
pub mod sliders;
pub mod squircle;
pub mod subline;
pub mod sync;
pub mod warm;
