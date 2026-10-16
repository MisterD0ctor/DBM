//! The soft edge on a scrolling list.
//!
//! Where a list's rows run on past the top or bottom of what the panel
//! shows, they blur and thin away over a short strip. Slint has no such
//! effect — nothing in it blurs what an element has drawn — so it is done
//! here, on the finished frame: after Slint has drawn the interface over the
//! pipeline's output, each strip is copied out of the window's framebuffer,
//! blurred, and written back mixed toward the frame as it was before the
//! interface went on. See `scroll_fade.frag` for the picture; this is the
//! plumbing around it.
//!
//! It costs two small draws a strip, and a strip is a few tens of rows of
//! one panel. With no list scrolled there are no strips and it does nothing.

use glow::HasContext;

use super::gfx::{bind_texture, saved_draw_fbo, Program, ScreenQuad, Target};

/// The side of its list, or its line, that a strip is at: where what is in
/// it thins away to nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge {
    Top,
    Bottom,
    Right,
}

/// One strip, in physical pixels measured from the window's top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strip {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    pub edge: Edge,
    /// How much there is past the strip's inner edge that the blur may read:
    /// the rest of the list, and never past it.
    pub room: i32,
    /// How much of the blur is on, 0..1. The strip's depth is the caller's
    /// to scale by the same number.
    pub strength: f32,
    /// Most blur, as a standard deviation in physical pixels.
    pub sigma: f32,
}

pub struct Fader {
    quad: ScreenQuad,
    program: Program,
    /// The region as Slint drew it.
    copied: Target,
    /// The same, blurred across.
    across: Target,
}

impl Fader {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            program: Program::new(
                gl,
                include_str!("../../shaders/fullscreen.vert"),
                include_str!("../../shaders/scroll_fade.frag"),
            )
            .map_err(|e| format!("scroll_fade: {e}"))?,
            copied: Target::new(),
            across: Target::new(),
        })
    }

    /// Soften every strip, on whatever framebuffer is bound — which in
    /// `AfterRendering` is the window's, with the interface already on it.
    ///
    /// `ground` is the pipeline's output: the frame under the interface.
    pub fn run(
        &mut self,
        gl: &glow::Context,
        ground: &Target,
        window: (u32, u32),
        strips: &[Strip],
    ) {
        if strips.is_empty() || window.0 == 0 || window.1 == 0 {
            return;
        }
        let screen = saved_draw_fbo(gl);
        let mut viewport = [0i32; 4];
        unsafe { gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport) };
        for strip in strips {
            self.soften(gl, ground, window, screen, strip);
        }
        // Slint is done with this frame, but it starts the next from what it
        // left, so it is handed back its framebuffer and viewport.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
        }
    }

    fn soften(
        &mut self,
        gl: &glow::Context,
        ground: &Target,
        window: (u32, u32),
        screen: Option<glow::Framebuffer>,
        strip: &Strip,
    ) {
        let (win_w, win_h) = (window.0 as i32, window.1 as i32);
        let (left, right) = (strip.left.max(0), strip.right.min(win_w));
        let (top, bottom) = (strip.top.max(0), strip.bottom.min(win_h));
        let (width, height) = (right - left, bottom - top);
        if width <= 0 || height <= 0 || strip.strength <= 0.004 {
            return;
        }
        // The blur reads past the strip's inner edge, so that much more is
        // copied with it — three deviations, which is all of a Gaussian an
        // 8-bit frame can show.
        let margin = ((strip.sigma * 3.0).ceil() as i32).clamp(0, strip.room.max(0));
        // The region copied, and how deep the strip is away from its edge.
        let (r_left, r_right, r_top, r_bottom, band) = match strip.edge {
            Edge::Top => (left, right, top, (bottom + margin).min(win_h), height),
            Edge::Bottom => (left, right, (top - margin).max(0), bottom, height),
            Edge::Right => ((left - margin).max(0), right, top, bottom, width),
        };
        let (r_width, r_height) = (r_right - r_left, r_bottom - r_top);
        if !self.copied.ensure_size(gl, r_width as u32, r_height as u32)
            || !self.across.ensure_size(gl, r_width as u32, r_height as u32)
        {
            return;
        }
        let (Some(copy_fbo), Some(ground_tex)) = (self.copied.fbo(), ground.texture()) else {
            return;
        };

        // Rows as GL counts them, up from the bottom of the window.
        let gl_bottom = win_h - r_bottom;
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(copy_fbo));
            gl.blit_framebuffer(
                r_left,
                gl_bottom,
                r_right,
                gl_bottom + r_height,
                0,
                0,
                r_width,
                r_height,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
        }

        // Distance from the strip's edge for a pixel of the region, which is
        // counted from its bottom-left: down from the top, up from the
        // bottom, or leftward from the right.
        let edge = match strip.edge {
            Edge::Top => [r_height as f32, 0.0, -1.0],
            Edge::Bottom => [0.0, 0.0, 1.0],
            Edge::Right => [r_width as f32, -1.0, 0.0],
        };
        let program = &self.program;
        program.bind(gl);
        program.set_vec2(gl, "u_region", r_width as f32, r_height as f32);
        program.set_vec3(gl, "u_edge", edge);
        program.set_f32(gl, "u_band", band as f32);
        program.set_f32(gl, "u_sigma", strip.sigma);
        program.set_f32(gl, "u_strength", strip.strength.clamp(0.0, 1.0));

        // Across, the whole region, into scratch.
        let scale = self.copied.scale();
        program.set_vec2(gl, "u_src_scale", scale.0, scale.1);
        program.set_vec2(gl, "u_dir", 1.0, 0.0);
        program.set_vec2(gl, "u_at0", 0.0, 0.0);
        program.set_vec2(gl, "u_span", r_width as f32, r_height as f32);
        bind_texture(gl, program, "u_src", 0, self.copied.texture());
        self.quad.draw_to(gl, &self.across);

        // Down, the strip alone, back onto the window.
        let scale = self.across.scale();
        let ground_scale = ground.scale();
        program.set_vec2(gl, "u_src_scale", scale.0, scale.1);
        program.set_vec2(gl, "u_dir", 0.0, 1.0);
        program.set_vec2(
            gl,
            "u_at0",
            (left - r_left) as f32,
            (r_bottom - bottom) as f32,
        );
        program.set_vec2(gl, "u_span", width as f32, height as f32);
        program.set_vec2(gl, "u_ground_scale", ground_scale.0, ground_scale.1);
        program.set_vec2(gl, "u_origin", r_left as f32, r_bottom as f32);
        program.set_vec2(gl, "u_window", win_w as f32, win_h as f32);
        bind_texture(gl, program, "u_src", 0, self.across.texture());
        bind_texture(gl, program, "u_ground", 1, Some(ground_tex));
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
