//! When the on-screen chrome is visible.
//!
//! This owns the clock (how long since anything happened) and the UI owns the
//! policy built on it, because whether the chrome should be up depends on
//! things, like an open menu, that Slint already knows.
//!
//! Activity arrives through the `Chrome` global rather than a callback on the
//! window, so a button or menu row buried inside a reusable component can
//! report it without every call site threading a callback down.
//!
//! The pointer is hooked up here too: it goes when the chrome does, on the
//! same clock, so the two can't disagree. What that takes on each platform is
//! in `cursor`.
//!
//! Activity has to come through `pointer-event`, not `TouchArea::moved`,
//! which only fires while a button is held, so the chrome would only wake on
//! a drag.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use crate::{Chrome, MainWindow};

/// How long the chrome stays up after the last activity.
pub const HIDE_AFTER: Duration = Duration::from_millis(3000);

/// How long a hand counts as still on the controls after it last moved.
///
/// A second, much shorter clock off the same bump. `HIDE_AFTER` is how long
/// a person expects the chrome to stay after they stop; this is whether
/// anything is still following the pointer (see
/// `driver::keep_the_interface_moving`). Longer than the gap between mouse
/// events at any report rate, so a drag never stutters, and short enough that
/// a hand off the mouse stops costing frames almost at once.
pub const STIR_FOR: Duration = Duration::from_millis(250);

/// How often the idle clock is checked.
///
/// Polled rather than re-armed on each activity, which arrives on every
/// mouse move, where restarting a timer costs more than a comparison.
const POLL: Duration = Duration::from_millis(200);

/// Shared "when did something last happen".
#[derive(Clone)]
pub struct Activity(Rc<Cell<Instant>>);

impl Activity {
    pub fn new() -> Self {
        Self(Rc::new(Cell::new(Instant::now())))
    }

    pub fn bump(&self) {
        self.0.set(Instant::now());
    }

    pub fn is_idle(&self) -> bool {
        self.0.get().elapsed() >= HIDE_AFTER
    }

    /// Whether a hand is on the controls this moment, rather than merely
    /// having been on them recently enough to keep the chrome up.
    pub fn stirring(&self) -> bool {
        self.0.get().elapsed() < STIR_FOR
    }
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

/// Connect the activity global and start the idle clock.
///
/// The returned timer must be kept alive, or the chrome would never hide
/// again.
#[must_use]
pub fn install(ui: &MainWindow, activity: Activity) -> slint::Timer {
    {
        let activity = activity.clone();
        ui.global::<Chrome>().on_activity(move || activity.bump());
    }

    {
        // The interface decides; this only carries it to the window. Held by
        // the callback rather than the timer below, which would bring the
        // pointer back up to a poll late.
        let mut pointer = crate::platform::cursor::Pointer::new();
        // Weak, because this callback is owned by the window it asks about.
        let weak = ui.as_weak();
        ui.global::<Chrome>().on_hide_pointer(move |hide| {
            if let Some(ui) = weak.upgrade() {
                pointer.set_hidden(ui.window(), hide);
            }
        });
    }

    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(slint::TimerMode::Repeated, POLL, move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let chrome = ui.global::<Chrome>();
        let idle = activity.is_idle();
        // Only write on a change: setting a property Slint already agrees
        // with is a no-op, but the comparison is cheaper than the call.
        if chrome.get_idle() != idle {
            chrome.set_idle(idle);
        }
    });
    timer
}
