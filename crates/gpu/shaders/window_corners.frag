// What lies outside the window's round corners, taken away.
//
// Drawn over each corner's square and multiplied into what is there, colour
// and alpha alike, so the frame stays premultiplied and a pixel the circle's
// edge crosses keeps the share of it that is inside.

out vec4 frag;

/// The window, and the corners' radius, in physical pixels.
uniform vec2 u_window;
uniform float u_radius;

void main() {
    vec2 p = gl_FragCoord.xy;
    vec2 centre = clamp(p, vec2(u_radius), u_window - u_radius);
    frag = vec4(clamp(u_radius - distance(p, centre) + 0.5, 0.0, 1.0));
}
