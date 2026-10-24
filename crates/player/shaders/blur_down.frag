// One step down the blur's levels: each pixel the average of the area it
// covers in the level above, at most half a size smaller on each axis.
//
// An area average rather than a point sample, so a thin bright line can't
// vanish between pixels and come back as it moves. A footprint covers at most
// three texels a side, which two bilinear reads per axis cover exactly.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_src;
/// The source's texture size, which its image need not fill (see `Target`).
uniform vec2 u_alloc;
/// The source's image inside its border, and the border.
uniform vec2 u_src_size;
uniform vec2 u_src_pad;
/// The same for what this draws.
uniform vec2 u_dst_size;
uniform vec2 u_dst_pad;

vec3 at(vec2 p) {
    vec2 size = u_src_size + 2.0 * u_src_pad;
    return textureLod(u_src, clamp(p, vec2(0.5), size - 0.5) / u_alloc, 0.0).rgb;
}

// The texels `lo..hi` of one axis as two bilinear reads: where the first
// goes, where the second goes, and the second's share.
vec3 footprint(float lo, float hi) {
    float first = floor(lo);
    float w0 = min(hi, first + 1.0) - lo;
    float w1 = max(0.0, min(hi, first + 2.0) - (first + 1.0));
    float w2 = max(0.0, hi - (first + 2.0));
    return vec3(first + 0.5 + w1 / (w0 + w1), first + 2.5, w2 / (hi - lo));
}

void main() {
    vec2 edge = u_src_size + 2.0 * u_src_pad;
    vec2 pixel = floor(gl_FragCoord.xy) - u_dst_pad;
    vec2 scale = u_src_size / u_dst_size;
    vec2 lo = pixel * scale + u_src_pad;
    vec2 hi = min((pixel + 1.0) * scale, u_src_size) + u_src_pad;
    // The border keeps the source's outermost texels, so what the Gaussian
    // extends past the edge is the picture's own edge, not its last block's
    // average.
    lo = mix(lo, vec2(0.0), step(pixel, vec2(-1.0)));
    hi = mix(hi, vec2(1.0), step(pixel, vec2(-1.0)));
    lo = mix(lo, edge - 1.0, step(u_dst_size, pixel));
    hi = mix(hi, edge, step(u_dst_size, pixel));

    vec3 fx = footprint(lo.x, hi.x);
    vec3 fy = footprint(lo.y, hi.y);
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
