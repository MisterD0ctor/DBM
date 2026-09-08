//! Reusable icon button with optional tooltip / shortcut hint.
//!
//! Replaces the `setButtonIcon` + `setButtonTooltip` imperative helpers from
//! the old JS app. All props that change with state are `Signal<T>` — pass a
//! memo or derived signal to drive icon/tooltip reactivity.
//!
//! Example:
//! ```ignore
//! <IconButton
//!     icon=Signal::derive(move || if paused.get() { "icons/play.svg" } else { "icons/pause.svg" }.to_string())
//!     tooltip=Signal::derive(move || Some(if paused.get() { "Play" } else { "Pause" }.to_string()))
//!     shortcut=Signal::derive(move || Some("Space".to_string()))
//!     on_click=Callback::new(move |_| toggle())
//! />
//! ```

use leptos::prelude::*;
use crate::util::assets::asset;

#[component]
pub fn IconButton(
    /// Icon image source (e.g. `"icons/play.svg"`).
    #[prop(into)]
    icon: Signal<String>,
    /// Accessible image alt text.
    #[prop(into, optional)]
    alt: Signal<String>,
    /// Tooltip body; hidden when `None`.
    #[prop(into, optional)]
    tooltip: Signal<Option<String>>,
    /// Keyboard shortcut hint shown under the tooltip body.
    #[prop(into, optional)]
    shortcut: Signal<Option<String>>,
    /// Dims the button and ignores clicks when true.
    #[prop(into, optional)]
    disabled: Signal<bool>,
    /// Extra CSS classes appended to `icon-button`.
    #[prop(into, optional)]
    class: String,
    /// Click callback. Skipped when `disabled`.
    #[prop(optional)]
    on_click: Option<Callback<()>>,
) -> impl IntoView {
    let class = format!("icon-button {class}");
    let on_click_inner = move |_| {
        if disabled.get_untracked() {
            return;
        }
        if let Some(cb) = on_click {
            cb.run(());
        }
    };

    view! {
        <button
            class=class
            class:disabled=move || disabled.get()
            disabled=move || disabled.get()
            on:click=on_click_inner
        >
            <img src=move || asset(&icon.get()) alt=move || alt.get() />
            <Show when=move || tooltip.get().is_some() || shortcut.get().is_some()>
                <div class="tooltip">
                    {move || tooltip.get().map(|t| view! { <span class="tooltip-text">{t}</span> })}
                    {move || shortcut.get().map(|s| view! { <span class="shortcut">{s}</span> })}
                </div>
            </Show>
        </button>
    }
}
