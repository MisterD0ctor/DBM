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
//        height
//          ^      ______________              <- flat centre, slope 0
//          |   __/              \__
//          | _/                    \_         <- steepening toward the rim
//          +--------------------------> radial   (t: 0 centre .. 1 rim)
//
// The viewer looks straight down, so a fragment's colour is whatever a
// vertical ray picks up at that surface. Three things happen at the
// interface, and all three are computed rather than faked:
//
//   * transmission — Snell's law bends the ray, so it lands somewhere else on
//     the backdrop. This gives the panel apparent thickness, and is by far
//     the strongest cue that it is an object rather than a hole.
//   * reflection — the mirrored ray either escapes upward, where it sees a
//     synthetic sky, or tips over and sees the backdrop again.
//   * Fresnel — how the two are weighted, from the exact unpolarised
//     reflectance. Grazing angles at the rim go strongly reflective, which is
//     why real glass edges look bright without any hand-placed highlight.
//
// Wavelength enters only as chromatic aberration on the transmitted ray.

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
/// Fraction by which the red and blue transmission offsets differ from green.
uniform float u_aberration;
/// Overall strength of the reflection, scaling the Fresnel weight.
uniform float u_specular;
/// Brightness of the synthetic sky seen where the reflection escapes upward.
uniform float u_sky;
/// Direction that sky highlight comes from, in screen space.
uniform vec2 u_light_dir;
uniform vec3 u_tint;
/// Global tint strength. The per-panel value in `u_panel_style` says whether
/// a panel takes tint at all; this says how much, and is the slider.
uniform float u_tint_amount;

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
// material read as glass — the Fresnel rim, the sky highlight, the mirrored
// backdrop — is added after this and is untouched, so a panel over a bright
// scene goes deep and keeps its edge rather than turning into a flat card.
//
// Not a uniform and not a slider, deliberately. The tint is a matter of taste
// and has a control; this is the floor under it, and a floor one click from
// zero is not one.

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
// infinity at the rim, so reflectance, the mirror direction and the swing from
// reflected backdrop to sky all happen within the last pixel or two — a
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
const float RIM_BAND = 0.25;

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

/// The glass surface at pixel `px` of one panel. A point just outside the
/// outline, as an edge sample can be, is shaded as the rim itself: `t` clamps
/// to 1 there, and every term below is finite at 1.
vec3 glass_at(vec2 px, vec2 centre, vec2 half_size, float radius, float tinted) {
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
    //     height(t) = 1 + 0.5 * sqrt(1 - t^4)
    //     slope(t)  = d(height)/dt = -t^3 / sqrt(1 - t^4)
    //
    // The fourth power keeps the centre genuinely flat and pushes the
    // curvature into the last stretch of the bevel, which is what makes
    // the lensing hug the rim. `slope` runs to -infinity as the surface
    // turns vertical at t = 1, so the root is floored; every term below
    // converges as |slope| grows, so a large finite value behaves.
    float dome = max(sqrt(max(1.0 - t * t * t * t, 0.0)), 1e-4);
    float height = 1.0 + 0.5 * dome;
    float slope = -t * t * t / dome;
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
    float transmission_ratio = -slope * (D - 1.0) / (slope2 + D);
    vec2 transmission_offset =
        -box_normal * transmission_ratio * height * thickness / u_size;

    // Chromatic aberration: shorter wavelengths bend more, so the same
    // offset is scaled up for red and down for blue.
    float ab = u_aberration;
    vec3 refracted = vec3(
        blur_at(uv + transmission_offset * (1.0 + ab)).r,
        blur_at(uv + transmission_offset).g,
        blur_at(uv + transmission_offset * (1.0 - ab)).b
    );

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
    float absorbed = (1.0 - 0.75 * (1.0 - exp(-1.5 * backdrop)) / backdrop)
                    * tinted;

    refracted *= 1.0 - absorbed;
    vec3 gray = vec3(max(max(refracted.r, refracted.g), refracted.b));

    // Saturate darkened areas
    refracted = mix(gray, refracted, 1.0 / (1.0 - 3.0 * absorbed * absorbed));

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

    // Pointing up, the mirrored ray escapes and sees a synthetic sky: a
    // broad highlight from `u_light_dir`, lit from both sides so the far
    // edge catches a dimmer rim of it too.
    float sky = dot(mirror3, normalize(vec3(u_light_dir, 0.0)));
    sky = sky < 0.0 ? -0.7 * sky : sky;
    sky *= 1.0 - mirror.y;
    sky *= sky * u_sky;

    // Tipped over and pointing down, it sees the backdrop again — a
    // screen-space reflection, displaced by the same lateral-over-
    // vertical reasoning as the refracted ray.
    //
    // The denominator vanishes as the mirrored ray passes horizontal
    // (|slope| -> 1), where the true displacement really is unbounded.
    // Floor it rather than let one ring of pixels smear.
    float denom = 1.0 - slope2;
    denom = abs(denom) < 1e-3 ? (denom < 0.0 ? -1e-3 : 1e-3) : denom;
    float reflection_ratio = -2.0 * slope / denom;
    vec2 reflection_offset =
        -box_normal * reflection_ratio * height * bevel / u_size;
    vec3 reflected = base_at(uv + reflection_offset) * u_specular;

    // Cross-fade the two as the mirrored ray swings through horizontal.
    vec3 specular = mix(reflected, vec3(sky), smoothstep(-0.7, 0.0, mirror.y));

    // Fresnel decides how much of each the viewer gets.
    return mix(refracted, specular, reflectance);
}

void main() {
    vec2 px = v_uv * u_size;
    vec3 col = base_at(v_uv);

    for (int i = 0; i < MAX_PANELS; i++) {
        if (i >= u_panel_count) {
            break;
        }
        vec4 rect = u_panel_rect[i];
        vec2 centre = 0.5 * (rect.xy + rect.zw);
        vec2 half_size = max(0.5 * (rect.zw - rect.xy), vec2(0.5));
        float radius = clamp(u_panel_style[i].x, 0.0, min(half_size.x, half_size.y));
        float tinted = u_panel_style[i].y;

        float d = sd_round_box(px - centre, half_size, radius);

        // One pixel of feather: the coverage mask, which is all the outline
        // needs (see antialiasing, above).
        float coverage = (1.0 - smoothstep(-1.5, 0.0, d)) * u_panel_style[i].z;
        if (coverage <= 0.0) {
            continue;
        }

        vec3 glass;
        if (d > -max(RIM_BAND * u_bevel * radius, 1.0)) {
            glass = vec3(0.0);
            for (int s = 0; s < 8; s++) {
                glass += glass_at(px + RIM_SAMPLES[s], centre, half_size, radius, tinted);
            }
            glass *= 0.125;
        } else {
            glass = glass_at(px, centre, half_size, radius, tinted);
        }

        col = glass; // mix(col, glass, coverage);
    }

    frag = vec4(col, 1.0);
}
