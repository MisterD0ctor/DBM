//! Where the subtitle line sits.
//!
//! Two things decide it. The person's Position (the stepper on the subtitles
//! page, saved with the settings) is where the line rests: up to 100 down the
//! picture, and past that on down the letterbox to the window's foot. The
//! bar, while it is up, is a floor the line may not sit under, which matters
//! most when paused: a paused film holds the bar up, and reading a line is the
//! commonest reason to pause.
//!
//! Both are worked out where the line really is, in fractions of the window's
//! height, and only then turned into what mpv takes. So a bar down in the
//! letterbox, under a line on the picture, leaves the line where it is.
//!
//! This is the one writer of mpv's `sub-pos`. The Position comes in through
//! [`Subline::set`], and the bar and the picture's place through
//! [`Subline::follow`], every frame. Two writers would fight: a step would put
//! the line back under a bar still up.
//!
//! The readout on the subtitles page shows the Position, not what mpv holds,
//! since that is what the person set, what is saved, and where the line goes
//! back to.

use std::cell::Cell;
use std::rc::Rc;

use dbm_mpv::Mpv;

use crate::playback::commands;
use crate::playback::state::OsdDims;
use crate::settings::Store;

/// Smaller moves aren't worth a command: a hundredth of `sub-pos` is a tenth
/// of a pixel at 1080, while every frame of the bar's fade is still a bigger
/// step.
const SETTLED: f64 = 0.01;

/// Where mpv has put the picture down the window, in fractions of the
/// window's height from its top. Past 0 or 1 where the picture is cropped to
/// fill the window.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Picture {
    top: f64,
    foot: f64,
    /// Cropped at some edge to fill the window.
    cropped: bool,
    /// Narrower than the window, with the border at its sides.
    narrower: bool,
}

impl Picture {
    /// From mpv's layout. Before mpv has laid anything out, the picture is
    /// taken to fill the window.
    pub fn from_osd(osd: OsdDims) -> Self {
        if osd.w <= 0.0 || osd.h <= 0.0 {
            return Self {
                top: 0.0,
                foot: 1.0,
                cropped: false,
                narrower: false,
            };
        }
        Self {
            top: osd.mt / osd.h,
            foot: 1.0 - osd.mb / osd.h,
            cropped: [osd.ml, osd.mt, osd.mr, osd.mb].iter().any(|m| *m < 0.0),
            narrower: osd.ml > 0.0 || osd.mr > 0.0,
        }
    }
}

/// What mpv is told: whether the line is laid out against the window rather
/// than the picture, and `sub-pos` within that.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Placement {
    window: bool,
    pos: f64,
}

/// Where the line's foot rests at Position `own`, in fractions of the
/// window's height: down the part of the picture inside the window, then down
/// the letterbox below it. Bitmaps stop at the picture's foot, since mpv keeps
/// them inside the picture.
fn rest(own: f64, picture: Picture, bitmaps: bool) -> f64 {
    let top = picture.top.max(0.0);
    let foot = picture.foot.min(1.0);
    if own <= commands::SUB_POS_DEFAULT || bitmaps {
        let own = own.min(commands::SUB_POS_DEFAULT);
        top + (foot - top) * own / commands::SUB_POS_DEFAULT
    } else {
        let below =
            (own - commands::SUB_POS_DEFAULT) / (commands::SUB_POS_MAX - commands::SUB_POS_DEFAULT);
        foot + (1.0 - foot) * below
    }
}

/// What mpv should be told for Position `own`, with the bar's `floor` (a
/// fraction of the window's height) applying by `lift`, from 0 with the bar
/// gone to 1 with it arrived.
fn placement(own: f64, floor: f64, lift: f64, picture: Picture, bitmaps: bool) -> Placement {
    // Only as far as the bar needs: a line already above it stays put, and
    // one resting in the letterbox comes all the way up. Subtracting the
    // bar's height instead would move the first and not clear the second.
    let rest = rest(own, picture, bitmaps);
    let at = rest - lift * (rest - floor).max(0.0);

    // Against the picture, as mpv lays out ASS unasked, wherever it can hold
    // the line: it can't below its foot, and loses a line off the window's
    // foot when cropped. Never against the window beside a narrower picture,
    // which wraps a line to the window's width and runs it out over the
    // border. Decided by where the line rests, so the bar's fade never
    // changes it, and bitmaps go within the picture whatever mpv is told.
    let window = picture.cropped || (!picture.narrower && rest > picture.foot);
    let pos = if window && !bitmaps {
        100.0 * at
    } else {
        100.0 * (at - picture.top) / (picture.foot - picture.top).max(f64::EPSILON)
    };
    Placement { window, pos }
}

pub struct Subline {
    params: Rc<Store>,
    /// The highest the line may rest while the bar is up, as a fraction of
    /// the window's height, and how far the bar has faded in, both as of the
    /// last frame.
    floor: Cell<(f64, f64)>,
    /// Where the picture is, and whether the track showing is bitmaps, as of
    /// the last frame.
    picture: Cell<(Picture, bool)>,
    /// What mpv was last told, so a still bar costs nothing.
    sent: Cell<Option<Placement>>,
    /// The Position the readout last showed.
    shown: Cell<Option<f64>>,
}

impl Subline {
    pub fn new(params: Rc<Store>) -> Self {
        Self {
            params,
            floor: Cell::new((1.0, 0.0)),
            picture: Cell::new((Picture::from_osd(OsdDims::default()), false)),
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

    /// Where the picture and the bar are this frame.
    ///
    /// `bitmaps` is whether the track showing is pictures rather than text.
    /// `floor` is the top of the bar as the interface draws it, as a fraction
    /// of the window's height; `lift` is how much of it applies, from 0 with
    /// the bar gone to 1 with it arrived.
    pub fn follow(&self, mpv: &Mpv, picture: Picture, bitmaps: bool, floor: f64, lift: f64) {
        self.picture.set((picture, bitmaps));
        self.floor.set((floor, lift.clamp(0.0, 1.0)));
        self.place(mpv);
    }

    /// The Position, when it has changed since the readout last showed it.
    pub fn readout(&self) -> Option<f64> {
        let own = f64::from(self.params.sub_pos());
        if self.shown.get() == Some(own) {
            return None;
        }
        self.shown.set(Some(own));
        Some(own)
    }

    fn place(&self, mpv: &Mpv) {
        let (floor, lift) = self.floor.get();
        let (picture, bitmaps) = self.picture.get();
        let to = placement(
            f64::from(self.params.sub_pos()),
            floor,
            lift,
            picture,
            bitmaps,
        );
        let sent = self.sent.get();
        if sent.is_some_and(|s| s.window == to.window && (s.pos - to.pos).abs() < SETTLED) {
            return;
        }
        // Only a resize, a fill or a step of the Position changes the frame,
        // never the bar's fade, so the two commands can't land a frame apart
        // in the middle of a movement.
        if sent.is_none_or(|s| s.window != to.window) {
            commands::set_sub_frame(mpv, to.window);
        }
        commands::set_sub_pos(mpv, to.pos);
        self.sent.set(Some(to));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16:9 picture in a 4:3 window: an eighth of the height above and
    /// below it.
    const LETTERBOXED: OsdDims = OsdDims {
        w: 1280.0,
        h: 960.0,
        ml: 0.0,
        mt: 120.0,
        mr: 0.0,
        mb: 120.0,
    };
    /// The same picture in a 2.39:1 window, with the border at its sides.
    const PILLARBOXED: OsdDims = OsdDims {
        w: 1720.0,
        h: 720.0,
        ml: 220.0,
        mt: 0.0,
        mr: 220.0,
        mb: 0.0,
    };
    /// The same picture filling the 2.39:1 window, its top and foot cropped.
    const CROPPED: OsdDims = OsdDims {
        w: 1720.0,
        h: 720.0,
        ml: 0.0,
        mt: -124.0,
        mr: 0.0,
        mb: -124.0,
    };

    fn at(own: f64, osd: OsdDims) -> Placement {
        placement(own, 1.0, 0.0, Picture::from_osd(osd), false)
    }

    fn close(placed: Placement, window: bool, pos: f64) {
        assert_eq!(placed.window, window, "{placed:?}");
        assert!((placed.pos - pos).abs() < 1e-9, "{placed:?} against {pos}");
    }

    #[test]
    fn a_line_rests_on_the_picture_and_past_it_in_the_letterbox() {
        close(at(100.0, LETTERBOXED), false, 100.0);
        close(at(0.0, LETTERBOXED), false, 0.0);
        close(at(125.0, LETTERBOXED), true, 93.75);
        close(at(150.0, LETTERBOXED), true, 100.0);
    }

    #[test]
    fn beside_a_narrower_picture_a_line_keeps_to_it() {
        close(at(100.0, PILLARBOXED), false, 100.0);
        // No letterbox to go into.
        close(at(150.0, PILLARBOXED), false, 100.0);
    }

    #[test]
    fn a_cropped_picture_keeps_the_line_in_the_window() {
        close(at(100.0, CROPPED), true, 100.0);
        close(at(150.0, CROPPED), true, 100.0);
    }

    #[test]
    fn a_bar_in_the_letterbox_leaves_a_line_on_the_picture_alone() {
        let picture = Picture::from_osd(LETTERBOXED);
        close(placement(100.0, 0.9, 1.0, picture, false), false, 100.0);
        // But one resting in the letterbox comes up onto it, laid out where
        // it rests, so the bar's fade doesn't change the frame.
        close(placement(150.0, 0.9, 1.0, picture, false), true, 90.0);
        close(placement(150.0, 0.8, 1.0, picture, false), true, 80.0);
    }

    #[test]
    fn the_bar_lifts_a_line_as_it_arrives() {
        let picture = Picture::from_osd(PILLARBOXED);
        close(placement(100.0, 0.8, 1.0, picture, false), false, 80.0);
        close(placement(100.0, 0.8, 0.5, picture, false), false, 90.0);
        // A line already above the bar stays put.
        close(placement(60.0, 0.8, 1.0, picture, false), false, 60.0);
    }

    #[test]
    fn bitmaps_are_placed_within_the_picture() {
        let letterboxed = Picture::from_osd(LETTERBOXED);
        close(placement(120.0, 1.0, 0.0, letterboxed, true), false, 100.0);
        let floor = 0.75;
        let lifted = placement(100.0, floor, 1.0, letterboxed, true);
        close(lifted, false, 100.0 * (floor - 0.125) / 0.75);
        // A cropped picture's foot is under the window's, so a bitmap is
        // lifted to the window's foot.
        let cropped = Picture::from_osd(CROPPED);
        let top = -124.0 / 720.0;
        close(
            placement(100.0, 1.0, 0.0, cropped, true),
            true,
            100.0 * (1.0 - top) / (1.0 - 2.0 * top),
        );
    }
}
