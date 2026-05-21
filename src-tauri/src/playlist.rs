//! Directory scanning + M3U-based playlist loading.
//!
//! Two entry points the rest of the app calls:
//! - [`load_video`]: takes one video file, scans its parent directory for
//!   sibling videos, builds a playlist, and starts on the chosen file.
//! - [`load_folder`]: takes a directory, recursively scans for video files,
//!   builds a playlist, and starts on the first.
//!
//! Both write a temp `playlist.m3u` to the app data dir and feed it to mpv
//! via `loadlist` — much cleaner than per-file `loadfile` round-trips.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::mpv::{app_data_dir, MpvPlayer};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
];

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| VIDEO_EXTENSIONS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
        .unwrap_or(false)
}

pub fn video_extensions() -> &'static [&'static str] {
    VIDEO_EXTENSIONS
}

/// Sorted list of videos directly in `dir` (no recursion).
fn video_files_in_directory(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut videos: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("read_dir: {e}"))?
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_file(p))
        .collect();
    if videos.is_empty() {
        return Err("no video files in directory".into());
    }
    videos.sort();
    Ok(videos)
}

/// Sorted list of videos in `dir` and all of its subdirectories.
fn video_files_recursive(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut videos = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Skipping {}: {e}", current.display());
                continue;
            }
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() && is_video_file(&path) {
                videos.push(path);
            }
        }
    }

    if videos.is_empty() {
        return Err("no video files in folder".into());
    }
    videos.sort();
    Ok(videos)
}

fn write_playlist(videos: &[PathBuf]) -> Result<PathBuf, String> {
    let path = app_data_dir().join("playlist.m3u");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = fs::File::create(&path).map_err(|e| format!("create m3u: {e}"))?;
    let mut writer = BufWriter::new(file);
    writeln!(writer, "#EXTM3U").map_err(|e| format!("write m3u: {e}"))?;
    for video in videos {
        writeln!(writer, "{}", video.display()).map_err(|e| format!("write m3u: {e}"))?;
    }
    Ok(path)
}

/// Scan the parent of `video_path` for sibling videos, then play the
/// chosen file inside that playlist.
pub fn load_video(player: &MpvPlayer, video_path: &Path) -> Result<(), String> {
    let dir = video_path.parent().ok_or("no parent directory")?;
    let videos = video_files_in_directory(dir)?;
    let index = videos.iter().position(|p| p == video_path).unwrap_or(0);
    load_playlist(player, &videos, index)
}

/// Recursively scan `folder_path` and play all video files found, starting
/// from the first.
pub fn load_folder(player: &MpvPlayer, folder_path: &Path) -> Result<(), String> {
    let videos = video_files_recursive(folder_path)?;
    load_playlist(player, &videos, 0)
}

/// Write the M3U + tell mpv to load it + jump to the chosen entry + unpause.
/// Also persists the playlist for next-launch restore.
pub fn load_playlist(player: &MpvPlayer, videos: &[PathBuf], index: usize) -> Result<(), String> {
    let playlist_path = write_playlist(videos)?;
    crate::persistence::save_last_playlist(videos);

    player
        .command(
            "loadlist",
            &[
                serde_json::Value::String(playlist_path.to_string_lossy().into_owned()),
                serde_json::Value::String("replace".into()),
            ],
        )
        .map_err(|e| format!("loadlist: {e}"))?;

    player
        .command("playlist-play-index", &[serde_json::json!(index)])
        .map_err(|e| format!("playlist-play-index: {e}"))?;

    // Give mpv a moment to populate the playlist + actually start the file
    // before unpausing — without this the unpause can fire while the player
    // is still in transition and silently get ignored.
    std::thread::sleep(std::time::Duration::from_millis(100));
    player
        .set_property_raw("pause", "no")
        .map_err(|e| format!("unpause: {e}"))
}
