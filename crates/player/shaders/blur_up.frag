// Dual-Kawase upsample. Paired with the downsample chain this approximates a
// wide Gaussian at a fraction of the tap count.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_src;
/// 1.0 / source size, in texels of the active image.
uniform vec2 u_texel;
uniform float u_offset;
/// Fraction of the source texture its active image covers.
uniform vec2 u_src_scale;

vec4 src(vec2 p) {
    return texture(u_src, clamp(p, vec2(0.0), vec2(1.0)) * u_src_scale);
}

void main() {
    vec2 o = u_texel * u_offset;
    vec4 sum = src(v_uv + vec2(-o.x * 2.0, 0.0));
    sum += src(v_uv + vec2(-o.x, o.y)) * 2.0;
    sum += src(v_uv + vec2(0.0, o.y * 2.0));
    sum += src(v_uv + vec2(o.x, o.y)) * 2.0;
    sum += src(v_uv + vec2(o.x * 2.0, 0.0));
    sum += src(v_uv + vec2(o.x, -o.y)) * 2.0;
    sum += src(v_uv + vec2(0.0, -o.y * 2.0));
    sum += src(v_uv + vec2(-o.x, -o.y)) * 2.0;
    frag = sum / 12.0;
}
