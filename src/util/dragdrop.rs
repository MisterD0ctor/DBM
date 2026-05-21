//! Drag-and-drop handler. Listens to Tauri's webview-level drag/drop events
//! and loads the first dropped file via [`commands::load_video`].

use leptos::task::spawn_local;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::bridge::commands;

#[wasm_bindgen]
extern "C" {
    pub type Webview;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "webview"], js_name = getCurrentWebview)]
    fn get_current_webview() -> Webview;

    #[wasm_bindgen(method, js_name = onDragDropEvent)]
    async fn on_drag_drop_event_js(this: &Webview, handler: &js_sys::Function) -> JsValue;
}

pub async fn install_drag_drop() {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) else {
            return;
        };
        let event_type = js_sys::Reflect::get(&payload, &JsValue::from_str("type"))
            .ok()
            .and_then(|v| v.as_string());
        if event_type.as_deref() != Some("drop") {
            return;
        }
        let Ok(paths) = js_sys::Reflect::get(&payload, &JsValue::from_str("paths")) else {
            return;
        };
        let Ok(arr) = paths.dyn_into::<js_sys::Array>() else {
            return;
        };
        let Some(path) = arr.iter().next().and_then(|v| v.as_string()) else {
            return;
        };
        spawn_local(async move {
            if let Err(e) = commands::load_video(path).await {
                leptos::logging::warn!("drop load_video failed: {e}");
            }
        });
    });

    let webview = get_current_webview();
    let _ = webview
        .on_drag_drop_event_js(closure.as_ref().unchecked_ref::<js_sys::Function>())
        .await;
    closure.forget();
}
