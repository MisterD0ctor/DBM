//! The player's own mark, as the light behind an empty window.
//!
//! The way in is glass, and glass needs something behind it to bend. With a
//! film left unfinished that is a frame of it (see `resume::last_watched`),
//! but a fresh install has none, so the mark lights the window instead.
//!
//! It goes through the backdrop pass like any frame: covered, softened and
//! dimmed. Drawn small and magnified to the window, it has no edges left to
//! read as a drawing, only its colour behind the panel, where the rim can
//! catch it.

use crate::library::preview::Still;

/// The logo, as the repository keeps it. Rendered rather than shipped as a
/// bitmap so that there is one drawing of it, not two to keep in step.
const LOGO: &[u8] = include_bytes!("../../../../public/death-by-mpv.svg");

/// The tile, 16:9 like the window it covers.
///
/// Tiny, because the backdrop pass softens by about a texel of its source, so
/// the tile's size sets the blur. At 64×36 the face still reads, which is a
/// picture rather than light; at 16×9 it barely lights anything. At 32×18 it
/// is colour with a shape to its edge and nothing to recognise.
const WIDTH: u32 = 32;
const HEIGHT: u32 = 18;

/// How much of the tile's height the mark takes: about the way in's height
/// in the window, so the light falls off across the panel's rim, where
/// refraction has something to show, rather than lying flat under it all.
const SIZE: f32 = 0.55;

/// The mark on black, as a backdrop still. `None` only if the drawing won't
/// parse, which this file's test catches.
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
