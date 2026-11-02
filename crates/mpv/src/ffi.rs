//! The C side: libmpv's types as client.h, render.h and render_gl.h lay them
//! out, and the entry points the player calls, loaded from the library.

use std::ffi::{c_char, c_int, c_void};
use std::mem::ManuallyDrop;
use std::path::Path;

use super::{Error, Result};

#[repr(C)]
pub(super) struct MpvHandle {
    _opaque: [u8; 0],
}

#[repr(C)]
pub(super) struct MpvRenderCtx {
    _opaque: [u8; 0],
}

pub(super) const RENDER_PARAM_INVALID: c_int = 0;
pub(super) const RENDER_PARAM_API_TYPE: c_int = 1;
pub(super) const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
pub(super) const RENDER_PARAM_OPENGL_FBO: c_int = 3;
pub(super) const RENDER_PARAM_FLIP_Y: c_int = 4;
/// `MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME`. Defaults to 1, which makes
/// `mpv_render_context_render` sleep until the frame is due to be shown.
pub(super) const RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;

/// `mpv_render_context_update` sets this bit when a new frame is ready.
pub(super) const UPDATE_FRAME: u64 = 1;

#[repr(C)]
pub(super) struct RenderParam {
    pub(super) kind: c_int,
    pub(super) data: *mut c_void,
}

#[repr(C)]
pub(super) struct OpenglInitParams {
    pub(super) get_proc_address:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void>,
    pub(super) ctx: *mut c_void,
}

#[repr(C)]
pub(super) struct OpenglFbo {
    pub(super) fbo: c_int,
    pub(super) w: c_int,
    pub(super) h: c_int,
    /// The attachment's GL internal format, or 0 for "unknown".
    ///
    /// How mpv learns the target can hold alpha. Left at 0, mpv treats the
    /// surface as opaque and fills the letterbox with black whatever
    /// `--background` says, erasing the ambient border.
    pub(super) internal_format: c_int,
}

/// `GL_RGBA8`. Hardcoded rather than pulled from glow so this module stays
/// free of a GL binding dependency.
pub(super) const GL_RGBA8: c_int = 0x8058;

// `mpv_format`, the scalar kinds.
pub(super) const FORMAT_STRING: c_int = 1;
pub(super) const FORMAT_FLAG: c_int = 3;
pub(super) const FORMAT_DOUBLE: c_int = 5;

// `mpv_event_id`, the handful we act on.
pub(super) const EVENT_SHUTDOWN: c_int = 1;
pub(super) const EVENT_START_FILE: c_int = 6;
pub(super) const EVENT_END_FILE: c_int = 7;
pub(super) const EVENT_FILE_LOADED: c_int = 8;
pub(super) const EVENT_SEEK: c_int = 20;
pub(super) const EVENT_PLAYBACK_RESTART: c_int = 21;
pub(super) const EVENT_COMMAND_REPLY: c_int = 5;
pub(super) const EVENT_PROPERTY_CHANGE: c_int = 22;

#[repr(C)]
pub(super) struct RawEvent {
    pub(super) event_id: c_int,
    pub(super) error: c_int,
    pub(super) reply_userdata: u64,
    pub(super) data: *mut c_void,
}

/// `mpv_event_end_file`. Only the first three fields are read; the rest of
/// the struct is longer and is never touched, which is safe because mpv owns
/// the allocation and we only ever read a prefix of it. The entry id has been
/// there since mpv 0.33.
#[repr(C)]
pub(super) struct RawEventEndFile {
    pub(super) reason: c_int,
    pub(super) error: c_int,
    pub(super) playlist_entry_id: i64,
}

/// `MPV_END_FILE_REASON_ERROR`. The other reasons (EOF, stop, quit,
/// redirect) are ordinary endings.
pub(super) const END_FILE_REASON_ERROR: c_int = 4;
/// `MPV_END_FILE_REASON_EOF`: the file played to its end. With the window
/// held open only at the end of the list, that is autoplay moving on.
/// Asked-for changes end a file with a stop.
pub(super) const END_FILE_REASON_EOF: c_int = 0;

#[repr(C)]
pub(super) struct RawEventProperty {
    /// Unread: the id the property was observed under says which it is.
    pub(super) _name: *const c_char,
    pub(super) format: c_int,
    pub(super) data: *mut c_void,
}

pub(super) struct Lib {
    /// Never unloaded: mpv's script threads are detached, and one can still
    /// be on its way out of mpv's code after `mpv_terminate_destroy` returns.
    /// Unloading then would pull that code from under it, a crash at quit.
    _library: ManuallyDrop<libloading::Library>,
    pub(super) mpv_create: unsafe extern "C" fn() -> *mut MpvHandle,
    pub(super) mpv_initialize: unsafe extern "C" fn(*mut MpvHandle) -> c_int,
    pub(super) mpv_terminate_destroy: unsafe extern "C" fn(*mut MpvHandle),
    pub(super) mpv_set_option_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    pub(super) mpv_set_property_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    pub(super) mpv_get_property_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char) -> *mut c_char,
    pub(super) mpv_free: unsafe extern "C" fn(*mut c_void),
    /// Turns an mpv error code into a sentence, "Unrecognized file format"
    /// rather than "error -13".
    pub(super) mpv_error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    pub(super) mpv_command_async:
        unsafe extern "C" fn(*mut MpvHandle, u64, *mut *const c_char) -> c_int,
    pub(super) mpv_command: unsafe extern "C" fn(*mut MpvHandle, *mut *const c_char) -> c_int,
    pub(super) mpv_observe_property:
        unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int) -> c_int,
    pub(super) mpv_wait_event: unsafe extern "C" fn(*mut MpvHandle, f64) -> *mut RawEvent,
    pub(super) mpv_set_wakeup_callback: unsafe extern "C" fn(
        *mut MpvHandle,
        Option<unsafe extern "C" fn(*mut c_void)>,
        *mut c_void,
    ),
    pub(super) mpv_render_context_create:
        unsafe extern "C" fn(*mut *mut MpvRenderCtx, *mut MpvHandle, *mut RenderParam) -> c_int,
    pub(super) mpv_render_context_set_update_callback: unsafe extern "C" fn(
        *mut MpvRenderCtx,
        Option<unsafe extern "C" fn(*mut c_void)>,
        *mut c_void,
    ),
    pub(super) mpv_render_context_update: unsafe extern "C" fn(*mut MpvRenderCtx) -> u64,
    pub(super) mpv_render_context_render:
        unsafe extern "C" fn(*mut MpvRenderCtx, *mut RenderParam) -> c_int,
    pub(super) mpv_render_context_report_swap: unsafe extern "C" fn(*mut MpvRenderCtx),
    pub(super) mpv_render_context_free: unsafe extern "C" fn(*mut MpvRenderCtx),
}

macro_rules! sym {
    ($library:expr, $name:literal) => {
        // SAFETY: each field's type is the signature client.h or render.h
        // declares for the symbol of its name.
        *unsafe { $library.get(concat!($name, "\0").as_bytes()) }
            .map_err(|e| Error::Load(format!("{}: {e}", $name)))?
    };
}

impl Lib {
    /// # Safety
    /// `path` must name a real libmpv: loading it runs its start-up code, and
    /// its symbols are taken to have the signatures above.
    pub(super) unsafe fn load(path: &Path) -> Result<Self> {
        // SAFETY: the caller vouches that `path` is a libmpv.
        let library = unsafe { libloading::Library::new(path) }
            .map_err(|e| Error::Load(format!("{}: {e}", path.display())))?;
        Ok(Self {
            mpv_create: sym!(library, "mpv_create"),
            mpv_initialize: sym!(library, "mpv_initialize"),
            mpv_terminate_destroy: sym!(library, "mpv_terminate_destroy"),
            mpv_set_option_string: sym!(library, "mpv_set_option_string"),
            mpv_set_property_string: sym!(library, "mpv_set_property_string"),
            mpv_get_property_string: sym!(library, "mpv_get_property_string"),
            mpv_free: sym!(library, "mpv_free"),
            mpv_error_string: sym!(library, "mpv_error_string"),
            mpv_command_async: sym!(library, "mpv_command_async"),
            mpv_command: sym!(library, "mpv_command"),
            mpv_observe_property: sym!(library, "mpv_observe_property"),
            mpv_wait_event: sym!(library, "mpv_wait_event"),
            mpv_set_wakeup_callback: sym!(library, "mpv_set_wakeup_callback"),
            mpv_render_context_create: sym!(library, "mpv_render_context_create"),
            mpv_render_context_set_update_callback: sym!(
                library,
                "mpv_render_context_set_update_callback"
            ),
            mpv_render_context_update: sym!(library, "mpv_render_context_update"),
            mpv_render_context_render: sym!(library, "mpv_render_context_render"),
            mpv_render_context_report_swap: sym!(library, "mpv_render_context_report_swap"),
            mpv_render_context_free: sym!(library, "mpv_render_context_free"),
            _library: ManuallyDrop::new(library),
        })
    }
}
