//! Where the glass goes: each surface's rect, read back out of the layout.
//!
//! Slint owns layout and content; the pipeline owns material. Every surface
//! that wants glass under it publishes its geometry through `glass-rects`,
//! and this hands that to the pipeline once a frame.

use dbm_gpu::pipeline::{GlassPanel, MAX_PANELS, OPEN_MILK, RING_MILK};
use dbm_gpu::rect::{Logical, Rect};
use slint::{ComponentHandle, Model};

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
        let mut rect = Rect::<Logical>::logical(g.x, g.y, g.x + g.width, g.y + g.height);
        let mut opacity = g.opacity;
        if g.ring {
            let surface = (g.host_height > 0.0).then_some(Band {
                top: g.host_y,
                bottom: g.host_y + g.host_height,
            });
            (rect, opacity) = within(rect, opacity, surface, &ui.get_scroll_fades());
            if opacity <= 0.004 {
                continue;
            }
        }
        out.push(GlassPanel {
            rect: rect.scaled(dpi),
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

/// A stretch of the window from `top` down to `bottom`, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    top: f32,
    bottom: f32,
}

impl Band {
    /// What of this is also in `other`; upside down where nothing is.
    fn within(self, other: Self) -> Self {
        Self {
            top: self.top.max(other.top),
            bottom: self.bottom.min(other.bottom),
        }
    }

    fn height(self) -> f32 {
        self.bottom - self.top
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
    rect: Rect<Logical>,
    opacity: f32,
    surface: Option<Band>,
    lists: &impl Model<Data = ScrollFade>,
) -> (Rect<Logical>, f32) {
    let (centre, height) = (rect.centre().0, rect.height());
    if height <= 0.0 {
        return (rect, opacity);
    }
    let pane = Band {
        top: rect.y0,
        bottom: rect.y1,
    };
    // The list the row is most inside, the first of any that tie.
    let list = lists
        .iter()
        .filter(|list| list.width > 0.0 && list.height > 0.0)
        .filter(|list| centre >= list.x && centre <= list.x + list.width)
        .map(|list| {
            pane.within(Band {
                top: list.y,
                bottom: list.y + list.height,
            })
        })
        .filter(|inside| inside.height() > 0.0)
        .reduce(|most, next| {
            if next.height() > most.height() {
                next
            } else {
                most
            }
        });
    let mut cut = surface.unwrap_or(pane);
    if let Some(list) = list {
        cut = cut.within(list);
    }
    let cut = pane.within(cut);
    if cut.top <= pane.top && cut.bottom >= pane.bottom {
        return (rect, opacity);
    }
    let inside = (cut.height() / height).max(0.0);
    (
        Rect::<Logical>::logical(rect.x0, cut.top, rect.x1, cut.bottom.max(cut.top)),
        opacity * (2.0 * inside - 1.0).clamp(0.0, 1.0),
    )
}

/// Said once rather than every frame, which would bury everything else.
fn warn_overflow() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        log::warn!("more than {MAX_PANELS} glass panels on screen; the rest get none");
    });
}

#[cfg(test)]
mod tests {
    use super::{Band, within};
    use crate::ScrollFade;
    use dbm_gpu::rect::{Logical, Rect};

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
        let rect = Rect::<Logical>::logical(40.0, 150.0, 340.0, 184.0);
        assert_eq!(within(rect, 1.0, None, &list()), (rect, 1.0));
    }

    #[test]
    fn a_row_half_out_of_its_list_has_no_pane_left() {
        let rect = Rect::<Logical>::logical(40.0, 83.0, 340.0, 117.0);
        let (cut, opacity) = within(rect, 1.0, None, &list());
        assert_eq!(cut, Rect::<Logical>::logical(40.0, 100.0, 340.0, 117.0));
        assert_eq!(opacity, 0.0);
    }

    #[test]
    fn a_row_leaving_its_list_fades_as_it_goes() {
        let rect = Rect::<Logical>::logical(40.0, 275.0, 340.0, 309.0);
        let (cut, opacity) = within(rect, 0.5, None, &list());
        assert_eq!(cut.y1, 300.0);
        assert!(opacity > 0.0 && opacity < 0.5, "{opacity}");
    }

    #[test]
    fn a_row_outside_every_list_is_left_whole() {
        let rect = Rect::<Logical>::logical(40.0, 40.0, 340.0, 74.0);
        assert_eq!(within(rect, 1.0, None, &list()), (rect, 1.0));
    }

    /// A panel from y 20 to 360.
    const PANEL: Band = Band {
        top: 20.0,
        bottom: 360.0,
    };

    /// That panel, the list inside it, and a row under the list, as the
    /// subtitles menu's way to a page is.
    #[test]
    fn a_row_below_its_panel_has_no_pane() {
        let (_, opacity) = within(
            Rect::<Logical>::logical(40.0, 380.0, 340.0, 414.0),
            1.0,
            Some(PANEL),
            &list(),
        );
        assert_eq!(opacity, 0.0);
    }

    #[test]
    fn a_row_rising_into_its_panel_fades_in() {
        let (cut, opacity) = within(
            Rect::<Logical>::logical(40.0, 335.0, 340.0, 369.0),
            1.0,
            Some(PANEL),
            &list(),
        );
        assert_eq!(cut, Rect::<Logical>::logical(40.0, 335.0, 340.0, 360.0));
        assert!(opacity > 0.0 && opacity < 1.0, "{opacity}");
    }

    #[test]
    fn a_row_inside_its_panel_and_list_keeps_its_pane() {
        let rect = Rect::<Logical>::logical(40.0, 150.0, 340.0, 184.0);
        assert_eq!(within(rect, 1.0, Some(PANEL), &list()), (rect, 1.0));
    }
}
