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
//! `ffi` is the C side: the types as mpv's headers lay them out, and the
//! entry points loaded from the library. `render` draws through the render
//! API, and `tracks` reads the list-shaped properties into types of their
//! own. This is the only crate in the player that calls C.

#![warn(clippy::undocumented_unsafe_blocks)]

mod ffi;
mod render;
pub mod tracks;

pub use render::RenderContext;

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use ffi::{
    END_FILE_REASON_EOF, END_FILE_REASON_ERROR, EVENT_COMMAND_REPLY, EVENT_END_FILE,
    EVENT_FILE_LOADED, EVENT_PLAYBACK_RESTART, EVENT_PROPERTY_CHANGE, EVENT_SEEK, EVENT_SHUTDOWN,
    EVENT_START_FILE, FORMAT_DOUBLE, FORMAT_FLAG, FORMAT_STRING, Lib, MpvHandle, RawEventEndFile,
    RawEventProperty,
};

/// What to receive an observed property as. Only the scalar kinds: anything
/// list-shaped (tracks, playlist) is read as JSON when its count changes
/// (see `tracks`), which avoids marshalling a node tree across the FFI
/// boundary.
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
