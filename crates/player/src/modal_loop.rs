//! Keeping the picture alive while Windows owns the message loop.
//!
//! Grab a window edge — even just hold it, without moving — and Windows enters
//! a *modal* move/size loop inside `DefWindowProc`. That loop runs its own
//! message pump and never returns to the application's event loop until the
//! mouse is released. winit 0.30 does nothing about this: `WM_ENTERSIZEMOVE`
//! only sets an internal flag, and its `PeekMessageW` pump is simply blocked
//! for the duration.
//!
//! The consequence for us is total. mpv signals new frames from its own thread
//! by waking the event loop (`upgrade_in_event_loop`), and that wakeup sits
//! unserviced. Nothing renders, so the video freezes. Dragging looked less
//! broken only because `WM_SIZE` kept arriving; holding still produces no
//! messages at all and the picture stops dead.
//!
//! The saving grace is that winit dispatches `WM_PAINT` straight into the
//! application from inside the window procedure — `send_event` calls the
//! handler synchronously for `RedrawRequested` — and window procedures *are*
//! called during the modal loop. So a message is all that is needed, and
//! `WM_TIMER` is the classic way to manufacture one: the modal pump dispatches
//! timer messages just like any other.
//!
//! So: subclass the window, start a timer when the modal loop begins, and turn
//! each tick into a paint. The old Tauri build never needed this because mpv
//! owned a separate HWND and presented from its own thread, entirely
//! independent of this thread's message pump.

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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Diagnostics for the window procedure, which has no other way to reach the
/// rest of the program.
#[cfg(windows)]
static TRACE: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static TICKS: AtomicU64 = AtomicU64::new(0);

/// Install the hook. The window handle is only available once the window
/// manager has actually created the window, which Slint documents as being
/// after at least one turn of the event loop — so this is expected to fail on
/// the first attempts and should be retried until it succeeds.
#[cfg(windows)]
pub fn keep_rendering_during_modal_loop(window: &slint::Window) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::SetWindowSubclass;

    let provider = window.window_handle();
    let handle = provider
        .window_handle()
        .map_err(|e| format!("no window handle yet: {e}"))?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return Err("window handle is not Win32".into());
    };
    let hwnd = HWND(win32.hwnd.get() as *mut core::ffi::c_void);
    TRACE.store(std::env::var_os("DBM_TRACE").is_some(), Ordering::Relaxed);

    if unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) }.as_bool() {
        if TRACE.load(Ordering::Relaxed) {
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
            if TRACE.load(Ordering::Relaxed) {
                eprintln!("dbm: modal loop entered — heartbeat timer started");
            }
        },
        WM_EXITSIZEMOVE => unsafe {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            if TRACE.load(Ordering::Relaxed) {
                eprintln!(
                    "dbm: modal loop exited after {} heartbeat ticks",
                    TICKS.load(Ordering::Relaxed)
                );
            }
        },
        WM_TIMER if wparam.0 == TIMER_ID => {
            TICKS.fetch_add(1, Ordering::Relaxed);
            // `RDW_INTERNALPAINT` posts a WM_PAINT without needing an invalid
            // region, which is what we want: nothing is actually dirty in
            // Windows' eyes, we just need winit to be handed a message so it
            // dispatches RedrawRequested and Slint renders the frame mpv has
            // been sitting on.
            let _ = unsafe { RedrawWindow(Some(hwnd), None, None, RDW_INTERNALPAINT) };
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Put the window into the modal size loop from code, the same state a user
/// creates by grabbing an edge. The only way to test this path without a hand
/// on the mouse — a programmatic `set_size` does *not* reproduce it, which is
/// exactly why the earlier resize measurements looked healthy while the real
/// thing froze.
#[cfg(windows)]
pub fn enter_modal_size_loop_for_test(window: &slint::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SC_SIZE, WM_SYSCOMMAND};

    let provider = window.window_handle();
    let Ok(handle) = provider.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(win32.hwnd.get() as *mut core::ffi::c_void);
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
pub fn keep_rendering_during_modal_loop(_window: &slint::Window) -> Result<(), String> {
    // X11/Wayland have no equivalent: resizing is handled through the normal
    // event stream, so the loop is never taken away from us.
    Ok(())
}
