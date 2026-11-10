//! Taking back a jump: a seek the pointer made, or a number key.
//!
//! The timeline lies straight along the row of buttons, and a click on it
//! lands wherever it was, so a hand on its way to a button can throw away the
//! place a film had been watched to, with nothing to find it by; a number
//! key, a tenth of the film at a time, does it in one press. For ten seconds
//! of the film after either moves it more than half a minute, the corner pill
//! offers the way back (see `back-offer` in pills.slint). The arrows seek by
//! steps someone chose, and are left alone. A title sequence the player
//! skipped by itself is offered back the same way (see `skips`).

use std::cell::RefCell;

use dbm_mpv::{MediaPath, Mpv};

use crate::playback::commands;

/// A seek shorter than this is a nudge, which leaves the place it came from
/// within reach of the arrow keys.
const WORTH_OFFERING: f64 = 30.0;
/// A place nearer the start than this is no place lost: Home goes back to it,
/// and a saved place this early is not asked about either (`arrival`).
const WORTH_KEEPING: f64 = 60.0;
/// How long the way back is offered after the seek lands, in seconds of the
/// film: as long as the place autoplay passed over is (`arrival`), being the
/// same pill asking the same kind of thing. A paused film keeps the offer.
const OFFER_FOR: f64 = 10.0;
/// How far the position may wander from where the seek landed, before it,
/// until the person is taken to have gone somewhere else themselves.
const WANDER: f64 = 2.0;

#[derive(Default)]
pub struct Retrace {
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    /// Where the film was when the pointer took hold of the timeline, until
    /// it lets go.
    holding: Option<f64>,
    offer: Option<Offer>,
}

struct Offer {
    path: MediaPath,
    /// The place to go back to.
    from: f64,
    /// Where the seek landed.
    to: f64,
    /// Whether the position has reached `to`. Until then it is still the
    /// place the seek left, which is no sign of the person moving on.
    arrived: bool,
}

impl Retrace {
    /// The pointer is on the timeline, dragging or held, with the film at
    /// `at`. The first of a drag is the place it left.
    pub fn hold(&self, at: f64) {
        self.state.borrow_mut().holding.get_or_insert(at);
    }

    /// A jump landed at `to`, the pointer letting go of the timeline or a
    /// number key, with the film at `at`. Jumping again while the way back is
    /// offered keeps the first place, since that is the one lost.
    pub fn landed(&self, path: &MediaPath, at: f64, to: f64) {
        // An exercise seeks to get somewhere, a file's credits or its end,
        // and the offer would take the corner from what it is there to see.
        if crate::run_mode::armed() {
            return;
        }
        // mpv lands a seek before the start at the start, and the offer
        // waits for the film to be where the seek landed.
        let to = to.max(0.0);
        let mut state = self.state.borrow_mut();
        let left = state.holding.take().unwrap_or(at);
        let from = match &state.offer {
            Some(offer) if offer.path == *path => offer.from,
            _ => left,
        };
        state.offer =
            ((to - from).abs() > WORTH_OFFERING && from >= WORTH_KEEPING).then(|| Offer {
                path: path.clone(),
                from,
                to,
                arrived: false,
            });
    }

    /// The player skipped from `from` to `to` by itself (see `skips`). That
    /// is offered back however short or early it was, because nobody chose
    /// it. The skips wait for the corner, so no other way back is lost.
    pub fn skipped(&self, path: &MediaPath, from: f64, to: f64) {
        self.state.borrow_mut().offer = Some(Offer {
            path: path.clone(),
            from,
            to,
            arrived: false,
        });
    }

    /// The place to go back to, while it is offered.
    pub fn offered(&self, path: Option<&MediaPath>, time_pos: f64) -> Option<f64> {
        let mut state = self.state.borrow_mut();
        let offer = state.offer.as_mut()?;
        offer.arrived |= (time_pos - offer.to).abs() <= WANDER;
        let gone = path != Some(&offer.path)
            || offer.arrived && (time_pos < offer.to - WANDER || time_pos > offer.to + OFFER_FOR);
        if gone {
            state.offer = None;
            return None;
        }
        Some(offer.from)
    }

    /// Take the offer: back to where the pointer's seek left.
    pub fn take(&self, mpv: &Mpv) {
        if let Some(offer) = self.state.borrow_mut().offer.take() {
            commands::seek_to(mpv, offer.from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_click_offers_the_place_it_left() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.landed(&a, 2106.0, 300.0);
        assert_eq!(retrace.offered(Some(&a), 2106.0), Some(2106.0));
        assert_eq!(retrace.offered(Some(&a), 300.0), Some(2106.0));
        assert_eq!(retrace.offered(Some(&a), 309.0), Some(2106.0));
        assert_eq!(retrace.offered(Some(&a), 311.0), None);
    }

    #[test]
    fn a_jump_to_before_the_start_ends_from_the_start() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.landed(&a, 600.0, -6.0);
        assert_eq!(retrace.offered(Some(&a), 0.0), Some(600.0));
        assert_eq!(retrace.offered(Some(&a), 11.0), None);
    }

    #[test]
    fn a_nudge_offers_nothing() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.landed(&a, 100.0, 120.0);
        assert_eq!(retrace.offered(Some(&a), 120.0), None);
    }

    #[test]
    fn the_start_is_not_a_place_to_go_back_to() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.landed(&a, 20.0, 1200.0);
        assert_eq!(retrace.offered(Some(&a), 1200.0), None);
    }

    #[test]
    fn a_drag_offers_where_it_started() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.hold(500.0);
        retrace.hold(700.0);
        retrace.landed(&a, 900.0, 1500.0);
        assert_eq!(retrace.offered(Some(&a), 1500.0), Some(500.0));
    }

    #[test]
    fn seeking_again_keeps_the_first_place() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.landed(&a, 2106.0, 300.0);
        assert_eq!(retrace.offered(Some(&a), 300.0), Some(2106.0));
        retrace.landed(&a, 300.0, 1200.0);
        assert_eq!(retrace.offered(Some(&a), 1200.0), Some(2106.0));
    }

    #[test]
    fn going_elsewhere_ends_it() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.landed(&a, 2106.0, 300.0);
        assert_eq!(retrace.offered(Some(&a), 300.0), Some(2106.0));
        assert_eq!(retrace.offered(Some(&a), 240.0), None);
        retrace.landed(&a, 240.0, 900.0);
        assert_eq!(
            retrace.offered(Some(&MediaPath::for_test("b.mkv")), 900.0),
            None
        );
    }

    #[test]
    fn a_skip_the_player_made_is_offered_back_from_the_start() {
        let retrace = Retrace::default();
        let a = MediaPath::for_test("a.mkv");
        retrace.skipped(&a, 2.0, 40.0);
        assert_eq!(retrace.offered(Some(&a), 40.0), Some(2.0));
        assert_eq!(retrace.offered(Some(&a), 51.0), None);
    }
}
