//! Playlist button + popover. Renders the current playlist with active /
//! playing indicators; clicking an item jumps to it.
//!
//! Title formatting reuses [`parse_tv_show`] so TV files get nicely split
//! into show / episode / title; everything else falls back to the cleaned
//! plain title.

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::PlaylistEntry;

use crate::bridge::commands;
use crate::components::toolbar::{Reflow, Row, Side};
use crate::components::Menu;
use crate::state::PlayerState;
use crate::util::parse::{clean_separators, parse_tv_show, strip_extension, strip_metadata, strip_path};

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
                <img src="public/icons/playlist.svg" alt="Playlist" />
                <div class="tooltip">
                    <span class="tooltip-text">"Playlist"</span>
                </div>
            </button>
            <Menu open=open anchor=btn_ref class="playlist-menu".to_string()>
                <div class="menu-heading">
                    <span>"Playlist"</span>
                    <img src="public/icons/playlist.svg" alt="" />
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
                <Show when=move || progress_pct.get().is_some()>
                    <div class="playlist-progress">
                        <div
                            class="playlist-progress-fill"
                            style:width=move || {
                                format!("{}%", progress_pct.get().unwrap_or(0.0))
                            }
                        ></div>
                    </div>
                </Show>
            </div>
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
                <span class="playlist-episode">{ep}</span>
                {tv.title.map(|t| view! { <span class="playlist-episode-title">{t}</span> })}
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
