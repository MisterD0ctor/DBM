//! mpv, and everything said to it or heard from it.
//!
//! The binding itself is `dbm_mpv`, the one place this program calls libmpv's
//! C API. Everything here is built on it: the mirror of the properties the
//! interface shows, what to call the tracks it lists, the commands the
//! interface sends, resuming where a film was left (and asking whether to),
//! putting audio back when its device returns, turning a timeline drag into
//! seeks mpv can keep up with, offering back the place a click on the
//! timeline or a number key left, and skipping an episode's titles and
//! credits for someone who asked to.

pub mod arrival;
pub mod audio;
pub mod commands;
pub mod ends;
pub mod labels;
pub mod retrace;
pub mod scrub;
pub mod session;
pub mod skips;
pub mod state;
