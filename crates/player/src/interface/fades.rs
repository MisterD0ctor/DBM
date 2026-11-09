//! Where a scrolling list's rows run out, and the bar's title with them: the
//! strips the pipeline softens, and the angle in a list's end that points
//! back to its current row, which the pipeline draws after them.
//!
//! Each list publishes where it is and how far it is from each of its ends
//! through `scroll-fades`; this turns that into the strips `gpu::fade` draws
//! and the angles `gpu::way` draws, in physical pixels and cut to the panel
//! the list is in.

use dbm_gpu::fade::{Edge, Strip};
use dbm_gpu::rect::Region;
use dbm_gpu::way::Way;
use slint::{ComponentHandle, Model};

use crate::{MainWindow, Paint, Rows};

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
    // How deep a strip comes into its list, past the list's reach beyond that
    // end (`ScrollFade.top-reach`): read rather than copied, since the
    // interface keeps the ring this far ahead.
    let fade = ui.global::<Rows>().get_fade();
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
        // Outward to whole pixels, since a list mid-slide ends partway across
        // a pixel, and rounding to the nearest could leave a sliver of
        // unsoftened words at its edge.
        let before = |logical: f32| (logical * dpi).floor() as i32;
        let after = |logical: f32| (logical * dpi).ceil() as i32;
        // Only what is inside the panel, since a page sliding in is half
        // outside it, where there is only the film.
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
                clip: Region::new(
                    before(list.clip_x),
                    before(list.clip_y),
                    after(list.clip_x + list.clip_width),
                    after(list.clip_y + list.clip_height),
                ),
                scale: dpi,
                wash,
                rest,
                bright,
            });
        }
        let sigma = SIGMA * dpi;
        // How deep a strip is: the list's reach past that end, and as much of
        // `fade` into the list as the strip has come on. Its outer edge is
        // fully gone however shallow it is; fading the whole strip in instead
        // would leave the cut ends of words there as a bright sliver. At rest
        // nothing is in the reach yet, so there is no strip to draw.
        let deep = |strength: f32, reach: f32, most: i32| {
            if strength <= 0.0 {
                return 0;
            }
            let inside = (px(fade) as f32 * strength.min(1.0)).ceil() as i32;
            (px(reach.max(0.0)) + inside).min(most)
        };
        let ends = [
            (Edge::Top, list.top, list.top_reach),
            (Edge::Bottom, list.bottom, list.bottom_reach),
        ];
        for (edge, strength, reach) in ends {
            // Two strips never meet in the middle of a list too short for both.
            let band = deep(strength, reach, (bottom - top) / 2);
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
        let band = deep(list.right, 0.0, (right - left) / 2);
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
