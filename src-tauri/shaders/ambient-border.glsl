//!PARAM edge_blur
//!DESC the starting blur from the edge
//!TYPE float
//!MINIMUM 0.0
//!MAXIMUM 1.0
0.01

//!PARAM spread
//!DESC spread of light columns
//!TYPE float
//!MINIMUM 0.0
1.5

//!PARAM grain
//!DESC grain amount
//!TYPE float
//!MINIMUM 0.0
256.0

//!PARAM falloff
//!DESC light falloff
//!TYPE float
//!MINIMUM 0.0
4.0

//!PARAM falloff_softness
//!DESC light falloff softness
//!TYPE float
//!MINIMUM 0.0
0.2

//!PARAM max_taps
//!DESC maximum number of samples for edge extension (performance cost)
//!TYPE float
//!MINIMUM 1
256.0

//!HOOK MAIN
//!BIND HOOKED
//!DESC edge pixel fill

vec4 hook() {
    vec2 pos = HOOKED_pos;
    vec2 pt = HOOKED_pt * 2;
    
    pos = clamp(pos, pt, vec2(1.0) - pt);

    return textureLod(HOOKED_raw, pos, 0.0);
}

//!HOOK BORDER
//!BIND HOOKED
//!DESC ambient edge extension pass1

#define get_weight(x) (exp(-(x) * (x) * 4.9))

vec4 hook() {
    vec2 pos = HOOKED_pos;
    vec4 r = BORDER_rect;

    // Border detection
    vec2 box_pos = clamp(pos, r.xy + HOOKED_pt, r.zw - HOOKED_pt);
    vec2 delta = box_pos - pos;
    float dist = length(delta);

    vec2 dir = sign(delta);

    // Edge extension
    float sigma = edge_blur;
    float radius = length(sigma * 3 * HOOKED_size);

    // The radius scales with the full frame size, so a large edge_blur would
    // otherwise spin this loop into the thousands and risk the driver
    // watchdog. Widen the step rather than shortening the radius, so the
    // blur keeps its extent and only loses sample density.
    float stride = max(2.0, radius / max_taps);

    float weight;
    vec4 c_sum = textureLod(HOOKED_raw, box_pos, 0.0);
    float w_sum = 1.0;
    for(float i = 1.0; i <= radius; i += stride) {
        weight = get_weight(i / radius);
        c_sum += textureLod(HOOKED_raw, box_pos + i * dir * HOOKED_pt, 0.0) * weight;
        w_sum += weight;
    }

    return c_sum / w_sum;
}

//!HOOK BORDER
//!BIND HOOKED
//!DESC ambient edge extension pass2

// Tight upper bound on how far a source can sit and still contribute more
// than `c` of the peak.
//
// Solved in units of `d` (for y = u/d) rather than in absolute distance.
// The absolute form ends on `u = (v - 1) / falloff`, which is 0/0 at
// falloff == 0 — and that NaN propagates into the tap bounds below and
// paints the whole border NaN. Same cubic, same Newton steps, same result
// for every other falloff; it just never divides by the parameter.
float light_spread_bound(float d) {
    float c = 0.01; // Tightness of the bound
    float s = falloff * max(d, 0.0);
    float a = 1.0 + s;
    float Q = a * a / c;

    // Solve s²y³ + 2sy² + y = Q for y ≥ 1. Initial guess reasonable in both
    // regimes: for small s, y ≈ Q; for large s, y ≈ (Q/s²)^(1/3). This
    // interpolates between them and stays finite at s == 0 (y = Q).
    float y = Q / (1.0 + pow(s * Q, 2.0/3.0));

    // Two Newton steps for high precision. The derivative is ≥ 1 for all
    // y, s ≥ 0, so this can't divide by zero either.
    for (int i = 0; i < 2; i++) {
        float f  = ((s*s*y + 2.0*s) * y + 1.0) * y - Q;
        float df = (3.0*s*s*y + 4.0*s) * y + 1.0;
        y -= f / df;
    }

    // y is the bound in units of d — undo the scaling for the half-width.
    return d * sqrt(max(y*y - 1.0, 0.0));
}

float distance_falloff(float x) {
    return 1 / (x * x + 2 * abs(x) + 1);
}

float soft_distance_falloff(float x) {
    if (falloff_softness == 0.0) {
        return 1 / ((x + 1) * (x + 1));
    } else {
        float c = 1 / falloff_softness;
        float th = abs(c * x) < 5 ? tanh(c * x) : sign(x);
        float den = x * th + 1;
        return 1 / (den * den);
    }
}

float spread_falloff(float x, float d) {
    // spread == 0 collapses the light to a line: a tap sitting exactly on
    // it (x == 0) gives a zero-length vector and an infinite weight, which
    // turns into NaN in the pow() below. Floor the length so such a tap
    // merely dominates instead.
    return d / max(length(vec2(x, d * spread)), 1e-8);
}

// Wide usage friendly PRNG, shamelessly stolen from a GLSL tricks forum post
float mod289(float x)  { return x - floor(x / 289.0) * 289.0; }
float permute(float x) { return mod289((34.0*x + 1.0) * x); }
float rand(float x)    { return fract(x / 41.0); }

vec4 light_spread(sampler2D image, vec2 pos, float edge_dist, vec2 dir, float rand) {
    float pt = length(HOOKED_pt * dir);

    float center = length(pos * dir);

    edge_dist += edge_blur + pt;

    float spread_bound = light_spread_bound(edge_dist * spread);
    float t0 = max(0, center - spread_bound);
    float t1 = min(1, center + spread_bound);

    vec4 c_sum = vec4(0.0);
    float w_sum = 0.0;

    // Fixed tap count instead of accumulating `t` until it passes `t1`: a
    // degenerate span (zero spread, or a zero-length bound) makes dt zero,
    // and `t += 0.0` never terminates — a hung GPU, not just a bad frame.
    // For a non-degenerate span this walks the exact same t0…t1 taps.
    int taps = int(clamp(max_taps, 1.0, 4096.0));
    float dt = (t1 - t0) / float(taps);

    for(int i = 0; i <= taps; i++) {
        float t = t0 + float(i) * dt;
        float jitter = (rand + t) * 43758.5453; // Random jitter based on position and t
        jitter = fract(jitter) * dt - dt / 2.0; // Jitter in range [-dt/2, dt/2]
        float t_jittered = clamp(t + jitter, t0, t1);
        float weight = distance_falloff(length(vec2((t_jittered - center), edge_dist)))
                       * spread_falloff(abs(t_jittered - center), edge_dist);
        weight = pow(weight, 2.2);
        c_sum += textureLod(image, pos * dir.yx + t_jittered * dir.xy, 0.0) * weight;
        w_sum += weight;
    }
    // Every weight can underflow to zero far from the edge; fall back to the
    // unfiltered texel rather than dividing 0 by 0.
    return w_sum > 0.0 ? c_sum / w_sum : textureLod(image, pos, 0.0);
}

vec4 hook() {
    vec2 pos = HOOKED_pos;
    vec4 r = BORDER_rect;

    // A collapsed rect (nothing loaded yet, mid-resize) would divide by zero
    // here and in `delta` below.
    vec2 rect_size = max(r.zw - r.xy, vec2(1e-6));

    float video_aspect = rect_size.x / rect_size.y
                       * (HOOKED_size.x / HOOKED_size.y);

    // Border detection
    vec2 box_pos = clamp(pos, r.xy, r.zw);
    vec2 delta = (pos - box_pos) / rect_size * vec2(video_aspect, 1.0);
    float dist = length(delta);

    // Inside the video rect delta is exactly zero, and normalize(vec2(0.0))
    // divides by zero. Those pixels sit under the video, so the direction
    // picked for them doesn't matter — only that it stays finite.
    vec2 dir = dist > 0.0 ? abs(normalize(delta.yx)) : vec2(0.0, 1.0);

    // Initialize the PRNG by hashing the position + the pixel color
    vec3 m = vec3(pos, 1.0) + vec3(textureLod(HOOKED_raw, pos, 0.0));
    float h = permute(permute(permute(m.x) + m.y) + m.z);

    // Add some random noise to the output
    vec4 noise;
    noise.x = rand(h); h = permute(h);
    noise.y = rand(h); h = permute(h);
    noise.z = rand(h); h = permute(h);
    noise.w = 0.5;
  
    return light_spread(HOOKED_raw, pos, dist, dir, h)
           * soft_distance_falloff(dist * falloff)
           + (grain/8192.0) * (noise - vec4(0.5));
}

