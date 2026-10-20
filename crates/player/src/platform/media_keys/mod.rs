//! The media keys, and the desktop's own display of what is playing.
//!
//! The play/pause and track buttons on a headset or a keyboard go to
//! whichever application the operating system has been told is playing, and
//! the desktop's media widget shows that application's title. Without this
//! the player is invisible to both and the buttons reach whatever else
//! happens to be open.
//!
//! Each platform's answer is different enough to be its own implementation
//! rather than a `cfg` arm: SMTC on Windows registers against a window handle
//! that doesn't exist for the first few frames and is told what changed,
//! while MPRIS on Linux claims a bus name at startup and serves properties to
//! any client at any moment. Both have the same face (`Controls::new(mpv)`,
//! then `publish` once a frame), so the frame loop never learns which it got.

#[cfg(not(windows))]
pub mod mpris;
#[cfg(windows)]
mod smtc;

#[cfg(not(windows))]
pub use mpris::Controls;
#[cfg(windows)]
pub use smtc::Controls;
