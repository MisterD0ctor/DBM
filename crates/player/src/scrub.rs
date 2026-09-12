//! Timeline scrubbing.
//!
//! Dragging produces position updates far faster than mpv can service seeks,
//! so this sits between the two and does three things:
//!
//! 1. **Never blocks.** Seeks go out through `mpv_command_async`. The
//!    synchronous call takes ~195ms for an exact seek, which on a drag means
//!    the interface locks up for a fifth of a second per mouse move.
//! 2. **Coalesces.** At most one seek is in flight. Updates arriving while
//!    one is outstanding overwrite a single pending slot rather than queueing,
//!    so the player always converges on where the pointer actually *is*
//!    instead of replaying where it has been. Without this, 120 drag updates
//!    become 120 seeks and the file scrubs for half a minute after release.
//! 3. **Seeks cheaply while moving.** Keyframe-accurate during the drag, one
//!    exact seek when the user lets go. Exact seeks decode forward from the
//!    preceding keyframe, which is most of what makes them slow, and that
//!    precision is invisible while the thumb is still moving.

use std::cell::Cell;

use crate::commands;
use crate::mpv::Mpv;

#[derive(Default)]
pub struct Scrubber {
    /// Where the pointer last was, if that has not been sent yet.
    pending: Cell<Option<f32>>,
    /// Whether a seek we issued is still outstanding.
    in_flight: Cell<bool>,
    /// Whether a drag is in progress, as opposed to a bare click.
    dragging: Cell<bool>,
    /// Whether playback should resume when the drag ends.
    resume: Cell<bool>,
    /// Counters for the drag just finished, reported under DBM_TRACE. The
    /// ratio between them is the whole point of this type.
    requested: Cell<u32>,
    issued: Cell<u32>,
}

impl Scrubber {
    pub fn new() -> Self {
        Self::default()
    }

    /// A drag moved. Sends immediately if the line is clear, otherwise
    /// replaces whatever was waiting.
    /// A drag moved. `paused` is the current playback state, used once at
    /// the start of a drag to decide whether to resume afterwards.
    ///
    /// Playback pauses for the duration: a moving picture fights the one the
    /// user is trying to find, and mpv has less to do if it is not also
    /// decoding forward.
    pub fn drag_to(&self, mpv: &Mpv, fraction: f32, paused: bool) {
        if !self.dragging.replace(true) {
            self.resume.set(!paused);
            if !paused {
                commands::set_pause(mpv, true);
            }
        }
        self.requested.set(self.requested.get() + 1);
        if self.in_flight.get() {
            self.pending.set(Some(fraction));
            return;
        }
        self.in_flight.set(true);
        self.issued.set(self.issued.get() + 1);
        commands::seek_scrub(mpv, fraction);
    }

    /// One of our seeks completed. Send the latest position if the pointer
    /// moved on while we were waiting.
    pub fn on_reply(&self, mpv: &Mpv) {
        match self.pending.take() {
            Some(fraction) => {
                self.issued.set(self.issued.get() + 1);
                commands::seek_scrub(mpv, fraction);
            }
            None => self.in_flight.set(false),
        }
    }

    /// The drag ended: land exactly on the final position.
    ///
    /// Any coalesced update is dropped — it is superseded by this one, and
    /// letting it through afterwards would seek away from where the user let
    /// go.
    pub fn release(&self, mpv: &Mpv, fraction: f32) {
        if std::env::var_os("DBM_TRACE").is_some() && self.requested.get() > 0 {
            eprintln!(
                "dbm: drag ended - {} updates coalesced into {} seeks",
                self.requested.get(),
                self.issued.get()
            );
        }
        self.requested.set(0);
        self.issued.set(0);
        self.pending.set(None);
        self.in_flight.set(false);
        commands::seek_fraction(mpv, fraction);
        // Only a real drag paused anything; a plain click on the timeline
        // leaves the playback state alone.
        if self.dragging.replace(false) && self.resume.replace(false) {
            commands::set_pause(mpv, false);
        }
    }
}
