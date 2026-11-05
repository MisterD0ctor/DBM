//! What follows the playing list and asks for background work when it
//! moves: the length of the file playing, the rest of the list described,
//! and the seasons beside it. Each compares a little state a frame, and only
//! a change sends anything to the worker.

use dbm_library::{durations, probe, shelf};
use dbm_mpv::MediaPath;

use crate::playback::state::PlayerState;
use crate::worker::{Completion, Worker};

/// Writes down how long the playing file is, once mpv has worked it out.
///
/// Lives on the frame path and does nothing there but compare two values: the
/// write itself goes to the worker, because it reads and rewrites a file.
#[derive(Default)]
pub struct Recorder {
    /// What was last handed to the worker, so the same fact isn't rewritten
    /// every frame.
    last: Option<(MediaPath, f64)>,
    /// The file playing, its length, and whether it was within a moment of
    /// its end. Autoplay moves straight past the end of a file and mpv never
    /// reports reaching it, so this is how that file is still counted.
    playing: Option<(MediaPath, f64, bool)>,
    /// The last file written down as finished, so it is written once.
    finished: Option<MediaPath>,
}

/// How near the end a file autoplay moved on from must have been: a frame or
/// two short of the last one, not a film abandoned in its final minute.
const NEAR_END: f64 = 2.0;

impl Recorder {
    pub fn poll(&mut self, worker: &Worker, player: &PlayerState) {
        self.watch_ending(worker, player);
        let Some(path) = player.path.clone() else {
            return;
        };
        let seconds = player.duration;
        // Zero until the file is open and demuxed, which is most of the first
        // moment of every file.
        if seconds <= 0.0 {
            return;
        }
        if self
            .last
            .as_ref()
            .is_some_and(|(p, s)| *p == path && *s == seconds)
        {
            return;
        }
        self.last = Some((path.clone(), seconds));
        worker.run(move |_mpv| durations::record(&path, seconds));
    }

    /// Notices a file reaching its ending: its credits, its last frame, or
    /// its last moments before autoplay took the next one.
    fn watch_ending(&mut self, worker: &Worker, player: &PlayerState) {
        if self.playing.as_ref().map(|p| &p.0) != player.path.as_ref() {
            if let Some((path, duration, true)) = self.playing.take() {
                self.finish(worker, path, duration);
            }
            self.playing = player.path.clone().map(|p| (p, 0.0, false));
        }
        let Some(path) = player.path.as_ref() else {
            return;
        };
        // The file's own numbers, and past its halfway point, since for a
        // frame after a new file opens the flags and position can still be
        // the last one's.
        let into_it = player.duration > 0.0 && player.time_pos >= player.duration * 0.5;
        if let Some(playing) = self.playing.as_mut() {
            playing.1 = player.duration;
            playing.2 = into_it && player.time_pos >= player.duration - NEAR_END;
        }
        let ended = player.credits_rolling()
            || (player.eof_reached && player.time_pos >= player.duration - NEAR_END);
        if into_it && ended && self.finished.as_ref() != Some(path) {
            self.finish(worker, path.clone(), player.time_pos);
        }
    }

    fn finish(&mut self, worker: &Worker, path: MediaPath, at: f64) {
        if self.finished.as_ref() == Some(&path) {
            return;
        }
        self.finished = Some(path.clone());
        worker.run(move |_mpv| durations::record_finished(&path, at));
    }
}

/// Keeps one scan running for whatever the playlist currently is.
#[derive(Default)]
pub struct Scan {
    /// The paths the running scan was started for, so a list read that
    /// changed only the tracks does not start the same scan over.
    requested: Vec<MediaPath>,
}

impl Scan {
    /// Describe every entry in this playlist that is not already known.
    ///
    /// Called when a list read lands, not per frame. Compares, and returns at
    /// once when the paths are the ones already being scanned.
    pub fn request(&mut self, paths: &[MediaPath], worker: &Worker) {
        if paths == self.requested.as_slice() {
            return;
        }
        self.requested = paths.to_vec();
        probe::scan(paths.to_vec(), worker.reporter(Completion::Probed));
    }
}

/// Asks for the seasons beside the playlist, once per list.
#[derive(Default)]
pub struct Neighbours {
    asked: Vec<MediaPath>,
    /// Ask again for the same list, since progress is written to disk without
    /// an event, and opening the panel is when it is worth reading again.
    stale: bool,
}

impl Neighbours {
    pub fn refresh(&mut self) {
        self.stale = true;
    }

    /// Look beside this list if it is a new one, or the last look is stale.
    ///
    /// Returns whether the list itself changed, so what was found beside the
    /// old one can be put away rather than shown around another show.
    pub fn request(&mut self, paths: &[MediaPath], worker: &Worker) -> bool {
        let moved = paths != self.asked.as_slice();
        if !moved && !self.stale {
            return false;
        }
        self.stale = false;
        self.asked = paths.to_vec();
        let paths = paths.to_vec();
        worker.submit(move |_mpv| {
            let seasons = shelf::look_beside(&paths);
            Some(Completion::Beside { paths, seasons })
        });
        moved
    }
}
