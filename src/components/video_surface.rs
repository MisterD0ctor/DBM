//! The clickable transparent region in front of mpv's video output.
//!
//! Single-click toggles pause; double-click toggles fullscreen. The
//! single-click action is deferred ~250 ms so a real dblclick can cancel it.

use std::time::Duration;

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::bridge::commands;
use crate::state::PlayerState;
use crate::util::overlay::any_menu_open;
use crate::util::window::{enter_fullscreen, exit_fullscreen};

const DBLCLICK_MS: u64 = 250;

#[component]
pub fn VideoSurface() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    // Bumped on every click. The deferred single-click action captures the
    // token it was scheduled with and skips itself if the token moved on
    // (i.e. a dblclick arrived in the meantime).
    let click_token = RwSignal::new(0u64);

    let on_click = move |ev: ev::MouseEvent| {
        if any_menu_open() {
            return;
        }
        match ev.detail() {
            1 => {
                click_token.update(|c| *c = c.wrapping_add(1));
                let token = click_token.get_untracked();
                let next_paused = !state.paused.get_untracked();
                set_timeout(
                    move || {
                        if click_token.get_untracked() == token {
                            spawn_local(async move {
                                let _ = commands::set_pause(next_paused).await;
                            });
                        }
                    },
                    Duration::from_millis(DBLCLICK_MS),
                );
            }
            2 => {
                // Invalidate the queued single-click.
                click_token.update(|c| *c = c.wrapping_add(1));
                let next = !state.fullscreen.get_untracked();
                spawn_local(async move {
                    if next {
                        enter_fullscreen(state).await;
                    } else {
                        exit_fullscreen(state).await;
                    }
                });
            }
            _ => {}
        }
    };

    view! {
        <div class="video-surface" on:click=on_click></div>
    }
}
