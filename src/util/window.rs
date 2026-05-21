//! Thin async wrappers around `window.__TAURI__.window.getCurrentWindow()`.
//! Cover fullscreen + the custom-decorator controls (minimize, toggle
//! maximize, close, isMaximized + onResized). Everything else still goes
//! through our mpv command surface.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::state::PlayerState;

#[wasm_bindgen]
extern "C" {
    pub type WebviewWindow;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "window"], js_name = getCurrentWindow)]
    fn get_current_window_js() -> WebviewWindow;

    #[wasm_bindgen(method, catch, js_name = setFullscreen)]
    async fn set_fullscreen_js(this: &WebviewWindow, fullscreen: bool) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    async fn minimize(this: &WebviewWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = toggleMaximize)]
    async fn toggle_maximize(this: &WebviewWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    async fn maximize(this: &WebviewWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    async fn unmaximize(this: &WebviewWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    async fn close(this: &WebviewWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = isMaximized)]
    async fn is_maximized(this: &WebviewWindow) -> Result<JsValue, JsValue>;

    /// Subscribe to the window's `tauri://resize` event. The handler fires
    /// on every resize (including maximize/restore). Resolves to an unlisten
    /// fn that we discard — the listener lives for the window's lifetime.
    #[wasm_bindgen(method, catch, js_name = onResized)]
    async fn on_resized(
        this: &WebviewWindow,
        handler: &js_sys::Function,
    ) -> Result<JsValue, JsValue>;
}

pub async fn set_fullscreen(fullscreen: bool) {
    let win = get_current_window_js();
    if let Err(e) = win.set_fullscreen_js(fullscreen).await {
        leptos::logging::warn!("set_fullscreen failed: {e:?}");
    }
}

pub async fn minimize_window() {
    let win = get_current_window_js();
    if let Err(e) = win.minimize().await {
        leptos::logging::warn!("minimize failed: {e:?}");
    }
}

pub async fn toggle_maximize_window() {
    let win = get_current_window_js();
    if let Err(e) = win.toggle_maximize().await {
        leptos::logging::warn!("toggle_maximize failed: {e:?}");
    }
}

pub async fn maximize_window() {
    let win = get_current_window_js();
    if let Err(e) = win.maximize().await {
        leptos::logging::warn!("maximize failed: {e:?}");
    }
}

pub async fn unmaximize_window() {
    let win = get_current_window_js();
    if let Err(e) = win.unmaximize().await {
        leptos::logging::warn!("unmaximize failed: {e:?}");
    }
}

pub async fn close_window() {
    let win = get_current_window_js();
    if let Err(e) = win.close().await {
        leptos::logging::warn!("close failed: {e:?}");
    }
}

pub async fn check_maximized() -> bool {
    let win = get_current_window_js();
    win.is_maximized()
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Single entry path into fullscreen: snapshot the OS maximized state so we
/// know how to restore on exit, maximize the window if it isn't already, then
/// flip the fullscreen flag. Use this from every shortcut (button, keyboard,
/// double-click) so the maximize-first behavior is uniform.
pub async fn enter_fullscreen(state: PlayerState) {
    let was_max = check_maximized().await;
    state.pre_fullscreen_maximized.set(was_max);
    if !was_max {
        maximize_window().await;
    }
    state.fullscreen.set(true);
    set_fullscreen(true).await;
}

/// Single exit path from fullscreen: drop fullscreen, then if the window was
/// *not* maximized before we entered, unmaximize so we return to the original
/// windowed size. If it was already maximized on entry, leave it maximized.
pub async fn exit_fullscreen(state: PlayerState) {
    state.fullscreen.set(false);
    set_fullscreen(false).await;
    if !state.pre_fullscreen_maximized.get_untracked() {
        unmaximize_window().await;
    }
}

/// Install a `tauri://resize` listener that calls `on_change` every time
/// the window is resized (drag, maximize, restore, snap…). The closure
/// leaks for the window's lifetime — we never need to unsubscribe.
pub fn install_resize_listener<F>(mut on_change: F)
where
    F: FnMut() + 'static,
{
    let cb = Closure::<dyn FnMut(JsValue)>::new(move |_| on_change());
    let func: &js_sys::Function = cb.as_ref().unchecked_ref();
    let func = func.clone();
    spawn_local(async move {
        let win = get_current_window_js();
        if let Err(e) = win.on_resized(&func).await {
            leptos::logging::warn!("onResized listen failed: {e:?}");
        }
    });
    cb.forget();
}
