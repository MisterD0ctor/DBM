//! Arriving at a file with a place saved in it.
//!
//! mpv resumes every file at the point its watch-later entry saved, on its
//! own and without a word. That is right for a film left last night and
//! wrong for one somebody means to see again, and only the person knows
//! which. So a file opened on purpose — a row of the playlist, the previous
//! and next buttons and keys, the way on at the end of a file, a file or a
//! folder opened — that has a real place saved in it is held, paused on
//! that place, until they say: on from there, or from the beginning. The
//! choice is the centre pill's — see `arriving` in app.slint.
//!
//! A file watched to its end, or all but, starts again from the top rather
//! than in its credits; one barely begun starts again too, rather than ask
//! about seconds. The way in's Continue has already said which, and so has
//! a pair of headphones skipping on: the media keys are answered from
//! threads of their own, by someone who may not be looking, and resume as
//! mpv always did.
//!
//! Autoplay cannot ask — nobody may be watching, and a held pause would end
//! the evening — so a file it brings in starts from the top. It got there
//! because the one before it played to its end, so the person is watching
//! in order, and a place saved in the next one is from an earlier viewing:
//! dropped into the middle of a scene with nobody at the controls is the
//! worse mistake. But not the only one, since the earlier viewing may be
//! what they rewatched the last episode to get back to; so for its first
//! ten seconds the corner pill offers to go on from that place.
//!
//! Decided before the file is asked for, from what the playlist already
//! knows — so that a file to be held is paused before it loads, rather than
//! heard playing from its saved place for the moment it took to notice —
//! and carried out when it has loaded.

use std::cell::RefCell;

use crate::library::durations::Progress;
use crate::playback::commands;
use crate::playback::mpv::Mpv;

/// A place saved this early is a file opened and left again rather than
/// one being watched, and it starts over rather than ask about seconds.
const WORTH_ASKING: f64 = 60.0;
/// Left this close to its end, a file has been watched to it in all but
/// name — its credits never heard, or its last checkpoint a little short of
/// them — and it starts over, as a finished one does. The last twentieth of
/// it, but never less than its last minute: a twentieth of an eleven-minute
/// episode is half a minute, and its credits run longer than that.
const ENDING_SHARE: f64 = 0.05;
const ENDING_LEAST: f64 = 60.0;
/// How far the position may wander from the saved place, in seconds, before
/// the choice is taken to have been made some other way: a seek, a skip.
const WANDER: f64 = 2.0;
/// How long into a file autoplay started over the offer to go on from its
/// saved place lasts, in seconds of the file.
const OFFER_FOR: f64 = 10.0;

/// What a file's saved place says to do on arriving at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Saved {
    /// Nothing saved, and mpv starts at the top by itself.
    Nothing,
    /// Watched through, or all but, or barely begun: from the top.
    Over,
    /// A place worth going back to, this many seconds in.
    Place(f64),
}

impl Saved {
    pub fn of(progress: &Progress) -> Self {
        if progress.start <= 0.0 {
            Saved::Nothing
        } else if progress.finished
            || progress.seconds > 0.0
                && progress.seconds - progress.start
                    <= (progress.seconds * ENDING_SHARE).max(ENDING_LEAST)
            || progress.start < WORTH_ASKING
        {
            Saved::Over
        } else {
            Saved::Place(progress.start)
        }
    }
}

/// What to do once the file asked for has loaded.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Then {
    /// Stay on the saved place, `from` seconds in, until told which.
    Ask { from: f64 },
    /// Back to the top, and then playing or not as it was asked for — and,
    /// for a file autoplay brought in, the place passed over, to offer.
    Over { play: bool, offer: Option<f64> },
}

#[derive(Default)]
struct State {
    /// Every playlist entry's path and what is saved for it, in order: what
    /// the driver last read of the lists.
    rows: Vec<(String, Progress)>,
    /// The entry playing, whether it is paused, how many files mpv has
    /// loaded and which is loaded now, as of the last frame.
    pos: i64,
    paused: bool,
    loads: u64,
    path: Option<String>,
    /// A file asked for and not loaded yet, and what then.
    pending: Option<Pending>,
    /// The file held on its saved place, waiting for an answer.
    asking: Option<Asking>,
    /// The place autoplay passed over, offered for a while.
    offer: Option<Offer>,
}

#[derive(Debug, Clone)]
struct Pending {
    wanted: String,
    then: Then,
    /// How many files had loaded, and which was playing, when it was asked
    /// for. A load is not enough to go by on its own: mpv says a file has
    /// loaded a moment before it says which, and for that moment the path
    /// is still the one being left.
    loads: u64,
    left: Option<String>,
}

#[derive(Debug, Clone)]
struct Offer {
    path: String,
    from: f64,
}

#[derive(Debug, Clone)]
struct Asking {
    path: String,
    from: f64,
    /// Whether the position has been seen on the saved place yet. Until it
    /// has, it is still arriving there and its wandering means nothing.
    settled: bool,
}

/// UI thread only.
#[derive(Default)]
pub struct Arrival {
    state: RefCell<State>,
}

impl Arrival {
    /// What the playlist knows of each entry, whenever the lists are read.
    pub fn learn(&self, rows: Vec<(String, Progress)>) {
        self.state.borrow_mut().rows = rows;
    }

    /// Where the playlist is, whether it is paused, how many files have
    /// loaded and which is loaded now: every frame. Carries out what was
    /// decided for a file asked for, once it is the one loaded.
    pub fn follow(&self, mpv: &Mpv, pos: i64, paused: bool, loads: u64, path: Option<&str>) {
        let mut state = self.state.borrow_mut();
        state.pos = pos;
        state.paused = paused;
        state.loads = loads;
        if state.path.as_deref() != path {
            state.path = path.map(str::to_owned);
        }
        let Some(pending) = &state.pending else {
            return;
        };
        if loads <= pending.loads {
            return;
        }
        if path != Some(pending.wanted.as_str()) {
            // Still the file being left, and the news is on its way; or a
            // file nobody here asked for, and the request is over.
            if path.is_some() && path != pending.left.as_deref() {
                state.pending = None;
            }
            return;
        }
        let Some(Pending { wanted, then, .. }) = state.pending.take() else {
            return;
        };
        match then {
            Then::Ask { from } => {
                state.asking = Some(Asking {
                    path: wanted,
                    from,
                    settled: false,
                });
            }
            Then::Over { play, offer } => {
                commands::restart(mpv);
                commands::set_pause(mpv, !play);
                state.offer = offer.map(|from| Offer { path: wanted, from });
            }
        }
    }

    /// The file `entry` played to its end and autoplay is moving on to
    /// `next`. mpv will resume it at its saved place by itself; this has it
    /// start from the top instead, offering the place for a while. Paused at
    /// once, since mpv is already opening it — usually in time for it to
    /// play nothing from that place first.
    pub fn played_out(&self, mpv: &Mpv, next: &str) {
        if crate::harness::armed() {
            return;
        }
        let mut state = self.state.borrow_mut();
        let saved = state
            .rows
            .iter()
            .find(|(path, _)| path == next)
            .map_or(Saved::Nothing, |(_, progress)| Saved::of(progress));
        let offer = match saved {
            Saved::Nothing => return,
            Saved::Over => None,
            Saved::Place(from) => Some(from),
        };
        commands::set_pause(mpv, true);
        Self::expect(
            &mut state,
            next.to_owned(),
            Then::Over { play: true, offer },
        );
    }

    /// The place autoplay passed over in the file playing, while it is still
    /// offered: for the first ten seconds of it, and not after anything else
    /// has been asked for.
    pub fn offered(&self, path: Option<&str>, time_pos: f64) -> Option<f64> {
        let mut state = self.state.borrow_mut();
        let offer = state.offer.as_ref()?;
        // The position passes through the place itself on the way: the file
        // reads nothing at first, mpv then resumes it at its place as it
        // opens, and the restart lands after that. So only being past the
        // first ten seconds anywhere but the place ends the offer, and it is
        // offered all the way through — shown and hidden as the position
        // went by, it flickered, and the title sequence's offer flashed up
        // in the gaps.
        let at_place = (time_pos - offer.from).abs() <= WANDER;
        if path != Some(offer.path.as_str()) || time_pos >= OFFER_FOR && !at_place {
            state.offer = None;
            return None;
        }
        Some(offer.from)
    }

    /// Take the offer: on from the place autoplay passed over.
    pub fn take_offer(&self, mpv: &Mpv) {
        if let Some(offer) = self.state.borrow_mut().offer.take() {
            commands::seek_to(mpv, offer.from);
        }
    }

    /// Note a request for `wanted`, to be carried out once it has loaded.
    fn expect(state: &mut State, wanted: String, then: Then) {
        state.pending = Some(Pending {
            wanted,
            then,
            loads: state.loads,
            left: state.path.clone(),
        });
    }

    /// Play entry `index` of the playlist, asked for on purpose. `play` is
    /// whether it should play once there; `None` keeps the pause as it is,
    /// which is what choosing a row or stepping with the bar always did.
    pub fn play_index(&self, mpv: &Mpv, index: i64, play: Option<bool>) {
        let mut state = self.state.borrow_mut();
        let playing = play.unwrap_or(!state.paused);
        let row = usize::try_from(index)
            .ok()
            .and_then(|i| state.rows.get(i))
            .filter(|_| !crate::harness::armed());
        let then = row.and_then(|(path, progress)| {
            let then = match Saved::of(progress) {
                Saved::Nothing => return None,
                Saved::Over => Then::Over {
                    play: playing,
                    offer: None,
                },
                Saved::Place(from) => Then::Ask { from },
            };
            Some((path.clone(), then))
        });
        // Whatever was being asked about or offered, the answer is now
        // another file.
        state.asking = None;
        state.offer = None;
        match then {
            None => {
                state.pending = None;
                commands::playlist_play(mpv, index);
                if play == Some(true) {
                    commands::set_pause(mpv, false);
                }
            }
            // Paused before it is asked for, so that what it plays first is
            // nothing, rather than its saved place for a moment.
            Some((wanted, then)) => {
                commands::set_pause(mpv, true);
                commands::playlist_play(mpv, index);
                Self::expect(&mut state, wanted, then);
            }
        }
    }

    /// The entry `delta` along from the one playing. Before the lists have
    /// been read there is nothing to go by, and it steps as it always did.
    pub fn step(&self, mpv: &Mpv, delta: i32, play: Option<bool>) {
        let (target, known) = {
            let state = self.state.borrow();
            (state.pos + i64::from(delta), state.rows.len() as i64)
        };
        if (0..known).contains(&target) {
            self.play_index(mpv, target, play);
        } else if known == 0 {
            if play == Some(true) {
                commands::advance(mpv, delta);
            } else {
                commands::playlist_step(mpv, delta);
            }
        }
    }

    /// A playlist about to be loaded whose first file is `path`, with
    /// `saved` for it. Whether the load should leave it paused — which it
    /// should while there is something to do once it is in.
    pub fn open_list(&self, mpv: &Mpv, path: String, saved: Saved) -> bool {
        let mut state = self.state.borrow_mut();
        state.asking = None;
        let then = match saved {
            _ if crate::harness::armed() => None,
            Saved::Nothing => None,
            Saved::Over => Some(Then::Over {
                play: true,
                offer: None,
            }),
            Saved::Place(from) => Some(Then::Ask { from }),
        };
        let Some(then) = then else {
            state.pending = None;
            return false;
        };
        commands::set_pause(mpv, true);
        Self::expect(&mut state, path, then);
        true
    }

    /// Whether a file is still on its way in to be held or started over —
    /// which is when nothing else should set it playing.
    pub fn holding(&self) -> bool {
        self.state.borrow().pending.is_some()
    }

    /// Where the file being held was left, if one is — or `None` once the
    /// choice has been made, by the pill or otherwise: playing on, a seek
    /// away from the place, another file.
    pub fn asking(&self, path: Option<&str>, time_pos: f64, paused: bool) -> Option<f64> {
        let mut state = self.state.borrow_mut();
        let asking = state.asking.as_mut()?;
        let near = (time_pos - asking.from).abs() <= WANDER;
        asking.settled |= near;
        let held = path == Some(asking.path.as_str()) && paused && (near || !asking.settled);
        let from = asking.from;
        if !held {
            state.asking = None;
            return None;
        }
        Some(from)
    }

    /// The answer: on from the saved place, or from the beginning.
    pub fn answer(&self, mpv: &Mpv, from_the_start: bool) {
        self.state.borrow_mut().asking = None;
        if from_the_start {
            commands::restart(mpv);
        }
        commands::set_pause(mpv, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(start: f64, seconds: f64, finished: bool) -> Saved {
        Saved::of(&Progress {
            seconds,
            fraction: if seconds > 0.0 {
                (start / seconds) as f32
            } else {
                0.0
            },
            start,
            finished,
        })
    }

    #[test]
    fn a_place_part_way_through_is_asked_about() {
        assert_eq!(saved(754.0, 1400.0, false), Saved::Place(754.0));
        // A length not known yet is no reason not to ask.
        assert_eq!(saved(754.0, 0.0, false), Saved::Place(754.0));
    }

    #[test]
    fn nothing_saved_is_left_to_mpv() {
        assert_eq!(saved(0.0, 1400.0, false), Saved::Nothing);
        // Watched through to the end: mpv forgot the place itself.
        assert_eq!(saved(0.0, 1400.0, true), Saved::Nothing);
    }

    #[test]
    fn the_end_and_the_very_beginning_start_over() {
        // Left in the credits.
        assert_eq!(saved(1350.0, 1400.0, true), Saved::Over);
        // All but finished, with no credits heard to say so: within the last
        // minute of an episode...
        assert_eq!(saved(650.0, 686.0, false), Saved::Over);
        // ...or the last twentieth of a film.
        assert_eq!(saved(6900.0, 7200.0, false), Saved::Over);
        assert_eq!(saved(6700.0, 7200.0, false), Saved::Place(6700.0));
        // Opened and left again.
        assert_eq!(saved(25.0, 1400.0, false), Saved::Over);
    }
}
