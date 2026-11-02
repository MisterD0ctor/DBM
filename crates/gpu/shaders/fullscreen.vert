// Fullscreen triangle with no vertex buffer: position and UV come from
// gl_VertexID, so a bare VAO is enough.
//
// UV v=0 lands at clip y=-1, which is row 0 of the target framebuffer. mpv is
// asked to flip on render, so row 0 of the video texture is the top of the
// image — meaning every target in the chain inherits the same top-left origin
// as the source, and Slint reads it back with TopLeft too. No flips anywhere.

out vec2 v_uv;

void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
