//! Thin async wrappers around `window.__TAURI__.window.getCurrentWindow()`.
//! The window wears native OS decorations, so all we need from here is
//! fullscreen — Tauri's own `setFullscreen`, which hands the job to the
//! platform (taskbar hiding, restore bounds, alt-tab behaviour included).

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::state::PlayerState;

#[wasm_bindgen]
extern "C" {
    pub type WebviewWindow;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "window"], js_name = getCurrentWindow)]
    fn get_current_window_js() -> WebviewWindow;

    #[wasm_bindgen(method, catch, js_name = setFullscreen)]
    async fn set_fullscreen(this: &WebviewWindow, fullscreen: bool) -> Result<JsValue, JsValue>;
}

/// Single entry path into fullscreen: flip the UI flag and ask the OS.
pub async fn enter_fullscreen(state: PlayerState) {
    state.fullscreen.set(true);
    if let Err(e) = get_current_window_js().set_fullscreen(true).await {
        leptos::logging::warn!("setFullscreen(true) failed: {e:?}");
        state.fullscreen.set(false);
    }
}

/// Single exit path from fullscreen — the OS restores the window to its
/// pre-fullscreen position and size.
pub async fn exit_fullscreen(state: PlayerState) {
    state.fullscreen.set(false);
    if let Err(e) = get_current_window_js().set_fullscreen(false).await {
        leptos::logging::warn!("setFullscreen(false) failed: {e:?}");
    }
}
