// Separable Gaussian, run at full resolution.
//
// The alternative to the Kawase pyramid. The pyramid gets a wide blur almost
// free by doing the work at reduced size; this does none of that, so cost
// scales linearly with radius. It exists for one reason: refraction magnifies
// the backdrop with sub-pixel displacement, and a pyramid-reconstructed
// texture can show faint blockiness under that magnification. A true Gaussian
// cannot.
//
// Run twice - horizontally, then vertically - which is what makes an O(n^2)
// kernel O(n).

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_src;
/// 1.0 / source size, in texels of the active image.
uniform vec2 u_texel;
/// Fraction of the source texture its active image covers.
uniform vec2 u_src_scale;
/// (1, 0) for the horizontal pass, (0, 1) for the vertical one.
uniform vec2 u_dir;
/// Standard deviation, in pixels.
uniform float u_sigma;

/// Hard ceiling on taps per side. Reaching it clamps the effective radius
/// rather than letting a large sigma run the shader off a cliff - the same
/// trade the ambient border shader makes with `max_taps`.
const int MAX_TAPS = 48;

vec3 src(vec2 p) {
    return texture(u_src, clamp(p, vec2(0.0), vec2(1.0)) * u_src_scale).rgb;
}

void main() {
    float sigma = max(u_sigma, 1e-3);
    // 3 sigma covers 99.7% of the kernel; past that the weights are below
    // what an 8-bit target can represent anyway.
    int taps = int(min(ceil(sigma * 3.0), float(MAX_TAPS)));

    vec3 acc = src(v_uv);
    float sum_w = 1.0;

    // Two samples per iteration, mirrored, so the loop runs half as long as
    // the kernel is wide.
    for (int i = 1; i <= MAX_TAPS; i++) {
        if (i > taps) {
            break;
        }
        float x = float(i);
        float w = exp(-0.5 * x * x / (sigma * sigma));
        vec2 off = u_dir * u_texel * x;
        acc += (src(v_uv + off) + src(v_uv - off)) * w;
        sum_w += 2.0 * w;
    }

    frag = vec4(acc / sum_w, 1.0);
}
