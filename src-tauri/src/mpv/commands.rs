//! Tauri-invokable commands. Each is a thin typed wrapper over `MpvPlayer`
//! that converts internal `MpvError` into `shared::MpvErrorDto` at the
//! boundary so the frontend can deserialize the result.

use std::collections::HashMap;
use std::sync::Arc;

use shared::{
    AmbientParams, MpvErrorDto, MpvErrorKind, PlayerSnapshot, PlaylistEntry, PreviewReady,
    SeekMode, SeekPrecision, Track, TrackSelection, WatchProgress,
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{to_dto, MpvPlayer};
use crate::{persistence, playlist, preview};

// ============================================================================
// Hydration
// ============================================================================

/// One-shot fetch the frontend calls on mount to populate its signals.
/// After this, live updates arrive via the `mpv://property` event stream.
#[tauri::command]
pub fn snapshot(player: State<Arc<MpvPlayer>>) -> Result<PlayerSnapshot, MpvErrorDto> {
    to_dto(player.snapshot())
}

#[tauri::command]
pub fn tracks(player: State<Arc<MpvPlayer>>) -> Result<Vec<Track>, MpvErrorDto> {
    to_dto(player.tracks())
}

#[tauri::command]
pub fn playlist(player: State<Arc<MpvPlayer>>) -> Result<Vec<PlaylistEntry>, MpvErrorDto> {
    to_dto(player.playlist())
}

// ============================================================================
// Playback
// ============================================================================

#[tauri::command]
pub fn play(player: State<Arc<MpvPlayer>>) -> Result<(), MpvErrorDto> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    to_dto(player.set_property_raw("pause", "no"))
}

#[tauri::command]
pub fn pause(player: State<Arc<MpvPlayer>>) -> Result<(), MpvErrorDto> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    to_dto(player.set_property_raw("pause", "yes"))
}

/// Typed pause setter — replaces the old generic `set_property("pause", ...)`
/// + the round-tripping `toggle_pause` from the JS version. Frontend flips
/// the bool locally and calls this.
#[tauri::command]
pub fn set_pause(player: State<Arc<MpvPlayer>>, paused: bool) -> Result<(), MpvErrorDto> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    to_dto(player.set_property_raw("pause", if paused { "yes" } else { "no" }))
}

#[tauri::command]
pub fn seek(
    player: State<Arc<MpvPlayer>>,
    target: f64,
    mode: Option<SeekMode>,
    precision: Option<SeekPrecision>,
) -> Result<(), MpvErrorDto> {
    if !player.is_file_loaded() {
        return Ok(());
    }
    let mode = mode.unwrap_or(SeekMode::Absolute);
    let precision = precision.unwrap_or(SeekPrecision::Keyframes);
    to_dto(player.command(
        "seek",
        &[
            target.into(),
            mode.as_mpv_str().into(),
            precision.as_mpv_str().into(),
        ],
    ))
}

// ============================================================================
// Audio
// ============================================================================

#[tauri::command]
pub fn set_mute(player: State<Arc<MpvPlayer>>, muted: bool) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_raw("mute", if muted { "yes" } else { "no" }))
}

#[tauri::command]
pub fn set_volume(player: State<Arc<MpvPlayer>>, volume: f64) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_value("volume", &serde_json::json!(volume)))
}

// ============================================================================
// Video
// ============================================================================

#[tauri::command]
pub fn set_panscan(player: State<Arc<MpvPlayer>>, value: f64) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_value("panscan", &serde_json::json!(value)))
}

// ============================================================================
// Tracks
// ============================================================================

#[tauri::command]
pub fn set_subtitle_track(
    player: State<Arc<MpvPlayer>>,
    selection: TrackSelection,
) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_value("sid", &track_selection_to_value(selection)))
}

#[tauri::command]
pub fn set_audio_track(
    player: State<Arc<MpvPlayer>>,
    selection: TrackSelection,
) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_value("aid", &track_selection_to_value(selection)))
}

#[tauri::command]
pub fn set_sub_visibility(
    player: State<Arc<MpvPlayer>>,
    visible: bool,
) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_raw("sub-visibility", if visible { "yes" } else { "no" }))
}

fn track_selection_to_value(sel: TrackSelection) -> serde_json::Value {
    match sel {
        TrackSelection::Id(id) => serde_json::Value::String(id.to_string()),
        TrackSelection::Special(shared::SpecialTrack::No) => serde_json::Value::String("no".into()),
        TrackSelection::Special(shared::SpecialTrack::Auto) => {
            serde_json::Value::String("auto".into())
        }
    }
}

// ============================================================================
// Ambient (border shader)
// ============================================================================

#[tauri::command]
pub fn set_ambient_enabled(
    player: State<Arc<MpvPlayer>>,
    enabled: bool,
) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_raw(
        "border-background",
        if enabled { "shader" } else { "color" },
    ))
}

/// Push the live shader options to mpv AND persist them to disk. Called on
/// every slider input — the disk write is small and the format is stable.
#[tauri::command]
pub fn apply_ambient_params(
    player: State<Arc<MpvPlayer>>,
    params: AmbientParams,
) -> Result<(), MpvErrorDto> {
    let opts = params
        .params
        .iter()
        .map(|p| format!("{}={}", p.name, p.value))
        .collect::<Vec<_>>()
        .join(",");
    to_dto(player.set_property_raw("border-background-shader-opts", &opts))?;
    if let Err(e) = persistence::save_ambient_params(&params) {
        log::warn!("save_ambient_params failed: {e}");
    }
    Ok(())
}

#[tauri::command]
pub fn load_ambient_params() -> Option<AmbientParams> {
    persistence::load_ambient_params()
}

// ============================================================================
// Playlist
// ============================================================================

/// Load a single video. Auto-builds a playlist from sibling video files in
/// the same directory and starts on the chosen one — matches typical media
/// player UX (click one episode, get the whole season).
#[tauri::command]
pub fn load_video(player: State<Arc<MpvPlayer>>, path: String) -> Result<(), MpvErrorDto> {
    let video = std::path::PathBuf::from(&path);
    if !video.is_file() || !playlist::is_video_file(&video) {
        return Err(MpvErrorDto {
            kind: MpvErrorKind::Other,
            message: "not a valid video file".into(),
        });
    }
    playlist::load_video(&player, &video).map_err(|e| MpvErrorDto {
        kind: MpvErrorKind::Other,
        message: e,
    })
}

/// Recursively scan `path` for video files and play them all in alphabetical
/// order, starting from the first.
#[tauri::command]
pub fn load_folder(player: State<Arc<MpvPlayer>>, path: String) -> Result<(), MpvErrorDto> {
    let folder = std::path::PathBuf::from(&path);
    if !folder.is_dir() {
        return Err(MpvErrorDto {
            kind: MpvErrorKind::Other,
            message: "not a folder".into(),
        });
    }
    playlist::load_folder(&player, &folder).map_err(|e| MpvErrorDto {
        kind: MpvErrorKind::Other,
        message: e,
    })
}

#[tauri::command]
pub fn get_watch_later_positions(paths: Vec<String>) -> HashMap<String, WatchProgress> {
    persistence::get_watch_later_positions(paths)
}

/// Show the Windows 11 "Snap Layouts" popover for the current window. The
/// only reliable way to trigger it from a custom title bar (without hooking
/// `WM_NCHITTEST` to return `HTMAXBUTTON` and letting DWM do the hover
/// magic itself) is to synthesize the Win+Z shortcut.
#[tauri::command]
pub fn show_snap_layouts() -> Result<(), MpvErrorDto> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LWIN,
        };
        const VK_Z: VIRTUAL_KEY = VIRTUAL_KEY(0x5A);
        let make = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [
            make(VK_LWIN, KEYBD_EVENT_FLAGS(0)),
            make(VK_Z, KEYBD_EVENT_FLAGS(0)),
            make(VK_Z, KEYEVENTF_KEYUP),
            make(VK_LWIN, KEYEVENTF_KEYUP),
        ];
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_preview(path: String) -> Option<PreviewReady> {
    preview::cached_preview(std::path::Path::new(&path))
}

/// Native file picker. The chosen file is loaded with sibling-scan playlist
/// behavior (see [`load_video`]).
#[tauri::command]
pub fn open_video_dialog(
    app: AppHandle,
    player: State<Arc<MpvPlayer>>,
) -> Result<(), MpvErrorDto> {
    let window = app.get_webview_window("main").ok_or_else(|| MpvErrorDto {
        kind: MpvErrorKind::Other,
        message: "window 'main' not found".into(),
    })?;

    let player = Arc::clone(&player);
    app.dialog()
        .file()
        .set_parent(&window)
        .add_filter("Video Files", playlist::video_extensions())
        .pick_file(move |picked| {
            let Some(file) = picked else {
                return;
            };
            let path = match file.into_path() {
                Ok(p) => p,
                Err(e) => {
                    log::error!("dialog: path resolve failed: {e}");
                    return;
                }
            };
            if !path.is_file() || !playlist::is_video_file(&path) {
                log::warn!("dialog: chosen entry is not a video file");
                return;
            }
            if let Err(e) = playlist::load_video(&player, &path) {
                log::error!("dialog: load_video failed: {e}");
            }
        });

    Ok(())
}

/// Native folder picker. Recursively scans the chosen folder; loads all
/// videos found.
#[tauri::command]
pub fn open_folder_dialog(
    app: AppHandle,
    player: State<Arc<MpvPlayer>>,
) -> Result<(), MpvErrorDto> {
    let window = app.get_webview_window("main").ok_or_else(|| MpvErrorDto {
        kind: MpvErrorKind::Other,
        message: "window 'main' not found".into(),
    })?;

    let player = Arc::clone(&player);
    app.dialog()
        .file()
        .set_parent(&window)
        .pick_folder(move |picked| {
            let Some(folder) = picked else {
                return;
            };
            let path = match folder.into_path() {
                Ok(p) => p,
                Err(e) => {
                    log::error!("folder dialog: path resolve failed: {e}");
                    return;
                }
            };
            if !path.is_dir() {
                log::warn!("folder dialog: chosen entry is not a folder");
                return;
            }
            if let Err(e) = playlist::load_folder(&player, &path) {
                log::error!("folder dialog: load_folder failed: {e}");
            }
        });

    Ok(())
}

#[tauri::command]
pub fn playlist_play_index(player: State<Arc<MpvPlayer>>, index: i64) -> Result<(), MpvErrorDto> {
    to_dto(player.command("playlist-play-index", &[index.into()]))
}

#[tauri::command]
pub fn playlist_prev(player: State<Arc<MpvPlayer>>) -> Result<(), MpvErrorDto> {
    to_dto(player.command("playlist-prev", &[]))
}

#[tauri::command]
pub fn playlist_next(player: State<Arc<MpvPlayer>>) -> Result<(), MpvErrorDto> {
    to_dto(player.command("playlist-next", &[]))
}

// ============================================================================
// Generic property escape hatches (prefer the typed setters above)
// ============================================================================

#[tauri::command]
pub fn set_property(
    player: State<Arc<MpvPlayer>>,
    name: String,
    value: serde_json::Value,
) -> Result<(), MpvErrorDto> {
    to_dto(player.set_property_value(&name, &value))
}

#[tauri::command]
pub fn get_property(
    player: State<Arc<MpvPlayer>>,
    name: String,
    format: Option<String>,
) -> Result<serde_json::Value, MpvErrorDto> {
    let fmt = format.unwrap_or_else(|| "string".to_string());
    to_dto(player.get_property(&name, &fmt))
}

/// Native file picker for sidecar subtitle files. Adds via mpv's `sub-add`
/// command and forces visibility on (mpv leaves it off in some cases).
#[tauri::command]
pub fn open_subtitle_dialog(
    app: AppHandle,
    player: State<Arc<MpvPlayer>>,
) -> Result<(), MpvErrorDto> {
    let window = app.get_webview_window("main").ok_or_else(|| MpvErrorDto {
        kind: MpvErrorKind::Other,
        message: "window 'main' not found".into(),
    })?;

    let player = Arc::clone(&player);
    app.dialog()
        .file()
        .set_parent(&window)
        .add_filter(
            "Subtitle Files",
            &["srt", "ass", "ssa", "vtt", "sub", "sup", "idx", "smi"],
        )
        .pick_file(move |picked| {
            let Some(file) = picked else {
                return;
            };
            let path = match file.into_path() {
                Ok(p) => p,
                Err(e) => {
                    log::error!("subtitle dialog: path resolve failed: {e}");
                    return;
                }
            };
            if !path.is_file() {
                return;
            }
            let path_str = path.to_string_lossy().to_string();
            if let Err(e) = player.command(
                "sub-add",
                &[
                    serde_json::Value::String(path_str),
                    serde_json::Value::String("select".into()),
                ],
            ) {
                log::error!("sub-add failed: {e}");
                return;
            }
            if let Err(e) = player.set_property_raw("sub-visibility", "yes") {
                log::warn!("sub-visibility set failed: {e}");
            }
        });

    Ok(())
}
