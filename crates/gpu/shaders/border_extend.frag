// Ambient border, pass 1 of 2: edge extension.
//
// Ported from the `//!HOOK BORDER` stage of ambient-border.glsl. mpv's hook
// macros become plain uniforms:
//
//   HOOKED_raw  -> u_src     (the video, with a transparent surround)
//   HOOKED_pos  -> v_uv
//   HOOKED_pt   -> u_texel
//   HOOKED_size -> u_size
//   BORDER_rect -> u_rect
//
// For every pixel outside the video rect this walks inward from the nearest
// edge and averages, so the border starts from a softened edge colour rather
// than a single hard texel. A subtitle on the edge is left out of it, so its
// letters never light the border.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_src;
/// Target size in pixels.
uniform vec2 u_size;
/// 1.0 / u_size.
uniform vec2 u_texel;
/// Video rect within the plane, normalised: (x0, y0, x1, y1).
uniform vec4 u_rect;

uniform float edge_blur;
uniform float max_taps;
/// Fraction of the source texture its active image covers. Targets are
/// allocated grow-only so a resize costs no reallocation, which leaves the
/// texture generally larger than the image drawn into it.
uniform vec2 u_src_scale;

/// The picture's alpha as mpv hands it on (subtitle_mask.glsl). Anything
/// above it is a subtitle drawn over the picture; the step past it is two
/// levels of an 8-bit target, which the picture's own rounding stays under.
const float PICTURE = 0.5;
const float OVER = 2.0 / 255.0;
/// How far past the usual reach a column under a subtitle looks for picture
/// of its own, in reaches, and how much it needs to find, as a share of what
/// the reach alone would have weighed.
const float BEYOND = 4.0;
const float ENOUGH = 0.25;

/// Sample in active-image coordinates, clamped to the image rather than to
/// the texture edge - the slack beyond the image holds nothing meaningful.
vec4 src(vec2 p) {
    return textureLod(u_src, clamp(p, vec2(0.0), vec2(1.0)) * u_src_scale, 0.0);
}

#define get_weight(x) (exp(-(x) * (x) * 4.9))

void main() {
    vec2 pos = v_uv;
    vec4 r = u_rect;

    // Two texels in, which folds in the original's separate pre-clamp pass
    // and keeps mpv's outermost row, blended against the transparent
    // surround, out of the average.
    vec2 box_pos = clamp(pos, r.xy + 2.0 * u_texel, r.zw - 2.0 * u_texel);
    vec2 delta = box_pos - pos;
    vec2 dir = sign(delta);

    // Inside the picture the clamp changes nothing, so `dir` is zero and every
    // tap would land on the same texel, whose colour is the exact answer. The
    // spread pass never reads this target inside the rect anyway.
    if (dir == vec2(0.0)) {
        frag = vec4(src(box_pos).rgb, 1.0);
        return;
    }

    float radius = length(edge_blur * 3.0 * u_size);

    // The radius scales with the full frame size, so a large edge_blur would
    // otherwise spin this loop into the thousands and risk the driver
    // watchdog. Widen the step rather than shortening the radius, so the blur
    // keeps its extent and only loses sample density.
    float stride = max(2.0, radius / max_taps);

    // Two averages at once: of the picture alone, and of everything, which
    // is what stands if there is no picture to be had. That is also the
    // whole answer when mpv didn't take the mask, since the picture then
    // comes out opaque and reads as subtitle throughout.
    vec3 own = vec3(0.0);
    float own_w = 0.0;
    vec3 whole = vec3(0.0);
    float whole_w = 0.0;
    for (float i = 0.0; i <= radius; i += (i == 0.0 ? 1.0 : stride)) {
        vec4 c = src(box_pos + i * dir * u_texel);
        float weight = i == 0.0 ? 1.0 : get_weight(i / radius);
        whole += c.rgb * weight;
        whole_w += weight;
        if (c.a <= PICTURE + OVER) {
            own += c.rgb * weight;
            own_w += weight;
        }
    }

    // Under a subtitle, the column's own picture from further in: the light
    // there is the film's, where its letters' would not be. Evenly weighted,
    // since it stands in for the edge rather than softening it.
    if (own_w < ENOUGH * whole_w) {
        float tail = get_weight(1.0);
        for (float i = radius; i <= BEYOND * radius; i += stride) {
            vec2 p = box_pos + i * dir * u_texel;
            if (any(lessThan(p, r.xy)) || any(greaterThan(p, r.zw))) {
                break;
            }
            vec4 c = src(p);
            if (c.a <= PICTURE + OVER) {
                own += c.rgb * tail;
                own_w += tail;
            }
        }
    }

    // Opaque out: every tap sits inside the video rect, and the surround this
    // feeds is a background, not something to composite through.
    frag = vec4(own_w > 0.0 ? own / own_w : whole / whole_w, 1.0);
}
