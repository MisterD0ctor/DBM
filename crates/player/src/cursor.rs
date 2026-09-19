//! Taking the pointer off screen over a film, and putting it back.
//!
//! This is here rather than in the interface because Slint decides a cursor
//! only while it is routing a mouse event. `mouse-cursor` is read as the
//! event passes through the item under the pointer, and
//! `WindowAdapter::set_mouse_cursor` is called from nowhere else — so the
//! property changing on its own reaches no one.
//!
//! That makes the obvious binding — hide the cursor when the chrome is gone —
//! not merely late but impossible. The chrome hides three seconds after the
//! last movement, by which time there is no event left to carry the decision;
//! and the next event is a movement, which brings the chrome straight back. A
//! binding on `mouse-cursor` can therefore only ever ask for a hidden cursor
//! at the exact moments one is not wanted. It was written that way, and the
//! pointer never once disappeared.
//!
//! So the window is told directly, from the same idle clock that fades the
//! chrome. The interface still owns the decision — it publishes
//! `pointer-hidden`, which folds in an open panel and whether a film is even
//! loaded — and this only carries it to the platform.
//!
//! The two platforms say it in different places. Windows has one display
//! count per input queue and no window in the call at all; Wayland and X11
//! have no such thing, and a pointer is hidden by giving the surface it is
//! over no cursor — which means reaching the winit window behind Slint's.
//! Hence the `&slint::Window` in a call Windows then ignores.

/// The pointer's visibility, as this window has last asked for it.
///
/// Holds the last request so a repeat is not sent: the Win32 call underneath
/// is a counter rather than a flag, and calling it twice in the same
/// direction takes two calls in the other to undo.
pub struct Pointer {
    hidden: bool,
}

impl Default for Pointer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
impl Pointer {
    pub fn new() -> Self {
        Self { hidden: false }
    }

    /// Hide the pointer, or show it. Does nothing if it is already that way.
    ///
    /// The window is not part of this on Windows: the count below belongs to
    /// the thread's input queue rather than to any one window.
    pub fn set_hidden(&mut self, _window: &slint::Window, hidden: bool) {
        if self.hidden == hidden {
            return;
        }
        self.hidden = hidden;
        // `ShowCursor` keeps a display count, not a flag: the pointer is drawn
        // while the count is zero or above, and every hide has to be matched
        // by a show. The count belongs to this thread's input queue, which is
        // why the pointer reappears over other applications without this
        // window doing anything about it.
        //
        // It is also independent of whichever cursor Slint last set. A hand
        // over a button and a hidden pointer are not in conflict: one says
        // which cursor, the other whether any is drawn at all.
        unsafe { windows::Win32::UI::WindowsAndMessaging::ShowCursor(!hidden) };
    }
}

/// Whether the system says a cursor is on screen at all.
///
/// For the harness. The display count is not readable, and this program's own
/// record of what it asked for would prove nothing about what happened — so
/// the question is put to the system, which is the only witness that counts.
#[cfg(windows)]
pub fn on_screen() -> Option<bool> {
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorInfo, CURSORINFO, CURSOR_SHOWING};

    let mut info = CURSORINFO {
        cbSize: std::mem::size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetCursorInfo(&mut info) }.ok()?;
    Some(info.flags == CURSOR_SHOWING)
}

// ---------------------------------------------------------------------------
// Everywhere else
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
impl Pointer {
    pub fn new() -> Self {
        Self { hidden: false }
    }

    /// Hide the pointer, or show it. Does nothing if it is already that way.
    ///
    /// Through the winit window behind Slint's, which is the only place the
    /// question can be put: a pointer is hidden per surface here, not per
    /// process. The accessor is still marked unstable at 1.17 — the reason
    /// this went unimplemented while Linux was not a target that shipped. It
    /// is one now, and a film with a cursor parked over it is worse than a
    /// dependency on an API that may be renamed.
    ///
    /// Nothing happens under a backend that is not winit, which is what the
    /// `None` from the accessor means. Setting a cursor *icon* while this is
    /// off does not undo it: winit holds the icon and applies it when the
    /// pointer comes back, so Slint's own `mouse-cursor` and this can both
    /// be true at once.
    pub fn set_hidden(&mut self, window: &slint::Window, hidden: bool) {
        use slint::winit_030::WinitWindowAccessor;

        if self.hidden == hidden {
            return;
        }
        self.hidden = hidden;
        window.with_winit_window(|w| w.set_cursor_visible(!hidden));
    }
}

#[cfg(not(windows))]
pub fn on_screen() -> Option<bool> {
    None
}

/// Put the pointer back. A program that exits with the cursor hidden has taken
/// something that is not its own: the display count is this thread's, and the
/// thread is about to end with the count still down.
///
/// Windows only, and not an oversight. Everywhere else the pointer is hidden
/// on the window rather than on the process, so it is already given back by
/// the window going away — and there is no window left to ask by the time
/// this runs.
#[cfg(windows)]
impl Drop for Pointer {
    fn drop(&mut self) {
        if !self.hidden {
            return;
        }
        self.hidden = false;
        unsafe { windows::Win32::UI::WindowsAndMessaging::ShowCursor(true) };
    }
}
