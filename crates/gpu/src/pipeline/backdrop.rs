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
    tex: glow::Texture,
    /// The picture's own size, not the window's: it is drawn to fit.
    size: Size,
    arrived: std::time::Instant,
    /// Whether the last draw was the finished one, so an unchanging picture
    /// is not redrawn every frame for nothing.
    settled: bool,
    /// The picture it arrives over rather than over black: the one shown
    /// when it came, as when the way in moves from one film's row to
    /// another's. Let go of once this one has arrived.
    leaving: Option<Leaving>,
}

/// A backdrop giving way to the next, drawn whole under it.
struct Leaving {
    tex: glow::Texture,
    size: Size,
}

impl Picture {
    /// How far it has come up, 0..1, eased out like every other arrival here.
    fn level(&self, animate: bool) -> f32 {
        let t = if animate {
            (self.arrived.elapsed().as_secs_f32() / BACKDROP_ARRIVAL.as_secs_f32()).min(1.0)
        } else {
            1.0
        };
        1.0 - (1.0 - t).powi(3)
    }

    /// Whichever is more on screen, it or what it is arriving over, for the
    /// next to arrive over in turn; `None` is black. Only one picture can lie
    /// whole under the next, so a mix part-way is given up for the nearer.
    fn into_leaving(self, gl: &glow::Context, animate: bool) -> Option<Leaving> {
        let mine = Leaving {
            tex: self.tex,
            size: self.size,
        };
        let (kept, gone) = if self.level(animate) >= 0.5 {
            (Some(mine), self.leaving)
        } else {
            (self.leaving, Some(mine))
        };
        if let Some(gone) = gone {
            unsafe { gl.delete_texture(gone.tex) };
        }
        kept
    }

    pub(super) fn release(self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.tex);
            if let Some(under) = self.leaving {
                gl.delete_texture(under.tex);
            }
        }
    }
}

impl Pipeline {
    /// Take delivery of the frame the empty window stands in front of.
    ///
    /// Uploaded once and kept: it is a few hundred kilobytes, and the window
    /// only needs it until a film arrives. One that comes while another is
    /// shown arrives over it rather than from black, so the light moves from
    /// one film to the next instead of going out between them.
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
            tex
        };
        let leaving = match self.backdrop.take() {
            Some(old) if self.backdrop_shown != Backdrop::Off => old.into_leaving(gl, self.animate),
            Some(old) => {
                old.release(gl);
                None
            }
            None => None,
        };
        self.backdrop = Some(Picture {
            tex,
            size,
            arrived: std::time::Instant::now(),
            settled: false,
            leaving,
        });
    }

    /// How the backdrop stands in for mpv's picture this frame. Each time it
    /// is shown it arrives again, from black.
    pub fn show_backdrop(&mut self, gl: &glow::Context, how: Backdrop) {
        let shown = how != Backdrop::Off;
        if let Some(b) = self.backdrop.as_mut() {
            if !shown && let Some(under) = b.leaving.take() {
                unsafe { gl.delete_texture(under.tex) };
            }
            if shown && self.backdrop_shown == Backdrop::Off {
                b.arrived = std::time::Instant::now();
                b.settled = false;
            }
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
        let animate = self.animate;
        let Some(b) = self.backdrop.as_mut() else {
            return;
        };
        let level = b.level(animate);
        b.settled = level >= 1.0;
        let (tex, image_size) = (b.tex, b.size);
        let under = b.leaving.as_ref().map(|under| (under.tex, under.size));

        let over_film = self.backdrop_shown == Backdrop::OverFilm;
        match under {
            // Whole, as it was on screen, and the new one laid over it with
            // its arrival as its alpha.
            Some((under, under_size)) => {
                self.draw_picture(gl, size, under, under_size, 1.0, over_film);
                self.draw_picture(gl, size, tex, image_size, level, true);
            }
            None => self.draw_picture(gl, size, tex, image_size, level, over_film),
        }

        if let Some(b) = self.backdrop.as_mut()
            && b.settled
            && let Some(under) = b.leaving.take()
        {
            unsafe { gl.delete_texture(under.tex) };
        }
    }

    /// One picture into the video target: laid over what is there with
    /// `level` as its alpha, or drawn in place of it, faded up from black.
    fn draw_picture(
        &self,
        gl: &glow::Context,
        size: Size,
        tex: glow::Texture,
        image_size: Size,
        level: f32,
        over: bool,
    ) {
        let lod = if self.backdrop_shown == Backdrop::OverFilm {
            END_SOFTENING
        } else {
            0.0
        };
        self.prog_backdrop.bind(gl);
        self.prog_backdrop.set(gl, "u_size", size.as_f32());
        self.prog_backdrop
            .set(gl, "u_image_size", image_size.as_f32());
        self.prog_backdrop.set(gl, "u_level", level);
        self.prog_backdrop
            .set(gl, "u_over", if over { 1.0 } else { 0.0 });
        self.prog_backdrop.set(gl, "u_lod", lod);
        bind_texture(gl, &self.prog_backdrop, "u_image", 0, Some(tex));
        if over {
            self.quad.blend_to(gl, &self.video);
        } else {
            self.quad.draw_to(gl, &self.video);
        }
    }
}
