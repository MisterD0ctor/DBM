//! Pictures laid over the finished frame once the soft edges are done: what
//! heads a scrolling list, so the list's rows can thin away under it rather
//! than take its words with them.
//!
//! A picture is made by the caller, white and premultiplied, and drawn here
//! in one colour: the player's `interface::heads` makes a heading's words as
//! femtovg would, its glyph and a way back's wash as resvg would. Each is
//! kept on the GPU, by what it shows, for as long as frames go on drawing it,
//! and let go of after the first frame that doesn't.

use std::collections::HashMap;
use std::rc::Rc;

use glow::HasContext;

use super::gfx::{Program, ScreenQuad, bind_texture, program, saved_draw_fbo};
use crate::rect::Region;
use crate::size::Size;

/// A picture, white and premultiplied as tiny-skia keeps everything, its
/// rows from the top. Its outermost pixels are clear (see `stamp.frag`).
#[derive(Debug, PartialEq)]
pub struct Picture {
    pub size: Size,
    pub rgba: Vec<u8>,
}

/// One picture to lay over the frame, in physical pixels from the window's
/// top-left.
#[derive(Debug, Clone)]
pub struct Stamp {
    /// What it shows, for keeping it between frames: two stamps with one
    /// key are one picture.
    pub key: u64,
    pub picture: Rc<Picture>,
    /// Where its top-left corner goes. A fraction of a pixel is drawn as
    /// one, since what it stands in for may sit mid-pixel as it moves.
    pub left: f32,
    pub top: f32,
    /// Premultiplied, with the surface's fade in it.
    pub colour: [f32; 4],
    /// The surface it is cut to.
    pub clip: Region,
}

pub struct Stamps {
    quad: ScreenQuad,
    program: Program,
    kept: HashMap<u64, Kept>,
}

struct Kept {
    texture: glow::Texture,
    /// Drawn this frame, so kept for the next.
    used: bool,
}

impl Stamps {
    pub fn new(gl: &glow::Context) -> Result<Self, crate::gfx::Error> {
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            program: program!(gl, "stamp")?,
            kept: HashMap::new(),
        })
    }

    /// Lay every stamp over whatever framebuffer is bound (the window's, in
    /// `AfterRendering`), in order, and let go of the pictures no stamp
    /// asked for.
    pub fn run(&mut self, gl: &glow::Context, window: Size, stamps: &[Stamp]) {
        for kept in self.kept.values_mut() {
            kept.used = false;
        }
        if !window.is_empty() && !stamps.is_empty() {
            let screen = saved_draw_fbo(gl);
            let mut viewport = [0i32; 4];
            unsafe { gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport) };
            for stamp in stamps.iter().filter(|s| s.colour[3] > 0.002) {
                self.draw(gl, window, screen, stamp);
            }
            // Handed back as the soft edge hands it back: Slint starts the
            // next frame from what it left.
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
                gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.use_program(None);
            }
        }
        self.kept.retain(|_, kept| {
            if !kept.used {
                unsafe { gl.delete_texture(kept.texture) };
            }
            kept.used
        });
    }

    fn draw(
        &mut self,
        gl: &glow::Context,
        window: Size,
        screen: Option<glow::Framebuffer>,
        stamp: &Stamp,
    ) {
        let picture = &stamp.picture;
        let clip = stamp.clip;
        if picture.size.is_empty() || clip.right <= clip.left || clip.bottom <= clip.top {
            return;
        }
        let Some(texture) = self.texture(gl, stamp.key, picture) else {
            return;
        };
        // Whole pixels round the picture, wherever in them it starts.
        let (width, height) = (picture.size.width as f32, picture.size.height as f32);
        let left = stamp.left.floor() as i32;
        let top = stamp.top.floor() as i32;
        let quad_w = (stamp.left + width).ceil() as i32 - left;
        let quad_h = (stamp.top + height).ceil() as i32 - top;
        let win_h = window.height as i32;

        let program = &self.program;
        program.bind(gl);
        program.set(gl, "u_size", [width, height]);
        program.set(gl, "u_quad", [quad_w as f32, quad_h as f32]);
        program.set(
            gl,
            "u_at",
            [stamp.left - left as f32, stamp.top - top as f32],
        );
        program.set(gl, "u_colour", stamp.colour);
        bind_texture(gl, program, "u_picture", 0, Some(texture));
        self.quad.blend_at(
            gl,
            screen,
            (left, win_h - top - quad_h, quad_w, quad_h),
            (
                clip.left,
                win_h - clip.bottom,
                clip.right - clip.left,
                clip.bottom - clip.top,
            ),
        );
    }

    /// The picture for `key`, uploaded the first frame it is asked for.
    fn texture(
        &mut self,
        gl: &glow::Context,
        key: u64,
        picture: &Picture,
    ) -> Option<glow::Texture> {
        if let Some(kept) = self.kept.get_mut(&key) {
            kept.used = true;
            return Some(kept.texture);
        }
        let texture = upload(gl, picture)?;
        self.kept.insert(
            key,
            Kept {
                texture,
                used: true,
            },
        );
        Some(texture)
    }

    pub fn release(self, gl: &glow::Context) {
        for kept in self.kept.into_values() {
            unsafe { gl.delete_texture(kept.texture) };
        }
        self.program.release(gl);
        self.quad.release(gl);
    }
}

fn upload(gl: &glow::Context, picture: &Picture) -> Option<glow::Texture> {
    let (w, h) = (picture.size.width, picture.size.height);
    if picture.rgba.len() != (w as usize) * (h as usize) * 4 {
        return None;
    }
    unsafe {
        let tex = gl.create_texture().ok()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&picture.rgba)),
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
