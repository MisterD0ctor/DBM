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

/// Emitted on mpv errors.
/// Event name: `mpv://error`
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct MpvErrorEvent {
    pub message: String,
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

                // Keep SMTC in sync with mpv state
                match name.as_str() {
                    "pause" => {
                        let playing = data.as_bool().map(|b| !b).unwrap_or(false);
                        crate::smtc::update_playback(&app, playing);
                    }
                    "filename" => {
                        if let Some(title) = data.as_str() {
                            crate::smtc::update_metadata(&app, title);

                            // Save watch-later for the previous file, then persist new path.
                            // Only when switching TO a file — during shutdown filename
                            // goes null and the instance is already destroyed.
                            let player = app.state::<std::sync::Arc<super::MpvPlayer>>();
                            let _ = player.write_watch_later();
                            if let Ok(val) = player.get_property("path", "string") {
                                if let Some(path) = val.as_str() {
                                    super::save_last_session(path);
                                    crate::preview::request_preview(
                                        &app,
                                        std::path::Path::new(path),
                                    );
                                }
                            }
                        }
                    }
                    "duration" => {
                        if let Some(duration) = data.as_f64() {
                            let player = app.state::<std::sync::Arc<super::MpvPlayer>>();
                            if let Ok(val) = player.get_property("path", "string") {
                                if let Some(path) = val.as_str() {
                                    super::save_duration(path, duration);
                                }
                            }
                        }
                    }
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
