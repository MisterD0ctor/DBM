//! The angle pointing a scrolling list back to its current row.
//!
//! While the file playing, or the track in use, is scrolled out of sight, an
//! angle at that end of its list points to it, with a circle under the
//! pointer. Both sit in the list's soft edge, which thins away whatever Slint
//! draws there, so they are drawn here, over the finished frame once the edge
//! is done. Slint keeps only the press target, and says where it is through
//! `ScrollFade` (see `WayToCurrent` in scroll_list.slint).
//!
//! The angle is the project's own icon, rendered by resvg rather than drawn
//! a second time in the shader, so there is one drawing of it.

use glow::HasContext;

use super::gfx::{bind_texture, saved_draw_fbo, Program, ScreenQuad};
use super::pipeline::GlassPanel;

const UP: &[u8] = include_bytes!("../../../../public/icons/angle-small-up.svg");
const DOWN: &[u8] = include_bytes!("../../../../public/icons/angle-small-down.svg");

/// The angle's side, in logical pixels: the icon set's smaller size.
const ICON: f32 = 18.0;
/// The circle's diameter, in logical pixels. Inside the row the angle stands
/// level with, with a little of it to spare at either side.
const CIRCLE: f32 = 28.0;

/// Whether the angle stands on a disc of glass the size of its circle, laid
/// over the rows going by (see `Pipeline::glass_over`). Off, it is the angle
/// alone, with its circle only under the pointer.
pub const GLASS: bool = true;

/// One angle, in physical pixels measured from the window's top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Way {
    /// Its centre.
    pub x: f32,
    pub y: f32,
    pub up: bool,
    /// How much of it is on, 0..1.
    pub on: f32,
    /// How far its circle is in, 0..1.
    pub lit: f32,
    /// The panel it is in: left, top, right, bottom. A list sliding out of
    /// its panel takes its angle with it, and only what is inside shows.
    pub clip: [i32; 4],
    /// Logical pixels to physical.
    pub scale: f32,
    /// The paint it is drawn in, as alphas of white: the circle's, and the
    /// angle's at rest and under the pointer.
    pub wash: f32,
    pub rest: f32,
    pub bright: f32,
}

impl Way {
    /// Its disc of glass, and the panel that cuts it; see [`GLASS`].
    pub fn pane(&self) -> (GlassPanel, [i32; 4]) {
        let radius = CIRCLE * self.scale / 2.0;
        let pane = GlassPanel {
            rect: [
                self.x - radius,
                self.y - radius,
                self.x + radius,
                self.y + radius,
            ],
            radius,
            // Tinted, as every other piece of the interface's glass is.
            tint_alpha: 1.0,
            opacity: self.on.clamp(0.0, 1.0),
        };
        (pane, self.clip)
    }
}

pub struct Ways {
    quad: ScreenQuad,
    program: Program,
    /// The two angles at the size they were last drawn at, and that size.
    up: Option<glow::Texture>,
    down: Option<glow::Texture>,
    px: u32,
}

impl Ways {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            program: Program::new(
                gl,
                include_str!("../../shaders/fullscreen.vert"),
                include_str!("../../shaders/way.frag"),
            )
            .map_err(|e| format!("way: {e}"))?,
            up: None,
            down: None,
            px: 0,
        })
    }

    /// Draw every angle that is on, over whatever framebuffer is bound (the
    /// window's, in `AfterRendering`).
    pub fn run(&mut self, gl: &glow::Context, window: (u32, u32), ways: &[Way]) {
        if window.0 == 0 || window.1 == 0 || !ways.iter().any(|w| w.on > 0.004) {
            return;
        }
        let screen = saved_draw_fbo(gl);
        let mut viewport = [0i32; 4];
        unsafe { gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport) };
        for way in ways.iter().filter(|w| w.on > 0.004) {
            self.draw(gl, window, screen, way);
        }
        // Handed back as the soft edge hands it back: Slint starts the next
        // frame from what it left.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
        }
    }

    fn draw(
        &mut self,
        gl: &glow::Context,
        window: (u32, u32),
        screen: Option<glow::Framebuffer>,
        way: &Way,
    ) {
        let win_h = window.1 as i32;
        let px = (ICON * way.scale).round().max(1.0) as u32;
        if !self.ensure(gl, px) {
            return;
        }
        let px = px as i32;
        // A square around the circle, with a pixel to spare for its rim, and
        // as much wider than the angle on each side as the other, so the
        // angle lands on whole pixels in it and the circle centres on it.
        let diameter = CIRCLE * way.scale;
        let spare = (((diameter + 2.0) as i32 - px).max(0) + 1) / 2;
        let side = px + 2 * spare;
        let left = (way.x - px as f32 / 2.0).round() as i32 - spare;
        let top = (way.y - px as f32 / 2.0).round() as i32 - spare;

        let [c_left, c_top, c_right, c_bottom] = way.clip;
        if c_right <= c_left || c_bottom <= c_top {
            return;
        }

        let program = &self.program;
        program.bind(gl);
        program.set_vec2(gl, "u_size", side as f32, side as f32);
        program.set_vec2(gl, "u_centre", side as f32 / 2.0, side as f32 / 2.0);
        program.set_f32(gl, "u_radius", diameter / 2.0);
        program.set_vec2(gl, "u_icon_at", spare as f32, spare as f32);
        program.set_f32(gl, "u_icon_px", px as f32);
        program.set_f32(gl, "u_on", way.on.clamp(0.0, 1.0));
        program.set_f32(gl, "u_lit", way.lit.clamp(0.0, 1.0));
        program.set_f32(gl, "u_wash", way.wash);
        program.set_f32(gl, "u_rest", way.rest);
        program.set_f32(gl, "u_bright", way.bright);
        let icon = if way.up { self.up } else { self.down };
        bind_texture(gl, program, "u_icon", 0, icon);
        self.quad.blend_at(
            gl,
            screen,
            (left, win_h - top - side, side, side),
            (c_left, win_h - c_bottom, c_right - c_left, c_bottom - c_top),
        );
    }

    /// Both angles at `px` pixels square, drawn again only when the size
    /// changes, as when a window moves to a screen of another scale.
    fn ensure(&mut self, gl: &glow::Context, px: u32) -> bool {
        if self.px == px && self.up.is_some() && self.down.is_some() {
            return true;
        }
        self.release_icons(gl);
        let (Some(up), Some(down)) = (raster(UP, px), raster(DOWN, px)) else {
            return false;
        };
        self.up = upload(gl, &up, px);
        self.down = upload(gl, &down, px);
        self.px = px;
        self.up.is_some() && self.down.is_some()
    }

    fn release_icons(&mut self, gl: &glow::Context) {
        unsafe {
            for tex in [self.up.take(), self.down.take()].into_iter().flatten() {
                gl.delete_texture(tex);
            }
        }
        self.px = 0;
    }

    pub fn release(&mut self, gl: &glow::Context) {
        self.release_icons(gl);
        self.program.release(gl);
        self.quad.release(gl);
    }
}

/// An icon, `px` pixels square, white and premultiplied as tiny-skia keeps
/// everything. `None` only if the drawing would not parse.
fn raster(svg: &[u8], px: u32) -> Option<Vec<u8>> {
    use resvg::tiny_skia::{Pixmap, Transform};

    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = Pixmap::new(px, px)?;
    let size = tree.size();
    resvg::render(
        &tree,
        Transform::from_scale(px as f32 / size.width(), px as f32 / size.height()),
        &mut pixmap.as_mut(),
    );
    Some(pixmap.take())
}

fn upload(gl: &glow::Context, rgba: &[u8], px: u32) -> Option<glow::Texture> {
    unsafe {
        let tex = gl.create_texture().ok()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            px as i32,
            px as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(rgba)),
        );
        for (k, v) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
        Some(tex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_angles_draw_and_point_opposite_ways() {
        let up = raster(UP, 18).expect("the up angle parses and renders");
        let down = raster(DOWN, 18).expect("the down angle parses and renders");
        assert_eq!(up.len(), 18 * 18 * 4);
        // How far down the ink sits, on average, in some of the columns: the
        // point is in the middle ones and the arms end in the outer ones, so
        // an angle pointing up is inked higher in the middle than outside.
        let depth = |rgba: &[u8], columns: &[usize]| {
            let (mut sum, mut weight) = (0.0, 0.0);
            for row in 0..18 {
                for &x in columns {
                    let a = f32::from(rgba[(row * 18 + x) * 4 + 3]);
                    sum += a * row as f32;
                    weight += a;
                }
            }
            assert!(weight > 0.0, "inked in columns {columns:?}");
            sum / weight
        };
        let (middle, outside) = ([8, 9], [4, 5, 12, 13]);
        assert!(depth(&up, &middle) < depth(&up, &outside));
        assert!(depth(&down, &middle) > depth(&down, &outside));
    }
}
