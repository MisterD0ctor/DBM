//! Sprite-tile thumbnail shown above the seek tooltip.
//!
//! Reads the sprite atlas + per-tile dimensions from `state.preview_sprite`
//! (populated by the backend's ffmpeg pipeline). The cursor's 0..100
//! percent along the seek track maps into one of `grid * grid` tiles; the
//! `<img>` inside this clip-box gets translated so just that tile is
//! visible.

use leptos::prelude::*;

use crate::bridge::commands::convert_file_src;
use crate::state::PlayerState;

#[component]
pub fn SeekPreview(cursor_pct: RwSignal<f64>) -> impl IntoView {
    let state = expect_context::<PlayerState>();

    let sprite_url = move || {
        state
            .preview_sprite
            .with(|s| s.as_ref().map(|s| convert_file_src(&s.sprite)))
    };

    // (tile_w, tile_h, grid) — pulled together so the multi-stage style
    // closures don't have to read the signal three times.
    let dims = move || {
        state
            .preview_sprite
            .with(|s| s.as_ref().map(|s| (s.tile_w, s.tile_h, s.grid)))
    };

    let tile_offset = move || -> (u32, u32) {
        let Some((tile_w, tile_h, grid)) = dims() else {
            return (0, 0);
        };
        let total = grid * grid;
        if total == 0 {
            return (0, 0);
        }
        let frac = (cursor_pct.get() / 100.0).clamp(0.0, 1.0);
        let idx = ((frac * total as f64).floor() as u32).min(total - 1);
        let col = idx % grid;
        let row = idx / grid;
        (col * tile_w, row * tile_h)
    };

    view! {
        <Show when=move || sprite_url().is_some()>
            <div
                class="seek-preview"
                style:width=move || dims().map(|(w, _, _)| format!("{w}px")).unwrap_or_default()
                style:height=move || dims().map(|(_, h, _)| format!("{h}px")).unwrap_or_default()
            >
                <img
                    src=move || sprite_url().unwrap_or_default()
                    alt=""
                    style:transform=move || {
                        let (x, y) = tile_offset();
                        format!("translate(-{x}px, -{y}px)")
                    }
                />
            </div>
        </Show>
    }
}
