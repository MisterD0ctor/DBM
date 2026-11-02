// A thin shadow under the interface's words and marks, straight down: no
// offset, only a soft dark rim that keeps white text readable over a bright
// frame.
//
// Slint has no shadow for text or images — only for rectangles — so it is
// made here, from the finished frame. Everything the interface draws is
// white at some opacity, from the floor of `Paint.dim` up to `Paint.lit`,
// and it is drawn over the pipeline's output. So comparing the two says how
// much white went on at each pixel: where the frame is `ground` and the
// interface laid white over it at `a`, the frame now reads
// `ground + a * (1 - ground)`, and `a` falls straight out of that.
//
// That coverage, blurred, is the shadow. It darkens the ground under it
// wherever the interface did not cover it — the result is the white laid
// back over a darker ground, exactly as if the shadow had been drawn first.
// Pixels the interface covered entirely are left as they are.
//
// White laid on white changes nothing, so over clear glass on a white scene
// the words would leave no trace to be found by. So the glass pass hands Slint
// the frame held `u_headroom` under white wherever this pass looks, and this
// pass measures against that and gives the rest back: the ground it lays the
// white over is the frame as it is.
//
// Only coverage past the washes casts a shadow: a row's highlight, the
// keyboard's included, or a track's unfilled length is not a word, and a dark
// rim round one would read as a border it does not have.
//
// Two passes, like every Gaussian here: the coverage blurred across into a
// scratch target, then down and applied onto the window. Pixels are counted
// from the region's bottom-left, as GL counts them; see `scroll_fade` for
// why the copy is not flipped.

in vec2 v_uv;
out vec4 frag;

/// First pass: the region as Slint drew it. Second: the blurred coverage.
uniform sampler2D u_src;
uniform vec2 u_src_scale;
/// Second pass: the region as Slint drew it.
uniform sampler2D u_frame;
uniform vec2 u_frame_scale;
/// The region's size, in pixels.
uniform vec2 u_region;
/// (1, 0) across, (0, 1) down.
uniform vec2 u_dir;
/// The shadow's spread, as a standard deviation in pixels.
uniform float u_sigma;
/// How dark it is where it is deepest: the share of the ground taken away.
uniform float u_depth;
/// How far under white the glass pass held the ground here.
uniform float u_headroom;

/// The frame before the interface was drawn on it, held under white, and
/// where the region's bottom-left pixel is in the window, from the window's
/// top-left.
uniform sampler2D u_ground;
uniform vec2 u_ground_scale;
uniform vec2 u_origin;
uniform vec2 u_window;

const int MAX_TAPS = 12;

vec2 inside(vec2 at) {
    return clamp(at, vec2(0.5), u_region - vec2(0.5));
}

vec3 ground_at(vec2 at) {
    vec2 window = vec2(u_origin.x + at.x, u_origin.y - at.y);
    return texture(u_ground, window / u_window * u_ground_scale).rgb;
}

/// How much white the interface laid over the ground here, 0..1. A channel
/// with no room left above the ground says nothing either way; one that
/// went darker is not the interface's white, and counts as none.
float coverage(vec3 frame, vec3 ground) {
    vec3 room = 1.0 - ground;
    vec3 a = (frame - ground) / max(room, vec3(1e-3));
    a *= step(vec3(0.02), room);
    return clamp(max(a.r, max(a.g, a.b)), 0.0, 1.0);
}

/// What casts a shadow at a pixel: the words and marks, not the washes.
/// The ring's wash, the strongest, is a third of full white; Dim, the
/// faintest mark, a half.
float caster(vec2 at) {
    at = inside(at);
    vec3 frame = texture(u_src, at / u_region * u_src_scale).rgb;
    return smoothstep(0.36, 0.48, coverage(frame, ground_at(at)));
}

float shade(vec2 at) {
    at = inside(at);
    return texture(u_src, at / u_region * u_src_scale).r;
}

void main() {
    vec2 at = v_uv * u_region;
    float sigma = max(u_sigma, 1e-3);
    int taps = int(min(ceil(sigma * 3.0), float(MAX_TAPS)));
    bool across = u_dir.y < 0.5;

    float acc = across ? caster(at) : shade(at);
    float sum_w = 1.0;
    for (int i = 1; i <= MAX_TAPS; i++) {
        if (i > taps) {
            break;
        }
        float x = float(i);
        float w = exp(-0.5 * x * x / (sigma * sigma));
        vec2 off = u_dir * x;
        acc += (across
            ? caster(at + off) + caster(at - off)
            : shade(at + off) + shade(at - off)) * w;
        sum_w += 2.0 * w;
    }
    float shadow = acc / sum_w;

    if (across) {
        frag = vec4(shadow, 0.0, 0.0, 1.0);
        return;
    }

    vec2 here = inside(at);
    vec3 frame = texture(u_frame, here / u_region * u_frame_scale).rgb;
    vec3 held = ground_at(here);
    float covered = coverage(frame, held);
    // What white over the ground cannot explain is a picture the interface
    // drew — the seek preview's frame — and is left as it was drawn: the
    // shadow is under the interface, never on it. Measured against what that
    // much white would have made, so a wash, which is white, does not count.
    vec3 white = held + covered * (1.0 - held);
    vec3 off = abs(frame - white);
    float picture = smoothstep(0.03, 0.1, max(off.r, max(off.g, off.b)));
    // The white as it was laid, over the ground as it is, darkened by the
    // shadow.
    vec3 ground = min(held / (1.0 - u_headroom), vec3(1.0));
    vec3 laid = covered + (1.0 - covered) * ground * (1.0 - u_depth * shadow);
    frag = vec4(mix(laid, frame, picture), 1.0);
}
