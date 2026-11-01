// An mpv user shader, not one of ours: it runs inside mpv's own renderer.
//
// Hands the picture on at half alpha, colours untouched. mpv blends
// subtitles over the picture after this stage, and a subtitle's own alpha
// raises the picture's wherever it lies, so past mpv the picture's alpha
// says which of its pixels are a subtitle's. The ambient border reads its
// light from the picture's edge, and leaves those out (border_extend.frag).
// Nothing else reads the picture's alpha.
//
// Four components, since mpv carries a picture without alpha of its own as
// three and would drop this one.

//!HOOK OUTPUT
//!BIND HOOKED
//!COMPONENTS 4
//!DESC DBM: mark the picture under subtitles

vec4 hook() {
    return vec4(HOOKED_tex(HOOKED_pos).rgb, 0.5);
}
