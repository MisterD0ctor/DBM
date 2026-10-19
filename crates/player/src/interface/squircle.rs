//! The glass's corner, for the shapes Slint draws on the glass.
//!
//! The pipeline cuts every surface with superelliptic corners: a corner that
//! eases out of the straight edges over `CORNER_SOFTNESS` times its radius,
//! where a circle's turns off them all at once; see `sdg_superbox` in
//! glass.frag. Slint's corners are circles, and a circle stands proud of that
//! corner. At a softness of 3.5, a highlight filling the 48px skip pill ran
//! two pixels past the glass at each end.
//!
//! So the shapes that sit on the glass, or stand in for it, take its outline
//! from here, written as an SVG that Slint renders at the size it is shown.
//! That is how the switches are drawn, for the same reason: femtovg lays a
//! curve of its own down as a few straight chords.
//!
//! Circles are left as circles. On a square box with a radius of half its
//! side the rule below draws a circle anyway, a few per cent small, and no
//! round button sits on a glass circle it would have to match.

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;
use std::fmt::Write;

use crate::gpu::pipeline::CORNER_SOFTNESS as SOFTNESS;

/// The furthest an outline strays from the curve it stands for, in logical
/// pixels: a fiftieth of one, which no scale factor brings into view.
const TOLERANCE: f64 = 0.02;

/// How many distinct shapes are kept before the cache starts again. A window
/// being resized asks for a new size every frame, and nothing else does.
const KEPT: usize = 256;

thread_local! {
    static CACHE: RefCell<HashMap<[i64; 4], slint::Image>> = RefCell::new(HashMap::new());
}

/// A `width` by `height` shape with the glass's corners of `radius`, in white
/// for Slint to colorize.
///
/// `inset` is how far inside a glass surface it sits on every side. It is
/// then that surface's outline moved in, rather than a corner of its own at
/// the smaller radius, so the gap between the two stays even round the
/// corner, as concentric circles' does: this corner is not a circle's, and
/// one of its own narrows the gap at the surface's corner. The end pill's
/// two choices sit in it so. The rows do not, since a row is too short to
/// carry a panel's corner moved in without drawing its ends out.
pub fn image(width: f32, height: f32, radius: f32, inset: f32) -> slint::Image {
    let key = [width, height, radius, inset].map(|v| (f64::from(v) * 64.0).round() as i64);
    if let Some(known) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return known;
    }
    let image = render(
        f64::from(width),
        f64::from(height),
        f64::from(radius),
        f64::from(inset),
    );
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= KEPT {
            cache.clear();
        }
        cache.insert(key, image.clone());
    });
    image
}

fn render(width: f64, height: f64, radius: f64, inset: f64) -> slint::Image {
    if width < 0.5 || height < 0.5 {
        return slint::Image::default();
    }
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\"><path fill=\"#fff\" d=\""
    );
    let (cx, cy) = (width / 2.0, height / 2.0);
    for (i, [x, y]) in outline(width, height, radius, inset)
        .into_iter()
        .enumerate()
    {
        let verb = if i == 0 { 'M' } else { 'L' };
        let _ = write!(svg, "{verb}{:.3} {:.3}", cx + x, cy - y);
    }
    svg.push_str("Z\"/></svg>");
    slint::Image::load_from_svg_data(svg.as_bytes()).unwrap_or_default()
}

/// The whole outline, about the box's centre with y up, once round.
fn outline(width: f64, height: f64, radius: f64, inset: f64) -> Vec<[f64; 2]> {
    let quarter = quarter(width, height, radius, inset);
    let mut all = Vec::with_capacity(quarter.len() * 4);
    all.extend(quarter.iter().copied());
    all.extend(quarter.iter().rev().map(|&[x, y]| [-x, y]));
    all.extend(quarter.iter().map(|&[x, y]| [-x, -y]));
    all.extend(quarter.iter().rev().map(|&[x, y]| [x, -y]));
    all
}

/// The corner as `sdg_superbox` sets it up for a box of half size `half` and
/// corner `radius`, about the box's centre.
struct Corner {
    /// Where the corner's curve is centred: never below zero, since a box too
    /// short for the full corner has it cut to fit.
    centre: [f64; 2],
    /// How far the curve runs from its centre along each axis.
    reach: [f64; 2],
    /// Its exponent along each: the curve is (x/a)^n + (y/b)^m = 1.
    power: [f64; 2],
}

/// How far a corner of softness `soft` has fallen below its edge, in radii,
/// three quarters of a radius short of the other edge; `corner_drop` in
/// glass.frag. Negative, and further the softer the corner.
fn corner_drop(soft: f64) -> f64 {
    soft * ((1.0 - (1.0 - 0.75 / soft).powf(2.0 * soft)).powf(0.5 / soft) - 1.0)
}

impl Corner {
    /// Line for line `sdg_superbox`, the glass's own: each axis's softness is
    /// cut to what the box can hold along it, and the box is drawn in along
    /// the other axis by the difference the cut makes to how far the corner
    /// has fallen, its corner there drawn as for a radius that much smaller.
    fn new(half: [f64; 2], radius: f64) -> Self {
        let fit = half.map(|h| SOFTNESS.min(h / radius));
        let full = corner_drop(SOFTNESS);
        let pull = [
            radius * (corner_drop(fit[1]) - full),
            radius * (corner_drop(fit[0]) - full),
        ];
        let reach = [fit[0] * (radius - pull[0]), fit[1] * (radius - pull[1])];
        Corner {
            centre: [half[0] - pull[0] - reach[0], half[1] - pull[1] - reach[1]],
            reach,
            power: fit.map(|s| 2.0 * s),
        }
    }

    /// The point `t` of the way round the curve, 0 on the x axis's side and
    /// a right angle on the y axis's, with its outward normal.
    fn at(&self, t: f64) -> ([f64; 2], [f64; 2]) {
        let [a, b] = self.reach;
        let [n, m] = self.power;
        let (u, v) = (
            t.cos().max(0.0).powf(2.0 / n),
            t.sin().max(0.0).powf(2.0 / m),
        );
        let point = [self.centre[0] + a * u, self.centre[1] + b * v];
        // The gradient of (x/a)^n + (y/b)^m.
        let (gx, gy) = (n / a * u.powf(n - 1.0), m / b * v.powf(m - 1.0));
        let length = gx.hypot(gy);
        (point, [gx / length, gy / length])
    }
}

/// One quarter of the outline, from the x axis round to the y axis, `inset`
/// in from the outline of a box that much larger on every side.
fn quarter(width: f64, height: f64, radius: f64, inset: f64) -> Vec<[f64; 2]> {
    let half = [width / 2.0 + inset, height / 2.0 + inset];
    // As the shader clamps it, and as `border-radius` does.
    let radius = (radius + inset).clamp(0.0, half[0].min(half[1]));
    if radius < 1e-3 {
        return vec![
            [half[0] - inset, 0.0],
            [half[0] - inset, half[1] - inset],
            [0.0, half[1] - inset],
        ];
    }
    let corner = Corner::new(half, radius);
    let moved = |t: f64| {
        let (p, n) = corner.at(t);
        [p[0] - inset * n[0], p[1] - inset * n[1]]
    };

    // Where the edges are, which is in from the box's own wherever the
    // corner drew it in.
    let edge = [
        corner.centre[0] + corner.reach[0] - inset,
        corner.centre[1] + corner.reach[1] - inset,
    ];
    let mut points = Vec::new();
    // The straight edges, where the box is long enough to have them.
    if corner.centre[1] > 0.0 {
        points.push([edge[0], 0.0]);
    }
    let steps = 16;
    for i in 0..steps {
        let (t0, t1) = (
            FRAC_PI_2 * i as f64 / steps as f64,
            FRAC_PI_2 * (i + 1) as f64 / steps as f64,
        );
        if i == 0 {
            points.push(moved(t0));
        }
        refine(&moved, t0, t1, moved(t0), moved(t1), 0, &mut points);
    }
    if corner.centre[0] > 0.0 {
        points.push([0.0, edge[1]]);
    }
    clip(untangle(points))
}

/// Add the curve from `t0` to `t1` to `points`, whose last point is already
/// `p0`, with as many points as it takes to stay within [`TOLERANCE`].
fn refine(
    curve: &impl Fn(f64) -> [f64; 2],
    t0: f64,
    t1: f64,
    p0: [f64; 2],
    p1: [f64; 2],
    depth: u32,
    points: &mut Vec<[f64; 2]>,
) {
    let tm = (t0 + t1) / 2.0;
    let pm = curve(tm);
    if depth < 12 && off_chord(pm, p0, p1) > TOLERANCE {
        refine(curve, t0, tm, p0, pm, depth + 1, points);
        refine(curve, tm, t1, pm, p1, depth + 1, points);
    } else {
        points.push(p1);
    }
}

/// How far `p` lies from the line through `a` and `b`.
fn off_chord(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy);
    if length < 1e-9 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / length
}

/// Drop the points where moving the outline in folded it back on itself.
///
/// Moved in further than the curve's own radius, an outline turns back for a
/// stretch, and the points along it walk the wrong way round. No shape here
/// is inset that far — the tightest corner, a panel's, curves no tighter
/// than 22px at any softness, against a 16px gap — but one that was would
/// otherwise be drawn with a knot in it.
fn untangle(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut kept: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for p in points {
        // A quarter outline only ever turns anticlockwise, from facing +x
        // to facing +y, so each step goes up and to the left of the last.
        let backwards = kept
            .last()
            .is_some_and(|q| p[0] > q[0] + 1e-9 || p[1] < q[1] - 1e-9);
        if !backwards {
            kept.push(p);
        }
    }
    kept
}

/// Keep the quarter to its own quadrant. The corner's centre is never below
/// zero, so this only trims what rounding leaves a hair past an axis.
fn clip(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut out = Vec::with_capacity(points.len());
    let within = |p: [f64; 2]| p[0] >= 0.0 && p[1] >= 0.0;
    for (i, &p) in points.iter().enumerate() {
        if i > 0 && within(p) != within(points[i - 1]) {
            out.push(crossing(points[i - 1], p));
        }
        if within(p) {
            out.push(p);
        }
    }
    out
}

/// Where the segment from `a` to `b` crosses the axis one of them is beyond.
fn crossing(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let axis = if a[1] < 0.0 || b[1] < 0.0 { 1 } else { 0 };
    let f = a[axis] / (a[axis] - b[axis]);
    let mut p = [a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1])];
    p[axis] = 0.0;
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glass's own inside test, from `sdg_superbox`: what the outline is
    /// checked against.
    fn inside_glass(p: [f64; 2], half: [f64; 2], radius: f64) -> bool {
        let c = Corner::new(half, radius);
        let q = [p[0].abs() - c.centre[0], p[1].abs() - c.centre[1]];
        let edge = [q[0] - c.reach[0], q[1] - c.reach[1]];
        if edge[0] >= 0.0 || edge[1] >= 0.0 {
            return false;
        }
        if q[0] <= 0.0 || q[1] <= 0.0 {
            return true;
        }
        (q[0] / c.reach[0]).powf(c.power[0]) + (q[1] / c.reach[1]).powf(c.power[1]) < 1.0
    }

    /// How far `p` is from the glass's edge, by walking out from the centre.
    fn depth(p: [f64; 2], half: [f64; 2], radius: f64) -> f64 {
        let length = p[0].hypot(p[1]);
        let d = [p[0] / length, p[1] / length];
        let (mut lo, mut hi) = (0.0, half[0].hypot(half[1]) + 1.0);
        for _ in 0..60 {
            let m = (lo + hi) / 2.0;
            if inside_glass([d[0] * m, d[1] * m], half, radius) {
                lo = m;
            } else {
                hi = m;
            }
        }
        lo - length
    }

    #[test]
    fn the_outline_is_the_glass_edge() {
        // The skip pill, a menu panel, and a pill short enough to have its
        // corner cut to fit along both axes.
        for (w, h, r) in [
            (180.0, 48.0, 24.0),
            (380.0, 400.0, 32.0),
            (60.0, 48.0, 24.0),
        ] {
            let half = [w / 2.0, h / 2.0];
            for p in outline(w, h, r, 0.0) {
                let d = depth(p, half, r);
                assert!(d.abs() < 0.01, "{w}x{h} r{r}: {p:?} is {d} from the edge");
            }
        }
    }

    /// How far `p` is from the nearest point of a closed outline.
    fn gap(p: [f64; 2], outline: &[[f64; 2]]) -> f64 {
        (0..outline.len())
            .map(|i| {
                let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let along = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy)
                    / (dx * dx + dy * dy).max(1e-12))
                .clamp(0.0, 1.0);
                (p[0] - a[0] - along * dx).hypot(p[1] - a[1] - along * dy)
            })
            .fold(f64::MAX, f64::min)
    }

    #[test]
    fn an_inset_outline_keeps_an_even_gap() {
        // A capsule 4px inside its 200 by 48 pill, the whole way round.
        let pill = outline(200.0, 48.0, 24.0, 0.0);
        for p in outline(192.0, 40.0, 20.0, 4.0) {
            let d = gap(p, &pill);
            assert!((d - 4.0).abs() < 0.03, "{p:?} is {d} in");
        }
    }

    #[test]
    fn an_inset_keeps_nearer_an_even_gap_than_a_corner_of_its_own() {
        // Where it is hardest: a box too short for the surface's full corner,
        // as a 348 by 34 row 16 inside a 380 by 400 menu would be, at the
        // rows' 16px radius. Not quite even, but nearer it than a corner of
        // the row's own, whatever the softness: at 3.5 that one pinched the
        // gap to 14.9, where this held between 16 and 16.6.
        let (w, h, inset) = (380.0, 400.0, 16.0);
        let panel = outline(w, h, 32.0, 0.0);
        let row_centre = h / 2.0 - inset - 17.0;
        let furthest = |row: Vec<[f64; 2]>| {
            row.into_iter()
                .filter(|p| p[1] >= 0.0)
                .map(|[x, y]| (gap([x, y + row_centre], &panel) - inset).abs())
                .fold(0.0, f64::max)
        };
        let moved_in = furthest(outline(w - 2.0 * inset, 34.0, 16.0, inset));
        let own = furthest(outline(w - 2.0 * inset, 34.0, 16.0, 0.0));
        assert!(
            moved_in < 0.75 && moved_in <= own + 0.01,
            "moved in, the gap strays {moved_in} from 16; a corner of its own, {own}"
        );
    }

    #[test]
    fn the_outline_goes_once_round() {
        let points = outline(180.0, 48.0, 24.0, 0.0);
        // The shoelace area of a closed outline, which a knot would cut down.
        let n = points.len();
        let area: f64 = (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0;
        assert!(area > 0.0 && area < 180.0 * 48.0);
        assert!(area > 180.0 * 48.0 - 4.0 * 24.0 * 24.0);
    }

    #[test]
    fn no_radius_is_a_rectangle() {
        assert_eq!(
            outline(10.0, 6.0, 0.0, 0.0).len(),
            12,
            "three corners of a quarter, four times"
        );
    }
}
