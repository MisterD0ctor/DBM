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

#define EDGE_BLUR 0.002
#define BORDER_BLUR 0.1

#define SIGMA 0.33 //blur spread or amount, (0.0, 10+]

#define get_weight(x) (exp(-(x) * (x) / (2.0 * SIGMA * SIGMA)))

vec4 hook() {
    vec2 pos = HOOKED_pos;
    vec4 r = BORDER_rect;

    // Border detection
    vec2 box_pos = clamp(pos, r.xy + HOOKED_pt, r.zw - HOOKED_pt);
    vec2 delta = box_pos - pos;
    float dist = length(delta);

    vec2 dir = sign(delta);

    // Blur
    float sigma = EDGE_BLUR + dist * BORDER_BLUR;
    float radius = length(sigma * 3 * HOOKED_size);

    float weight;
    vec4 csum = textureLod(HOOKED_raw, box_pos, 0.0);
    float wsum = 1.0;
    for(float i = 1.0; i <= radius; ++i) {
        weight = get_weight(i / radius);
        csum += textureLod(HOOKED_raw, box_pos + i * dir * HOOKED_pt, 0.0) * weight;
        wsum += weight;
    }

    return csum / wsum;
}

//!HOOK BORDER
//!BIND HOOKED
//!DESC ambient edge extension pass2

#define EDGE_BLUR 0.002
#define BORDER_BLUR 0.4

#define SIGMA 0.33 //blur spread or amount, (0.0, 10+]
#define get_weight(x) (exp(-(x) * (x) / (2.0 * SIGMA * SIGMA)))

#define FALLOFF 75
#define FALLOFF_SOFTNESS 2
#define soft_light_falloff(x) ((abs(x) / FALLOFF_SOFTNESS + 1) / ((x * x + abs(x)) / FALLOFF_SOFTNESS + 1))
#define light_falloff(x) (1 / (x * x + 2 * abs(x) + 1))

#define GRAIN 128

// Wide usage friendly PRNG, shamelessly stolen from a GLSL tricks forum post
float mod289(float x)  { return x - floor(x / 289.0) * 289.0; }
float permute(float x) { return mod289((34.0*x + 1.0) * x); }
float rand(float x)    { return fract(x / 41.0); }

vec4 blur(sampler2D image, vec2 pos, float edge_dist, vec2 dir, float sigma) {
    float radius = length(sigma * 3 * HOOKED_size);

    float weight;
    vec4 csum = textureLod(image, pos, 0.0);
    float wsum = 1.0;
    for(float i = 1.0; i <= radius; ++i) {
        weight = get_weight(i / radius);
        csum += (textureLod(image, pos - i * dir * HOOKED_pt, 0.0) 
               + textureLod(image, pos + i * dir * HOOKED_pt, 0.0)) * weight;
        wsum += 2.0 * weight;
    }
    return csum / wsum;
}

vec4 blur2(sampler2D image, vec2 pos, float edge_dist, vec2 dir) {
    float pt = length(HOOKED_pt * dir);
    if (pt == 0.0) return vec4(0.0);
    float center = length(pos * dir);

    float weight;
    vec4 csum = vec4(0.0);
    float wsum = 0.0;
    for(float i = 0; i <= 1; i += pt) {        
        weight = FALLOFF * light_falloff(FALLOFF * length(vec2(i - center, edge_dist))) / 2;

        csum += textureLod(image, pos * dir.yx + i * dir, 0.0) * weight;
        wsum += 1.0;
    }
    return csum / wsum;
}

vec4 hook() {
    vec2 pos = HOOKED_pos;
    vec4 r = BORDER_rect;

    // Border detection
    vec2 box_pos = clamp(pos, r.xy, r.zw);
    vec2 delta = box_pos - pos;
    float dist = length(delta);

    vec2 dir = abs(sign(delta.yx));
    float sigma = EDGE_BLUR + dist * BORDER_BLUR;

    // Initialize the PRNG by hashing the position + a random uniform
    vec3 m = vec3(pos, 1.0) + vec3(textureLod(HOOKED_raw, pos, 0.0));
    float h = permute(permute(permute(m.x) + m.y) + m.z);

    // Add some random noise to the output
    vec4 noise;
    noise.x = rand(h); h = permute(h);
    noise.y = rand(h); h = permute(h);
    noise.z = rand(h); h = permute(h);
    noise.w = 0.5;
 
    return blur(HOOKED_raw, pos, dist, dir, sigma) 
           * soft_light_falloff(dist * FALLOFF)
           + (GRAIN/8192.0) * (noise - vec4(0.5));
}

