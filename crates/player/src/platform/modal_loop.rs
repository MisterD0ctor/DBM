//! Keeping the picture alive while Windows owns the message loop.
//!
//! Grab a window edge — even just hold it, without moving — and Windows enters
//! a *modal* move/size loop inside `DefWindowProc`. That loop runs its own
//! message pump and never returns to the application's event loop until the
//! mouse is released. winit 0.30 does nothing about this: `WM_ENTERSIZEMOVE`
//! only sets an internal flag, and its `PeekMessageW` pump is simply blocked
//! for the duration.
//!
//! mpv signals new frames by waking the event loop (`upgrade_in_event_loop`),
//! and that wakeup sits unserviced, so the video freezes; holding an edge
//! still produces no messages at all.
//!
//! But winit dispatches `WM_PAINT` straight into the application from inside
//! the window procedure (`send_event` calls the handler synchronously for
//! `RedrawRequested`), and window procedures are called during the modal
//! loop. So a message is all that is needed, and the modal pump dispatches
//! `WM_TIMER` like any other: subclass the window, start a timer when the
//! modal loop begins, and turn each tick into a paint.

use crate::platform::window::{Attempt, UntilWindow};

/// Interval between forced repaints while the modal loop owns the thread.
/// ~8ms keeps a 60fps source smooth without spinning harder than the display.
#[cfg(windows)]
const TICK_MS: u32 = 8;

/// Arbitrary, just has to be unique for this window.
#[cfg(windows)]
const TIMER_ID: usize = 0x00DB_3001;

#[cfg(windows)]
const SUBCLASS_ID: usize = 0x00DB_3002;

#[cfg(windows)]
use std::sync::atomic::{AtomicU64, Ordering};

/// Heartbeat ticks in the current modal loop, for the trace line that says
/// the hook did its job.
#[cfg(windows)]
static TICKS: AtomicU64 = AtomicU64::new(0);

/// Installs the hook, retrying from the frame path until the window exists
/// (see [`UntilWindow`]). `DBM_NO_MODAL_HOOK` leaves it out, to see what
/// happens without it.
pub struct Hook {
    until: UntilWindow,
}

impl Hook {
    pub fn new() -> Self {
        Self {
            until: if std::env::var_os("DBM_NO_MODAL_HOOK").is_some() {
                UntilWindow::never()
            } else {
                UntilWindow::new()
            },
        }
    }

    pub fn poll(&mut self, window: &slint::Window) {
        if let Attempt::GaveUp(e) = self.until.poll(|| keep_rendering_during_modal_loop(window)) {
            eprintln!(
                "dbm: modal-loop hook unavailable ({e}); holding a window edge will stall rendering"
            );
        }
    }
}

impl Default for Hook {
    fn default() -> Self {
        Self::new()
    }
}

/// Subclass the window, so its modal loop can be turned into paints.
#[cfg(windows)]
fn keep_rendering_during_modal_loop(window: &slint::Window) -> Result<(), String> {
    use windows::Win32::UI::Shell::SetWindowSubclass;

    let hwnd = crate::platform::window::hwnd(window)?;

    if unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) }.as_bool() {
        if crate::diagnostics::trace() {
            eprintln!("dbm: modal-loop hook installed");
        }
        Ok(())
    } else {
        Err(format!(
            "SetWindowSubclass failed: {:?}",
            windows::core::Error::from_win32()
        ))
    }
}

#[cfg(windows)]
unsafe extern "system" fn subclass_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_INTERNALPAINT};
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::{
        KillTimer, SetTimer, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_TIMER,
    };

    match msg {
        WM_ENTERSIZEMOVE => unsafe {
            SetTimer(Some(hwnd), TIMER_ID, TICK_MS, None);
            TICKS.store(0, Ordering::Relaxed);
            if crate::diagnostics::trace() {
                eprintln!("dbm: modal loop entered — heartbeat timer started");
            }
        },
        WM_EXITSIZEMOVE => unsafe {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            if crate::diagnostics::trace() {
                eprintln!(
                    "dbm: modal loop exited after {} heartbeat ticks",
                    TICKS.load(Ordering::Relaxed)
                );
            }
        },
        WM_TIMER if wparam.0 == TIMER_ID => {
            TICKS.fetch_add(1, Ordering::Relaxed);
            // `RDW_INTERNALPAINT` posts a WM_PAINT without an invalid region,
            // since nothing is dirty in Windows' eyes; it only hands winit a
            // message, so it dispatches RedrawRequested and Slint renders.
            let _ = unsafe { RedrawWindow(Some(hwnd), None, None, RDW_INTERNALPAINT) };
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Put the window into the modal size loop from code, the state a user
/// creates by grabbing an edge, to test this path without a hand on the
/// mouse, since a programmatic `set_size` doesn't reproduce it.
#[cfg(windows)]
pub fn enter_modal_size_loop_for_test(window: &slint::Window) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SC_SIZE, WM_SYSCOMMAND};

    let Ok(hwnd) = crate::platform::window::hwnd(window) else {
        return;
    };
    let _ = unsafe {
        PostMessageW(
            Some(hwnd),
            WM_SYSCOMMAND,
            WPARAM(SC_SIZE as usize),
            LPARAM(0),
        )
    };
}

#[cfg(not(windows))]
pub fn enter_modal_size_loop_for_test(_window: &slint::Window) {}

#[cfg(not(windows))]
fn keep_rendering_during_modal_loop(_window: &slint::Window) -> Result<(), String> {
    // X11/Wayland have no equivalent: resizing is handled through the normal
    // event stream, so the loop is never taken away from us.
    Ok(())
}
