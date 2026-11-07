//! A thin shadow under the interface's words and marks.
//!
//! White text over a bright frame is where the interface can lose its
//! contrast, and Slint has no shadow for text or images. So it is drawn here,
//! after Slint, from the finished frame: wherever the interface laid white
//! over the pipeline's output, the ground around it is darkened a little,
//! with no offset, as a soft rim rather than a cast shadow. See
//! `ui_shadow.frag` for how the words are told from the frame.
//!
//! Only where there is glass: every surface with words or marks on it has
//! some, so the glass rects already say where to look, and with the chrome
//! asleep there are none and this does nothing.
//!
//! Words are told from the frame by how much white they added to it, and
//! white on white adds nothing. So wherever this looks, the glass pass hands
//! Slint the frame held [`HEADROOM`] under white, and this pass gives back what
//! it held: white words keep a trace of themselves over clear glass on a white
//! scene, which is where they most need their shadow.

use glow::HasContext;

use super::gfx::{Program, ScreenQuad, Target, bind_texture, program, saved_draw_fbo};
use super::pipeline::GlassPanel;
use crate::rect::Region;
use crate::size::Size;

/// The shadow's spread, as a standard deviation in logical pixels.
const SIGMA: f32 = 1.50;
/// How much of the ground it takes away at its darkest.
const DEPTH: f32 = 0.33;
/// The share of white the frame is held under inside the regions (see
/// [`regions`]), so the words laid on it can always be measured. An eighth, so
/// white words on white are still read in thirty-odd steps, while the frame
/// given back is scaled by little enough not to band.
pub const HEADROOM: f32 = 0.125;

pub struct Shadow {
    quad: ScreenQuad,
    program: Program,
    /// The region as Slint drew it.
    copied: Target,
    /// The coverage, blurred across.
    across: Target,
}

impl Shadow {
    pub fn new(gl: &glow::Context) -> Result<Self, crate::gfx::Error> {
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            program: program!(gl, "ui_shadow")?,
            copied: Target::default(),
            across: Target::default(),
        })
    }

    /// Shade the interface on whatever framebuffer is bound (the window's, in
    /// `AfterRendering`). `ground` is the pipeline's output, the frame under
    /// the interface, held under white inside `regions`, which are the ones
    /// [`regions`] gave the glass pass this frame.
    pub fn run(
        &mut self,
        gl: &glow::Context,
        ground: &Target,
        window: Size,
        regions: &[Region],
        scale: f32,
    ) {
        if window.is_empty() || regions.is_empty() {
            return;
        }
        let sigma = SIGMA * scale;
        let screen = saved_draw_fbo(gl);
        let mut viewport = [0i32; 4];
        unsafe { gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport) };
        for region in regions {
            self.shade(gl, ground, window, screen, *region, sigma);
        }
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
        }
    }

    fn shade(
        &mut self,
        gl: &glow::Context,
        ground: &Target,
        window: Size,
        screen: Option<glow::Framebuffer>,
        region: Region,
        sigma: f32,
    ) {
        let (left, right, bottom) = (region.left, region.right, region.bottom);
        let (width, height) = (region.width(), region.height());
        let win_h = window.height as i32;
        let size = Size::new(width as u32, height as u32);
        if !self.copied.ensure_size(gl, size) || !self.across.ensure_size(gl, size) {
            return;
        }
        let (Some(copy_fbo), Some(ground_tex)) = (self.copied.fbo(), ground.texture()) else {
            return;
        };
        let gl_bottom = win_h - bottom;
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(copy_fbo));
            gl.blit_framebuffer(
                left,
                gl_bottom,
                right,
                gl_bottom + height,
                0,
                0,
                width,
                height,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
        }

        let program = &self.program;
        let copied = self.copied.scale();
        let ground_scale = ground.scale();
        program.bind(gl);
        program.set(gl, "u_region", [width as f32, height as f32]);
        program.set(gl, "u_sigma", sigma);
        program.set(gl, "u_depth", DEPTH);
        program.set(gl, "u_headroom", HEADROOM);
        program.set(gl, "u_ground_scale", [ground_scale.0, ground_scale.1]);
        program.set(gl, "u_origin", [left as f32, bottom as f32]);
        program.set(gl, "u_window", window.as_f32());
        bind_texture(gl, program, "u_ground", 1, Some(ground_tex));

        // Across: what casts a shadow, blurred, into scratch.
        program.set(gl, "u_dir", [1.0, 0.0]);
        program.set(gl, "u_src_scale", [copied.0, copied.1]);
        bind_texture(gl, program, "u_src", 0, self.copied.texture());
        self.quad.draw_to(gl, &self.across);

        // Down, and laid under the interface on the window.
        let across = self.across.scale();
        program.set(gl, "u_dir", [0.0, 1.0]);
        program.set(gl, "u_src_scale", [across.0, across.1]);
        program.set(gl, "u_frame_scale", [copied.0, copied.1]);
        bind_texture(gl, program, "u_src", 0, self.across.texture());
        bind_texture(gl, program, "u_frame", 2, self.copied.texture());
        self.quad
            .draw_at(gl, screen, (left, win_h - bottom, width, height));
    }

    pub fn release(mut self, gl: &glow::Context) {
        self.copied.release(gl);
        self.across.release(gl);
        self.program.release(gl);
        self.quad.release(gl);
    }
}

/// Where the shadow is cast this frame, in physical pixels from the window's
/// top-left: each pane of glass on screen and as far round it as the shadow
/// reaches, with overlaps folded together. Into `out`.
///
/// Worked out once a frame, before the glass pass, which holds these under
/// white, and handed to [`Shadow::run`], which gives it back: the two have to
/// agree to the pixel.
pub fn regions(panels: &[GlassPanel], window: Size, scale: f32, out: &mut Vec<Region>) {
    out.clear();
    // The shadow reaches three deviations past what casts it.
    let reach = (SIGMA * scale * 3.0).ceil() as i32 + 1;
    for panel in panels.iter().filter(|p| p.opacity > 0.004) {
        let [x0, y0, x1, y1] = panel.rect.as_array();
        let rect = Region::new(
            (x0.floor() as i32 - reach).max(0),
            (y0.floor() as i32 - reach).max(0),
            (x1.ceil() as i32 + reach).min(window.width as i32),
            (y1.ceil() as i32 + reach).min(window.height as i32),
        );
        if !rect.is_empty() {
            out.push(rect);
        }
    }
    merge(out);
}

/// Fold overlapping regions into the box around them, until none overlap.
///
/// A pixel shaded twice would be darkened twice: the second pass reads the
/// first one's shadow as ground the interface did not cover.
fn merge(regions: &mut Vec<Region>) {
    let mut i = 0;
    while i < regions.len() {
        let mut merged = false;
        for j in (i + 1)..regions.len() {
            let (a, b) = (regions[i], regions[j]);
            if a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom {
                regions[i] = Region::new(
                    a.left.min(b.left),
                    a.top.min(b.top),
                    a.right.max(b.right),
                    a.bottom.max(b.bottom),
                );
                regions.swap_remove(j);
                merged = true;
                break;
            }
        }
        if !merged {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::merge;
    use crate::rect::Region;

    #[test]
    fn overlapping_regions_become_one() {
        let mut regions = vec![
            Region::new(0, 0, 10, 10),
            Region::new(5, 5, 20, 20),
            Region::new(30, 30, 40, 40),
        ];
        merge(&mut regions);
        regions.sort();
        assert_eq!(
            regions,
            vec![Region::new(0, 0, 20, 20), Region::new(30, 30, 40, 40)]
        );
    }

    #[test]
    fn a_merge_that_reaches_a_third_takes_it_in_too() {
        let mut regions = vec![
            Region::new(0, 0, 10, 10),
            Region::new(25, 0, 35, 10),
            Region::new(8, 0, 27, 10),
        ];
        merge(&mut regions);
        assert_eq!(regions, vec![Region::new(0, 0, 35, 10)]);
    }
}
