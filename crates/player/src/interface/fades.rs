//! Where a scrolling list's rows run out, and the bar's title with them: the
//! strips the pipeline softens — and the angle in a list's end that points
//! back to its current row, which the pipeline draws after them.
//!
//! Each list publishes where it is and how far it is from each of its ends
//! through `scroll-fades`; this turns that into the strips `gpu::fade` draws
//! and the angles `gpu::way` draws, in physical pixels and cut to the panel
//! the list is in.

use slint::{ComponentHandle, Model};

use crate::gpu::fade::{Edge, Strip};
use crate::gpu::way::Way;
use crate::{MainWindow, Paint};

/// How tall a strip is, in logical pixels. `Rows.fade` in the interface is
/// the same number, there to say how far a list has to move before a strip
/// is fully on.
const BAND: f32 = 28.0;
/// Most blur, at the list's very edge: a standard deviation in logical
/// pixels. Enough that a row's words are a smear before they are gone, and
/// small enough that they are still a row's worth of smear.
const SIGMA: f32 = 2.0;

/// Read the strips and the angles out of the interface, into `out` and
/// `ways`.
pub fn collect(ui: &MainWindow, out: &mut Vec<Strip>, ways: &mut Vec<Way>) {
    let dpi = ui.window().scale_factor();
    out.clear();
    ways.clear();
    let paint = ui.global::<Paint>();
    let alpha = |c: slint::Color| f32::from(c.alpha()) / 255.0;
    let (wash, rest, bright) = (
        alpha(paint.get_wash_button()),
        alpha(paint.get_body()),
        alpha(paint.get_lit()),
    );
    let lists = ui.get_scroll_fades();
    for i in 0..lists.row_count() {
        let Some(list) = lists.row_data(i) else {
            continue;
        };
        if list.width <= 0.0 || list.height <= 0.0 {
            continue;
        }
        let px = |logical: f32| (logical * dpi).round() as i32;
        // Outward to whole pixels. A title mid-resize, or a page mid-slide,
        // ends partway across a pixel; rounded to the nearest, the strip
        // stopped short of that pixel half the time and left a sliver of
        // unsoftened words standing at its edge.
        let before = |logical: f32| (logical * dpi).floor() as i32;
        let after = |logical: f32| (logical * dpi).ceil() as i32;
        // Sideways and up and down, only what is inside the panel: a page
        // sliding in is half outside it, and there is nothing of the list
        // there to soften — only the film.
        let left = before(list.x).max(before(list.clip_x));
        let right = after(list.x + list.width).min(after(list.clip_x + list.clip_width));
        let top = before(list.y).max(before(list.clip_y));
        let bottom = after(list.y + list.height).min(after(list.clip_y + list.clip_height));
        if right <= left || bottom <= top {
            continue;
        }
        if list.way > 0.004 {
            ways.push(Way {
                x: list.way_x * dpi,
                y: list.way_y * dpi,
                up: list.way_up,
                on: list.way,
                lit: list.way_lit,
                clip: [
                    before(list.clip_x),
                    before(list.clip_y),
                    after(list.clip_x + list.clip_width),
                    after(list.clip_y + list.clip_height),
                ],
                scale: dpi,
                wash,
                rest,
                bright,
            });
        }
        let sigma = SIGMA * dpi;
        // How deep a strip is: all of `BAND` once that much has gone past the
        // edge, and until then only as deep as what has. Its outer edge is
        // always where things are gone entirely. Thinning the whole strip in
        // instead left that edge half there while it arrived — the cut ends
        // of the words standing in it as a bright sliver.
        let deep = |strength: f32, most: i32| {
            ((px(BAND) as f32 * strength.clamp(0.0, 1.0)).ceil() as i32).min(most)
        };
        let ends = [(Edge::Top, list.top), (Edge::Bottom, list.bottom)];
        for (edge, strength) in ends {
            // Two strips never meet in the middle of a list too short for both.
            let band = deep(strength, (bottom - top) / 2);
            if band <= 0 {
                continue;
            }
            out.push(Strip {
                left,
                right,
                top: if edge == Edge::Top {
                    top
                } else {
                    bottom - band
                },
                bottom: if edge == Edge::Top {
                    top + band
                } else {
                    bottom
                },
                edge,
                room: bottom - top - band,
                strength,
                sigma,
            });
        }
        // A line of words running out: the same strip, on its side.
        let band = deep(list.right, (right - left) / 2);
        if band > 0 {
            out.push(Strip {
                left: right - band,
                right,
                top,
                bottom,
                edge: Edge::Right,
                room: right - left - band,
                strength: list.right,
                sigma,
            });
        }
    }
}
