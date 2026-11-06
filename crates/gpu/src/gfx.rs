//! Minimal GL plumbing for the post-processing passes.
//!
//! Everything here runs on Slint's own GL context. Deliberately small: a
//! render target, a compiled program, and a fullscreen triangle to drive
//! fragment shaders with. The passes themselves live in `pipeline`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroU32;

use glow::HasContext;

/// Allocation granularity. Rounding up means a drag that sweeps a few hundred
/// pixels costs a handful of reallocations instead of one per frame.
const ALLOC_QUANTUM: u32 = 64;

/// Version prologue for our shader sources.
///
/// Slint hands us GLES on Windows (straight NVIDIA GLES 3.2) and may hand us
/// desktop GL elsewhere, so the `#version` line is chosen at runtime. The
/// shader bodies stay in the syntax common to GLSL ES 3.00 and GLSL 3.30 -
/// `in`/`out`, `texture()`, `textureLod()` - so only the header differs.
pub fn shader_prologue(gl: &glow::Context) -> &'static str {
    if gl.version().is_embedded {
        "#version 300 es\nprecision highp float;\nprecision highp sampler2D;\n#line 1\n"
    } else {
        "#version 330 core\n#line 1\n"
    }
}

// ---------------------------------------------------------------------------
// Render target
// ---------------------------------------------------------------------------

/// An RGBA8 texture with a framebuffer bound to it.
///
/// The texture is allocated **grow-only** and the active image occupies the
/// sub-rectangle `(0, 0)..used` of it. This is the trick mpv's own renderer
/// uses (`fbotex_change` with the fuzzy flags): resizing a window smaller, or
/// back and forth during a drag, then costs no reallocation at all. Tearing
/// down and recreating a pile of textures every drag frame forces a driver
/// sync each time and is what makes a resize crawl.
///
/// Consumers need `scale()` to convert a position in the active image into a
/// texture coordinate, since the texture is generally larger than the image.
///
/// Starts with nothing allocated; the first `ensure_size` makes the texture.
#[derive(Default)]
pub struct Target {
    fbo: Option<glow::Framebuffer>,
    tex: Option<glow::Texture>,
    /// Size of the underlying texture.
    alloc: (u32, u32),
    /// Size of the active image within it, anchored at the origin.
    used: (u32, u32),
}

impl Target {
    /// Size of the active image — what every caller means by "the size".
    pub fn size(&self) -> (u32, u32) {
        self.used
    }

    pub fn alloc(&self) -> (u32, u32) {
        self.alloc
    }

    /// Fraction of the texture the active image covers. Multiply a
    /// `0..1`-over-the-image coordinate by this to get a texture coordinate.
    pub fn scale(&self) -> (f32, f32) {
        if self.alloc.0 == 0 || self.alloc.1 == 0 {
            return (1.0, 1.0);
        }
        (
            self.used.0 as f32 / self.alloc.0 as f32,
            self.used.1 as f32 / self.alloc.1 as f32,
        )
    }

    pub fn texture(&self) -> Option<glow::Texture> {
        self.tex
    }

    /// Texture id in the form Slint wants for a borrowed-texture `Image`.
    pub fn texture_id(&self) -> Option<NonZeroU32> {
        self.tex.map(|t| t.0)
    }

    pub fn fbo(&self) -> Option<glow::Framebuffer> {
        self.fbo
    }

    /// Make room for a `w` x `h` image. Reallocates only when the current
    /// texture is too small in some dimension. A zero dimension (minimised
    /// window) is treated as "no target" rather than as a GL error.
    ///
    /// A failed grow leaves the previous target untouched, so its texture
    /// still stands for anything already pointing at it. Allocating before
    /// releasing also means a driver that refuses the new size cannot leave
    /// a deleted texture name in the hands of the compositor.
    ///
    /// Returns whether the requested size is usable afterwards. On `false`
    /// an older target may still be, at its older size.
    pub fn ensure_size(&mut self, gl: &glow::Context, w: u32, h: u32) -> bool {
        if w == 0 || h == 0 {
            return false;
        }
        if self.tex.is_some() && self.fbo.is_some() && self.alloc.0 >= w && self.alloc.1 >= h {
            self.used = (w, h);
            return true;
        }

        // Never shrink: a drag oscillates, and giving the memory back just to
        // ask for it again next frame is the whole problem.
        let (Some(aw), Some(ah)) = (round_up(w.max(self.alloc.0)), round_up(h.max(self.alloc.1)))
        else {
            return false;
        };
        if aw > i32::MAX as u32 || ah > i32::MAX as u32 {
            return false;
        }

        // Everything this call touches is put back, because it runs in the
        // middle of a frame Slint is already drawing.
        clear_errors(gl);
        let saved_draw = framebuffer_binding(gl, glow::DRAW_FRAMEBUFFER_BINDING);
        let saved_read = framebuffer_binding(gl, glow::READ_FRAMEBUFFER_BINDING);
        let saved_texture = texture_binding(gl);
        let mut viewport = [0i32; 4];
        let mut clear_color = [0f32; 4];
        let scissor = unsafe {
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut clear_color);
            gl.is_enabled(glow::SCISSOR_TEST)
        };

        let old = (self.fbo, self.tex);
        let tex = unsafe {
            let Ok(tex) = gl.create_texture() else {
                return false;
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                aw as i32,
                ah as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            tex
        };
        if let Some(error) = take_error(gl) {
            restore_state(
                gl,
                saved_draw,
                saved_read,
                viewport,
                scissor,
                clear_color,
                saved_texture,
            );
            unsafe { gl.delete_texture(tex) };
            log::warn!("could not allocate a {aw}x{ah} render target ({error:#x})");
            return false;
        }

        unsafe {
            // Linear because every consumer samples between texels. Clamping
            // is only a backstop here: the shaders clamp to the *active*
            // region themselves, since the texture edge is generally further
            // out and holds nothing meaningful.
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
        }

        let fbo = unsafe {
            let Ok(fbo) = gl.create_framebuffer() else {
                restore_state(
                    gl,
                    saved_draw,
                    saved_read,
                    viewport,
                    scissor,
                    clear_color,
                    saved_texture,
                );
                gl.delete_texture(tex);
                return false;
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            fbo
        };

        let mut complete =
            unsafe { gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE };
        let mut error = take_error(gl);
        // Clear once so the slack around the active image starts black rather
        // than as whatever the driver handed us.
        if complete && error.is_none() {
            unsafe {
                gl.viewport(0, 0, aw as i32, ah as i32);
                gl.disable(glow::SCISSOR_TEST);
                gl.clear_color(0.0, 0.0, 0.0, 1.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            error = take_error(gl);
            complete = error.is_none();
        }

        restore_state(
            gl,
            saved_draw,
            saved_read,
            viewport,
            scissor,
            clear_color,
            saved_texture,
        );

        if !complete {
            unsafe {
                gl.delete_framebuffer(fbo);
                gl.delete_texture(tex);
            }
            match error {
                Some(error) => {
                    log::warn!("could not allocate a {aw}x{ah} render target ({error:#x})");
                }
                None => {
                    log::warn!("the driver would not complete a {aw}x{ah} render target");
                }
            }
            return false;
        }

        self.fbo = Some(fbo);
        self.tex = Some(tex);
        self.alloc = (aw, ah);
        self.used = (w, h);

        // The state is handed back before the old target goes, so neither a
        // binding nor a texture the caller had can be left dangling.
        unsafe {
            if let Some(fbo) = old.0 {
                gl.delete_framebuffer(fbo);
            }
            if let Some(tex) = old.1 {
                gl.delete_texture(tex);
            }
        }
        true
    }

    /// Read back single texels as RGBA, in the active region's coordinates.
    /// A full pipeline stall - diagnostics only, never on the frame path.
    pub fn sample_grid(&self, gl: &glow::Context, points: &[(u32, u32)]) -> Vec<[u8; 4]> {
        let Some(fbo) = self.fbo else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(points.len());
        unsafe {
            let saved = saved_draw_fbo(gl);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            for &(x, y) in points {
                let mut px = [0u8; 4];
                gl.read_pixels(
                    x as i32,
                    y as i32,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut px)),
                );
                out.push(px);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, saved);
        }
        out
    }

    pub fn release(&mut self, gl: &glow::Context) {
        unsafe {
            if let Some(fbo) = self.fbo.take() {
                gl.delete_framebuffer(fbo);
            }
            if let Some(tex) = self.tex.take() {
                gl.delete_texture(tex);
            }
        }
        self.alloc = (0, 0);
        self.used = (0, 0);
    }
}

fn round_up(x: u32) -> Option<u32> {
    x.checked_next_multiple_of(ALLOC_QUANTUM)
}

fn clear_errors(gl: &glow::Context) {
    // A failed call from earlier in the frame must not fail this one. The
    // cap is only so a lost context cannot spin here for ever.
    for _ in 0..64 {
        if unsafe { gl.get_error() } == glow::NO_ERROR {
            return;
        }
    }
}

fn take_error(gl: &glow::Context) -> Option<u32> {
    let error = unsafe { gl.get_error() };
    (error != glow::NO_ERROR).then_some(error)
}

fn framebuffer_binding(gl: &glow::Context, target: u32) -> Option<glow::Framebuffer> {
    let id = unsafe { gl.get_parameter_i32(target) };
    NonZeroU32::new(id as u32).map(glow::NativeFramebuffer)
}

fn texture_binding(gl: &glow::Context) -> Option<glow::Texture> {
    let id = unsafe { gl.get_parameter_i32(glow::TEXTURE_BINDING_2D) };
    NonZeroU32::new(id as u32).map(glow::NativeTexture)
}

fn restore_state(
    gl: &glow::Context,
    draw: Option<glow::Framebuffer>,
    read: Option<glow::Framebuffer>,
    viewport: [i32; 4],
    scissor: bool,
    clear_color: [f32; 4],
    texture: Option<glow::Texture>,
) {
    unsafe {
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        if scissor {
            gl.enable(glow::SCISSOR_TEST);
        } else {
            gl.disable(glow::SCISSOR_TEST);
        }
        gl.clear_color(
            clear_color[0],
            clear_color[1],
            clear_color[2],
            clear_color[3],
        );
        gl.bind_texture(glow::TEXTURE_2D, texture);
    }
}

pub fn saved_draw_fbo(gl: &glow::Context) -> Option<glow::Framebuffer> {
    framebuffer_binding(gl, glow::DRAW_FRAMEBUFFER_BINDING)
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

/// The fullscreen triangle with the fragment shader `shaders/<name>.frag`,
/// which every pass is, its build errors named by the shader. A macro because
/// `include_str!` takes only a literal path.
macro_rules! program {
    ($gl:expr, $name:literal) => {
        $crate::gfx::Program::new(
            $gl,
            include_str!("../shaders/fullscreen.vert"),
            include_str!(concat!("../shaders/", $name, ".frag")),
        )
        .map_err(|e| $crate::gfx::Error::Named {
            name: $name,
            source: Box::new(e),
        })
    };
}
pub(crate) use program;

/// Why a shader program could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The GL context itself refused a call.
    Gl(String),
    /// A shader would not compile, and what its compiler said.
    Shader { stage: &'static str, log: String },
    /// The shaders compiled but would not link.
    Link(String),
    /// A named program's failure, from the `program!` macro.
    Named {
        name: &'static str,
        source: Box<Self>,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gl(e) => f.write_str(e),
            Self::Shader { stage, log } => write!(f, "{stage} shader: {log}"),
            Self::Link(log) => write!(f, "link: {log}"),
            Self::Named { name, source } => write!(f, "{name}: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Named { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub struct Program {
    program: glow::Program,
    /// Each uniform's location, looked up on first use: it is fixed once the
    /// program links, and the passes set the same names every frame.
    locations: RefCell<HashMap<String, Option<glow::UniformLocation>>>,
}

/// One shader compiled, or what its compiler said, leaving nothing behind.
fn compile(
    gl: &glow::Context,
    kind: u32,
    stage: &'static str,
    source: &str,
) -> Result<<glow::Context as HasContext>::Shader, Error> {
    unsafe {
        let shader = gl.create_shader(kind).map_err(Error::Gl)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if gl.get_shader_compile_status(shader) {
            return Ok(shader);
        }
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        Err(Error::Shader { stage, log })
    }
}

impl Program {
    pub fn new(gl: &glow::Context, vert: &str, frag: &str) -> Result<Self, Error> {
        unsafe {
            let program = gl.create_program().map_err(Error::Gl)?;
            let prologue = shader_prologue(gl);
            let mut shaders = Vec::new();
            let mut built = Ok(());
            for (stage, kind, src) in [
                ("vertex", glow::VERTEX_SHADER, vert),
                ("fragment", glow::FRAGMENT_SHADER, frag),
            ] {
                match compile(gl, kind, stage, &format!("{prologue}{src}")) {
                    Ok(shader) => {
                        gl.attach_shader(program, shader);
                        shaders.push(shader);
                    }
                    Err(e) => {
                        built = Err(e);
                        break;
                    }
                }
            }
            if built.is_ok() {
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    built = Err(Error::Link(gl.get_program_info_log(program)));
                }
            }
            // Whichever way it went, so a failure part-way leaves nothing.
            for s in shaders {
                gl.detach_shader(program, s);
                gl.delete_shader(s);
            }
            if let Err(e) = built {
                gl.delete_program(program);
                return Err(e);
            }
            Ok(Self {
                program,
                locations: RefCell::default(),
            })
        }
    }

    pub fn bind(&self, gl: &glow::Context) {
        unsafe { gl.use_program(Some(self.program)) };
    }

    fn location(&self, gl: &glow::Context, name: &str) -> Option<glow::UniformLocation> {
        if let Some(known) = self.locations.borrow().get(name) {
            return *known;
        }
        let found = unsafe { gl.get_uniform_location(self.program, name) };
        self.locations.borrow_mut().insert(name.to_owned(), found);
        found
    }

    /// Set a uniform, or nothing where the shader has none of that name,
    /// which is the case for one its compiler found unused.
    pub fn set(&self, gl: &glow::Context, name: &str, value: impl Uniform) {
        if let Some(at) = self.location(gl, name) {
            value.upload(gl, &at);
        }
    }

    pub fn release(self, gl: &glow::Context) {
        unsafe { gl.delete_program(self.program) };
    }
}

/// A value a shader takes as a uniform: `f32` and `i32`, `[f32; 2..=4]` for
/// its vectors, and references to arrays for its `float[]` and `vec4[]`.
///
/// An array is named by its first element, as `"u_weight[0]"`: drivers differ
/// on whether the bare array name resolves, and `name[0]` always does.
pub trait Uniform {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation);
}

impl Uniform for i32 {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation) {
        unsafe { gl.uniform_1_i32(Some(at), self) };
    }
}

impl Uniform for f32 {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation) {
        unsafe { gl.uniform_1_f32(Some(at), self) };
    }
}

impl Uniform for [f32; 2] {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation) {
        unsafe { gl.uniform_2_f32(Some(at), self[0], self[1]) };
    }
}

impl Uniform for [f32; 3] {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation) {
        unsafe { gl.uniform_3_f32(Some(at), self[0], self[1], self[2]) };
    }
}

impl Uniform for [f32; 4] {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation) {
        unsafe { gl.uniform_4_f32(Some(at), self[0], self[1], self[2], self[3]) };
    }
}

impl<const N: usize> Uniform for &[f32; N] {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation) {
        unsafe { gl.uniform_1_f32_slice(Some(at), self) };
    }
}

impl<const N: usize> Uniform for &[[f32; 4]; N] {
    fn upload(self, gl: &glow::Context, at: &glow::UniformLocation) {
        unsafe { gl.uniform_4_f32_slice(Some(at), self.as_flattened()) };
    }
}

// ---------------------------------------------------------------------------
// Fullscreen triangle
// ---------------------------------------------------------------------------

/// A single oversized triangle covering the viewport.
///
/// No vertex buffer: the vertex shader derives position and UV from
/// `gl_VertexID`, so this only needs an empty VAO to satisfy core profile.
pub struct ScreenQuad {
    vao: glow::VertexArray,
}

impl ScreenQuad {
    pub fn new(gl: &glow::Context) -> Result<Self, Error> {
        let vao = unsafe { gl.create_vertex_array().map_err(Error::Gl)? };
        Ok(Self { vao })
    }

    /// Draw into `target`'s active region. The viewport is the active image,
    /// not the whole texture, so `v_uv` still spans 0..1 across the image.
    pub fn draw_to(&self, gl: &glow::Context, target: &Target) {
        let (Some(fbo), (w, h)) = (target.fbo(), target.size()) else {
            return;
        };
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, w as i32, h as i32);
            gl.bind_vertex_array(Some(self.vao));
            // Every pass writes every pixel of its region and reads none of
            // the target, so blending and depth would only cost time.
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    }

    /// Draw over what `target` already holds, by the alpha the pass writes:
    /// the one pass that lays a picture on another rather than replacing it.
    pub fn blend_to(&self, gl: &glow::Context, target: &Target) {
        let (Some(fbo), (w, h)) = (target.fbo(), target.size()) else {
            return;
        };
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, w as i32, h as i32);
            gl.bind_vertex_array(Some(self.vao));
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.disable(glow::BLEND);
            gl.bind_vertex_array(None);
        }
    }

    /// Draw into a rectangle of a framebuffer that is not one of ours — the
    /// window's. `rect` is x, y, width and height as GL counts them, from
    /// the bottom-left, and `v_uv` spans 0..1 across it.
    pub fn draw_at(
        &self,
        gl: &glow::Context,
        fbo: Option<glow::Framebuffer>,
        rect: (i32, i32, i32, i32),
    ) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
            gl.viewport(rect.0, rect.1, rect.2, rect.3);
            gl.bind_vertex_array(Some(self.vao));
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::STENCIL_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(true, true, true, true);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    }

    /// The same, laid over what is already there as premultiplied colour
    /// rather than written in its place, and cut to `clip`: x, y, width and
    /// height, as GL counts them.
    pub fn blend_at(
        &self,
        gl: &glow::Context,
        fbo: Option<glow::Framebuffer>,
        rect: (i32, i32, i32, i32),
        clip: (i32, i32, i32, i32),
    ) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
            gl.viewport(rect.0, rect.1, rect.2, rect.3);
            gl.bind_vertex_array(Some(self.vao));
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::STENCIL_TEST);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(clip.0, clip.1, clip.2.max(0), clip.3.max(0));
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            // The window's alpha is left as it is.
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
            gl.color_mask(true, true, true, true);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_vertex_array(None);
        }
    }

    /// The same, multiplying what is already there by the shader's alpha,
    /// colour and alpha alike: a mask, which leaves the window premultiplied.
    pub fn mask_at(
        &self,
        gl: &glow::Context,
        fbo: Option<glow::Framebuffer>,
        rect: (i32, i32, i32, i32),
    ) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
            gl.viewport(rect.0, rect.1, rect.2, rect.3);
            gl.bind_vertex_array(Some(self.vao));
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::STENCIL_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ZERO, glow::SRC_ALPHA);
            gl.color_mask(true, true, true, true);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.disable(glow::BLEND);
            gl.bind_vertex_array(None);
        }
    }

    pub fn release(self, gl: &glow::Context) {
        unsafe { gl.delete_vertex_array(self.vao) };
    }
}

/// Bind `tex` to unit `unit` and point the sampler uniform at it.
pub fn bind_texture(
    gl: &glow::Context,
    program: &Program,
    name: &str,
    unit: u32,
    tex: Option<glow::Texture>,
) {
    unsafe {
        gl.active_texture(glow::TEXTURE0 + unit);
        gl.bind_texture(glow::TEXTURE_2D, tex);
    }
    program.set(gl, name, unit as i32);
}

#[cfg(test)]
mod tests {
    use super::round_up;

    #[test]
    fn allocation_rounds_up_without_wrapping_past_the_end() {
        assert_eq!(round_up(0), Some(0));
        assert_eq!(round_up(1), Some(64));
        assert_eq!(round_up(64), Some(64));
        assert_eq!(round_up(65), Some(128));
        assert_eq!(round_up(u32::MAX), None);
    }
}
