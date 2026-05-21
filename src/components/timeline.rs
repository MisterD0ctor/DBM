//! Seek track with scrubbing. Drives mpv's `seek` while the user drags,
//! pausing playback for the duration of the scrub (matching the JS player).
//!
//! Window-level `mousemove` / `mouseup` listeners are installed once on
//! mount and gated on the `scrubbing` / `hovering` signals — cheaper than
//! attach/detach per drag.

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{SeekMode, SeekPrecision};

use crate::bridge::commands;
use crate::components::SeekPreview;
use crate::state::PlayerState;
use crate::util::format_time::format_time;

#[component]
pub fn Timeline() -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let track_ref = NodeRef::<html::Div>::new();

    let scrubbing = RwSignal::new(false);
    let was_playing = RwSignal::new(false);
    let hovering = RwSignal::new(false);
    // 0..100 — cursor position over the track (for tooltip + scrubbing fill).
    let cursor_pct = RwSignal::new(0.0_f64);

    // Map a window-space clientX to a 0..100 percent along the track.
    let pct_at = move |client_x: f64| -> Option<f64> {
        let el = track_ref.get_untracked()?;
        let rect = el.get_bounding_client_rect();
        let w = rect.width();
        if w <= 0.0 {
            return None;
        }
        let x = (client_x - rect.left()).clamp(0.0, w);
        Some((x / w) * 100.0)
    };

    let seek_to_pct = move |pct: f64| {
        spawn_local(async move {
            let _ = commands::seek(pct, SeekMode::AbsolutePercent, SeekPrecision::Exact).await;
        });
    };

    let on_mousedown = move |ev: ev::MouseEvent| {
        if ev.button() != 0 {
            return;
        }
        let Some(pct) = pct_at(ev.client_x() as f64) else {
            return;
        };
        was_playing.set(!state.paused.get_untracked());
        scrubbing.set(true);
        cursor_pct.set(pct);
        if was_playing.get_untracked() {
            spawn_local(async move {
                let _ = commands::pause().await;
            });
        }
        seek_to_pct(pct);
    };

    // Track hover — show tooltip without scrubbing.
    let on_track_mousemove = move |ev: ev::MouseEvent| {
        if scrubbing.get_untracked() {
            return; // window listener handles drag positioning
        }
        if let Some(pct) = pct_at(ev.client_x() as f64) {
            cursor_pct.set(pct);
        }
    };
    let on_mouseenter = move |_| hovering.set(true);
    let on_mouseleave = move |_| hovering.set(false);

    // Window-level drag handling — installed once, gated on `scrubbing`.
    let move_handle = window_event_listener(ev::mousemove, move |ev: ev::MouseEvent| {
        if !scrubbing.get_untracked() {
            return;
        }
        if let Some(pct) = pct_at(ev.client_x() as f64) {
            cursor_pct.set(pct);
            seek_to_pct(pct);
        }
    });

    let up_handle = window_event_listener(ev::mouseup, move |ev: ev::MouseEvent| {
        if ev.button() != 0 || !scrubbing.get_untracked() {
            return;
        }
        scrubbing.set(false);
        if was_playing.get_untracked() {
            spawn_local(async move {
                let _ = commands::play().await;
            });
        }
    });

    on_cleanup(move || {
        move_handle.remove();
        up_handle.remove();
    });

    // Optimistic fill: while scrubbing, follow the cursor; otherwise follow
    // mpv's reported position so the fill stays in sync without a round trip.
    let fill_pct = move || {
        if scrubbing.get() {
            cursor_pct.get()
        } else {
            state.percent_pos.get()
        }
    };

    // Hover/scrub state drives the two-tone gradient on rail/fill/thumb.
    let active = move || hovering.get() || scrubbing.get();

    // Where the hover cursor splits the seek-fill, expressed as a percent of
    // the *fill's own width* (the fill spans 0..fill_pct of the track). When
    // the cursor is at or past the playhead, the entire visible fill is the
    // "behind" portion (hot).
    let fill_split = move || {
        let f = fill_pct();
        let c = cursor_pct.get();
        if f <= 0.0 || c >= f { 100.0 } else { (c / f) * 100.0 }
    };

    let track_style = move || {
        format!(
            "--cursor-pct: {}%; --fill-split: {}%;",
            cursor_pct.get(),
            fill_split(),
        )
    };

    // Thumb sits at fill_pct; when the hover cursor is to its left, the whole
    // thumb is on the "ahead" side and flips to the cold tone.
    let thumb_cold = move || active() && cursor_pct.get() < fill_pct();

    let show_tooltip = move || scrubbing.get() || hovering.get();
    let tooltip_time = move || {
        let dur = state.duration.get();
        if dur <= 0.0 {
            "0:00".into()
        } else {
            format_time(dur * cursor_pct.get() / 100.0, 0)
        }
    };

    view! {
        <div class="timeline">
            <span class="time-display">
                {move || format_time(state.time_pos.get(), 0)}
            </span>
            <div
                class="seek-track"
                class:hovering=active
                node_ref=track_ref
                style=track_style
                on:mousedown=on_mousedown
                on:mouseenter=on_mouseenter
                on:mouseleave=on_mouseleave
                on:mousemove=on_track_mousemove
            >
                <div class="seek-rail"></div>
                <div
                    class="seek-fill"
                    style:width=move || format!("{}%", fill_pct())
                ></div>
                <div
                    class="seek-thumb"
                    class:cold=thumb_cold
                    style:left=move || format!("{}%", fill_pct())
                ></div>
                <div
                    class="seek-tooltip"
                    class:hidden=move || !show_tooltip()
                    style:left=move || format!("{}%", cursor_pct.get())
                >
                    <SeekPreview cursor_pct=cursor_pct/>
                    <span class="tooltip-text">{tooltip_time}</span>
                </div>
            </div>
            <span class="time-display">
                {move || format_time(state.duration.get(), 0)}
            </span>
        </div>
    }
}
