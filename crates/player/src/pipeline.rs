//! The per-frame GPU chain: mpv's frame in, a composited image and a blurred
//! backdrop out.
//!
//! ```text
//!   mpv ─> video ─> extend ─> spread(+composite) ─> composite ─┬─> glass ─> Slint
//!                                                              │        ^
//!                                                              └─ blur ─┘
//! ```
//!
//! `extend` and `spread` are the two halves of the ambient border, ported off
//! the forked mpv `//!HOOK BORDER` stage. `spread` also lays the video over
//! the border it just drew, which is the step mpv's own compositor used to do
//! and the reason the fork existed.
//!
//! The blur is a separable Gaussian at the window's own resolution, and feeds
//! the glass pass as the backdrop the panels refract.
//!
//! The division of labour with the UI: Slint owns layout and content, this
//! owns material. Panel rectangles are read out of Slint's own layout every
//! frame, in-process and in the same frame, so the glass cannot drift from
//! the widget it belongs to.

use glow::HasContext;

use crate::gfx::{bind_texture, saved_draw_fbo, Program, ScreenQuad, Target};
use crate::mpv::RenderContext;

/// Must match `MAX_PANELS` in glass.frag.
///
/// Ten is what the interface can put on screen at once: five pieces of bar,
/// one open panel and the seek preview, with room left over. The shader loops
/// to this bound whatever the count, so the headroom costs an early exit.
pub const MAX_PANELS: usize = 14;

/// Live parameters for the ambient border. Defaults match the `//!PARAM`
/// defaults the mpv shader shipped with.
#[derive(Clone, Copy, Debug)]
pub struct BorderParams {
    pub edge_blur: f32,
    pub spread: f32,
    pub grain: f32,
    pub falloff: f32,
    pub falloff_softness: f32,
    pub max_taps: f32,
}

impl Default for BorderParams {
    fn default() -> Self {
        Self {
            edge_blur: 0.007,
            spread: 0.68,
            grain: 124.0,
            falloff: 4.0,
            falloff_softness: 0.2,
            max_taps: 256.0,
        }
    }
}

/// One glass panel, in physical pixels, top-left origin.
///
/// Filled from Slint's own layout every frame, so the material can never
/// drift from the widget it belongs to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassPanel {
    pub rect: [f32; 4],
    pub radius: f32,
    pub tint_alpha: f32,
    /// How far the panel has faded in, 0..1. Scales the glass with it, so a
    /// surface arriving does not have fully-formed glass beneath it.
    pub opacity: f32,
}

/// Look of the glass. These are the knobs worth exposing to a settings panel
/// later; the defaults aim at Apple-ish rather than subtle.
#[derive(Clone, Copy, Debug)]
pub struct GlassParams {
    /// Gaussian radius of the backdrop blur, in pixels; 0 is a sharp
    /// passthrough.
    pub blur_sigma: f32,
    /// Effective glass thickness for transmission, as a fraction of the bevel
    /// width. How far the refracted ray descends before it reaches the
    /// backdrop, so it scales how strongly the rim displaces what is behind
    /// it.
    pub refract: f32,
    /// Dispersive power — how far the index of refraction spreads across the
    /// visible band, as (n_F - n_C) / (n_d - 1), the reciprocal of the Abbe
    /// number. The shader integrates the transmitted ray over wavelength
    /// with it, so this is the width of a spectrum rather than the gap
    /// between three copies of an edge: the prismatic fringe.
    pub aberration: f32,
    /// Brightness of the synthetic sky seen where the reflection escapes
    /// upward off the bevel.
    pub sky: f32,
    /// Direction that sky highlight comes from, in screen space.
    pub light_dir: [f32; 2],
    pub tint: [f32; 3],
    /// How far a panel pulls toward the tint colour, 0..1. Scales the
    /// per-panel opt-in rather than replacing it.
    pub tint_amount: f32,
}

// --- the material's fixed properties ----------------------------------------
//
// Three numbers that were sliders on the glass page and are settings no
// longer. They are what the material *is* rather than how much of it you
// want: the rim is the corner, the glass is flint, and a mirror reflects.
// Turning any of them makes the panes stop being one material — and unlike
// blur, fringe, sky or tint, none of them has a range where the answer is a
// matter of taste rather than of whether the glass still reads as glass.
//
// They live here rather than in `GlassParams` because a value nobody can set
// is not a parameter. Nothing carries them through the store, nothing writes
// them to disk, and `settings.rs` has three fewer entries in its registry —
// the panel's glass page follows the registry, so it simply shows five rows.
// An old settings file naming `glass.bevel_ratio`, `glass.ior` or
// `glass.specular` is skipped the way any unknown key is.

/// Width of the domed rim, as a fraction of each panel's own corner radius —
/// how far in from the edge the lensing and lighting reach.
///
/// Relative rather than absolute so one value suits a 380px panel and a 60px
/// pill alike; see `u_bevel` in glass.frag. At 1.0 the rim runs the full
/// width of the corner, which is what makes a capsule rim the whole way
/// through and gives a small pane a small rolled edge — the Proportional Rim
/// Rule, and also how real glass is made.
pub const BEVEL: f32 = 1.0;

/// Index of refraction: 1.0 bends nothing, 1.33 water, 1.5 window glass,
/// 1.8 flint, 2.4 diamond. Drives both the bending and — through the Fresnel
/// term — how mirror-like the rim becomes.
///
/// Well past a window pane, so the rim bends and mirrors hard enough to read
/// at UI scale. It sat at 1.7 while it was a slider and was tuned up from
/// there by eye; 2.0 is what shipped configs actually hold. Higher is not
/// better — at diamond the lensing pulls a dark patch of the backdrop into a
/// black oval across the top of the way in, which is the wall this was
/// stopped short of.
pub const IOR: f32 = 2.0;

/// Overall reflection strength, scaling the Fresnel weight.
///
/// Unity: the Fresnel term is already the physical answer to how much light
/// a surface at that angle returns, so this exists to scale it and there is
/// no reason to. It was a slider because everything beside it was one.
pub const SPECULAR: f32 = 1.0;

impl Default for GlassParams {
    fn default() -> Self {
        Self {
            blur_sigma: 2.0,
            // The glass is as thick as the rim is wide; see `BEVEL`.
            refract: 1.0,
            aberration: 0.1,
            sky: 0.0,
            // Light from the upper left, the convention every OS uses.
            light_dir: [-0.707, -0.707],
            tint: [0.0, 0.0, 0.0],
            tint_amount: 0.2,
        }
    }
}

pub struct Pipeline {
    quad: ScreenQuad,
    prog_extend: Program,
    prog_spread: Program,

    /// mpv renders here, with a transparent surround.
    video: Target,
    /// Edge-extension pass output.
    extended: Target,
    /// Ambient border with the video composited over it. What Slint shows.
    composite: Target,
    /// The composite, blurred: what the glass refracts.
    ///
    /// Single-buffered: this was double-buffered while Slint displayed the
    /// blur directly, because a rounded-clip subtree gets cached into a layer
    /// that only invalidates when a property changes — and a borrowed GL
    /// texture compares by id, so republishing the same one was a no-op and
    /// froze the panel. Now the glass pass consumes this and Slint only ever
    /// sees the finished frame, so one buffer is enough.
    blur_out: Target,
    /// Composite with the glass panels drawn into it. This is what Slint
    /// shows; the panels themselves contribute only their content.
    glassed: Target,
    prog_glass: Program,
    prog_gauss: Program,
    /// Scratch for the horizontal half of the Gaussian.
    gauss_tmp: Target,
    panels: Vec<GlassPanel>,
    pub glass: GlassParams,

    /// (mpv, border+blur, glass) from the last frame, when `DBM_GPU_TIME`
    /// is on. Zero otherwise.
    pub timings: (
        std::time::Duration,
        std::time::Duration,
        std::time::Duration,
    ),
    /// (update_rect, border, blur) — the same frame as `timings`.
    pub split: (
        std::time::Duration,
        std::time::Duration,
        std::time::Duration,
    ),
    gpu_time: bool,
    pub params: BorderParams,
    /// Video rect within the window, normalised (x0, y0, x1, y1).
    rect: [f32; 4],
    /// `border` off means skip both border passes and show the video alone.
    pub border_enabled: bool,
    /// Glass off is expressed as zero panels rather than a shader branch:
    /// the pass already short-circuits on an empty list, so it costs one
    /// full-screen copy and no special case.
    pub glass_enabled: bool,

    prog_backdrop: Program,
    /// A frame of the last film left unfinished — see [`Pipeline::set_backdrop`].
    backdrop: Option<Backdrop>,
    /// Whether that frame stands in for mpv's picture. The driver sets it
    /// from whether a file is loaded; the moment one is, mpv's own frames
    /// take the target back.
    pub show_backdrop: bool,
}

/// How long the backdrop takes to come up. Longer than a surface's 120ms,
/// because this is the window's light changing rather than a control
/// answering, and a whole picture arriving in a tenth of a second is a flash.
const BACKDROP_ARRIVAL: std::time::Duration = std::time::Duration::from_millis(400);

struct Backdrop {
    tex: glow::Texture,
    width: u32,
    height: u32,
    arrived: std::time::Instant,
    /// Whether the last draw was the finished one, so an unchanging picture
    /// is not redrawn every frame for nothing.
    settled: bool,
}

impl Pipeline {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let vert = include_str!("../shaders/fullscreen.vert");
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            prog_extend: Program::new(gl, vert, include_str!("../shaders/border_extend.frag"))
                .map_err(|e| format!("border_extend: {e}"))?,
            prog_spread: Program::new(gl, vert, include_str!("../shaders/border_spread.frag"))
                .map_err(|e| format!("border_spread: {e}"))?,
            video: Target::new(),
            extended: Target::new(),
            composite: Target::new(),
            blur_out: Target::new(),
            glassed: Target::new(),
            prog_glass: Program::new(gl, vert, include_str!("../shaders/glass.frag"))
                .map_err(|e| format!("glass: {e}"))?,
            prog_gauss: Program::new(gl, vert, include_str!("../shaders/blur_gauss.frag"))
                .map_err(|e| format!("blur_gauss: {e}"))?,
            gauss_tmp: Target::new(),
            panels: Vec::new(),
            glass: GlassParams::default(),
            timings: Default::default(),
            split: Default::default(),
            gpu_time: std::env::var_os("DBM_GPU_TIME").is_some(),
            params: BorderParams::default(),
            rect: [0.0, 0.0, 1.0, 1.0],
            border_enabled: true,
            glass_enabled: true,
            prog_backdrop: Program::new(gl, vert, include_str!("../shaders/backdrop.frag"))
                .map_err(|e| format!("backdrop: {e}"))?,
            backdrop: None,
            show_backdrop: false,
        })
    }

    /// Take delivery of the frame the empty window stands in front of.
    ///
    /// Uploaded once and kept: it is a few hundred kilobytes, and the window
    /// only needs it until a film arrives.
    pub fn set_backdrop(&mut self, gl: &glow::Context, still: &crate::preview::Still) {
        let tex = unsafe {
            let Ok(tex) = gl.create_texture() else {
                return;
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                still.width as i32,
                still.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&still.rgba)),
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
            if let Some(old) = self.backdrop.take() {
                gl.delete_texture(old.tex);
            }
            tex
        };
        self.backdrop = Some(Backdrop {
            tex,
            width: still.width,
            height: still.height,
            arrived: std::time::Instant::now(),
            settled: false,
        });
    }

    /// Whether the backdrop still has frames of its arrival to draw, and so
    /// whether the driver should keep asking for them: nothing else is, with
    /// no film playing and nobody touching anything.
    pub fn backdrop_arriving(&self) -> bool {
        self.show_backdrop && self.backdrop.as_ref().is_some_and(|b| !b.settled)
    }

    fn draw_backdrop(&mut self, gl: &glow::Context, w: u32, h: u32) {
        let Some(b) = self.backdrop.as_mut() else {
            return;
        };
        let t = (b.arrived.elapsed().as_secs_f32() / BACKDROP_ARRIVAL.as_secs_f32()).min(1.0);
        // Ease out, like every other arrival here.
        let level = 1.0 - (1.0 - t).powi(3);
        b.settled = t >= 1.0;
        let (tex, iw, ih) = (b.tex, b.width, b.height);

        self.prog_backdrop.bind(gl);
        self.prog_backdrop
            .set_vec2(gl, "u_size", w as f32, h as f32);
        self.prog_backdrop
            .set_vec2(gl, "u_image_size", iw as f32, ih as f32);
        self.prog_backdrop.set_f32(gl, "u_level", level);
        bind_texture(gl, &self.prog_backdrop, "u_image", 0, Some(tex));
        self.quad.draw_to(gl, &self.video);
    }

    /// The image Slint draws: video, ambient border and glass panels, all
    /// composited. Slint draws only panel *content* over this.
    pub fn output(&self) -> &Target {
        &self.glassed
    }

    /// Video plus ambient border, before the glass pass. Diagnostics.
    /// The video rect the border was last drawn from. For diagnostics that
    /// need to compare it against what mpv currently reports.
    pub fn border_rect(&self) -> [f32; 4] {
        self.rect
    }

    pub fn composite(&self) -> &Target {
        &self.composite
    }

    /// The blurred backdrop glass panels sample — the buffer most recently
    /// written, which is the one Slint should be showing.
    pub fn blur(&self) -> &Target {
        &self.blur_out
    }

    /// Sample the centre of every stage of the blur, to find where a black
    /// frame enters. Diagnostics only - stalls the GL pipeline.
    pub fn blur_chain_debug(&self, gl: &glow::Context) -> String {
        let probe = |t: &Target| {
            let (w, h) = t.size();
            let px = t.sample_grid(gl, &[(w / 2, h / 2)]);
            format!("{w}x{h}{:?}", px.first().copied().unwrap_or([0; 4]))
        };
        format!(
            "composite {} | horizontal {} | blur_out {}",
            probe(&self.composite),
            probe(&self.gauss_tmp),
            probe(&self.blur_out),
        )
    }

    /// The raw mpv output, before the border pass. Diagnostics only.
    #[allow(dead_code)]
    pub fn video(&self) -> &Target {
        &self.video
    }

    /// Allocate every target for a window of `w` x `h` physical pixels.
    ///
    /// Returns `(allocated_ok, resized)`. A resize discards every target's
    /// contents, so the caller has to re-run the chain even without a new
    /// frame from mpv.
    fn ensure_sizes(&mut self, gl: &glow::Context, w: u32, h: u32) -> (bool, bool) {
        let resized = self.composite.size() != (w, h);
        // Every target is the window's own size: both halves of the Gaussian
        // run at native resolution, like everything else.
        let ok = [
            &mut self.video,
            &mut self.extended,
            &mut self.composite,
            &mut self.glassed,
            &mut self.gauss_tmp,
            &mut self.blur_out,
        ]
        .into_iter()
        .all(|target| target.ensure_size(gl, w, h));
        (ok, resized)
    }

    /// Pull mpv's current frame and run the whole chain.
    ///
    /// Returns false if nothing could be rendered (a zero-sized window, or a
    /// failed allocation), in which case the previous textures still stand.
    pub fn render(
        &mut self,
        gl: &glow::Context,
        ctx: &RenderContext,
        w: u32,
        h: u32,
        new_frame: bool,
        // A material parameter changed, so the passes must run again even
        // with nothing else moving - otherwise a slider does nothing on a
        // paused frame.
        params_dirty: bool,
        // Video rect within the window, normalised. Comes from observed
        // state rather than being read from mpv here — see `state::OBSERVED`.
        rect: [f32; 4],
        panels: &[GlassPanel],
    ) -> Result<bool, String> {
        let (ok, resized) = self.ensure_sizes(gl, w, h);
        if !ok {
            return Ok(false);
        }
        let saved_fbo = saved_draw_fbo(gl);

        // `DBM_GPU_TIME=1` inserts a glFinish between passes so the numbers
        // reflect GPU work rather than how fast we queued it. It serialises
        // the pipeline by construction, so it is opt-in.
        let gpu_time = self.gpu_time;
        let mark = |gl: &glow::Context| {
            if gpu_time {
                unsafe { gl.finish() };
            }
            std::time::Instant::now()
        };
        let t0 = mark(gl);

        // Also on resize, not only on a new frame. The target was just
        // reallocated and cleared, so without this a paused player shows
        // black after a resize — and worse, the border pass samples that
        // black and grows an ambient glow out of nothing. mpv is happy to
        // re-render the frame it is already holding.
        //
        // With nothing loaded and a backdrop on hand, that stands in for the
        // frame instead: drawn when it arrives, while it fades up, and after
        // a resize cleared it. mpv's `new_frame` is ignored meanwhile — with
        // no file it has nothing to render but black over the top of it.
        let mut drew_backdrop = false;
        if self.show_backdrop && self.backdrop.is_some() {
            if resized || self.backdrop_arriving() {
                self.draw_backdrop(gl, w, h);
                drew_backdrop = true;
            }
        } else if new_frame || resized {
            let Some(fbo) = self.video.fbo() else {
                return Ok(false);
            };
            let saved = saved_draw_fbo(gl);
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                let mut viewport = [0i32; 4];
                gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
                let res = ctx.render_to_fbo(fbo.0.get(), w as i32, h as i32);
                gl.bind_framebuffer(glow::FRAMEBUFFER, saved);
                gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
                res.map_err(|e| e.to_string())?;
            }
        }

        // Nothing downstream changes unless mpv produced a frame, the targets
        // were reallocated, or a panel moved. Slint redraws for its own
        // reasons — a resize drag most of all — and re-running a
        // 256-tap-per-pixel border pass on every one of those was most of the
        // resize lag.
        let t1 = mark(gl);
        let panels_changed = self.panels != panels;
        // The border is drawn from `rect`, so a change in it invalidates the
        // border as surely as a new frame does — and it can change on its
        // own. mpv reports `osd-dimensions` asynchronously, so the rect for a
        // new window size often arrives a frame or two *after* the resize
        // that caused it. While playing, the next frame redraws the border
        // and nobody sees it; paused, there is no next frame, and the border
        // keeps the previous window's geometry until playback resumes.
        let rect_changed = self.rect != rect;
        let full = new_frame || resized || params_dirty || rect_changed || drew_backdrop;
        if !full && !panels_changed {
            return Ok(true);
        }

        if full {
            self.rect = rect;
            let ta = mark(gl);
            self.run_border(gl, w, h);
            let tb = mark(gl);
            self.run_blur(gl);
            let tc = mark(gl);
            if gpu_time {
                self.split = (
                    ta.duration_since(t1),
                    tb.duration_since(ta),
                    tc.duration_since(tb),
                );
            }
        }
        let t2 = mark(gl);

        // A panel can move without a new video frame — a menu opening, a
        // toolbar sliding — and only the glass pass depends on that, so the
        // border and blur above are skipped in that case.
        self.panels.clear();
        self.panels.extend_from_slice(panels);
        self.run_glass(gl, w, h);
        let t3 = mark(gl);
        if gpu_time {
            self.timings = (
                t1.duration_since(t0),
                t2.duration_since(t1),
                t3.duration_since(t2),
            );
        }

        // Each pass leaves its own target bound; hand Slint back the
        // framebuffer it was rendering into.
        unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, saved_fbo) };
        Ok(true)
    }

    /// Separable Gaussian at native resolution: horizontal into scratch,
    /// vertical into the blur output.
    ///
    /// Native rather than downsampled because refraction magnifies the
    /// backdrop with sub-pixel displacement, and a texture rebuilt from a
    /// smaller one shows its blocks under that magnification. It also makes
    /// a radius of 0 a true passthrough, which no downsampling chain can be.
    fn run_blur(&mut self, gl: &glow::Context) {
        let sigma = self.glass.blur_sigma.max(0.0);
        self.prog_gauss.bind(gl);
        self.prog_gauss.set_f32(gl, "u_sigma", sigma);

        for (dir, src, dst) in [
            ((1.0f32, 0.0f32), &self.composite, &self.gauss_tmp),
            ((0.0, 1.0), &self.gauss_tmp, &self.blur_out),
        ] {
            let (sw, sh) = src.size();
            let scale = src.scale();
            self.prog_gauss.set_vec2(gl, "u_dir", dir.0, dir.1);
            self.prog_gauss.set_vec2(
                gl,
                "u_texel",
                1.0 / sw.max(1) as f32,
                1.0 / sh.max(1) as f32,
            );
            self.prog_gauss
                .set_vec2(gl, "u_src_scale", scale.0, scale.1);
            bind_texture(gl, &self.prog_gauss, "u_src", 0, src.texture());
            self.quad.draw_to(gl, dst);
        }
    }

    /// Composite the glass panels over the finished frame.
    fn run_glass(&mut self, gl: &glow::Context, w: u32, h: u32) {
        let g = self.glass;
        self.prog_glass.bind(gl);
        self.prog_glass.set_vec2(gl, "u_size", w as f32, h as f32);

        let (base_scale, blur_scale) = (self.composite.scale(), self.blur().scale());
        self.prog_glass
            .set_vec2(gl, "u_base_scale", base_scale.0, base_scale.1);
        self.prog_glass
            .set_vec2(gl, "u_blur_scale", blur_scale.0, blur_scale.1);

        // Uniform arrays are uploaded whole; unused slots are simply never
        // read, since the shader loop stops at `u_panel_count`.
        let count = if self.glass_enabled {
            self.panels.len().min(MAX_PANELS)
        } else {
            0
        };
        let mut rects = [0.0f32; MAX_PANELS * 4];
        let mut styles = [0.0f32; MAX_PANELS * 4];
        for (i, panel) in self.panels.iter().take(count).enumerate() {
            rects[i * 4..i * 4 + 4].copy_from_slice(&panel.rect);
            styles[i * 4] = panel.radius;
            styles[i * 4 + 1] = panel.tint_alpha;
            styles[i * 4 + 2] = panel.opacity;
        }
        self.prog_glass.set_i32(gl, "u_panel_count", count as i32);
        self.prog_glass
            .set_vec4_array(gl, "u_panel_rect[0]", &rects);
        self.prog_glass
            .set_vec4_array(gl, "u_panel_style[0]", &styles);

        self.prog_glass.set_f32(gl, "u_bevel", BEVEL);
        self.prog_glass.set_f32(gl, "u_refract", g.refract);
        self.prog_glass.set_f32(gl, "u_ior", IOR);
        self.prog_glass.set_f32(gl, "u_aberration", g.aberration);
        self.prog_glass.set_f32(gl, "u_specular", SPECULAR);
        self.prog_glass.set_f32(gl, "u_sky", g.sky);
        self.prog_glass
            .set_vec2(gl, "u_light_dir", g.light_dir[0], g.light_dir[1]);
        self.prog_glass.set_vec3(gl, "u_tint", g.tint);
        self.prog_glass.set_f32(gl, "u_tint_amount", g.tint_amount);

        bind_texture(gl, &self.prog_glass, "u_base", 0, self.composite.texture());
        bind_texture(gl, &self.prog_glass, "u_blur", 1, self.blur().texture());
        self.quad.draw_to(gl, &self.glassed);
    }

    fn run_border(&mut self, gl: &glow::Context, w: u32, h: u32) {
        let size = (w as f32, h as f32);
        let texel = (1.0 / size.0, 1.0 / size.1);
        let p = self.params;

        // Pass 1 — edge extension. Skipped entirely with the border off; the
        // spread pass then takes its short path and never reads `extended`.
        if self.border_enabled {
            self.prog_extend.bind(gl);
            self.prog_extend.set_vec2(gl, "u_size", size.0, size.1);
            self.prog_extend.set_vec2(gl, "u_texel", texel.0, texel.1);
            self.prog_extend.set_vec4(gl, "u_rect", self.rect);
            self.prog_extend.set_f32(gl, "edge_blur", p.edge_blur);
            self.prog_extend.set_f32(gl, "max_taps", p.max_taps);
            let vs = self.video.scale();
            self.prog_extend.set_vec2(gl, "u_src_scale", vs.0, vs.1);
            bind_texture(gl, &self.prog_extend, "u_src", 0, self.video.texture());
            self.quad.draw_to(gl, &self.extended);
        }

        // Pass 2 — light spread, then the video composited on top.
        self.prog_spread.bind(gl);
        self.prog_spread.set_vec2(gl, "u_size", size.0, size.1);
        self.prog_spread.set_vec2(gl, "u_texel", texel.0, texel.1);
        self.prog_spread.set_vec4(gl, "u_rect", self.rect);
        self.prog_spread.set_f32(gl, "edge_blur", p.edge_blur);
        self.prog_spread.set_f32(gl, "spread", p.spread);
        self.prog_spread.set_f32(gl, "grain", p.grain);
        self.prog_spread.set_f32(gl, "falloff", p.falloff);
        self.prog_spread
            .set_f32(gl, "falloff_softness", p.falloff_softness);
        self.prog_spread.set_f32(gl, "max_taps", p.max_taps);
        self.prog_spread
            .set_f32(gl, "u_border", if self.border_enabled { 1.0 } else { 0.0 });
        let (es, vs) = (self.extended.scale(), self.video.scale());
        self.prog_spread.set_vec2(gl, "u_src_scale", es.0, es.1);
        self.prog_spread.set_vec2(gl, "u_video_scale", vs.0, vs.1);
        bind_texture(gl, &self.prog_spread, "u_src", 0, self.extended.texture());
        bind_texture(gl, &self.prog_spread, "u_video", 1, self.video.texture());
        self.quad.draw_to(gl, &self.composite);
    }

    pub fn release(&mut self, gl: &glow::Context) {
        for target in [
            &mut self.video,
            &mut self.extended,
            &mut self.composite,
            &mut self.glassed,
            &mut self.gauss_tmp,
            &mut self.blur_out,
        ] {
            target.release(gl);
        }
        self.quad.release(gl);
        if let Some(b) = self.backdrop.take() {
            unsafe { gl.delete_texture(b.tex) };
        }
        for p in [
            &self.prog_extend,
            &self.prog_spread,
            &self.prog_glass,
            &self.prog_gauss,
            &self.prog_backdrop,
        ] {
            p.release(gl);
        }
    }
}
