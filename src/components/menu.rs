//! Popover menu primitive. One implementation replaces the four hand-rolled
//! click-outside-to-close blocks in the JS app (open / playlist / tracks /
//! ambient).
//!
//! Usage: parent owns an `RwSignal<bool>` for visibility and a `NodeRef` to
//! its toggle button. The menu installs a window-level mousedown listener
//! and closes itself on any click outside both the menu and the anchor.
//!
//! Example:
//! ```ignore
//! let open = RwSignal::new(false);
//! let btn = NodeRef::<html::Button>::new();
//! view! {
//!     <button node_ref=btn on:click=move |_| open.update(|v| *v = !*v)>
//!         "Open"
//!     </button>
//!     <Menu open anchor=btn>
//!         <MenuItem>"Hello"</MenuItem>
//!     </Menu>
//! }
//! ```

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

#[component]
pub fn Menu(
    /// Visibility — caller's responsibility to flip this on the anchor click.
    open: RwSignal<bool>,
    /// Anchor button. Clicks on it (or its descendants) are NOT treated as
    /// "outside" — so the anchor's own toggle handler is the single source
    /// of truth for opening / closing.
    #[prop(optional)]
    anchor: Option<NodeRef<html::Button>>,
    /// Extra CSS classes appended to `menu`.
    #[prop(into, optional)]
    class: String,
    children: Children,
) -> impl IntoView {
    let menu_ref = NodeRef::<html::Div>::new();

    let handle = window_event_listener(ev::mousedown, move |ev: ev::MouseEvent| {
        if !open.get_untracked() {
            return;
        }
        let Some(target) = ev.target() else {
            return;
        };
        let Ok(target_node) = target.dyn_into::<web_sys::Node>() else {
            return;
        };

        let in_menu = menu_ref
            .get_untracked()
            .map(|el| el.contains(Some(&target_node)))
            .unwrap_or(false);
        let in_anchor = anchor
            .and_then(|r| r.get_untracked())
            .map(|el| el.contains(Some(&target_node)))
            .unwrap_or(false);

        if !in_menu && !in_anchor {
            open.set(false);
        }
    });
    on_cleanup(move || handle.remove());

    let class = format!("menu {class}");
    view! {
        <div class=class class:hidden=move || !open.get() node_ref=menu_ref>
            {children()}
        </div>
    }
}
