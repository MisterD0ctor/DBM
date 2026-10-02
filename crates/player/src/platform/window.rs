//! The native window behind Slint's, for the code that has to reach it.
//!
//! On Windows several things can only be done to the window itself — taking
//! over its drop target, hooking the modal resize loop, claiming the media
//! keys for it — and none of them can happen before the window manager has
//! made the window. Slint documents that as after at least one turn of the
//! event loop, so each of them is tried from the frame path until it takes,
//! by way of [`UntilWindow`].

/// A setup step retried once a frame until the window exists for it.
///
/// It succeeds once and is never tried again; or, after a couple of seconds'
/// worth of frames without a window handle, it is given up on, once.
pub struct UntilWindow {
    attempts: u32,
    settled: bool,
}

/// What one [`UntilWindow::poll`] came to.
pub enum Attempt<T> {
    /// It took, this frame. The step is not tried again.
    Took(T),
    /// No window to try against yet, or no longer anything to try.
    Waiting,
    /// It failed for the last time, this frame, with the last error. The step
    /// is not tried again.
    GaveUp(String),
}

impl UntilWindow {
    /// A couple of seconds of frames.
    const GIVE_UP_AFTER: u32 = 120;

    pub fn new() -> Self {
        Self {
            attempts: 0,
            settled: false,
        }
    }

    /// A step that is not to be tried at all.
    pub fn never() -> Self {
        Self {
            attempts: 0,
            settled: true,
        }
    }

    /// Try the step, unless it already took or was given up on.
    pub fn poll<T>(&mut self, step: impl FnOnce() -> Result<T, String>) -> Attempt<T> {
        if self.settled {
            return Attempt::Waiting;
        }
        match step() {
            Ok(value) => {
                self.settled = true;
                Attempt::Took(value)
            }
            Err(e) => {
                self.attempts += 1;
                if self.attempts < Self::GIVE_UP_AFTER {
                    return Attempt::Waiting;
                }
                self.settled = true;
                Attempt::GaveUp(e)
            }
        }
    }
}

impl Default for UntilWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// The Win32 handle of the window behind `window`.
///
/// Fails until the window manager has made the window — see [`UntilWindow`].
#[cfg(windows)]
pub fn hwnd(window: &slint::Window) -> Result<windows::Win32::Foundation::HWND, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let provider = window.window_handle();
    let handle = provider
        .window_handle()
        .map_err(|e| format!("no window handle yet: {e}"))?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return Err("window handle is not Win32".into());
    };
    Ok(windows::Win32::Foundation::HWND(
        win32.hwnd.get() as *mut core::ffi::c_void
    ))
}
