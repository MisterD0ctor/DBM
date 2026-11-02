// The angle pointing a list back to its current row, and the circle that
// comes up under it with the pointer — drawn over the finished frame, after
// the soft edge it sits in. See `gpu::way`.
//
// One quad around the circle. The angle is the icon as resvg drew it, white
// and premultiplied, placed whole pixels into the quad so its texels land on
// the window's; the circle is worked out here, a pixel of antialiasing at
// its rim. Both are white, so the colour is the coverage.

in vec2 v_uv;
out vec4 frag;

/// The angle, the size it is drawn at.
uniform sampler2D u_icon;
/// The quad, in pixels.
uniform vec2 u_size;
/// The circle's centre, in pixels from the quad's top-left, and its radius.
uniform vec2 u_centre;
uniform float u_radius;
/// The angle's top-left in the quad, and its side, in pixels.
uniform vec2 u_icon_at;
uniform float u_icon_px;
/// How much of it is on, 0..1: the row's way back into sight, and the
/// panel's own fade.
uniform float u_on;
/// How far the circle is in, 0..1.
uniform float u_lit;
/// The circle's alpha when it is all the way in, and the angle's at rest
/// and under the pointer: the interface's own paint.
uniform float u_wash;
uniform float u_rest;
uniform float u_bright;

void main() {
    // Down from the top: GL counts the viewport's rows up from its bottom.
    vec2 at = vec2(v_uv.x, 1.0 - v_uv.y) * u_size;

    float rim = clamp(u_radius - length(at - u_centre) + 0.5, 0.0, 1.0);
    float disc = rim * u_wash * u_lit;

    vec2 t = (at - u_icon_at) / u_icon_px;
    float glyph = 0.0;
    if (t.x >= 0.0 && t.y >= 0.0 && t.x <= 1.0 && t.y <= 1.0) {
        glyph = texture(u_icon, t).a * mix(u_rest, u_bright, u_lit);
    }

    // The angle over the circle, premultiplied.
    float a = (glyph + disc * (1.0 - glyph)) * u_on;
    frag = vec4(vec3(a), a);
}
