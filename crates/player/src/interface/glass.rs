//! Where the glass goes: each surface's rect, read back out of the layout.
//!
//! Slint owns layout and content; the pipeline owns material. Every surface
//! that wants glass under it publishes its geometry through `glass-rects`,
//! and this hands that to the pipeline once a frame.

use slint::{ComponentHandle, Model};

use crate::gpu::pipeline::{GlassPanel, MAX_PANELS};
use crate::MainWindow;

/// Read the glass panel geometry out of Slint's own layout.
///
/// Same process, same frame, read synchronously — so the glass cannot lag the
/// widget it belongs to. Slint works in logical pixels and the pipeline in
/// physical ones, hence the scale factor.
pub fn collect(ui: &MainWindow, out: &mut Vec<GlassPanel>) {
    let dpi = ui.window().scale_factor();
    out.clear();
    let rects = ui.get_glass_rects();
    for i in 0..rects.row_count() {
        let Some(g) = rects.row_data(i) else { continue };
        // A hidden panel publishes a zero-sized rect rather than dropping out
        // of the array, which keeps the UI side a plain literal. Skip those.
        // A panel mid-fade still needs glass; one faded out entirely is as
        // absent as one that was never opened.
        if g.width <= 0.0 || g.height <= 0.0 || g.opacity <= 0.004 {
            continue;
        }
        // The cap counts panels that are really on screen, not how far into
        // the declaration list we have read. Capping the read instead meant
        // that once the list grew past `MAX_PANELS` — which it did the moment
        // the timeline became two pills — whatever was declared last silently
        // got no glass, however few panels were actually open. Every entry
        // past the live ones is a placeholder for something closed.
        if out.len() == MAX_PANELS {
            warn_overflow();
            return;
        }
        out.push(GlassPanel {
            rect: [
                g.x * dpi,
                g.y * dpi,
                (g.x + g.width) * dpi,
                (g.y + g.height) * dpi,
            ],
            radius: g.radius * dpi,
            tint_alpha: g.tint,
            opacity: g.opacity,
        });
    }
}

/// Said once, not once a frame. Losing glass off the end of the list is quiet
/// enough that it wants saying at all, and repeating it sixty times a second
/// would bury everything else.
fn warn_overflow() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        eprintln!("dbm: more than {MAX_PANELS} glass panels on screen; the rest get none");
    });
}
