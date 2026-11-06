//! What the chain is asked to draw: the border's and the glass's settings,
//! the panes, and the glass's fixed material.

use crate::rect::{Pixels, Rect, Region};

/// How far the glass's corners ease into its straight edges: the corner's
/// curve runs this many radii along each, where 1 is a quarter of an ellipse
/// and anything larger squarer and smoother; see `sdg_superbox` in
/// glass.frag.
///
/// Handed to the shader as `u_corner_softness` rather than written there,
/// because the interface draws the same corner on its highlights
/// (`interface::squircle`) and the two must not disagree.
pub const CORNER_SOFTNESS: f64 = 1.25;

/// Must match `MAX_PANELS` in glass.frag.
///
/// More than the interface can put up at once: at most the bar's pane and
/// the disc under the ring, the window's three buttons, the flash's two,
/// the tip, the seek preview, one panel, the pane under the ring in it, and a
/// pill. The shader stops at the real count, so the headroom costs nothing.
pub const MAX_PANELS: usize = 16;

/// Live parameters for the ambient border. Defaults match the `//!PARAM`
/// defaults the mpv shader shipped with.
#[derive(Clone, Copy, Debug)]
pub struct BorderParams {
    pub edge_blur: f32,
    pub spread: f32,
    pub grain: f32,
    pub falloff: f32,
    pub falloff_softness: f32,
    pub max_taps: f32,
}

impl Default for BorderParams {
    fn default() -> Self {
        Self {
            edge_blur: 0.007,
            spread: 0.7,
            grain: 64.0,
            falloff: 4.0,
            falloff_softness: 0.5,
            max_taps: 256.0,
        }
    }
}

/// One glass panel, in physical pixels, top-left origin.
///
/// Filled from Slint's layout every frame, so the material can't drift from
/// its widget.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassPanel {
    pub rect: Rect<Pixels>,
    pub radius: f32,
    pub tint_alpha: f32,
    /// How far the panel has faded in, 0..1, so the glass fades with it.
    pub opacity: f32,
    /// Laid on another pane, so it casts no shadow; see `GlassRect.nested`.
    pub nested: bool,
    /// Under the keyboard's ring, so its rim is lit at [`RING_RIM`]; see
    /// `GlassRect.ring`.
    pub ring: bool,
    /// How far what it passes is lifted toward white, 0..1: [`RING_MILK`]
    /// under the keyboard's ring, [`OPEN_MILK`] under a bar button whose panel
    /// is open, and 0 for every other pane, which is clear glass.
    pub milk: f32,
}

/// A pane laid over the finished frame, and the panel it is cut to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassOver {
    pub pane: GlassPanel,
    pub clip: Region,
}

/// Look of the glass: what the settings panel's glass page adjusts, plus the
/// tint colour, which stays in code. The defaults aim at visible rather than
/// subtle.
#[derive(Clone, Copy, Debug)]
pub struct GlassParams {
    /// Gaussian radius of the backdrop blur, in pixels; 0 is a sharp
    /// passthrough.
    pub blur_sigma: f32,
    /// Effective glass thickness for transmission, as a fraction of the bevel
    /// width. How far the refracted ray descends before it reaches the
    /// backdrop, so it scales how strongly the rim displaces what is behind
    /// it.
    pub refract: f32,
    /// How dispersive the glass is, 0..1: a share of the most it can be with
    /// every visible wavelength still bending, where the band's red end meets
    /// an index of 1 (a dispersive power of about 2.16, whatever the index).
    /// The shader integrates the transmitted ray over wavelength with it, so
    /// this is the width of the prismatic fringe over a sharp backdrop, and
    /// wider over a blurred one (see [`fringe_scale`]).
    pub aberration: f32,
    /// How strongly the rim light shows: white added round each pane's
    /// outermost edge, where the surface has turned past 45 degrees. Zero
    /// turns it off.
    pub rim: f32,
    pub tint: [f32; 3],
    /// How far a panel pulls toward the tint colour, 0..1. Scales the
    /// per-panel opt-in rather than replacing it.
    pub tint_amount: f32,
    /// How much of a white backdrop the glass absorbs, 0..1; darker backdrops
    /// lose less, and 0 absorbs nothing. Keeps text legible over a bright
    /// frame (see glass.frag).
    pub absorb: f32,
    /// How dark the shadow each pane casts on the picture is, at its darkest,
    /// 0..1. Its size follows the pane's corner radius; see glass.frag.
    pub shadow: f32,
}

// --- the material's fixed properties ----------------------------------------
//
// What the material is, rather than how much of it you want: the rim is the
// corner, and the glass is flint. Changing either stops the panes reading as
// one material, so they are constants rather than settings, and the glass
// page has no row for them. Old settings naming `glass.bevel_ratio`,
// `glass.ior` or `glass.specular` are skipped like any unknown key.

/// Width of the domed rim, as a fraction of each panel's corner radius: how
/// far in from the edge the lensing and lighting reach.
///
/// Relative, so one value suits a 380px panel and a 60px pill (see `u_bevel`
/// in glass.frag). At 1.0 the rim runs the corner's full width, so a capsule
/// is rim the whole way through and a small pane has a small rolled edge.
pub const BEVEL: f32 = 1.0;

/// Index of refraction: 1.0 bends nothing, 1.33 water, 1.5 window glass,
/// 1.8 flint, 2.4 diamond.
///
/// Well past a window pane, so the rim bends hard enough to read at interface
/// scale. No higher, because at diamond the lensing pulls a dark
/// patch of the backdrop into a black oval across the top of the way in.
pub const IOR: f32 = 2.0;

/// The contrast the lowest step of text keeps with the glass behind it,
/// however bright the frame; see [`legible_ceiling`]. WCAG asks 4.5 of text
/// this size. The margin is for its thinnest strokes, which antialiasing
/// leaves short of their full alpha, so that the pixels on screen measure
/// past 4.5 too.
pub const LEGIBLE: f32 = 5.0;

/// The most light the glass may let through, as a luminance: that of the
/// brightest grey on which white at `text` alpha still reads at `ratio`.
/// Zero where no grey would do, which leaves the glass as it is.
pub fn legible_ceiling(text: f32, ratio: f32) -> f32 {
    let contrast = |grey: f32| {
        let words = text + (1.0 - text) * grey;
        (luminance(words) + 0.05) / (luminance(grey) + 0.05)
    };
    if contrast(0.0) < ratio {
        return 0.0;
    }
    // The contrast falls steadily as the grey brightens.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = f32::midpoint(lo, hi);
        if contrast(mid) >= ratio {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    luminance(lo)
}

/// The rim light on the pane under the keyboard's ring, whatever the rim
/// slider says, so the edge of the milk is as sure as its middle.
pub const RING_RIM: f32 = 1.0;

/// How milky the pane under the keyboard's ring is: the share of white mixed
/// into what it passes. Enough to stand 3:1 lighter than glass held to its
/// ceiling over a white frame, and to keep dark words on it past 5:1 over a
/// black one, where it is that share of white alone.
pub const RING_MILK: f32 = 0.72;

/// The same for the disc under a bar button whose panel is open: a step short
/// of the ring's, as the open state is a step under the ring everywhere.
pub const OPEN_MILK: f32 = 0.45;

/// An encoded channel's share of light, as WCAG decodes it.
fn luminance(encoded: f32) -> f32 {
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// How much wider the dispersion fringe is drawn than the dispersive power
/// alone makes it, over a backdrop blurred by `sigma`: none wider over a
/// sharp one, and with the square root of the blur past that.
///
/// A fringe is seen against the edges it splits, which a blur widens, so it
/// widens too, but more slowly than they do: a heavy blur's edges run tens
/// of pixels, and a fringe kept as wide is a wash of colour, not an edge.
pub fn fringe_scale(sigma: f32) -> f32 {
    (sigma.max(0.0) + 1.0).sqrt()
}

impl Default for GlassParams {
    fn default() -> Self {
        Self {
            blur_sigma: 10.0,
            // The glass is as thick as the rim is wide; see `BEVEL`.
            refract: 1.0,
            aberration: 0.3,
            rim: 0.2,
            tint: [0.0, 0.0, 0.0],
            tint_amount: 0.0,
            absorb: 0.35,
            shadow: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LEGIBLE, RING_MILK, fringe_scale, legible_ceiling, luminance};

    #[test]
    fn the_fringe_is_the_glass_own_when_sharp_and_widens_with_the_root_of_the_blur() {
        assert!((fringe_scale(0.0) - 1.0).abs() < 1e-6);
        // Four times the blur, twice the fringe.
        let wide = fringe_scale(99.0) / fringe_scale(24.0);
        assert!((wide - 2.0).abs() < 1e-6, "{wide}");
        // Never narrower than the glass's own.
        assert!((fringe_scale(-1.0) - 1.0).abs() < 1e-6);
    }

    /// The brightest 8-bit grey under a ceiling.
    fn grey_at(ceiling: f32) -> f32 {
        (0..=255)
            .map(|v| v as f32 / 255.0)
            .take_while(|g| luminance(*g) <= ceiling)
            .last()
            .unwrap()
    }

    #[test]
    fn body_text_keeps_its_contrast_on_glass_at_the_ceiling() {
        let body = 224.0 / 255.0;
        let ceiling = legible_ceiling(body, LEGIBLE);
        // A mid grey, nowhere near white.
        assert!((0.08..0.16).contains(&ceiling), "{ceiling}");
        let grey = grey_at(ceiling);
        let text = body + (1.0 - body) * grey;
        let contrast = (luminance(text) + 0.05) / (luminance(grey) + 0.05);
        assert!(contrast >= LEGIBLE, "{contrast}");
    }

    #[test]
    fn the_ring_is_lighter_than_glass_at_its_ceiling() {
        // Over a white frame, where the glass around it is as light as it
        // ever gets.
        let grey = grey_at(legible_ceiling(1.0, LEGIBLE));
        let milk = grey + (1.0 - grey) * RING_MILK;
        let contrast = (luminance(milk) + 0.05) / (luminance(grey) + 0.05);
        assert!(contrast >= 3.0, "{contrast}");
    }

    #[test]
    fn dark_words_read_on_the_ring_over_black() {
        // Over black the milk is its share of white alone, the least it is,
        // and the words are `Paint.knob`, #101010 at 0xe6.
        let alpha = 230.0 / 255.0;
        let words = alpha * 16.0 / 255.0 + (1.0 - alpha) * RING_MILK;
        let contrast = (luminance(RING_MILK) + 0.05) / (luminance(words) + 0.05);
        assert!(contrast >= LEGIBLE, "{contrast}");
    }

    #[test]
    fn brighter_text_lets_the_glass_be_brighter() {
        assert!(legible_ceiling(1.0, LEGIBLE) > legible_ceiling(224.0 / 255.0, LEGIBLE));
    }

    #[test]
    fn text_that_could_never_read_leaves_the_glass_alone() {
        assert_eq!(legible_ceiling(0.0, LEGIBLE), 0.0);
    }
}
