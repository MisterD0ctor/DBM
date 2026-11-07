//! Turning a path into something mpv can play.
//!
//! Opening a single file is rarely what someone means: they want that
//! episode, with the rest of the season queued behind it. So a file becomes
//! its directory's videos with the chosen one selected, and a directory
//! becomes everything beneath it.
//!
//! Every entry is the file's real path, with every link followed, since
//! everything remembered about a file is filed under its path, and two ways
//! to one file would otherwise be two histories. The list itself is the
//! folder the person opened, links and all.
//!
//! All of this is filesystem work (`read_dir` on a network share or a
//! directory of thousands can take seconds), so it runs on the worker thread.
//! Nothing here touches mpv or the UI; it produces a plain list.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use dbm_mpv::{MediaPath, RealPath};

use crate::resume::Saved;

/// Extensions treated as video, matched case-insensitively. Public so the
/// open dialog filters by the same list, and never shows a file the scanner
/// then ignores.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "ts", "m2ts",
];

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| VIDEO_EXTENSIONS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
}

/// Why a path could not be turned into a playlist.
///
/// The failure is a value rather than a sentence, so whoever shows it can say
/// it in the language spoken then, and whoever reads the log can see which
/// kind it was.
#[derive(Debug)]
pub enum Error {
    /// Nothing is at the path: it does not exist, or it is neither a file
    /// nor a folder.
    NothingAt(PathBuf),
    /// A folder held no videos directly in it.
    NoVideosIn(PathBuf),
    /// A folder and everything beneath it held no videos.
    NoVideosUnder(PathBuf),
    /// A folder could not be listed.
    Unreadable { dir: PathBuf, source: io::Error },
    /// The playlist file mpv is to read could not be written.
    PlaylistWrite { path: PathBuf, source: io::Error },
}

/// Scan a path a person handed the player into a playlist, and write it out
/// for mpv's `loadlist`.
///
/// Every way in comes through here, so this is where a path becomes the one
/// every record of the file is filed under: a document portal's path is
/// looked up, then every link is followed.
///
/// **Blocking**: worker thread only. It asks the portal over D-Bus and reads
/// a directory that may be huge or on a network share.
pub fn prepare(path: &Path) -> Result<Prepared, Error> {
    let selection = resolve(&crate::documents::real(path))?;
    let m3u = crate::paths::playlist_m3u();
    write_m3u(&selection.videos, &m3u).map_err(|source| {
        log::warn!("could not write {}: {source}", m3u.display());
        Error::PlaylistWrite {
            path: m3u.clone(),
            source,
        }
    })?;
    // As the list tells mpv it, which is what it will report and hash.
    let first = selection
        .videos
        .get(selection.start)
        .map(MediaPath::from)
        .ok_or_else(|| Error::NothingAt(path.to_path_buf()))?;
    let saved = crate::durations::of(std::slice::from_ref(&first))
        .first()
        .map_or(Saved::Nothing, Saved::of);
    Ok(Prepared {
        m3u,
        start: selection.start,
        count: selection.videos.len(),
        first,
        saved,
    })
}

/// A playlist that has been scanned and written out, ready to hand to mpv.
pub struct Prepared {
    /// The M3U on disk, this open's own, for whoever loads it to delete once
    /// mpv has read it.
    pub m3u: PathBuf,
    /// Which entry to start on.
    pub start: usize,
    pub count: usize,
    /// That entry's path, and what is saved for it (see `arrival`).
    pub first: MediaPath,
    pub saved: Saved,
}

/// What to play, and where to start.
pub struct Selection {
    pub videos: Vec<RealPath>,
    pub start: usize,
}

/// Build a playlist from whatever the user pointed at.
///
/// A directory is scanned recursively; a file gets its siblings, which is
/// what makes "next episode" work without asking for the folder. `path` has
/// had a portal's document looked up (see [`prepare`]).
fn resolve(path: &Path) -> Result<Selection, Error> {
    // Absolute, so a relative path's folder is known, but not followed: a
    // linked file's siblings are the ones beside the link.
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if path.is_dir() {
        let videos = scan_recursive(&path)?;
        return Ok(Selection { videos, start: 0 });
    }
    if !path.is_file() {
        return Err(Error::NothingAt(path));
    }
    let real = RealPath::of(&path).map_err(|source| Error::Unreadable {
        dir: path.clone(),
        source,
    })?;
    // A file the scan doesn't take for a video, or one whose folder can't be
    // listed, plays alone.
    let videos = path
        .parent()
        .and_then(|dir| scan_flat(dir).ok())
        .unwrap_or_default();
    match videos.iter().position(|video| *video == real) {
        Some(start) => Ok(Selection { videos, start }),
        None => Ok(Selection {
            videos: vec![real],
            start: 0,
        }),
    }
}

/// Judge the folder `video` is in, for a name shown where no playlist of
/// that folder has judged it, as on the way in.
///
/// **Blocking.** Worker thread only: it lists the folder.
pub(crate) fn notice_folder_of(video: &Path) {
    if let Some(dir) = video.parent() {
        // Listing it is what judges it. One that can't be listed leaves the
        // name to say what it is on its own.
        let _ = scan_flat(dir);
    }
}

/// Videos directly in `dir`, in watching order, each at its real path. No
/// recursion: the siblings of an episode are its season, not the whole
/// library.
pub fn scan_flat(dir: &Path) -> Result<Vec<RealPath>, Error> {
    let unreadable = |source| Error::Unreadable {
        dir: dir.to_path_buf(),
        source,
    };
    let real_dir = RealPath::of(dir).map_err(unreadable)?;
    let found = real_dir
        .entries()
        .map_err(unreadable)?
        .map(|entry| Found::of(entry, dir))
        .filter(|found| !found.folder && found.video())
        .collect();
    let videos = in_watching_order(found);
    if videos.is_empty() {
        return Err(Error::NoVideosIn(dir.to_path_buf()));
    }
    Ok(videos)
}

/// One entry of a folder: where it was found, which is what a person sees
/// and what the list is ordered by, and where it really is.
struct Found {
    seen: PathBuf,
    real: RealPath,
    folder: bool,
}

impl Found {
    /// `entry`, of the folder a person reached as `dir`.
    fn of(entry: dbm_mpv::Entry, dir: &Path) -> Self {
        Self {
            seen: dir.join(&entry.name),
            real: entry.real,
            folder: entry.folder,
        }
    }

    /// Either name will do, since a link and what it points at can be
    /// called differently.
    fn video(&self) -> bool {
        is_video_file(&self.seen) || is_video_file(self.real.as_path())
    }
}

/// Real paths, in the order the paths they were found at are watched in,
/// each once: two links to one file are one entry.
///
/// Their folders are judged here, since every listing passes through: by the
/// paths found, which the order reads, and the real ones, which everything
/// after reads.
fn in_watching_order(found: Vec<Found>) -> Vec<RealPath> {
    let mut names = Vec::new();
    for found in &found {
        names.push(found.seen.to_string_lossy());
        if found.real.as_path() != found.seen {
            names.push(found.real.as_path().to_string_lossy());
        }
    }
    dbm_naming::notice(&names);
    let mut kept = HashSet::new();
    ordered(found, |found| &found.seen)
        .into_iter()
        .map(|found| found.real)
        .filter(|real| kept.insert(real.clone()))
        .collect()
}

/// Put videos in the order a person watches them.
///
/// Not the order their bytes sort in, which puts `Show - 100` before
/// `Show - 11` and `Season 10` between `Season 1` and `Season 2`; autoplay,
/// Next and the end-of-season pill all trust this order.
///
/// A show's files are kept together, where the first of them falls in name
/// order, and ordered by the season and episode their names carry, so two
/// release groups can't split one show around another. Everything else is
/// compared as Explorer compares names: digit runs as numbers, case ignored.
pub fn watching_order(videos: Vec<PathBuf>) -> Vec<PathBuf> {
    ordered(videos, |video| video)
}

/// [`watching_order`] for anything that has a path.
fn ordered<T>(mut items: Vec<T>, path: impl Fn(&T) -> &Path) -> Vec<T> {
    items.sort_by(|a, b| natural_path(path(a), path(b)));
    let keys: Vec<Option<(String, u32, u32)>> = items
        .iter()
        .map(
            |item| match dbm_naming::parse(&path(item).to_string_lossy()) {
                dbm_naming::Media::Episode {
                    show,
                    season,
                    episode,
                    ..
                } => Some((show.to_lowercase(), season.unwrap_or(0), episode)),
                dbm_naming::Media::Movie { .. } => None,
            },
        )
        .collect();
    let mut first: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, key) in keys.iter().enumerate() {
        if let Some((show, ..)) = key {
            first.entry(show.as_str()).or_insert(i);
        }
    }
    // Name order breaks every tie, so equal numbers keep a stable order.
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| match &keys[i] {
        Some((show, season, episode)) => (first[show.as_str()], *season, *episode, i),
        None => (i, 0, 0, i),
    });
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

/// Two paths, component by component, each compared as a person reads it.
fn natural_path(a: &Path, b: &Path) -> Ordering {
    let (mut ac, mut bc) = (a.components(), b.components());
    loop {
        match (ac.next(), bc.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = natural(
                    &x.as_os_str().to_string_lossy(),
                    &y.as_os_str().to_string_lossy(),
                );
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Digit runs as numbers, everything else without regard to case.
fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (na, nb) = (digits(&mut a), digits(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let order = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn digits(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut run = String::new();
    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
        run.push(c);
        chars.next();
    }
    run
}

/// The other seasons of the show this file belongs to, each in a folder beside
/// this file's own, and which season each is — in season order.
///
/// **Blocking.** Worker thread only: it lists the folder above this one and
/// looks inside each sibling.
///
/// A folder is judged by the first video in it in name order, read the way
/// every other name in the player is read. Where two folders hold the same
/// season, the first in name order stands for it, so the answer is the same
/// on every run, whatever order the disk lists them in.
pub fn seasons_beside(current: &Path) -> Vec<(PathBuf, u32)> {
    let dbm_naming::Media::Episode {
        show,
        season: Some(season),
        ..
    } = dbm_naming::parse(&current.to_string_lossy())
    else {
        return Vec::new();
    };
    let Some(folder) = current.parent() else {
        return Vec::new();
    };
    let Some(Ok(dirs)) = folder.parent().map(fs::read_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = dirs
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|dir| dir != folder && dir.is_dir())
        .collect();
    dirs.sort_by(|a, b| natural_path(a, b));
    let mut found: Vec<(PathBuf, u32)> = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let videos: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| is_video_file(p) && p.is_file())
            .collect();
        let Some(first) = videos.iter().min_by(|a, b| natural_path(a, b)) else {
            continue;
        };
        let names: Vec<_> = videos.iter().map(|video| video.to_string_lossy()).collect();
        dbm_naming::notice(&names);
        if let dbm_naming::Media::Episode {
            show: other,
            season: Some(n),
            ..
        } = dbm_naming::parse(&first.to_string_lossy())
            && n != season
            && other.eq_ignore_ascii_case(&show)
            && !found.iter().any(|(_, m)| *m == n)
        {
            found.push((dir, n));
        }
    }
    found.sort_by_key(|(_, n)| *n);
    found
}

/// The folder holding the next season of the show this file belongs to, when
/// one sits beside this file's own folder — and which season it is.
///
/// **Blocking.** Worker thread only; see [`seasons_beside`].
///
/// Asked at the end of a season, where the useful offer is the next one. The
/// nearest later season wins, so a shelf holding seasons 8 and 9 offers 8.
pub fn next_season(current: &Path) -> Option<(PathBuf, u32)> {
    let dbm_naming::Media::Episode {
        season: Some(season),
        ..
    } = dbm_naming::parse(&current.to_string_lossy())
    else {
        return None;
    };
    seasons_beside(current)
        .into_iter()
        .find(|(_, n)| *n > season)
}

/// Past this many folders a scan stops and plays what it found: something
/// far wider than a library was opened, such as a home folder.
const MOST_FOLDERS: usize = 10_000;

/// Videos in `dir` and everything beneath it, in watching order, each at its
/// real path.
///
/// Iterative rather than recursive, so a deep tree can't blow the stack.
/// Each real folder is listed once, so a link back up the tree can't send
/// the walk round it again, and a link to a folder holding the opened one
/// (Wine's `z: -> /`) isn't followed at all. Unreadable subdirectories are
/// skipped, so one permission-denied folder doesn't stop a library loading.
fn scan_recursive(dir: &Path) -> Result<Vec<RealPath>, Error> {
    let real_dir = RealPath::of(dir).map_err(|source| Error::Unreadable {
        dir: dir.to_path_buf(),
        source,
    })?;
    let mut listed = HashSet::from([real_dir.clone()]);
    let mut stack = vec![(dir.to_path_buf(), real_dir.clone())];
    let mut videos = Vec::new();
    // Only the folder asked for is an error; a subfolder that cannot be read
    // is skipped, so one permission-denied corner doesn't stop a library.
    let mut unreadable_root = None;

    while let Some((seen, real)) = stack.pop() {
        let entries = match real.entries() {
            Ok(entries) => entries,
            Err(source) => {
                if seen.as_path() == dir {
                    unreadable_root = Some(source);
                }
                log::warn!("skipping unreadable {}", seen.display());
                continue;
            }
        };
        for found in entries.map(|entry| Found::of(entry, &seen)) {
            if !found.folder {
                if found.video() {
                    videos.push(found);
                }
            } else if real_dir.as_path().starts_with(found.real.as_path())
                || listed.contains(&found.real)
            {
                log::info!("not following {} round again", found.seen.display());
            } else if listed.len() < MOST_FOLDERS {
                listed.insert(found.real.clone());
                stack.push((found.seen, found.real));
            } else {
                log::warn!(
                    "{} holds over {MOST_FOLDERS} folders, stopped there",
                    dir.display()
                );
                stack.clear();
                break;
            }
        }
    }

    if let Some(source) = unreadable_root {
        return Err(Error::Unreadable {
            dir: dir.to_path_buf(),
            source,
        });
    }
    let videos = in_watching_order(videos);
    if videos.is_empty() {
        return Err(Error::NoVideosUnder(dir.to_path_buf()));
    }
    Ok(videos)
}

/// Write the list as an M3U for mpv to `loadlist`.
///
/// One command regardless of length, which mpv parses far faster than it
/// would service a `loadfile` per file.
fn write_m3u(videos: &[RealPath], dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut text = String::from("#EXTM3U\n");
    for video in videos {
        let _ = writeln!(text, "{}", video.as_path().display());
    }
    fs::write(dest, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installer offers to open these; the player has to mean it.
    ///
    /// Two lists in two languages that must agree, so this reads the
    /// installer and compares, and a format added to only one is a failing
    /// test.
    #[test]
    fn the_installer_registers_exactly_what_the_player_opens() {
        let nsi = include_str!("../../../packaging/death-by-mpv.nsi");
        let registered: Vec<&str> = nsi
            .lines()
            .filter_map(|line| line.trim().strip_prefix("!insertmacro ${MACRO} \""))
            .filter_map(|rest| rest.split('"').next())
            .collect();
        assert_eq!(registered, VIDEO_EXTENSIONS);
    }

    /// The same promise on Linux, where a desktop entry claims MIME types
    /// rather than extensions. The table is the one place the two meet: every
    /// extension the player opens must be mapped, and the entry must claim
    /// exactly the types that mapping produces.
    #[test]
    fn the_desktop_entry_claims_exactly_what_the_player_opens() {
        const TYPES: &[(&str, &[&str])] = &[
            ("mp4", &["video/mp4"]),
            ("mkv", &["video/x-matroska"]),
            ("avi", &["video/x-msvideo", "video/vnd.avi"]),
            ("mov", &["video/quicktime"]),
            ("wmv", &["video/x-ms-wmv"]),
            ("flv", &["video/x-flv"]),
            ("webm", &["video/webm"]),
            ("m4v", &["video/x-m4v"]),
            ("mpg", &["video/mpeg"]),
            ("mpeg", &["video/mpeg"]),
            ("ts", &["video/mp2t"]),
            ("m2ts", &["video/mp2t"]),
        ];
        let mapped: Vec<&str> = TYPES.iter().map(|(ext, _)| *ext).collect();
        assert_eq!(mapped, VIDEO_EXTENSIONS);

        let entry = include_str!("../../../packaging/flatpak/io.github.MisterD0ctor.DBM.desktop");
        let claimed: std::collections::BTreeSet<&str> = entry
            .lines()
            .find_map(|line| line.strip_prefix("MimeType="))
            .expect("desktop entry has a MimeType line")
            .split(';')
            .filter(|t| !t.is_empty())
            .collect();
        let expected: std::collections::BTreeSet<&str> = TYPES
            .iter()
            .flat_map(|(_, types)| types.iter().copied())
            .collect();
        assert_eq!(claimed, expected);
    }

    #[cfg(unix)]
    fn plain_paths(videos: Vec<RealPath>) -> Vec<PathBuf> {
        videos.iter().map(|v| v.as_path().to_path_buf()).collect()
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn a_path_with_nothing_at_it_is_a_typed_failure() {
        let missing =
            std::env::temp_dir().join(format!("dbm-playlist-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&missing);
        let Err(error) = resolve(&missing) else {
            panic!("a missing path resolves to a playlist");
        };
        assert!(matches!(error, Error::NothingAt(_)), "{error:?}");
    }

    #[cfg(unix)]
    #[test]
    fn an_empty_folder_says_whether_it_was_the_folder_or_the_tree() {
        let root = folder("empty", &[]);
        let error = scan_flat(&root).unwrap_err();
        assert!(matches!(error, Error::NoVideosIn(_)), "{error:?}");
        let Err(error) = resolve(&root) else {
            panic!("an empty folder resolves to a playlist");
        };
        assert!(matches!(error, Error::NoVideosUnder(_)), "{error:?}");
        fs::remove_dir_all(&root).unwrap();
    }

    /// A scratch folder holding `files`, empty, at the paths given.
    #[cfg(unix)]
    fn folder(name: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("dbm-playlist-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for file in files {
            let file = root.join(file);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, b"").unwrap();
        }
        fs::canonicalize(root).unwrap()
    }

    #[cfg(unix)]
    fn link(target: impl AsRef<Path>, at: impl AsRef<Path>) {
        std::os::unix::fs::symlink(target, at).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_file_opens_the_folder_the_link_is_in_at_its_real_path() {
        let root = folder(
            "linked-file",
            &["Season 1/b.mkv", "Here/a.mkv", "Here/c.mkv"],
        );
        link(root.join("Season 1/b.mkv"), root.join("Here/b.mkv"));

        let chosen = resolve(&root.join("Here/b.mkv")).unwrap();
        assert_eq!(
            plain_paths(chosen.videos),
            [
                root.join("Here/a.mkv"),
                root.join("Season 1/b.mkv"),
                root.join("Here/c.mkv")
            ]
        );
        assert_eq!(chosen.start, 1);
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_in_a_linked_folder_is_filed_under_its_real_path() {
        let root = folder("linked-folder", &["Season 1/a.mkv"]);
        link(root.join("Season 1"), root.join("Shows"));

        let chosen = resolve(&root.join("Shows/a.mkv")).unwrap();
        assert_eq!(plain_paths(chosen.videos), [root.join("Season 1/a.mkv")]);
        assert!(resolve(&root.join("Shows/b.mkv")).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_loop_gives_each_file_once_and_ends() {
        let root = folder("loop", &["Show/a.mkv", "Show/b.mkv"]);
        link("..", root.join("Show/back"));
        link(".", root.join("Show/here"));
        // Wine's `z: -> /`: the opened folder and everything around it.
        link(&root, root.join("Show/z:"));

        let chosen = resolve(&root.join("Show")).unwrap();
        assert_eq!(
            plain_paths(chosen.videos),
            [root.join("Show/a.mkv"), root.join("Show/b.mkv")]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn two_links_to_one_file_are_one_entry() {
        let root = folder("twice", &["a.mkv"]);
        link(root.join("a.mkv"), root.join("b.mkv"));
        link("a.mkv", root.join("c.mkv"));
        // A dangling link is left out.
        link("gone.mkv", root.join("d.mkv"));

        assert_eq!(plain_paths(scan_flat(&root).unwrap()), [root.join("a.mkv")]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_is_followed() {
        let root = folder("followed", &["Opened/a.mkv", "Elsewhere/b.mkv"]);
        link(root.join("Elsewhere"), root.join("Opened/More"));

        let chosen = resolve(&root.join("Opened")).unwrap();
        assert_eq!(
            plain_paths(chosen.videos),
            [root.join("Opened/a.mkv"), root.join("Elsewhere/b.mkv")]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_not_taken_for_a_video_plays_alone() {
        let root = folder("alone", &["clip.vob", "notes.txt"]);
        let chosen = resolve(&root.join("clip.vob")).unwrap();
        assert_eq!(plain_paths(chosen.videos), [root.join("clip.vob")]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn seasons_beside_are_judged_the_same_on_every_run() {
        let root = folder(
            "beside",
            &[
                "Show S01/Show S01E01.mkv",
                "Show S02 b/Show S02E01.mkv",
                "Show S02 a/Show S02E01.mkv",
                // Named first, so it judges its folder whatever the disk lists first.
                "Show S03/Show S03E01.mkv",
                "Show S03/Show S04E01.mkv",
            ],
        );
        let beside = seasons_beside(&root.join("Show S01/Show S01E01.mkv"));
        assert_eq!(
            beside,
            [(root.join("Show S02 a"), 2), (root.join("Show S03"), 3)]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_numbered_show_is_kept_together_once_listed() {
        // Name order puts the other file between the two release groups'.
        let root = folder(
            "numbered",
            &[
                "[A] Show 01 One.mkv",
                "[B] Show 02 Two.mkv",
                "[A] Show 03 Three.mkv",
                "[A] Zebra.mkv",
                "[B] Show 04 Four.mkv",
            ],
        );
        let videos = plain_paths(scan_flat(&root).unwrap());
        assert_eq!(
            videos,
            [
                root.join("[A] Show 01 One.mkv"),
                root.join("[B] Show 02 Two.mkv"),
                root.join("[A] Show 03 Three.mkv"),
                root.join("[B] Show 04 Four.mkv"),
                root.join("[A] Zebra.mkv"),
            ]
        );
        assert!(matches!(
            dbm_naming::parse(&videos[1].to_string_lossy()),
            dbm_naming::Media::Episode { episode: 2, .. }
        ));
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn season_folders_of_numbered_files_are_seasons_beside() {
        let episodes = ["01.mkv", "02.mkv", "03.mkv", "04.mkv"];
        let files: Vec<String> = ["Season 1", "Season 2"]
            .iter()
            .flat_map(|season| episodes.map(|episode| format!("Show/{season}/{episode}")))
            .collect();
        let root = folder(
            "numbered-seasons",
            &files.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let first = scan_flat(&root.join("Show/Season 1")).unwrap()[0].clone();
        assert_eq!(
            seasons_beside(first.as_path()),
            [(root.join("Show/Season 2"), 2)]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn numbers_sort_as_numbers_not_as_text() {
        let sorted = watching_order(vec![
            PathBuf::from("[Group] Show - 100.mkv"),
            PathBuf::from("[Group] Show - 11.mkv"),
            PathBuf::from("[Group] Show - 9.mkv"),
        ]);
        assert_eq!(
            names(&sorted),
            [
                "[Group] Show - 9.mkv",
                "[Group] Show - 11.mkv",
                "[Group] Show - 100.mkv"
            ]
        );
    }

    #[test]
    fn a_season_folder_sorts_by_its_season_and_episode() {
        let sorted = watching_order(vec![
            PathBuf::from("Show/Season 10/Show S10E01.mkv"),
            PathBuf::from("Show/Season 2/Show S02E02.mkv"),
            PathBuf::from("Show/Season 2/show s02e01.mkv"),
            PathBuf::from("Show/Season 1/Show S01E01.mkv"),
        ]);
        assert_eq!(
            names(&sorted),
            [
                "Show/Season 1/Show S01E01.mkv",
                "Show/Season 2/show s02e01.mkv",
                "Show/Season 2/Show S02E02.mkv",
                "Show/Season 10/Show S10E01.mkv",
            ]
        );
    }

    #[test]
    fn unrelated_films_fall_back_to_natural_names() {
        let sorted = watching_order(vec![
            PathBuf::from("b/Zodiac (2007).mkv"),
            PathBuf::from("B/alien (1979).mkv"),
            PathBuf::from("a/Film 10.mkv"),
            PathBuf::from("a/Film 2.mkv"),
        ]);
        assert_eq!(
            names(&sorted),
            [
                "a/Film 2.mkv",
                "a/Film 10.mkv",
                "B/alien (1979).mkv",
                "b/Zodiac (2007).mkv"
            ]
        );
    }

    #[test]
    fn a_show_among_other_things_is_kept_together() {
        // Name order puts Dandadan between the two Frieren episodes, because
        // their release groups differ.
        let sorted = watching_order(vec![
            PathBuf::from("[SubsPlease] Frieren - 01.mkv"),
            PathBuf::from("[SubsPlease] Dandadan - 01.mkv"),
            PathBuf::from("[Erai-raws] Frieren - 02.mkv"),
        ]);
        assert_eq!(
            names(&sorted),
            [
                "[SubsPlease] Frieren - 01.mkv",
                "[Erai-raws] Frieren - 02.mkv",
                "[SubsPlease] Dandadan - 01.mkv",
            ]
        );
    }
}
