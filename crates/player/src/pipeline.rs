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
//! The blur chain is a dual-Kawase pyramid ending at quarter resolution, and
//! feeds the glass pass as the backdrop the panels refract.
//!
//! The division of labour with the UI: Slint owns layout and content, this
//! owns material. Panel rectangles are read out of Slint's own layout every
//! frame, in-process and in the same frame, so the glass cannot drift from
//! the widget it belongs to.

use glow::HasContext;

use crate::gfx::{bind_texture, saved_draw_fbo, Program, ScreenQuad, Target};
use crate::mpv::RenderContext;

/// Depth of the downsample pyramid; the smallest level is 1/2^LEVELS of the
/// window. This sets how far the blur reaches, independently of the
/// resolution it comes back at.
const LEVELS: usize = 2;
/// Pyramid level the upsample chain stops at, and so the resolution of the
/// blur texture: 0 is native, 1 half, 2 quarter.
///
/// Must be less than `LEVELS` - the chain climbs from `LEVELS` down to this,
/// so if they are equal there is nothing to climb and the blur texture is
/// never written at all.
const OUTPUT_LEVEL: usize = 0;

/// How the backdrop blur is produced.
// One variant is always unused: `BLUR` is a compile-time choice.
#[allow(dead_code)]
#[derive(PartialEq, Eq)]
pub enum BlurKind {
    /// Dual-Kawase pyramid. A wide blur for very little work, because most of
    /// it happens at reduced size. `LEVELS` and `OUTPUT_LEVEL` apply, and
    /// `GlassParams::blur` scales the tap offset.
    ///
    /// Note that even at `blur: 0.0` this still softens the backdrop, because
    /// the downsampling itself is a low-pass. There is no setting here that
    /// yields a genuinely sharp backdrop.
    Pyramid,
    /// Separable Gaussian at native resolution. No downsampling anywhere, so
    /// cost scales linearly with radius instead of being nearly free.
    /// `GlassParams::blur_sigma` is the radius in pixels - and at 0 it is a
    /// passthrough, giving a perfectly sharp backdrop. `LEVELS` and
    /// `OUTPUT_LEVEL` are unused.
    ///
    /// Worth the cost for two reasons: a sharp backdrop is unreachable with
    /// the pyramid at all, and refraction magnifies the backdrop, under which
    /// a pyramid-reconstructed texture can show faint blockiness.
    FullRes,
}

pub const BLUR: BlurKind = BlurKind::FullRes;

// Catch that at build time rather than as a silently black backdrop: an empty
// upsample range leaves `blur_out` holding the black it was cleared to, and
// the glass then refracts nothing.
const _: () = assert!(
    OUTPUT_LEVEL < LEVELS,
    "OUTPUT_LEVEL must be less than LEVELS, or the upsample chain never runs"
);
/// Base Kawase tap offset, in texels of each pass's source. Scaled at runtime
/// by `GlassParams::blur`.
const KAWASE_OFFSET: f32 = 1.0;
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
    /// Gaussian radius in pixels; 0 is a sharp passthrough. Only used when
    /// `BLUR` is `FullRes`.
    pub blur_sigma: f32,
    /// Backdrop blur strength, as a multiplier on the Kawase tap offset.
    /// Only used when `BLUR` is `Pyramid`.
    ///
    /// 1.0 is the natural radius for the pyramid depth; roughly, effective
    /// radius scales as `blur * 2^LEVELS`. Push much past ~3.0 and the taps
    /// spread far enough apart that the pyramid stops hiding its own
    /// structure and banding shows — widen `LEVELS` instead for more than
    /// that, at the cost of two passes and a coarser backdrop.
    pub blur: f32,
    /// Width of the domed rim, as a fraction of each panel's own corner
    /// radius - how far in from the edge the lensing and lighting reach.
    /// Relative rather than absolute so one value suits a 380px panel and a
    /// 60px pill alike; see `u_bevel` in glass.frag.
    pub bevel: f32,
    /// Effective glass thickness for transmission, as a fraction of the bevel
    /// width. How far the refracted ray descends before it reaches the
    /// backdrop, so it scales how strongly the rim displaces what is behind
    /// it.
    pub refract: f32,
    /// Index of refraction: 1.0 bends nothing, 1.33 water, 1.5 window glass,
    /// 1.8 flint, 2.4 diamond. Drives both the bending and - through the
    /// Fresnel term - how mirror-like the rim becomes.
    pub ior: f32,
    /// Fraction by which the red and blue transmission offsets differ from
    /// green: the prismatic fringe.
    pub aberration: f32,
    /// Overall reflection strength, scaling the Fresnel weight.
    pub specular: f32,
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

impl Default for GlassParams {
    fn default() -> Self {
        Self {
            blur_sigma: 2.3,
            blur: 1.0,
            // The rim runs the full width of the corner radius, and the
            // glass is as thick as that rim is wide.
            bevel: 1.0,
            refract: 1.0,
            // Diamond. Physically absurd for a window pane, but the whole
            // point here is a rim that bends and mirrors hard enough to read
            // at UI scale.
            ior: 3.0,
            aberration: 0.1,
            specular: 1.0,
            sky: 0.0,
            // Light from the upper left, the convention every OS uses.
            light_dir: [-0.707, -0.707],
            tint: [0.01, 0.01, 0.01],
            tint_amount: 0.2,
        }
    }
}

pub struct Pipeline {
    quad: ScreenQuad,
    prog_extend: Program,
    prog_spread: Program,
    prog_down: Program,
    prog_up: Program,

    /// mpv renders here, with a transparent surround.
    video: Target,
    /// Edge-extension pass output.
    extended: Target,
    /// Ambient border with the video composited over it. What Slint shows.
    composite: Target,
    down: Vec<Target>,
    up: Vec<Target>,
    /// Final blur output, at `OUTPUT_LEVEL`.
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
    /// Scratch for the horizontal half of the full-resolution Gaussian.
    /// Unallocated unless `BLUR` is `FullRes`.
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
            prog_down: Program::new(gl, vert, include_str!("../shaders/blur_down.frag"))
                .map_err(|e| format!("blur_down: {e}"))?,
            prog_up: Program::new(gl, vert, include_str!("../shaders/blur_up.frag"))
                .map_err(|e| format!("blur_up: {e}"))?,
            video: Target::new(),
            extended: Target::new(),
            composite: Target::new(),
            down: (0..LEVELS).map(|_| Target::new()).collect(),
            up: (0..LEVELS).map(|_| Target::new()).collect(),
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
        })
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

    /// The bevel slider as it stands, for diagnostics that need to say what
    /// width the ratio works out to on a given panel.
    pub fn glass_bevel(&self) -> f32 {
        self.glass.bevel
    }

    pub fn composite(&self) -> &Target {
        &self.composite
    }

    /// The blurred backdrop glass panels sample — the buffer most recently
    /// written, which is the one Slint should be showing.
    pub fn blur(&self) -> &Target {
        &self.blur_out
    }

    /// Sample the centre of every stage of the blur pyramid, to find where
    /// a black frame enters. Diagnostics only - stalls the GL pipeline.
    pub fn blur_chain_debug(&self, gl: &glow::Context) -> String {
        let mut out = String::new();
        let probe = |t: &Target| {
            let (w, h) = t.size();
            let px = t.sample_grid(gl, &[(w / 2, h / 2)]);
            format!("{w}x{h}{:?}", px.first().copied().unwrap_or([0; 4]))
        };
        out.push_str(&format!("composite {} | ", probe(&self.composite)));
        for (k, t) in self.down.iter().enumerate() {
            out.push_str(&format!("down[{k}]=L{} {} | ", k + 1, probe(t)));
        }
        for (level, t) in self.up.iter().enumerate() {
            if (OUTPUT_LEVEL + 1..LEVELS).contains(&level) {
                out.push_str(&format!("up[{level}] {} | ", probe(t)));
            }
        }
        out.push_str(&format!("blur_out {}", probe(&self.blur_out)));
        out
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
        if !self.video.ensure_size(gl, w, h)
            || !self.extended.ensure_size(gl, w, h)
            || !self.composite.ensure_size(gl, w, h)
            || !self.glassed.ensure_size(gl, w, h)
        {
            return (false, resized);
        }
        if BLUR == BlurKind::FullRes {
            // Both Gaussian passes run at native size; the pyramid targets
            // are dead weight in this mode and stay unallocated.
            if !self.gauss_tmp.ensure_size(gl, w, h) || !self.blur_out.ensure_size(gl, w, h) {
                return (false, resized);
            }
            return (true, resized);
        }

        // `down[k]` holds level k+1, so the chain descends 1..=LEVELS.
        for k in 0..LEVELS {
            let (lw, lh) = level_size(w, h, k + 1);
            if !self.down[k].ensure_size(gl, lw, lh) {
                return (false, resized);
            }
        }
        // Coming back up, levels LEVELS-1 down to OUTPUT_LEVEL are written.
        // The last of those lands in `blur_out`, so only the intermediate
        // ones need a target of their own.
        for level in OUTPUT_LEVEL + 1..LEVELS {
            let (lw, lh) = level_size(w, h, level);
            if !self.up[level].ensure_size(gl, lw, lh) {
                return (false, resized);
            }
        }
        let (ow, oh) = level_size(w, h, OUTPUT_LEVEL);
        if !self.blur_out.ensure_size(gl, ow, oh) {
            return (false, resized);
        }
        (true, resized)
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
        if new_frame || resized {
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
        let full = new_frame || resized || params_dirty || rect_changed;
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
    fn run_blur_gauss(&mut self, gl: &glow::Context) {
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

        self.prog_glass.set_f32(gl, "u_bevel", g.bevel);
        self.prog_glass.set_f32(gl, "u_refract", g.refract);
        self.prog_glass.set_f32(gl, "u_ior", g.ior);
        self.prog_glass.set_f32(gl, "u_aberration", g.aberration);
        self.prog_glass.set_f32(gl, "u_specular", g.specular);
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

    fn run_blur(&mut self, gl: &glow::Context) {
        if BLUR == BlurKind::FullRes {
            self.run_blur_gauss(gl);
            return;
        }
        // Down the pyramid. Each pass reads the level above it, so the texel
        // size passed in is always the *source* texel size.
        self.prog_down.bind(gl);
        let offset = KAWASE_OFFSET * self.glass.blur.max(0.0);
        self.prog_down.set_f32(gl, "u_offset", offset);
        let mut src = self.composite.texture();
        let mut src_size = self.composite.size();
        let mut src_scale = self.composite.scale();
        for i in 0..LEVELS {
            self.prog_down.set_vec2(
                gl,
                "u_texel",
                1.0 / src_size.0.max(1) as f32,
                1.0 / src_size.1.max(1) as f32,
            );
            self.prog_down
                .set_vec2(gl, "u_src_scale", src_scale.0, src_scale.1);
            bind_texture(gl, &self.prog_down, "u_src", 0, src);
            self.quad.draw_to(gl, &self.down[i]);
            src = self.down[i].texture();
            src_size = self.down[i].size();
            src_scale = self.down[i].scale();
        }

        // Back up to OUTPUT_LEVEL. Because the levels are now numbered by
        // their actual shift, OUTPUT_LEVEL 0 means a final pass at native
        // resolution rather than being unreachable.
        self.prog_up.bind(gl);
        self.prog_up.set_f32(gl, "u_offset", offset);
        for level in (OUTPUT_LEVEL..LEVELS).rev() {
            let target: &Target = if level == OUTPUT_LEVEL {
                &self.blur_out
            } else {
                &self.up[level]
            };
            self.prog_up.set_vec2(
                gl,
                "u_texel",
                1.0 / src_size.0.max(1) as f32,
                1.0 / src_size.1.max(1) as f32,
            );
            self.prog_up
                .set_vec2(gl, "u_src_scale", src_scale.0, src_scale.1);
            bind_texture(gl, &self.prog_up, "u_src", 0, src);
            self.quad.draw_to(gl, target);
            src = target.texture();
            src_size = target.size();
            src_scale = target.scale();
        }
    }

    pub fn release(&mut self, gl: &glow::Context) {
        self.video.release(gl);
        self.extended.release(gl);
        self.composite.release(gl);
        self.glassed.release(gl);
        self.gauss_tmp.release(gl);
        for t in self.down.iter_mut().chain(self.up.iter_mut()) {
            t.release(gl);
        }
        self.blur_out.release(gl);
        self.quad.release(gl);
        for p in [
            &self.prog_extend,
            &self.prog_spread,
            &self.prog_down,
            &self.prog_up,
            &self.prog_glass,
            &self.prog_gauss,
        ] {
            p.release(gl);
        }
    }
}

/// Size of pyramid level `i`, never smaller than one texel.
fn level_size(w: u32, h: u32, level: usize) -> (u32, u32) {
    ((w >> level).max(1), (h >> level).max(1))
}
