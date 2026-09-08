//! Top-level player layout. Provides `PlayerState` + `ActionFeedback` via
//! context, hydrates from the backend, installs event listeners + keyboard
//! shortcuts + overlay-visibility tracking, and renders the full player tree.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::bridge::{commands, events};
use crate::components::{
    ActionFeedback, ActionOverlay, EndOfPlayback, Reflow, Timeline, ToolbarMainRow,
    ToolbarOverflowRow, VideoSurface,
};
use crate::state::PlayerState;
use crate::util::dragdrop::install_drag_drop;
use crate::util::keyboard::install_shortcuts;
use crate::util::overlay::install_overlay_visibility;
use crate::util::tooltip::install_tooltip_clamp;

#[component]
pub fn App() -> impl IntoView {
    let state = PlayerState::new();
    let feedback = ActionFeedback::new();
    let reflow = Reflow::new();
    provide_context(state);
    provide_context(feedback);
    provide_context(reflow);

    spawn_local(async move {
        // Listeners FIRST, snapshot SECOND. The backend's `startup` triggers
        // the previous session's file load before the frontend mounts, so
        // mpv's "file loaded" property events can fire while we're still
        // bootstrapping. If we snapshot before installing listeners, those
        // events arrive at a backend with no subscriber and are dropped —
        // the UI then sits at whatever partial state mpv exposed mid-load.
        events::install_listeners(state).await;
        match commands::snapshot().await {
            Ok(snap) => state.hydrate(snap),
            Err(e) => leptos::logging::warn!("snapshot failed: {e}"),
        }

        // Snapshot doesn't include watch-later progress; refresh it now that
        // we know which files are in the playlist.
        let paths: Vec<String> = state
            .playlist
            .with_untracked(|p| p.iter().map(|e| e.filename.clone()).collect());
        if !paths.is_empty() {
            if let Ok(progress) = commands::get_watch_later_positions(paths).await {
                state.playlist_progress.set(progress);
            }
        }

        // The initial path was set via `hydrate` (or by listeners) and may
        // not have gone through the property path — fetch its preview here.
        if let Some(path) = state.path.get_untracked() {
            if let Ok(Some(ready)) = commands::get_preview(path).await {
                state.preview_sprite.set(Some(ready));
            }
        }

        install_drag_drop().await;
    });

    install_shortcuts(state, feedback);
    install_overlay_visibility(state);
    install_tooltip_clamp();

    view! {
        <div
            class="player"
            class:controls-hidden = move || !state.controls_visible.get()
            class:fullscreen = move || state.fullscreen.get()
        >
            <VideoSurface/>
            <ActionOverlay/>
            <EndOfPlayback/>
            <div class="controls-panel">
                <ToolbarOverflowRow/>
                <Timeline/>
                <ToolbarMainRow/>
            </div>
        </div>
    }
}
