//! Where the glass goes: each surface's rect, read back out of the layout.
//!
//! Slint owns layout and content; the pipeline owns material. Every surface
//! that wants glass under it publishes its geometry through `glass-rects`,
//! and this hands that to the pipeline once a frame.

use slint::{ComponentHandle, Model};

use crate::gpu::pipeline::{GlassPanel, MAX_PANELS, OPEN_MILK, RING_MILK};
use crate::{MainWindow, ScrollFade};

/// Read the glass panel geometry out of Slint's own layout.
///
/// Read synchronously in the same frame, so the glass can't lag its widget.
/// Slint works in logical pixels and the pipeline in physical ones, hence the
/// scale factor.
pub fn collect(ui: &MainWindow, out: &mut Vec<GlassPanel>) {
    let dpi = ui.window().scale_factor();
    out.clear();
    let rects = ui.get_glass_rects();
    for i in 0..rects.row_count() {
        let Some(g) = rects.row_data(i) else { continue };
        // A hidden panel publishes a zero-sized rect rather than dropping out
        // of the array, which keeps the UI side a plain literal. A panel
        // mid-fade still needs glass; one faded out entirely does not.
        if g.width <= 0.0 || g.height <= 0.0 || g.opacity <= 0.004 {
            continue;
        }
        // The cap counts panels actually on screen, not entries read: the
        // list declares more surfaces than `MAX_PANELS`, most of them closed
        // placeholders, and capping the read would drop the last ones.
        if out.len() == MAX_PANELS {
            warn_overflow();
            return;
        }
        let mut rect = [g.x, g.y, g.x + g.width, g.y + g.height];
        let mut opacity = g.opacity;
        if g.ring {
            let surface = (g.host_height > 0.0).then_some((g.host_y, g.host_y + g.host_height));
            (rect, opacity) = within(rect, opacity, surface, &ui.get_scroll_fades());
            if opacity <= 0.004 {
                continue;
            }
        }
        out.push(GlassPanel {
            rect: rect.map(|v| v * dpi),
            radius: g.radius * dpi,
            tint_alpha: g.tint,
            opacity,
            nested: g.nested,
            ring: g.ring,
            milk: if g.ring {
                RING_MILK
            } else if g.open {
                OPEN_MILK
            } else {
                0.0
            },
        });
    }
}

/// The pane under the ring, cut to the surface its row is in, given as its
/// top and bottom, and to the scrolling list it is in there.
///
/// The list clips its rows, and the glass under one would otherwise go on past
/// the list's end into the heading above it. So does the surface, as a page
/// rises into a panel growing to fit it, and the glass under a row still below
/// the panel would lie on the bar. The pane fades as its row leaves, and is
/// gone by the time half the row is, so a cut pane is never more than a
/// glimpse. A row inside its surface and outside every list, like a page's
/// way back, is left whole.
fn within(
    rect: [f32; 4],
    opacity: f32,
    surface: Option<(f32, f32)>,
    lists: &impl Model<Data = ScrollFade>,
) -> ([f32; 4], f32) {
    let centre = 0.5 * (rect[0] + rect[2]);
    let height = rect[3] - rect[1];
    if height <= 0.0 {
        return (rect, opacity);
    }
    let (mut top, mut bottom) = surface.unwrap_or((rect[1], rect[3]));
    let mut best: Option<(f32, f32, f32)> = None;
    for list in lists.iter() {
        if list.width <= 0.0 || list.height <= 0.0 {
            continue;
        }
        if centre < list.x || centre > list.x + list.width {
            continue;
        }
        let (top, bottom) = (rect[1].max(list.y), rect[3].min(list.y + list.height));
        let inside = (bottom - top) / height;
        if inside > 0.0 && best.is_none_or(|(most, _, _)| inside > most) {
            best = Some((inside, top, bottom));
        }
    }
    if let Some((_, list_top, list_bottom)) = best {
        (top, bottom) = (top.max(list_top), bottom.min(list_bottom));
    }
    let (top, bottom) = (rect[1].max(top), rect[3].min(bottom));
    if top <= rect[1] && bottom >= rect[3] {
        return (rect, opacity);
    }
    let inside = ((bottom - top) / height).max(0.0);
    (
        [rect[0], top, rect[2], bottom.max(top)],
        opacity * (2.0 * inside - 1.0).clamp(0.0, 1.0),
    )
}

/// Said once rather than every frame, which would bury everything else.
fn warn_overflow() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        eprintln!("dbm: more than {MAX_PANELS} glass panels on screen; the rest get none");
    });
}

#[cfg(test)]
mod tests {
    use super::within;
    use crate::ScrollFade;

    /// A list from y 100 to 300, as wide as its rows' washes.
    fn list() -> slint::VecModel<ScrollFade> {
        slint::VecModel::from(vec![ScrollFade {
            x: 40.0,
            y: 100.0,
            width: 300.0,
            height: 200.0,
            ..Default::default()
        }])
    }

    #[test]
    fn a_row_inside_its_list_keeps_its_pane() {
        let rect = [40.0, 150.0, 340.0, 184.0];
        assert_eq!(within(rect, 1.0, None, &list()), (rect, 1.0));
    }

    #[test]
    fn a_row_half_out_of_its_list_has_no_pane_left() {
        let (cut, opacity) = within([40.0, 83.0, 340.0, 117.0], 1.0, None, &list());
        assert_eq!(cut, [40.0, 100.0, 340.0, 117.0]);
        assert_eq!(opacity, 0.0);
    }

    #[test]
    fn a_row_leaving_its_list_fades_as_it_goes() {
        let (cut, opacity) = within([40.0, 275.0, 340.0, 309.0], 0.5, None, &list());
        assert_eq!(cut[3], 300.0);
        assert!(opacity > 0.0 && opacity < 0.5, "{opacity}");
    }

    #[test]
    fn a_row_outside_every_list_is_left_whole() {
        let rect = [40.0, 40.0, 340.0, 74.0];
        assert_eq!(within(rect, 1.0, None, &list()), (rect, 1.0));
    }

    /// A panel from y 20 to 360, the list inside it, and a row under the
    /// list, as the subtitles menu's way to a page is.
    #[test]
    fn a_row_below_its_panel_has_no_pane() {
        let (_, opacity) = within([40.0, 380.0, 340.0, 414.0], 1.0, Some((20.0, 360.0)), &list());
        assert_eq!(opacity, 0.0);
    }

    #[test]
    fn a_row_rising_into_its_panel_fades_in() {
        let (cut, opacity) =
            within([40.0, 335.0, 340.0, 369.0], 1.0, Some((20.0, 360.0)), &list());
        assert_eq!(cut, [40.0, 335.0, 340.0, 360.0]);
        assert!(opacity > 0.0 && opacity < 1.0, "{opacity}");
    }

    #[test]
    fn a_row_inside_its_panel_and_list_keeps_its_pane() {
        let rect = [40.0, 150.0, 340.0, 184.0];
        assert_eq!(within(rect, 1.0, Some((20.0, 360.0)), &list()), (rect, 1.0));
    }
}
