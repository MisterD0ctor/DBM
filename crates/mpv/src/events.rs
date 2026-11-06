//! What mpv reports, and the one handle that reads it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::{Error, ErrorCode, Result, check};
use crate::ffi::{
    END_FILE_REASON_EOF, END_FILE_REASON_ERROR, EVENT_COMMAND_REPLY, EVENT_END_FILE,
    EVENT_FILE_LOADED, EVENT_NONE, EVENT_PLAYBACK_RESTART, EVENT_PROPERTY_CHANGE, EVENT_SEEK,
    EVENT_SET_PROPERTY_REPLY, EVENT_SHUTDOWN, EVENT_START_FILE, RawEvent, RawEventEndFile,
    RawEventProperty,
};
use crate::tracks::EntryId;
use crate::{Command, Mpv, Value};

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
        id: u32,
        failed: bool,
    },
    StartFile,
    FileLoaded,
    /// A file stopped playing. `entry` is which playlist entry it was, by
    /// mpv's id, since a file that fails at once is over before `path` has
    /// been reported for it; `None` where mpv didn't say.
    EndFile {
        entry: Option<EntryId>,
        reason: EndReason,
    },
    Seek,
    PlaybackRestart,
    Shutdown,
}

/// Why a file stopped playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// It played to its end. Where mpv goes on to the next entry, this is
    /// autoplay moving on, since a file asked for ends the last with
    /// [`Stopped`](Self::Stopped).
    Eof,
    /// It couldn't be played, for mpv's reason.
    Failed(ErrorCode),
    /// Any other ending: another file asked for, a stop, a quit, or a list
    /// that opened into its entries.
    Stopped,
}

/// mpv's event queue.
///
/// `mpv_wait_event` hands every caller the same struct, so only one thread
/// may read the queue at a time. [`Mpv::new`] makes the only `Events` there
/// is, and reading takes it mutably, so whoever holds it is that one reader.
pub struct Events {
    pub(crate) mpv: Arc<Mpv>,
}

/// The reply id [`Events::run_and_wait`] waits on, out of the range of the
/// `u32` ids callers give [`Mpv::run`].
const RUN_AND_WAIT: u64 = u64::MAX;

impl Events {
    /// Drain one queued event, or `None` when the queue is empty.
    ///
    /// Never blocks. The struct mpv hands back is only valid until the next
    /// read, so everything is copied out before returning.
    pub fn poll(&mut self) -> Option<Event> {
        loop {
            // SAFETY: `handle` is live, and `&mut self` makes this the only
            // read of the queue in progress.
            let raw = unsafe { (self.mpv.lib.mpv_wait_event)(self.mpv.handle, 0.0) };
            if raw.is_null() {
                return None;
            }
            // SAFETY: non-null, and valid until the next `mpv_wait_event`,
            // which cannot come while `self` is borrowed.
            let ev = unsafe { &*raw };
            if ev.event_id == EVENT_NONE {
                return None;
            }
            // One nothing here acts on is passed over rather than taken for
            // the end of the queue, which would leave what is behind it
            // waiting for a wakeup that has already been and gone.
            // SAFETY: `ev` is the event mpv just handed back.
            if let Some(event) = unsafe { translate(ev) } {
                return Some(event);
            }
        }
    }

    /// Queue a command and wait until mpv has carried it out, behind
    /// whatever was queued before it. Events that arrive meanwhile are
    /// dropped.
    ///
    /// For when nothing else needs the queue any more, such as quitting.
    /// libmpv's render thread must never wait on mpv while it still renders
    /// through it.
    pub fn run_and_wait(&mut self, command: Command<'_>, timeout: Duration) -> Result<()> {
        self.mpv.queue(RUN_AND_WAIT, command)?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::NoReply);
            }
            // SAFETY: `handle` is live, and `&mut self` makes this the only
            // read of the queue in progress. mpv never returns null, but an
            // empty event on timeout.
            let ev =
                unsafe { &*(self.mpv.lib.mpv_wait_event)(self.mpv.handle, left.as_secs_f64()) };
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

/// Copy an event out of mpv's struct, or `None` for one nothing here acts
/// on.
///
/// # Safety
/// `ev` is the event the last `mpv_wait_event` returned.
unsafe fn translate(ev: &RawEvent) -> Option<Event> {
    match ev.event_id {
        EVENT_SHUTDOWN => Some(Event::Shutdown),
        // The binding's own reply, out of `u32`'s range, is no caller's.
        EVENT_COMMAND_REPLY | EVENT_SET_PROPERTY_REPLY => u32::try_from(ev.reply_userdata)
            .ok()
            .map(|id| Event::Reply {
                id,
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
            let reason = match end.map(|end| end.reason) {
                Some(END_FILE_REASON_EOF) => EndReason::Eof,
                // An error ending always carries its code, but one without is
                // still a failure.
                Some(END_FILE_REASON_ERROR) => EndReason::Failed(
                    end.and_then(|end| ErrorCode::of(end.error))
                        .unwrap_or(ErrorCode::Generic),
                ),
                _ => EndReason::Stopped,
            };
            Some(Event::EndFile {
                entry: end.map(|end| EntryId(end.playlist_entry_id)),
                reason,
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
        _ => None,
    }
}
