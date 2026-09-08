//! App-level persistence: duration cache + watch-later position reads.
//!
//! The mpv wrapper writes its own watch-later files (via mpv's
//! `save-position-on-quit` and our periodic `write-watch-later-config`
//! calls). We layer two app-side bookkeeping pieces on top:
//!
//! 1. **Duration cache** — `durations.json` mapping `path -> seconds`. mpv
//!    knows the duration once it's loaded a file; we save it so the playlist
//!    menu can draw progress bars for files we haven't touched this session.
//! 2. **Watch-later position lookup** — `get_watch_later_positions(paths)`
//!    cross-references the md5-named watch-later files with the duration
//!    cache to build the `{ start, duration }` entries the frontend renders.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use md5::Digest;
use shared::{AmbientParams, WatchProgress};
use tauri::{AppHandle, Listener, Manager};

use crate::mpv::{app_data_dir, MpvPlayer};

// ---------------------------------------------------------------------------
// Duration cache
// ---------------------------------------------------------------------------

fn duration_cache_path() -> std::path::PathBuf {
    app_data_dir().join("durations.json")
}

pub fn load_duration_cache() -> HashMap<String, f64> {
    std::fs::read_to_string(duration_cache_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_duration(path: &str, duration: f64) {
    if duration <= 0.0 {
        return;
    }
    let mut cache = load_duration_cache();
    if cache.get(path).copied() == Some(duration) {
        return;
    }
    cache.insert(path.to_string(), duration);
    let target = duration_cache_path();
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::write(&target, json);
    }
}

// ---------------------------------------------------------------------------
// Watch-later positions
// ---------------------------------------------------------------------------

/// Look up per-file resume positions from mpv's watch-later files + our
/// duration cache. Files without both a saved start and a known duration
/// are skipped.
pub fn get_watch_later_positions(paths: Vec<String>) -> HashMap<String, WatchProgress> {
    let watch_dir = app_data_dir().join("watch_later");
    let durations = load_duration_cache();
    let mut out = HashMap::new();

    for path in &paths {
        let hash = md5::Md5::digest(path.as_bytes());
        let filename: String = hash.iter().map(|b| format!("{:02X}", b)).collect();
        let file_path = watch_dir.join(&filename);

        let Ok(contents) = std::fs::read_to_string(&file_path) else {
            continue;
        };
        for line in contents.lines() {
            let Some(val) = line.strip_prefix("start=") else {
                continue;
            };
            let Ok(start) = val.parse::<f64>() else {
                break;
            };
            let duration = durations.get(path).copied().unwrap_or(0.0);
            if duration > 0.0 {
                out.insert(path.clone(), WatchProgress { start, duration });
            }
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Last-session restore
// ---------------------------------------------------------------------------

fn last_session_path() -> std::path::PathBuf {
    app_data_dir().join("last_session.txt")
}

fn last_playlist_path() -> std::path::PathBuf {
    app_data_dir().join("last_playlist.json")
}

/// Save the currently-playing file path so the next launch can resume it.
pub fn save_last_session(path: &str) {
    let target = last_session_path();
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&target, path) {
        log::warn!("save_last_session: {e}");
    }
}

pub fn load_last_session() -> Option<String> {
    let s = std::fs::read_to_string(last_session_path()).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() || !std::path::PathBuf::from(&s).is_file() {
        return None;
    }
    Some(s)
}

/// Save the playlist used in this session so the next launch can restore it.
pub fn save_last_playlist(paths: &[std::path::PathBuf]) {
    let target = last_playlist_path();
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let strings: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    match serde_json::to_string(&strings) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&target, json) {
                log::warn!("save_last_playlist: {e}");
            }
        }
        Err(e) => log::warn!("save_last_playlist serialize: {e}"),
    }
}

/// Read the previously saved playlist, filtering out files that no longer
/// exist on disk. Returns `None` if the file's missing or every entry is.
pub fn load_last_playlist() -> Option<Vec<std::path::PathBuf>> {
    let json = std::fs::read_to_string(last_playlist_path()).ok()?;
    let strings: Vec<String> = serde_json::from_str(&json).ok()?;
    let paths: Vec<std::path::PathBuf> = strings
        .into_iter()
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_file())
        .collect();
    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

// ---------------------------------------------------------------------------
// Subtitle appearance
// ---------------------------------------------------------------------------

/// Size and placement are preferences about *this display*, not about a
/// particular file, so unlike `sub-delay` (which rides along in mpv's
/// watch-later data) they're stored once and reapplied at startup.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SubtitlePrefs {
    pub scale: f64,
    pub pos: f64,
}

impl Default for SubtitlePrefs {
    fn default() -> Self {
        // mpv's own defaults.
        Self {
            scale: 1.0,
            pos: 100.0,
        }
    }
}

fn subtitle_prefs_path() -> std::path::PathBuf {
    app_data_dir().join("subtitle-prefs.json")
}

pub fn load_subtitle_prefs() -> Option<SubtitlePrefs> {
    let json = std::fs::read_to_string(subtitle_prefs_path()).ok()?;
    serde_json::from_str(&json).ok()
}

pub fn save_subtitle_prefs(prefs: &SubtitlePrefs) {
    let target = subtitle_prefs_path();
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string(prefs) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&target, json) {
                log::warn!("save_subtitle_prefs: {e}");
            }
        }
        Err(e) => log::warn!("save_subtitle_prefs serialize: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Preferred subtitle language
// ---------------------------------------------------------------------------

fn sub_language_path() -> std::path::PathBuf {
    app_data_dir().join("sub_language.txt")
}

/// Remember the language of a subtitle track the user picked, so the next
/// file can be given the same language rather than whatever `sid` happens
/// to hold. Only ever written for an explicit choice — see the caller.
pub fn save_sub_language(lang: &str) {
    let lang = lang.trim();
    if lang.is_empty() {
        return;
    }
    let target = sub_language_path();
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&target, lang) {
        log::warn!("save_sub_language: {e}");
    }
}

pub fn load_sub_language() -> Option<String> {
    let lang = std::fs::read_to_string(sub_language_path()).ok()?;
    let lang = lang.trim().to_string();
    if lang.is_empty() {
        None
    } else {
        Some(lang)
    }
}

// ---------------------------------------------------------------------------
// Ambient shader params
// ---------------------------------------------------------------------------

fn ambient_params_path() -> std::path::PathBuf {
    app_data_dir().join("ambient-params.json")
}

pub fn save_ambient_params(params: &AmbientParams) -> Result<(), String> {
    let target = ambient_params_path();
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(params).map_err(|e| e.to_string())?;
    std::fs::write(&target, json).map_err(|e| e.to_string())
}

pub fn load_ambient_params() -> Option<AmbientParams> {
    let json = std::fs::read_to_string(ambient_params_path()).ok()?;
    serde_json::from_str(&json).ok()
}

// ---------------------------------------------------------------------------
// Side-effect listeners
// ---------------------------------------------------------------------------

/// Subscribe to `mpv://property` on the Rust side for two app-side
/// concerns:
/// - save the duration to the cache so the playlist menu can draw progress
///   bars before the file is opened next time
/// - kick off preview-sprite generation when the loaded path changes
pub fn install_property_listener(app: &AppHandle) {
    let app = app.clone();
    app.clone().listen("mpv://property", move |event| {
        let prop: shared::MpvProperty = match serde_json::from_str(event.payload()) {
            Ok(p) => p,
            Err(_) => return,
        };
        match prop {
            shared::MpvProperty::Duration(Some(duration)) if duration > 0.0 => {
                let player = app.state::<Arc<MpvPlayer>>();
                if let Ok(val) = player.get_property("path", "string") {
                    if let Some(path) = val.as_str() {
                        save_duration(path, duration);
                    }
                }
            }
            shared::MpvProperty::Path(Some(path)) if !path.is_empty() => {
                save_last_session(&path);
                crate::preview::request_preview(&app, std::path::Path::new(&path));
            }
            _ => {}
        }
    });
}

/// Periodically tell mpv to persist watch-later config for the currently
/// playing file. mpv's own `save-position-on-quit` covers clean shutdown;
/// this covers crashes.
pub fn spawn_watch_later_writer(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        let player = app.state::<Arc<MpvPlayer>>();
        if player.is_file_loaded() {
            let _ = player.write_watch_later();
        }
    });
}
