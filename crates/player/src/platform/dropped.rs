//! Files dropped onto the window.
//!
//! Slint has no drag-and-drop API, and the `DroppedFile` events winit makes
//! are never surfaced by Slint, so the drop is handled at the window itself.
//!
//! On Windows that means the shell's older, simpler route: revoke the OLE
//! drop target winit registered (which discards what it receives anyway),
//! ask for `WM_DROPFILES` instead, and read the path out of the message.
//!
//! The first path is handed to the same `open-path` the file dialog answers
//! with. A dropped file adopts its folder's siblings as a playlist, as an
//! opened one does, so dropping three files from one folder is the same as
//! dropping one.
//!
//! Linux would be a different implementation against a different protocol
//! (XDND, or `wl_data_device`), and there is no honest `cfg` arm for it here,
//! so there is none: on other platforms this does nothing at all.

/// Whether a drop reaches this program at all on this platform.
///
/// Published to the interface, which mentions dropping in two places and
/// mustn't where it does nothing. A constant, since the `cfg` above decides
/// it at compile time.
pub const SUPPORTED: bool = cfg!(windows);

/// Distinct from the modal-loop hook's, which is on the same window.
#[cfg(windows)]
const SUBCLASS_ID: usize = 0x00DB_3003;

/// Where a dropped path is delivered. A static because the window procedure
/// is a bare function with nowhere to keep anything.
///
/// `Weak` rather than a handle: the drop arrives on the UI thread but inside a
/// window procedure, where application code would re-enter the event loop
/// mid-message, so `upgrade_in_event_loop` turns it into an ordinary event.
#[cfg(windows)]
static TARGET: std::sync::Mutex<Option<slint::Weak<crate::MainWindow>>> =
    std::sync::Mutex::new(None);

/// Installs the hook, retrying from the frame path until the window exists
/// (see [`UntilWindow`](crate::platform::window::UntilWindow)).
#[cfg(windows)]
#[derive(Default)]
pub struct Accepting {
    until: crate::platform::window::UntilWindow,
}

#[cfg(windows)]
impl Accepting {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll(&mut self, ui: &crate::MainWindow) {
        use crate::platform::window::Attempt;

        match self.until.poll(|| accept(ui)) {
            Attempt::Took(()) => {
                if crate::diagnostics::trace() {
                    eprintln!("dbm: accepting dropped files");
                }
            }
            Attempt::GaveUp(e) => eprintln!("dbm: cannot accept dropped files ({e})"),
            Attempt::Waiting => {}
        }
    }
}

#[cfg(windows)]
fn accept(ui: &crate::MainWindow) -> Result<(), String> {
    use slint::ComponentHandle;
    use windows::Win32::System::Ole::RevokeDragDrop;
    use windows::Win32::UI::Shell::{DragAcceptFiles, SetWindowSubclass};

    let hwnd = crate::platform::window::hwnd(ui.window())?;

    // winit's own drop target, which would swallow every drop into an event
    // Slint never delivers. An error means there was none to revoke.
    let _ = unsafe { RevokeDragDrop(hwnd) };
    unsafe { DragAcceptFiles(hwnd, true) };

    if !unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) }.as_bool() {
        return Err(format!(
            "SetWindowSubclass failed: {:?}",
            windows::core::Error::from_win32()
        ));
    }
    *TARGET.lock().unwrap() = Some(ui.as_weak());
    Ok(())
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
    use windows::Win32::UI::Shell::{DefSubclassProc, HDROP};
    use windows::Win32::UI::WindowsAndMessaging::WM_DROPFILES;

    if msg == WM_DROPFILES {
        let drop = HDROP(wparam.0 as *mut core::ffi::c_void);
        let path = unsafe { first_path(drop) };
        // Frees the shell's copy of the list. Owed whether or not anything
        // was read out of it.
        unsafe { windows::Win32::UI::Shell::DragFinish(drop) };
        if let Some(path) = path {
            open(path);
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// The first path in a drop, or `None` if it carried none.
///
/// # Safety
/// `drop` must be a live `HDROP`.
#[cfg(windows)]
unsafe fn first_path(drop: windows::Win32::UI::Shell::HDROP) -> Option<String> {
    use windows::Win32::UI::Shell::DragQueryFileW;

    // 0xFFFFFFFF asks how many files there are rather than for one of them.
    let count = unsafe { DragQueryFileW(drop, 0xFFFF_FFFF, None) };
    if count == 0 {
        return None;
    }
    // Returned length excludes the terminator the copy will write.
    let length = unsafe { DragQueryFileW(drop, 0, None) } as usize;
    if length == 0 {
        return None;
    }
    let mut buffer = vec![0u16; length + 1];
    let written = unsafe { DragQueryFileW(drop, 0, Some(&mut buffer)) } as usize;
    if written == 0 {
        return None;
    }
    buffer.truncate(written);
    Some(String::from_utf16_lossy(&buffer))
}

/// Hand a dropped path to the interface, as an ordinary event rather than
/// from inside the window procedure.
#[cfg(windows)]
fn open(path: String) {
    let target = TARGET.lock().unwrap().clone();
    let Some(weak) = target else {
        return;
    };
    eprintln!("dbm: dropped {path}");
    let _ = weak.upgrade_in_event_loop(move |ui| ui.invoke_open_path(path.into(), true));
}

// ---------------------------------------------------------------------------
// Everywhere else
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub struct Accepting;

#[cfg(not(windows))]
impl Accepting {
    pub fn new() -> Self {
        Self
    }

    /// Nothing yet: X11 and Wayland each want their own protocol.
    pub fn poll(&mut self, _ui: &crate::MainWindow) {}
}

// ---------------------------------------------------------------------------
// Exercising it
// ---------------------------------------------------------------------------

/// Post a file drop at our own window, as the shell would.
///
/// For `DBM_DROP_TEST`. Here rather than in `harness` because it is the
/// inverse of [`first_path`], the same `DROPFILES` block written instead of
/// read, and the two should change together.
///
/// It can't test whether a real drag reaches us, which depends on the drop
/// target being ours rather than winit's; only a drag from Explorer does.
#[cfg(windows)]
pub fn post_test_drop(ui: &crate::MainWindow, path: &str) -> Result<(), String> {
    use slint::ComponentHandle;
    use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_DROPFILES};

    /// The shell's header, followed by a double-NUL-terminated list of paths.
    #[repr(C)]
    struct DropFiles {
        files: u32,
        pt: POINT,
        non_client: i32,
        wide: i32,
    }

    let hwnd = crate::platform::window::hwnd(ui.window())?;
    let mut wide: Vec<u16> = path.encode_utf16().collect();
    // One terminator for the string, one for the list.
    wide.push(0);
    wide.push(0);

    let header = std::mem::size_of::<DropFiles>();
    let bytes = header + wide.len() * 2;
    let block =
        unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }.map_err(|e| format!("GlobalAlloc: {e}"))?;
    let base = unsafe { GlobalLock(block) };
    if base.is_null() {
        return Err("GlobalLock returned null".into());
    }
    unsafe {
        std::ptr::write(
            base as *mut DropFiles,
            DropFiles {
                files: header as u32,
                pt: POINT { x: 0, y: 0 },
                non_client: 0,
                wide: 1,
            },
        );
        std::ptr::copy_nonoverlapping(
            wide.as_ptr(),
            (base as *mut u8).add(header) as *mut u16,
            wide.len(),
        );
        let _ = GlobalUnlock(block);
    }

    // Posted rather than sent, as a real drop arrives as a queued message.
    unsafe {
        PostMessageW(
            Some(hwnd),
            WM_DROPFILES,
            WPARAM(block.0 as usize),
            LPARAM(0),
        )
    }
    .map_err(|e| format!("PostMessage: {e}"))
}
