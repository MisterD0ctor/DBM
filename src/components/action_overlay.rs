//! Brief center-screen feedback shown on keyboard shortcuts.
//!
//! [`ActionFeedback`] is provided via context by `App`. Any handler can
//! `feedback.show(kind, text, position)` to flash an icon. The overlay's
//! animation re-triggers on each call via a generation counter — the same
//! trick the JS app pulled off by force-reflowing the DOM, but driven by
//! a reactive signal instead.

use leptos::html;
use leptos::prelude::*;

use crate::state::PlayerState;
use crate::util::assets::asset;

/// The icon flashed in the overlay. Add variants as new shortcuts surface.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Pause,
    Play,
    SeekForward,
    SeekBackward,
    Rewind,
    Previous,
    Next,
    PanscanOn,
    PanscanOff,
    FullscreenOn,
    FullscreenOff,
    AmbientOn,
    AmbientOff,
    MuteOn,
    MuteOff,
    SubtitlesOn,
    SubtitlesOff,
    SubDelay,
    Volume,
}

impl ActionKind {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Pause => "public/icons/pause.svg",
            Self::Play => "public/icons/play.svg",
            Self::SeekForward => "public/icons/seek-forward.svg",
            Self::SeekBackward => "public/icons/seek-backward.svg",
            Self::Rewind => "public/icons/rotate-left.svg",
            Self::Previous => "public/icons/step-backward.svg",
            Self::Next => "public/icons/step-forward.svg",
            Self::PanscanOn => "public/icons/panscan-on.svg",
            Self::PanscanOff => "public/icons/panscan-off.svg",
            Self::FullscreenOn => "public/icons/fullscreen-on.svg",
            Self::FullscreenOff => "public/icons/fullscreen-off.svg",
            Self::AmbientOn => "public/icons/ambience-on.svg",
            Self::AmbientOff => "public/icons/ambience-slash.svg",
            Self::MuteOn => "public/icons/volume-mute.svg",
            Self::MuteOff => "public/icons/volume.svg",
            Self::SubtitlesOn => "public/icons/subtitles-solid.svg",
            Self::SubtitlesOff => "public/icons/subtitles-slash.svg",
            Self::SubDelay => "public/icons/subtitle-time.svg",
            Self::Volume => "public/icons/volume.svg",
        }
    }
}

#[derive(Clone, Copy)]
pub struct ActionFeedback {
    /// Bump on each `show()` to force the animation restart.
    pub generation: RwSignal<u32>,
    pub kind: RwSignal<Option<ActionKind>>,
    pub text: RwSignal<Option<String>>,
    /// Horizontal position 0..100 (% of player width).
    pub position: RwSignal<f64>,
}

impl ActionFeedback {
    pub fn new() -> Self {
        Self {
            generation: RwSignal::new(0),
            kind: RwSignal::new(None),
            text: RwSignal::new(None),
            position: RwSignal::new(50.0),
        }
    }

    pub fn show(&self, kind: ActionKind, text: Option<String>, position: Option<f64>) {
        self.kind.set(Some(kind));
        self.text.set(text);
        self.position.set(position.unwrap_or(50.0));
        self.generation.update(|g| *g = g.wrapping_add(1));
    }
}

impl Default for ActionFeedback {
    fn default() -> Self {
        Self::new()
    }
}

#[component]
pub fn ActionOverlay() -> impl IntoView {
    let fb = expect_context::<ActionFeedback>();
    let _state = expect_context::<PlayerState>();

    let icon_src = move || fb.kind.get().map(|k| asset(k.icon())).unwrap_or_default();
    let text = move || fb.text.get();
    let el_ref = NodeRef::<html::Div>::new();

    // Restart the CSS animation on every `show()`. A reactive `class:visible`
    // binding can't do this on its own — once the class is on, it stays on
    // and the animation never re-applies. The standard fix is the JS app's
    // trick: drop the class, force a layout flush by touching `offsetWidth`,
    // then add the class back. The reflow flush is what makes the browser
    // re-process the `animation` property.
    Effect::new(move |_| {
        let g = fb.generation.get();
        if g == 0 {
            return;
        }
        let Some(el) = el_ref.get() else {
            return;
        };
        let _ = el.class_list().remove_1("visible");
        let _ = el.offset_width();
        let _ = el.class_list().add_1("visible");
    });

    view! {
        <div
            class="action-overlay"
            node_ref=el_ref
            style:left=move || format!("{}%", fb.position.get())
        >
            // The circle lives on the wrapper, the glyph on the img — split
            // so per-icon `transform: scale(...)` rules can grow the glyph
            // without inflating the surrounding ring.
            <div class="action-icon">
                <img src=icon_src alt="" />
            </div>
            <Show when=move || text().is_some()>
                <span class="action-text">{move || text().unwrap_or_default()}</span>
            </Show>
        </div>
    }
}
