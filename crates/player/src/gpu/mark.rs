//! The player's own mark, as the light behind an empty window.
//!
//! The way in is glass, and glass needs something behind it to bend. With a
//! film left unfinished that is a frame of it — see `session::last_watched` —
//! but a fresh install has none, and until now its window stayed black and
//! the way in fell back to a flat fill: at the one moment the player is met
//! for the first time, the material it exists for was the one thing missing.
//!
//! So when no film can light the window, the mark does. It goes through the
//! backdrop pass like any frame — covered, softened, dimmed and fallen away
//! toward the corners — which is what makes it light rather than a logo: the
//! pass softens by about one texel of what it is given, so the mark is drawn
//! small, and magnified to the window it has no edges left to read as a
//! drawing. What remains is the colour of the thing, behind the panel where
//! the rim can catch it. The light of the player, standing in for the light
//! of a film until there is one.

use crate::library::preview::Still;

/// The logo, as the repository keeps it. Rendered rather than shipped as a
/// bitmap so that there is one drawing of it, not two to keep in step.
const LOGO: &[u8] = include_bytes!("../../../../public/death-by-mpv.svg");

/// The tile, 16:9 like the window it covers.
///
/// Tiny, deliberately: the backdrop pass softens by about a texel of its
/// source, so the size of the tile is the size of the blur. Photographed at
/// three sizes. At 64×36 the eye sockets and the jaw still read — a face
/// glowing behind the panel, which is a picture and not light, and the
/// backdrop's own rule is to soften past the point where a face reads. At
/// 16×9 the mark averaged away into so few texels that it barely lit
/// anything. 32×18 is the size where it is colour with a shape to its edge
/// and nothing to recognise.
const WIDTH: u32 = 32;
const HEIGHT: u32 = 18;

/// How much of the tile's height the mark takes. About the way in's own
/// height in the window, so the light falls off across the panel's rim —
/// where refraction has something to show — rather than lying flat and even
/// under all of it, which a pane of glass gives back as a pane of colour.
const SIZE: f32 = 0.55;

/// The mark on black, as a backdrop still. `None` only if the drawing would
/// not parse, which a test of this file would catch long before anyone did.
pub fn still() -> Option<Still> {
    use resvg::tiny_skia::{Color, Pixmap, Transform};

    let tree = resvg::usvg::Tree::from_data(LOGO, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT)?;
    pixmap.fill(Color::BLACK);

    let size = tree.size();
    let scale = HEIGHT as f32 * SIZE / size.height();
    let x = (WIDTH as f32 - size.width() * scale) / 2.0;
    let y = (HEIGHT as f32 - size.height() * scale) / 2.0;
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale).post_translate(x, y),
        &mut pixmap.as_mut(),
    );

    // Premultiplied, as tiny-skia keeps everything; over opaque black that
    // is already the colour, and every alpha is 255.
    Some(Still {
        rgba: pixmap.take(),
        width: WIDTH,
        height: HEIGHT,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_mark_draws() {
        let still = super::still().expect("the logo parses and renders");
        assert_eq!(still.rgba.len(), (still.width * still.height * 4) as usize);
        // Something other than black, or the window would be lit by nothing.
        assert!(still.rgba.chunks(4).any(|px| px[0] > 64));
    }
}
