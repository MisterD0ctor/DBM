//! Subtitles + audio track menu. Reads `state.tracks` (already typed via
//! the wrapper) and splits into the two lists.
//!
//! Track naming lives in [`crate::util::lang::build_track_titles`] — shared
//! with the caption toggle's action overlay, so a track is called the same
//! thing wherever it's named.

use std::time::Duration;

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{SpecialTrack, Track, TrackKind, TrackSelection};
use wasm_bindgen::JsCast;

use crate::bridge::commands;
use crate::components::toolbar::{Reflow, Row, Side};
use crate::components::Menu;
use crate::state::PlayerState;
use crate::util::assets::asset;
use crate::util::lang::build_track_titles;
use crate::util::subtitles::{format_delay, is_shifted, step_delay, DELAY_STEP};

#[component]
pub fn TracksAnchor(row: Row) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();
    let open = RwSignal::new(false);
    let btn_ref = NodeRef::<html::Button>::new();

    let hidden = move || reflow.hidden(row, Side::End, "tracks");
    // Which of the menu's two faces is showing. Always reopens on the track
    // list — the settings view is somewhere you go, not a mode you leave the
    // menu parked in.
    let settings_open = RwSignal::new(false);
    let toggle = move |_| {
        settings_open.set(false);
        open.update(|v| *v = !*v);
    };

    let subs_list_ref = NodeRef::<html::Div>::new();
    let audio_list_ref = NodeRef::<html::Div>::new();

    let measure = move || {
        let (Some(s), Some(a)) = (
            subs_list_ref.get_untracked(),
            audio_list_ref.get_untracked(),
        ) else {
            return;
        };
        let s_el: &web_sys::HtmlElement = s.unchecked_ref();
        let a_el: &web_sys::HtmlElement = a.unchecked_ref();
        resize_track_lists(s_el, a_el);
    };

    // Re-cap each list's min-height whenever the menu opens or the track
    // list changes. Deferred to the next tick so the new rows are in the
    // DOM by the time we read `scrollHeight`.
    //
    // `settings_open` counts as a trigger too: leaving the settings view
    // remounts the lists, and a fresh element carries no min-height.
    Effect::new(move |_| {
        let _ = state.tracks.with(|t| t.len());
        let _ = open.get();
        let _ = settings_open.get();
        set_timeout(move || measure(), Duration::from_millis(0));
    });

    // Window resize while open — the menu's max-height is viewport-bound
    // (`min(600px, calc(100vh - 80px))`), so the share changes with height.
    let resize_handle = window_event_listener(ev::resize, move |_| {
        if !open.get_untracked() {
            return;
        }
        measure();
    });
    on_cleanup(move || resize_handle.remove());
    let subs_icon = move || {
        if state.sub_visibility.get() {
            "public/icons/subtitles-solid.svg"
        } else {
            "public/icons/subtitles.svg"
        }
    };

    let subs = Memo::new(move |_| {
        state.tracks.with(|ts| {
            ts.iter()
                .filter(|t| t.kind == TrackKind::Sub)
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let audio = Memo::new(move |_| {
        state.tracks.with(|ts| {
            ts.iter()
                .filter(|t| t.kind == TrackKind::Audio)
                .cloned()
                .collect::<Vec<_>>()
        })
    });

    view! {
        <div class="tracks-anchor" class:hidden=hidden>
            <button
                node_ref=btn_ref
                class="icon-button"
                on:click=toggle
            >
                <img src=move || asset(subs_icon()) alt="Subtitles" />
                <div class="tooltip">
                    <span class="tooltip-text">"Subtitles/Audio"</span>
                </div>
            </button>
            <Menu open anchor=btn_ref class="tracks-menu">
                // Two `Show`s rather than one wrapper per view: `Show` adds no
                // element of its own, so the track lists stay direct children
                // of `.menu` — which is what `resize_track_lists` walks.
                <Show when=move || settings_open.get()>
                    <SubtitleSettingsPanel
                        on_back=Callback::new(move |()| settings_open.set(false))
                    />
                </Show>
                <Show when=move || !settings_open.get()>
                <div class="menu-heading">
                    <img src=asset("public/icons/subtitles.svg") alt="" />
                    <span>"Subtitles"</span>
                    // Only offer the settings when there's a subtitle to
                    // apply them to.
                    <Show when=move || !subs.get().is_empty()>
                        <button
                            class="heading-action"
                            on:click=move |_| settings_open.set(true)
                        >
                            <img src=asset("public/icons/settings.svg") alt="" />
                        </button>
                    </Show>
                </div>
                <div class="tracks-list tracks-subtitle" node_ref=subs_list_ref>
                    <TrackItem
                        title="Off".into()
                        // "Off" is active whenever no subtitle is actually
                        // rendering — subs hidden, no `sid` set, or the file
                        // has no subtitle tracks at all. Without the last
                        // two checks a freshly-loaded subtitle-less video
                        // leaves the section with nothing highlighted.
                        active=Signal::derive(move || {
                            if !state.sub_visibility.get() {
                                return true;
                            }
                            let sid = state.sid.with(|s| s.clone());
                            let Some(sid) = sid else { return true };
                            subs.with(|ts| !ts.iter().any(|t| t.id.to_string() == sid))
                        })
                        on_select=Callback::new(move |()| {
                            open.set(false);
                            spawn_local(async move {
                                let _ = commands::set_sub_visibility(false).await;
                            });
                        })
                    />
                    {move || {
                        let titles = build_track_titles(&subs.get());
                        subs.get()
                            .into_iter()
                            .map(|track| {
                                let id = track.id;
                                // The one place a subtitle language is chosen
                                // on purpose. Remembered so the caption
                                // toggle can prefer the same language on the
                                // next file — see `util::keyboard`.
                                let lang = track.lang.clone();
                                let label = titles
                                    .get(&id)
                                    .cloned()
                                    .unwrap_or_else(|| format!("Track {id}"));
                                let active = Signal::derive(move || {
                                    state.sub_visibility.get()
                                        && state.sid.with(|s| s.as_deref() == Some(&id.to_string()))
                                });
                                let on_select = Callback::new(move |()| {
                                    open.set(false);
                                    let lang = lang.clone();
                                    spawn_local(async move {
                                        let _ = commands::set_subtitle_track(TrackSelection::Id(id)).await;
                                        let _ = commands::set_sub_visibility(true).await;
                                        if let Some(lang) = lang {
                                            let _ = commands::save_sub_language(lang).await;
                                        }
                                    });
                                });
                                view! { <TrackItem title=label active=active on_select=on_select/> }
                            })
                            .collect_view()
                    }}
                    <OpenSubtitleItem on_select=open/>
                </div>
                // <div class="menu-divider"></div>
                <div class="menu-heading">
                    <img src=asset("public/icons/speaking.svg") alt="" />
                    <span>"Audio"</span>
                </div>
                <div class="tracks-list tracks-audio" node_ref=audio_list_ref>
                    {move || {
                        let titles = build_track_titles(&audio.get());
                        audio.get()
                            .into_iter()
                            .map(|track| {
                                let id = track.id;
                                let label = titles
                                    .get(&id)
                                    .cloned()
                                    .unwrap_or_else(|| format!("Track {id}"));
                                let active = Signal::derive(move || {
                                    state.aid.with(|s| s.as_deref() == Some(&id.to_string()))
                                });
                                let on_select = Callback::new(move |()| {
                                    open.set(false);
                                    spawn_local(async move {
                                        let _ = commands::set_audio_track(TrackSelection::Id(id)).await;
                                    });
                                });
                                view! { <TrackItem title=label active=active on_select=on_select/> }
                            })
                            .collect_view()
                    }}
                </div>
                </Show>
            </Menu>
        </div>
    }
}

// Unused; pulled in for future "Auto" / sentinel-aware selection UI.
#[allow(dead_code)]
fn auto() -> TrackSelection {
    TrackSelection::Special(SpecialTrack::Auto)
}

/// The menu's second face: everything about how the subtitles are drawn,
/// as opposed to which one is playing. Sits in the same popover box, so
/// there's no second anchor to position and no extra toolbar button.
#[component]
fn SubtitleSettingsPanel(on_back: Callback<()>) -> impl IntoView {
    let state = expect_context::<PlayerState>();

    view! {
        <div class="menu-heading settings-heading">
            <button class="settings-back" on:click=move |_| on_back.run(())>
                <img
                    class="settings-back-arrow"
                    src="public/icons/angle-small-left.svg"
                    alt="Back"
                />
                <span>"Subtitle settings"</span>
            </button>
        </div>
        <div class="setting-rows">
            <SubDelayRow/>
            <SettingSlider
                label="Size"
                icon="public/icons/text-size.svg"
                min=0.5
                max=2.0
                step=0.05
                value=Signal::derive(move || state.sub_scale.get())
                display=Callback::new(|v: f64| format!("{v:.2}×"))
                on_change=Callback::new(|v: f64| {
                    spawn_local(async move {
                        let _ = commands::set_sub_scale(v).await;
                    });
                })
            />
            <SettingSlider
                label="Position"
                icon="public/icons/arrows-up-down.svg"
                min=0.0
                max=150.0
                step=1.0
                value=Signal::derive(move || state.sub_pos.get())
                display=Callback::new(|v: f64| format!("{v:.0}"))
                on_change=Callback::new(|v: f64| {
                    spawn_local(async move {
                        let _ = commands::set_sub_pos(v).await;
                    });
                })
            />
        </div>
    }
}

/// Label + value + slider, in the setting's own units. The displayed value
/// tracks mpv rather than the input element, so a clamp or a change from
/// anywhere else shows up here.
#[component]
fn SettingSlider(
    label: &'static str,
    /// Optional so the ambient menu's rows can share this styling later
    /// without growing icons they don't have.
    #[prop(optional)]
    icon: Option<&'static str>,
    min: f64,
    max: f64,
    step: f64,
    #[prop(into)] value: Signal<f64>,
    display: Callback<f64, String>,
    on_change: Callback<f64>,
) -> impl IntoView {
    let on_input = move |ev: ev::Event| {
        let Some(input) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        if let Ok(v) = input.value().parse::<f64>() {
            on_change.run(v);
        }
    };
    // Same one-step-per-notch, scroll-up-increases convention as the volume
    // slider and the delay row.
    let on_wheel = move |ev: ev::WheelEvent| {
        ev.prevent_default();
        let delta = if ev.delta_y() < 0.0 { step } else { -step };
        on_change.run((value.get_untracked() + delta).clamp(min, max));
    };
    let progress = move || (value.get() - min) / (max - min) * 100.0;

    view! {
        <div class="setting-row">
            <div class="setting-row-head">
                <span class="setting-row-name">
                    {icon.map(|src| view! { <img src=asset(src) alt="" /> })}
                    <span class="setting-row-label">{label}</span>
                </span>
                <span class="setting-row-value">{move || display.run(value.get())}</span>
            </div>
            <input
                class="slider"
                type="range"
                min=min
                max=max
                step=step
                prop:value=move || value.get()
                on:input=on_input
                on:wheel=on_wheel
                style:--slider-progress=move || format!("{}%", progress())
            />
        </div>
    }
}

/// Subtitle timing. Nudges mpv's `sub-delay` in mpv's own 0.1s steps —
/// positive shows the subtitles later, matching the sign convention on
/// mpv's OSD so the numbers mean the same thing in both places.
///
/// Writes go straight to mpv and the displayed value comes back through the
/// `sub-delay` property, so holding a button can't desync the readout from
/// what's actually applied. The value doubles as the reset button.
#[component]
fn SubDelayRow() -> impl IntoView {
    let state = expect_context::<PlayerState>();

    let nudge = move |delta: f64| {
        let next = step_delay(state.sub_delay.get_untracked(), delta);
        spawn_local(async move {
            let _ = commands::set_sub_delay(next).await;
        });
    };
    let reset = move |_| {
        spawn_local(async move {
            let _ = commands::set_sub_delay(0.0).await;
        });
    };
    // Scroll anywhere over the row, not just the buttons — same one-step-per-
    // notch, scroll-up-increases convention as the volume slider.
    let on_wheel = move |ev: ev::WheelEvent| {
        ev.prevent_default();
        nudge(if ev.delta_y() < 0.0 {
            DELAY_STEP
        } else {
            -DELAY_STEP
        });
    };

    // Kept out of the `view!` below on purpose: a `>=` inside an attribute
    // expression gets read as the end of the tag.
    let modified = move || is_shifted(state.sub_delay.get());
    let label = move || format_delay(state.sub_delay.get());

    view! {
        <div class="sub-delay" on:wheel=on_wheel>
            // "Delay", not "Subtitle delay" — the panel it sits in already
            // says subtitle, and the rows beside it are Size and Position.
            <span class="setting-row-name">
                <img src=asset("public/icons/clock.svg") alt="" />
                <span class="setting-row-label">"Delay"</span>
            </span>
            <div class="sub-delay-controls">
                <button
                    class="sub-delay-btn"
                    on:click=move |_| nudge(-DELAY_STEP)
                    title="Show subtitles earlier (z)"
                >"−"</button>
                <button
                    class="sub-delay-value"
                    class:modified=modified
                    on:click=reset
                    title="Reset to 0 · scroll to adjust"
                >{label}</button>
                <button
                    class="sub-delay-btn"
                    on:click=move |_| nudge(DELAY_STEP)
                    title="Show subtitles later (Shift+Z)"
                >"+"</button>
            </div>
        </div>
    }
}

#[component]
fn OpenSubtitleItem(on_select: RwSignal<bool>) -> impl IntoView {
    let click = move |_| {
        on_select.set(false);
        spawn_local(async move {
            let _ = commands::open_subtitle_dialog().await;
        });
    };
    view! {
        <div class="menu-item subtitle-open" on:click=click>
            <div class="highlight"></div>
            <img src=asset("public/icons/folder-open.svg") alt="" />
            <span class="title">"Open subtitle file…"</span>
        </div>
    }
}

#[component]
fn TrackItem(
    title: String,
    #[prop(into)] active: Signal<bool>,
    on_select: Callback<()>,
) -> impl IntoView {
    view! {
        <div
            class="menu-item track-item"
            class:active=move || active.get()
            on:click=move |_| on_select.run(())
        >
            <div class="highlight"></div>
            <span class="title">{title}</span>
        </div>
    }
}

/// Cap each `.tracks-list` so the menu fits within its max-height. Lists
/// that naturally fit within an equal share keep their full content height
/// (via `min-height: fit-content`); the remaining lists share the leftover
/// space via flex shrink + `overflow-y: auto`. Mirrors `resizeTrackListMenus`
/// in death-by-mpv/src/ui/tracks.js.
fn resize_track_lists(subs_el: &web_sys::HtmlElement, audio_el: &web_sys::HtmlElement) {
    let Some(menu_node) = subs_el.parent_element() else {
        return;
    };
    let Ok(menu) = menu_node.dyn_into::<web_sys::HtmlElement>() else {
        return;
    };
    let Some(win) = web_sys::window() else {
        return;
    };

    // Reset so we can measure each list's natural (unconstrained) height.
    let _ = subs_el.style().set_property("min-height", "");
    let _ = audio_el.style().set_property("min-height", "");

    let Ok(Some(menu_cs)) = win.get_computed_style(&menu) else {
        return;
    };
    let parse_px = |s: &str| -> f64 { s.trim_end_matches("px").trim().parse().unwrap_or(0.0) };
    let prop = |cs: &web_sys::CssStyleDeclaration, name: &str| -> f64 {
        cs.get_property_value(name)
            .ok()
            .map(|s| parse_px(&s))
            .unwrap_or(0.0)
    };

    // `min(...)` / `calc(...)` should resolve to a px value in computed
    // style; if it doesn't, fall back to the menu's rendered height.
    let max_height = {
        let raw = menu_cs.get_property_value("max-height").unwrap_or_default();
        let v = parse_px(&raw);
        if v > 0.0 {
            v
        } else {
            menu.get_bounding_client_rect().height()
        }
    };
    let pad = prop(&menu_cs, "padding-top") + prop(&menu_cs, "padding-bottom");

    // Sum the heights of every menu child that isn't a `.tracks-list`
    // (headings, dividers, …). These are fixed in height; only the lists
    // are negotiable.
    let children = menu.children();
    let mut fixed = 0.0_f64;
    for i in 0..children.length() {
        let Some(child) = children.item(i) else {
            continue;
        };
        if child
            .class_name()
            .split_whitespace()
            .any(|c| c == "tracks-list")
        {
            continue;
        }
        let Ok(child_el) = child.dyn_into::<web_sys::HtmlElement>() else {
            continue;
        };
        let Ok(Some(child_cs)) = win.get_computed_style(&child_el) else {
            continue;
        };
        let margin = prop(&child_cs, "margin-top") + prop(&child_cs, "margin-bottom");
        fixed += child_el.offset_height() as f64 + margin;
    }

    let available = max_height - fixed - pad;
    if available <= 0.0 {
        return;
    }
    let share = available / 2.0;

    for list in [subs_el, audio_el] {
        if (list.scroll_height() as f64) <= share {
            let _ = list.style().set_property("min-height", "fit-content");
        }
    }
}
