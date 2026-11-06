//! Arriving at a file with a place saved in it.
//!
//! mpv resumes every file at its saved place on its own, which is right for a
//! film left last night and wrong for one somebody means to see again, and
//! only the person knows which. So a file opened on purpose (a playlist row,
//! previous and next, the way on, a file or folder opened) that has a real
//! place saved is held, paused there, until they choose: on from there, or
//! from the beginning (see `arriving` in app.slint).
//!
//! A file watched to its end, or all but, starts from the top rather than in
//! its credits, and so does one barely begun. The way in's Continue has
//! already chosen, and so have the media keys, answered on threads of their
//! own for someone who may not be looking, which resume as mpv does.
//!
//! Autoplay can't ask, since nobody may be watching, so a file it brings in
//! starts from the top: the person is watching in order, so a place saved in
//! the next file is from an earlier viewing. That viewing may be what they
//! meant to get back to, so for its first ten seconds the corner pill offers
//! to go on from it.
//!
//! Decided before the file is asked for, from what the playlist knows, so a
//! file to be held is paused before it loads rather than heard from its saved
//! place first, and carried out once it has loaded.

use std::cell::RefCell;

use dbm_library::durations::Progress;
use dbm_library::resume::Saved;
use dbm_mpv::{Direction, MediaPath, Mpv};

use crate::playback::commands;

/// How far the position may wander from the saved place, in seconds, before
/// the choice is taken to have been made some other way: a seek, a skip.
const WANDER: f64 = 2.0;
/// How long into a file autoplay started over the offer to go on from its
/// saved place lasts, in seconds of the file.
const OFFER_FOR: f64 = 10.0;

/// What to do once the file asked for has loaded.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Then {
    /// Stay on the saved place, `from` seconds in, until told which.
    Ask { from: f64 },
    /// Back to the top, then playing or not as asked, and, for a file
    /// autoplay brought in, the place passed over, to offer.
    Over { play: bool, offer: Option<f64> },
}

/// Whether a file asked for plays once it is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// It plays, as "next" means.
    Playing,
    /// It plays or stays paused as the file before it did, as choosing a row
    /// or stepping with the bar does.
    AsBefore,
}

/// What becomes of a file autoplay brings in: nothing, where nothing is
/// saved for it, and otherwise it starts from the top, with its place
/// offered where it was left part way.
fn autoplayed(progress: &Progress) -> Option<Then> {
    let offer = match Saved::of(progress) {
        Saved::Nothing => return None,
        Saved::Over => None,
        Saved::Place(from) => Some(from),
    };
    Some(Then::Over { play: true, offer })
}

#[derive(Default)]
struct State {
    /// Every playlist entry's path and what is saved for it, in order: what
    /// the player last read of the lists.
    rows: Vec<(MediaPath, Progress)>,
    /// The entry playing, whether it is paused, how many files mpv has
    /// loaded and which is loaded now, as of the last frame.
    pos: Option<usize>,
    paused: bool,
    loads: u64,
    path: Option<MediaPath>,
    /// What, if anything, is waiting on a file asked for or on a choice.
    phase: Phase,
}

/// What the one file in hand is waiting for.
///
/// One phase at a time: a request in flight, the answer it turns into when
/// the file loads, or the offer autoplay leaves behind. Each replaces the
/// last, so a choice about an earlier file cannot linger beside a newer one.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// A file asked for and not loaded yet, and what then.
    Pending(Pending),
    /// The file held on its saved place, waiting for an answer.
    Asking(Asking),
    /// The place autoplay passed over, offered for a while.
    Offering(Offer),
}

#[derive(Debug, Clone)]
struct Pending {
    wanted: MediaPath,
    then: Then,
    /// How many files had loaded, and which was playing, when it was asked
    /// for. A load alone isn't enough, since mpv says a file has loaded a
    /// moment before it says which.
    loads: u64,
    left: Option<MediaPath>,
}

#[derive(Debug, Clone)]
struct Offer {
    path: MediaPath,
    from: f64,
}

#[derive(Debug, Clone)]
struct Asking {
    path: MediaPath,
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
    pub fn learn(&self, rows: Vec<(MediaPath, Progress)>) {
        self.state.borrow_mut().rows = rows;
    }

    /// Where the playlist is, whether it is paused, how many files have
    /// loaded and which is loaded now: on every run of the player. Carries
    /// out what was decided for a file asked for, once it is the one loaded.
    pub fn follow(
        &self,
        mpv: &Mpv,
        pos: Option<usize>,
        paused: bool,
        loads: u64,
        path: Option<&MediaPath>,
    ) {
        let mut state = self.state.borrow_mut();
        state.pos = pos;
        state.paused = paused;
        state.loads = loads;
        if state.path.as_ref() != path {
            state.path = path.cloned();
        }
        let Phase::Pending(pending) = &state.phase else {
            return;
        };
        if loads <= pending.loads {
            return;
        }
        if path != Some(&pending.wanted) {
            // Still the file being left, and the news is on its way; or a
            // file nobody here asked for, and the request is over.
            if path.is_some() && path != pending.left.as_ref() {
                state.phase = Phase::Idle;
            }
            return;
        }
        let Phase::Pending(pending) = std::mem::replace(&mut state.phase, Phase::Idle) else {
            unreachable!("the phase was just read as pending");
        };
        let Pending { wanted, then, .. } = pending;
        match then {
            Then::Ask { from } => {
                state.phase = Phase::Asking(Asking {
                    path: wanted,
                    from,
                    settled: false,
                });
            }
            Then::Over { play, offer } => {
                commands::restart(mpv);
                commands::set_pause(mpv, !play);
                state.phase = offer
                    .map(|from| Phase::Offering(Offer { path: wanted, from }))
                    .unwrap_or(Phase::Idle);
            }
        }
    }

    /// The file `entry` played to its end and autoplay is moving on to
    /// `next`. mpv would resume it at its saved place; this starts it from
    /// the top instead, offering the place for a while. Paused at once, since
    /// mpv is already opening it, usually in time to play nothing from there.
    pub fn played_out(&self, mpv: &Mpv, next: &MediaPath) {
        if crate::run_mode::armed() {
            return;
        }
        let mut state = self.state.borrow_mut();
        // The file that ended can have nothing left to answer or offer.
        state.phase = Phase::Idle;
        let Some(then) = state
            .rows
            .iter()
            .find(|(path, _)| path == next)
            .and_then(|(_, progress)| autoplayed(progress))
        else {
            return;
        };
        commands::set_pause(mpv, true);
        Self::expect(&mut state, next.clone(), then);
    }

    /// Autoplay going on before the file playing has ended, from its credits
    /// (see `skips`): the next entry starts from the top, playing, as after
    /// [`played_out`](Self::played_out).
    pub fn go_on(&self, mpv: &Mpv) {
        let mut state = self.state.borrow_mut();
        let Some(index) = state.pos.map(|pos| pos + 1) else {
            return;
        };
        let then = state
            .rows
            .get(index)
            .filter(|_| !crate::run_mode::armed())
            .and_then(|(path, progress)| Some((path.clone(), autoplayed(progress)?)));
        Self::ask_for(&mut state, mpv, index, then, Start::Playing);
    }

    /// The place autoplay passed over in the file playing, while it is still
    /// offered: for the first ten seconds of it, and not after anything else
    /// has been asked for.
    pub fn offered(&self, path: Option<&MediaPath>, time_pos: f64) -> Option<f64> {
        let mut state = self.state.borrow_mut();
        let Phase::Offering(offer) = &state.phase else {
            return None;
        };
        // The position passes through the place on the way (mpv resumes it
        // there as it opens, then the restart lands), so only being past the
        // first ten seconds anywhere but the place ends the offer. Following
        // the position in and out would flicker.
        let at_place = (time_pos - offer.from).abs() <= WANDER;
        if path != Some(&offer.path) || time_pos >= OFFER_FOR && !at_place {
            state.phase = Phase::Idle;
            return None;
        }
        Some(offer.from)
    }

    /// Take the offer: on from the place autoplay passed over.
    pub fn take_offer(&self, mpv: &Mpv) {
        let mut state = self.state.borrow_mut();
        let Phase::Offering(offer) = &state.phase else {
            return;
        };
        let from = offer.from;
        state.phase = Phase::Idle;
        commands::seek_to(mpv, from);
    }

    /// Note a request for `wanted`, to be carried out once it has loaded.
    fn expect(state: &mut State, wanted: MediaPath, then: Then) {
        state.phase = Phase::Pending(Pending {
            wanted,
            then,
            loads: state.loads,
            left: state.path.clone(),
        });
    }

    /// Play entry `index` of the playlist, asked for on purpose, playing or
    /// not once there as `start` says.
    pub fn play_index(&self, mpv: &Mpv, index: usize, start: Start) {
        let mut state = self.state.borrow_mut();
        let playing = match start {
            Start::Playing => true,
            Start::AsBefore => !state.paused,
        };
        let row = state.rows.get(index).filter(|_| !crate::run_mode::armed());
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
        Self::ask_for(&mut state, mpv, index, then, start);
    }

    /// Ask mpv for entry `index`, and note what to do once it has loaded.
    fn ask_for(
        state: &mut State,
        mpv: &Mpv,
        index: usize,
        then: Option<(MediaPath, Then)>,
        start: Start,
    ) {
        // Whatever was being asked about or offered gives way to another
        // file.
        state.phase = Phase::Idle;
        match then {
            None => {
                commands::playlist_play(mpv, index);
                if start == Start::Playing {
                    commands::set_pause(mpv, false);
                }
            }
            // Paused before it is asked for, so that what it plays first is
            // nothing, rather than its saved place for a moment.
            Some((wanted, then)) => {
                commands::set_pause(mpv, true);
                commands::playlist_play(mpv, index);
                Self::expect(state, wanted, then);
            }
        }
    }

    /// The entry next to the one playing, `step` along. Before the lists
    /// have been read there is nothing to go by, and it steps plainly.
    pub fn step(&self, mpv: &Mpv, step: Direction, start: Start) {
        let (target, known) = {
            let state = self.state.borrow();
            // Nothing playing is just before the first.
            let from = state.pos.map_or(-1, |pos| pos as i64);
            let by = match step {
                Direction::Forward => 1,
                Direction::Back => -1,
            };
            (from + by, state.rows.len())
        };
        if let Some(target) = usize::try_from(target).ok().filter(|&row| row < known) {
            self.play_index(mpv, target, start);
        } else if known == 0 {
            match start {
                Start::Playing => commands::advance(mpv, step),
                Start::AsBefore => commands::playlist_step(mpv, step),
            }
        }
    }

    /// A playlist about to be loaded whose first file is `path`, with
    /// `saved` for it.
    pub fn open_list(&self, mpv: &Mpv, path: MediaPath, saved: Saved) {
        let mut state = self.state.borrow_mut();
        // The file that was in hand, or its answer, is not this list's.
        state.phase = Phase::Idle;
        let then = match saved {
            _ if crate::run_mode::armed() => None,
            Saved::Nothing => None,
            Saved::Over => Some(Then::Over {
                play: true,
                offer: None,
            }),
            Saved::Place(from) => Some(Then::Ask { from }),
        };
        let Some(then) = then else {
            return;
        };
        commands::set_pause(mpv, true);
        Self::expect(&mut state, path, then);
    }

    /// Whether a file is still on its way in to be held or started over,
    /// when nothing else should set it playing.
    pub fn holding(&self) -> bool {
        matches!(self.state.borrow().phase, Phase::Pending(_))
    }

    /// Where the file being held was left, or `None` once the choice has been
    /// made, by the pill or otherwise: playing on, a seek away from the
    /// place, another file.
    pub fn asking(&self, path: Option<&MediaPath>, time_pos: f64, paused: bool) -> Option<f64> {
        let mut state = self.state.borrow_mut();
        let (held, from) = {
            let Phase::Asking(asking) = &mut state.phase else {
                return None;
            };
            let near = (time_pos - asking.from).abs() <= WANDER;
            asking.settled |= near;
            (
                path == Some(&asking.path) && paused && (near || !asking.settled),
                asking.from,
            )
        };
        if !held {
            state.phase = Phase::Idle;
            return None;
        }
        Some(from)
    }

    /// The answer: on from the saved place, or from the beginning.
    pub fn answer(&self, mpv: &Mpv, from_the_start: bool) {
        {
            let mut state = self.state.borrow_mut();
            if matches!(state.phase, Phase::Asking(_)) {
                state.phase = Phase::Idle;
            }
        }
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
            seconds: Some(seconds).filter(|seconds| *seconds > 0.0),
            start: Some(start).filter(|start| *start > 0.0),
            finished,
        })
    }

    #[test]
    fn a_new_request_replaces_what_the_last_file_was_offering() {
        let mut state = State {
            phase: Phase::Offering(Offer {
                path: "old.mkv".into(),
                from: 12.0,
            }),
            ..State::default()
        };
        Arrival::expect(&mut state, "new.mkv".into(), Then::Ask { from: 34.0 });
        assert!(matches!(state.phase, Phase::Pending(_)));
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
