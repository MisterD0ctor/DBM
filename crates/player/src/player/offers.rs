//! The films the way in offers to pick back up, and which of them lights the
//! empty window.

use dbm_library::preview::Still;
use dbm_library::resume::Resume;
use dbm_mpv::MediaPath;

/// What the last look found left unfinished, newest first and never empty,
/// with the film whose frame is behind the way in.
pub(super) struct Offers {
    films: Vec<Resume>,
    /// The newest, until the pointer or the ring comes to another's row.
    lit: MediaPath,
}

impl Offers {
    /// `None` when nothing was left unfinished, which on a fresh install is
    /// every time.
    pub(super) fn new(films: Vec<Resume>) -> Option<Self> {
        let lit = films.first()?.path.clone();
        Some(Self { films, lit })
    }

    pub(super) fn films(&self) -> &[Resume] {
        &self.films
    }

    /// The frame behind the way in, where one was ever cut for that film.
    pub(super) fn lit_still(&self) -> Option<&Still> {
        let film = self.films.iter().find(|film| film.path == self.lit)?;
        film.still.as_ref()
    }

    /// Come to the film offered at `path`, and give its frame when that
    /// changes the light. A film with no frame leaves the light as it is,
    /// rather than putting out a picture for no reason anyone can see.
    pub(super) fn light(&mut self, path: &str) -> Option<&Still> {
        let film = self.films.iter().find(|film| film.path.as_str() == path)?;
        if film.path == self.lit {
            return None;
        }
        let still = film.still.as_ref()?;
        self.lit = film.path.clone();
        Some(still)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn film(path: &str, framed: bool) -> Resume {
        Resume {
            path: MediaPath::for_test(path),
            show: None,
            title: path.to_owned(),
            fraction: 0.5,
            seconds_left: 600.0,
            still: framed.then(|| Still {
                rgba: vec![0; 4],
                width: 1,
                height: 1,
            }),
        }
    }

    #[test]
    fn nothing_unfinished_is_nothing_to_offer() {
        assert!(Offers::new(Vec::new()).is_none());
    }

    #[test]
    fn the_newest_lights_the_window_until_another_is_come_to() {
        let mut offers = Offers::new(vec![
            film("/newest.mkv", true),
            film("/older.mkv", true),
            film("/unframed.mkv", false),
        ])
        .unwrap();
        assert!(offers.lit_still().is_some());
        // Coming back to the film already lit changes nothing.
        assert!(offers.light("/newest.mkv").is_none());

        assert!(offers.light("/older.mkv").is_some());
        assert!(offers.light("/older.mkv").is_none());

        // One with no frame, or a path no row offers, leaves the light where
        // it was, so the way back to the newest is a change again.
        assert!(offers.light("/unframed.mkv").is_none());
        assert!(offers.light("/gone.mkv").is_none());
        assert!(offers.light("/newest.mkv").is_some());
    }

    #[test]
    fn a_newest_with_no_frame_leaves_the_window_to_the_mark() {
        let mut offers = Offers::new(vec![
            film("/unframed.mkv", false),
            film("/framed.mkv", true),
        ])
        .unwrap();
        assert!(offers.lit_still().is_none());
        assert!(offers.light("/framed.mkv").is_some());
    }
}
