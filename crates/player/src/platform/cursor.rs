//! Taking the pointer off screen over a film, and putting it back.
//!
//! Here rather than in the interface, because Slint decides a cursor only
//! while routing a mouse event: `mouse-cursor` is read as an event passes the
//! item under the pointer, so the property changing on its own reaches no
//! one. A binding that hid the cursor when the chrome goes would only act on
//! the next movement, which brings the chrome straight back.
//!
//! So the window is told directly, on the idle clock that fades the chrome.
//! The interface still owns the decision (`pointer-hidden`, which folds in an
//! open panel and whether a film is loaded); this only carries it out.
//!
//! Windows keeps one display count per input queue, with no window in the
//! call; Wayland and X11 hide a pointer by giving its surface no cursor,
//! which means reaching the winit window behind Slint's. Hence the
//! `&slint::Window` in a call Windows ignores.

/// The pointer's visibility, as this window has last asked for it.
///
/// Holds the last request so a repeat isn't sent, since the Win32 call
/// underneath is a counter, and two calls one way take two the other to undo.
pub struct Pointer {
    hidden: bool,
}

impl Pointer {
    pub fn new() -> Self {
        Self { hidden: false }
    }
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
    /// Hide the pointer, or show it. Does nothing if it is already that way.
    ///
    /// The window is not part of this on Windows: the count below belongs to
    /// the thread's input queue rather than to any one window.
    pub fn set_hidden(&mut self, _window: &slint::Window, hidden: bool) {
        if self.hidden == hidden {
            return;
        }
        self.hidden = hidden;
        // `ShowCursor` keeps a display count: the pointer is drawn while it is
        // zero or above, and every hide must be matched by a show. It belongs
        // to this thread's input queue, so the pointer reappears over other
        // applications by itself, and it is independent of the cursor Slint
        // last set, which says which cursor rather than whether one is drawn.
        unsafe { windows::Win32::UI::WindowsAndMessaging::ShowCursor(!hidden) };
    }
}

/// Whether the system says a cursor is on screen at all.
///
/// For the harness: the display count isn't readable, and our own record of
/// what we asked proves nothing, so the system is asked.
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
    /// Hide the pointer, or show it. Does nothing if it is already that way.
    ///
    /// Through the winit window behind Slint's, since a pointer is hidden per
    /// surface here, not per process. The accessor is behind Slint's unstable
    /// `unstable-winit-030` feature, which is worth it: a film with a cursor
    /// parked over it is worse than an API that may be renamed.
    ///
    /// Nothing happens under a backend that isn't winit (the accessor's
    /// `None`). Setting a cursor icon while hidden doesn't undo this, since
    /// winit holds the icon and applies it when the pointer comes back.
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

/// Put the pointer back, since the display count is this thread's, which is
/// about to end with the count still down.
///
/// Windows only, because everywhere else the pointer is hidden on the window,
/// which gives it back by going away.
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
