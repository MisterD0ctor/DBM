//! "Next" / "Restart" overlay shown when mpv reports `eof-reached`.
//!
//! On the last playlist entry the button rewinds; otherwise it advances.
//! Replaces the imperative `setEndOfPlayback` + `refreshEndOfPlayback`
//! bookkeeping in [playback.js].

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{SeekMode, SeekPrecision};

use crate::bridge::commands;
use crate::state::PlayerState;
use crate::util::assets::asset;

#[component]
pub fn EndOfPlayback() -> impl IntoView {
    let state = expect_context::<PlayerState>();

    let visible = move || state.eof_reached.get();
    let is_last = move || state.is_last_video();

    let label = move || if is_last() { "Restart" } else { "Next" };
    let icon = move || {
        if is_last() {
            "public/icons/rewind.svg"
        } else {
            "public/icons/step-forward.svg"
        }
    };

    let on_click = move |_| {
        if is_last() {
            spawn_local(async move {
                let _ = commands::seek(0.0, SeekMode::Absolute, SeekPrecision::Exact).await;
                let _ = commands::play().await;
            });
        } else {
            spawn_local(async move {
                let _ = commands::playlist_next().await;
                let _ = commands::play().await;
            });
        }
    };

    view! {
        <button
            class="end-of-playback"
            class:hidden=move || !visible()
            on:click=on_click
        >
            <img src=move || asset(icon()) alt="" />
            <span>{label}</span>
        </button>
    }
}
