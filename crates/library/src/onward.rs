//! Where to go when a list runs out.
//!
//! At the end of the last file, or in its closing credits, the interface
//! names a place to go rather than offering a file-manager verb: the next
//! season of the show where one sits beside this one on disk, and failing
//! that, whatever else was left unfinished.

use std::path::PathBuf;

use dbm_lang::fill;
use dbm_mpv::MediaPath;

use crate::{playlist, resume};

/// The way on from the end of a list.
///
/// Named here rather than in the interface, since what it is called depends
/// on whether it is a season or a film, which are found in different ways.
pub struct Onward {
    /// A season's folder, or a film.
    pub path: PathBuf,
    /// What the pill says: a noun, like *Season 8*, for the thing you are
    /// going to.
    pub label: String,
    /// Whether it is more of the same show, since only another episode is a
    /// fair reason for the credits pill to cut credits short.
    pub season: bool,
}

/// The way on from `path`, which has just ended or is in its credits.
///
/// **Blocking.** Worker thread only: it lists the folder above this file's,
/// and failing that reads the resume files.
pub fn after(path: &MediaPath) -> Option<Onward> {
    next_season(path).or_else(|| unfinished(path))
}

fn next_season(path: &MediaPath) -> Option<Onward> {
    let (folder, season) = playlist::next_season(path.as_path())?;
    Some(Onward {
        path: folder,
        label: fill!("Season {}", season),
        season: true,
    })
}

/// Nothing of this show comes after it: a film, or the last season on disk.
/// The way on is then whatever else was left unfinished, the same film the
/// empty window offers to continue.
fn unfinished(path: &MediaPath) -> Option<Onward> {
    let film = resume::last_watched().filter(|r| r.path != *path)?;
    Some(Onward {
        label: match &film.show {
            Some(show) => format!("{show} · {}", film.title),
            None => film.title.clone(),
        },
        path: film.path.as_path().to_path_buf(),
        season: false,
    })
}
