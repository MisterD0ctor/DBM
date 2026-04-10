use tauri::State;

use std::sync::Arc;

use super::{MpvPlayer, MpvResult};

// ---------------------------------------------------------------------------
// Playback commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn play(player: State<Arc<MpvPlayer>>) -> MpvResult<()> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    player.set_property_raw("pause", "no")
}

#[tauri::command]
pub fn pause(player: State<Arc<MpvPlayer>>) -> MpvResult<()> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    player.set_property_raw("pause", "yes")
}

#[tauri::command]
pub fn toggle_pause(player: State<Arc<MpvPlayer>>) -> MpvResult<()> {
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
    player: State<Arc<MpvPlayer>>,
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
pub fn set_volume(player: State<Arc<MpvPlayer>>, volume: f64) -> MpvResult<()> {
    player.set_property_value("volume", &serde_json::json!(volume))
}

#[tauri::command]
pub fn set_speed(player: State<Arc<MpvPlayer>>, speed: f64) -> MpvResult<()> {
    player.set_property_value("speed", &serde_json::json!(speed))
}

// ---------------------------------------------------------------------------
// Playlist navigation
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn playlist_play_index(player: State<Arc<MpvPlayer>>, index: i64) -> MpvResult<()> {
    player.command("playlist-play-index", &[index.into()])
}

#[tauri::command]
pub fn playlist_prev(player: State<Arc<MpvPlayer>>) -> MpvResult<()> {
    player.command("playlist-prev", &[])
}

#[tauri::command]
pub fn playlist_next(player: State<Arc<MpvPlayer>>) -> MpvResult<()> {
    player.command("playlist-next", &[])
}

// ---------------------------------------------------------------------------
// Property access (generic escape hatches)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_property(
    player: State<Arc<MpvPlayer>>,
    name: String,
    value: serde_json::Value,
) -> MpvResult<()> {
    player.set_property_value(&name, &value)
}

#[tauri::command]
pub fn get_property(
    player: State<Arc<MpvPlayer>>,
    name: String,
    format: Option<String>,
) -> MpvResult<serde_json::Value> {
    let fmt = format.unwrap_or_else(|| "string".to_string());
    player.get_property(&name, &fmt)
}
