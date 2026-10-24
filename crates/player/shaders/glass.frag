// Liquid glass: a physically based lens over the finished frame.
//
// Because the video is a texture in our own pipeline rather than a sibling
// window, a panel can be a real material instead of a blur. Slint owns layout
// and content and this owns material: the panel rectangles come from Slint
// each frame, so nothing is mirrored by hand.
//
// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------
//
// Each panel is a box with superelliptic corners and a domed surface: flat
// across the middle, curving down to meet the screen at the rim. Everything is
// evaluated in a 2D cross-section, because the dome is symmetric about the
// panel outline — the box's signed distance gives the outward radial
// direction, and one (radial, up) slice does the rest.
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
// vertical ray picks up at that surface. What it transmits is computed rather
// than faked: Snell's law bends the ray, so it lands somewhere else on the
// backdrop. This gives the panel apparent thickness, and is by far the
// strongest cue that it is an object rather than a hole.
//
// What it reflects is not traced but placed by hand, from the way the
// mirrored ray points: the rim light round each pane's outermost edge (see
// `u_panel_light`), and over a dark frame a key light from above and a fill
// from below (see the light over a dark frame, below).
//
// Wavelength enters on the transmitted ray, and enters properly: the index
// of refraction is a function of it, so every wavelength lands somewhere
// slightly different and the colour at a fragment is the integral of what
// they each found, weighted by the eye's response under D65. See dispersion,
// below.

in vec2 v_uv;
out vec4 frag;

/// Everything behind the glass, already composited: the video and its ambient
/// border, or for a pane laid over the interface the finished frame (see
/// `glass_over` in pipeline.rs). Drawn as it is wherever there is no glass.
uniform sampler2D u_base;
/// Blurred copy of the same. The refracted ray samples this rather than
/// `u_base`, so the backdrop blur radius doubles as the material's frostiness.
uniform sampler2D u_blur;
uniform vec2 u_base_scale;
uniform vec2 u_blur_scale;

/// Target size in pixels; all panel geometry below is in pixels too.
uniform vec2 u_size;

#define MAX_PANELS 16
uniform int u_panel_count;
/// (x0, y0, x1, y1) in pixels, top-left origin.
uniform vec4 u_panel_rect[MAX_PANELS];
/// (corner_radius, tint_alpha, opacity, nested)
///
/// `opacity` is how far the panel has faded in. It scales coverage, so the
/// glass arrives with its surface rather than ahead of it. `nested` is 1 for
/// a pane laid on another, which casts no shadow: the pane beneath would hide
/// it, and past that pane's edge it would fall on the picture. Nor does it
/// reach past that edge itself (see `main`).
uniform vec4 u_panel_style[MAX_PANELS];
/// (ceiling, rim, unused, unused): the brightest luminance the pane may pass,
/// 0 for no limit (see legibility, below), and how strongly its rim light
/// shows: white added round its outermost edge, where the surface has turned
/// past 45 degrees and the mirrored ray points back down. Per pane, because
/// the pane under the keyboard's ring is held darker and lit harder than the
/// rest.
uniform vec4 u_panel_light[MAX_PANELS];

// --- Material parameters ---------------------------------------------------

/// Width of the domed rim, as a fraction of the panel's own corner radius.
///
/// A fraction rather than pixels because the panes aren't one size: a rim
/// that suits a 32px corner swallows a 15px pill. Tied to the radius, a small
/// pane has a small rolled edge, as real glass does.
uniform float u_bevel;
/// How far the corners ease into the straight edges: the corner's curve runs
/// this many radii along each, where 1 is a quarter of an ellipse and larger
/// is squarer and smoother. See `sdg_superbox`, below, and `CORNER_SOFTNESS`
/// in pipeline.rs, which the interface draws its highlights with too.
uniform float u_corner_softness;
/// Effective glass thickness for transmission, as a fraction of the bevel
/// width above. How far the refracted ray descends before reaching the
/// backdrop, and so how strongly the rim displaces what is behind it.
///
/// Proportional to the bevel for the same reason: thickness and rim width are
/// two dimensions of one piece of glass, and a rim that narrows without
/// thinning looks painted on.
uniform float u_refract;
/// Index of refraction. 1.0 bends nothing, 1.33 water, 1.5 window glass,
/// 1.8 flint, 2.4 diamond. Higher bends harder.
uniform float u_ior;
/// How strongly the index of refraction varies with wavelength, 0..1, as a
/// share of the most it can: at 1 the band's red end, `LAMBDA_MAX`, is at an
/// index of exactly 1, so every wavelength still bends and none is clamped.
/// As dispersive power, the reciprocal of the Abbe number glass is specified
/// by, that top is about 2.16, where real glass runs from 1/70 for crown to
/// 1/20 for dense flint: here the fringe is meant to be seen. That is the
/// fringe at the default blur; see `u_fringe_scale`.
uniform float u_aberration;
/// How far each wavelength's ray is moved off the d line's, against what
/// `u_aberration` alone gives: in step with how wide the blurred backdrop's
/// edges are, which a fringe is seen against (see `pipeline::fringe_scale`).
uniform float u_fringe_scale;
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

/// Where the shadow under the interface's words is cast, as (x0, y0, x1, y1)
/// in pixels, and how far under white the frame is held there: the words are
/// found by the white they add, and white on white adds none. The shadow pass
/// gives back what is held (see `shadow::HEADROOM`).
uniform int u_shaded_count;
uniform vec4 u_shaded[MAX_PANELS];
uniform float u_headroom;

// --- shadow -------------------------------------------------------------------
//
// A pane casts a soft shadow onto the picture, darkest at its edge and fading
// out all round, lowered by `SHADOW_DROP`. Every shadow is laid on the frame
// before any glass, so one pane's shadow never falls on another's glass (they
// stand at the same height), and a pane's glass, which refracts the frame as
// it was, isn't dimmed by its own shadow.
//
// Sized from each pane's corner radius, like the rim, so a pill casts a small
// shadow and a panel a larger one.

/// How far the shadow falls below its pane, as a fraction of the radius.
const float SHADOW_DROP = 0.0;
/// How far its edge fades, as a fraction of the radius, plus a few pixels so
/// a pane with square corners still casts something soft.
const float SHADOW_SOFTNESS = 1.0;
const float SHADOW_SOFTNESS_MIN = 4.0;

// --- absorption -------------------------------------------------------------
//
// Over snow, a white room or a title card, the glass shows a bright backdrop
// and white text on it disappears. The tint can't fix that: it mixes toward a
// fixed grey, so a pane still sits at that grey's luminance. Absorption does:
// a pane that transmits less the more there is to transmit, as tinted glass
// does.
//
// It applies to the transmitted light only. The rim light is added after, so
// a pane over a bright scene goes deep and keeps its edge.
//
// `u_absorb` is how much of a white backdrop is absorbed; a darker one loses
// less, along a hyperbola, and black loses nothing:
//
//     absorbed = 1 - (1 - a) / ((1 - a) + a * luma)
//
// The saturation lift after it gives back the colour the darkening takes. It
// grows with what was absorbed and with how bright the colour is, and only
// where there is colour already, so a grey stays grey.

/// Rec. 709, matching how the eye weights the three channels rather than
/// averaging them: a saturated green frame is far brighter than its mean.
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// --- the light over a dark frame ------------------------------------------------
//
// Absorption's opposite. Over black there is nothing for the glass to bend or
// blur, and a pane all but vanishes: a bar over a fade, the way on at the end
// of an episode, are words on nothing. So two soft lights fall on the dome,
// each raised 30 degrees off the screen toward the eye: a key from above,
// across the pane and brightest at its top, and a fainter fill from below,
// along its bottom edge. Each is weighted by how squarely the mirrored ray
// faces it, and together they are a sheen added to the glass, whole over
// black and gone once the glass under it is a twentieth of white, so over a
// lit frame, where the refraction already draws the pane, there is none.

// --- legibility ---------------------------------------------------------------
//
// Absorption is a look, and the slider can turn it down; legibility can't be
// turned down. So whatever absorption leaves, the glass lets through no more
// light than the lowest step of text needs to keep its contrast on it:
// each pane's ceiling, a luminance worked out from that step's alpha (see
// `legible_ceiling` in pipeline.rs). Over dark glass it does nothing. Short
// of the ceiling it eases in, from `KNEE` of it, so a bright area keeps its
// shading rather than going flat at a line. Like absorption it comes before
// the lights, so the rim keeps its edge.

const float KNEE = 0.7;

float decode_sRGB(float c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, 2.4), step(0.04045, c));
}

vec3 decode_sRGB(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c));
}

float encode_sRGB(float c) {
    return mix(c * 12.92, 1.055 * pow(c, 1.0 / 2.4) - 0.055, step(0.0031308, c));
}

vec3 encode_sRGB(vec3 c) {
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c));
}

/// Luminance, from the encoded colour.
float luminance(vec3 c) {
    return dot(decode_sRGB(c), LUMA);
}

/// The colour, darkened where it is brighter than the words on it can
/// stand: no brighter than `ceiling`, a luminance, or as it is at 0. See
/// legibility, above.
vec3 legible(vec3 c, float ceiling) {
    float lum = luminance(c);
    float knee = KNEE * ceiling;
    if (ceiling <= 0.0 || lum <= knee) {
        return c;
    }
    float room = ceiling - knee;
    float wanted = knee + room * (1.0 - exp(-(lum - knee) / room));
    // Scaling an encoded colour scales its luminance by about the 2.2th power
    // of the scale; a second step takes out what that leaves.
    float scale = encode_sRGB(wanted / lum);
    scale *= encode_sRGB(wanted / max(luminance(c * scale), 1e-6));
    return c * scale;
}

// --- antialiasing -------------------------------------------------------------
//
// The outline needs nothing more: `coverage` is a smooth function of
// distance, so the silhouette is antialiased analytically, and counting
// samples would only quantise it into steps.
//
// What aliases is the shading just inside it. The dome's slope runs to
// infinity at the rim, so the mirror direction and the rim light change
// within the last pixel or two, and one sample per pixel
// makes the edge crawl as a panel moves. So the shading there, and only
// there, is supersampled.

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
/// outer half, which holds the mirror's turn through horizontal, where the
/// slope passes 1 and the rim light starts (t = 0.84), as well as the rim
/// itself.
const float RIM_BAND = 0.5;

// --- dispersion ---------------------------------------------------------------
//
// The index of refraction is a function of wavelength, and the transmitted
// colour an integral over the band rather than one tap per channel, which
// past a pixel or two of spread shows as three coloured copies of the edge:
//
//     C = integral over lambda of  backdrop(offset(n(lambda))) * w(lambda)
//
// `w` is the CIE 1931 2-degree observer under D65, in linear sRGB, normalised
// so the band sums to white. Its three lobes overlap as the eye's cones do,
// so a smooth sweep of sample positions becomes a smooth sweep of hue, and
// reads as one edge seen through glass.
//
// The band is integrated in strata, each weighted by the exact difference of
// the cumulative curve at its two ends rather than by a point sample. The
// weights then add to exactly white at any number of strata, so the count
// changes how finely the smear resolves but never the colour of a flat
// backdrop. A running normalisation would instead divide by a small sum that
// includes the negative lobes.
//
// Linear sRGB can't hold a spectral colour: its red weight goes negative
// between about 470 and 600nm. That doesn't show, because the sampled
// backdrop is blurred, so neighbouring wavelengths land on similar values,
// and the result is floored at zero before anything divides by its luminance.

/// The visible band, in microns, as bin edges: 395nm to 705nm in 10nm steps.
const float LAMBDA_MIN = 0.395;
const float LAMBDA_MAX = 0.705;
#define SPECTRUM_STEPS 31

/// 1/lambda^2 at the sodium d line, 587.6nm — where `u_ior` is the index.
const float INV_D2 = 2.896647;

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
/// Stratified samples at the same wavelengths in every pixel draw the bands
/// they were meant to dissolve; a per-pixel rotation turns them into noise,
/// far below what the blurred backdrop is doing anyway. Hashed from the pixel
/// rather than a clock, so a still panel doesn't crawl.
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

// ---------------------------------------------------------------------------------------
// Signed distance + gradient for a box with superelliptic ("squircle") corners:
//
//   max((|x| - w)/a + 1, 0)^(2 sx) + max((|y| - h)/b + 1, 0)^(2 sy) = 1
//
//   p  : sample point (box centred at the origin)
//   wh : vec2(w, h), half size of the box
//   r  : corner radius
//   s  : corner softness, `u_corner_softness` (1 = elliptical corner, larger =
//        squarer/smoother)
//
// (sx, sy) is the softness the box can hold along each axis, at most s, and (a, b)
// how far the corner reaches along each; `sdg_superbox` works both out, and draws a
// box too short for the full corner in by `pull`: a = sx (r - pull.x), b = sy
// (r - pull.y).
//
// Returns vec3(d, n): d is the exact Euclidean signed distance (negative inside) and
// n = grad(d) is the unit normal (outward on the surface, normal of the nearest
// boundary point everywhere else).
//
// Assumes s >= 1.
//
// Method: the corner curve, in coordinates relative to the corner centre, is
// (a u, b v) with u^(2sx) + v^(2sy) = 1. It is split at the point
// u^(2sx) = v^(2sy) = 1/2 into two halves, and each half is a well-conditioned graph
// (slope <= ~1). On each half the closest point is found by a few curvature-aware
// Newton steps: each step rotates about the centre of the osculating circle, which is
// exact for circles and robust where the distance function is not convex (deep inside).
// Max error in float32 is ~1e-5 * r against a brute-force reference.
// ---------------------------------------------------------------------------------------

#define SUPERBOX_ITERS 2

// One half of the corner curve, written as a graph over its second coordinate:
//     c(t) = ( A (1 - t^M)^(1/N),  B t ),   0 <= t <= T
// q is given in the same (first, second) frame. Returns vec3(squared distance, unit normal).
vec3 superbox_half(vec2 q, float A, float B, float N, float M, float T) {
    float t = clamp(q.y/B, 0.0, T);                 // start straight across from q
    for (int i = 0; i < SUPERBOX_ITERS; i++)
    {
        float t2  = pow(t, M - 2.0);                // t^(M-2)
        float t1  = t2*t;                           // t^(M-1)
        float G   = 1.0 - t1*t;                     // 1 - t^M  (>= 1/2 on this half)
        float g   = pow(G, 1.0/N);
        float k   = t1/G;
        float X   = A*g;                                                  // x(y)
        float Xp  = -(A/B)*(M/N)*k*g;                                     // dx/dy
        float Xpp = -(A/(B*B))*(M/N)*g*((M - 1.0)*t2/G + (M - M/N)*k*k);  // d2x/dy2
        float L   = sqrt(1.0 + Xp*Xp);
        float kap = -Xpp/(L*L*L);                   // curvature (>= 0, the shape is convex)
        vec2  w   = q - vec2(X, B*t);
        float wt  = (w.x*Xp + w.y)/L;               // offset of q along the tangent
        float wn  = (w.y*Xp - w.x)/L;               // offset of q along the inward normal
        // arc length to move: rotate about the centre of the osculating circle
        float ds  = (kap*length(w) > 1e-4) ? atan(kap*wt, 1.0 - kap*wn)/kap
                                           : wt/(1.0 - kap*wn);
        t = clamp(t + ds/(B*L), 0.0, T);
    }
    float t1 = pow(t, M - 1.0);
    float G  = 1.0 - t1*t;
    float g  = pow(G, 1.0/N);
    vec2  w  = q - vec2(A*g, B*t);
    vec2  n  = normalize(vec2(N/A*G/g, M/B*t1));    // gradient of (x/A)^N + (y/B)^M
    return vec3(dot(w, w), n);
}

// How far a corner of softness `s` has fallen below its edge, in radii, three
// quarters of a radius short of the other edge: on the curve u^(2s) + v^(2s) = 1,
// spanning s radii each way, how far v is below 1 at u = 1 - 0.75/s. Negative, and
// further the softer the corner. Needs s >= 0.75, where the point reaches the
// corner's centre.
float corner_drop(float s) {
    return s*(pow(1.0 - pow(1.0 - 0.75/s, 2.0*s), 0.5/s) - 1.0);
}

vec2 corner_drop(vec2 s) {
    return vec2(corner_drop(s.x), corner_drop(s.y));
}

vec3 sdg_superbox(vec2 p, vec2 wh, float r, float s) {
    // A box shorter along an axis than the corner would reach there (s r)
    // cannot hold the full corner, so that axis's softness is cut to what fits.
    vec2 fit = min(vec2(s), wh/r);

    // The cut corner is fuller than the full one: three quarters of a radius
    // from the side, it has fallen less far below its edge (`corner_drop`). To
    // make up for it the box is drawn in by the difference, along the other
    // axis, where the fuller corner stands out: a pill whose height cuts its
    // softness is drawn in at its ends. The corner along that axis is drawn as
    // for a radius `pull` smaller too, which keeps its centre inside the box,
    // so a square box with a radius of half its side is still a circle.
    vec2 pull = r*(corner_drop(fit).yx - corner_drop(s));
    wh -= pull;

    vec2 sp = 2.0*step(0.0, p) - 1.0;               // sign(p), but +1 at 0
    vec2 ab = fit*(vec2(r) - pull);                 // how far the corner reaches (a, b)
    vec2 nm = 2.0*fit;                              // corner exponents
    vec2 q  = abs(p) - wh + ab;                     // relative to the corner centre
    vec2 e  = q - ab;                               // = abs(p) - wh: distance to the edge lines

    // outside, facing a straight edge
    if (q.y <= 0.0 && e.x >= 0.0) return vec3(e.x, sp.x, 0.0);
    if (q.x <= 0.0 && e.y >= 0.0) return vec3(e.y, 0.0, sp.y);
    // inside: nearest straight edge; away from the corner patch it is always the answer
    vec3 edge = (e.x > e.y) ? vec3(e.x, sp.x, 0.0) : vec3(e.y, 0.0, sp.y);
    if (q.x <= 0.0 && q.y <= 0.0) return edge;

    // closest point on the corner curve (both halves, keep the nearer)
    vec2 T  = pow(vec2(0.5), 1.0/nm);
    vec3 hA = superbox_half(q,    ab.x, ab.y, nm.x, nm.y, T.y);   // half next to x = w
    vec3 hB = superbox_half(q.yx, ab.y, ab.x, nm.y, nm.x, T.x);   // half next to y = h
    vec3 c  = (hA.x < hB.x) ? vec3(sqrt(hA.x), hA.yz) : vec3(sqrt(hB.x), hB.zy);

    vec2 uv = max(q/ab, 0.0);
    if (pow(uv.x, nm.x) + pow(uv.y, nm.y) >= 1.0)
        return vec3(c.x, c.yz*sp);                               // outside: the curve
    return (-c.x > edge.x) ? vec3(-c.x, c.yz*sp) : edge;         // inside: nearer of curve / edges
}

// Distance only
float sd_superbox(vec2 p, vec2 wh, float r, float s) { return sdg_superbox(p, wh, r, s).x; }

/// One published panel: its centre and half size in pixels, and its corner
/// radius, clamped to what the box can hold.
void panel_shape(int i, out vec2 centre, out vec2 half_size, out float radius) {
    vec4 rect = u_panel_rect[i];
    centre = 0.5 * (rect.xy + rect.zw);
    half_size = max(0.5 * (rect.zw - rect.xy), vec2(0.5));
    radius = clamp(u_panel_style[i].x, 0.0, min(half_size.x, half_size.y));
}

/// How much of one panel's shadow falls on pixel `px`, 0..1, before the
/// strength: whole under the panel's outline, dropped by `SHADOW_DROP`, and
/// fading out beyond its edge over `SHADOW_SOFTNESS` of its radius and
/// `SHADOW_SOFTNESS_MIN` pixels more.
float shadow_at(vec2 px, vec2 centre, vec2 half_size, float radius) {
    float soft = SHADOW_SOFTNESS * radius + SHADOW_SOFTNESS_MIN;
    vec2 drop = vec2(0.0, SHADOW_DROP * radius);
    float d = max(0.0, sd_superbox(px - centre - drop, half_size, radius, u_corner_softness));
    return smoothstep(-soft, 0.0, -d) * exp(-4.0 * d / soft);
}

float saturation(vec3 c) {
    float c_max = max(c.r, max(c.g, c.b));
    float c_min = min(c.r, min(c.g, c.b));
    return 1.0 - max(1e-5, c_min) / max(1e-5, c_max);
}

vec3 saturate(vec3 c, float s) {
    float c_max = max(c.r, max(c.g, c.b));
    float c_min = min(c.r, min(c.g, c.b));
    return max(1e-5, c_max - c_min * (1.0 - s)) / max(1e-5, c_max - c_min) * (c - vec3(c_min))
           + vec3(c_min) * (1.0 - s);
}

/// The glass surface at pixel `px` of one panel. A point just outside the
/// outline, as an edge sample can be, is shaded as the rim itself: `t` clamps
/// to 1 there, and every term below is finite at 1.
///
/// `light` is the pane's (ceiling, rim) from `u_panel_light`. `budget` is
/// the most wavelengths this sample may take and `dither` where in each
/// stratum it takes them; see dispersion, above.
vec3 glass_at(vec2 px, vec2 centre, vec2 half_size, float radius, float tinted,
              vec4 light, int budget, float dither) {
    vec2 uv = px / u_size;
    vec2 rel = px - centre;
    vec2 rel_uv = 0.5 * rel / half_size + vec2(0.5);
    vec3 sdg = sdg_superbox(rel, half_size, radius, u_corner_softness);
    float d = sdg.x;

    // Outward radial direction. Every 2D vector below lives in the
    // (radial, up) slice and is lifted back to screen space through this.
    vec2 box_normal = sdg.yz;

    // Both material widths are relative, so they resolve here, per
    // panel, from the radius this one happens to have.
    float bevel = u_bevel * radius;
    float thickness = u_refract * bevel;

    // Position across the bevel: 0 on the flat centre, 1 at the rim.
    float t = clamp(1.0 + d / max(bevel, 1e-3), 0.0, 1.0);

    // Dome profile, in units of bevel width:
    //
    //     height(t) = 0.5 + (1 - t^4)^(1/4)
    //     slope(t)  = d(height)/dt = -t^3 / (1 - t^4)^(3/4)
    //
    // The fourth power keeps the centre flat and pushes the curvature
    // into the last stretch of the bevel, which is what makes the
    // lensing hug the rim. `slope` runs to -infinity as the surface
    // turns vertical at t = 1, so the root is floored; every term below
    // converges as |slope| grows, so a large finite value behaves.
    float dome = max(pow(max(1.0 - t*t*t*t, 0.0), 0.25), 1e-4);
    float height = 0.5 + dome;
    float slope = -t*t*t / (dome*dome*dome);
    float slope2 = slope * slope;

    // Snell's law for a vertical incident ray. With surface normal
    // n = (-slope, 1)/sqrt(1 + slope^2):
    //
    //     cos_i = 1 / sqrt(1 + slope^2)
    //     cos_t = D / (ior * sqrt(1 + slope^2)),  D = sqrt(ior^2 + (ior^2 - 1) slope^2)
    float ior = max(u_ior, 1.0);

    // The vector form of Snell's law reduces, for a vertical incident
    // ray, to a lateral-over-vertical ratio of -slope*(D-1)/(slope^2+D),
    // which is `transmit_ratio`, above, since dispersion wants it once per
    // wavelength. Multiplied by how far the ray descends, it gives the
    // displacement. `slope` is negative on the bevel, so the ratio is
    // positive and the sample is pulled inward, along `reach`: the rim
    // shows magnified content from under the panel, exactly as a bevelled
    // pane does. `reach` is the part every wavelength shares: the
    // direction, and how far the ray descends, in pixels.
    vec2 reach = -box_normal * height * thickness;

    // Dispersion. Cauchy's B as the slider's share of the most there can be,
    // which brings the band's red end down to an index of 1, and the two ends
    // of the band from it — violet has the highest index and lands furthest
    // in. Each wavelength's ray is the d line's, moved by its own difference
    // from it scaled with the blur, rather than a scaled B, which would take
    // red's index under 1.
    float cauchy_b = u_aberration * (ior - 1.0) / (INV_D2 - 1.0 / (LAMBDA_MAX * LAMBDA_MAX));
    float ratio_d = transmit_ratio(slope, slope2, ior);
    float ratio_violet = transmit_ratio(slope, slope2, index_at(LAMBDA_MIN, ior, cauchy_b));
    float ratio_red = transmit_ratio(slope, slope2, index_at(LAMBDA_MAX, ior, cauchy_b));

    // How far apart those two land, in pixels, decides how many wavelengths
    // are worth taking. Under a pixel there is nothing to resolve and the
    // d-line ray is the answer, as across every flat middle, where the slope
    // and every offset are zero. Past that, about one wavelength per pixel of
    // smear, since fewer would show as bands, or per as many pixels as the
    // fringe is widened: over edges that much wider, neighbouring wavelengths
    // land on that much more alike values.
    float spread = length(reach) * abs(ratio_violet - ratio_red) * u_fringe_scale;
    int bands = int(clamp(ceil(spread / max(u_fringe_scale, 1.0)), 1.0, float(budget)));

    vec3 refracted;
    if (bands <= 1) {
        refracted = blur_at(uv + reach * ratio_d / u_size);
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
            float ratio = ratio_d + (transmit_ratio(slope, slope2, n) - ratio_d) * u_fringe_scale;
            refracted += share * blur_at(uv + reach * ratio / u_size);
        }
        // Out of gamut comes out negative; see dispersion, above.
        refracted = max(refracted, vec3(0.0));
    }

    refracted = mix(refracted, u_tint, clamp(tinted * u_tint_amount, 0.0, 1.0));

    // Then absorb, by how bright what is left is.
    float luma = dot(refracted, LUMA);
    // Floored, because at full absorption over black the curve is 0/0.
    float absorbed = (1.0 - (1.0 - u_absorb) / max(1.0 - u_absorb + u_absorb * luma, 1e-5))
                     * tinted;
    
    refracted *= 1.0 - absorbed;

    // The saturation lift; see absorption, above.
    float s = absorbed 
              * pow(luma, 4.0) 
              * (1.0 - pow(1.0 - saturation(refracted), 4.0));

    refracted = saturate(refracted, s);

    luma = dot(refracted, LUMA);

    // Never brighter than the lowest step of text can stand on; see
    // legibility, above.
    refracted = mix(refracted, legible(refracted, light.x), tinted);

    // Mirror direction for the same vertical view ray:
    // reflect((0,-1), n) = (-2*slope, 1 - slope^2) / (1 + slope^2),
    // which is exactly unit length at every slope. `mirror.x` is how far
    // it leans out across the screen, along the outward normal, and
    // `mirror.y` how far it points up; `mirror3` is the same lifted into
    // screen space.
    vec2 mirror = vec2(-2.0 * slope, 1.0 - slope2) / (1.0 + slope2);
    vec3 mirror3 = vec3(box_normal * mirror.x, mirror.y);

    // The key and the fill; see the light over a dark frame, above. Each root
    // and power is floored at zero, since rounding can take what it is given a
    // hair below, and `sqrt` or `pow` of a negative is NaN.
    float key = sqrt(max(0.5 + 0.5 * dot(mirror3, vec3(0.0, -0.866, 0.5)), 0.0)) 
                * pow(max(0.0, 1.0 - rel_uv.y), 1.0)
                * 0.1;

    float fill = sqrt(max(0.5 + 0.5 * dot(mirror3, vec3(0.0, 0.866, 0.5)), 0.0)) 
                 * pow(max(0.0, rel_uv.y), 7.0) 
                 * 0.08;

    float sheen = (key + fill) * (1.0 - min(1.0, 20.0 * luma));

    // The rim light, where the mirrored ray has tipped over and points back
    // down. Nothing while the surface is under 45 degrees and the ray still
    // points up, nor at 45, where it lies flat along the screen
    // (`mirror.x` = 1); from there it rises steeply as the ray swings round
    // to point straight down, to full where the surface stands vertical at
    // the outermost edge.
    float rim = max(0.0, -mirror.y * (1.0 - mirror.x) * (1.0 - mirror.x))
                * light.y;

    // The sheen and the rim are both light added over the glass.
    return refracted + vec3(sheen) + vec3(rim);
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
            if (u_panel_style[i].w > 0.5) {
                continue;
            }
            panel_shape(i, centre, half_size, radius);
            float shadow = shadow_at(px, centre, half_size, radius) * u_panel_style[i].z;
            col *= 1.0 - clamp(u_shadow, 0.0, 1.0) * shadow;
        }
    }

    // How much of this pixel the panes so far cover, which is as far as a
    // pane laid on one of them reaches: the one under the ring stays on its
    // panel while a page carries it in from past the panel's edge.
    float beneath = 0.0;
    for (int i = 0; i < MAX_PANELS; i++) {
        if (i >= u_panel_count) {
            break;
        }
        panel_shape(i, centre, half_size, radius);
        float tinted = u_panel_style[i].y;

        float d = sd_superbox(px - centre, half_size, radius, u_corner_softness);

        // Two pixels of feather, centred half a pixel outside the outline:
        // the coverage mask, which is all the outline needs (see
        // antialiasing, above).
        float shape = smoothstep(-1.5, 0.5, -d);
        if (u_panel_style[i].w > 0.5) {
            shape *= beneath;
        } else {
            beneath = max(beneath, shape);
        }
        float coverage = shape * u_panel_style[i].z;
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
                                  u_panel_light[i], RIM_BANDS,
                                  fract(dither + float(s) * 0.125));
            }
            glass *= 0.125;
        } else {
            glass = glass_at(px, centre, half_size, radius, tinted, u_panel_light[i],
                             MAX_BANDS, dither);
        }

        col = mix(col, glass, coverage);
    }

    for (int i = 0; i < MAX_PANELS; i++) {
        if (i >= u_shaded_count) {
            break;
        }
        vec4 region = u_shaded[i];
        if (all(greaterThanEqual(px, region.xy)) && all(lessThan(px, region.zw))) {
            col *= 1.0 - u_headroom;
            break;
        }
    }

    frag = vec4(col, 1.0);
}
