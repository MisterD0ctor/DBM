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

use glow::HasContext;

use super::gfx::{bind_texture, saved_draw_fbo, Program, ScreenQuad, Target};
use super::pipeline::GlassPanel;

/// The shadow's spread, as a standard deviation in logical pixels.
const SIGMA: f32 = 1.50;
/// How much of the ground it takes away at its darkest.
const DEPTH: f32 = 0.33;

pub struct Shadow {
    quad: ScreenQuad,
    program: Program,
    /// The region as Slint drew it.
    copied: Target,
    /// The coverage, blurred across.
    across: Target,
    /// The regions to shade this frame, reused.
    regions: Vec<[i32; 4]>,
}

impl Shadow {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            program: Program::new(
                gl,
                include_str!("../../shaders/fullscreen.vert"),
                include_str!("../../shaders/ui_shadow.frag"),
            )
            .map_err(|e| format!("ui_shadow: {e}"))?,
            copied: Target::new(),
            across: Target::new(),
            regions: Vec::new(),
        })
    }

    /// Shade the interface on whatever framebuffer is bound (the window's, in
    /// `AfterRendering`). `ground` is the pipeline's output, the frame under
    /// the interface; `panels` the glass, in physical pixels.
    pub fn run(
        &mut self,
        gl: &glow::Context,
        ground: &Target,
        window: (u32, u32),
        panels: &[GlassPanel],
        scale: f32,
    ) {
        if window.0 == 0 || window.1 == 0 {
            return;
        }
        let sigma = SIGMA * scale;
        // The shadow reaches three deviations past what casts it.
        let reach = (sigma * 3.0).ceil() as i32 + 1;
        self.regions.clear();
        for panel in panels.iter().filter(|p| p.opacity > 0.004) {
            let [x0, y0, x1, y1] = panel.rect;
            let rect = [
                (x0.floor() as i32 - reach).max(0),
                (y0.floor() as i32 - reach).max(0),
                (x1.ceil() as i32 + reach).min(window.0 as i32),
                (y1.ceil() as i32 + reach).min(window.1 as i32),
            ];
            if rect[2] > rect[0] && rect[3] > rect[1] {
                self.regions.push(rect);
            }
        }
        merge(&mut self.regions);
        if self.regions.is_empty() {
            return;
        }

        let screen = saved_draw_fbo(gl);
        let mut viewport = [0i32; 4];
        unsafe { gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport) };
        let regions = std::mem::take(&mut self.regions);
        for region in &regions {
            self.shade(gl, ground, window, screen, *region, sigma);
        }
        self.regions = regions;
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
        window: (u32, u32),
        screen: Option<glow::Framebuffer>,
        region: [i32; 4],
        sigma: f32,
    ) {
        let [left, top, right, bottom] = region;
        let (width, height) = (right - left, bottom - top);
        let win_h = window.1 as i32;
        if !self.copied.ensure_size(gl, width as u32, height as u32)
            || !self.across.ensure_size(gl, width as u32, height as u32)
        {
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
        program.set_vec2(gl, "u_region", width as f32, height as f32);
        program.set_f32(gl, "u_sigma", sigma);
        program.set_f32(gl, "u_depth", DEPTH);
        program.set_vec2(gl, "u_ground_scale", ground_scale.0, ground_scale.1);
        program.set_vec2(gl, "u_origin", left as f32, bottom as f32);
        program.set_vec2(gl, "u_window", window.0 as f32, window.1 as f32);
        bind_texture(gl, program, "u_ground", 1, Some(ground_tex));

        // Across: what casts a shadow, blurred, into scratch.
        program.set_vec2(gl, "u_dir", 1.0, 0.0);
        program.set_vec2(gl, "u_src_scale", copied.0, copied.1);
        bind_texture(gl, program, "u_src", 0, self.copied.texture());
        self.quad.draw_to(gl, &self.across);

        // Down, and laid under the interface on the window.
        let across = self.across.scale();
        program.set_vec2(gl, "u_dir", 0.0, 1.0);
        program.set_vec2(gl, "u_src_scale", across.0, across.1);
        program.set_vec2(gl, "u_frame_scale", copied.0, copied.1);
        bind_texture(gl, program, "u_src", 0, self.across.texture());
        bind_texture(gl, program, "u_frame", 2, self.copied.texture());
        self.quad
            .draw_at(gl, screen, (left, win_h - bottom, width, height));
    }

    pub fn release(&mut self, gl: &glow::Context) {
        self.copied.release(gl);
        self.across.release(gl);
        self.program.release(gl);
        self.quad.release(gl);
    }
}

/// Fold overlapping regions into the box around them, until none overlap.
///
/// A pixel shaded twice would be darkened twice: the second pass reads the
/// first one's shadow as ground the interface did not cover.
fn merge(regions: &mut Vec<[i32; 4]>) {
    let mut i = 0;
    while i < regions.len() {
        let mut merged = false;
        for j in (i + 1)..regions.len() {
            let (a, b) = (regions[i], regions[j]);
            if a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3] {
                regions[i] = [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ];
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

    #[test]
    fn overlapping_regions_become_one() {
        let mut regions = vec![[0, 0, 10, 10], [5, 5, 20, 20], [30, 30, 40, 40]];
        merge(&mut regions);
        regions.sort();
        assert_eq!(regions, vec![[0, 0, 20, 20], [30, 30, 40, 40]]);
    }

    #[test]
    fn a_merge_that_reaches_a_third_takes_it_in_too() {
        let mut regions = vec![[0, 0, 10, 10], [25, 0, 35, 10], [8, 0, 27, 10]];
        merge(&mut regions);
        assert_eq!(regions, vec![[0, 0, 35, 10]]);
    }
}
