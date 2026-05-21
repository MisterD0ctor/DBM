//! Auto-hide the controls panel when the user is idle.
//!
//! One window-level listener bumps an `activity` counter on every
//! mousemove/wheel/mousedown. An `Effect` watching that counter restarts a
//! 3-second timeout each time it changes; when the timeout fires, controls
//! hide unless a menu is open.

use std::time::Duration;

use leptos::ev;
use leptos::prelude::*;

use crate::state::PlayerState;

const HIDE_DELAY: Duration = Duration::from_millis(3000);

pub fn install_overlay_visibility(state: PlayerState) {
    let activity = RwSignal::new(0u64);

    let bump = move || {
        state.controls_visible.set(true);
        activity.update(|n| *n = n.wrapping_add(1));
    };

    let h_move = window_event_listener(ev::mousemove, move |_| bump());
    let h_down = window_event_listener(ev::mousedown, move |_| bump());
    let h_wheel = window_event_listener(ev::wheel, move |_| bump());

    on_cleanup(move || {
        h_move.remove();
        h_down.remove();
        h_wheel.remove();
    });

    Effect::new(move |_| {
        let _ = activity.get(); // track

        let handle = set_timeout_with_handle(
            move || {
                // Keep controls up while a menu is open — the user is
                // clearly still interacting even without moving.
                if any_menu_open() {
                    return;
                }
                state.controls_visible.set(false);
            },
            HIDE_DELAY,
        )
        .ok();

        on_cleanup(move || {
            if let Some(h) = handle {
                h.clear();
            }
        });
    });
}

/// Returns true if any `.menu` element in the DOM is currently not `.hidden`.
/// Used by both the auto-hide effect and the video click handler so they
/// don't fight with open popovers.
pub fn any_menu_open() -> bool {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    doc.query_selector(".menu:not(.hidden)")
        .ok()
        .flatten()
        .is_some()
}
