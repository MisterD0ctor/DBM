//! Direct libmpv bindings for the render-API path.
//!
//! The Tauri build talks to mpv through `libmpv-wrapper.dll`, which marshals
//! every call as JSON because that was the shape the IPC boundary wanted.
//! There is no IPC boundary here, and the wrapper never exported
//! `mpv_render_context_*` in the first place, so this binds libmpv directly.
//!
//! Loaded at runtime with `libloading` rather than linked, which keeps the
//! build free of an import library and lets the DLL be swapped: the vendored
//! fork today, stock libmpv once the border shader lives in our own pipeline
//! and the fork stops being needed.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Raw types - mirrors client.h / render.h / render_gl.h
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct MpvHandle {
    _opaque: [u8; 0],
}

#[repr(C)]
pub struct MpvRenderCtx {
    _opaque: [u8; 0],
}

const RENDER_PARAM_INVALID: c_int = 0;
const RENDER_PARAM_API_TYPE: c_int = 1;
const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const RENDER_PARAM_OPENGL_FBO: c_int = 3;
const RENDER_PARAM_FLIP_Y: c_int = 4;
/// `MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME`. Defaults to 1, which makes
/// `mpv_render_context_render` sleep until the frame is due to be shown.
const RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;

/// `mpv_render_context_update` sets this bit when a new frame is ready.
const UPDATE_FRAME: u64 = 1;

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct OpenglInitParams {
    get_proc_address: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void>,
    ctx: *mut c_void,
}

#[repr(C)]
struct OpenglFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    /// The attachment's GL internal format, or 0 for "unknown".
    ///
    /// This is not cosmetic: it is how mpv learns the target can hold alpha.
    /// Left at 0, mpv treats the surface as opaque and fills the letterbox
    /// with black no matter what `--background` says — which erases the
    /// ambient border, since the border pass uses the video's own alpha as
    /// its mask.
    internal_format: c_int,
}

/// `GL_RGBA8`. Hardcoded rather than pulled from glow so this module stays
/// free of a GL binding dependency.
const GL_RGBA8: c_int = 0x8058;

/// `mpv_format`. We only observe the scalar kinds; anything list-shaped
/// (tracks, playlist) is pulled with a command when its count changes,
/// which avoids marshalling a node tree across the FFI boundary.
#[allow(dead_code)]
pub const FORMAT_NONE: c_int = 0;
pub const FORMAT_STRING: c_int = 1;
pub const FORMAT_FLAG: c_int = 3;
pub const FORMAT_DOUBLE: c_int = 5;

// `mpv_event_id`, the handful we act on.
const EVENT_NONE: c_int = 0;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_START_FILE: c_int = 6;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_SEEK: c_int = 20;
const EVENT_PLAYBACK_RESTART: c_int = 21;
const EVENT_COMMAND_REPLY: c_int = 5;
const EVENT_PROPERTY_CHANGE: c_int = 22;

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

/// A value mpv reported for an observed property.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Flag(bool),
    Double(f64),
    Str(String),
    /// The property exists but currently has no value - `time-pos` before
    /// anything is loaded, for instance. Distinct from "unchanged".
    Unset,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Property { name: String, value: Value },
    /// An async command finished. `id` is whatever was passed to
    /// [`Mpv::command_async`], which is how a caller recognises its own.
    CommandReply { id: u64 },
    StartFile,
    FileLoaded,
    EndFile,
    Seek,
    PlaybackRestart,
    Shutdown,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    Load(String),
    Create,
    /// An mpv API call returned a negative status code.
    Api { call: &'static str, code: c_int },
    Nul,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Load(m) => write!(f, "failed to load libmpv: {m}"),
            Error::Create => write!(f, "mpv_create returned null"),
            Error::Api { call, code } => write!(f, "{call} failed with mpv error {code}"),
            Error::Nul => write!(f, "string contained an interior NUL"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn check(call: &'static str, code: c_int) -> Result<()> {
    if code < 0 {
        Err(Error::Api { call, code })
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Dynamically loaded entry points
// ---------------------------------------------------------------------------

struct Lib {
    _library: libloading::Library,
    mpv_create: unsafe extern "C" fn() -> *mut MpvHandle,
    mpv_initialize: unsafe extern "C" fn(*mut MpvHandle) -> c_int,
    mpv_terminate_destroy: unsafe extern "C" fn(*mut MpvHandle),
    mpv_set_option_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    mpv_set_property_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    mpv_get_property_string: unsafe extern "C" fn(*mut MpvHandle, *const c_char) -> *mut c_char,
    mpv_free: unsafe extern "C" fn(*mut c_void),
    mpv_command_async:
        unsafe extern "C" fn(*mut MpvHandle, u64, *mut *const c_char) -> c_int,
    mpv_observe_property:
        unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int) -> c_int,
    mpv_wait_event: unsafe extern "C" fn(*mut MpvHandle, f64) -> *mut RawEvent,
    mpv_set_wakeup_callback:
        unsafe extern "C" fn(*mut MpvHandle, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    mpv_render_context_create:
        unsafe extern "C" fn(*mut *mut MpvRenderCtx, *mut MpvHandle, *mut RenderParam) -> c_int,
    mpv_render_context_set_update_callback: unsafe extern "C" fn(
        *mut MpvRenderCtx,
        Option<unsafe extern "C" fn(*mut c_void)>,
        *mut c_void,
    ),
    mpv_render_context_update: unsafe extern "C" fn(*mut MpvRenderCtx) -> u64,
    mpv_render_context_render: unsafe extern "C" fn(*mut MpvRenderCtx, *mut RenderParam) -> c_int,
    mpv_render_context_report_swap: unsafe extern "C" fn(*mut MpvRenderCtx),
    mpv_render_context_free: unsafe extern "C" fn(*mut MpvRenderCtx),
}

macro_rules! sym {
    ($library:expr, $name:literal) => {
        *unsafe { $library.get(concat!($name, "\0").as_bytes()) }
            .map_err(|e| Error::Load(format!("{}: {e}", $name)))?
    };
}

impl Lib {
    unsafe fn load(path: &Path) -> Result<Self> {
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
            mpv_command_async: sym!(library, "mpv_command_async"),
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
            _library: library,
        })
    }
}

/// Where to find libmpv.
///
/// `DBM_LIBMPV` wins, so a stock build can be dropped in without touching the
/// tree. Otherwise the copy this app ships with, and failing that the bare
/// name, which hands the search to the system loader.
fn libmpv_path() -> PathBuf {
    if let Some(p) = std::env::var_os("DBM_LIBMPV") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) {
        "libmpv-2.dll"
    } else {
        "libmpv.so.2"
    };
    crate::paths::vendored(&[name]).unwrap_or_else(|| PathBuf::from(name))
}

// ---------------------------------------------------------------------------
// Player handle
// ---------------------------------------------------------------------------

pub struct Mpv {
    lib: Lib,
    handle: *mut MpvHandle,
}

// SAFETY: libmpv's client API is thread-safe for properties and commands, so
// a handle may be shared. Two things are *not*, and both are kept on one
// thread by construction rather than by convention:
//
//   * `poll_event` wraps `mpv_wait_event`, which mpv documents as callable
//     from a single thread only. It is called from the render driver, which
//     only ever runs on the UI thread.
//   * the render context (`RenderContext`) belongs to the GL thread, and is
//     not reachable from here.
//
// Anything else — `get_property`, `set_property`, `command`, `command_async`
// — is safe to call from the worker thread.
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Mpv {
    /// Create and initialize a player configured for render-API output.
    ///
    /// `vo=libmpv` is mandatory: it is what makes mpv defer presentation to
    /// the render context instead of creating a window of its own. That is
    /// the whole point of this path - no second HWND, so no DWM boundary
    /// between the video and the UI, so the UI can sample the video.
    /// Create and initialize a player.
    ///
    /// `configure` runs after the built-in options and before
    /// `mpv_initialize`, which is the only window in which some options —
    /// `watch-later-directory` among them — can still be set.
    pub fn new(configure: impl FnOnce(&Self)) -> Result<Self> {
        let lib = unsafe { Lib::load(&libmpv_path()) }?;
        let handle = unsafe { (lib.mpv_create)() };
        if handle.is_null() {
            return Err(Error::Create);
        }
        let mpv = Self { lib, handle };

        // `vo=libmpv` is the one option that must land — without it mpv opens
        // a window of its own and the whole point is lost. The rest are
        // tuning, and which of them exist varies across mpv versions (the
        // transparent-padding option in particular was renamed), so a
        // rejected one is reported and stepped over rather than fatal.
        mpv.set_option("vo", "libmpv")?;
        for (k, v) in [
            ("hwdec", "auto-safe"),
            ("keep-open", "always"),
            ("pause", "no"),
            ("deband", "yes"),
            ("deband-iterations", "8"),
            ("sub-visibility", "yes"),
        ] {
            if let Err(e) = mpv.set_option(k, v) {
                eprintln!("dbm: mpv rejected {k}={v} ({e}); continuing");
            }
        }

        // Note on the letterbox: mpv fills it opaquely regardless of
        // `--background`/`--alpha`, so the border pass cannot use the video's
        // alpha as its mask. It masks on mpv's reported video rectangle
        // instead, which is both version-independent and more correct — it
        // follows panscan and zoom, which alpha would only have tracked by
        // accident. Whatever mpv paints out there is simply discarded.

        configure(&mpv);

        check("mpv_initialize", unsafe {
            (mpv.lib.mpv_initialize)(mpv.handle)
        })?;
        Ok(mpv)
    }

    pub fn set_option(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        check("mpv_set_option_string", unsafe {
            (self.lib.mpv_set_option_string)(self.handle, n.as_ptr(), v.as_ptr())
        })
    }

    #[allow(dead_code)]
    pub fn set_property(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        check("mpv_set_property_string", unsafe {
            (self.lib.mpv_set_property_string)(self.handle, n.as_ptr(), v.as_ptr())
        })
    }

    /// Read a property as a string.
    ///
    /// **Blocking.** Waits on mpv's core, which under load costs tens to
    /// hundreds of milliseconds. Call it from the worker thread, never from
    /// the frame path — see `worker` for why. `None` when mpv has no value for it yet —
    /// which is the normal state for anything video-shaped until a file is
    /// loaded and the first frame has been decoded.
    pub fn get_property(&self, name: &str) -> Option<String> {
        let n = cstr(name).ok()?;
        let raw = unsafe { (self.lib.mpv_get_property_string)(self.handle, n.as_ptr()) };
        if raw.is_null() {
            return None;
        }
        let out = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
        // The string is mpv-allocated; it has to go back to mpv, not to Rust.
        unsafe { (self.lib.mpv_free)(raw as *mut c_void) };
        Some(out)
    }

    /// Read a flag property. mpv renders these as "yes"/"no" in string form.
    /// A missing property reads as false, which is the useful default for
    /// every flag this app asks about.
    pub fn get_bool(&self, name: &str) -> bool {
        matches!(self.get_property(name).as_deref(), Some("yes") | Some("true"))
    }

    /// Convenience for the numeric properties this app reads.
    pub fn get_f64(&self, name: &str) -> Option<f64> {
        self.get_property(name)?.parse().ok()
    }

    /// Queue a command and return immediately.
    ///
    /// The synchronous [`Mpv::command`] waits for mpv core to finish, which
    /// for an exact seek is around 200ms of a locked UI thread. Anything on
    /// an interactive path must use this instead; completion arrives as
    /// [`Event::CommandReply`] carrying `reply_id`.
    /// Ask mpv to report changes to `name`. Values arrive through
    /// [`Mpv::poll_event`] as [`Event::Property`].
    pub fn observe(&self, name: &str, format: c_int) -> Result<()> {
        let n = cstr(name)?;
        check("mpv_observe_property", unsafe {
            (self.lib.mpv_observe_property)(self.handle, 0, n.as_ptr(), format)
        })
    }

    /// Called from an mpv thread whenever the event queue becomes non-empty.
    /// The callback must not re-enter mpv, so in practice it only nudges the
    /// UI loop into draining.
    pub fn set_wakeup(&self, notify: Box<dyn Fn() + Send + Sync>) {
        let raw = Box::into_raw(Box::new(notify));
        unsafe {
            (self.lib.mpv_set_wakeup_callback)(self.handle, Some(wakeup_thunk), raw as *mut c_void)
        };
    }

    /// Drain one queued event, or `None` when the queue is empty.
    ///
    /// **UI thread only.** `mpv_wait_event` must not be called concurrently;
    /// see the `Send`/`Sync` note above.
    ///
    /// Non-blocking. The struct mpv hands back is only valid until the next
    /// call on this handle, so everything is copied out before returning.
    pub fn poll_event(&self) -> Option<Event> {
        let raw = unsafe { (self.lib.mpv_wait_event)(self.handle, 0.0) };
        if raw.is_null() {
            return None;
        }
        let ev = unsafe { &*raw };
        match ev.event_id {
            EVENT_NONE => None,
            EVENT_SHUTDOWN => Some(Event::Shutdown),
            EVENT_COMMAND_REPLY => Some(Event::CommandReply {
                id: ev.reply_userdata,
            }),
            EVENT_START_FILE => Some(Event::StartFile),
            EVENT_FILE_LOADED => Some(Event::FileLoaded),
            EVENT_END_FILE => Some(Event::EndFile),
            EVENT_SEEK => Some(Event::Seek),
            EVENT_PLAYBACK_RESTART => Some(Event::PlaybackRestart),
            EVENT_PROPERTY_CHANGE => {
                if ev.data.is_null() {
                    return None;
                }
                let prop = unsafe { &*(ev.data as *const RawEventProperty) };
                if prop.name.is_null() {
                    return None;
                }
                let name = unsafe { CStr::from_ptr(prop.name) }
                    .to_string_lossy()
                    .into_owned();
                // A null `data` means the property has no value right now,
                // which is normal and not an error.
                let value = if prop.data.is_null() {
                    Value::Unset
                } else {
                    match prop.format {
                        FORMAT_FLAG => Value::Flag(unsafe { *(prop.data as *const c_int) } != 0),
                        FORMAT_DOUBLE => Value::Double(unsafe { *(prop.data as *const f64) }),
                        FORMAT_STRING => {
                            let s = unsafe { *(prop.data as *const *const c_char) };
                            if s.is_null() {
                                Value::Unset
                            } else {
                                Value::Str(
                                    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned(),
                                )
                            }
                        }
                        _ => Value::Unset,
                    }
                };
                Some(Event::Property { name, value })
            }
            _ => None,
        }
    }

    pub fn command_async(&self, reply_id: u64, args: &[&str]) -> Result<()> {
        let owned: Vec<CString> = args.iter().map(|a| cstr(a)).collect::<Result<_>>()?;
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        check("mpv_command_async", unsafe {
            (self.lib.mpv_command_async)(self.handle, reply_id, ptrs.as_mut_ptr())
        })
    }

}

impl Drop for Mpv {
    fn drop(&mut self) {
        // The render context borrows this handle, so it must already be gone.
        unsafe { (self.lib.mpv_terminate_destroy)(self.handle) };
    }
}

unsafe extern "C" fn wakeup_thunk(ctx: *mut c_void) {
    let notify = unsafe { &*(ctx as *const Box<dyn Fn() + Send + Sync>) };
    notify();
}

fn cstr(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::Nul)
}

// ---------------------------------------------------------------------------
// Render context
// ---------------------------------------------------------------------------

/// Trampoline handing mpv's C-style `get_proc_address` to the Rust closure
/// Slint provides. `ctx` points at the fat pointer for that closure, which
/// stays alive for the duration of `mpv_render_context_create` - the only
/// window in which mpv calls this.
unsafe extern "C" fn get_proc_address_thunk(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    let f = unsafe { &*(ctx as *const &dyn Fn(&CStr) -> *const c_void) };
    let name = unsafe { CStr::from_ptr(name) };
    f(name) as *mut c_void
}

/// mpv's "a frame is ready" notification. Fires on an mpv-owned thread, so it
/// must not touch GL - it only pokes the Slint event loop, which comes back to
/// us on the UI thread in `BeforeRendering`.
unsafe extern "C" fn update_thunk(ctx: *mut c_void) {
    let notify = unsafe { &*(ctx as *const Box<dyn Fn() + Send + Sync>) };
    notify();
}

pub struct RenderContext {
    lib: *const Lib,
    ctx: *mut MpvRenderCtx,
    /// Kept alive as long as mpv might invoke it, and freed in `Drop` only
    /// after the update callback has been unregistered.
    notify: *mut Box<dyn Fn() + Send + Sync>,
}

impl RenderContext {
    /// Build the render context against the GL context Slint is already
    /// using. Must be called from a rendering-notifier callback, where that
    /// context is current and `get_proc_address` is live.
    ///
    /// # Safety
    /// The caller guarantees a current OpenGL context, and that `mpv`
    /// outlives the returned `RenderContext`.
    pub unsafe fn new(
        mpv: &Mpv,
        get_proc_address: &dyn Fn(&CStr) -> *const c_void,
        notify: Box<dyn Fn() + Send + Sync>,
    ) -> Result<Self> {
        let api_type = cstr("opengl")?;
        let mut init = OpenglInitParams {
            get_proc_address: Some(get_proc_address_thunk),
            ctx: &get_proc_address as *const &dyn Fn(&CStr) -> *const c_void as *mut c_void,
        };
        let mut params = [
            RenderParam {
                kind: RENDER_PARAM_API_TYPE,
                data: api_type.as_ptr() as *mut c_void,
            },
            RenderParam {
                kind: RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: &mut init as *mut _ as *mut c_void,
            },
            RenderParam {
                kind: RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];

        let mut ctx: *mut MpvRenderCtx = std::ptr::null_mut();
        check("mpv_render_context_create", unsafe {
            (mpv.lib.mpv_render_context_create)(&mut ctx, mpv.handle, params.as_mut_ptr())
        })?;
        drop(api_type);

        let notify = Box::into_raw(Box::new(notify));
        unsafe {
            (mpv.lib.mpv_render_context_set_update_callback)(
                ctx,
                Some(update_thunk),
                notify as *mut c_void,
            )
        };

        Ok(Self {
            lib: &mpv.lib as *const Lib,
            ctx,
            notify,
        })
    }

    fn lib(&self) -> &Lib {
        unsafe { &*self.lib }
    }

    /// True when mpv has a new frame waiting. Cheap; safe to poll every frame.
    pub fn wants_redraw(&self) -> bool {
        unsafe { (self.lib().mpv_render_context_update)(self.ctx) & UPDATE_FRAME != 0 }
    }

    /// Draw the current frame into `fbo` at `w` x `h`.
    ///
    /// # Safety
    /// Requires the same current GL context the render context was made on.
    pub unsafe fn render_to_fbo(&self, fbo: u32, w: i32, h: i32) -> Result<()> {
        let mut target = OpenglFbo {
            fbo: fbo as c_int,
            w,
            h,
            internal_format: GL_RGBA8,
        };
        // Verified empirically against a red-on-top test clip: 0 leaves row 0
        // of the texture holding the *top* of the image, which is the
        // top-down convention Slint's `TopLeft` origin, the `osd-dimensions`
        // margins and our own passes all already assume. Setting this to 1
        // flips to GL's bottom-up order and the picture comes out inverted.
        let mut flip: c_int = 0;
        // Do not sleep until the frame is due.
        //
        // mpv's default is to block here until the frame's target display
        // time, which suits a player whose thread has nothing else to do. On
        // this one it is the UI thread: a 30fps film means a 33ms sleep in
        // the middle of every frame, and nothing — no hover, no slider, no
        // preview — can move faster than the film does. Presentation is the
        // display's business here, and `report_swap` is what keeps mpv's
        // timing informed of it.
        let mut block: c_int = 0;
        let mut params = [
            RenderParam {
                kind: RENDER_PARAM_OPENGL_FBO,
                data: &mut target as *mut _ as *mut c_void,
            },
            RenderParam {
                kind: RENDER_PARAM_FLIP_Y,
                data: &mut flip as *mut _ as *mut c_void,
            },
            RenderParam {
                kind: RENDER_PARAM_BLOCK_FOR_TARGET_TIME,
                data: &mut block as *mut _ as *mut c_void,
            },
            RenderParam {
                kind: RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        check("mpv_render_context_render", unsafe {
            (self.lib().mpv_render_context_render)(self.ctx, params.as_mut_ptr())
        })
    }

    /// Tell mpv the frame reached the screen, so its frame timing stays honest.
    pub fn report_swap(&self) {
        unsafe { (self.lib().mpv_render_context_report_swap)(self.ctx) };
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe {
            // Unregister before freeing the closure, or a late callback from
            // mpv's thread would read freed memory.
            (self.lib().mpv_render_context_set_update_callback)(
                self.ctx,
                None,
                std::ptr::null_mut(),
            );
            (self.lib().mpv_render_context_free)(self.ctx);
            drop(Box::from_raw(self.notify));
        }
    }
}
