use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::mpv::MpvPlayer;

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

fn video_files_in_directory(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut videos: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory: {}", e))?
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_file(p))
        .collect();

    if videos.is_empty() {
        return Err("No video files found in the directory".to_string());
    }

    videos.sort();
    Ok(videos)
}

fn video_files_recursive(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut videos = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Skipping directory {}: {}", current.display(), e);
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
        return Err("No video files found in the folder".to_string());
    }

    videos.sort();
    Ok(videos)
}

fn write_playlist(videos: &[PathBuf]) -> Result<PathBuf, String> {
    let playlist_path = crate::mpv::app_data_dir().join("playlist.m3u");
    let _ = fs::create_dir_all(playlist_path.parent().unwrap());
    let mut writer = BufWriter::new(fs::File::create(&playlist_path).map_err(|e| e.to_string())?);
    writeln!(writer, "#EXTM3U").map_err(|e| e.to_string())?;
    for video in videos {
        writeln!(writer, "{}", video.display()).map_err(|e| e.to_string())?;
    }
    Ok(playlist_path)
}

/// Build a playlist from the video's sibling files, load it into mpv, and start playback.
pub fn load_video(player: &MpvPlayer, video_path: &Path) -> Result<(), String> {
    let dir = video_path
        .parent()
        .ok_or("Failed to get parent directory")?;
    let videos = video_files_in_directory(dir)?;
    let index = videos.iter().position(|p| p == video_path).unwrap_or(0);
    load_playlist(player, &videos, index)
}

/// Recursively scan a folder and play all video files found.
pub fn load_folder(player: &MpvPlayer, folder_path: &Path) -> Result<(), String> {
    let videos = video_files_recursive(folder_path)?;
    load_playlist(player, &videos, 0)
}

/// Load a pre-built list of video paths into mpv at the given index.
pub fn load_playlist(
    player: &MpvPlayer,
    videos: &[PathBuf],
    index: usize,
) -> Result<(), String> {
    let playlist_path = write_playlist(videos)?;
    crate::mpv::save_last_playlist(videos);

    player
        .command(
            "loadlist",
            &[
                serde_json::Value::String(playlist_path.to_string_lossy().into_owned()),
                serde_json::Value::String("replace".into()),
            ],
        )
        .map_err(|e| e.to_string())?;

    player
        .command("playlist-play-index", &[serde_json::json!(index)])
        .map_err(|e| e.to_string())?;

    std::thread::sleep(std::time::Duration::from_millis(100));
    player
        .set_property_raw("pause", "no")
        .map_err(|e| e.to_string())?;

    Ok(())
}
