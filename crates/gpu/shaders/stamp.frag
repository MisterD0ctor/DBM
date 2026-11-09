// A picture laid over the finished frame in one colour: a heading's words, its
// glyph or its wash, over the soft edge of the list under it. See `gpu::stamp`.
//
// The picture is white and premultiplied, so its alpha is its coverage, and
// is drawn as femtovg draws a glyph or a colorized image: the colour times
// that coverage. It may start a fraction of a pixel into the quad, as Slint
// may place a thing mid-pixel; its texels then fall between the window's and
// are sampled linearly, as femtovg's would be. Its outermost pixels are
// clear, so its edges fall off rather than smear.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_picture;
/// The picture's size, in pixels.
uniform vec2 u_size;
/// The quad, in pixels, and where the picture's top-left falls in it.
uniform vec2 u_quad;
uniform vec2 u_at;
/// Premultiplied, with the surface's fade in it.
uniform vec4 u_colour;

void main() {
    // Down from the top: GL counts the viewport's rows up from its bottom.
    vec2 at = vec2(v_uv.x, 1.0 - v_uv.y) * u_quad - u_at;
    frag = u_colour * texture(u_picture, at / u_size).a;
}
