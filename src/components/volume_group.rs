//! Mute button + volume slider, with mouse-wheel adjustment.
//!
//! Volume icon adapts to the current level (mute / none / down / up). The
//! slider snaps to 100 when the user drags within ±6 of it. Wheel events on
//! the slider step it by ±2.

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::bridge::commands;
use crate::components::IconButton;
use crate::state::PlayerState;

const STEP: f64 = 2.0;
const MIN: f64 = 0.0;
const MAX: f64 = 200.0;
const SNAP_RADIUS: f64 = 6.0;

#[component]
pub fn VolumeGroup() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let slider_ref = NodeRef::<html::Input>::new();

    let icon = Signal::derive(move || {
        if state.muted.get() {
            "public/icons/volume-mute.svg".to_string()
        } else {
            volume_icon(state.volume.get()).to_string()
        }
    });
    let mute_tooltip = Signal::derive(move || {
        Some(if state.muted.get() { "Unmute" } else { "Mute" }.to_string())
    });

    let on_mute = Callback::new(move |()| {
        let next = !state.muted.get();
        spawn_local(async move {
            let _ = commands::set_mute(next).await;
        });
    });

    let on_input = move |ev: ev::Event| {
        let target = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
        let Some(input) = target else { return };
        let mut v: f64 = input.value().parse().unwrap_or(state.volume.get_untracked());
        if (v - 100.0).abs() <= SNAP_RADIUS {
            v = 100.0;
            input.set_value(&v.to_string());
        }
        spawn_local(async move {
            let _ = commands::set_volume(v).await;
        });
    };

    let on_wheel = move |ev: ev::WheelEvent| {
        ev.prevent_default();
        let delta = if ev.delta_y() < 0.0 { STEP } else { -STEP };
        let next = (state.volume.get_untracked() + delta).clamp(MIN, MAX);
        spawn_local(async move {
            let _ = commands::set_volume(next).await;
        });
    };

    view! {
        <div class="volume-group" class:muted=move || state.muted.get()>
            <IconButton
                icon=icon
                tooltip=mute_tooltip
                shortcut=Signal::derive(|| Some("M".to_string()))
                on_click=on_mute
            />
            // The slider + its tooltip share a positioning context so the
            // tooltip can ride the thumb via `left: var(--volume-progress)`.
            // The custom property lives on the wrapper so both the slider's
            // gradient and the tooltip's offset read the same value.
            <div
                class="volume-slider-area"
                style:--volume-progress=move || format!("{}%", state.volume.get() / MAX * 100.0)
            >
                <input
                    node_ref=slider_ref
                    type="range"
                    class="volume-slider"
                    min=MIN
                    max=MAX
                    step=STEP
                    prop:value=move || state.volume.get()
                    on:input=on_input
                    on:wheel=on_wheel
                />
                <div class="tooltip">
                    <span>{move || format!("{:.0}", state.volume.get())}</span>
                    <span>"%"</span>
                </div>
            </div>
        </div>
    }
}

fn volume_icon(volume: f64) -> &'static str {
    if volume > 133.0 {
        "public/icons/volume-up.svg"
    } else if volume > 66.0 {
        "public/icons/volume.svg"
    } else if volume > 0.0 {
        "public/icons/volume-down.svg"
    } else {
        "public/icons/volume-none.svg"
    }
}
