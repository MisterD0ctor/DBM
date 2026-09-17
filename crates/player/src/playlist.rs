//! Turning a path into something mpv can play.
//!
//! Opening a single file is rarely what someone means: they want that
//! episode, with the rest of the season queued behind it. So a file becomes
//! its directory's videos with the chosen one selected, and a directory
//! becomes everything beneath it.
//!
//! All of this is filesystem work — `read_dir` on a network share or a
//! directory of thousands can take seconds — so it runs on the worker thread
//! and never on the frame path. Nothing here touches mpv or the UI; it
//! produces a plain list, and the caller decides what to do with it.

use std::cmp::Ordering;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Extensions treated as video. Matched case-insensitively. Public so the
/// open dialog can offer the same set as a filter — one list, so a file the
/// dialog shows is never one the scanner then ignores.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "ts", "m2ts",
];

/// A folder's own name, for a message that has to fit on one line. The full
/// path goes to the log; a capsule in the middle of the picture gets the part
/// that identifies it to the person who opened it.
fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| VIDEO_EXTENSIONS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
}

/// Scan a path into a playlist and write it out for mpv's `loadlist`.
///
/// Runs on the worker: this is `read_dir` over a directory that may be huge
/// or on a network share. The command line and the open dialog both arrive
/// here, so there is one answer to what a path means.
pub fn prepare(path: &Path) -> Result<crate::worker::Playlist, String> {
    let selection = resolve(path)?;
    let m3u = crate::paths::playlist_m3u();
    write_m3u(&selection.videos, &m3u)?;
    Ok(crate::worker::Playlist {
        m3u,
        start: selection.start,
        count: selection.videos.len(),
    })
}

/// What to play, and where to start.
pub struct Selection {
    pub videos: Vec<PathBuf>,
    pub start: usize,
}

/// Build a playlist from whatever the user pointed at.
///
/// A directory is scanned recursively; a file gets its siblings, which is
/// what makes "next episode" work without asking for the folder.
pub fn resolve(path: &Path) -> Result<Selection, String> {
    if path.is_dir() {
        let videos = scan_recursive(path)?;
        return Ok(Selection { videos, start: 0 });
    }
    if !path.is_file() {
        return Err(format!("There is nothing at {}", folder_name(path)));
    }

    let dir = path.parent().ok_or("file has no parent directory")?;
    let videos = scan_flat(dir)?;
    // The file itself might not be recognised as video by our extension
    // list, in which case fall back to playing just it.
    match videos.iter().position(|p| p == path) {
        Some(start) => Ok(Selection { videos, start }),
        None => Ok(Selection {
            videos: vec![path.to_path_buf()],
            start: 0,
        }),
    }
}

/// Videos directly in `dir`, sorted. No recursion: the siblings of an
/// episode are its season, not the whole library.
pub fn scan_flat(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let videos: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("Could not read {} — {e}", folder_name(dir)))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_file(p))
        .collect();
    if videos.is_empty() {
        return Err(format!("No video files in {}", folder_name(dir)));
    }
    Ok(watching_order(videos))
}

/// Put videos in the order a person watches them.
///
/// Not the order their bytes sort in. `Show - 100` sorted before `Show - 11`,
/// a recursive scan put `Season 10` between `Season 1` and `Season 2`, and an
/// upper-case release group jumped the queue — and autoplay, Next, the dimmed
/// ends of the bar and the end-of-season pill all trust this order.
///
/// A show's files are kept together, where the first of them falls in name
/// order, and go among themselves by the numbering their names carry: season,
/// then episode. Everything else is compared the way Explorer compares names,
/// digit runs as numbers and case ignored. Two release groups' names used to
/// split one show around another, so autoplay played half of each in turn.
pub fn watching_order(mut videos: Vec<PathBuf>) -> Vec<PathBuf> {
    videos.sort_by(|a, b| natural_path(a, b));
    let keys: Vec<Option<(String, u32, u32)>> = videos
        .iter()
        .map(|p| match crate::naming::parse(&p.to_string_lossy()) {
            crate::naming::Media::Episode {
                show,
                season,
                episode,
                ..
            } => Some((show.to_lowercase(), season.unwrap_or(0), episode)),
            crate::naming::Media::Movie { .. } => None,
        })
        .collect();
    let mut first: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, key) in keys.iter().enumerate() {
        if let Some((show, ..)) = key {
            first.entry(show.as_str()).or_insert(i);
        }
    }
    // Name order breaks every tie, so equal numbers keep a stable order.
    let mut order: Vec<usize> = (0..videos.len()).collect();
    order.sort_by_key(|&i| match &keys[i] {
        Some((show, season, episode)) => (first[show.as_str()], *season, *episode, i),
        None => (i, 0, 0, i),
    });
    let mut slots: Vec<Option<PathBuf>> = videos.into_iter().map(Some).collect();
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
    while let Some(c) = chars.peek().copied().filter(|c| c.is_ascii_digit()) {
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
/// A folder is judged by the first video in it, read the way every other name
/// in the player is read. Where two folders hold the same season, the first
/// found stands for it.
pub fn seasons_beside(current: &Path) -> Vec<(PathBuf, u32)> {
    let crate::naming::Media::Episode {
        show,
        season: Some(season),
        ..
    } = crate::naming::parse(&current.to_string_lossy())
    else {
        return Vec::new();
    };
    let Some(folder) = current.parent() else {
        return Vec::new();
    };
    let Some(Ok(dirs)) = folder.parent().map(fs::read_dir) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, u32)> = Vec::new();
    for dir in dirs.filter_map(Result::ok).map(|e| e.path()) {
        if !dir.is_dir() || dir == folder {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let first = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.is_file() && is_video_file(p));
        let Some(first) = first else {
            continue;
        };
        if let crate::naming::Media::Episode {
            show: other,
            season: Some(n),
            ..
        } = crate::naming::parse(&first.to_string_lossy())
        {
            if n != season
                && other.eq_ignore_ascii_case(&show)
                && !found.iter().any(|(_, m)| *m == n)
            {
                found.push((dir, n));
            }
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
    let crate::naming::Media::Episode {
        season: Some(season),
        ..
    } = crate::naming::parse(&current.to_string_lossy())
    else {
        return None;
    };
    seasons_beside(current)
        .into_iter()
        .find(|(_, n)| *n > season)
}

/// Videos in `dir` and everything beneath it, sorted.
///
/// Iterative rather than recursive so a deep or symlink-looped tree cannot
/// blow the stack, and unreadable subdirectories are skipped rather than
/// failing the whole scan — one permission-denied folder should not stop a
/// library from loading.
fn scan_recursive(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut videos = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            eprintln!("dbm: skipping unreadable {}", current.display());
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_video_file(&path) {
                videos.push(path);
            }
        }
    }

    if videos.is_empty() {
        return Err(format!("No video files under {}", folder_name(dir)));
    }
    Ok(watching_order(videos))
}

/// Write the list as an M3U for mpv to `loadlist`.
///
/// One command regardless of length. Appending a thousand files one
/// `loadfile` at a time would work, but this is a single round trip and mpv
/// parses it far faster than it would service a thousand commands.
pub fn write_m3u(videos: &[PathBuf], dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = fs::File::create(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    let mut out = BufWriter::new(file);
    writeln!(out, "#EXTM3U").map_err(|e| format!("write m3u: {e}"))?;
    for video in videos {
        writeln!(out, "{}", video.display()).map_err(|e| format!("write m3u: {e}"))?;
    }
    out.flush().map_err(|e| format!("flush m3u: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installer offers to open these; the player has to mean it.
    ///
    /// Two lists in two languages that must agree, so rather than generate
    /// one from the other this reads the installer and compares. Adding a
    /// format to the player and forgetting the installer is then a failing
    /// test rather than a file type that opens a player which refuses it.
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
        let expected: std::collections::BTreeSet<&str> =
            TYPES.iter().flat_map(|(_, types)| types.iter().copied()).collect();
        assert_eq!(claimed, expected);
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect()
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
            ["[Group] Show - 9.mkv", "[Group] Show - 11.mkv", "[Group] Show - 100.mkv"]
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
            ["a/Film 2.mkv", "a/Film 10.mkv", "B/alien (1979).mkv", "b/Zodiac (2007).mkv"]
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
