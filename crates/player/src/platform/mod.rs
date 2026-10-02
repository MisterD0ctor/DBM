//! What each operating system has to be asked for in its own way.
//!
//! Keeping the display awake, hiding the pointer, a native file dialog, files
//! dropped on the window, frames during a Windows resize, and the media keys
//! — SMTC on Windows, MPRIS on Linux. Each module covers both platforms, even
//! where one of them does nothing, so nothing outside this folder needs a
//! `cfg` to call them.

pub mod awake;
pub mod cursor;
pub mod dialog;
pub mod dropped;
pub mod modal_loop;
#[cfg(not(windows))]
pub mod mpris;
pub mod smtc;
