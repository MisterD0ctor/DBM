//! Translates raw mpv-wrapper event JSON into typed [`shared::MpvProperty`]
//! and [`shared::MpvEvent`] payloads, then forwards them to the frontend
//! on the `mpv://property` and `mpv://event` Tauri channels.

use std::ffi::{c_char, c_void, CStr};

use log::{error, trace};
use shared::{MpvEvent, MpvProperty};
use tauri::{AppHandle, Emitter};

/// Stored for the lifetime of the mpv instance and handed to the C callback
/// so it can re-enter Rust with both the app handle and the libmpv-wrapper
/// `free` fn pointer (used to free the event JSON the wrapper allocates).
pub struct EventUserData {
    pub app: AppHandle,
    pub free_fn: unsafe extern "C" fn(*mut c_char),
}

/// # Safety
/// Called from the mpv event thread. `event` is a JSON C-string owned by the
/// wrapper (we free it via `free_fn`); `userdata` points to a valid
/// `EventUserData` for the lifetime of the mpv instance.
pub unsafe extern "C" fn event_callback(event: *const c_char, userdata: *mut c_void) {
    if event.is_null() || userdata.is_null() {
        return;
    }

    let ud = unsafe { &*(userdata as *const EventUserData) };
    let event_str = unsafe { CStr::from_ptr(event).to_string_lossy().to_string() };

    // Free the wrapper's allocation immediately — we now own a Rust copy.
    unsafe { (ud.free_fn)(event as *mut c_char) };

    let app = ud.app.clone();

    // Defer parsing + emit off the mpv thread so we don't block events.
    tauri::async_runtime::spawn(async move {
        dispatch(&app, &event_str);
    });
}

fn dispatch(app: &AppHandle, event_str: &str) {
    let parsed: serde_json::Value = match serde_json::from_str(event_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse mpv event JSON: {}", e);
            return;
        }
    };

    let event_type = parsed
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match event_type.as_str() {
        "property-change" => emit_property(app, parsed),
        _ => emit_event(app, parsed),
    }
}

fn emit_property(app: &AppHandle, parsed: serde_json::Value) {
    // MpvProperty is `#[serde(tag = "name", content = "data")]`, so it
    // deserializes from `{ "name": "...", "data": ... }`. Strip the outer
    // `event` discriminator and feed the remainder in.
    let serde_json::Value::Object(mut map) = parsed else {
        return;
    };
    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    map.remove("event");

    // mpv reports inherently-integer counters (`playlist-pos`, the various
    // counts) as JSON floats because we observe them with format "double",
    // but the matching `MpvProperty` variants hold `i64` / `Option<i64>`.
    // Serde won't widen `5.0` into `5`, so without this coercion every
    // property-change event for these names silently fails to deserialize
    // and the UI sits on a stale value (e.g. the playlist active-row
    // highlight never moves when the user picks a different entry).
    if matches!(
        name.as_str(),
        "playlist-pos" | "playlist-count" | "track-list/count"
    ) {
        if let Some(data) = map.get_mut("data") {
            coerce_int(data);
        }
    }

    match serde_json::from_value::<MpvProperty>(serde_json::Value::Object(map)) {
        Ok(prop) => {
            if let Err(e) = app.emit("mpv://property", &prop) {
                error!("Failed to emit mpv://property ({name}): {e}");
            }
        }
        Err(e) => {
            // Unobserved or shape mismatch — trace, don't warn. The list of
            // observed properties is curated; anything here is either a
            // surprise from mpv (e.g. null in a non-Option variant) or an
            // entry that needs adding to the MpvProperty enum.
            trace!("Skipping property '{name}': {e}");
        }
    }
}

/// Replace a JSON float that's actually an integer (no fractional part) with
/// the integer form so `serde` can deserialize it into an `i64` slot. Leaves
/// nulls, non-numbers, and true floats untouched.
fn coerce_int(v: &mut serde_json::Value) {
    let serde_json::Value::Number(n) = v else {
        return;
    };
    let Some(f) = n.as_f64() else { return };
    if !f.is_finite() {
        return;
    }
    let i = f as i64;
    if (f - i as f64).abs() < 1e-9 {
        *v = serde_json::Value::Number(serde_json::Number::from(i));
    }
}

fn emit_event(app: &AppHandle, parsed: serde_json::Value) {
    // MpvEvent is `#[serde(tag = "event")]`, so the raw JSON deserializes
    // straight into a variant. Unknown events fall through silently.
    match serde_json::from_value::<MpvEvent>(parsed) {
        Ok(ev) => {
            if let Err(e) = app.emit("mpv://event", &ev) {
                error!("Failed to emit mpv://event: {e}");
            }
        }
        Err(_) => { /* unhandled event type — ignore */ }
    }
}
