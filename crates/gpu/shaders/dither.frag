// The grain the passes add to a gradient, so it does not step.
//
// Eight bits a channel cannot hold the wide, smooth gradients this player
// draws: the ambient border's spread light and the glass's rim both cross a
// few dozen pixels and land in visible contours. A pixel-sized offset of up
// to one bit, taken from a hash of where the pixel is and what is about to be
// written there, turns each contour into grain.
//
// The colour is part of the hash so that the two sides of a band edge, one
// step apart, get unrelated offsets: the edge then dissolves into the same
// grain as the band it separates, rather than staying a line.
//
// The hash is mpv's, from ambient-border.glsl, and the border's spread loop
// jitters its taps with the same seed — see border_spread.frag.

float mod289(float x)  { return x - floor(x / 289.0) * 289.0; }
float permute(float x) { return mod289((34.0 * x + 1.0) * x); }
float rand(float x)    { return fract(x / 41.0); }

/// The hash a pass takes its grain from: `pixel` in physical pixels, and the
/// `colour` about to be written there.
float grain_seed(vec2 pixel, vec3 colour) {
    vec3 m = vec3(pixel, 1.0) + colour;
    return permute(permute(permute(m.x) + m.y) + m.z);
}

/// Peak to peak, in colour: two steps of an 8-bit channel, so every pixel is
/// moved by up to one of them either way. Enough to lose a contour, too
/// little to be seen as texture.
const float DITHER_STEP = 2.0 / 255.0;

/// `colour` with `amount` of grain added, up to half of it either way, from
/// `seed`. Callers that have a knob of their own pass what it says; the rest
/// pass [`DITHER_STEP`].
vec3 dither(float seed, float amount, vec3 colour) {
    vec3 noise = vec3(
        rand(seed),
        rand(permute(seed)),
        rand(permute(permute(seed)))
    );
    return colour + amount * (noise - vec3(0.5));
}
