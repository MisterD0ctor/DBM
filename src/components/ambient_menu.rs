//! Ambient (border shader) anchor: a button + popover with the toggle and
//! one slider per shader parameter.
//!
//! Each slider reads a unified [0, 1] value and the apply call maps back
//! into the param's own range — same convention as the JS player.

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{AmbientParam, AmbientParams};
use wasm_bindgen::JsCast;

use crate::bridge::commands;
use crate::components::toolbar::{Reflow, Row, Side};
use crate::components::Menu;
use crate::state::PlayerState;

/// (name, label, min, max, default-in-param-units). `step` on the slider
/// is fixed at 0.01 since the slider always runs on [0, 1].
const PARAMS: &[(&str, &str, f64, f64, f64)] = &[
    ("edge_blur", "Edge blur", 0.0, 0.1, 0.01),
    ("spread", "Spread", 0.01, 2.0, 1.0),
    ("falloff", "Falloff", 0.0, 10.0, 4.0),
    ("falloff_softness", "Falloff softness", 0.0, 2.0, 0.2),
];

#[component]
pub fn AmbientAnchor(row: Row) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();
    let open = RwSignal::new(false);
    let btn_ref = NodeRef::<html::Button>::new();
    let hidden = move || reflow.hidden(row, Side::End, "ambient");

    // One signal per param, holding the current value in *param units* (not
    // slider units). The view derives the slider position; on input we map
    // back into param units before storing.
    let values: Vec<RwSignal<f64>> = PARAMS
        .iter()
        .map(|(_, _, _, _, default)| RwSignal::new(*default))
        .collect();
    // Memoize once so the closures below all see the same Vec.
    let values_for_load = values.clone();
    let values_for_effect = values.clone();
    let values_for_view = values.clone();

    // Load saved params (if any) and seed the signals on mount. Match by
    // name so an older saved file with a different param order still works.
    spawn_local(async move {
        let Ok(Some(saved)) = commands::load_ambient_params().await else {
            return;
        };
        for (i, (name, _, _, _, _)) in PARAMS.iter().enumerate() {
            if let Some(p) = saved.params.iter().find(|p| p.name == *name) {
                values_for_load[i].set(p.value);
            }
        }
        if saved.enabled != state.border_background.with_untracked(|b| b == "shader") {
            let _ = commands::set_ambient_enabled(saved.enabled).await;
        }
    });

    // Push every change to the backend (sets the live shader opts + saves
    // to disk). One Effect that reads every signal so any input retriggers.
    Effect::new(move |_| {
        let params: Vec<AmbientParam> = PARAMS
            .iter()
            .zip(&values_for_effect)
            .map(|((name, _, _, _, _), sig)| AmbientParam {
                name: name.to_string(),
                value: sig.get(),
            })
            .collect();
        let enabled = state.border_background.with(|b| b == "shader");
        spawn_local(async move {
            let _ = commands::apply_ambient_params(AmbientParams { enabled, params }).await;
        });
    });

    let toggle_open = move |_| open.update(|v| *v = !*v);
    let enabled = move || state.border_background.with(|b| b == "shader");
    let toggle_enabled = move |_| {
        let next = !enabled();
        spawn_local(async move {
            let _ = commands::set_ambient_enabled(next).await;
        });
    };
    let icon = move || {
        if enabled() {
            "public/icons/bulb-solid.svg"
        } else {
            "public/icons/bulb.svg"
        }
    };

    view! {
        <div class="ambient-anchor" class:hidden=hidden>
            <button
                node_ref=btn_ref
                class="icon-button"
                on:click=toggle_open
            >
                <img src=icon alt="OLED saver" />
                <div class="tooltip">
                    <span class="tooltip-text">"OLED saver"</span>
                    <span class="shortcut">"B"</span>
                </div>
            </button>
            <Menu open=open anchor=btn_ref class="ambient-menu".to_string()>
                <button class="menu-item ambient-toggle" on:click=toggle_enabled>
                    <span>"OLED saver"</span>
                    <span class="ambient-toggle-track" class:on=enabled>
                        <span class="ambient-toggle-knob"></span>
                    </span>
                </button>
                <div class="menu-divider"></div>
                <div class="ambient-sliders">
                    {PARAMS
                        .iter()
                        .enumerate()
                        .map(|(i, &(_, label, min, max, _))| {
                            let sig = values_for_view[i];
                            view! {
                                <AmbientSlider label=label.to_string() min=min max=max value=sig/>
                            }
                        })
                        .collect_view()}
                </div>
            </Menu>
        </div>
    }
}

#[component]
fn AmbientSlider(
    label: String,
    min: f64,
    max: f64,
    value: RwSignal<f64>,
) -> impl IntoView {
    let to_slider = move |v: f64| (v - min) / (max - min);
    let from_slider = move |s: f64| min + s * (max - min);

    let slider_pct = move || (to_slider(value.get()) * 100.0).clamp(0.0, 100.0);
    let display = move || format!("{:.2}", to_slider(value.get()));

    let on_input = move |ev: ev::Event| {
        let Some(input) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let s: f64 = input.value().parse().unwrap_or(0.0);
        value.set(from_slider(s));
    };

    view! {
        <div class="ambient-row">
            <div class="ambient-row-head">
                <span class="ambient-row-label">{label}</span>
                <span class="ambient-row-value">{display}</span>
            </div>
            <input
                class="ambient-slider"
                type="range"
                min="0"
                max="1"
                step="0.01"
                prop:value=move || to_slider(value.get()).to_string()
                on:input=on_input
                style:--slider-progress=move || format!("{}%", slider_pct())
            />
        </div>
    }
}
