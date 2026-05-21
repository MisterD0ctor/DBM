//! Custom title bar — replaces the native OS chrome (`decorations: false`
//! in tauri.conf.json). Fades in/out with `.controls-hidden` like the
//! bottom toolbar, so the window looks frameless while the user is just
//! watching.
//!
//! Drag is handled by Tauri's `data-tauri-drag-region` attribute: any
//! mousedown on an element carrying that attribute starts a window move.
//! Buttons override it on their own bounds so clicks go to the handler
//! instead of starting a drag.

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::bridge::commands;
use crate::state::PlayerState;
use crate::util::window;

#[component]
pub fn WindowDecorator() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let is_max = RwSignal::new(false);

    // Hidden entirely in fullscreen — the OS already chromes the window
    // and the player should take the full surface. `.player` flips its
    // height calc on the same signal so it reclaims the reserved strip.
    let fullscreen = move || state.fullscreen.get();

    // Initial poll + keep it in sync when the user maximizes via the OS
    // (drag to top edge, Win+Up, snap, …) so the icon stays correct.
    spawn_local(async move {
        is_max.set(window::check_maximized().await);
    });
    window::install_resize_listener(move || {
        spawn_local(async move {
            is_max.set(window::check_maximized().await);
        });
    });

    let on_min = move |_: ev::MouseEvent| {
        spawn_local(async move {
            window::minimize_window().await;
        });
    };
    let on_max = move |_: ev::MouseEvent| {
        spawn_local(async move {
            window::toggle_maximize_window().await;
        });
    };
    // Right-click → Windows 11 Snap Layouts. The native title bar gets the
    // popover via DWM's `WM_NCHITTEST` magic, but a CSS-driven custom bar
    // doesn't, so we synthesize Win+Z (the same shortcut the OS exposes).
    let on_max_contextmenu = move |ev: ev::MouseEvent| {
        ev.prevent_default();
        spawn_local(async move {
            if let Err(e) = commands::show_snap_layouts().await {
                leptos::logging::warn!("show_snap_layouts failed: {e}");
            }
        });
    };
    let on_close = move |_: ev::MouseEvent| {
        spawn_local(async move {
            window::close_window().await;
        });
    };
    // Standard Windows convention: double-clicking the title region toggles
    // maximize. `data-tauri-drag-region` doesn't do this for us.
    let on_title_dblclick = move |_: ev::MouseEvent| {
        spawn_local(async move {
            window::toggle_maximize_window().await;
        });
    };

    view! {
        <div
            class="window-decorator"
            class:hidden=fullscreen
            data-tauri-drag-region="true"
        >
            <div
                class="window-logo"
                data-tauri-drag-region="true"
                on:dblclick=on_title_dblclick
            >
                <img src="public/logo.svg" alt="Logo"/>
            </div>
            <div class="window-buttons">
                <button class="window-btn" on:click=on_min title="Minimize">
                    <svg viewBox="0 0 10 10" aria-hidden="true">
                        <path d="M1 5h8" stroke="currentColor" stroke-width="1" stroke-linecap="square"/>
                    </svg>
                </button>
                <button
                    class="window-btn"
                    on:click=on_max
                    on:contextmenu=on_max_contextmenu
                    title="Maximize"
                >
                    {move || if is_max.get() {
                        // Restore: two offset rectangles (Windows convention).
                        view! {
                            <svg viewBox="0 0 10 10" aria-hidden="true">
                                <rect x="1.5" y="2.5" width="8" height="8" stroke="currentColor" stroke-width="1" fill="none"/>
                                <rect x="3" y="1" width="8" height="8" stroke="currentColor" stroke-width="1" fill="none"/>
                            </svg>
                        }.into_any()
                    } else {
                        view! {
                            <svg viewBox="0 0 10 10" aria-hidden="true">
                                <rect x="1.5" y="1.5" width="8" height="8" stroke="currentColor" stroke-width="1" fill="none"/>
                            </svg>
                        }.into_any()
                    }}
                </button>
                <button class="window-btn close" on:click=on_close title="Close">
                    <svg viewBox="0 0 10 10" aria-hidden="true">
                        <path d="M1 1 L9 9 M9 1 L1 9" stroke="currentColor" stroke-width="1" stroke-linecap="square"/>
                    </svg>
                </button>
            </div>
        </div>
    }
}
