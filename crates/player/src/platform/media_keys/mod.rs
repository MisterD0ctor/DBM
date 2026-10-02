//! The media keys, and the desktop's own display of what is playing.
//!
//! The play/pause and track buttons on a headset or a keyboard go to
//! whichever application the operating system has been told is playing, and
//! the desktop's media widget shows that application's title. Without this
//! the player is invisible to both and the buttons reach whatever else
//! happens to be open.
//!
//! Each platform has its own answer, different enough in shape to be its own
//! implementation rather than a `cfg` arm of one: SMTC on Windows registers
//! against a window handle that does not exist for the first few frames and
//! is told what changed, while MPRIS on Linux claims a bus name at startup
//! and serves properties any client may ask for at any moment. Both wear the
//! same face — `Controls::new(mpv)`, then `publish` once a frame — so the
//! frame loop asks for one type and never learns which it got.

#[cfg(not(windows))]
pub mod mpris;
#[cfg(windows)]
mod smtc;

#[cfg(not(windows))]
pub use mpris::Controls;
#[cfg(windows)]
pub use smtc::Controls;
