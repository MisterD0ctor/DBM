use serde::{Deserialize, Serialize};
use tauri::State;

use super::{MpvPlayer, MpvResult};

// ---------------------------------------------------------------------------
// Shared state type (returned from get_state)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpvState {
    pub paused: bool,
    pub time_pos: f64,
    pub percent_pos: f64,
    pub duration: f64,
    pub volume: f64,
    pub mute: bool,
    pub speed: f64,
    pub panscan: f64,
    pub filename: Option<String>,
    pub path: Option<String>,
    pub media_title: Option<String>,
    pub border_background: Option<String>,
}

// ---------------------------------------------------------------------------
// Playback commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn play(player: State<MpvPlayer>) -> MpvResult<()> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    player.set_property_raw("pause", "no")
}

#[tauri::command]
pub fn pause(player: State<MpvPlayer>) -> MpvResult<()> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    player.set_property_raw("pause", "yes")
}

#[tauri::command]
pub fn toggle_pause(player: State<MpvPlayer>) -> MpvResult<()> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    let paused = player.get_property("pause", "flag")?;
    let is_paused = paused.as_bool().unwrap_or(false);
    if is_paused {
        player.set_property_raw("pause", "no")
    } else {
        player.set_property_raw("pause", "yes")
    }
}

#[tauri::command]
pub fn seek(
    player: State<MpvPlayer>,
    target: f64,
    mode: Option<String>,
    precision: Option<String>,
) -> MpvResult<()> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    let mode = mode.unwrap_or_else(|| "absolute".to_string());
    let precision = precision.unwrap_or_else(|| "keyframes".to_string());
    player.command("seek", &[target.into(), mode.into(), precision.into()])
}

#[tauri::command]
pub fn set_volume(player: State<MpvPlayer>, volume: f64) -> MpvResult<()> {
    player.set_property_value("volume", &serde_json::json!(volume))
}

#[tauri::command]
pub fn set_speed(player: State<MpvPlayer>, speed: f64) -> MpvResult<()> {
    player.set_property_value("speed", &serde_json::json!(speed))
}

// ---------------------------------------------------------------------------
// Playlist navigation
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn playlist_play_index(player: State<MpvPlayer>, index: i64) -> MpvResult<()> {
    player.command("playlist-play-index", &[index.into()])
}

#[tauri::command]
pub fn playlist_prev(player: State<MpvPlayer>) -> MpvResult<()> {
    player.command("playlist-prev", &[])
}

#[tauri::command]
pub fn playlist_next(player: State<MpvPlayer>) -> MpvResult<()> {
    player.command("playlist-next", &[])
}

// ---------------------------------------------------------------------------
// Property access (generic escape hatches)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_property(
    player: State<MpvPlayer>,
    name: String,
    value: serde_json::Value,
) -> MpvResult<()> {
    player.set_property_value(&name, &value)
}

#[tauri::command]
pub fn get_property(
    player: State<MpvPlayer>,
    name: String,
    format: Option<String>,
) -> MpvResult<serde_json::Value> {
    let fmt = format.unwrap_or_else(|| "string".to_string());
    player.get_property(&name, &fmt)
}

// ---------------------------------------------------------------------------
// State snapshot (for initial sync)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_state(player: State<MpvPlayer>) -> MpvResult<MpvState> {
    let paused = player
        .get_property("pause", "flag")
        .map(|v| v.as_bool().unwrap_or(true))
        .unwrap_or(true);

    let time_pos = player
        .get_property("time-pos", "double")
        .map(|v| v.as_f64().unwrap_or(0.0))
        .unwrap_or(0.0);

    let duration = player
        .get_property("duration", "double")
        .map(|v| v.as_f64().unwrap_or(0.0))
        .unwrap_or(0.0);

    let percent_pos = player
        .get_property("percent-pos", "double")
        .map(|v| v.as_f64().unwrap_or(0.0))
        .unwrap_or(0.0);

    let volume = player
        .get_property("volume", "double")
        .map(|v| v.as_f64().unwrap_or(100.0))
        .unwrap_or(100.0);

    let mute = player
        .get_property("mute", "flag")
        .map(|v| v.as_bool().unwrap_or(false))
        .unwrap_or(false);

    let speed = player
        .get_property("speed", "double")
        .map(|v| v.as_f64().unwrap_or(1.0))
        .unwrap_or(1.0);

    let panscan = player
        .get_property("panscan", "double")
        .map(|v| v.as_f64().unwrap_or(0.0))
        .unwrap_or(0.0);

    let filename = player
        .get_property("filename", "string")
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let path = player
        .get_property("path", "string")
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let media_title = player
        .get_property("media-title", "string")
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let border_background = player
        .get_property("border-background", "string")
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    Ok(MpvState {
        paused,
        time_pos,
        percent_pos,
        duration,
        volume,
        mute,
        speed,
        panscan,
        filename,
        path,
        media_title,
        border_background,
    })
}
