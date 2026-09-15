//! Keeping the display on while a video plays.
//!
//! mpv does this itself when it owns the window — `stop-screensaver` — but
//! here it renders into ours through the render API and never learns there is
//! a window at all, so nothing was asking and the monitors went to sleep in
//! the middle of a film.
//!
//! Only while playing: paused, finished or empty, the machine is allowed to
//! idle as it would with anything else open. The request is per thread and
//! lasts until changed, so it is made from the frame path — always the UI
//! thread — and only when the answer changes, not sixty times a second.
//!
//! Linux would inhibit through the desktop's D-Bus screensaver interface,
//! which is its own piece of work; everything here compiles away there.

use crate::state::PlayerState;

#[derive(Default)]
pub struct Awake {
    held: bool,
}

impl Awake {
    /// Hold the display on or let it go, to match what is playing.
    pub fn follow(&mut self, player: &PlayerState) {
        let playing = player.path.is_some() && !player.paused && !player.eof_reached;
        if playing != self.held {
            self.held = playing;
            request(playing);
        }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        if self.held {
            request(false);
        }
    }
}

#[cfg(windows)]
fn request(on: bool) {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
    };
    let flags = if on {
        ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    // Returns the previous state, or zero when Windows refused.
    if unsafe { SetThreadExecutionState(flags) }.0 == 0 {
        eprintln!("dbm: Windows would not {} the display", if on { "hold" } else { "release" });
    }
}

#[cfg(not(windows))]
fn request(_on: bool) {}
