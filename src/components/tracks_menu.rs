//! Subtitles + audio track menu. Reads `state.tracks` (already typed via
//! the wrapper) and splits into the two lists.
//!
//! Track display logic is a Rust port of [tracks.js]'s `buildTrackTitles`:
//! tracks with a unique language show just the language; tracks sharing a
//! language get either "Language - Title" or "Language - N" disambiguation.
//! Region codes (e.g. `es-ES` vs `es-419`) only appear when needed.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{SpecialTrack, Track, TrackKind, TrackSelection};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::bridge::commands;
use crate::components::toolbar::{Reflow, Row, Side};
use crate::components::Menu;
use crate::state::PlayerState;

#[component]
pub fn TracksAnchor(row: Row) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();
    let open = RwSignal::new(false);
    let btn_ref = NodeRef::<html::Button>::new();

    let hidden = move || reflow.hidden(row, Side::End, "tracks");
    let toggle = move |_| open.update(|v| *v = !*v);

    let subs_list_ref = NodeRef::<html::Div>::new();
    let audio_list_ref = NodeRef::<html::Div>::new();

    let measure = move || {
        let (Some(s), Some(a)) =
            (subs_list_ref.get_untracked(), audio_list_ref.get_untracked())
        else {
            return;
        };
        let s_el: &web_sys::HtmlElement = s.unchecked_ref();
        let a_el: &web_sys::HtmlElement = a.unchecked_ref();
        resize_track_lists(s_el, a_el);
    };

    // Re-cap each list's min-height whenever the menu opens or the track
    // list changes. Deferred to the next tick so the new rows are in the
    // DOM by the time we read `scrollHeight`.
    Effect::new(move |_| {
        let _ = state.tracks.with(|t| t.len());
        let _ = open.get();
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
        state
            .tracks
            .with(|ts| ts.iter().filter(|t| t.kind == TrackKind::Sub).cloned().collect::<Vec<_>>())
    });
    let audio = Memo::new(move |_| {
        state
            .tracks
            .with(|ts| ts.iter().filter(|t| t.kind == TrackKind::Audio).cloned().collect::<Vec<_>>())
    });

    view! {
        <div class="tracks-anchor" class:hidden=hidden>
            <button
                node_ref=btn_ref
                class="icon-button"
                on:click=toggle
            >
                <img src=subs_icon alt="Subtitles" />
                <div class="tooltip">
                    <span class="tooltip-text">"Subtitles/Audio"</span>
                </div>
            </button>
            <Menu open anchor=btn_ref class="tracks-menu">
                <div class="menu-heading">
                    <span>"Subtitles"</span>
                    <img src="public/icons/subtitles.svg" alt="" />
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
                                    spawn_local(async move {
                                        let _ = commands::set_subtitle_track(TrackSelection::Id(id)).await;
                                        let _ = commands::set_sub_visibility(true).await;
                                    });
                                });
                                view! { <TrackItem title=label active=active on_select=on_select/> }
                            })
                            .collect_view()
                    }}
                    <OpenSubtitleItem on_select=open/>
                </div>
                <div class="menu-divider"></div>
                <div class="menu-heading">
                    <span>"Audio"</span>
                    <img src="public/icons/volume.svg" alt="" />
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
            </Menu>
        </div>
    }
}

// Unused; pulled in for future "Auto" / sentinel-aware selection UI.
#[allow(dead_code)]
fn auto() -> TrackSelection {
    TrackSelection::Special(SpecialTrack::Auto)
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
            <span class="title">"Open subtitle file…"</span>
            <img src="public/icons/folder-open.svg" alt="" />
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

/// Build human-readable titles for a list of tracks, disambiguating by
/// language / numbering when multiple tracks share a language. Mirrors
/// `buildTrackTitles` from death-by-mpv/src/ui/tracks.js — uses the
/// platform's `Intl.DisplayNames` to localize the language code into its
/// endonym (e.g. "es" → "Español"). Region qualifiers (e.g. "es-419" vs
/// "es-ES") are only included when the same base language has multiple
/// regional variants in the track list.
fn build_track_titles(tracks: &[Track]) -> HashMap<u32, String> {
    let mut lang_counts: HashMap<String, usize> = HashMap::new();
    let mut base_to_codes: HashMap<String, HashSet<String>> = HashMap::new();
    for t in tracks {
        if let Some(l) = &t.lang {
            *lang_counts.entry(l.clone()).or_insert(0) += 1;
            if let Some(base) = base_language(l) {
                base_to_codes.entry(base).or_default().insert(l.clone());
            }
        }
    }
    // A code needs its region qualifier when its base language has multiple
    // regional siblings in this track list.
    let mut needs_region: HashSet<String> = HashSet::new();
    for codes in base_to_codes.values() {
        if codes.len() > 1 {
            for c in codes {
                needs_region.insert(c.clone());
            }
        }
    }

    let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
    for t in tracks {
        if let Some(l) = &t.lang {
            let title = t.title.clone().unwrap_or_default();
            *pair_counts.entry((l.clone(), title)).or_insert(0) += 1;
        }
    }

    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    let mut out: HashMap<u32, String> = HashMap::new();

    for t in tracks {
        let lang = t.lang.as_deref().unwrap_or("");
        let title = t.title.as_deref().unwrap_or("");
        let has_lang = !lang.is_empty();
        let has_title = !title.is_empty();

        let label = if !has_lang && !has_title {
            format!("Track {}", t.id)
        } else if !has_lang {
            title.to_string()
        } else {
            let unique = lang_counts.get(lang).copied().unwrap_or(0) == 1;
            // Falls back to a title-cased raw code when Intl rejects the
            // tag (junk codes from the source, very old webviews, …).
            let lang_label = language_endonym(lang, needs_region.contains(lang))
                .unwrap_or_else(|| title_case(lang));
            if unique {
                lang_label
            } else {
                let pair_key = (lang.to_string(), title.to_string());
                let total = pair_counts.get(&pair_key).copied().unwrap_or(0);
                let needs_number = total > 1 || !has_title;
                let n = seen.entry(pair_key).and_modify(|c| *c += 1).or_insert(1);
                let n = *n;
                if has_title && !needs_number {
                    // The track's own title already mentions the language
                    // (e.g. "English (Director's commentary)") — don't
                    // double up. Compare against the localized name, since
                    // that's what the user would see.
                    let title_includes_lang =
                        title.to_lowercase().contains(&lang_label.to_lowercase());
                    if title_includes_lang {
                        title.to_string()
                    } else {
                        format!("{lang_label} - {title}")
                    }
                } else if has_title && needs_number {
                    format!("{lang_label} - {title} {n}")
                } else {
                    format!("{lang_label} - {n}")
                }
            }
        };

        out.insert(t.id, label);
    }
    out
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

// ============================================================================
// Intl bindings — ported from `languageCodeEndonym` in tracks.js
// ============================================================================

// The Rust type name doubles as the JS class name unless `js_name` overrides
// it — without these, wasm-bindgen would look up `Intl.IntlLocale` /
// `Intl.IntlDisplayNames`, both `undefined`, and every call would throw.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Intl, js_name = "Locale")]
    type IntlLocale;

    #[wasm_bindgen(constructor, js_namespace = Intl, js_class = "Locale", catch)]
    fn new(tag: &str) -> Result<IntlLocale, JsValue>;

    #[wasm_bindgen(method, getter, js_class = "Locale")]
    fn language(this: &IntlLocale) -> String;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Intl, js_name = "DisplayNames")]
    type IntlDisplayNames;

    #[wasm_bindgen(constructor, js_namespace = Intl, js_class = "DisplayNames", catch)]
    fn new(
        locales: &js_sys::Array,
        options: &js_sys::Object,
    ) -> Result<IntlDisplayNames, JsValue>;

    #[wasm_bindgen(method, catch, js_class = "DisplayNames")]
    fn of(this: &IntlDisplayNames, code: &str) -> Result<JsValue, JsValue>;
}

/// Base language code of a BCP 47 tag — `"en"` for `"en-US"`, `"es"` for
/// `"es-419"`. Returns `None` when `Intl.Locale` rejects the tag.
fn base_language(code: &str) -> Option<String> {
    IntlLocale::new(code).ok().map(|l| l.language())
}

/// Localized language name for a BCP 47 tag, in the language's own script
/// (e.g. `"es"` → `"Español"`, `"ja"` → `"日本語"`). When `with_region` is
/// true the region qualifier is included (`"es-419"` → `"Español (Latinoamérica)"`).
/// First letter uppercased to match the JS port. Returns `None` when Intl
/// can't resolve the code at all.
fn language_endonym(code: &str, with_region: bool) -> Option<String> {
    if code.is_empty() {
        return None;
    }
    let locale = IntlLocale::new(code).ok()?;
    let base = locale.language();

    let locales = js_sys::Array::of1(&JsValue::from_str(&base));
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("type"),
        &JsValue::from_str("language"),
    )
    .ok()?;
    let display = IntlDisplayNames::new(&locales, &options).ok()?;

    let lookup: &str = if with_region { code } else { &base };
    let name = display
        .of(lookup)
        .ok()
        .and_then(|v| v.as_string())
        .or_else(|| display.of(&base).ok().and_then(|v| v.as_string()))?;

    let mut chars = name.chars();
    let first: String = chars.next()?.to_uppercase().collect();
    Some(format!("{first}{}", chars.as_str()))
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
        cs.get_property_value(name).ok().map(|s| parse_px(&s)).unwrap_or(0.0)
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
