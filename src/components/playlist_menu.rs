//! Playlist button + popover. Renders the current playlist with active /
//! playing indicators; clicking an item jumps to it.
//!
//! Title formatting reuses [`parse_tv_show`] so TV files get nicely split
//! into show / episode / title; everything else falls back to the cleaned
//! plain title.

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{PlaylistEntry, PreviewReady};

use crate::bridge::commands;
use crate::components::toolbar::{Reflow, Row, Side};
use crate::components::Menu;
use crate::state::PlayerState;
use crate::util::assets::asset;
use crate::util::parse::{
    clean_separators, parse_tv_show, strip_extension, strip_metadata, strip_path,
};

/// Rendered height of a playlist thumbnail (px). Width is derived from the
/// sprite's tile aspect ratio so videos with non-16:9 sources keep their
/// proportions.
const THUMB_H: f64 = 44.0;

#[component]
pub fn PlaylistAnchor(row: Row) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let reflow = expect_context::<Reflow>();
    let open = RwSignal::new(false);
    let btn_ref = NodeRef::<html::Button>::new();

    // Hide the anchor when no playlist is loaded OR when the reflow logic
    // has overflowed it out of this row.
    let hidden = move || {
        state.playlist.with(|p| p.is_empty()) || reflow.hidden(row, Side::Start, "playlist")
    };

    let toggle = Callback::new(move |()| open.update(|v| *v = !*v));

    let entries = move || {
        state
            .playlist
            .get()
            .into_iter()
            .enumerate()
            .collect::<Vec<_>>()
    };

    view! {
        <div class="playlist-anchor" class:hidden=hidden>
            <button
                node_ref=btn_ref
                class="icon-button"
                on:click=move |_| toggle.run(())
            >
                <img src=asset("public/icons/playlist.svg") alt="Playlist" />
                <div class="tooltip">
                    <span class="tooltip-text">"Playlist"</span>
                </div>
            </button>
            <Menu open=open anchor=btn_ref class="playlist-menu".to_string()>
                <div class="menu-heading">
                    <img src=asset("public/icons/playlist.svg") alt="" />
                    <span>"Playlist"</span>
                </div>
                <div class="playlist-items">
                    <For
                        each=entries
                        key=|(i, e)| (*i, e.filename.clone())
                        children=move |(index, entry)| {
                            view! { <PlaylistItem index=index as i64 entry=entry on_select=open/> }
                        }
                    />
                </div>
            </Menu>
        </div>
    }
}

#[component]
fn PlaylistItem(index: i64, entry: PlaylistEntry, on_select: RwSignal<bool>) -> impl IntoView {
    let state = expect_context::<PlayerState>();
    let filename = entry.filename.clone();

    let is_active = move || state.playlist_pos.get() == Some(index);

    let display = display_entry(&entry);

    // Per-item sprite atlas. Fetched once on mount via the same backend
    // command the seek-bar preview uses, so cached entries return
    // immediately and uncached files just stay un-thumbnailed.
    let preview = RwSignal::new(None::<PreviewReady>);
    {
        let path = filename.clone();
        spawn_local(async move {
            if let Ok(Some(p)) = commands::get_preview(path).await {
                preview.set(Some(p));
            }
        });
    }

    // Resume position from the watch-later cache, expressed as a 0..100
    // fraction. Returns None when we don't have both a start and a duration.
    let progress_pct: Memo<Option<f64>> = {
        let path = filename.clone();
        Memo::new(move |_| {
            state.playlist_progress.with(|m| {
                m.get(&path).and_then(|p| {
                    if p.duration > 0.0 && p.start > 0.0 {
                        Some((p.start / p.duration * 100.0).min(100.0))
                    } else {
                        None
                    }
                })
            })
        })
    };

    // Fraction (0..1) used to choose which sprite tile to display. Active
    // entry tracks live playback; other entries point at the resume tile.
    // The `if is_active()` short-circuit means inactive items never
    // subscribe to `percent_pos`, so playback doesn't re-render them.
    let frac = move || -> f64 {
        if is_active() {
            (state.percent_pos.get() / 100.0).clamp(0.0, 1.0)
        } else {
            progress_pct
                .get()
                .map(|p| (p / 100.0).clamp(0.0, 1.0))
                .unwrap_or(0.0)
        }
    };

    // Inline CSS that paints the chosen tile as the thumb's background. We
    // scale the sprite so each tile renders at `THUMB_H × thumb_w`, then
    // shift it so the correct (col,row) lands at (0,0). Returns None when
    // we don't yet have a sprite for this entry.
    let thumb_style = move || -> Option<String> {
        // Prefer the live sprite for the active entry — it's already in
        // state and might be fresher than the disk cache during generation.
        let sprite = if is_active() {
            state
                .preview_sprite
                .with(|s| s.clone())
                .or_else(|| preview.get())
        } else {
            preview.get()
        }?;
        let total = sprite.grid * sprite.grid;
        if total == 0 || sprite.tile_w == 0 || sprite.tile_h == 0 {
            return None;
        }
        let idx = ((frac() * total as f64).floor() as u32).min(total - 1);
        let col = idx % sprite.grid;
        let row = idx / sprite.grid;

        let aspect = sprite.tile_w as f64 / sprite.tile_h as f64;
        let thumb_w = THUMB_H * aspect;
        let url = commands::convert_file_src(&sprite.sprite);

        Some(format!(
            "width: {tw:.2}px; height: {th:.2}px; \
             background-image: url({url}); \
             background-size: {bsw:.2}px {bsh:.2}px; \
             background-position: -{bpx:.2}px -{bpy:.2}px;",
            tw = thumb_w,
            th = THUMB_H,
            url = url,
            bsw = thumb_w * sprite.grid as f64,
            bsh = THUMB_H * sprite.grid as f64,
            bpx = col as f64 * thumb_w,
            bpy = row as f64 * THUMB_H,
        ))
    };

    // Used by both the thumb-overlay and the fallback inline progress bars.
    let progress_bar = move || {
        view! {
            <div class="playlist-progress">
                <div
                    class="playlist-progress-fill"
                    style:width=move || format!("{}%", progress_pct.get().unwrap_or(0.0))
                ></div>
            </div>
        }
    };

    let click = move |_| {
        on_select.set(false);
        spawn_local(async move {
            let _ = commands::playlist_play_index(index).await;
            let _ = commands::play().await;
        });
    };

    view! {
        <div
            class="menu-item playlist-item"
            class:active=is_active
            on:click=click
        >
            <div class="highlight"></div>
            <div class="playlist-indicator">
                <span class="playlist-number">{index + 1}</span>
            </div>
            <div class="playlist-content">
                {display}
                // Fallback inline progress bar — only used while the sprite
                // is still loading (or for files that never get one). Once
                // the thumbnail appears, the bar moves into its bottom edge.
                <Show when=move || thumb_style().is_none() && progress_pct.get().is_some()>
                    {progress_bar()}
                </Show>
            </div>
            {move || thumb_style().map(|s| view! {
                <div class="playlist-thumb" style=s>
                    <Show when=move || progress_pct.get().is_some()>
                        {progress_bar()}
                    </Show>
                </div>
            })}
        </div>
    }
}

fn display_entry(entry: &PlaylistEntry) -> AnyView {
    let filename = entry
        .title
        .as_deref()
        .or(Some(entry.filename.as_str()))
        .unwrap_or("");
    let name = strip_extension(strip_path(filename));

    if let Some(tv) = parse_tv_show(name) {
        let ep = format!("S{:02}:E{:02}", tv.season, tv.episode);
        view! {
            <div class="playlist-content-text">
                <span class="playlist-show">{tv.show}</span>
                <span class="playlist-episode-group">
                    <span class="playlist-episode">{ep}</span>
                    {tv.title.map(|t| view! { <span class="playlist-episode-title">{t}</span> })}
                </span>
            </div>
        }
        .into_any()
    } else {
        let title = strip_metadata(&clean_separators(name));
        view! {
            <div class="playlist-content-text">
                <span class="playlist-title">{title}</span>
            </div>
        }
        .into_any()
    }
}
