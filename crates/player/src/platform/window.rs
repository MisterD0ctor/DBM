//! The native window behind Slint's, for the code that has to reach it.
//!
//! On Windows several things can only be done to the window itself (taking
//! over its drop target, hooking the modal resize loop, claiming the media
//! keys), and none before the window manager has made it, which Slint says is
//! after at least one turn of the event loop. So each is tried from the frame
//! path until it takes, by way of [`UntilWindow`]. Minimizing goes to the
//! window too, everywhere but Windows (see [`minimize`]). The window's corners
//! are round, as the desktop's other windows' are: Windows rounds them itself
//! once asked (see [`Rounding`]), and elsewhere they are cut from the frame
//! (see [`corner_radius`]).

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
/// Fails until the window manager has made the window (see [`UntilWindow`]).
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

/// Minimize the window, from its own button.
///
/// Straight to winit rather than through Slint's `minimized`, because a
/// Wayland client is never told it was restored: the property stays set after
/// the first time, and setting it again does nothing. Nothing happens under a
/// backend that isn't winit.
#[cfg(not(windows))]
pub fn minimize(window: &slint::Window) {
    use slint::winit_030::WinitWindowAccessor;

    window.with_winit_window(|w| w.set_minimized(true));
}

/// Windows says when the window is restored, so Slint's own property keeps up
/// and this build needs no unstable API for it.
#[cfg(windows)]
pub fn minimize(window: &slint::Window) {
    window.set_minimized(true);
}

/// The radius of the desktop's own windows' corners, in logical pixels:
/// libadwaita's, which most of a Linux desktop's apps are drawn with.
#[cfg(not(windows))]
pub const CORNER_RADIUS: f32 = 15.0;

/// How round the frame's corners are cut, in logical pixels (see
/// `gpu::corners`).
///
/// Square while the window fills the screen, as other windows are then. A
/// tiled window keeps its round corners, since winit doesn't say when a
/// window is tiled.
#[cfg(not(windows))]
pub fn corner_radius(window: &slint::Window) -> f32 {
    if window.is_maximized() || window.is_fullscreen() {
        0.0
    } else {
        CORNER_RADIUS
    }
}

/// Never cut on Windows, which rounds the window itself (see [`Rounding`]).
#[cfg(windows)]
pub fn corner_radius(_window: &slint::Window) -> f32 {
    0.0
}

/// Asks Windows 11 to round the window's corners, retrying from the frame
/// path until the window exists (see [`UntilWindow`]).
///
/// A window with no frame stays square unless it asks. Windows then rounds it
/// as it does its own, and squares it again while it is maximized or snapped.
#[derive(Default)]
pub struct Rounding {
    #[cfg(windows)]
    until: UntilWindow,
}

impl Rounding {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(windows)]
    pub fn poll(&mut self, window: &slint::Window) {
        if let Attempt::GaveUp(e) = self.until.poll(|| round_corners(window)) {
            eprintln!("dbm: cannot round the window's corners ({e})");
        }
    }

    /// Nothing to ask: the frame's corners are cut instead.
    #[cfg(not(windows))]
    pub fn poll(&mut self, _window: &slint::Window) {}
}

#[cfg(windows)]
fn round_corners(window: &slint::Window) -> Result<(), String> {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    let hwnd = hwnd(window)?;
    let preference = DWMWCP_ROUND;
    let asked = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::from_ref(&preference).cast(),
            std::mem::size_of_val(&preference) as u32,
        )
    };
    // Windows 10 refuses it, having no round corners to give. That is an
    // answer too, so it is not asked again.
    if let Err(e) = asked {
        if crate::diagnostics::trace() {
            eprintln!("dbm: the window's corners stay square ({e})");
        }
    }
    Ok(())
}
