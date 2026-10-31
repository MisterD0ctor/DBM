//! mpv, and everything said to it or heard from it.
//!
//! `mpv` is the binding itself, the one place this program calls libmpv's C
//! API. Everything else here is built on it: the mirror of the properties the
//! interface shows, the list-shaped ones read on demand, the commands the
//! interface sends, resuming where a film was left (and asking whether to),
//! putting audio back when its device returns, turning a timeline drag into
//! seeks mpv can keep up with, and offering back the place a click on the
//! timeline or a number key left.

pub mod arrival;
pub mod audio;
pub mod commands;
pub mod mpv;
pub mod retrace;
pub mod scrub;
pub mod session;
pub mod state;
pub mod tracks;
