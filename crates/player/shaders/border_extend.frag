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
// than a single hard texel.

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

/// Sample in active-image coordinates, clamped to the image rather than to
/// the texture edge - the slack beyond the image holds nothing meaningful.
vec4 src(vec2 p) {
    return textureLod(u_src, clamp(p, vec2(0.0), vec2(1.0)) * u_src_scale, 0.0);
}

#define get_weight(x) (exp(-(x) * (x) * 4.9))

void main() {
    vec2 pos = v_uv;
    vec4 r = u_rect;

    // Two texels rather than one: the original relied on a separate `//!HOOK
    // MAIN` pass that pre-clamped the video by 2 texels, and folding it in
    // here keeps the outermost row — which mpv blends against the transparent
    // surround — out of the average.
    vec2 box_pos = clamp(pos, r.xy + 2.0 * u_texel, r.zw - 2.0 * u_texel);
    vec2 delta = box_pos - pos;
    vec2 dir = sign(delta);

    float radius = length(edge_blur * 3.0 * u_size);

    // The radius scales with the full frame size, so a large edge_blur would
    // otherwise spin this loop into the thousands and risk the driver
    // watchdog. Widen the step rather than shortening the radius, so the blur
    // keeps its extent and only loses sample density.
    float stride = max(2.0, radius / max_taps);

    vec4 c_sum = src(box_pos);
    float w_sum = 1.0;
    for (float i = 1.0; i <= radius; i += stride) {
        float weight = get_weight(i / radius);
        c_sum += src(box_pos + i * dir * u_texel) * weight;
        w_sum += weight;
    }

    // Opaque out: every tap sits inside the video rect, and the surround this
    // feeds is a background, not something to composite through.
    frag = vec4((c_sum / w_sum).rgb, 1.0);
}
