// The last film left unfinished, standing in for a film until there is one.
//
// Drawn into the video target in place of mpv's picture while nothing is
// loaded, so everything downstream treats it as the frame: the glass refracts
// it, the blur frosts it, and the empty window's panel has something to be
// glass over. Over black it had nothing to bend, and with the tint at zero it
// was not there at all.
//
// The source is one seek-preview tile, about 192x108, so it is spread across
// the window rather than shown: covered, softened past the point where its
// pixels read, and dimmed well below the film it came from. It is the light
// of the thing, not the thing.

in vec2 v_uv;
out vec4 frag;

uniform sampler2D u_image;
/// The tile's own size in pixels.
uniform vec2 u_image_size;
/// The target's size in pixels.
uniform vec2 u_size;
/// How far it has arrived, 0..1, already eased.
uniform float u_level;

/// How much of the frame's brightness survives. Low enough that the way in
/// reads as sitting in front of a picture rather than on top of one, and that
/// white text over the glass never has to rely on absorption to be read.
const float DIM = 0.42;

/// The tile at `uv`, reconstructed with a cubic B-spline rather than
/// bilinearly.
///
/// Bilinear magnification is continuous but has a crease at every source
/// texel, and at six or seven times the tile's size those creases are a
/// lattice across the whole window — which survived a Gaussian laid over it,
/// because blurring a creased surface blurs the creases in place. The
/// B-spline is smooth through every texel, so there is nothing periodic left
/// to find. Four bilinear taps, weighted and offset so the hardware filter
/// does the rest.
vec3 smooth_at(vec2 uv) {
    vec2 st = uv * u_image_size - 0.5;
    vec2 i = floor(st);
    vec2 f = st - i;
    vec2 f2 = f * f;
    vec2 f3 = f2 * f;
    vec2 w0 = (-f3 + 3.0 * f2 - 3.0 * f + 1.0) / 6.0;
    vec2 w1 = (3.0 * f3 - 6.0 * f2 + 4.0) / 6.0;
    vec2 w2 = (-3.0 * f3 + 3.0 * f2 + 3.0 * f + 1.0) / 6.0;
    vec2 w3 = f3 / 6.0;
    vec2 g0 = w0 + w1;
    vec2 g1 = w2 + w3;
    vec2 h0 = (i - 1.0 + w1 / g0 + 0.5) / u_image_size;
    vec2 h1 = (i + 1.0 + w3 / g1 + 0.5) / u_image_size;
    return g0.y * (g0.x * texture(u_image, h0).rgb + g1.x * texture(u_image, vec2(h1.x, h0.y)).rgb)
         + g1.y * (g0.x * texture(u_image, vec2(h0.x, h1.y)).rgb + g1.x * texture(u_image, h1).rgb);
}

void main() {
    // Cover rather than fit: a letterboxed thumbnail in an empty window would
    // be a picture of a player, not the room a film lights.
    float scale = max(u_size.x / u_image_size.x, u_size.y / u_image_size.y);
    vec2 shown = u_image_size * scale / u_size;
    vec2 uv = (v_uv - 0.5) / shown + 0.5;

    // Then softened a little further, past the point where a face in the tile
    // reads as a face: this is the light of the film, not a picture of it.
    vec2 texel = 1.0 / u_image_size;
    vec3 sum = vec3(0.0);
    float weight = 0.0;
    for (int i = -1; i <= 1; i++) {
        for (int j = -1; j <= 1; j++) {
            float k = exp(-float(i * i + j * j) / 2.0);
            sum += smooth_at(clamp(uv + vec2(i, j) * texel, 0.0, 1.0)) * k;
            weight += k;
        }
    }
    vec3 colour = sum / weight;

    // Falls away toward the edges, so the middle — where the panel is — holds
    // the light and the corners go back toward the black they were.
    vec2 d = (v_uv - 0.5) * vec2(u_size.x / u_size.y, 1.0);
    float vignette = 1.0 - smoothstep(0.2, 1.1, length(d));

    frag = vec4(colour * DIM * vignette * u_level, 1.0);
}
