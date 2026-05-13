use std::ffi::{c_char, c_void, CStr};

use log::error;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

// ---------------------------------------------------------------------------
// Callback userdata — stored for the lifetime of the mpv instance
// ---------------------------------------------------------------------------

pub struct EventUserData {
    pub app: AppHandle,
    pub free_fn: unsafe extern "C" fn(*mut c_char),
}

// ---------------------------------------------------------------------------
// Structured events emitted to the frontend
// ---------------------------------------------------------------------------

/// Emitted on every observed property change.
/// Event name: `mpv://property`
#[derive(Debug, Clone, Serialize)]
pub struct PropertyChangeEvent {
    pub name: String,
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// C callback — called by libmpv-wrapper for every mpv event
// ---------------------------------------------------------------------------

/// # Safety
/// Called from the mpv event thread via the C wrapper. `event` is a JSON
/// C-string that must be freed with `free_fn`; `userdata` points to a
/// valid `EventUserData`.
pub unsafe extern "C" fn event_callback(event: *const c_char, userdata: *mut c_void) {
    if event.is_null() || userdata.is_null() {
        return;
    }

    let ud = unsafe { &*(userdata as *const EventUserData) };

    let event_str = unsafe { CStr::from_ptr(event).to_string_lossy().to_string() };

    // Free the C string immediately
    unsafe { (ud.free_fn)(event as *mut c_char) };

    let app = ud.app.clone();

    tauri::async_runtime::spawn(async move {
        let parsed: serde_json::Value = match serde_json::from_str(&event_str) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse mpv event JSON: {}", e);
                return;
            }
        };

        // The wrapper emits events like:
        //   { "event": "property-change", "name": "pause", "data": true }
        //   { "event": "file-loaded" }

        let event_type = parsed.get("event").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "property-change" => {
                let name = parsed
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let data = parsed
                    .get("data")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                // Keep SMTC + watch-later/preview cache in sync with mpv state
                match name.as_str() {
                    "pause" => on_pause_change(&app, &data),
                    "filename" => on_filename_change(&app, &data),
                    "duration" => on_duration_change(&app, &data),
                    _ => {}
                }

                let payload = PropertyChangeEvent { name, data };
                if let Err(e) = app.emit("mpv://property", &payload) {
                    error!("Failed to emit mpv://property: {}", e);
                }
            }
            _ => {
                // Forward any other events as-is under a generic channel
                if let Err(e) = app.emit("mpv://event", &parsed) {
                    error!("Failed to emit mpv://event: {}", e);
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Per-property reactions — kept small and focused
// ---------------------------------------------------------------------------

fn on_pause_change(app: &AppHandle, data: &serde_json::Value) {
    let playing = data.as_bool().map(|b| !b).unwrap_or(false);
    crate::smtc::update_playback(app, playing);
}

/// When the loaded file changes: push title to SMTC, persist watch-later for
/// the previous file, save the new path as last-session, and kick off preview
/// sprite generation. Only fires for real filenames — during shutdown
/// `filename` goes null and the mpv instance is already torn down.
fn on_filename_change(app: &AppHandle, data: &serde_json::Value) {
    let Some(title) = data.as_str() else {
        return;
    };
    crate::smtc::update_metadata(app, title);

    let player = app.state::<std::sync::Arc<super::MpvPlayer>>();
    let _ = player.write_watch_later();
    let Ok(val) = player.get_property("path", "string") else {
        return;
    };
    let Some(path) = val.as_str() else {
        return;
    };
    super::save_last_session(path);
    crate::preview::request_preview(app, std::path::Path::new(path));
}

fn on_duration_change(app: &AppHandle, data: &serde_json::Value) {
    let Some(duration) = data.as_f64() else {
        return;
    };
    let player = app.state::<std::sync::Arc<super::MpvPlayer>>();
    let Ok(val) = player.get_property("path", "string") else {
        return;
    };
    if let Some(path) = val.as_str() {
        super::save_duration(path, duration);
    }
}
