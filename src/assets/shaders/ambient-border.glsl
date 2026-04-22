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
128.0

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
128.0

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

    float weight;
    vec4 c_sum = textureLod(HOOKED_raw, box_pos, 0.0);
    float w_sum = 1.0;
    for(float i = 1.0; i <= radius; i += 2) {
        weight = get_weight(i / radius);
        c_sum += textureLod(HOOKED_raw, box_pos + i * dir * HOOKED_pt, 0.0) * weight;
        w_sum += weight;
    }

    return c_sum / w_sum;
}

//!HOOK BORDER
//!BIND HOOKED
//!DESC ambient edge extension pass2

float light_spread_bound(float d) {
    // Tight upper bound
    float c = 0.1; // Tightness of the bound
    float k = falloff;
    float kd = k * d;
    float a = kd + 1.0;
    float kR = kd * a * a / c;
    
    // Initial guess that's reasonable in both regimes:
    // For small kR: v ≈ 1 + kR.  For large kR: v ≈ (kR)^(1/3) + 1/3.
    // This expression interpolates: always ≥ 1, grows like cbrt for large kR.
    float v = 1.0 + kR / (1.0 + pow(kR, 2.0/3.0));
    
    // Two Newton steps for high precision
    v -= (v*v*v - v*v - kR) / (3.0*v*v - 2.0*v);
    v -= (v*v*v - v*v - kR) / (3.0*v*v - 2.0*v);
    
    float u = (v - 1.0) / k;
    return sqrt(max(u*u - d*d, 0.0));
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
    return d / length(vec2(x, d));
}

// Wide usage friendly PRNG, shamelessly stolen from a GLSL tricks forum post
float mod289(float x)  { return x - floor(x / 289.0) * 289.0; }
float permute(float x) { return mod289((34.0*x + 1.0) * x); }
float rand(float x)    { return fract(x / 41.0); }

vec4 light_spread(sampler2D image, vec2 pos, float edge_dist, vec2 dir, float rand) {
    float pt = length(HOOKED_pt * dir);

    float center = length(pos * dir);

    edge_dist += edge_blur + pt;

    float spread_bound = min(1.0, light_spread_bound(edge_dist * spread));
    float t0 = max(0, center - spread_bound + spread_bound * (rand - 0.5) / max_taps / 64);
    float t1 = min(1, center + spread_bound + spread_bound * (rand - 0.5) / max_taps / 64);
    float dt = max((t1 - t0) / max_taps, pt);

    vec4 c_sum = vec4(0.0);
    float w_sum = 0.0;

    for(float t = t0; t <= t1; t += dt) {
        float weight = distance_falloff(length(vec2((t - center), edge_dist))) 
                       * edge_dist / length(vec2((t - center), edge_dist * spread));
        weight = pow(weight, 2.2);
        c_sum += textureLod(image, pos * dir.yx + t * dir, 0.0) * weight;
        w_sum += weight;
    }
    return c_sum / w_sum;
}

vec4 hook() {
    vec2 pos = HOOKED_pos;
    vec4 r = BORDER_rect;

    float video_aspect = (BORDER_rect.z - BORDER_rect.x) / (BORDER_rect.w - BORDER_rect.y)
                       * (HOOKED_size.x / HOOKED_size.y);

    // Border detection
    vec2 box_pos = clamp(pos, r.xy, r.zw);
    vec2 delta = (box_pos - pos) / (r.xy - r.zw) * vec2(video_aspect, 1.0);
    float dist = length(delta);

    vec2 dir = abs(normalize(delta.yx));

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

