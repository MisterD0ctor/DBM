//! What each operating system has to be asked for in its own way.
//!
//! Keeping the display awake, hiding the pointer, a native file dialog, files
//! dropped on the window, frames during a Windows resize, minimizing, the
//! window's round corners, whether the desktop asks for less motion, the
//! media keys (SMTC on Windows, MPRIS on Linux), and handing freed memory
//! back. Each module covers both platforms, even where one does nothing, so
//! nothing outside this folder needs a `cfg`.

pub mod awake;
pub mod cursor;
pub mod dialog;
pub mod dropped;
pub mod media_keys;
pub mod memory;
pub mod modal_loop;
pub mod motion;
pub mod window;
