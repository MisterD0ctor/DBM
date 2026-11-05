//! Direct libmpv bindings for the render-API path.
//!
//! Only the handful of entry points this player calls: typed options and
//! properties, async commands, the event queue, and the render context that
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
//! entry points loaded from the library. [`prop`] names mpv's properties with
//! the types of their values, and [`Command`] its commands with theirs, so
//! nothing above this crate spells either. `render` draws through the render
//! API, and `tracks` holds the entries of the list-shaped properties. A file
//! to play is a [`MediaPath`] from here up. This is the only crate in the
//! player that calls C.

#![warn(clippy::undocumented_unsafe_blocks)]

mod command;
mod ffi;
mod media_path;
pub mod prop;
mod render;
pub mod tracks;
mod value;

pub use command::{Command, Direction, SeekTarget};
pub use media_path::MediaPath;
pub use prop::{AnyProperty, Property};
pub use render::RenderContext;
pub use value::{Format, FromMpv, ToMpv, Value};

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use ffi::{
    END_FILE_REASON_EOF, END_FILE_REASON_ERROR, EVENT_COMMAND_REPLY, EVENT_END_FILE,
    EVENT_FILE_LOADED, EVENT_PLAYBACK_RESTART, EVENT_PROPERTY_CHANGE, EVENT_SEEK,
    EVENT_SET_PROPERTY_REPLY, EVENT_SHUTDOWN, EVENT_START_FILE, FORMAT_STRING, Lib, MpvHandle,
    RawEventEndFile, RawEventProperty,
};
use prop::KeepOpen;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// An observed property changed. `id` is whatever it was observed
    /// under (see [`Mpv::observe`]). `None` is a property with no value
    /// right now, like `time-pos` before anything is loaded.
    Property {
        id: u64,
        value: Option<Value>,
    },
    /// A queued command or set finished. `id` is whatever was passed to
    /// [`Mpv::run`], which is how a caller recognises its own, and 0 for a
    /// set. `failed` says mpv refused it or couldn't carry it out.
    Reply {
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
    /// [`Mpv::run_and_wait`] gave up, or mpv shut down first.
    #[error("mpv did not answer")]
    NoReply,
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
//   * `poll_event` and `run_and_wait` wrap `mpv_wait_event`, which only one
//     thread may call at a time, so they are `unsafe` and their caller keeps
//     them to one thread.
//   * the render context (`RenderContext`) belongs to the GL thread, and is
//     not reachable from here.
//   * the wakeup closure is only replaced or freed under its lock.
//
// Anything else (`set`, `get`, `observe`, `run`) is safe from any thread.
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
        mpv.set_option(prop::VO, "libmpv")?;
        mpv.tune(prop::HWDEC, "auto-safe");
        mpv.tune(prop::KEEP_OPEN, KeepOpen::Always);
        mpv.tune(prop::PAUSE, false);
        mpv.tune(prop::DEBAND, true);
        mpv.tune(prop::DEBAND_ITERATIONS, 8);
        mpv.tune(prop::SUB_VISIBILITY, true);

        // A clear surround, so a subtitle mpv lays in the letterbox keeps its
        // own alpha and the border pass can put it over the ambient border.
        // The colour was `background` itself before mpv 0.38. Refused both
        // ways, the surround is opaque black and the border pass discards it.
        mpv.clear_surround = [prop::BACKGROUND_COLOR, prop::BACKGROUND]
            .into_iter()
            .any(|colour| mpv.set_option(colour, CLEAR).is_ok());
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

    /// Set an option before `mpv_initialize`, which some only take then.
    pub fn set_option<T: ToMpv>(&self, option: Property<T>, value: impl Into<T>) -> Result<()> {
        let name = cstr(option.name())?;
        check(
            "mpv_set_option",
            // SAFETY: `handle` is live, and `name` and the payload outlive
            // the call.
            with_payload(&value.into().to_mpv(), |format, data| unsafe {
                (self.lib.mpv_set_option)(self.handle, name.as_ptr(), format, data)
            })?,
        )
    }

    /// An option the player is better for but can do without.
    fn tune<T: ToMpv>(&self, option: Property<T>, value: impl Into<T>) {
        if let Err(e) = self.set_option(option, value) {
            log::warn!("mpv rejected {option:?} ({e}); continuing");
        }
    }

    /// Queue a property change and return at once.
    ///
    /// Queued behind every command and set before it, which mpv runs in
    /// order. Never blocking, so safe on the frame path. A refusal arrives as
    /// [`Event::Reply`] with id 0.
    pub fn set<T: ToMpv>(&self, property: Property<T>, value: impl Into<T>) -> Result<()> {
        let name = cstr(property.name())?;
        check(
            "mpv_set_property_async",
            // SAFETY: `handle` is live, `name` and the payload outlive the
            // call, and mpv copies the value before it returns.
            with_payload(&value.into().to_mpv(), |format, data| unsafe {
                (self.lib.mpv_set_property_async)(self.handle, 0, name.as_ptr(), format, data)
            })?,
        )
    }

    /// Read a property.
    ///
    /// **Blocking.** Waits on mpv's core, which under load costs tens to
    /// hundreds of milliseconds, so call it from the worker thread, never the
    /// frame path (see `worker`). It also takes the core ahead of anything
    /// still queued, so it may not see a [`Mpv::set`] made just before it.
    ///
    /// `None` when mpv has no value for it yet, which is the normal state for
    /// anything video-shaped until a file is loaded and the first frame has
    /// been decoded.
    pub fn get<T: FromMpv, A>(&self, property: Property<T, A>) -> Option<T> {
        /// Room for any scalar mpv writes back.
        #[repr(C)]
        union Slot {
            int: i64,
            string: *mut c_char,
        }
        let name = cstr(property.name()).ok()?;
        let format = T::FORMAT.code();
        let mut slot = Slot { int: 0 };
        // SAFETY: `handle` is live, `name` outlives the call, and `slot` has
        // room for any format asked for.
        let read = unsafe {
            (self.lib.mpv_get_property)(self.handle, name.as_ptr(), format, (&raw mut slot).cast())
        };
        if read < 0 {
            return None;
        }
        // SAFETY: mpv wrote a value of `format` into `slot`.
        let value = unsafe { Value::read(format, (&raw const slot).cast()) };
        if format == FORMAT_STRING {
            // SAFETY: a string read leaves an mpv allocation in `slot`, which
            // goes back to mpv now that it is copied out. Freeing null is a
            // no-op.
            unsafe { (self.lib.mpv_free)(slot.string.cast()) };
        }
        T::from_mpv(value?)
    }

    /// Ask mpv to report changes to `property`. Values arrive through
    /// [`Mpv::poll_event`] as [`Event::Property`], under `id`, which is
    /// cheaper to tell apart than the name, copied out on every change.
    pub fn observe(&self, id: u64, property: impl Into<AnyProperty>) -> Result<()> {
        let property = property.into();
        let name = cstr(property.name())?;
        // SAFETY: `handle` is live, and `name` outlives the call.
        check("mpv_observe_property", unsafe {
            (self.lib.mpv_observe_property)(
                self.handle,
                id,
                name.as_ptr(),
                property.format().code(),
            )
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
            EVENT_COMMAND_REPLY | EVENT_SET_PROPERTY_REPLY => Some(Event::Reply {
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
                Some(Event::Property {
                    id: ev.reply_userdata,
                    // SAFETY: `data` holds `format`, and stays mpv's until the
                    // next event.
                    value: unsafe { Value::read(prop.format, prop.data) },
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

    /// Queue a command and return at once.
    ///
    /// What every thread uses, since a command run with `mpv_command` waits
    /// for mpv's core, which an exact seek holds for about 200ms, and then
    /// runs ahead of whatever is still queued. Completion arrives as
    /// [`Event::Reply`] carrying `reply`. [`RUN_AND_WAIT`] is taken.
    pub fn run(&self, reply: u64, command: Command<'_>) -> Result<()> {
        check(
            "mpv_command_async",
            // SAFETY: `handle` is live, and `argv` is null-terminated and
            // outlives the call.
            with_argv(&command.args(), |argv| unsafe {
                (self.lib.mpv_command_async)(self.handle, reply, argv)
            })?,
        )
    }

    /// Queue a command and wait until mpv has carried it out, behind
    /// whatever was queued before it. Events that arrive meanwhile are
    /// dropped.
    ///
    /// For when nothing else reads the event queue any more: the UI thread
    /// once its event loop has returned. libmpv's render thread must never
    /// wait on mpv while it still renders through it.
    ///
    /// # Safety
    /// As for [`Mpv::poll_event`]: never while another thread reads events.
    pub unsafe fn run_and_wait(&self, command: Command<'_>, timeout: Duration) -> Result<()> {
        self.run(RUN_AND_WAIT, command)?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::NoReply);
            }
            // SAFETY: `handle` is live, and the caller keeps this to one
            // thread. mpv never returns null, but an empty event on timeout.
            let ev = unsafe { &*(self.lib.mpv_wait_event)(self.handle, left.as_secs_f64()) };
            match ev.event_id {
                EVENT_COMMAND_REPLY if ev.reply_userdata == RUN_AND_WAIT => {
                    return check("mpv_command_async", ev.error);
                }
                EVENT_SHUTDOWN => return Err(Error::NoReply),
                _ => {}
            }
        }
    }
}

/// The reply id [`Mpv::run_and_wait`] waits on, which no caller of
/// [`Mpv::run`] may use.
pub const RUN_AND_WAIT: u64 = u64::MAX;

/// `args` as the null-terminated array of C strings mpv's commands take,
/// alive for as long as `f` runs.
fn with_argv<T>(args: &[impl AsRef<str>], f: impl FnOnce(*mut *const c_char) -> T) -> Result<T> {
    let owned: Vec<CString> = args
        .iter()
        .map(|a| cstr(a.as_ref()))
        .collect::<Result<_>>()?;
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

/// `value` laid out as client.h says its format is, with that format, alive
/// for as long as `f` runs.
fn with_payload<T>(value: &Value, f: impl FnOnce(c_int, *mut c_void) -> T) -> Result<T> {
    let format = value.format().code();
    Ok(match value {
        Value::Flag(on) => {
            let mut on = c_int::from(*on);
            f(format, (&raw mut on).cast())
        }
        Value::Int(n) => {
            let mut n = *n;
            f(format, (&raw mut n).cast())
        }
        Value::Double(n) => {
            let mut n = *n;
            f(format, (&raw mut n).cast())
        }
        Value::Str(text) => {
            let text = cstr(text)?;
            let mut text = text.as_ptr();
            f(format, (&raw mut text).cast())
        }
    })
}

fn cstr(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::Nul)
}
