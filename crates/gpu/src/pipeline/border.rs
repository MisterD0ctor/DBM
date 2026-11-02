//! The ambient border: the picture's edge extended outward, then spread into
//! the light around it.

use super::Pipeline;
use crate::gfx::bind_texture;

impl Pipeline {
    pub(super) fn run_border(&self, gl: &glow::Context, w: u32, h: u32) {
        let size = (w as f32, h as f32);
        let texel = (1.0 / size.0, 1.0 / size.1);
        let p = self.params;
        // The backdrop is drawn across the whole target, surround and all,
        // and would come up over the border with it.
        let surround = if self.clear_surround && !self.show_backdrop {
            1.0
        } else {
            0.0
        };

        // Pass 1 — edge extension. Skipped entirely with the border off; the
        // spread pass then takes its short path and never reads `extended`.
        if self.border_enabled {
            self.prog_extend.bind(gl);
            self.prog_extend.set(gl, "u_size", [size.0, size.1]);
            self.prog_extend.set(gl, "u_texel", [texel.0, texel.1]);
            self.prog_extend.set(gl, "u_rect", self.rect);
            self.prog_extend.set(gl, "edge_blur", p.edge_blur);
            self.prog_extend.set(gl, "max_taps", p.max_taps);
            let vs = self.video.scale();
            self.prog_extend.set(gl, "u_src_scale", [vs.0, vs.1]);
            bind_texture(gl, &self.prog_extend, "u_src", 0, self.video.texture());
            self.quad.draw_to(gl, &self.extended);
        }

        // Pass 2 — light spread, then the video composited on top.
        self.prog_spread.bind(gl);
        self.prog_spread.set(gl, "u_size", [size.0, size.1]);
        self.prog_spread.set(gl, "u_texel", [texel.0, texel.1]);
        self.prog_spread.set(gl, "u_rect", self.rect);
        self.prog_spread.set(gl, "edge_blur", p.edge_blur);
        self.prog_spread.set(gl, "spread", p.spread);
        self.prog_spread.set(gl, "grain", p.grain);
        self.prog_spread.set(gl, "falloff", p.falloff);
        self.prog_spread
            .set(gl, "falloff_softness", p.falloff_softness);
        self.prog_spread.set(gl, "max_taps", p.max_taps);
        self.prog_spread
            .set(gl, "u_border", if self.border_enabled { 1.0 } else { 0.0 });
        self.prog_spread.set(gl, "u_surround", surround);
        let (es, vs) = (self.extended.scale(), self.video.scale());
        self.prog_spread.set(gl, "u_src_scale", [es.0, es.1]);
        self.prog_spread.set(gl, "u_video_scale", [vs.0, vs.1]);
        bind_texture(gl, &self.prog_spread, "u_src", 0, self.extended.texture());
        bind_texture(gl, &self.prog_spread, "u_video", 1, self.video.texture());
        self.quad.draw_to(gl, &self.composite);
    }
}
