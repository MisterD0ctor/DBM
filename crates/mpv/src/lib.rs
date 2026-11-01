//! Direct libmpv bindings for the render-API path.
//!
//! Only the handful of entry points this player calls: options and properties
//! as strings, async commands, the event queue, and the render context that
//! draws into Slint's own GL context. Bound here rather than through a
//! wrapper because the render API, which the architecture rests on, is what
//! a wrapper is likeliest to leave out.
//!
//! Loaded at runtime with `libloading` rather than linked, which keeps the
//! build free of an import library and lets the library be swapped without
//! recompiling: the vendored build on Windows, the system's or the Flatpak's
//! on Linux, or whatever `DBM_LIBMPV` names.
//!
//! `tracks` reads the list-shaped properties into types of their own. This
//! is the only crate in the player that calls C.

#![warn(clippy::undocumented_unsafe_blocks)]

pub mod tracks;

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

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
    /// How mpv learns the target can hold alpha. Left at 0, mpv treats the
    /// surface as opaque and fills the letterbox with black whatever
    /// `--background` says, erasing the ambient border.
    internal_format: c_int,
}

/// `GL_RGBA8`. Hardcoded rather than pulled from glow so this module stays
/// free of a GL binding dependency.
const GL_RGBA8: c_int = 0x8058;

// `mpv_format`, the scalar kinds.
const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_DOUBLE: c_int = 5;

/// What to receive an observed property as. Only the scalar kinds: anything
/// list-shaped (tracks, playlist) is pulled with a command when its count
/// changes, which avoids marshalling a node tree across the FFI boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    String,
    Flag,
    Double,
}

impl Format {
    const fn code(self) -> c_int {
        match self {
            Self::String => FORMAT_STRING,
            Self::Flag => FORMAT_FLAG,
            Self::Double => FORMAT_DOUBLE,
        }
    }
}

// `mpv_event_id`, the handful we act on.
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

/// `mpv_event_end_file`. Only the first three fields are read; the rest of
/// the struct is longer and is never touched, which is safe because mpv owns
/// the allocation and we only ever read a prefix of it. The entry id has been
/// there since mpv 0.33.
#[repr(C)]
struct RawEventEndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
}

/// `MPV_END_FILE_REASON_ERROR`. The other reasons (EOF, stop, quit,
/// redirect) are ordinary endings.
const END_FILE_REASON_ERROR: c_int = 4;
/// `MPV_END_FILE_REASON_EOF`: the file played to its end. With the window
/// held open only at the end of the list, that is autoplay moving on.
/// Asked-for changes end a file with a stop.
const END_FILE_REASON_EOF: c_int = 0;

#[repr(C)]
struct RawEventProperty {
    /// Unread: the id the property was observed under says which it is.
    _name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

/// A value mpv reported for an observed property.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Flag(bool),
    Double(f64),
    Str(String),
    /// The property exists but has no value, like `time-pos` before anything
    /// is loaded. Distinct from "unchanged".
    Unset,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// An observed property changed. `id` is whatever it was observed
    /// under (see [`Mpv::observe`]).
    Property {
        id: u64,
        value: Value,
    },
    /// An async command finished. `id` is whatever was passed to
    /// [`Mpv::command_async`], which is how a caller recognises its own, and
    /// `failed` says mpv refused it or couldn't carry it out.
    CommandReply {
        id: u64,
        failed: bool,
    },
    StartFile,
    FileLoaded,
    /// A file stopped playing. `failure` carries mpv's own description when it
    /// stopped because it couldn't be played, and is `None` for every ordinary
    /// ending, so the interface can tell "the film finished" from "that file
    /// isn't a video".
    ///
    /// `entry` is which playlist entry it was, by mpv's id, since a file that
    /// fails at once is over before `path` has been reported for it.
    EndFile {
        failure: Option<String>,
        entry: i64,
        /// Played to its end, rather than stopped or failed.
        eof: bool,
    },
    Seek,
    PlaybackRestart,
    Shutdown,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load libmpv: {0}")]
    Load(String),
    #[error("mpv_create returned null")]
    Create,
    /// An mpv API call returned a negative status code.
    #[error("{call} failed with mpv error {code}")]
    Api { call: &'static str, code: c_int },
    #[error("string contained an interior NUL")]
    Nul,
}

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
    /// Never unloaded: mpv's script threads are detached, and one can still
    /// be on its way out of mpv's code after `mpv_terminate_destroy` returns.
    /// Unloading then would pull that code from under it, a crash at quit.
    _library: ManuallyDrop<libloading::Library>,
    mpv_create: unsafe extern "C" fn() -> *mut MpvHandle,
    mpv_initialize: unsafe extern "C" fn(*mut MpvHandle) -> c_int,
    mpv_terminate_destroy: unsafe extern "C" fn(*mut MpvHandle),
    mpv_set_option_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    mpv_set_property_string:
        unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    mpv_get_property_string: unsafe extern "C" fn(*mut MpvHandle, *const c_char) -> *mut c_char,
    mpv_free: unsafe extern "C" fn(*mut c_void),
    /// Turns an mpv error code into a sentence, "Unrecognized file format"
    /// rather than "error -13".
    mpv_error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    mpv_command_async: unsafe extern "C" fn(*mut MpvHandle, u64, *mut *const c_char) -> c_int,
    mpv_command: unsafe extern "C" fn(*mut MpvHandle, *mut *const c_char) -> c_int,
    mpv_observe_property: unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int) -> c_int,
    mpv_wait_event: unsafe extern "C" fn(*mut MpvHandle, f64) -> *mut RawEvent,
    mpv_set_wakeup_callback: unsafe extern "C" fn(
        *mut MpvHandle,
        Option<unsafe extern "C" fn(*mut c_void)>,
        *mut c_void,
    ),
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
    unsafe fn load(path: &Path) -> Result<Self> {
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

// ---------------------------------------------------------------------------
// Player handle
// ---------------------------------------------------------------------------

/// A callback mpv makes from a thread of its own, so it may only nudge the
/// UI loop.
pub type Wake = Box<dyn Fn() + Send + Sync>;

/// A libmpv player. `handle` is live from `new` until `drop`, which every
/// `SAFETY` note in this file assumes.
pub struct Mpv {
    lib: Lib,
    handle: *mut MpvHandle,
    /// The closure `set_wakeup` handed mpv, freed once mpv can no longer
    /// call it.
    wakeup: Mutex<*mut Wake>,
    /// Whether mpv leaves what is outside the picture transparent rather
    /// than black; see [`Mpv::clear_surround`].
    clear_surround: bool,
}

/// No colour at all, in mpv's `r/g/b/a` form.
const CLEAR: &str = "0/0/0/0";

// SAFETY: libmpv's client API is thread-safe for properties and commands, so
// a handle may be shared. What is not is kept out of the safe methods:
//
//   * `poll_event` wraps `mpv_wait_event`, which only one thread may call at
//     a time, so it is `unsafe` and its caller keeps it to one thread.
//   * the render context (`RenderContext`) belongs to the GL thread, and is
//     not reachable from here.
//   * the wakeup closure is only replaced or freed under its lock.
//
// Anything else (`get_property`, `set_property`, `observe`, `command_async`,
// `command`) is safe to call from the worker thread.
unsafe impl Send for Mpv {}
// SAFETY: as for `Send`, above.
unsafe impl Sync for Mpv {}

impl Mpv {
    /// Create and initialize a player configured for render-API output.
    ///
    /// `vo=libmpv` is mandatory: it makes mpv present through the render
    /// context instead of a window of its own, so there is no compositor
    /// boundary between the video and the UI, and the UI can sample it.
    ///
    /// `configure` runs after the built-in options and before
    /// `mpv_initialize`, the only window in which some options, like
    /// `watch-later-directory`, can be set.
    ///
    /// # Safety
    /// `library` must name a libmpv: loading it runs its start-up code, and
    /// its symbols are taken to have the signatures client.h gives them.
    pub unsafe fn new(library: &Path, configure: impl FnOnce(&Self)) -> Result<Self> {
        // SAFETY: the caller vouches for `library`.
        let lib = unsafe { Lib::load(library) }?;
        // SAFETY: takes nothing, and a null result is checked below.
        let handle = unsafe { (lib.mpv_create)() };
        if handle.is_null() {
            return Err(Error::Create);
        }
        let mut mpv = Self {
            lib,
            handle,
            wakeup: Mutex::new(std::ptr::null_mut()),
            clear_surround: false,
        };

        // `vo=libmpv` is the one option that must land. The rest are tuning,
        // and which exist varies across mpv versions (the transparent-padding
        // option was renamed), so a rejected one is reported and stepped
        // over.
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
                log::warn!("mpv rejected {k}={v} ({e}); continuing");
            }
        }

        // A clear surround, so a subtitle mpv lays in the letterbox keeps its
        // own alpha and the border pass can put it over the ambient border.
        // The colour was `background` itself before mpv 0.38. Refused both
        // ways, the surround is opaque black and the border pass discards it.
        mpv.clear_surround = ["background-color", "background"]
            .iter()
            .any(|k| mpv.set_option(k, CLEAR).is_ok());
        if !mpv.clear_surround {
            log::warn!("mpv kept an opaque surround; the border will cover its letterbox");
        }

        configure(&mpv);

        // SAFETY: `handle` is live and not yet initialized.
        check("mpv_initialize", unsafe {
            (mpv.lib.mpv_initialize)(mpv.handle)
        })?;
        Ok(mpv)
    }

    /// Whether what mpv draws outside the picture comes with its own alpha
    /// over a transparent surround, so it can be laid over something else.
    pub fn clear_surround(&self) -> bool {
        self.clear_surround
    }

    pub fn set_option(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        // SAFETY: `handle` is live, and `n` and `v` outlive the call.
        check("mpv_set_option_string", unsafe {
            (self.lib.mpv_set_option_string)(self.handle, n.as_ptr(), v.as_ptr())
        })
    }

    /// Set a property and wait for mpv to take it.
    ///
    /// **Blocking**, like [`Mpv::get_property`]: worker thread only. Anything
    /// on an interactive path goes through `commands`, which queues a `set`.
    pub fn set_property(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        // SAFETY: `handle` is live, and `n` and `v` outlive the call.
        check("mpv_set_property_string", unsafe {
            (self.lib.mpv_set_property_string)(self.handle, n.as_ptr(), v.as_ptr())
        })
    }

    /// Read a property as a string.
    ///
    /// **Blocking.** Waits on mpv's core, which under load costs tens to
    /// hundreds of milliseconds, so call it from the worker thread, never the
    /// frame path (see `worker`).
    ///
    /// `None` when mpv has no value for it yet, which is the normal state for
    /// anything video-shaped until a file is loaded and the first frame has
    /// been decoded.
    pub fn get_property(&self, name: &str) -> Option<String> {
        let n = cstr(name).ok()?;
        // SAFETY: `handle` is live, and `n` outlives the call.
        let raw = unsafe { (self.lib.mpv_get_property_string)(self.handle, n.as_ptr()) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: non-null, NUL-terminated, and mpv's until the `mpv_free` below.
        let out = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: mpv allocated it, so it goes back to mpv rather than to
        // Rust, and nothing reads it after.
        unsafe { (self.lib.mpv_free)(raw.cast()) };
        Some(out)
    }

    /// Read a flag property. mpv renders these as "yes"/"no" in string form.
    /// A missing property reads as false, which is the useful default for
    /// every flag this app asks about.
    pub fn get_bool(&self, name: &str) -> bool {
        matches!(
            self.get_property(name).as_deref(),
            Some("yes") | Some("true")
        )
    }

    /// Convenience for the numeric properties this app reads.
    pub fn get_f64(&self, name: &str) -> Option<f64> {
        self.get_property(name)?.parse().ok()
    }

    /// Ask mpv to report changes to `name`. Values arrive through
    /// [`Mpv::poll_event`] as [`Event::Property`], under `id`, which is
    /// cheaper to tell apart than the name, copied out on every change.
    pub fn observe(&self, id: u64, name: &str, format: Format) -> Result<()> {
        let n = cstr(name)?;
        // SAFETY: `handle` is live, and `n` outlives the call.
        check("mpv_observe_property", unsafe {
            (self.lib.mpv_observe_property)(self.handle, id, n.as_ptr(), format.code())
        })
    }

    /// Called from an mpv thread whenever the event queue becomes non-empty.
    /// The callback must not re-enter mpv, so in practice it only nudges the
    /// UI loop into draining.
    pub fn set_wakeup(&self, notify: Wake) {
        let raw = Box::into_raw(Box::new(notify));
        let mut held = self.wakeup.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `handle` is live, and `raw` lives until it is replaced here
        // or `drop` frees it once mpv can no longer call it.
        unsafe {
            (self.lib.mpv_set_wakeup_callback)(self.handle, Some(wakeup_thunk), raw.cast());
        };
        // mpv calls the closure under the lock it replaces it under, so the
        // one replaced can't still be running.
        let replaced = std::mem::replace(&mut *held, raw);
        if !replaced.is_null() {
            // SAFETY: it came from `Box::into_raw` on an earlier call, and mpv
            // is done with it.
            drop(unsafe { Box::from_raw(replaced) });
        }
    }

    /// Drain one queued event, or `None` when the queue is empty.
    ///
    /// Non-blocking. The struct mpv hands back is only valid until the next
    /// call on this handle, so everything is copied out before returning.
    ///
    /// # Safety
    /// Never on two threads at once: mpv hands every caller the same struct.
    pub unsafe fn poll_event(&self) -> Option<Event> {
        // SAFETY: `handle` is live, and the caller keeps this to one thread.
        let raw = unsafe { (self.lib.mpv_wait_event)(self.handle, 0.0) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: non-null, and valid until the next `mpv_wait_event`, which
        // cannot come while `ev` is borrowed from `self`.
        let ev = unsafe { &*raw };
        match ev.event_id {
            EVENT_SHUTDOWN => Some(Event::Shutdown),
            EVENT_COMMAND_REPLY => Some(Event::CommandReply {
                id: ev.reply_userdata,
                failed: ev.error < 0,
            }),
            EVENT_START_FILE => Some(Event::StartFile),
            EVENT_FILE_LOADED => Some(Event::FileLoaded),
            EVENT_END_FILE => {
                // Absent data means an ending with no detail, which is the
                // ordinary case and not a failure.
                let end = (!ev.data.is_null()).then(|| {
                    // SAFETY: an end-file event's `data` is an
                    // `mpv_event_end_file`, which `RawEventEndFile` is a prefix of.
                    unsafe { &*ev.data.cast::<RawEventEndFile>() }
                });
                let failure = end
                    .filter(|end| end.reason == END_FILE_REASON_ERROR)
                    .map(|end| self.error_text(end.error));
                Some(Event::EndFile {
                    failure,
                    entry: end.map_or(0, |end| end.playlist_entry_id),
                    eof: end.is_some_and(|end| end.reason == END_FILE_REASON_EOF),
                })
            }
            EVENT_SEEK => Some(Event::Seek),
            EVENT_PLAYBACK_RESTART => Some(Event::PlaybackRestart),
            EVENT_PROPERTY_CHANGE => {
                if ev.data.is_null() {
                    return None;
                }
                // SAFETY: a property event's `data` is an `mpv_event_property`.
                let prop = unsafe { &*ev.data.cast::<RawEventProperty>() };
                // A null `data` means the property has no value right now,
                // which is normal and not an error.
                let value = if prop.data.is_null() {
                    Value::Unset
                } else {
                    match prop.format {
                        // SAFETY: a flag's `data` points at an `int`.
                        FORMAT_FLAG => Value::Flag(unsafe { *prop.data.cast::<c_int>() } != 0),
                        // SAFETY: a double's `data` points at a `double`.
                        FORMAT_DOUBLE => Value::Double(unsafe { *prop.data.cast::<f64>() }),
                        FORMAT_STRING => {
                            // SAFETY: a string's `data` points at a `char *`.
                            let s = unsafe { *prop.data.cast::<*const c_char>() };
                            if s.is_null() {
                                Value::Unset
                            } else {
                                // SAFETY: non-null, NUL-terminated, and mpv's
                                // until the next event.
                                let text = unsafe { CStr::from_ptr(s) };
                                Value::Str(text.to_string_lossy().into_owned())
                            }
                        }
                        _ => Value::Unset,
                    }
                };
                Some(Event::Property {
                    id: ev.reply_userdata,
                    value,
                })
            }
            // `MPV_EVENT_NONE` for an empty queue, or an event nothing here acts on.
            _ => None,
        }
    }

    /// mpv's own description of an error code.
    fn error_text(&self, code: c_int) -> String {
        // SAFETY: takes any code at all.
        let raw = unsafe { (self.lib.mpv_error_string)(code) };
        if raw.is_null() {
            return "unknown error".into();
        }
        // SAFETY: non-null and NUL-terminated, in static storage mpv owns, so
        // it is not ours to free.
        unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned()
    }

    /// Queue a command and return immediately.
    ///
    /// What the UI thread uses while its event loop runs, since
    /// [`Mpv::command`] waits for mpv's core, which for an exact seek locks it
    /// for about 200ms. Completion arrives as [`Event::CommandReply`] carrying
    /// `reply_id`.
    pub fn command_async(&self, reply_id: u64, args: &[&str]) -> Result<()> {
        check(
            "mpv_command_async",
            // SAFETY: `handle` is live, and `argv` is null-terminated and
            // outlives the call.
            with_argv(args, |argv| unsafe {
                (self.lib.mpv_command_async)(self.handle, reply_id, argv)
            })?,
        )
    }

    /// Run a command and wait until mpv has carried it out.
    ///
    /// Only where nothing is waiting on this thread: the worker, or the UI
    /// thread once its event loop has returned. libmpv's render thread must
    /// never wait on mpv while it still renders through it.
    pub fn command(&self, args: &[&str]) -> Result<()> {
        check(
            "mpv_command",
            // SAFETY: `handle` is live, and `argv` is null-terminated and
            // outlives the call.
            with_argv(args, |argv| unsafe {
                (self.lib.mpv_command)(self.handle, argv)
            })?,
        )
    }
}

/// `args` as the null-terminated array of C strings mpv's commands take,
/// alive for as long as `f` runs.
fn with_argv<T>(args: &[&str], f: impl FnOnce(*mut *const c_char) -> T) -> Result<T> {
    let owned: Vec<CString> = args.iter().map(|a| cstr(a)).collect::<Result<_>>()?;
    let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    Ok(f(ptrs.as_mut_ptr()))
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // SAFETY: the render context holds an `Arc` of this, so it is already
        // gone, and nothing uses `handle` after.
        unsafe { (self.lib.mpv_terminate_destroy)(self.handle) };
        // Only now, since mpv can still wake while it terminates.
        let wakeup = *self
            .wakeup
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        if !wakeup.is_null() {
            // SAFETY: it came from `Box::into_raw` in `set_wakeup`, and mpv
            // has terminated.
            drop(unsafe { Box::from_raw(wakeup) });
        }
    }
}

unsafe extern "C" fn wakeup_thunk(ctx: *mut c_void) {
    // SAFETY: `ctx` is the box `set_wakeup` registered, which lives until mpv
    // can no longer call this.
    let notify = unsafe { &*ctx.cast::<Wake>() };
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
    // SAFETY: mpv names the function in a NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) };
    if name == c"glFenceSync" {
        return no_fence as *mut c_void;
    }
    // SAFETY: `ctx` points at the closure reference `RenderContext::new`
    // passed, alive for the whole of the call this comes from.
    let f = unsafe { &*ctx.cast::<&dyn Fn(&CStr) -> *const c_void>() };
    f(name) as *mut c_void
}

/// What mpv is given for `glFenceSync`: no fence, ever.
///
/// Up to and including 0.41, mpv's GL renderer makes a fence for every frame
/// it draws and deletes them only in a buffer swap, which the render API never
/// does (fixed upstream in f74adc4, in no release yet). Each costs the driver
/// some kilobytes, NVIDIA's about five, so a film grew the player by its frame
/// rate times that, for as long as it played. A null fence is one mpv simply
/// doesn't keep. Its only other fences guard the upload buffers of direct
/// rendering, which libmpv uses only with `MPV_RENDER_PARAM_ADVANCED_CONTROL`,
/// never asked for here; asking for it would need real fences back.
unsafe extern "C" fn no_fence(_condition: u32, _flags: u32) -> *const c_void {
    std::ptr::null()
}

/// mpv's "a frame is ready" notification. Fires on an mpv-owned thread, so it
/// must not touch GL - it only pokes the Slint event loop, which comes back to
/// us on the UI thread in `BeforeRendering`.
unsafe extern "C" fn update_thunk(ctx: *mut c_void) {
    // SAFETY: `ctx` is the box `RenderContext::new` registered, freed only
    // after `drop` unregisters it.
    let notify = unsafe { &*ctx.cast::<Wake>() };
    notify();
}

/// mpv drawing into Slint's GL context. Every call into it, dropping it
/// included, needs that context current, as it is inside Slint's rendering
/// notifier.
pub struct RenderContext {
    /// Held so the handle outlives the render context, which mpv requires
    /// to be freed first.
    mpv: Arc<Mpv>,
    ctx: *mut MpvRenderCtx,
    /// Kept alive as long as mpv might invoke it, and freed in `Drop` only
    /// after the update callback has been unregistered.
    notify: *mut Wake,
}

impl RenderContext {
    /// Build the render context against the GL context Slint is already
    /// using. Must be called from a rendering-notifier callback, where that
    /// context is current and `get_proc_address` is live.
    ///
    /// # Safety
    /// The caller guarantees a current OpenGL context.
    pub unsafe fn new(
        mpv: Arc<Mpv>,
        get_proc_address: &dyn Fn(&CStr) -> *const c_void,
        notify: Wake,
    ) -> Result<Self> {
        let api_type = cstr("opengl")?;
        let mut init = OpenglInitParams {
            get_proc_address: Some(get_proc_address_thunk),
            ctx: (&raw const get_proc_address).cast_mut().cast(),
        };
        let mut params = [
            RenderParam {
                kind: RENDER_PARAM_API_TYPE,
                data: api_type.as_ptr().cast_mut().cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: (&raw mut init).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];

        let mut ctx: *mut MpvRenderCtx = std::ptr::null_mut();
        // SAFETY: the caller has a GL context current, `handle` is live, and
        // `params` and all it points at outlive the call.
        check("mpv_render_context_create", unsafe {
            (mpv.lib.mpv_render_context_create)(&mut ctx, mpv.handle, params.as_mut_ptr())
        })?;
        drop(api_type);

        let notify = Box::into_raw(Box::new(notify));
        // SAFETY: `ctx` was just made, and `notify` lives until `drop`
        // unregisters it.
        unsafe {
            (mpv.lib.mpv_render_context_set_update_callback)(
                ctx,
                Some(update_thunk),
                notify.cast(),
            );
        };

        Ok(Self { mpv, ctx, notify })
    }

    fn lib(&self) -> &Lib {
        &self.mpv.lib
    }

    /// True when mpv has a new frame waiting. Cheap enough to poll every frame.
    ///
    /// # Safety
    /// Requires the same current GL context the render context was made on.
    pub unsafe fn wants_redraw(&self) -> bool {
        // SAFETY: the caller has the context current, and `ctx` is live until `drop`.
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
        // 0 leaves row 0 of the texture holding the top of the image (checked
        // against a red-on-top clip), the top-down convention Slint's
        // `TopLeft` origin, the `osd-dimensions` margins and our passes
        // assume. 1 would flip it to GL's bottom-up order.
        let mut flip: c_int = 0;
        // Do not sleep until the frame is due.
        //
        // mpv's default blocks until the frame's display time, which on the
        // UI thread would mean a 33ms sleep per frame for a 30fps film, and
        // nothing could move faster than the film. `report_swap` keeps mpv's
        // timing informed instead.
        let mut block: c_int = 0;
        let mut params = [
            RenderParam {
                kind: RENDER_PARAM_OPENGL_FBO,
                data: (&raw mut target).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_FLIP_Y,
                data: (&raw mut flip).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_BLOCK_FOR_TARGET_TIME,
                data: (&raw mut block).cast(),
            },
            RenderParam {
                kind: RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: the caller has the context current, `ctx` is live, and
        // `params` and all it points at outlive the call.
        check("mpv_render_context_render", unsafe {
            (self.lib().mpv_render_context_render)(self.ctx, params.as_mut_ptr())
        })
    }

    /// Tell mpv the frame reached the screen, so its frame timing stays honest.
    ///
    /// # Safety
    /// Requires the same current GL context the render context was made on.
    pub unsafe fn report_swap(&self) {
        // SAFETY: the caller has the context current, and `ctx` is live until `drop`.
        unsafe { (self.lib().mpv_render_context_report_swap)(self.ctx) };
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        // SAFETY: it drops inside Slint's notifier, with the context current
        // (see the type's docs), and `notify` came from `Box::into_raw`.
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
