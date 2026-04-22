//! Background thumbnail-sprite generation for the seek tooltip.
//!
//! On file load we spawn ffmpeg in a worker thread to extract a GRID×GRID tile
//! atlas of preview frames. When finished, an event `preview://ready` is
//! emitted so the frontend can attach the sprite to its seek tooltip.
//!
//! Sprites are cached in the app data dir under `previews/<md5>.jpg` and keyed
//! off the video's path + mtime so edits invalidate old sprites.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter, Manager};

pub const GRID: u32 = 12;
pub const TILE_W: u32 = 160;
pub const TILE_H: u32 = 90;
const FRAMES: u32 = GRID * GRID;

static ACTIVE_JOB: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

#[derive(serde::Serialize, Clone)]
pub struct PreviewReady {
    pub path: String,
    pub sprite: String,
    pub grid: u32,
    pub tile_w: u32,
    pub tile_h: u32,
}

fn previews_dir() -> PathBuf {
    crate::mpv::app_data_dir().join("previews")
}

fn sprite_path_for(video: &Path) -> PathBuf {
    use md5::Digest;
    let key = video.to_string_lossy();
    let digest = md5::Md5::digest(key.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    previews_dir().join(format!("{hex}.jpg"))
}

fn is_cache_fresh(video: &Path, sprite: &Path) -> bool {
    let Ok(v) = std::fs::metadata(video) else { return false };
    let Ok(s) = std::fs::metadata(sprite) else { return false };
    match (v.modified(), s.modified()) {
        (Ok(vm), Ok(sm)) => sm >= vm,
        _ => false,
    }
}

/// Look up an existing cached sprite for a video, if one is valid.
pub fn cached_preview(video: &Path) -> Option<PreviewReady> {
    let sprite = sprite_path_for(video);
    if is_cache_fresh(video, &sprite) {
        Some(PreviewReady {
            path: video.to_string_lossy().into_owned(),
            sprite: sprite.to_string_lossy().into_owned(),
            grid: GRID,
            tile_w: TILE_W,
            tile_h: TILE_H,
        })
    } else {
        None
    }
}

/// Resolve the ffmpeg sidecar binary. In a bundled app the sidecar sits next
/// to the executable; in dev builds tauri-build copies it into `target/*/`.
fn ffmpeg_path(app: &AppHandle) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidates = [
        exe_dir.join("ffmpeg-x86_64-pc-windows-msvc.exe"),
        exe_dir.join("ffmpeg.exe"),
        app.path()
            .resource_dir()
            .ok()
            .map(|d| d.join("ffmpeg-x86_64-pc-windows-msvc.exe"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn parse_duration_seconds(stderr: &str) -> Option<f64> {
    // ffmpeg prints e.g. "Duration: 01:23:45.67, start: ..."
    let key = "Duration:";
    let idx = stderr.find(key)?;
    let rest = &stderr[idx + key.len()..];
    let end = rest.find(',')?;
    let s = rest[..end].trim();
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 { return None; }
    let h: f64 = parts[0].parse().ok()?;
    let m: f64 = parts[1].parse().ok()?;
    let sec: f64 = parts[2].parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

#[cfg(windows)]
fn cmd_no_window(path: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(path);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

#[cfg(not(windows))]
fn cmd_no_window(path: &Path) -> Command {
    Command::new(path)
}

fn run_ffmpeg(ffmpeg: &Path, video: &Path, sprite: &Path) -> Result<(), String> {
    // 1. Probe duration.
    let probe = cmd_no_window(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(video)
        .output()
        .map_err(|e| format!("ffmpeg probe spawn: {e}"))?;
    let stderr = String::from_utf8_lossy(&probe.stderr);
    let duration = parse_duration_seconds(&stderr)
        .ok_or_else(|| format!("could not parse duration from ffmpeg output: {stderr}"))?;
    if duration <= 0.0 {
        return Err("video has zero duration".into());
    }

    // 2. Build the sprite. fps = FRAMES / duration gives exactly FRAMES sampled frames.
    let fps = FRAMES as f64 / duration;
    let vf = format!(
        "fps={fps:.6},scale={w}:{h}:force_original_aspect_ratio=decrease,\
         pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,tile={g}x{g}",
        w = TILE_W,
        h = TILE_H,
        g = GRID,
    );

    if let Some(parent) = sprite.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let status = cmd_no_window(ffmpeg)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(video)
        .arg("-vf")
        .arg(&vf)
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("5")
        .arg(sprite)
        .status()
        .map_err(|e| format!("ffmpeg extract spawn: {e}"))?;

    if !status.success() {
        return Err(format!("ffmpeg exited with {status}"));
    }
    Ok(())
}

/// Queue a preview-generation job for `video_path`. If a sprite is already
/// cached, we emit the ready event immediately and skip ffmpeg. Otherwise we
/// spawn a worker thread. Only one job runs at a time; newer requests
/// supersede older ones (the older job still finishes but its result is
/// ignored by the frontend because the event carries the file path).
pub fn request_preview(app: &AppHandle, video_path: &Path) {
    let path_str = video_path.to_string_lossy().into_owned();
    let sprite = sprite_path_for(video_path);

    if let Some(ready) = cached_preview(video_path) {
        let _ = app.emit("preview://ready", ready);
        return;
    }

    {
        let mut active = ACTIVE_JOB.lock().unwrap();
        if active.as_deref() == Some(path_str.as_str()) {
            return; // already running for this path
        }
        *active = Some(path_str.clone());
    }

    let Some(ffmpeg) = ffmpeg_path(app) else {
        log::warn!("ffmpeg sidecar not found, skipping preview generation");
        *ACTIVE_JOB.lock().unwrap() = None;
        return;
    };

    let app = app.clone();
    let video = video_path.to_path_buf();
    std::thread::spawn(move || {
        let result = run_ffmpeg(&ffmpeg, &video, &sprite);
        match result {
            Ok(()) => {
                let _ = app.emit(
                    "preview://ready",
                    PreviewReady {
                        path: path_str.clone(),
                        sprite: sprite.to_string_lossy().into_owned(),
                        grid: GRID,
                        tile_w: TILE_W,
                        tile_h: TILE_H,
                    },
                );
            }
            Err(e) => log::warn!("preview generation failed for {}: {}", video.display(), e),
        }
        let mut active = ACTIVE_JOB.lock().unwrap();
        if active.as_deref() == Some(path_str.as_str()) {
            *active = None;
        }
    });
}
