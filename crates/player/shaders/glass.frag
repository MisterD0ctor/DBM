// Liquid glass — a physically-based lens over the finished frame.
//
// This is the pass the whole architecture was for. Because the video is a
// texture in our own pipeline rather than a sibling window, a panel can be a
// real material instead of a CSS blur.
//
// Slint owns layout and content; this owns material. Panel rectangles come
// straight out of Slint's own layout each frame, so nothing is mirrored by
// hand and nothing can drift.
//
// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------
//
// Each panel is a rounded box with a domed surface: flat across the middle,
// curving down to meet the screen at the rim. Everything is evaluated in a 2D
// cross-section, because the dome is symmetric about the panel outline — the
// rounded-box SDF gives the outward radial direction, and one (radial, up)
// slice does the rest.
//
//     height
//       ^            ________________              <- flat centre, slope 0
//       |       ____/                \____
//       |    __/                          \__
//       |   /                                \
//       |  /                                  \      <- steepening toward the rim
//       | |                                    |
//       | |                                    |
//       |_|                                    |_
//       +----------------------------------------> radial   (t: 0 centre .. 1 rim)
//
// The viewer looks straight down, so a fragment's colour is whatever a
// vertical ray picks up at that surface. Three things happen at the
// interface, and all three are computed rather than faked:
//
//   * transmission — Snell's law bends the ray, so it lands somewhere else on
//     the backdrop. This gives the panel apparent thickness, and is by far
//     the strongest cue that it is an object rather than a hole.
//   * reflection — the mirrored ray either tips over and sees the backdrop
//     again, or escapes upward off the screen, where there is nothing to see.
//   * Fresnel — how the two are weighted, from the exact unpolarised
//     reflectance. Grazing angles at the rim go strongly reflective, which is
//     why real glass edges look bright without any hand-placed highlight.
//
// One term is placed by hand on top of those: the rim light, a line of light
// along the top and bottom of each pane's outermost edge — see `u_rim`.
//
// Wavelength enters on the transmitted ray, and enters properly: the index
// of refraction is a function of it, so every wavelength lands somewhere
// slightly different and the colour at a fragment is the integral of what
// they each found, weighted by the eye's response under D65. See dispersion,
// below.

in vec2 v_uv;
out vec4 frag;

/// Everything behind the glass, already composited (video + ambient border).
uniform sampler2D u_base;
/// Blurred copy of the same. Both the refracted and the reflected ray sample
/// this rather than `u_base`, so the backdrop blur radius doubles as the
/// material's frostiness.
uniform sampler2D u_blur;
uniform vec2 u_base_scale;
uniform vec2 u_blur_scale;

/// Target size in pixels; all panel geometry below is in pixels too.
uniform vec2 u_size;

#define MAX_PANELS 14
uniform int u_panel_count;
/// (x0, y0, x1, y1) in pixels, top-left origin.
uniform vec4 u_panel_rect[MAX_PANELS];
/// (corner_radius, tint_alpha, opacity, unused)
///
/// `opacity` is how far the panel has faded in. It scales coverage, so a
/// panel arriving at a third of its opacity refracts a third as hard — the
/// glass appears with the surface rather than under it.
uniform vec4 u_panel_style[MAX_PANELS];

// --- Material parameters ---------------------------------------------------

/// Width of the domed rim, as a fraction of the panel's own corner radius.
///
/// A fraction rather than a count of pixels because the panels are not one
/// size: the same absolute rim that looks right rolling around a 32px corner
/// swallows a 15px pill whole. Tying it to the radius makes one slider mean
/// the same thing on every piece of glass — a small pane simply has a small
/// rolled edge, which is also how real glass is made.
uniform float u_bevel;
/// Effective glass thickness for transmission, as a fraction of the bevel
/// width above. How far the refracted ray descends before reaching the
/// backdrop, and so how strongly the rim displaces what is behind it.
///
/// Proportional to the bevel for the same reason the bevel is proportional to
/// the radius: thickness and rim width are two dimensions of one piece of
/// glass, and a rim that narrows without thinning stops looking like glass
/// and starts looking like a painted edge.
uniform float u_refract;
/// Index of refraction. 1.0 bends nothing, 1.33 water, 1.5 window glass,
/// 1.8 flint, 2.4 diamond. Higher bends harder and, through the Fresnel term,
/// also makes the rim more mirror-like.
uniform float u_ior;
/// How strongly the index of refraction varies with wavelength, as dispersive
/// power: the spread of the index between the F and C lines as a fraction of
/// (n_d - 1). That is the reciprocal of the Abbe number, which is how glass
/// is actually specified. Real glasses run about 1/70 for crown to 1/20 for
/// dense flint; this goes well past that, because here the fringe is a thing
/// to be seen rather than a defect to be corrected out.
uniform float u_aberration;
/// Overall strength of the reflection, scaling the Fresnel weight.
uniform float u_specular;
/// Brightness of the rim light: a line along the top and bottom of each pane's
/// outermost edge, where the surface has turned past 45 degrees and the
/// mirrored ray points back down. Zero turns it off.
uniform float u_rim;
uniform vec3 u_tint;
/// Global tint strength. The per-panel value in `u_panel_style` says whether
/// a panel takes tint at all; this says how much, and is the slider.
uniform float u_tint_amount;
/// How much of a white backdrop the glass absorbs, 0..1; darker backdrops
/// lose less. 0 absorbs nothing. See absorption, below.
uniform float u_absorb;
/// How dark the shadow under a pane is at its darkest, 0..1. Zero turns it
/// off; see shadow, below.
uniform float u_shadow;

// --- shadow -------------------------------------------------------------------
//
// A pane standing off the picture casts a soft shadow onto it, darkest just
// under its lower edge and fading out all round. It darkens the picture and
// nothing else: every pane's shadow is laid on the frame before any glass is,
// so one pane's shadow never falls across another's glass — they stand at the
// same height — and a pane's own glass, which refracts the frame as it was,
// is not dimmed by what it casts.
//
// Sized from each pane's own corner radius, like the rim: a pill casts a
// small shadow and a panel a larger one, without either value being a count
// of pixels that suits one and swamps the other.

/// How far the shadow falls below its pane, as a fraction of the radius.
const float SHADOW_DROP = 0.0;
/// How far its edge fades, as a fraction of the radius, plus a few pixels so
/// a pane with square corners still casts something soft.
const float SHADOW_SOFTNESS = 1.0;
const float SHADOW_SOFTNESS_MIN = 4.0;

// --- absorption -------------------------------------------------------------
//
// Every word in the interface sits on glass, and the glass shows whatever the
// film is showing. Over a night scene that is free; over snow, a white room or
// a title card the panel resolved to roughly 0.85 luminance and the body text
// at 69% white resolved to nothing. The type was described as being sized and
// weighted for legibility over arbitrary moving content, and nothing in this
// shader was holding up that end of it.
//
// The tint above cannot fix it: it mixes toward a fixed mid grey, so at full
// strength a panel still sits at that grey's luminance no matter how bright
// the backdrop. What is needed is absorption — a pane that transmits less the
// more there is to transmit, which is what tinted glass physically does.
//
// Applied to the transmitted component only. Everything that makes the
// material read as glass — the Fresnel rim, the rim light, the mirrored
// backdrop — is added after this and is untouched, so a panel over a bright
// scene goes deep and keeps its edge rather than turning into a flat card.
//
// The glass page's Absorb slider, `u_absorb`, is how much of a white backdrop
// the pane absorbs; a darker one loses less, along a hyperbola, and black loses
// nothing:
//
//     absorbed = 1 - (1 - a) / ((1 - a) + a * luma)
//
// So 0 is a clear pane and 1 absorbs everything that comes through. The
// saturation lift that follows grows with what was absorbed, to give back the
// colour the darkening takes. Lower, the floor gives way — white text over a
// bright frame has less of a ground under it — which is what the slider
// trades for a clearer pane.

/// Rec. 709, matching how the eye weights the three channels rather than
/// averaging them: a saturated green frame is far brighter than its mean.
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// --- antialiasing -------------------------------------------------------------
//
// The outline needs nothing more than it has: `coverage` below is a smooth
// function of distance, so the silhouette is already antialiased analytically,
// and counting samples in and out would only quantise it into steps.
//
// What does alias is the shading just inside it. The dome's slope runs to
// infinity at the rim, so reflectance, the mirror direction and the reflection
// fading as it swings upward all happen within the last pixel or two — a
// single sample per pixel lands on whichever part of that it happens to hit,
// and the bright edge crawls as a panel moves. So the shading, and only the
// shading, is supersampled there. Across the flat middle it changes slowly and
// one sample is exact enough.

/// MSAA 8x offsets, in pixels. Eight samples resolving both horizontal
/// and vertical edges at eight distinct positions each.
const vec2 RIM_SAMPLES[8] = vec2[8](
    vec2( 1.0, -3.0) * 0.0625,
    vec2(-1.0,  3.0) * 0.0625,
    vec2( 5.0,  1.0) * 0.0625,
    vec2(-3.0, -5.0) * 0.0625,
    vec2(-5.0,  5.0) * 0.0625,
    vec2(-7.0, -1.0) * 0.0625,
    vec2( 3.0,  7.0) * 0.0625,
    vec2( 7.0, -7.0) * 0.0625
);
/// How far in the supersampling reaches, as a fraction of the bevel: the
/// stretch where the slope is past about 1, which contains the reflection's
/// turn through horizontal (t near 0.87) as well as the rim itself.
const float RIM_BAND = 0.5;

// --- dispersion ---------------------------------------------------------------
//
// The fringe used to be three taps: the same offset scaled up for red, left
// alone for green, scaled down for blue. That is fine while it is subtle and
// falls apart the moment it is not — past a pixel or two of spread the panel
// stops having a coloured edge and starts having three coloured copies of
// its edge, because three copies is all it ever was. It was also the wrong
// way round. Shorter wavelengths bend *more*, so blue should land further
// from where it entered than red, and it was doing the opposite.
//
// So the index is a function of wavelength and the transmitted colour is an
// integral rather than three samples:
//
//     C = integral over lambda of  backdrop(offset(n(lambda))) * w(lambda)
//
// `w` is the CIE 1931 2-degree observer under D65, carried through to linear
// sRGB and normalised so the whole band sums to white. Its three lobes are
// what turn a smooth sweep of sample positions into a smooth sweep of hue,
// and they overlap the way the eye's cones do, which is why the result reads
// as one edge seen through glass rather than as a stack of coloured copies.
//
// The band is integrated in strata, and a stratum's weight is the exact
// difference of the cumulative curve at its two ends rather than a point
// sample of the curve. That matters more than it sounds: the weights then
// add to exactly white at any number of strata, however few, so changing how
// many are taken changes how finely the smear is resolved and never what
// colour a flat backdrop comes out. A running normalisation would have had
// to divide by a sum that includes the negative lobes, which is a small
// number divided by a small number on the one channel that can least afford
// it.
//
// Linear sRGB cannot hold a spectral colour — its red weight goes negative
// between about 470 and 600nm, which is the gamut saying so rather than an
// error. Two things keep that from showing: what is sampled is the *blurred*
// backdrop, so neighbouring wavelengths land on values close to each other
// and the negative lobes have nothing sharp to ring against; and the result
// is floored at zero before anything downstream divides by its luminance.

/// The visible band, in microns, as bin edges: 395nm to 705nm in 10nm steps.
const float LAMBDA_MIN = 0.395;
const float LAMBDA_MAX = 0.705;
#define SPECTRUM_STEPS 31

/// 1/lambda^2 at the sodium d line, 587.6nm — where `u_ior` is the index.
const float INV_D2 = 2.896647;
/// 1/lambda_F^2 - 1/lambda_C^2, the hydrogen F and C lines at 486.1 and
/// 656.3nm. Dispersive power is measured across those two, so this is what
/// turns `u_aberration` into Cauchy's B.
const float INV_FC = 1.910389;

/// Running integral of the weight curve, at each bin edge.
///
/// CIE 1931 2-degree colour matching functions times the D65 relative SPD,
/// through the XYZ-to-linear-sRGB matrix, normalised so the last entry is
/// exactly white. Integrating the tables that produced this puts the white
/// point at x=0.3126 y=0.3293, against D65's nominal 0.3127, 0.3290 — the
/// difference is the band's two tails, which are outside 395-705nm and
/// carry nothing the eye can see.
const vec3 SPECTRUM_CUM[SPECTRUM_STEPS + 1] = vec3[SPECTRUM_STEPS + 1](
    vec3( 0.00000000,  0.00000000,  0.00000000), // 395 nm
    vec3( 0.00093823, -0.00080654,  0.00568553), // 405 nm
    vec3( 0.00404350, -0.00351265,  0.02489721), // 415 nm
    vec3( 0.01357884, -0.01198610,  0.08596233), // 425 nm
    vec3( 0.03098087, -0.02803644,  0.20748264), // 435 nm
    vec3( 0.05311861, -0.05003088,  0.39262043), // 445 nm
    vec3( 0.06951930, -0.07004844,  0.60172096), // 455 nm
    vec3( 0.07152753, -0.08118067,  0.79931933), // 465 nm
    vec3( 0.05530278, -0.07739773,  0.94676202), // 475 nm
    vec3( 0.02127480, -0.05526971,  1.03871131), // 485 nm
    vec3(-0.02501829, -0.01632332,  1.08526277), // 495 nm
    vec3(-0.08902081,  0.04700714,  1.10827798), // 505 nm
    vec3(-0.17317586,  0.14294143,  1.11493777), // 515 nm
    vec3(-0.26527511,  0.26915942,  1.10911351), // 525 nm
    vec3(-0.34809658,  0.41766198,  1.09664552), // 535 nm
    vec3(-0.40120054,  0.56663327,  1.08109925), // 545 nm
    vec3(-0.41396470,  0.70896237,  1.06436262), // 555 nm
    vec3(-0.37644293,  0.83098707,  1.04863943), // 565 nm
    vec3(-0.28447462,  0.92638733,  1.03497219), // 575 nm
    vec3(-0.13607994,  0.99377601,  1.02363832), // 585 nm
    vec3( 0.04599053,  1.02944966,  1.01554964), // 595 nm
    vec3( 0.25728695,  1.04257596,  1.00967816), // 605 nm
    vec3( 0.46795548,  1.04019214,  1.00572908), // 615 nm
    vec3( 0.64979452,  1.03078720,  1.00323507), // 625 nm
    vec3( 0.78224303,  1.02090310,  1.00179215), // 635 nm
    vec3( 0.87623919,  1.01252772,  1.00093805), // 645 nm
    vec3( 0.93356458,  1.00692433,  1.00047846), // 655 nm
    vec3( 0.96799488,  1.00339281,  1.00022333), // 665 nm
    vec3( 0.98539565,  1.00156592,  1.00009968), // 675 nm
    vec3( 0.99370136,  1.00067989,  1.00004242), // 685 nm
    vec3( 0.99784600,  1.00023300,  1.00001444), // 695 nm
    vec3( 1.00000000,  1.00000000,  1.00000000)  // 705 nm
);

/// The weight curve integrated from 395nm up to `micron`.
///
/// Linear between table entries, which makes the difference of two of these
/// the trapezoid rule over the stretch between them.
vec3 spectrum_upto(float micron) {
    float u = clamp((micron - LAMBDA_MIN) / (LAMBDA_MAX - LAMBDA_MIN), 0.0, 1.0)
            * float(SPECTRUM_STEPS);
    int i = min(int(u), SPECTRUM_STEPS - 1);
    return mix(SPECTRUM_CUM[i], SPECTRUM_CUM[i + 1], u - float(i));
}

/// Index of refraction at one wavelength, by Cauchy's two-term law
/// n(lambda) = A + B / lambda^2, written against the d line so that `u_ior`
/// stays the index of the material rather than a coefficient of it.
float index_at(float micron, float n_d, float b) {
    return max(n_d + b * (1.0 / (micron * micron) - INV_D2), 1.0);
}

/// Lateral-over-vertical displacement of a vertical ray entering a surface of
/// this slope at this index. Split out of `glass_at` because dispersion
/// needs it once per wavelength; see the derivation there.
float transmit_ratio(float slope, float slope2, float n) {
    float n2 = n * n;
    float d = sqrt(n2 + (n2 - 1.0) * slope2);
    return -slope * (d - 1.0) / (slope2 + d);
}

/// Most wavelengths a fragment will take, and the most it may take inside the
/// supersampled rim band, where eight of these are averaged anyway.
#define MAX_BANDS 24
#define RIM_BANDS 8

/// A fixed per-pixel offset in [0,1), to break the strata out of lockstep.
///
/// Stratified sampling of a band puts the samples at the same wavelengths in
/// every pixel, which draws the bands it was meant to dissolve. Rotating the
/// set by a per-pixel amount turns that structure into noise at the same
/// amplitude, and the noise is far below what the blurred backdrop is doing
/// anyway. Hashed from the pixel rather than from a clock, so it is the same
/// every frame and a still panel does not crawl.
float dither_at(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

vec3 base_at(vec2 p) {
    return texture(u_base, clamp(p, vec2(0.0), vec2(1.0)) * u_base_scale).rgb;
}

vec3 blur_at(vec2 p) {
    return texture(u_blur, clamp(p, vec2(0.0), vec2(1.0)) * u_blur_scale).rgb;
}

/// Signed distance to a rounded box. Negative inside.
float sd_round_box(vec2 p, vec2 half_size, float r) {
    vec2 q = abs(p) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - r;
}

/// Outward unit normal of that SDF, analytically rather than by finite
/// differences — the extra evaluations would cost more and be less stable
/// right at the corners.
vec2 sd_round_box_normal(vec2 p, vec2 half_size, float r) {
    vec2 s = sign(p);
    vec2 q = abs(p) - half_size + r;
    if (max(q.x, q.y) > 0.0) {
        return s * normalize(max(q, vec2(1e-6)));
    }
    return q.x > q.y ? vec2(s.x, 0.0) : vec2(0.0, s.y);
}

/// One published panel: its centre and half size in pixels, and its corner
/// radius, clamped to what the box can hold.
void panel_shape(int i, out vec2 centre, out vec2 half_size, out float radius) {
    vec4 rect = u_panel_rect[i];
    centre = 0.5 * (rect.xy + rect.zw);
    half_size = max(0.5 * (rect.zw - rect.xy), vec2(0.5));
    radius = clamp(u_panel_style[i].x, 0.0, min(half_size.x, half_size.y));
}

/// How much of one panel's shadow falls on pixel `px`, 0..1, before the
/// strength: the panel's outline dropped by `SHADOW_DROP` and faded across
/// `SHADOW_SOFTNESS` either side of its edge.
float shadow_at(vec2 px, vec2 centre, vec2 half_size, float radius) {
    float soft = SHADOW_SOFTNESS * radius + SHADOW_SOFTNESS_MIN;
    vec2 drop = vec2(0.0, SHADOW_DROP * radius);
    float d = max(0.0, sd_round_box(px - centre - drop, half_size, radius));
    return smoothstep(-soft, 0.0, -d) * exp(-4.0 * d / soft);
}

/// The glass surface at pixel `px` of one panel. A point just outside the
/// outline, as an edge sample can be, is shaded as the rim itself: `t` clamps
/// to 1 there, and every term below is finite at 1.
///
/// `budget` is the most wavelengths this sample may take and `dither` where
/// in each stratum it takes them; see dispersion, above.
vec3 glass_at(vec2 px, vec2 centre, vec2 half_size, float radius, float tinted,
              int budget, float dither) {
    vec2 uv = px / u_size;
    vec2 rel = px - centre;
    float d = sd_round_box(rel, half_size, radius);

    // Outward radial direction. Every 2D vector below lives in the
    // (radial, up) slice and is lifted back to screen space through this.
    vec2 box_normal = sd_round_box_normal(rel, half_size, radius);

    // Both material widths are relative, so they resolve here, per
    // panel, from the radius this one happens to have.
    float bevel = u_bevel * radius;
    float thickness = u_refract * bevel;

    // Position across the bevel: 0 on the flat centre, 1 at the rim.
    float t = clamp(1.0 + d / max(bevel, 1e-3), 0.0, 1.0);

    // Dome profile, in units of bevel width:
    //
    //     height(t) = 1 + 0.5 * sqrt(1 - t^6)
    //     slope(t)  = d(height)/dt = -1.5 * t^5 / sqrt(1 - t^6)
    //
    // The sixth power keeps the centre flat and pushes the curvature 
    // into the last stretch of the bevel, which is what makes the 
    // lensing hug the rim. `slope` runs to -infinity as the surface
    // turns vertical at t = 1, so the root is floored; every term below
    // converges as |slope| grows, so a large finite value behaves.
    float dome = max(sqrt(max(1.0 - t * t * t * t * t * t, 0.0)), 1e-4);
    float height = 1.0 + 0.5 * dome;
    float slope = -1.5 * t * t * t * t * t / dome;
    float slope2 = slope * slope;

    // Snell's law for a vertical incident ray. With surface normal
    // n = (-slope, 1)/sqrt(1 + slope^2):
    //
    //     cos_i = 1 / sqrt(1 + slope^2)
    //     cos_t = D / (ior * sqrt(1 + slope^2))
    //
    // `D` collects the shared awkward part once — the refracted direction
    // and both Fresnel terms are all expressible in it.
    float ior = max(u_ior, 1.0);
    float ior2 = ior * ior;
    float D = sqrt(ior2 + (ior2 - 1.0) * slope2);

    // The vector form of Snell's law reduces, for a vertical incident
    // ray, to a lateral-over-vertical ratio of slope*(D-1)/(slope^2+D).
    // Multiplying by how far the ray descends gives the displacement.
    // `slope` is negative on the bevel, so this pulls the sample inward:
    // the rim shows magnified content from under the panel, exactly as a
    // bevelled pane does.
    // That ratio is `transmit_ratio`, above, because it is now wanted once
    // per wavelength. This is the part every wavelength shares: the
    // direction, and how far the ray descends, in pixels.
    vec2 reach = -box_normal * height * thickness;

    // Dispersion. Cauchy's B from the dispersive power, and the two ends of
    // the band from it — violet has the highest index and lands furthest in.
    float cauchy_b = u_aberration * (ior - 1.0) / INV_FC;
    float ratio_violet = transmit_ratio(slope, slope2, index_at(LAMBDA_MIN, ior, cauchy_b));
    float ratio_red = transmit_ratio(slope, slope2, index_at(LAMBDA_MAX, ior, cauchy_b));

    // How far apart those two land, in pixels, is the whole of what decides
    // how many wavelengths are worth taking. Under a pixel there is nothing
    // to resolve and the d-line ray is the answer, which is every fragment
    // across the flat middle of every panel, where the slope is zero and so
    // is every offset. Past that, about one wavelength per pixel of smear:
    // fewer and the sweep arrives as bands, which is the three-copy look
    // again with more copies.
    float spread = length(reach) * abs(ratio_violet - ratio_red);
    int bands = int(clamp(ceil(spread), 1.0, float(budget)));

    vec3 refracted;
    if (bands <= 1) {
        refracted = blur_at(uv + reach * transmit_ratio(slope, slope2, ior) / u_size);
    } else {
        refracted = vec3(0.0);
        float width = (LAMBDA_MAX - LAMBDA_MIN) / float(bands);
        vec3 below = vec3(0.0);
        for (int i = 0; i < MAX_BANDS; i++) {
            if (i >= bands) {
                break;
            }
            float start = LAMBDA_MIN + float(i) * width;
            // The stratum's exact share of the curve, so the shares add to
            // white at any count; its ray from a dithered point inside it.
            vec3 upto = spectrum_upto(start + width);
            vec3 share = upto - below;
            below = upto;
            float n = index_at(start + dither * width, ior, cauchy_b);
            refracted += share * blur_at(uv + reach * transmit_ratio(slope, slope2, n) / u_size);
        }
        // Out of gamut comes out negative; see dispersion, above. The
        // absorption below divides by this colour's luminance.
        refracted = max(refracted, vec3(0.0));
    }

    // Tint first. Glass is not a neutral filter, and without this a panel
    // over dark video reads as a hole rather than a surface.
    refracted = mix(refracted, u_tint, clamp(tinted * u_tint_amount, 0.0, 1.0));

    // Then absorb, by how bright what is left turned out to be. The
    // sample is already blurred, so a small specular glint under a panel
    // does not pull the whole surface down — only a genuinely bright area
    // does, and it does so smoothly across the panel rather than in
    // patches. Opted in per panel by the same flag as the tint.
    // Floored, because the curve below divides by it: over a frame that is
    // exactly black with the tint at zero it was 0/0, every pixel of every
    // panel came out NaN, and the glass drew as nothing at all — the
    // empty window and every credits roll. The floor is far below any
    // real backdrop, and the curve's own limit at zero is where it lands.
    float backdrop = max(dot(refracted, LUMA), 1e-4);
    float absorb = clamp(u_absorb, 0.0, 1.0);
    float absorbed = (1.0 - (1.0 - absorb) / (1.0 - absorb + absorb * backdrop))
                    * tinted;

    refracted *= 1.0 - absorbed;
    vec3 gray = vec3(dot(refracted, LUMA));

    // Saturate darkened areas
    refracted = mix(gray, refracted, 1.0 / (1.0 - 0.5 * absorbed));

    // Exact unpolarised Fresnel reflectance, averaging s and p. Written
    // in D so it needs no further trigonometry:
    //
    //     Rs = ((D - 1) / (D + 1))^2
    //     Rp = ((D - ior^2) / (D + ior^2))^2
    //
    // Both approach 1 as the surface turns vertical, which is what makes
    // the rim read as a bright mirrored edge on its own.
    float rs = (D - 1.0) / (D + 1.0);
    float rp = (D - ior2) / (D + ior2);
    float reflectance = 0.5 * (rs * rs + rp * rp);

    // Mirror direction for the same vertical view ray:
    // reflect((0,-1), n) = (-2*slope, 1 - slope^2) / (1 + slope^2),
    // which is exactly unit length at every slope.
    vec2 mirror = vec2(-2.0 * slope, 1.0 - slope2) / (1.0 + slope2);
    vec3 mirror3 = vec3(box_normal * mirror.x, mirror.y);

    // Tipped over and pointing down, the mirrored ray sees the backdrop
    // again — a screen-space reflection, displaced by the same lateral-over-
    // vertical reasoning as the refracted ray.
    //
    // The denominator vanishes as the mirrored ray passes horizontal
    // (|slope| -> 1), where the true displacement really is unbounded.
    // Floor it rather than let one ring of pixels smear.
    //float denom = 1.0 - slope2;
    //denom = abs(denom) < 1e-3 ? (denom < 0.0 ? -1e-3 : 1e-3) : denom;
    //float reflection_ratio = -2.0 * slope / denom;
    //vec2 reflection_offset =
    //    -box_normal * reflection_ratio * height * bevel / u_size;
    //vec3 reflected = base_at(uv + reflection_offset) * u_specular;

    float rim = max(0.0, -mirror.y * (1.0 - mirror.x))
              * max(0.0, 2.0 * abs(mirror3.y / mirror.x) - 1.0)
              * 0.5 * (1.0 + cos(0.5 * 3.1459 * (rel.x / half_size.x - 0.25 * sign(box_normal.y))))
              * u_rim;

    // Pointing up, the mirrored ray leaves the screen and finds nothing to
    // reflect, so the reflection fades out as it swings through horizontal —
    // which also hides the turn, where the screen-space offset above runs off
    // to infinity.
    //vec3 specular = mix(reflected, vec3(1.0), rim) * (1.0 - smoothstep(-0.7, 0.0, mirror.y));

    // Fresnel decides how much of each the viewer gets.
    return mix(refracted, vec3(1.0), rim * reflectance); // mix(refracted, specular, reflectance);
}

void main() {
    vec2 px = v_uv * u_size;
    vec3 col = base_at(v_uv);
    vec2 centre;
    vec2 half_size;
    float radius;

    // Every shadow first, onto the picture alone; see shadow, above. Each
    // fades in with its pane.
    if (u_shadow > 0.0) {
        for (int i = 0; i < MAX_PANELS; i++) {
            if (i >= u_panel_count) {
                break;
            }
            panel_shape(i, centre, half_size, radius);
            float shadow = shadow_at(px, centre, half_size, radius) * u_panel_style[i].z;
            col *= 1.0 - clamp(u_shadow, 0.0, 1.0) * shadow;
        }
    }

    for (int i = 0; i < MAX_PANELS; i++) {
        if (i >= u_panel_count) {
            break;
        }
        panel_shape(i, centre, half_size, radius);
        float tinted = u_panel_style[i].y;

        float d = sd_round_box(px - centre, half_size, radius);

        // One pixel of feather: the coverage mask, which is all the outline
        // needs (see antialiasing, above).
        float coverage = smoothstep(-1.5, 0.5, -d) * u_panel_style[i].z;
        if (coverage <= 0.0) {
            continue;
        }

        float dither = dither_at(px);
        vec3 glass;
        if (d > -max(RIM_BAND * u_bevel * radius, 1.0)) {
            glass = vec3(0.0);
            // Each sub-sample takes its wavelengths from a different eighth
            // of the stratum, so the eight together resolve the band as
            // finely as one sample taking eight times as many — the rim,
            // where the smear is widest, gets its spectral resolution from
            // the supersampling it was doing anyway.
            for (int s = 0; s < 8; s++) {
                glass += glass_at(px + RIM_SAMPLES[s], centre, half_size, radius, tinted,
                                  RIM_BANDS, fract(dither + float(s) * 0.125));
            }
            glass *= 0.125;
        } else {
            glass = glass_at(px, centre, half_size, radius, tinted, MAX_BANDS, dither);
        }

        col = mix(col, glass, coverage);
    }

    frag = vec4(col, 1.0);
}
