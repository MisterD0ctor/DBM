//! Where to go when a list runs out.
//!
//! The end of the last file — or its closing credits, where the credits pill
//! makes the same offer before the file is over — is the loudest the
//! interface ever speaks, and the last thing it says about a film. So it
//! names a place to go rather than offering a file-manager verb: the next
//! season of the show where one sits beside this one on disk, and failing
//! that, whatever else was left unfinished.

use std::path::Path;

use crate::library::{playlist, resume};

/// The way on from the end of a list.
///
/// Named here rather than in the interface: what it is called depends on
/// which of the two it turned out to be, and the interface should not have to
/// know that a season and a film are found in different ways.
pub struct Onward {
    pub path: String,
    /// What the pill says. A noun, as *Season 8* is: the thing you are going
    /// to, not a verb about going there.
    pub label: String,
    /// Whether it is more of the same show. The credits pill is the offer to
    /// skip ahead to the next episode, and only another episode can honour
    /// that — a different film is a fair thing to offer when a file has ended
    /// and not a fair thing to cut a film's credits short for.
    pub season: bool,
}

/// The way on from `path`, which has just ended or is in its credits.
///
/// **Blocking.** Worker thread only: it lists the folder above this file's,
/// and failing that reads the resume files.
pub fn after(path: &str) -> Option<Onward> {
    next_season(path).or_else(|| unfinished(path))
}

fn next_season(path: &str) -> Option<Onward> {
    let (folder, season) = playlist::next_season(Path::new(path))?;
    Some(Onward {
        path: folder.to_string_lossy().into_owned(),
        label: format!("Season {season}"),
        season: true,
    })
}

/// Nothing of this show comes after it — a film, or the last season on disk.
/// The next thing to watch is then whatever else was left unfinished, which
/// is the same film the empty window offers to continue and the only other
/// thing the player can name without being told.
fn unfinished(path: &str) -> Option<Onward> {
    let film = resume::last_watched().filter(|r| r.path != path)?;
    Some(Onward {
        label: match &film.show {
            Some(show) => format!("{show} · {}", film.title),
            None => film.title.clone(),
        },
        path: film.path,
        season: false,
    })
}
