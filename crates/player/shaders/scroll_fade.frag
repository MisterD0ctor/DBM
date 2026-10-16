// The edge of a scrolling list, where its rows run out under the panel: they
// blur and thin away over a short strip instead of being cut by a line.
//
// Run after Slint has drawn, on the strip alone. The strip is copied out of
// the finished frame with a margin on its inner side, blurred across into a
// scratch target, then blurred down and written back over itself — a
// separable Gaussian, with one difference from `blur_gauss`: the radius is
// not one number but grows from nothing at the strip's inner edge to its
// most at the list's edge. Each pixel is blurred by the radius its own place
// in the strip calls for, in both passes.
//
// The second pass also lets the rows go. What they thin away to is the frame
// as it was before Slint drew on it — the glass under the panel — so the
// strip ends in the panel's own surface and not in a colour chosen to look
// like it.
//
// Everything is measured in pixels of the copied region, from its bottom-left
// as GL counts them. The copy is taken unflipped: a blit that flips is one
// more thing a driver may refuse from the window's framebuffer.
//
// A strip can be at any side. A list's are at its top and bottom; the title
// in the bar runs out sideways, and its strip is at its right.

in vec2 v_uv;
out vec4 frag;

/// The copied region in the first pass, the scratch target in the second.
uniform sampler2D u_src;
/// Fraction of that texture the region covers.
uniform vec2 u_src_scale;
/// The region's size, in pixels.
uniform vec2 u_region;
/// (1, 0) across into scratch, (0, 1) down onto the window.
uniform vec2 u_dir;
/// Where in the region this pass's bottom-left pixel is. Nothing in the
/// first pass, which draws the whole region; the strip's own corner in the
/// second.
uniform vec2 u_at0;
/// How much this pass draws: the region, or the strip.
uniform vec2 u_span;
/// Distance from the edge the strip is at, as `u_edge.x + dot(u_edge.yz, at)`.
uniform vec3 u_edge;
/// How deep the strip is, in pixels, away from that edge.
uniform float u_band;
/// Most blur, as a standard deviation in pixels, reached at that edge.
uniform float u_sigma;
/// How much of the blur is on, 0..1: the list's distance from this end of
/// itself. The strip is also only that much of its full depth, which is the
/// caller's doing, so it arrives as the rows begin to pass under it.
uniform float u_strength;

/// The frame before the interface was drawn on it. Second pass only.
uniform sampler2D u_ground;
uniform vec2 u_ground_scale;
/// Where the region's bottom-left pixel is in the window, measured from the
/// window's top-left, and the window's size.
uniform vec2 u_origin;
uniform vec2 u_window;

const int MAX_TAPS = 24;

/// A pixel of the region, and never anything past it. Held to the centres of
/// the outermost pixels rather than to the region's edge: the texture is
/// larger than the region and what lies beyond is whatever was there last,
/// and a sample on the edge itself is half one of those.
vec3 src(vec2 at) {
    vec2 inside = clamp(at, vec2(0.5), u_region - vec2(0.5));
    return texture(u_src, inside / u_region * u_src_scale).rgb;
}

void main() {
    // This pixel, in the region's own pixels.
    vec2 at = u_at0 + v_uv * u_span;
    float dist = max(u_edge.x + dot(u_edge.yz, at), 0.0);
    float along = clamp(dist / u_band, 0.0, 1.0);

    float sigma = u_sigma * u_strength * (1.0 - along);
    vec3 acc = src(at);
    if (sigma > 0.05) {
        int taps = int(min(ceil(sigma * 3.0), float(MAX_TAPS)));
        float sum_w = 1.0;
        for (int i = 1; i <= MAX_TAPS; i++) {
            if (i > taps) {
                break;
            }
            float x = float(i);
            float w = exp(-0.5 * x * x / (sigma * sigma));
            acc += (src(at + u_dir * x) + src(at - u_dir * x)) * w;
            sum_w += 2.0 * w;
        }
        acc /= sum_w;
    }

    if (u_dir.y < 0.5) {
        frag = vec4(acc, 1.0);
        return;
    }

    // Eased at both ends, so neither the strip's inner edge nor the outer
    // one shows as a line — and nothing at all is kept at the outer one,
    // however little of the strip there is yet.
    float kept = smoothstep(0.0, 1.0, along);
    // The same pixel of the frame underneath. That texture's first row is the
    // top of the picture, so the row is counted down from the region's top.
    vec2 window = vec2(u_origin.x + at.x, u_origin.y - at.y);
    vec3 ground = texture(u_ground, window / u_window * u_ground_scale).rgb;
    frag = vec4(mix(ground, acc, kept), 1.0);
}
