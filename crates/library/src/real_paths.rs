//! Moving what was kept under a path with a link in it to the file's real
//! path, once.
//!
//! Everything remembered about a file is filed under its real path (see
//! `playlist`). Players before 1.0 filed a file found in an opened folder
//! under the path it was found at, so on Fedora's atomic desktops, where
//! `/home` links to `/var/home`, and on a mapped Windows drive, the watched
//! marks and resume points kept until then would go unread.

use std::collections::{HashMap, HashSet};

use dbm_mpv::MediaPath;

use crate::durations::{FINISHED, LENGTHS, watch_later_name};
use crate::opensubtitles::KEPT;

/// Written once the move is done, so it is done once.
const DONE: &str = "real-paths";

/// Move them, unless that was done before.
///
/// On a thread of its own, since following a path on a share that is gone
/// can take seconds, and nothing waits for it: until it is done, a file kept
/// under a link reads as not yet watched.
pub fn move_once() {
    let done = crate::paths::app_data_dir().join(DONE);
    if done.exists() {
        return;
    }
    crate::threads::spawn("dbm-real-paths", move || {
        let moves = moves();
        if !moves.is_empty() {
            log::info!("filing {} path(s) under where they really are", moves.len());
            refile(&moves);
        }
        let _ = std::fs::write(&done, "1");
    });
}

/// Each path kept that has a link in it, to its real path. Only paths some
/// map knows: a watch-later file is named by a hash, which can't be followed
/// back to a path. The lengths know every file ever played here.
fn moves() -> HashMap<MediaPath, MediaPath> {
    let known: HashSet<MediaPath> = LENGTHS
        .load()
        .into_keys()
        .chain(FINISHED.load().into_keys())
        .chain(KEPT.load().into_keys())
        .collect();
    known
        .into_iter()
        .filter_map(|path| {
            let real = MediaPath::from(crate::playlist::real(path.as_path())?.as_path());
            (real != path).then_some((path, real))
        })
        .collect()
}

/// The lengths are only a cache, but the empty window's Continue finds what
/// was left unfinished through them (see `resume`), so they move too.
fn refile(moves: &HashMap<MediaPath, MediaPath>) {
    FINISHED.refile(moves);
    KEPT.refile(moves);
    LENGTHS.refile(moves);
    let dir = crate::paths::watch_later_dir();
    for (from, to) in moves {
        let (from, to) = (
            dir.join(watch_later_name(from)),
            dir.join(watch_later_name(to)),
        );
        if from.is_file()
            && !to.exists()
            && let Err(e) = std::fs::rename(&from, &to)
        {
            log::warn!("could not move {}: {e}", from.display());
        }
    }
}
