//! The picture behind an empty window: a still of the film, eased in, and
//! blurred further at the end of a file.

use glow::HasContext;

use super::{Backdrop, Pipeline};
use crate::gfx::bind_texture;
use crate::size::Size;

/// The mip level the end of a file reads its backdrop from: a quarter of the
/// tile's size each way, past the point where a face in it reads as one with
/// nothing over the middle of the window to cover it.
const END_SOFTENING: f32 = 2.0;

/// How long the backdrop takes to come up. Longer than a surface's 120ms,
/// because it is the window's light changing, and a whole picture arriving
/// that fast is a flash.
const BACKDROP_ARRIVAL: std::time::Duration = std::time::Duration::from_millis(400);

pub(super) struct Picture {
    pub(super) tex: glow::Texture,
    /// The picture's own size, not the window's: it is drawn to fit.
    size: Size,
    arrived: std::time::Instant,
    /// Whether the last draw was the finished one, so an unchanging picture
    /// is not redrawn every frame for nothing.
    settled: bool,
}

impl Pipeline {
    /// Take delivery of the frame the empty window stands in front of.
    ///
    /// Uploaded once and kept: it is a few hundred kilobytes, and the window
    /// only needs it until a film arrives.
    pub fn set_backdrop(&mut self, gl: &glow::Context, rgba: &[u8], size: Size) {
        let tex = unsafe {
            let Ok(tex) = gl.create_texture() else {
                return;
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size.width as i32,
                size.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
            // With its smaller levels, for the end of a file to read from.
            gl.generate_mipmap(glow::TEXTURE_2D);
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            if let Some(old) = self.backdrop.take() {
                gl.delete_texture(old.tex);
            }
            tex
        };
        self.backdrop = Some(Picture {
            tex,
            size,
            arrived: std::time::Instant::now(),
            settled: false,
        });
    }

    /// How the backdrop stands in for mpv's picture this frame. Each time it
    /// is shown it arrives again.
    pub fn show_backdrop(&mut self, how: Backdrop) {
        let shown = how != Backdrop::Off;
        if shown
            && self.backdrop_shown == Backdrop::Off
            && let Some(b) = self.backdrop.as_mut()
        {
            b.arrived = std::time::Instant::now();
            b.settled = false;
        }
        self.film_wanted |= self.backdrop_shown != Backdrop::Off && !shown;
        self.backdrop_shown = how;
    }

    /// Whether the backdrop still has frames of its arrival to draw, so the
    /// driver keeps asking for them, since with no film nothing else does.
    pub fn backdrop_arriving(&self) -> bool {
        self.backdrop_shown != Backdrop::Off && self.backdrop.as_ref().is_some_and(|b| !b.settled)
    }

    pub(super) fn draw_backdrop(&mut self, gl: &glow::Context, size: Size) {
        let Some(b) = self.backdrop.as_mut() else {
            return;
        };
        let t = if self.animate {
            (b.arrived.elapsed().as_secs_f32() / BACKDROP_ARRIVAL.as_secs_f32()).min(1.0)
        } else {
            1.0
        };
        // Ease out, like every other arrival here.
        let level = 1.0 - (1.0 - t).powi(3);
        b.settled = t >= 1.0;
        let (tex, image_size) = (b.tex, b.size);

        self.prog_backdrop.bind(gl);
        self.prog_backdrop.set(gl, "u_size", size.as_f32());
        self.prog_backdrop
            .set(gl, "u_image_size", image_size.as_f32());
        self.prog_backdrop.set(gl, "u_level", level);
        let over_film = self.backdrop_shown == Backdrop::OverFilm;
        let (over, lod) = if over_film {
            (1.0, END_SOFTENING)
        } else {
            (0.0, 0.0)
        };
        self.prog_backdrop.set(gl, "u_over", over);
        self.prog_backdrop.set(gl, "u_lod", lod);
        bind_texture(gl, &self.prog_backdrop, "u_image", 0, Some(tex));
        if over_film {
            self.quad.blend_to(gl, &self.video);
        } else {
            self.quad.draw_to(gl, &self.video);
        }
    }
}
