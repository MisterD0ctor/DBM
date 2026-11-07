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

use std::cell::RefCell;
use std::time::Instant;

use dbm_mpv::Mpv;

use crate::playback::commands;

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

/// Turns a timeline drag into seeks mpv can keep up with. Shared by the
/// callbacks, which feed it positions, and the frame loop, which feeds it
/// the replies that let the next seek go out; one borrow a call, since
/// nothing it calls comes back into it.
#[derive(Default)]
pub struct Scrubber(RefCell<Option<Drag>>);

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
        let mut slot = self.0.borrow_mut();
        let drag = match slot.take() {
            Some(mut drag) => {
                drag.take_speed(fraction, duration, now);
                drag
            }
            None => {
                if !paused {
                    commands::set_pause(mpv, true);
                }
                Drag::starting(fraction, !paused, now)
            }
        };
        let drag = slot.insert(drag);
        drag.moved = now;
        drag.requested += 1;
        drag.ask(mpv, fraction);
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
        if let Some(drag) = self.0.borrow_mut().as_mut() {
            drag.held_at(mpv, fraction);
        }
    }

    /// One of our seeks completed. Send the latest position if the pointer
    /// moved on while we were waiting.
    pub fn on_reply(&self, mpv: &Mpv) {
        if let Some(drag) = self.0.borrow_mut().as_mut() {
            drag.on_reply(mpv);
        }
    }

    /// The button came up: land exactly on the final position.
    ///
    /// Whatever the drag was waiting to send is dropped with it, or it would
    /// seek away from where the user let go.
    pub fn release(&self, mpv: &Mpv, fraction: f32) {
        let drag = self.0.borrow_mut().take();
        if let Some(drag) = &drag {
            log::debug!(
                "drag ended - {} updates coalesced into {} seeks, {} of them exact",
                drag.requested,
                drag.issued,
                drag.exact
            );
        }
        commands::seek_fraction(mpv, fraction);
        // Only a real drag paused anything; a plain click on the timeline
        // leaves the playback state alone.
        if drag.is_some_and(|drag| drag.resume) {
            commands::set_pause(mpv, false);
        }
    }
}

/// A drag under way, from its first move until the button comes up.
struct Drag {
    /// Whether playback resumes when the drag ends.
    resume: bool,
    /// How fast the hand is moving, which says how exact its seeks are.
    pace: Pace,
    /// Where the pointer was when its speed was last taken, and when.
    mark: (Instant, f32),
    /// When the pointer last moved.
    moved: Instant,
    /// The last seek sent, if one was.
    sent: Option<Sent>,
    line: Line,
    /// Counters reported under DBM_TRACE. The ratio between the first two is
    /// the whole point of this type.
    requested: u32,
    issued: u32,
    exact: u32,
}

/// A seek the drag sent: where it went, and whether it was the exact frame
/// there or a keyframe before it.
///
/// Named rather than a pair, so a call site cannot pass the two the wrong
/// way round — both are numbers, and one of them is a pixel of the timeline
/// and the other a bool.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sent {
    fraction: f32,
    exact: bool,
}

impl Sent {
    fn new(fraction: f32, exact: bool) -> Self {
        Self { fraction, exact }
    }
}

/// Whether one of the drag's seeks is on its way to mpv.
enum Line {
    Clear,
    /// One is, and `next` is where the pointer has gone since, sent once it
    /// is answered.
    Busy {
        next: Option<f32>,
    },
}

/// How fast a hand is moving along the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pace {
    /// Slowly enough to be shown exact frames.
    Slow,
    /// Too fast for an exact frame to keep up, so shown keyframes.
    Fast,
}

impl Drag {
    fn starting(fraction: f32, resume: bool, now: Instant) -> Self {
        Self {
            resume,
            // A press that has only just moved is the slowest scrub there is,
            // and starting fast would open every careful drag with a jump to
            // a keyframe and back.
            pace: Pace::Slow,
            mark: (now, fraction),
            moved: now,
            sent: None,
            line: Line::Clear,
            requested: 0,
            issued: 0,
            exact: 0,
        }
    }

    fn held_at(&mut self, mpv: &Mpv, fraction: f32) {
        if self.moved.elapsed().as_secs_f64() < REST {
            return;
        }
        self.pace = Pace::Slow;
        self.mark = (Instant::now(), fraction);
        self.ask(mpv, fraction);
    }

    fn on_reply(&mut self, mpv: &Mpv) {
        self.line = match std::mem::replace(&mut self.line, Line::Clear) {
            Line::Busy {
                next: Some(fraction),
            } => self.send(mpv, fraction),
            Line::Busy { next: None } | Line::Clear => Line::Clear,
        };
    }

    /// Take the hand's speed, once enough time has passed to take it over.
    fn take_speed(&mut self, fraction: f32, duration: f64, now: Instant) {
        let (then, from) = self.mark;
        let elapsed = now.duration_since(then).as_secs_f64();
        let covered = f64::from((fraction - from).abs()) * duration;
        // Too soon to take a speed, unless the pointer has already gone
        // further than a fast drag would in the whole stretch, so a flick
        // doesn't spend its first tenth of a second on exact seeks.
        if elapsed < WINDOW && covered <= FAST * WINDOW {
            return;
        }
        if let Some(pace) = pace(covered / elapsed.max(f64::EPSILON)) {
            self.pace = pace;
        }
        self.mark = (now, fraction);
    }

    /// Seek to `fraction` now, or leave it for when the line is clear.
    fn ask(&mut self, mpv: &Mpv, fraction: f32) {
        if let Line::Busy { next } = &mut self.line {
            *next = Some(fraction);
        } else {
            self.line = self.send(mpv, fraction);
        }
    }

    /// Send a seek, as exact as the drag currently is. The line is busy
    /// after if one went out: none does when the picture asked for is the
    /// one already there, nor when mpv refused it, and then there is no
    /// reply to wait for.
    fn send(&mut self, mpv: &Mpv, fraction: f32) -> Line {
        let exact = self.pace == Pace::Slow;
        if !worth_sending(self.sent, fraction, exact) || !commands::seek_scrub(mpv, fraction, exact)
        {
            return Line::Clear;
        }
        self.sent = Some(Sent::new(fraction, exact));
        self.issued += 1;
        if exact {
            self.exact += 1;
        }
        Line::Busy { next: None }
    }
}

/// What a speed, in seconds of film a second, says about a drag: slow, fast,
/// or, between the two, whatever it already was.
fn pace(speed: f64) -> Option<Pace> {
    if speed < SLOW {
        Some(Pace::Slow)
    } else if speed > FAST {
        Some(Pace::Fast)
    } else {
        None
    }
}

/// Whether a seek would show anything the last one did not. To the same
/// place it only does when it is exact and the last was not.
fn worth_sending(sent: Option<Sent>, fraction: f32, exact: bool) -> bool {
    match sent {
        Some(sent) if sent.fraction == fraction => exact && !sent.exact,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_is_slow_or_fast_or_left_as_it_was() {
        assert_eq!(pace(0.0), Some(Pace::Slow));
        assert_eq!(pace(SLOW - 1.0), Some(Pace::Slow));
        assert_eq!(pace(f64::midpoint(SLOW, FAST)), None);
        assert_eq!(pace(FAST + 1.0), Some(Pace::Fast));
    }

    #[test]
    fn the_same_picture_is_not_asked_for_twice() {
        // Nothing sent yet, or somewhere else: always worth it.
        assert!(worth_sending(None, 0.5, false));
        assert!(worth_sending(Some(Sent::new(0.4, true)), 0.5, false));
        // The same place again shows nothing new...
        assert!(!worth_sending(Some(Sent::new(0.5, false)), 0.5, false));
        assert!(!worth_sending(Some(Sent::new(0.5, true)), 0.5, true));
        assert!(!worth_sending(Some(Sent::new(0.5, true)), 0.5, false));
        // ...unless it was a keyframe and the frame itself is now wanted.
        assert!(worth_sending(Some(Sent::new(0.5, false)), 0.5, true));
    }
}
