//! Installs `mpv://property`, `mpv://event`, and `open-file` listeners and
//! writes incoming updates straight into [`PlayerState`]. Call once on app
//! mount.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{MpvEvent, MpvProperty, PreviewReady};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::bridge::commands;
use crate::state::PlayerState;

#[wasm_bindgen]
extern "C" {
    /// Tauri's `event.listen` — returns Promise<UnlistenFn>. We discard the
    /// unlisten because the listeners live for the app's lifetime.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
}

pub async fn install_listeners(state: PlayerState) {
    install_property_listener(state).await;
    install_event_listener(state).await;
    install_open_file_listener().await;
    install_preview_ready_listener(state).await;
}

async fn install_property_listener(state: PlayerState) {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) else {
            return;
        };
        match serde_wasm_bindgen::from_value::<MpvProperty>(payload) {
            Ok(prop) => apply_property(state, prop),
            Err(e) => leptos::logging::warn!("MpvProperty parse failed: {:?}", e),
        }
    });
    listen(
        "mpv://property",
        closure.as_ref().unchecked_ref::<js_sys::Function>(),
    )
    .await;
    closure.forget();
}

async fn install_event_listener(state: PlayerState) {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) else {
            return;
        };
        if let Ok(ev) = serde_wasm_bindgen::from_value::<MpvEvent>(payload) {
            apply_event(state, ev);
        }
    });
    listen(
        "mpv://event",
        closure.as_ref().unchecked_ref::<js_sys::Function>(),
    )
    .await;
    closure.forget();
}

/// `tauri-plugin-single-instance` emits this when a second launch hands us
/// a file path; we just call `load_video`. The first launch handles the CLI
/// arg backend-side (see `startup` in lib.rs) since the frontend isn't
/// mounted yet at that point.
/// Sprite-ready notifications from the backend's ffmpeg pipeline. Caches
/// the result in `state.preview_sprite` for the seek tooltip.
async fn install_preview_ready_listener(state: PlayerState) {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) else {
            return;
        };
        let Ok(ready) = serde_wasm_bindgen::from_value::<PreviewReady>(payload) else {
            return;
        };
        // Only accept the sprite if it's for the file we're currently
        // showing. Late deliveries from a previous video are dropped.
        let cur = state.path.with(|p| p.clone());
        if cur.as_deref() == Some(ready.path.as_str()) {
            state.preview_sprite.set(Some(ready));
        }
    });
    listen(
        "preview://ready",
        closure.as_ref().unchecked_ref::<js_sys::Function>(),
    )
    .await;
    closure.forget();
}

async fn install_open_file_listener() {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) else {
            return;
        };
        if let Some(path) = payload.as_string() {
            spawn_local(async move {
                if let Err(e) = commands::load_video(path).await {
                    leptos::logging::warn!("open-file load failed: {e}");
                }
            });
        }
    });
    listen(
        "open-file",
        closure.as_ref().unchecked_ref::<js_sys::Function>(),
    )
    .await;
    closure.forget();
}

fn apply_property(state: PlayerState, prop: MpvProperty) {
    use MpvProperty::*;
    match prop {
        TimePos(v) => state.time_pos.set(v.unwrap_or(0.0)),
        Duration(v) => state.duration.set(v.unwrap_or(0.0)),
        PercentPos(v) => state.percent_pos.set(v.unwrap_or(0.0)),
        Filename(v) => state.filename.set(v),
        Path(v) => {
            // Reset the sprite immediately so the old video's tiles don't
            // bleed through; then ask the backend whether a cached sprite
            // exists for the new path. (The backend kicks off generation
            // independently — we only need cache lookup here.)
            state.preview_sprite.set(None);
            let p = v.clone();
            state.path.set(v);
            if let Some(path) = p {
                spawn_local(async move {
                    if let Ok(Some(ready)) = commands::get_preview(path.clone()).await {
                        // Re-check the current path to avoid a TOCTOU race
                        // with another file load that may have just landed.
                        let cur = state.path.get_untracked();
                        if cur.as_deref() == Some(path.as_str()) {
                            state.preview_sprite.set(Some(ready));
                        }
                    }
                });
            }
        }
        Pause(v) => state.paused.set(v),
        Mute(v) => state.muted.set(v),
        Volume(v) => state.volume.set(v),
        Panscan(v) => state.panscan.set(v),
        Sid(v) => state.sid.set(v),
        Aid(v) => state.aid.set(v),
        SubVisibility(v) => state.sub_visibility.set(v),
        BorderBackground(v) => state.border_background.set(v),
        EofReached(v) => state.eof_reached.set(v),
        PlaylistPos(v) => state.playlist_pos.set(v),
        PlaylistCount(v) => state.playlist_count.set(v),
        TrackListCount(_) => spawn_local(async move {
            if let Ok(t) = commands::tracks().await {
                state.tracks.set(t);
            }
        }),
    }
}

fn apply_event(state: PlayerState, ev: MpvEvent) {
    match ev {
        MpvEvent::FileLoaded => {
            // A new file means fresh tracks + a possibly-mutated playlist +
            // possibly-new watch-later positions to look up.
            spawn_local(async move {
                if let Ok(t) = commands::tracks().await {
                    state.tracks.set(t);
                }
                if let Ok(p) = commands::playlist().await {
                    let paths: Vec<String> = p.iter().map(|e| e.filename.clone()).collect();
                    state.playlist_count.set(p.len() as i64);
                    state.playlist.set(p);
                    if let Ok(progress) = commands::get_watch_later_positions(paths).await {
                        state.playlist_progress.set(progress);
                    }
                }
            });
        }
        MpvEvent::StartFile | MpvEvent::Seek | MpvEvent::EndFile { .. } => {}
    }
}
