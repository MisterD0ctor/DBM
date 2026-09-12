// Dual-Kawase downsample. Halving resolution while blurring is what makes
// this cheap: the glass backdrop carries almost no information (a heavy
// low-pass filter over a panel a few hundred pixels wide leaves only a few
// dozen meaningful samples), so the whole chain runs at reduced size.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_src;
/// 1.0 / source size, in texels of the active image.
uniform vec2 u_texel;
uniform float u_offset;
/// Fraction of the source texture its active image covers. Targets are
/// allocated grow-only, so the texture is usually larger than the image.
uniform vec2 u_src_scale;

/// Sample in active-image coordinates, clamped to the image rather than to
/// the texture edge - the slack beyond the image holds nothing meaningful.
vec4 src(vec2 p) {
    return texture(u_src, clamp(p, vec2(0.0), vec2(1.0)) * u_src_scale);
}

void main() {
    vec2 o = u_texel * u_offset;
    vec4 sum = src(v_uv) * 4.0;
    sum += src(v_uv - o);
    sum += src(v_uv + o);
    sum += src(v_uv + vec2(o.x, -o.y));
    sum += src(v_uv - vec2(o.x, -o.y));
    frag = sum / 8.0;
}
