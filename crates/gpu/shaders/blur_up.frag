// The blur's working level back at full size.
//
// Bilinear up to twice the size, a cubic B-spline beyond. The B-spline's
// weights are all positive, so it can't ring, and its slope is continuous,
// so the glass's refraction, which magnifies the backdrop, finds no seams
// where bilinear has a kink at every texel. Each axis is two bilinear reads
// placed between pairs of texels by their weights, as the Gaussian's taps are.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_src;
/// The source's texture size, which its image need not fill (see `Target`).
uniform vec2 u_alloc;
/// The working level's image inside its border, and the border.
uniform vec2 u_src_size;
uniform vec2 u_src_pad;
/// The size this draws.
uniform vec2 u_dst_size;

vec3 at(vec2 p) {
    vec2 size = u_src_size + 2.0 * u_src_pad;
    return textureLod(u_src, clamp(p, vec2(0.5), size - 0.5) / u_alloc, 0.0).rgb;
}

// One axis: where the two reads go in the source, and the second's share.
vec3 axis(float frag_at, float size, float pad, float dst) {
    if (size == dst) {
        return vec3(frag_at, frag_at, 0.0);
    }
    float uv = frag_at / dst;
    if (2.0 * size >= dst) {
        float p = uv * size + pad;
        return vec3(p, p, 0.0);
    }
    float p = uv * size - 0.5;
    float base = floor(p);
    float f = p - base;
    float w0 = (1.0 - f) * (1.0 - f) * (1.0 - f) / 6.0;
    float w1 = (3.0 * f * f * f - 6.0 * f * f + 4.0) / 6.0;
    float w2 = (-3.0 * f * f * f + 3.0 * f * f + 3.0 * f + 1.0) / 6.0;
    float w3 = f * f * f / 6.0;
    return vec3(base - 0.5 + w1 / (w0 + w1) + pad, base + 1.5 + w3 / (w2 + w3) + pad, w2 + w3);
}

void main() {
    vec3 fx = axis(gl_FragCoord.x, u_src_size.x, u_src_pad.x, u_dst_size.x);
    vec3 fy = axis(gl_FragCoord.y, u_src_size.y, u_src_pad.y, u_dst_size.y);
    vec3 c = at(vec2(fx.x, fy.x));
    if (fx.z > 0.0) {
        c = mix(c, at(vec2(fx.y, fy.x)), fx.z);
    }
    if (fy.z > 0.0) {
        vec3 row = at(vec2(fx.x, fy.y));
        if (fx.z > 0.0) {
            row = mix(row, at(vec2(fx.y, fy.y)), fx.z);
        }
        c = mix(c, row, fy.z);
    }
    frag = vec4(c, 1.0);
}
