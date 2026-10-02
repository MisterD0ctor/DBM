//! Where the subtitle line sits.
//!
//! Two things decide it, and mpv can only be told one number. The person's
//! own Position — the stepper on the subtitles page, saved with the settings —
//! is where the line rests. The bar, while it is up, is a floor the line may
//! not sit under: it covers the bottom 110-odd pixels of the window, and the
//! bottom of the window is where subtitles live.
//!
//! The case that made this necessary is pausing. A paused film holds the bar
//! up, because pausing is stopping to look at where you are, and stopping to
//! read a line is the commonest reason to pause there is. The line was under
//! the timeline and behind the transport for exactly as long as somebody was
//! trying to read it.
//!
//! So this is the one writer of mpv's `sub-pos`. The Position goes in through
//! [`Subline::set`] and the bar through [`Subline::follow`], every frame; what
//! mpv is told is the lower of the two places, eased between them by the
//! bar's own fade. Two writers would fight: a step on the stepper would put
//! the line back under a bar that was still up, and nothing would lift it
//! again until the bar moved.
//!
//! The readout on the subtitles page shows the Position, not what mpv holds.
//! While the bar is up those differ, and the number belongs to the person: it
//! is what they set and what is saved, and it is where the line goes back to.

use std::cell::Cell;
use std::rc::Rc;

use crate::playback::commands;
use crate::playback::mpv::Mpv;
use crate::settings::Store;

/// Smaller moves than this are not worth a command. A hundredth of a percent
/// of the window is a tenth of a pixel at 1080 — under it the line cannot
/// move, and above it every frame of the bar's fade is still a step.
const SETTLED: f64 = 0.01;

pub struct Subline {
    params: Rc<Store>,
    /// The highest the line may sit while the bar is up, in `sub-pos` units,
    /// and how far the bar has faded in — both as of the last frame.
    floor: Cell<(f64, f64)>,
    /// What mpv was last told, so a still bar costs nothing.
    sent: Cell<Option<f64>>,
    /// The Position the readout last showed.
    shown: Cell<Option<f64>>,
}

impl Subline {
    pub fn new(params: Rc<Store>) -> Self {
        Self {
            params,
            floor: Cell::new((commands::SUB_POS_MAX, 0.0)),
            sent: Cell::new(None),
            shown: Cell::new(None),
        }
    }

    /// Put the line somewhere new, returning where it was put after the
    /// clamp.
    ///
    /// The clamp is applied to the Position rather than to what mpv is sent,
    /// because the Position is what is saved; a value outside the range
    /// would come back next run as one the stepper never offered.
    pub fn set(&self, mpv: &Mpv, value: f64) -> f64 {
        let value = value.clamp(commands::SUB_POS_MIN, commands::SUB_POS_MAX);
        self.params.set_sub_pos(value as f32);
        self.place(mpv);
        value
    }

    /// Where the bar is this frame.
    ///
    /// `ceiling` is the highest a line may rest while the bar is up, as the
    /// interface works it out from where the timeline is; `lift` is how much
    /// of that applies, from 0 with the bar gone to 1 with it arrived.
    pub fn follow(&self, mpv: &Mpv, ceiling: f64, lift: f64) {
        self.floor.set((ceiling, lift.clamp(0.0, 1.0)));
        self.place(mpv);
    }

    /// The Position, when it has changed since the readout last showed it.
    pub fn readout(&self) -> Option<f64> {
        let own = self.params.sub_pos() as f64;
        if self.shown.get() == Some(own) {
            return None;
        }
        self.shown.set(Some(own));
        Some(own)
    }

    fn place(&self, mpv: &Mpv) {
        let own = self.params.sub_pos() as f64;
        let (ceiling, lift) = self.floor.get();
        // Only as far as the bar needs. A line the person already placed
        // above the timeline stays exactly where they put it; one resting in
        // the letterbox — `sub-pos` past 100, which is what the range runs
        // to 150 for — comes all the way up. Subtracting the bar's height
        // instead would move the first for nothing and leave the second
        // still underneath.
        let at = own - lift * (own - ceiling).max(0.0);
        if self
            .sent
            .get()
            .is_some_and(|sent| (sent - at).abs() < SETTLED)
        {
            return;
        }
        commands::set_sub_pos(mpv, at);
        self.sent.set(Some(at));
    }
}
