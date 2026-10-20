//! Timeline scrubbing.
//!
//! Dragging produces position updates far faster than mpv can service seeks,
//! so this sits between the two and does three things:
//!
//! 1. **Never blocks.** Seeks go out through `mpv_command_async`, since a
//!    synchronous exact seek takes about 200ms, which on a drag would lock
//!    the interface on every mouse move.
//! 2. **Coalesces.** At most one seek is in flight. Updates arriving while
//!    one is outstanding overwrite a single pending slot rather than queueing,
//!    so the player always converges on where the pointer actually *is*
//!    instead of replaying where it has been. Without this, 120 drag updates
//!    become 120 seeks and the file scrubs for half a minute after release.
//! 3. **Seeks as exactly as the hand is moving.** A keyframe seek is cheap
//!    and lands up to a group of pictures early (ten seconds in an ordinary
//!    encode); an exact one decodes forward from that keyframe, which is what
//!    makes it slow. While the pointer covers minutes a second the difference
//!    is invisible, but a slow drag looking for something would see the same
//!    keyframe until it crossed into the next group. So a slow drag gets
//!    exact seeks, a fast one keyframes, and a pointer that stops with the
//!    button down the exact frame it stopped on.

use std::cell::Cell;
use std::time::Instant;

use crate::playback::commands;
use crate::playback::mpv::Mpv;

/// Film covered per second of dragging, below which the drag is slow and
/// above which it is fast. In seconds of film rather than pixels, since what
/// matters is how far the picture is from the one asked for, and a pixel of
/// timeline is a third of a second of a music video and seven of a feature.
///
/// An exact seek takes around a sixth of a second and a keyframe is up to ten
/// seconds early. Past a minute of film a second the exact frame is that out
/// of date by the time it is drawn; well under it, only exact seeks follow
/// the pointer.
///
/// Two numbers, so a hand moving at about the boundary doesn't flicker
/// between the two: between them the drag stays as it was.
const SLOW: f64 = 40.0;
const FAST: f64 = 60.0;
/// The shortest stretch a speed is taken over. Pointer moves arrive every
/// few milliseconds and a pixel at a time, and the speed between two of them
/// is a pixel divided by nearly nothing.
const WINDOW: f64 = 0.1;
/// How long a pointer has to be still, with the button down, to have
/// stopped. Longer than the gap between two moves of a hand in motion, and
/// short enough to pass for the picture simply arriving.
const REST: f64 = 0.1;

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
    /// Whether the hand is moving slowly enough to be shown exact frames.
    slow: Cell<bool>,
    /// Where the pointer was when its speed was last taken, and when.
    mark: Cell<Option<(Instant, f32)>>,
    /// When the pointer last moved.
    moved: Cell<Option<Instant>>,
    /// The last seek sent: where to, and whether it was exact.
    sent: Cell<Option<(f32, bool)>>,
    /// Counters for the drag just finished, reported under DBM_TRACE. The
    /// ratio between the first two is the whole point of this type.
    requested: Cell<u32>,
    issued: Cell<u32>,
    exact: Cell<u32>,
}

impl Scrubber {
    pub fn new() -> Self {
        Self::default()
    }

    /// A drag moved. Sends immediately if the line is clear, otherwise
    /// replaces whatever was waiting. `paused` is the current playback state,
    /// used once at the start of a drag to decide whether to resume
    /// afterwards; `duration` is the film's length, which is what turns a
    /// distance along the track into a speed through the film.
    ///
    /// Playback pauses for the drag, since a moving picture fights the one
    /// being looked for, and mpv has less to do if it isn't also decoding
    /// forward.
    pub fn drag_to(&self, mpv: &Mpv, fraction: f32, paused: bool, duration: f64) {
        let now = Instant::now();
        if !self.dragging.replace(true) {
            self.resume.set(!paused);
            if !paused {
                commands::set_pause(mpv, true);
            }
            // A press that has only just moved is the slowest scrub there is,
            // and starting fast would open every careful drag with a jump to a
            // keyframe and back.
            self.slow.set(true);
            self.mark.set(Some((now, fraction)));
            self.sent.set(None);
        } else {
            self.pace(fraction, duration, now);
        }
        self.moved.set(Some(now));
        self.requested.set(self.requested.get() + 1);
        self.ask(mpv, fraction);
    }

    /// The button is still down, and this is where the pointer is. Called a
    /// few times a second for as long as it is held.
    ///
    /// Stopping isn't a move, so nothing else here hears of it, and a drag
    /// that was fast until it stopped would leave its last keyframe on screen,
    /// up to ten seconds from the pointer. Once the pointer has been still for
    /// a moment this lands on the frame instead, once, since asking again for
    /// the frame already there sends nothing.
    pub fn held_at(&self, mpv: &Mpv, fraction: f32) {
        let still = self
            .moved
            .get()
            .is_some_and(|at| at.elapsed().as_secs_f64() >= REST);
        if !self.dragging.get() || !still {
            return;
        }
        self.slow.set(true);
        self.mark.set(Some((Instant::now(), fraction)));
        self.ask(mpv, fraction);
    }

    /// One of our seeks completed. Send the latest position if the pointer
    /// moved on while we were waiting.
    pub fn on_reply(&self, mpv: &Mpv) {
        let sent = self
            .pending
            .take()
            .is_some_and(|fraction| self.send(mpv, fraction));
        self.in_flight.set(sent);
    }

    /// The drag ended: land exactly on the final position.
    ///
    /// Any coalesced update is dropped, or it would seek away from where the
    /// user let go.
    pub fn release(&self, mpv: &Mpv, fraction: f32) {
        if crate::diagnostics::trace() && self.requested.get() > 0 {
            eprintln!(
                "dbm: drag ended - {} updates coalesced into {} seeks, {} of them exact",
                self.requested.get(),
                self.issued.get(),
                self.exact.get()
            );
        }
        self.requested.set(0);
        self.issued.set(0);
        self.exact.set(0);
        self.pending.set(None);
        self.in_flight.set(false);
        self.mark.set(None);
        self.moved.set(None);
        self.sent.set(None);
        commands::seek_fraction(mpv, fraction);
        // Only a real drag paused anything; a plain click on the timeline
        // leaves the playback state alone.
        if self.dragging.replace(false) && self.resume.replace(false) {
            commands::set_pause(mpv, false);
        }
    }

    /// Take the hand's speed, once enough time has passed to take it over.
    fn pace(&self, fraction: f32, duration: f64, now: Instant) {
        let Some((then, from)) = self.mark.get() else {
            self.mark.set(Some((now, fraction)));
            return;
        };
        let elapsed = now.duration_since(then).as_secs_f64();
        let covered = f64::from((fraction - from).abs()) * duration;
        // Too soon to take a speed, unless the pointer has already gone
        // further than a fast drag would in the whole stretch, so a flick
        // doesn't spend its first tenth of a second on exact seeks.
        if elapsed < WINDOW && covered <= FAST * WINDOW {
            return;
        }
        if let Some(slow) = pace(covered / elapsed.max(f64::EPSILON)) {
            self.slow.set(slow);
        }
        self.mark.set(Some((now, fraction)));
    }

    /// Seek to `fraction` now, or leave it for when the line is clear.
    fn ask(&self, mpv: &Mpv, fraction: f32) {
        if self.in_flight.get() {
            self.pending.set(Some(fraction));
        } else {
            self.in_flight.set(self.send(mpv, fraction));
        }
    }

    /// Send a seek, as exact as the drag currently is. Returns whether one
    /// went out: none does when the picture asked for is the one already
    /// there, and then there is no reply to wait for.
    fn send(&self, mpv: &Mpv, fraction: f32) -> bool {
        let exact = self.slow.get();
        if !worth_sending(self.sent.get(), fraction, exact) {
            return false;
        }
        self.sent.set(Some((fraction, exact)));
        self.issued.set(self.issued.get() + 1);
        if exact {
            self.exact.set(self.exact.get() + 1);
        }
        commands::seek_scrub(mpv, fraction, exact);
        true
    }
}

/// What a speed, in seconds of film a second, says about a drag: slow, fast,
/// or, between the two, whatever it already was.
fn pace(speed: f64) -> Option<bool> {
    if speed < SLOW {
        Some(true)
    } else if speed > FAST {
        Some(false)
    } else {
        None
    }
}

/// Whether a seek would show anything the last one did not. To the same
/// place it only does when it is exact and the last was not.
fn worth_sending(sent: Option<(f32, bool)>, fraction: f32, exact: bool) -> bool {
    match sent {
        Some((at, was_exact)) if at == fraction => exact && !was_exact,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_is_slow_or_fast_or_left_as_it_was() {
        assert_eq!(pace(0.0), Some(true));
        assert_eq!(pace(SLOW - 1.0), Some(true));
        assert_eq!(pace((SLOW + FAST) / 2.0), None);
        assert_eq!(pace(FAST + 1.0), Some(false));
    }

    #[test]
    fn the_same_picture_is_not_asked_for_twice() {
        // Nothing sent yet, or somewhere else: always worth it.
        assert!(worth_sending(None, 0.5, false));
        assert!(worth_sending(Some((0.4, true)), 0.5, false));
        // The same place again shows nothing new...
        assert!(!worth_sending(Some((0.5, false)), 0.5, false));
        assert!(!worth_sending(Some((0.5, true)), 0.5, true));
        assert!(!worth_sending(Some((0.5, true)), 0.5, false));
        // ...unless it was a keyframe and the frame itself is now wanted.
        assert!(worth_sending(Some((0.5, false)), 0.5, true));
    }
}
