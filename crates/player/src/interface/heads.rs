//! What heads a scrolling list, drawn by the player over the finished frame
//! once the list's soft edge is done (see `HeadMark` in models.slint).
//!
//! The soft edge works on the frame Slint has drawn, and under a heading the
//! heading's words and the rows passing under them are one mix of white over
//! glass there, which nothing can pull apart again. So Slint lays a lifted
//! heading out and draws none of it, and this draws it afterwards, as Slint
//! would have: its words by the layout Slint uses (parley, styled as
//! `sharedparley` styles a `Text`) and the rasterizer femtovg uses (swash,
//! hinted, placed as `femtovg::text` places a glyph), from the same font
//! files; its glyph and a way back's wash by resvg, as Slint renders an SVG.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use dbm_gpu::rect::Region;
use dbm_gpu::size::Size;
use dbm_gpu::stamp::{Picture, Stamp};
use parley::fontique::{Blob, GenericFamily};
use parley::style::{
    FontFamily, FontFamilyName, FontStyle, FontWeight, LineHeight, OverflowWrap, TextWrapMode,
    WordBreak,
};
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, StyleProperty};
use slint::{ComponentHandle, Model};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};

use crate::{HeadGlyph, HeadMark, MainWindow, Type};

/// The faces Slint is handed in theme.slint, by weight.
const FACES: [(i32, &[u8]); 3] = [
    (500, include_bytes!("../../fonts/IBMPlexSans-Medium.ttf")),
    (600, include_bytes!("../../fonts/IBMPlexSans-SemiBold.ttf")),
    (700, include_bytes!("../../fonts/IBMPlexSans-Bold.ttf")),
];

const SPEAKING: &[u8] = include_bytes!("../../../../public/icons/speaking.svg");
const SUBTITLES: &[u8] = include_bytes!("../../../../public/icons/subtitles.svg");
const PLAYLIST: &[u8] = include_bytes!("../../../../public/icons/playlist.svg");
const BACK: &[u8] = include_bytes!("../../../../public/icons/angle-small-left.svg");

/// Below this, a mark is as good as gone.
const GONE: f32 = 0.004;

/// Read what heads each list out of the interface, as stamps for the
/// pipeline to lay over the frame, in the order each is drawn: wash, glyph,
/// words.
pub fn collect(ui: &MainWindow, out: &mut Vec<Stamp>) {
    out.clear();
    let dpi = ui.window().scale_factor();
    let family = ui.global::<Type>().get_face();
    let marks = ui.get_head_marks();
    PICTURES.with_borrow_mut(|pictures| {
        pictures.begin();
        for mark in marks.iter() {
            pictures.stamps(&mark, dpi, &family, out);
        }
        pictures.end();
    });
}

thread_local! {
    static PICTURES: RefCell<Pictures> = RefCell::new(Pictures::default());
    static TEXT: RefCell<Option<Text>> = const { RefCell::new(None) };
}

/// What a picture shows, whole enough to make it again. Lengths are in
/// sixty-fourths of a physical pixel, as whole numbers so they can be keys.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Recipe {
    Words {
        text: slint::SharedString,
        family: slint::SharedString,
        size: i32,
        weight: i32,
        spacing: i32,
        width: i32,
        height: i32,
        /// Logical pixels to physical, which parley scales the style by.
        dpi: i32,
    },
    Glyph {
        glyph: Glyph,
        px: u32,
    },
    Wash {
        width: u32,
        height: u32,
        radius: i32,
        dpi: i32,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Glyph {
    Speaking,
    Subtitles,
    Playlist,
    Back,
}

/// Pictures made on the CPU, kept for as long as a frame draws them. Each
/// has an id of its own, which is what the GPU keeps its copy by.
#[derive(Default)]
struct Pictures {
    kept: HashMap<Recipe, Kept>,
    next: u64,
}

struct Kept {
    id: u64,
    picture: Rc<Picture>,
    /// Where its top-left is, in physical pixels from where the mark's box
    /// starts.
    at: (f32, f32),
    used: bool,
}

impl Pictures {
    fn begin(&mut self) {
        for kept in self.kept.values_mut() {
            kept.used = false;
        }
    }

    fn end(&mut self) {
        self.kept.retain(|_, kept| kept.used);
    }

    fn stamps(
        &mut self,
        mark: &HeadMark,
        dpi: f32,
        family: &slint::SharedString,
        out: &mut Vec<Stamp>,
    ) {
        if mark.opacity <= GONE {
            return;
        }
        let clip = Region::new(
            (mark.clip_x * dpi).floor() as i32,
            (mark.clip_y * dpi).floor() as i32,
            ((mark.clip_x + mark.clip_width) * dpi).ceil() as i32,
            ((mark.clip_y + mark.clip_height) * dpi).ceil() as i32,
        );
        if clip.right <= clip.left || clip.bottom <= clip.top {
            return;
        }
        let mut stamp = |pictures: &mut Self, recipe: Recipe, x: f32, y: f32, colour| {
            if let Some((id, picture, at)) = pictures.get(recipe) {
                out.push(Stamp {
                    key: id,
                    picture,
                    left: x * dpi + at.0,
                    top: y * dpi + at.1,
                    colour: premultiplied(colour, mark.opacity),
                    clip,
                });
            }
        };
        let fixed = |v: f32| (v * 64.0).round() as i32;
        if mark.wash_color.alpha() > 0 && mark.wash_width > 0.0 && mark.wash_height > 0.0 {
            let recipe = Recipe::Wash {
                width: (mark.wash_width * dpi).round() as u32,
                height: (mark.wash_height * dpi).round() as u32,
                radius: fixed(mark.wash_radius),
                dpi: fixed(dpi),
            };
            stamp(self, recipe, mark.wash_x, mark.wash_y, mark.wash_color);
        }
        let glyph = match mark.glyph {
            HeadGlyph::None => None,
            HeadGlyph::Speaking => Some(Glyph::Speaking),
            HeadGlyph::Subtitles => Some(Glyph::Subtitles),
            HeadGlyph::Playlist => Some(Glyph::Playlist),
            HeadGlyph::Back => Some(Glyph::Back),
        };
        if let Some(glyph) = glyph {
            let recipe = Recipe::Glyph {
                glyph,
                px: (mark.glyph_size * dpi).round() as u32,
            };
            stamp(self, recipe, mark.glyph_x, mark.glyph_y, mark.glyph_color);
        }
        if !mark.text.is_empty() {
            let recipe = Recipe::Words {
                text: mark.text.clone(),
                family: family.clone(),
                size: fixed(mark.font_size),
                weight: mark.font_weight,
                spacing: fixed(mark.letter_spacing),
                width: fixed(mark.width * dpi),
                height: fixed(mark.height * dpi),
                dpi: fixed(dpi),
            };
            stamp(self, recipe, mark.x, mark.y, mark.color);
        }
    }

    /// The picture a recipe makes, made now if no frame has kept it.
    fn get(&mut self, recipe: Recipe) -> Option<(u64, Rc<Picture>, (f32, f32))> {
        if let Some(kept) = self.kept.get_mut(&recipe) {
            kept.used = true;
            return Some((kept.id, kept.picture.clone(), kept.at));
        }
        let (picture, at) = make(&recipe)?;
        let id = self.next;
        self.next += 1;
        let picture = Rc::new(picture);
        self.kept.insert(
            recipe,
            Kept {
                id,
                picture: picture.clone(),
                at,
                used: true,
            },
        );
        Some((id, picture, at))
    }
}

/// A colour as the pipeline takes it: premultiplied, with the fade in it.
fn premultiplied(colour: slint::Color, fade: f32) -> [f32; 4] {
    let alpha = f32::from(colour.alpha()) / 255.0 * fade.clamp(0.0, 1.0);
    let channel = |c: u8| f32::from(c) / 255.0 * alpha;
    [
        channel(colour.red()),
        channel(colour.green()),
        channel(colour.blue()),
        alpha,
    ]
}

fn make(recipe: &Recipe) -> Option<(Picture, (f32, f32))> {
    let back = |v: i32| v as f32 / 64.0;
    match recipe {
        Recipe::Words {
            text,
            family,
            size,
            weight,
            spacing,
            width,
            height,
            dpi,
        } => TEXT.with_borrow_mut(|cx| {
            cx.get_or_insert_with(Text::new).words(&Words {
                text,
                family,
                size: back(*size),
                weight: *weight,
                spacing: back(*spacing),
                width: back(*width),
                height: back(*height),
                dpi: back(*dpi),
            })
        }),
        Recipe::Glyph { glyph, px } => {
            let svg = match glyph {
                Glyph::Speaking => SPEAKING,
                Glyph::Subtitles => SUBTITLES,
                Glyph::Playlist => PLAYLIST,
                Glyph::Back => BACK,
            };
            Some((svg_picture(svg, *px, *px)?, (-1.0, -1.0)))
        }
        Recipe::Wash {
            width,
            height,
            radius,
            dpi,
        } => {
            let dpi = back(*dpi);
            let svg = super::squircle::svg(
                f64::from(*width as f32 / dpi),
                f64::from(*height as f32 / dpi),
                f64::from(back(*radius)),
                0.0,
            );
            Some((svg_picture(svg.as_bytes(), *width, *height)?, (-1.0, -1.0)))
        }
    }
}

/// An SVG drawn to fill `width` by `height` pixels, as Slint draws one into
/// an `Image` of that size, with a clear pixel round it.
fn svg_picture(svg: &[u8], width: u32, height: u32) -> Option<Picture> {
    use resvg::tiny_skia::{Pixmap, Transform};

    if width == 0 || height == 0 {
        return None;
    }
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = Pixmap::new(width + 2, height + 2)?;
    let size = tree.size();
    resvg::render(
        &tree,
        Transform::from_scale(width as f32 / size.width(), height as f32 / size.height())
            .post_translate(1.0, 1.0),
        &mut pixmap.as_mut(),
    );
    Some(Picture {
        size: Size::new(width + 2, height + 2),
        rgba: pixmap.take(),
    })
}

/// A line of a heading's words, as its `Text` has it.
struct Words<'a> {
    text: &'a str,
    family: &'a str,
    /// Logical pixels, as the `Text` has them.
    size: f32,
    weight: i32,
    spacing: f32,
    /// The box, in physical pixels.
    width: f32,
    height: f32,
    dpi: f32,
}

/// The fonts, and parley's and swash's working state, made once the first
/// time words are drawn: finding the system's fonts, for the characters the
/// interface's own font lacks, takes a moment.
struct Text {
    fonts: FontContext,
    layouts: LayoutContext<()>,
    scale: ScaleContext,
}

impl Text {
    fn new() -> Self {
        let mut fonts = FontContext::new();
        for (_, face) in FACES {
            fonts
                .collection
                .register_fonts(Blob::new(Arc::new(face)), None);
        }
        Self {
            fonts,
            layouts: LayoutContext::new(),
            scale: ScaleContext::new(),
        }
    }

    /// The line laid out as `sharedparley` lays out a single line of a
    /// `Text` that doesn't wrap: its family, then the generic ones Slint
    /// falls back on; the line as tall as the font says, wherever another
    /// font fills in.
    fn layout(&mut self, words: &Words, text: &str) -> parley::Layout<()> {
        let families = [
            FontFamilyName::named(words.family),
            FontFamilyName::Generic(GenericFamily::SansSerif),
            FontFamilyName::Generic(GenericFamily::SystemUi),
        ];
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text, words.dpi, false);
        if let Some(ratio) = line_height_ratio(words.weight) {
            builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
                ratio,
            )));
        }
        builder.push_default(FontFamily::List(Cow::Borrowed(&families)));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            words.weight as f32,
        )));
        builder.push_default(StyleProperty::LetterSpacing(words.spacing));
        builder.push_default(StyleProperty::FontStyle(FontStyle::Normal));
        builder.push_default(StyleProperty::FontSize(words.size));
        builder.push_default(StyleProperty::WordBreak(WordBreak::KeepAll));
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Normal));
        builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        builder.build(text)
    }

    /// The words, white over clear, and where the picture's top-left is from
    /// the box's, in physical pixels.
    fn words(&mut self, words: &Words) -> Option<(Picture, (f32, f32))> {
        let mut layout = self.layout(words, words.text);
        layout.break_all_lines(Some(words.width));
        layout.align(Alignment::Left, AlignmentOptions::default());
        let ellipsis = self.ellipsis(words);
        // Centred down the box, as `vertical-alignment: center` puts it.
        let y_offset = (words.height - layout.height()) / 2.0;

        let mut glyphs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let parley::layout::PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let font = run.run().font().clone();
                let size = run.run().font_size();
                let coords = run.run().normalized_coords().to_vec();
                let end = run.offset() + run.advance();
                // Cut short with an ellipsis where it runs past the box, as
                // `overflow: elide` does.
                let elide = end.floor() > words.width.ceil();
                let room = ellipsis.as_ref().map_or(0.0, |e| e.0.advance);
                for glyph in run.positioned_glyphs() {
                    if elide && glyph.x + glyph.advance + room > words.width {
                        if let Some((mut dots, dots_font, dots_size)) = ellipsis.clone() {
                            dots.x = glyph.x;
                            dots.y = run.baseline();
                            glyphs.push((dots_font, dots_size, Vec::new(), dots));
                        }
                        break;
                    }
                    glyphs.push((font.clone(), size, coords.clone(), glyph));
                }
            }
        }
        self.raster(&glyphs, y_offset)
    }

    /// The ellipsis, from a layout of its own, as Slint takes it.
    fn ellipsis(
        &mut self,
        words: &Words,
    ) -> Option<(parley::layout::Glyph, parley::FontData, f32)> {
        let mut layout = self.layout(words, "…");
        layout.break_all_lines(None);
        let line = layout.lines().next()?;
        let parley::layout::PositionedLayoutItem::GlyphRun(run) = line.items().next()? else {
            return None;
        };
        let glyph = run.positioned_glyphs().next()?;
        Some((glyph, run.run().font().clone(), run.run().font_size()))
    }

    /// Every glyph drawn as femtovg draws it: rasterized by swash, hinted,
    /// at the run's size, shifted by a tenth of a pixel's steps across, and
    /// placed at the whole pixel its pen position falls in across and the
    /// nearest one down.
    fn raster(
        &mut self,
        glyphs: &[(parley::FontData, f32, Vec<i16>, parley::layout::Glyph)],
        y_offset: f32,
    ) -> Option<(Picture, (f32, f32))> {
        let mut drawn = Vec::new();
        for (font, size, coords, glyph) in glyphs {
            let Some(face) = swash::FontRef::from_index(font.data.as_ref(), font.index as usize)
            else {
                continue;
            };
            let mut scaler = self
                .scale
                .builder(face)
                .size(*size)
                .hint(true)
                .normalized_coords(coords)
                .build();
            // femtovg's `quantize`, to the nearest tenth.
            let across = (glyph.x.fract() / 0.1 + 0.5).trunc() * 0.1;
            let Some(image) = Render::new(&[
                Source::ColorOutline(0),
                Source::ColorBitmap(StrikeWith::BestFit),
                Source::Outline,
            ])
            .format(Format::Alpha)
            .offset(Vector::new(across, 0.0))
            .render(&mut scaler, glyph.id as u16) else {
                continue;
            };
            let placement = image.placement;
            if placement.width == 0 || placement.height == 0 {
                continue;
            }
            let x = glyph.x.trunc() as i32 + placement.left;
            let y = (glyph.y + y_offset).round() as i32 - placement.top;
            drawn.push((x, y, placement.width, placement.height, image.data));
        }
        let left = drawn.iter().map(|g| g.0).min()? - 1;
        let top = drawn.iter().map(|g| g.1).min()? - 1;
        let right = drawn.iter().map(|g| g.0 + g.2 as i32).max()? + 1;
        let bottom = drawn.iter().map(|g| g.1 + g.3 as i32).max()? + 1;
        let (width, height) = ((right - left) as usize, (bottom - top) as usize);
        let mut alpha = vec![0f32; width * height];
        for (x, y, w, h, data) in &drawn {
            for row in 0..*h as usize {
                for col in 0..*w as usize {
                    let Some(&cover) = data.get(row * *w as usize + col) else {
                        continue;
                    };
                    let at = (*y - top) as usize * width + row * width + (*x - left) as usize + col;
                    let a = f32::from(cover) / 255.0;
                    // One over another, as femtovg blends glyph after glyph.
                    alpha[at] = a + alpha[at] * (1.0 - a);
                }
            }
        }
        let rgba = alpha
            .iter()
            .flat_map(|a| [(a * 255.0).round() as u8; 4])
            .collect();
        Some((
            Picture {
                size: Size::new(width as u32, height as u32),
                rgba,
            },
            (left as f32, top as f32),
        ))
    }
}

/// How tall a line is for its font size, as `sharedparley` works it out
/// from the face the words are set in: ascent, descent and leading.
fn line_height_ratio(weight: i32) -> Option<f32> {
    use skrifa::MetadataProvider;

    let (_, face) = FACES.iter().min_by_key(|(w, _)| (w - weight).abs())?;
    let font = skrifa::FontRef::new(face).ok()?;
    let metrics = font.metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );
    let upem = f32::from(metrics.units_per_em);
    (upem > 0.0).then(|| (metrics.ascent - metrics.descent + metrics.leading) / upem)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str, width: f32) -> Option<(Picture, (f32, f32))> {
        Text::new().words(&Words {
            text,
            family: "IBM Plex Sans",
            size: 12.0,
            weight: 500,
            spacing: 1.0,
            width,
            height: 34.0,
            dpi: 1.0,
        })
    }

    /// Where a picture has ink, as its leftmost and rightmost columns.
    fn inked(picture: &Picture) -> (u32, u32) {
        let width = picture.size.width as usize;
        let columns: Vec<usize> = picture
            .rgba
            .chunks(4)
            .enumerate()
            .filter(|(_, px)| px[3] > 0)
            .map(|(i, _)| i % width)
            .collect();
        (
            *columns.iter().min().unwrap() as u32,
            *columns.iter().max().unwrap() as u32,
        )
    }

    #[test]
    fn words_sit_centred_down_their_box() {
        let (picture, (_, top)) = words("SÄSONG 1", 300.0).unwrap();
        let bottom = top + picture.size.height as f32;
        // Capitals of 12px in a 34px row: clear of both ends, nearer the
        // middle than either.
        assert!(top > 6.0 && bottom < 28.0, "{top}..{bottom}");
        assert!((top + bottom - 34.0).abs() < 6.0, "{top}..{bottom}");
    }

    #[test]
    fn words_too_long_for_their_box_end_in_an_ellipsis_inside_it() {
        let text = "A SEASON WITH A NAME FAR TOO LONG FOR ITS PANEL";
        let (whole, _) = words(text, 1000.0).unwrap();
        let (cut, (left, _)) = words(text, 120.0).unwrap();
        let (_, right) = inked(&cut);
        assert!(cut.size.width < whole.size.width);
        assert!(left + right as f32 <= 121.0, "{}", left + right as f32);
    }

    #[test]
    fn a_way_back_has_its_glyph_and_its_wash() {
        assert!(
            make(&Recipe::Glyph {
                glyph: Glyph::Back,
                px: 18
            })
            .is_some()
        );
        let (wash, at) = make(&Recipe::Wash {
            width: 300,
            height: 34,
            radius: 16 * 64,
            dpi: 64,
        })
        .unwrap();
        assert_eq!(wash.size, Size::new(302, 36));
        assert_eq!(at, (-1.0, -1.0));
    }
}
