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
//!
//! A player comes as three handles, so that what may only happen on some
//! threads is a matter of who holds what: [`Mpv`], whose every call returns
//! at once; [`Reader`], which adds the reads that wait on mpv's core; and
//! [`Events`], the one reader of the event queue.

#![warn(clippy::undocumented_unsafe_blocks)]

mod command;
mod error;
mod events;
mod ffi;
mod media_path;
pub mod prop;
mod render;
pub mod tracks;
mod value;

pub use command::{Command, Direction, SeekTarget};
pub use error::{Error, ErrorCode, Result};
pub use events::{EndReason, Event, Events};
pub use media_path::MediaPath;
pub use prop::{AnyProperty, Property};
pub use render::RenderContext;
pub use value::{Format, FromMpv, ToMpv, Value};

use std::ffi::{CString, c_char, c_int, c_void};
use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use error::check;
use ffi::{FORMAT_STRING, Lib, MpvHandle};
use prop::KeepOpen;

// ---------------------------------------------------------------------------
// Player handle
// ---------------------------------------------------------------------------

/// A callback mpv makes from a thread of its own, so it may only nudge the
/// UI loop.
pub type Wake = Box<dyn Fn() + Send + Sync>;

/// A libmpv player. `handle` is live from `new` until `drop`, which every
/// `SAFETY` note in this crate assumes.
pub struct Mpv {
    lib: Lib,
    handle: *mut MpvHandle,
    /// The closure `set_wakeup` handed mpv, freed once mpv can no longer
    /// call it.
    wakeup: Mutex<*mut Wake>,
    /// Whether mpv leaves what is outside the picture transparent rather
    /// than black; see [`Mpv::clear_surround`].
    clear_surround: bool,
    /// Whether the player has asked for a file and not yet heard it load.
    /// Read by the resume saving, which must not write a place for a file
    /// mpv has not opened yet.
    opening: AtomicBool,
}

/// No colour at all, in mpv's `r/g/b/a` form.
const CLEAR: &str = "0/0/0/0";

// SAFETY: libmpv's client API is thread-safe for properties and commands, so
// a handle may be shared. What is not is kept out of `Mpv`'s methods:
//
//   * `mpv_wait_event`, which only one thread may call at a time, is only
//     called by `Events`, of which there is one, and which reads through
//     `&mut self`.
//   * the render context (`RenderContext`) belongs to the GL thread, and is
//     not reachable from here.
//   * the wakeup closure is only replaced or freed under its lock.
unsafe impl Send for Mpv {}
// SAFETY: as for `Send`, above.
unsafe impl Sync for Mpv {}

impl Mpv {
    /// Create and initialize a player configured for render-API output, and
    /// hand it back as its three handles.
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
    pub unsafe fn new(
        library: &Path,
        configure: impl FnOnce(&Options<'_>),
    ) -> Result<(Arc<Self>, Events, Reader)> {
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
            opening: AtomicBool::new(false),
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

        configure(&Options(&mpv));

        // SAFETY: `handle` is live and not yet initialized.
        check("mpv_initialize", unsafe {
            (mpv.lib.mpv_initialize)(mpv.handle)
        })?;
        let mpv = Arc::new(mpv);
        Ok((mpv.clone(), Events { mpv: mpv.clone() }, Reader(mpv)))
    }

    /// Whether what mpv draws outside the picture comes with its own alpha
    /// over a transparent surround, so it can be laid over something else.
    pub fn clear_surround(&self) -> bool {
        self.clear_surround
    }

    /// A file has been asked for and has not loaded yet.
    pub fn begin_opening(&self) {
        self.opening.store(true, Ordering::Relaxed);
    }

    /// The file asked for has loaded, or the move to it went nowhere.
    pub fn end_opening(&self) {
        self.opening.store(false, Ordering::Relaxed);
    }

    /// Whether a file is still on its way in.
    pub fn is_opening(&self) -> bool {
        self.opening.load(Ordering::Relaxed)
    }

    /// Set an option before `mpv_initialize`, which some only take then.
    fn set_option<T: ToMpv>(&self, option: Property<T>, value: impl Into<T>) -> Result<()> {
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

    /// Ask mpv to report changes to `property`. Values arrive through
    /// [`Events::poll`] as [`Event::Property`], under `id`, which is cheaper
    /// to tell apart than the name, copied out on every change.
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

    /// Queue a command and return at once.
    ///
    /// What every thread uses, since a command run with `mpv_command` waits
    /// for mpv's core, which an exact seek holds for about 200ms, and then
    /// runs ahead of whatever is still queued. Completion arrives as
    /// [`Event::Reply`] carrying `reply`.
    pub fn run(&self, reply: u32, command: Command<'_>) -> Result<()> {
        self.queue(reply.into(), command)
    }

    fn queue(&self, reply: u64, command: Command<'_>) -> Result<()> {
        check(
            "mpv_command_async",
            // SAFETY: `handle` is live, and `argv` is null-terminated and
            // outlives the call.
            with_argv(&command.args(), |argv| unsafe {
                (self.lib.mpv_command_async)(self.handle, reply, argv)
            })?,
        )
    }
}

/// mpv before `mpv_initialize`, which is all [`Mpv::new`]'s `configure` is
/// given: setting options is the one thing to do with it then.
pub struct Options<'a>(&'a Mpv);

impl Options<'_> {
    pub fn set<T: ToMpv>(&self, option: Property<T>, value: impl Into<T>) -> Result<()> {
        self.0.set_option(option, value)
    }
}

/// mpv, with the reads that wait on its core.
///
/// A read can take hundreds of milliseconds while mpv is busy, so it is kept
/// off [`Mpv`], whose calls all return at once: a thread that must never wait
/// is kept from waiting by never being handed a `Reader`. Everything an
/// `Mpv` does, it does too.
#[derive(Clone)]
pub struct Reader(Arc<Mpv>);

impl Reader {
    /// Read a property.
    ///
    /// **Blocking.** Waits on mpv's core, which under load costs tens to
    /// hundreds of milliseconds. It also takes the core ahead of anything
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
        let mpv = &*self.0;
        let name = cstr(property.name()).ok()?;
        let format = T::FORMAT.code();
        let mut slot = Slot { int: 0 };
        // SAFETY: `handle` is live, `name` outlives the call, and `slot` has
        // room for any format asked for.
        let read = unsafe {
            (mpv.lib.mpv_get_property)(mpv.handle, name.as_ptr(), format, (&raw mut slot).cast())
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
            unsafe { (mpv.lib.mpv_free)(slot.string.cast()) };
        }
        T::from_mpv(value?)
    }
}

impl Deref for Reader {
    type Target = Mpv;

    fn deref(&self) -> &Mpv {
        &self.0
    }
}

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
