//! The window's round corners, cut from the finished frame.
//!
//! A window with no frame has square corners unless it draws round ones, and
//! the desktop's own windows have round ones. So the last pass over the frame
//! takes away what lies outside each corner's circle, alpha and colour alike,
//! and the compositor shows the desktop there. How round is the platform's to
//! say (see `platform::window::corner_radius`).

use glow::HasContext;

use super::gfx::{saved_draw_fbo, Program, ScreenQuad};

pub struct Corners {
    quad: ScreenQuad,
    program: Program,
    /// Whether the window has an alpha channel to cut with, once asked.
    /// Without one a cut corner comes out black, so it is left square.
    alpha: Option<bool>,
}

impl Corners {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            program: Program::new(
                gl,
                include_str!("../../shaders/fullscreen.vert"),
                include_str!("../../shaders/window_corners.frag"),
            )
            .map_err(|e| format!("window_corners: {e}"))?,
            alpha: None,
        })
    }

    /// Cut the corners of the framebuffer bound (the window's, in
    /// `AfterRendering`) `radius` physical pixels round. Nothing at zero.
    pub fn run(&mut self, gl: &glow::Context, window: (u32, u32), radius: f32) {
        let (width, height) = (window.0 as i32, window.1 as i32);
        let side = (radius.ceil() as i32).min(width / 2).min(height / 2);
        if side <= 0 {
            return;
        }
        let screen = saved_draw_fbo(gl);
        let alpha = *self.alpha.get_or_insert_with(|| {
            let alpha = has_alpha(gl, screen);
            if !alpha {
                eprintln!("dbm: the window has no alpha channel, so its corners stay square");
            }
            alpha
        });
        if !alpha {
            return;
        }
        let mut viewport = [0i32; 4];
        unsafe { gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport) };
        self.program.bind(gl);
        self.program
            .set_vec2(gl, "u_window", width as f32, height as f32);
        self.program.set_f32(gl, "u_radius", radius);
        for (x, y) in [
            (0, 0),
            (width - side, 0),
            (0, height - side),
            (width - side, height - side),
        ] {
            self.quad.mask_at(gl, screen, (x, y, side, side));
        }
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.use_program(None);
        }
    }

    pub fn release(&mut self, gl: &glow::Context) {
        self.program.release(gl);
        self.quad.release(gl);
    }
}

/// Whether `fbo` (`None` for the window's own) keeps an alpha channel.
fn has_alpha(gl: &glow::Context, fbo: Option<glow::Framebuffer>) -> bool {
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
        // Desktop GL's core profile has no `ALPHA_BITS`, and GLES 2 can't ask
        // the window's framebuffer for its attachments.
        let bits = if gl.version().is_embedded {
            gl.get_parameter_i32(glow::ALPHA_BITS)
        } else {
            let attachment = if fbo.is_some() {
                glow::COLOR_ATTACHMENT0
            } else {
                glow::BACK_LEFT
            };
            gl.get_framebuffer_attachment_parameter_i32(
                glow::FRAMEBUFFER,
                attachment,
                glow::FRAMEBUFFER_ATTACHMENT_ALPHA_SIZE,
            )
        };
        bits > 0
    }
}
