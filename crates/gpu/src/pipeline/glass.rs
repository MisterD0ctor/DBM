//! The glass pass: the panes drawn over the composite, refracting the blur,
//! for the window and for a surface drawn on its own.

use glow::HasContext;

use super::{
    BEVEL, CORNER_SOFTNESS, GlassPanel, IOR, LEGIBLE, MAX_PANELS, Pipeline, RING_RIM, fringe_scale,
    legible_ceiling,
};
use crate::blur;
use crate::gfx::{Target, bind_texture, saved_draw_fbo};
use crate::rect::{Pixels, Rect};
use crate::shadow::HEADROOM;

impl Pipeline {
    /// Composite the glass panels over the finished frame.
    pub(super) fn run_glass(&self, gl: &glow::Context, w: u32, h: u32) {
        let panels: &[GlassPanel] = if self.glass_enabled {
            &self.panels
        } else {
            &[]
        };
        self.draw_glass(
            gl,
            (w, h),
            &self.composite,
            self.blur(),
            panels,
            &self.shaded,
            &self.glassed,
        );
    }

    /// The glass pass: `panels` over `base`, refracting `blur`, into `out`,
    /// all of them `size` and the panels placed in it, and the frame held
    /// under white inside `shaded` (see `shadow::HEADROOM`).
    #[expect(
        clippy::too_many_arguments,
        reason = "each is a separate input of the one pass"
    )]
    fn draw_glass(
        &self,
        gl: &glow::Context,
        size: (u32, u32),
        base: &Target,
        blur: &Target,
        panels: &[GlassPanel],
        shaded: &[[i32; 4]],
        out: &Target,
    ) {
        let g = self.glass;
        self.prog_glass.bind(gl);
        self.prog_glass
            .set(gl, "u_size", [size.0 as f32, size.1 as f32]);

        let (base_scale, blur_scale) = (base.scale(), blur.scale());
        self.prog_glass
            .set(gl, "u_base_scale", [base_scale.0, base_scale.1]);
        self.prog_glass
            .set(gl, "u_blur_scale", [blur_scale.0, blur_scale.1]);

        // Uniform arrays are uploaded whole; unused slots are simply never
        // read, since the shader loop stops at `u_panel_count`.
        let ceiling = if self.legible {
            legible_ceiling(self.text_floor, LEGIBLE)
        } else {
            0.0
        };
        let count = panels.len().min(MAX_PANELS);
        let mut rects = [[0.0f32; 4]; MAX_PANELS];
        let mut styles = [[0.0f32; 4]; MAX_PANELS];
        let mut lights = [[0.0f32; 4]; MAX_PANELS];
        for (i, panel) in panels.iter().take(count).enumerate() {
            rects[i] = panel.rect.as_array();
            styles[i] = [
                panel.radius,
                panel.tint_alpha,
                panel.opacity,
                if panel.nested { 1.0 } else { 0.0 },
            ];
            let rim = if panel.ring {
                g.rim.max(RING_RIM)
            } else {
                g.rim
            };
            lights[i] = [ceiling, rim, panel.milk, 0.0];
        }
        self.prog_glass.set(gl, "u_panel_count", count as i32);
        self.prog_glass.set(gl, "u_panel_rect[0]", &rects);
        self.prog_glass.set(gl, "u_panel_style[0]", &styles);
        self.prog_glass.set(gl, "u_panel_light[0]", &lights);

        let held = shaded.len().min(MAX_PANELS);
        let mut regions = [[0.0f32; 4]; MAX_PANELS];
        for (i, region) in shaded.iter().take(held).enumerate() {
            regions[i] = region.map(|v| v as f32);
        }
        self.prog_glass.set(gl, "u_shaded_count", held as i32);
        self.prog_glass.set(gl, "u_shaded[0]", &regions);
        self.prog_glass.set(gl, "u_headroom", HEADROOM);

        self.prog_glass.set(gl, "u_bevel", BEVEL);
        self.prog_glass
            .set(gl, "u_corner_softness", CORNER_SOFTNESS as f32);
        self.prog_glass.set(gl, "u_refract", g.refract);
        self.prog_glass.set(gl, "u_ior", IOR);
        self.prog_glass.set(gl, "u_aberration", g.aberration);
        self.prog_glass
            .set(gl, "u_fringe_scale", fringe_scale(g.blur_sigma));
        self.prog_glass.set(gl, "u_tint", g.tint);
        self.prog_glass.set(gl, "u_tint_amount", g.tint_amount);
        self.prog_glass.set(gl, "u_absorb", g.absorb);
        self.prog_glass.set(gl, "u_shadow", g.shadow);

        bind_texture(gl, &self.prog_glass, "u_base", 0, base.texture());
        bind_texture(gl, &self.prog_glass, "u_blur", 1, blur.texture());
        self.quad.draw_to(gl, out);
    }

    /// Glass laid over the finished frame, interface and all, for a pane
    /// that sits on what Slint drew rather than under it: the angle pointing
    /// a list back to its current row (see `way`), over the rows going by.
    ///
    /// Each pane's surroundings are copied out of the window's framebuffer,
    /// blurred and glassed as the panes under the interface are, and written
    /// back cut to `clip`, the panel it is in. A region is the pane plus room
    /// for the blur, rim and shadow, so it costs a few small draws. Physical
    /// pixels from the window's top-left, as `GlassPanel` takes them.
    pub fn glass_over(
        &mut self,
        gl: &glow::Context,
        window: (u32, u32),
        panes: &[(GlassPanel, [i32; 4])],
    ) {
        if !self.glass_enabled || panes.is_empty() || window.0 == 0 || window.1 == 0 {
            return;
        }
        let screen = saved_draw_fbo(gl);
        let mut viewport = [0i32; 4];
        unsafe { gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport) };
        for (pane, clip) in panes {
            self.glass_one_over(gl, window, screen, pane, *clip);
        }
        // Slint starts the next frame from what it left.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.use_program(None);
        }
    }

    fn glass_one_over(
        &mut self,
        gl: &glow::Context,
        window: (u32, u32),
        screen: Option<glow::Framebuffer>,
        pane: &GlassPanel,
        clip: [i32; 4],
    ) {
        let (win_w, win_h) = (window.0 as i32, window.1 as i32);
        // The blur's reach, and the shadow, which fades over the pane's radius
        // and a few pixels more.
        let margin =
            (blur::REACH * self.glass.blur_sigma.max(0.0) + pane.radius + 8.0).ceil() as i32;
        let [x0, y0, x1, y1] = pane.rect.as_array();
        let left = (x0.floor() as i32 - margin).max(0);
        let top = (y0.floor() as i32 - margin).max(0);
        let right = (x1.ceil() as i32 + margin).min(win_w);
        let bottom = (y1.ceil() as i32 + margin).min(win_h);
        let (width, height) = (right - left, bottom - top);
        // What is written back: the region, inside the pane's panel.
        let (w_left, w_top) = (left.max(clip[0]), top.max(clip[1]));
        let (w_right, w_bottom) = (right.min(clip[2]), bottom.min(clip[3]));
        if width <= 0 || height <= 0 || w_right <= w_left || w_bottom <= w_top {
            return;
        }
        let size = (width as u32, height as u32);
        for target in [&mut self.over_base, &mut self.over_blur, &mut self.over_out] {
            if !target.ensure_size(gl, size.0, size.1) {
                return;
            }
        }
        let (Some(base_fbo), Some(out_fbo)) = (self.over_base.fbo(), self.over_out.fbo()) else {
            return;
        };

        // Out of the window, turned top-down as the pipeline's own targets
        // are: GL counts the window's rows up from its bottom.
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(base_fbo));
            gl.blit_framebuffer(
                left,
                win_h - bottom,
                right,
                win_h - top,
                0,
                height,
                width,
                0,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
        }

        let sigma = self.glass.blur_sigma;
        self.blur_chain
            .run(gl, &self.quad, &self.over_base, &self.over_blur, sigma);
        let placed = GlassPanel {
            rect: Rect::<Pixels>::pixels(
                x0 - left as f32,
                y0 - top as f32,
                x1 - left as f32,
                y1 - top as f32,
            ),
            ..*pane
        };
        // Not held under white again: what it copied already is, and the
        // shadow pass gives that back.
        self.draw_glass(
            gl,
            size,
            &self.over_base,
            &self.over_blur,
            &[placed],
            &[],
            &self.over_out,
        );

        // And back, turned up again.
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(out_fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, screen);
            gl.blit_framebuffer(
                w_left - left,
                w_top - top,
                w_right - left,
                w_bottom - top,
                w_left,
                win_h - w_top,
                w_right,
                win_h - w_bottom,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
        }
    }
}
