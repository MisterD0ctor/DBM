// One axis of the blur's Gaussian, at the level the blur works at.
//
// The weights come from `gpu::blur::Kernel`, two texels to a tap: a bilinear
// read placed between them by their weights returns their weighted sum, so
// the loop runs half as long as the kernel is wide.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_src;
/// The source's texture size, which its image need not fill (see `Target`).
uniform vec2 u_alloc;
/// The image's size, border included. This draws one the same size.
uniform vec2 u_size;
/// (1, 0) for the horizontal pass, (0, 1) for the vertical one.
uniform vec2 u_dir;
uniform float u_center;
uniform int u_taps;
/// Taps either side of the centre. Sized to `blur::MAX_TAP_PAIRS`.
uniform float u_weight[12];
uniform float u_offset[12];

vec3 at(vec2 p) {
    return textureLod(u_src, clamp(p, vec2(0.5), u_size - 0.5) / u_alloc, 0.0).rgb;
}

void main() {
    vec2 p = gl_FragCoord.xy;
    vec3 acc = at(p) * u_center;
    for (int i = 0; i < 12; i++) {
        if (i >= u_taps) {
            break;
        }
        vec2 off = u_dir * u_offset[i];
        acc += (at(p - off) + at(p + off)) * u_weight[i];
    }
    frag = vec4(acc, 1.0);
}
