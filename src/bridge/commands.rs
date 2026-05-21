//! Typed `invoke()` wrappers. One fn per Tauri command in
//! `src-tauri/src/mpv/commands.rs`. Each takes/returns shared types so the
//! call site never sees a `JsValue`.

#![allow(dead_code)] // bridge surface is exhaustive; components grow into it

use std::collections::HashMap;

use serde::{de::DeserializeOwned, Serialize};
use shared::{
    AmbientParams, MpvErrorDto, MpvErrorKind, PlayerSnapshot, PlaylistEntry, PreviewReady,
    SeekMode, SeekPrecision, Track, TrackSelection, WatchProgress,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `catch` makes the Tauri invoke promise's reject side observable as
    /// `Err(JsValue)` instead of throwing.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Generic typed-`invoke` helper. Args serialize via `serde_wasm_bindgen`,
/// the return JsValue deserializes into `R`, and a rejected promise is
/// parsed back into `MpvErrorDto`.
async fn call<A, R>(cmd: &str, args: A) -> Result<R, MpvErrorDto>
where
    A: Serialize,
    R: DeserializeOwned,
{
    let args_js = serde_wasm_bindgen::to_value(&args).map_err(|e| MpvErrorDto {
        kind: MpvErrorKind::Other,
        message: format!("arg serialize: {e}"),
    })?;

    match invoke(cmd, args_js).await {
        Ok(v) => serde_wasm_bindgen::from_value(v).map_err(|e| MpvErrorDto {
            kind: MpvErrorKind::Other,
            message: format!("response deserialize: {e}"),
        }),
        Err(e) => Err(
            serde_wasm_bindgen::from_value::<MpvErrorDto>(e).unwrap_or_else(|_| MpvErrorDto {
                kind: MpvErrorKind::Other,
                message: format!("invoke({cmd}) rejected"),
            }),
        ),
    }
}

// ============================================================================
// Hydration
// ============================================================================

pub async fn snapshot() -> Result<PlayerSnapshot, MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("snapshot", Args {}).await
}

/// Synthesizes Win+Z to pop Windows 11's Snap Layouts overlay for the
/// focused window. No-op on non-Windows platforms.
pub async fn show_snap_layouts() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("show_snap_layouts", Args {}).await
}

pub async fn tracks() -> Result<Vec<Track>, MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("tracks", Args {}).await
}

pub async fn playlist() -> Result<Vec<PlaylistEntry>, MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("playlist", Args {}).await
}

// ============================================================================
// Playback
// ============================================================================

pub async fn play() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("play", Args {}).await
}

pub async fn pause() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("pause", Args {}).await
}

pub async fn set_pause(paused: bool) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        paused: bool,
    }
    call("set_pause", Args { paused }).await
}

pub async fn seek(
    target: f64,
    mode: SeekMode,
    precision: SeekPrecision,
) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        target: f64,
        mode: SeekMode,
        precision: SeekPrecision,
    }
    call(
        "seek",
        Args {
            target,
            mode,
            precision,
        },
    )
    .await
}

// ============================================================================
// Audio
// ============================================================================

pub async fn set_mute(muted: bool) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        muted: bool,
    }
    call("set_mute", Args { muted }).await
}

pub async fn set_volume(volume: f64) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        volume: f64,
    }
    call("set_volume", Args { volume }).await
}

// ============================================================================
// Video
// ============================================================================

pub async fn set_panscan(value: f64) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        value: f64,
    }
    call("set_panscan", Args { value }).await
}

// ============================================================================
// Tracks
// ============================================================================

pub async fn set_subtitle_track(selection: TrackSelection) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        selection: TrackSelection,
    }
    call("set_subtitle_track", Args { selection }).await
}

pub async fn set_audio_track(selection: TrackSelection) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        selection: TrackSelection,
    }
    call("set_audio_track", Args { selection }).await
}

pub async fn set_sub_visibility(visible: bool) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        visible: bool,
    }
    call("set_sub_visibility", Args { visible }).await
}

// ============================================================================
// Playlist
// ============================================================================

pub async fn load_video(path: String) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        path: String,
    }
    call("load_video", Args { path }).await
}

pub async fn open_video_dialog() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("open_video_dialog", Args {}).await
}

pub async fn open_folder_dialog() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("open_folder_dialog", Args {}).await
}

pub async fn load_folder(path: String) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        path: String,
    }
    call("load_folder", Args { path }).await
}

pub async fn open_subtitle_dialog() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("open_subtitle_dialog", Args {}).await
}

pub async fn get_watch_later_positions(
    paths: Vec<String>,
) -> Result<HashMap<String, WatchProgress>, MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        paths: Vec<String>,
    }
    call("get_watch_later_positions", Args { paths }).await
}

pub async fn get_preview(path: String) -> Result<Option<PreviewReady>, MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        path: String,
    }
    call("get_preview", Args { path }).await
}

#[wasm_bindgen]
extern "C" {
    /// Tauri's helper that turns an absolute filesystem path into an
    /// `asset://` URL the webview can load. Required because we want the
    /// `<img src="...">` tag to read the sprite directly off disk.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = convertFileSrc)]
    pub fn convert_file_src(path: &str) -> String;
}

pub async fn set_ambient_enabled(enabled: bool) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        enabled: bool,
    }
    call("set_ambient_enabled", Args { enabled }).await
}

pub async fn apply_ambient_params(params: AmbientParams) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        params: AmbientParams,
    }
    call("apply_ambient_params", Args { params }).await
}

pub async fn load_ambient_params() -> Result<Option<AmbientParams>, MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("load_ambient_params", Args {}).await
}

pub async fn playlist_play_index(index: i64) -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {
        index: i64,
    }
    call("playlist_play_index", Args { index }).await
}

pub async fn playlist_prev() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("playlist_prev", Args {}).await
}

pub async fn playlist_next() -> Result<(), MpvErrorDto> {
    #[derive(Serialize)]
    struct Args {}
    call("playlist_next", Args {}).await
}
