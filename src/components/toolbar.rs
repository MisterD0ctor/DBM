//! Player toolbar — three pill-shaped clusters (start / center / end) plus
//! an optional overflow row above for buttons that don't fit at narrow
//! widths.
//!
//! Reflow is per-side: the end pill is much wider than the start pill (the
//! volume slider takes ~150 px), so the end side drops buttons first when
//! the toolbar tightens. Each candidate is rendered twice (once in the
//! main toolbar, once in the overflow row) and `class:hidden` swaps which
//! copy shows based on the computed overflow level.

use std::time::Duration;

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{SeekMode, SeekPrecision};

use crate::bridge::commands;
use crate::components::{
    AmbientAnchor, IconButton, MediaTitle, PlaylistAnchor, TracksAnchor, VolumeGroup,
};
use crate::state::PlayerState;
use crate::util::window::{enter_fullscreen, exit_fullscreen};

const SEEK_STEP: f64 = 10.0;

// ============================================================================
// Layout pixel budgets (must match styles.css)
// ============================================================================

const ICON_BTN: f64 = 44.0;
const VOLUME_GROUP_FULL: f64 = 153.0; // mute (44) + slider (100) + 5+4 margins
const VOLUME_GROUP_MUTED: f64 = 44.0;
const PILL_PAD: f64 = 8.0; // 4 left + 4 right
const PILL_BORDER: f64 = 2.0; // 1 each side
const PILL_GAP: f64 = 4.0;
const CENTER_WIDTH: f64 = 246.0; // 5 buttons + 4 gaps + padding + border
const TOOLBAR_GAP: f64 = 10.0;
const CONTROLS_PAD: f64 = 20.0; // .controls-panel left+right padding

// Overflow priority — items at the FRONT of the array overflow FIRST. The
// last item in each list is the "stickiest" — least likely to leave the
// main row. Mirrors `startPriority` / `endPriority` in
// death-by-mpv/src/utils/toolbarOverflow.js.
const START_ORDER: &[&str] = &["media-title", "open", "playlist"];
const END_ORDER: &[&str] = &["ambient", "tracks", "panscan", "fullscreen"];

// ============================================================================
// Reflow state
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Start,
    End,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// Render in the main toolbar; hide if its priority has overflowed.
    Main,
    /// Render in the overflow row; hide if its priority is NOT overflowed.
    Overflow,
}

#[derive(Clone, Copy)]
pub struct Reflow {
    /// Number of items currently overflowed from each side.
    pub start_level: RwSignal<u8>,
    pub end_level: RwSignal<u8>,
    /// Last measured natural width (px) of the media-title element. Reported
    /// by [`MediaTitle`] via its `scrollWidth` so reflow can decide whether
    /// to push the title into the overflow row.
    pub media_title_width: RwSignal<f64>,
}

impl Reflow {
    pub fn new() -> Self {
        Self {
            start_level: RwSignal::new(0),
            end_level: RwSignal::new(0),
            media_title_width: RwSignal::new(0.0),
        }
    }

    fn level(&self, side: Side) -> u8 {
        match side {
            Side::Start => self.start_level.get(),
            Side::End => self.end_level.get(),
        }
    }

    /// `true` when this id is the Nth-or-later candidate where N is the
    /// current overflow level — i.e. it has been pushed out of the main row.
    fn overflowed(&self, side: Side, id: &'static str) -> bool {
        let order = match side {
            Side::Start => START_ORDER,
            Side::End => END_ORDER,
        };
        let level = self.level(side);
        order
            .iter()
            .position(|x| *x == id)
            .map(|pos| (pos as u8) < level)
            .unwrap_or(false)
    }

    pub fn hidden(&self, row: Row, side: Side, id: &'static str) -> bool {
        let overflowed = self.overflowed(side, id);
        match row {
            Row::Main => overflowed,
            Row::Overflow => !overflowed,
        }
    }
}

// ============================================================================
// Width budget calculations
// ============================================================================

/// Width of the start pill assuming `level` items have been overflowed and
/// the playlist anchor is or isn't visible at all (depends on whether a
/// file is loaded). `media_title_w` is the title's measured natural width
/// (`scrollWidth`), so reflow follows the actual rendered content.
fn start_width(level: u8, playlist_visible: bool, media_title_w: f64) -> f64 {
    let mut total_item_width = 0.0;
    let mut visible_items = 0u8;
    for (i, id) in START_ORDER.iter().enumerate() {
        if (i as u8) < level {
            continue;
        }
        if *id == "playlist" && !playlist_visible {
            continue;
        }
        let w = match *id {
            "media-title" => media_title_w,
            _ => ICON_BTN,
        };
        total_item_width += w;
        visible_items += 1;
    }
    let gaps = (visible_items as f64 - 1.0).max(0.0) * PILL_GAP;
    total_item_width + gaps + PILL_PAD + PILL_BORDER
}

/// Width of the end pill assuming `level` items have been overflowed.
fn end_width(level: u8, muted: bool) -> f64 {
    let volume = if muted { VOLUME_GROUP_MUTED } else { VOLUME_GROUP_FULL };
    let visible_btn = (END_ORDER.len() as u8 - level) as f64;
    let flex_children = visible_btn + 1.0; // + volume group
    let gaps = (flex_children - 1.0).max(0.0) * PILL_GAP;
    volume + visible_btn * ICON_BTN + gaps + PILL_PAD + PILL_BORDER
}

/// Compute (start_level, end_level) needed so each side fits in its
/// `1fr` track. Returns `(0, 0)` when the width is unknown — better to
/// briefly show the un-reflowed layout than to flash everything overflowed.
fn compute_levels(
    toolbar_width: f64,
    muted: bool,
    playlist_visible: bool,
    media_title_w: f64,
) -> (u8, u8) {
    let available = (toolbar_width - CENTER_WIDTH - 2.0 * TOOLBAR_GAP) / 2.0;
    if !(available > 0.0) {
        return (0, 0);
    }

    let mut end_level = 0u8;
    while end_width(end_level, muted) > available && (end_level as usize) < END_ORDER.len() {
        end_level += 1;
    }

    let mut start_level = 0u8;
    while start_width(start_level, playlist_visible, media_title_w) > available
        && (start_level as usize) < START_ORDER.len()
    {
        start_level += 1;
    }

    (start_level, end_level)
}

// ============================================================================
// Toolbar components — `ToolbarOverflowRow` + `ToolbarMainRow` are siblings
// inside `.controls-panel` with `Timeline` between them so the overflow row
// sits above the progress bar (matching death-by-mpv).
// ============================================================================

#[component]
pub fn ToolbarOverflowRow() -> impl IntoView {
    let reflow = expect_context::<Reflow>();

    let overflow_row_hidden =
        move || reflow.start_level.get() == 0 && reflow.end_level.get() == 0;
    let start_pill_empty = move || reflow.start_level.get() == 0;
    let end_pill_empty = move || reflow.end_level.get() == 0;

    view! {
        <div class="toolbar-overflow" class:hidden=overflow_row_hidden>
            <div class="toolbar-start" class:empty=start_pill_empty>
                <MediaTitle row=Row::Overflow/>
                <OpenAnchor row=Row::Overflow/>
                <PlaylistAnchor row=Row::Overflow/>
            </div>
            <div class="toolbar-end" class:empty=end_pill_empty>
                <TracksAnchor row=Row::Overflow/>
                <AmbientAnchor row=Row::Overflow/>
                <PanscanButton row=Row::Overflow/>
                <FullscreenButton row=Row::Overflow/>
            </div>
        </div>
    }
}

#[component]
pub fn ToolbarMainRow() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();

    let toolbar_ref = NodeRef::<html::Div>::new();

    // Read the actual toolbar element's rendered width — far more reliable
    // than `window.innerWidth` minus padding estimates.
    let measure_and_update = move || {
        let Some(el) = toolbar_ref.get_untracked() else {
            return;
        };
        let toolbar_w = el.get_bounding_client_rect().width();
        let muted = state.muted.get_untracked();
        let playlist_visible = !state.playlist.with_untracked(|p| p.is_empty());
        let media_title_w = reflow.media_title_width.get_untracked();
        let (start, end) = compute_levels(toolbar_w, muted, playlist_visible, media_title_w);
        reflow.start_level.set(start);
        reflow.end_level.set(end);
    };

    // First measurement — defer to the next paint so the toolbar element is
    // actually mounted and laid out. Subsequent runs are triggered by the
    // signals we read here (muted / playlist / measured media-title width).
    Effect::new(move |_| {
        let _ = state.muted.get();
        let _ = state.playlist.with(|p| p.is_empty());
        let _ = reflow.media_title_width.get();
        set_timeout(measure_and_update, Duration::from_millis(0));
        // Belt-and-suspenders: also try after a real frame in case the DOM
        // wasn't laid out at the 0ms tick.
        set_timeout(measure_and_update, Duration::from_millis(50));
    });

    let resize_handle = window_event_listener(ev::resize, move |_| measure_and_update());
    on_cleanup(move || resize_handle.remove());

    view! {
        <div class="toolbar" node_ref=toolbar_ref>
            <div class="toolbar-start">
                <MediaTitle row=Row::Main/>
                <PlaylistAnchor row=Row::Main/>
                <OpenAnchor row=Row::Main/>
            </div>
            <div class="toolbar-center">
                <PreviousButton/>
                <SeekBackButton/>
                <PlayPauseButton/>
                <SeekForwardButton/>
                <NextButton/>
            </div>
            <div class="toolbar-end">
                <VolumeGroup/>
                <AmbientAnchor row=Row::Main/>
                <TracksAnchor row=Row::Main/>
                <PanscanButton row=Row::Main/>
                <FullscreenButton row=Row::Main/>
            </div>
        </div>
    }
}

// ============================================================================
// Center cluster (never overflows)
// ============================================================================

#[component]
fn PlayPauseButton() -> impl IntoView {
    let state = expect_context::<PlayerState>();

    let icon = Signal::derive(move || {
        if state.eof_reached.get() {
            "public/icons/rotate-left.svg".to_string()
        } else if state.paused.get() {
            "public/icons/play.svg".to_string()
        } else {
            "public/icons/pause.svg".to_string()
        }
    });
    let tooltip = Signal::derive(move || {
        Some(
            if state.eof_reached.get() {
                "Restart"
            } else if state.paused.get() {
                "Play"
            } else {
                "Pause"
            }
            .to_string(),
        )
    });

    let on_click = Callback::new(move |()| {
        if state.eof_reached.get() {
            spawn_local(async move {
                let _ = commands::seek(0.0, SeekMode::Absolute, SeekPrecision::Exact).await;
                let _ = commands::play().await;
            });
        } else {
            let next = !state.paused.get();
            spawn_local(async move {
                let _ = commands::set_pause(next).await;
            });
        }
    });

    let disabled = Signal::derive(move || state.duration.get() <= 0.0);

    view! {
        <IconButton
            icon=icon
            tooltip=tooltip
            shortcut=Signal::derive(|| Some("Space".to_string()))
            disabled=disabled
            on_click=on_click
        />
    }
}

#[component]
fn SeekBackButton() -> impl IntoView {
    let on_click = Callback::new(move |()| {
        spawn_local(async move {
            let _ = commands::seek(-SEEK_STEP, SeekMode::Relative, SeekPrecision::Exact).await;
        });
    });
    view! {
        <IconButton
            icon=Signal::derive(|| "public/icons/seek-backward.svg".to_string())
            tooltip=Signal::derive(|| Some("Skip Backward".to_string()))
            shortcut=Signal::derive(|| Some("Left".to_string()))
            on_click=on_click
        />
    }
}

#[component]
fn SeekForwardButton() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let on_click = Callback::new(move |()| {
        spawn_local(async move {
            let _ = commands::seek(SEEK_STEP, SeekMode::Relative, SeekPrecision::Exact).await;
        });
    });
    let disabled = Signal::derive(move || state.eof_reached.get());
    view! {
        <IconButton
            icon=Signal::derive(|| "public/icons/seek-forward.svg".to_string())
            tooltip=Signal::derive(|| Some("Skip Forward".to_string()))
            shortcut=Signal::derive(|| Some("Right".to_string()))
            disabled=disabled
            on_click=on_click
        />
    }
}

#[component]
fn PreviousButton() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let disabled = Signal::derive(move || state.playlist_pos.get().unwrap_or(0) <= 0);
    let on_click = Callback::new(move |()| {
        spawn_local(async move {
            let _ = commands::playlist_prev().await;
            let _ = commands::play().await;
        });
    });
    view! {
        <IconButton
            icon=Signal::derive(|| "public/icons/step-backward.svg".to_string())
            tooltip=Signal::derive(|| Some("Previous".to_string()))
            shortcut=Signal::derive(|| Some("Ctrl + Left".to_string()))
            disabled=disabled
            on_click=on_click
        />
    }
}

#[component]
fn NextButton() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let disabled = Signal::derive(move || state.is_last_video());
    let on_click = Callback::new(move |()| {
        spawn_local(async move {
            let _ = commands::playlist_next().await;
            let _ = commands::play().await;
        });
    });
    view! {
        <IconButton
            icon=Signal::derive(|| "public/icons/step-forward.svg".to_string())
            tooltip=Signal::derive(|| Some("Next".to_string()))
            shortcut=Signal::derive(|| Some("Ctrl + Right".to_string()))
            disabled=disabled
            on_click=on_click
        />
    }
}

// ============================================================================
// End cluster — overflow candidates
// ============================================================================

#[component]
fn PanscanButton(row: Row) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();

    let icon = Signal::derive(move || {
        if state.panscan.get() > 0.5 {
            "public/icons/compress-alt.svg"
        } else {
            "public/icons/expand-alt.svg"
        }
        .to_string()
    });
    let tooltip = Signal::derive(move || {
        Some(if state.panscan.get() > 0.5 { "Fit" } else { "Cover" }.to_string())
    });
    let on_click = Callback::new(move |()| {
        let next = if state.panscan.get() > 0.5 { 0.0 } else { 1.0 };
        spawn_local(async move {
            let _ = commands::set_panscan(next).await;
        });
    });

    let hidden = move || reflow.hidden(row, Side::End, "panscan");

    view! {
        <div class="reflow-wrapper" class:hidden=hidden>
            <IconButton
                icon=icon
                tooltip=tooltip
                shortcut=Signal::derive(|| Some("T".to_string()))
                on_click=on_click
            />
        </div>
    }
}

#[component]
fn FullscreenButton(row: Row) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();

    let icon = Signal::derive(move || {
        if state.fullscreen.get() {
            "public/icons/compress.svg"
        } else {
            "public/icons/expand.svg"
        }
        .to_string()
    });
    let tooltip = Signal::derive(move || {
        Some(if state.fullscreen.get() { "Exit Fullscreen" } else { "Fullscreen" }.to_string())
    });
    let on_click = Callback::new(move |()| {
        spawn_local(async move {
            if state.fullscreen.get_untracked() {
                exit_fullscreen(state).await;
            } else {
                enter_fullscreen(state).await;
            }
        });
    });

    let hidden = move || reflow.hidden(row, Side::End, "fullscreen");

    view! {
        <div class="reflow-wrapper" class:hidden=hidden>
            <IconButton
                icon=icon
                tooltip=tooltip
                shortcut=Signal::derive(|| Some("F".to_string()))
                on_click=on_click
            />
        </div>
    }
}

// ============================================================================
// Open menu — start side, overflow candidate
// ============================================================================

#[component]
fn OpenAnchor(row: Row) -> impl IntoView {
    use crate::components::Menu;
    use leptos::html;

    let reflow = expect_context::<Reflow>();
    let hidden = move || reflow.hidden(row, Side::Start, "open");

    let open = RwSignal::new(false);
    let btn_ref = NodeRef::<html::Button>::new();
    let toggle = move |_| open.update(|v| *v = !*v);

    let pick_file = move |_| {
        open.set(false);
        spawn_local(async move {
            let _ = commands::open_video_dialog().await;
        });
    };
    let pick_folder = move |_| {
        open.set(false);
        spawn_local(async move {
            let _ = commands::open_folder_dialog().await;
        });
    };

    view! {
        <div class="open-anchor" class:hidden=hidden>
            <button
                node_ref=btn_ref
                class="icon-button"
                on:click=toggle
            >
                <img src="public/icons/folder-open.svg" alt="Open" />
                <div class="tooltip">
                    <span class="tooltip-text">"Open"</span>
                </div>
            </button>
            <Menu open=open anchor=btn_ref class="open-menu".to_string()>
                <div class="menu-item" on:click=pick_file>
                    <span class="text">"Open file…"</span>
                    <span class="shortcut">"Ctrl+O"</span>
                </div>
                <div class="menu-item" on:click=pick_folder>
                    <span class="text">"Open folder…"</span>
                    <span class="shortcut">"Ctrl+Shift+O"</span>
                </div>
            </Menu>
        </div>
    }
}
