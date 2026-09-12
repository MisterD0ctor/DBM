//! When the on-screen chrome is visible.
//!
//! Split of responsibility: this owns the *clock* — how long since anything
//! happened — and the UI owns the *policy* built on it, because whether a
//! given panel should be up depends on things (an open menu, a hovered
//! button) that Slint already knows and Rust would have to be told about.
//!
//! Activity arrives through the `Chrome` global rather than a callback on the
//! window, so a button or menu row buried inside a reusable component can
//! report it without every call site threading a callback down.
//!
//! The pointer is hooked up here too. It is the same policy wearing another
//! face — the cursor goes when the chrome does — and it is the same clock
//! that decides, so the two cannot drift apart and disagree about whether
//! anything has happened lately. What that takes on each platform lives in
//! `cursor`.
//!
//! The subtlety that makes this work at all: Slint's `TouchArea::moved` only
//! fires while the pointer is *held down*. Plain hovering has to come through
//! `pointer-event`, which is delivered for `PointerEventKind::Move`
//! unconditionally. Using `moved` here means the chrome only ever wakes on a
//! drag — which is exactly the bug this module was extracted to fix.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use crate::{Chrome, MainWindow};

/// How long the chrome stays up after the last activity.
pub const HIDE_AFTER: Duration = Duration::from_millis(3000);

/// How often the idle clock is checked.
///
/// Polled rather than re-armed on each activity: activity arrives on every
/// mouse move, and restarting a timer that often costs more than one cheap
/// comparison. The cost of polling is a `Duration` subtraction four times a
/// second.
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
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

/// Connect the activity global and start the idle clock.
///
/// The returned timer must be kept alive; dropping it stops the clock and the
/// chrome would never hide again.
#[must_use]
pub fn install(ui: &MainWindow, activity: Activity) -> slint::Timer {
    {
        let activity = activity.clone();
        ui.global::<Chrome>().on_activity(move || activity.bump());
    }

    {
        // The interface decides; this only carries it to the window. Held by
        // the callback rather than by the timer below because the pointer has
        // to come back the moment the hand moves, and the timer would make
        // that up to a poll late.
        let mut pointer = crate::cursor::Pointer::new();
        ui.global::<Chrome>()
            .on_hide_pointer(move |hide| pointer.set_hidden(hide));
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
