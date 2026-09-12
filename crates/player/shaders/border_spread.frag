// Ambient border, pass 2 of 2: light spread, plus compositing the video on
// top.
//
// Ported from the second `//!HOOK BORDER` stage of ambient-border.glsl. The
// numerical hardening in here was worked out against the mpv version and is
// carried over unchanged — the reparameterised `light_spread_bound`, the
// fixed-count tap loop, the guarded normalize and the zero-weight fallback
// are all load-bearing, not defensive noise.
//
// The one addition is the final composite: mpv now draws into a texture with
// a transparent surround instead of owning the window, so the video is laid
// over the ambient background here rather than by mpv's own compositor. That
// is what removes the need for the forked BORDER hook entirely.

in vec2 v_uv;
out vec4 frag;

/// Output of the edge-extension pass.
uniform sampler2D u_src;
/// The video itself, transparent outside its rect.
uniform sampler2D u_video;
uniform vec2 u_size;
uniform vec2 u_texel;
uniform vec4 u_rect;

uniform float edge_blur;
uniform float spread;
uniform float grain;
uniform float falloff;
uniform float falloff_softness;
uniform float max_taps;
/// 1 to draw the ambient border, 0 to show the video on black. Uniform across
/// the draw, so the branch below costs nothing on the GPU — and with it off
/// the whole light-spread loop and the edge-extension pass are skipped.
uniform float u_border;
/// Fraction of each source texture its active image covers. Targets are
/// allocated grow-only so a resize costs no reallocation, which leaves the
/// textures generally larger than the images drawn into them.
uniform vec2 u_src_scale;
uniform vec2 u_video_scale;

/// Sample in active-image coordinates, clamped to the image rather than to
/// the texture edge - the slack beyond the image holds nothing meaningful,
/// and the spread loop below deliberately reaches past the frame.
vec4 src(vec2 p) {
    return textureLod(u_src, clamp(p, vec2(0.0), vec2(1.0)) * u_src_scale, 0.0);
}

vec4 vid(vec2 p) {
    return textureLod(u_video, clamp(p, vec2(0.0), vec2(1.0)) * u_video_scale, 0.0);
}

// Tight upper bound on how far a source can sit and still contribute more
// than `c` of the peak.
//
// Solved in units of `d` (for y = u/d) rather than in absolute distance. The
// absolute form ends on `u = (v - 1) / falloff`, which is 0/0 at falloff == 0
// — and that NaN propagates into the tap bounds below and paints the whole
// border NaN. Same cubic, same Newton steps, same result for every other
// falloff; it just never divides by the parameter.
float light_spread_bound(float d) {
    float c = 0.01; // Tightness of the bound
    float s = falloff * max(d, 0.0);
    float a = 1.0 + s;
    float Q = a * a / c;

    // Solve s^2 y^3 + 2 s y^2 + y = Q for y >= 1. Initial guess reasonable in
    // both regimes: for small s, y ~ Q; for large s, y ~ (Q/s^2)^(1/3). This
    // interpolates between them and stays finite at s == 0 (y = Q).
    float y = Q / (1.0 + pow(s * Q, 2.0 / 3.0));

    // Two Newton steps for high precision. The derivative is >= 1 for all
    // y, s >= 0, so this cannot divide by zero either.
    for (int i = 0; i < 2; i++) {
        float f  = ((s * s * y + 2.0 * s) * y + 1.0) * y - Q;
        float df = (3.0 * s * s * y + 4.0 * s) * y + 1.0;
        y -= f / df;
    }

    // y is the bound in units of d — undo the scaling for the half-width.
    return d * sqrt(max(y * y - 1.0, 0.0));
}

float distance_falloff(float x) {
    return 1.0 / (x * x + 2.0 * abs(x) + 1.0);
}

float soft_distance_falloff(float x) {
    if (falloff_softness == 0.0) {
        return 1.0 / ((x + 1.0) * (x + 1.0));
    } else {
        float th = abs(x / falloff_softness) < 5.0
            ? tanh(x / falloff_softness)
            : sign(x);
        float den = x * th + 1.0;
        return 1.0 / (den * den);
    }
}

float spread_falloff(float x, float d) {
    // spread == 0 collapses the light to a line: a tap sitting exactly on it
    // (x == 0) gives a zero-length vector and an infinite weight, which turns
    // into NaN in the pow() below. Floor the length so such a tap merely
    // dominates instead.
    return d / max(length(vec2(x, d * spread)), 1e-8);
}

// Wide usage friendly PRNG, shamelessly stolen from a GLSL tricks forum post
float mod289(float x)  { return x - floor(x / 289.0) * 289.0; }
float permute(float x) { return mod289((34.0 * x + 1.0) * x); }
float rand(float x)    { return fract(x / 41.0); }

vec4 light_spread(vec2 pos, float edge_dist, vec2 dir, float rnd) {
    float pt = length(u_texel * dir);
    float center = length(pos * dir);

    edge_dist += edge_blur + pt;

    float spread_bound = light_spread_bound(edge_dist * spread);
    float t0 = max(0.0, center - spread_bound);
    float t1 = min(1.0, center + spread_bound);

    vec4 c_sum = vec4(0.0);
    float w_sum = 0.0;

    // Fixed tap count instead of accumulating `t` until it passes `t1`: a
    // degenerate span (zero spread, or a zero-length bound) makes dt zero,
    // and `t += 0.0` never terminates — a hung GPU, not just a bad frame.
    // For a non-degenerate span this walks the exact same t0..t1 taps.
    int taps = int(clamp(max_taps, 1.0, 4096.0));
    float dt = (t1 - t0) / float(taps);

    for (int i = 0; i <= taps; i++) {
        float t = t0 + float(i) * dt;
        float jitter = (rnd + t) * 43758.5453; // Random jitter from position and t
        jitter = fract(jitter) * dt - dt / 2.0; // Jitter in range [-dt/2, dt/2]
        float t_jittered = clamp(t + jitter, t0, t1);
        float weight = distance_falloff(length(vec2((t_jittered - center), edge_dist)))
                     * spread_falloff(abs(t_jittered - center), edge_dist);
        weight = pow(weight, 2.2);
        c_sum += src(pos * dir.yx + t_jittered * dir.xy) * weight;
        w_sum += weight;
    }
    // Every weight can underflow to zero far from the edge; fall back to the
    // unfiltered texel rather than dividing 0 by 0.
    return w_sum > 0.0 ? c_sum / w_sum : src(pos);
}

void main() {
    vec2 pos = v_uv;
    vec4 r = u_rect;

    vec4 video_texel = vid(pos);
    if (u_border < 0.5) {
        frag = vec4(video_texel.rgb, 1.0);
        return;
    }

    // Coverage mask for the video rect, feathered by a texel so the seam
    // against the border does not alias. mpv paints the letterbox opaque
    // whatever `--background` is set to, so its alpha is not usable as a
    // mask; `u_rect` comes from mpv's own `osd-dimensions` and therefore
    // tracks panscan and zoom.
    vec2 lo = smoothstep(r.xy - u_texel, r.xy + u_texel, pos);
    vec2 hi = vec2(1.0) - smoothstep(r.zw - u_texel, r.zw + u_texel, pos);
    float in_video = lo.x * lo.y * hi.x * hi.y;

    // Most pixels are video, and none of the spread work below survives the
    // mix for them. Bailing here is the difference between paying a 256-tap
    // loop across the whole frame and paying it only across the letterbox.
    if (in_video >= 1.0) {
        frag = vec4(video_texel.rgb, 1.0);
        return;
    }

    // A collapsed rect (nothing loaded yet, mid-resize) would divide by zero
    // here and in `delta` below.
    vec2 rect_size = max(r.zw - r.xy, vec2(1e-6));

    float video_aspect = rect_size.x / rect_size.y * (u_size.x / u_size.y);

    vec2 box_pos = clamp(pos, r.xy, r.zw);
    vec2 delta = (pos - box_pos) / rect_size * vec2(video_aspect, 1.0);
    float dist = length(delta);

    // Inside the video rect delta is exactly zero, and normalize(vec2(0.0))
    // divides by zero. Those pixels sit under the video, so the direction
    // picked for them does not matter — only that it stays finite.
    vec2 dir = dist > 0.0 ? abs(normalize(delta.yx)) : vec2(0.0, 1.0);

    // Initialize the PRNG by hashing the position + the pixel color
    vec3 m = vec3(pos, 1.0) + src(pos).rgb;
    float h = permute(permute(permute(m.x) + m.y) + m.z);

    vec4 noise;
    noise.x = rand(h); h = permute(h);
    noise.y = rand(h); h = permute(h);
    noise.z = rand(h); h = permute(h);
    noise.w = 0.5;

    vec4 border = light_spread(pos, dist, dir, h)
                * soft_distance_falloff(dist * falloff)
                + (grain / 8192.0) * (noise - vec4(0.5));

    // Lay the video over the ambient background it just drew. This is the
    // step mpv's own compositor used to do, and the reason the forked BORDER
    // hook existed at all.
    frag = vec4(mix(border.rgb, video_texel.rgb, in_video), 1.0);
}
