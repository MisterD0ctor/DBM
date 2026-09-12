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
        return Err(format!("There is nothing at {}", path.display()));
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
fn scan_flat(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut videos: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_file(p))
        .collect();
    if videos.is_empty() {
        return Err(format!("No video files in {}", folder_name(dir)));
    }
    videos.sort();
    Ok(videos)
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
    videos.sort();
    Ok(videos)
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
}
