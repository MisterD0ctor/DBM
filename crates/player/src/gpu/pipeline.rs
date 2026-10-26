//! The per-frame GPU chain: mpv's frame in, a composited image and a blurred
//! backdrop out.
//!
//! ```text
//!   mpv ─> video ─> extend ─> spread(+composite) ─> composite ─┬─> glass ─> Slint
//!                                                              │        ^
//!                                                              └─ blur ─┘
//! ```
//!
//! `extend` and `spread` are the two halves of the ambient border, ported from
//! the forked mpv `//!HOOK BORDER` stage. `spread` also lays the video over
//! the border it just drew, where mpv's compositor would otherwise do it.
//!
//! The blur is a Gaussian of the composite (see `blur`), and feeds the glass
//! pass as the backdrop the panels refract.
//!
//! Slint owns layout and content, and this owns material. Panel rectangles
//! are read from Slint's layout in the same frame, so the glass can't drift
//! from its widget.

use glow::HasContext;

use crate::gpu::blur::{self, Blur};
use crate::gpu::gfx::{bind_texture, saved_draw_fbo, Program, ScreenQuad, Target};
use crate::gpu::shadow::HEADROOM;
use crate::playback::mpv::RenderContext;

/// How far the glass's corners ease into its straight edges: the corner's
/// curve runs this many radii along each, where 1 is a quarter of an ellipse
/// and anything larger squarer and smoother; see `sdg_superbox` in
/// glass.frag.
///
/// Handed to the shader as `u_corner_softness` rather than written there,
/// because the interface draws the same corner on its highlights
/// (`interface::squircle`) and the two must not disagree.
pub const CORNER_SOFTNESS: f64 = 1.25;

/// Must match `MAX_PANELS` in glass.frag.
///
/// More than the interface can put up at once: at most the bar's pane and
/// the disc under the ring, the window's three buttons, the flash's two,
/// the tip, the seek preview, one panel, the pane under the ring in it, and a
/// pill. The shader stops at the real count, so the headroom costs nothing.
pub const MAX_PANELS: usize = 16;

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
            spread: 0.7,
            grain: 64.0,
            falloff: 4.0,
            falloff_softness: 0.5,
            max_taps: 256.0,
        }
    }
}

/// One glass panel, in physical pixels, top-left origin.
///
/// Filled from Slint's layout every frame, so the material can't drift from
/// its widget.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassPanel {
    pub rect: [f32; 4],
    pub radius: f32,
    pub tint_alpha: f32,
    /// How far the panel has faded in, 0..1, so the glass fades with it.
    pub opacity: f32,
    /// Laid on another pane, so it casts no shadow; see `GlassRect.nested`.
    pub nested: bool,
    /// Under the keyboard's ring, so it is held darker, for Lit words on the
    /// ring's wash, and its rim is lit at [`RING_RIM`]; see `GlassRect.ring`.
    pub ring: bool,
}

/// Look of the glass: what the settings panel's glass page adjusts, plus the
/// tint colour, which stays in code. The defaults aim at visible rather than
/// subtle.
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
    /// How dispersive the glass is, 0..1: a share of the most it can be with
    /// every visible wavelength still bending, where the band's red end meets
    /// an index of 1 (a dispersive power of about 2.16, whatever the index).
    /// The shader integrates the transmitted ray over wavelength with it, so
    /// this is the width of the prismatic fringe over a sharp backdrop, and
    /// wider over a blurred one (see [`fringe_scale`]).
    pub aberration: f32,
    /// How strongly the rim light shows: white added round each pane's
    /// outermost edge, where the surface has turned past 45 degrees. Zero
    /// turns it off.
    pub rim: f32,
    pub tint: [f32; 3],
    /// How far a panel pulls toward the tint colour, 0..1. Scales the
    /// per-panel opt-in rather than replacing it.
    pub tint_amount: f32,
    /// How much of a white backdrop the glass absorbs, 0..1; darker backdrops
    /// lose less, and 0 absorbs nothing. Keeps text legible over a bright
    /// frame (see glass.frag).
    pub absorb: f32,
    /// How dark the shadow each pane casts on the picture is, at its darkest,
    /// 0..1. Its size follows the pane's corner radius; see glass.frag.
    pub shadow: f32,
}

// --- the material's fixed properties ----------------------------------------
//
// What the material is, rather than how much of it you want: the rim is the
// corner, and the glass is flint. Changing either stops the panes reading as
// one material, so they are constants rather than settings, and the glass
// page has no row for them. Old settings naming `glass.bevel_ratio`,
// `glass.ior` or `glass.specular` are skipped like any unknown key.

/// Width of the domed rim, as a fraction of each panel's corner radius: how
/// far in from the edge the lensing and lighting reach.
///
/// Relative, so one value suits a 380px panel and a 60px pill (see `u_bevel`
/// in glass.frag). At 1.0 the rim runs the corner's full width, so a capsule
/// is rim the whole way through and a small pane has a small rolled edge.
pub const BEVEL: f32 = 1.0;

/// Index of refraction: 1.0 bends nothing, 1.33 water, 1.5 window glass,
/// 1.8 flint, 2.4 diamond.
///
/// Well past a window pane, so the rim bends hard enough to read at interface
/// scale. No higher, because at diamond the lensing pulls a dark
/// patch of the backdrop into a black oval across the top of the way in.
pub const IOR: f32 = 2.0;

/// The contrast the lowest step of text keeps with the glass behind it,
/// however bright the frame; see [`legible_ceiling`]. WCAG asks 4.5 of text
/// this size. The margin is for its thinnest strokes, which antialiasing
/// leaves short of their full alpha, so that the pixels on screen measure
/// past 4.5 too.
pub const LEGIBLE: f32 = 5.0;

/// The most light the glass may let through, as a luminance: that of the
/// brightest grey on which white at `text` alpha still reads at `ratio`,
/// standing on white at `wash` alpha laid over that grey. Zero where no grey
/// would do, which leaves the glass as it is.
pub fn legible_ceiling(text: f32, wash: f32, ratio: f32) -> f32 {
    let contrast = |grey: f32| {
        let under = wash + (1.0 - wash) * grey;
        let words = text + (1.0 - text) * under;
        (luminance(words) + 0.05) / (luminance(under) + 0.05)
    };
    if contrast(0.0) < ratio {
        return 0.0;
    }
    // The contrast falls steadily as the grey brightens.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if contrast(mid) >= ratio {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    luminance(lo)
}

/// The lower of two ceilings from [`legible_ceiling`], where zero sets none.
fn stricter(a: f32, b: f32) -> f32 {
    match (a > 0.0, b > 0.0) {
        (true, true) => a.min(b),
        (true, false) => a,
        _ => b,
    }
}

/// The rim light on the pane under the keyboard's ring, whatever the rim
/// slider says: its edge is what tells the ring from the pointer's wash, which
/// a wash alone can't do at 3:1 and keep its words at 4.5:1 too.
pub const RING_RIM: f32 = 1.0;

/// An encoded channel's share of light, as WCAG decodes it.
fn luminance(encoded: f32) -> f32 {
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// How much wider the dispersion fringe is drawn than the dispersive power
/// alone makes it, over a backdrop blurred by `sigma`: none wider over a
/// sharp one, and with the square root of the blur past that.
///
/// A fringe is seen against the edges it splits, which a blur widens, so it
/// widens too, but more slowly than they do: a heavy blur's edges run tens
/// of pixels, and a fringe kept as wide is a wash of colour, not an edge.
pub fn fringe_scale(sigma: f32) -> f32 {
    (sigma.max(0.0) + 1.0).sqrt()
}

impl Default for GlassParams {
    fn default() -> Self {
        Self {
            blur_sigma: 10.0,
            // The glass is as thick as the rim is wide; see `BEVEL`.
            refract: 1.0,
            aberration: 0.3,
            rim: 0.2,
            tint: [0.0, 0.0, 0.0],
            tint_amount: 0.0,
            absorb: 0.35,
            shadow: 0.2,
        }
    }
}

/// What one frame is drawn from.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    /// The window, in physical pixels.
    pub width: u32,
    pub height: u32,
    /// mpv has a new picture to render.
    pub new_frame: bool,
    /// A material parameter changed, so the passes must run again even with
    /// nothing else moving, or a slider would do nothing on a paused frame.
    pub params_dirty: bool,
    /// The video's rect within the window, normalised. From observed state
    /// rather than read from mpv here (see `state::OBSERVED`).
    pub rect: [f32; 4],
    /// The surfaces that want glass under them.
    pub panels: &'a [GlassPanel],
    /// Where the shadow pass will look, from `shadow::regions`, which the
    /// glass pass holds under white for it.
    pub shaded: &'a [[i32; 4]],
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
    /// Single-buffered, because only the glass pass reads it. Slint mustn't
    /// show it directly: a rounded-clip subtree caches into a layer, and a
    /// republished texture with the same id never invalidates it.
    blur_out: Target,
    /// Composite with the glass panels drawn into it. This is what Slint
    /// shows; the panels themselves contribute only their content.
    glassed: Target,
    prog_glass: Program,
    /// The passes that make `blur_out`, and the over-pass's blur.
    blur_chain: Blur,
    /// Glass over the interface rather than under it (see
    /// [`Pipeline::glass_over`]): the region as Slint left it, blurred, and
    /// glassed.
    over_base: Target,
    over_blur: Target,
    over_out: Target,
    panels: Vec<GlassPanel>,
    /// The frame's `shaded`, as last drawn.
    shaded: Vec<[i32; 4]>,
    pub glass: GlassParams,

    /// (mpv, border+blur, glass) from the last frame, when `DBM_GPU_TIME`
    /// is on. Zero otherwise.
    pub timings: (
        std::time::Duration,
        std::time::Duration,
        std::time::Duration,
    ),
    /// (update_rect, border, blur), for the same frame as `timings`.
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
    /// The alpha of the lowest step of text, `Paint.body`, which the glass
    /// never grows too bright for; see [`legible_ceiling`].
    pub text_floor: f32,
    /// The alpha of the ring's wash, `Paint.wash-ring`, which the pane under
    /// the ring is held dark enough for, with Lit words on it.
    pub ring_wash: f32,
    /// Whether it is held to that at all: off, the glass is only what its
    /// sliders make it.
    pub legible: bool,

    prog_backdrop: Program,
    /// A frame of the last film left unfinished (see [`Pipeline::set_backdrop`]).
    backdrop: Option<Backdrop>,
    /// Whether that frame stands in for mpv's picture, which is while no file
    /// is loaded.
    pub show_backdrop: bool,
    /// Whether the backdrop fades up or is simply there: the Animations
    /// setting, for the one movement the pipeline draws itself.
    pub animate: bool,
}

/// How long the backdrop takes to come up. Longer than a surface's 120ms,
/// because it is the window's light changing, and a whole picture arriving
/// that fast is a flash.
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
        let vert = include_str!("../../shaders/fullscreen.vert");
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            prog_extend: Program::new(gl, vert, include_str!("../../shaders/border_extend.frag"))
                .map_err(|e| format!("border_extend: {e}"))?,
            prog_spread: Program::new(gl, vert, include_str!("../../shaders/border_spread.frag"))
                .map_err(|e| format!("border_spread: {e}"))?,
            video: Target::new(),
            extended: Target::new(),
            composite: Target::new(),
            blur_out: Target::new(),
            glassed: Target::new(),
            prog_glass: Program::new(gl, vert, include_str!("../../shaders/glass.frag"))
                .map_err(|e| format!("glass: {e}"))?,
            blur_chain: Blur::new(gl, vert)?,
            over_base: Target::new(),
            over_blur: Target::new(),
            over_out: Target::new(),
            panels: Vec::new(),
            shaded: Vec::new(),
            glass: GlassParams::default(),
            timings: Default::default(),
            split: Default::default(),
            gpu_time: std::env::var_os("DBM_GPU_TIME").is_some(),
            params: BorderParams::default(),
            rect: [0.0, 0.0, 1.0, 1.0],
            border_enabled: true,
            glass_enabled: true,
            text_floor: 0.0,
            ring_wash: 0.0,
            legible: true,
            prog_backdrop: Program::new(gl, vert, include_str!("../../shaders/backdrop.frag"))
                .map_err(|e| format!("backdrop: {e}"))?,
            backdrop: None,
            show_backdrop: false,
            animate: true,
        })
    }

    /// Take delivery of the frame the empty window stands in front of.
    ///
    /// Uploaded once and kept: it is a few hundred kilobytes, and the window
    /// only needs it until a film arrives.
    pub fn set_backdrop(&mut self, gl: &glow::Context, still: &crate::library::preview::Still) {
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

    /// Whether the backdrop still has frames of its arrival to draw, so the
    /// driver keeps asking for them, since with no film nothing else does.
    pub fn backdrop_arriving(&self) -> bool {
        self.show_backdrop && self.backdrop.as_ref().is_some_and(|b| !b.settled)
    }

    fn draw_backdrop(&mut self, gl: &glow::Context, w: u32, h: u32) {
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

    /// The video rect the border was last drawn from. For diagnostics that
    /// need to compare it against what mpv currently reports.
    pub fn border_rect(&self) -> [f32; 4] {
        self.rect
    }

    /// Video plus ambient border, before the glass pass. Diagnostics.
    pub fn composite(&self) -> &Target {
        &self.composite
    }

    /// The blurred composite the glass panels refract.
    pub fn blur(&self) -> &Target {
        &self.blur_out
    }

    /// Sample the centre of every stage of the blur, to find where a black
    /// frame enters. Diagnostics only; stalls the GL pipeline.
    pub fn blur_chain_debug(&self, gl: &glow::Context) -> String {
        let probe = |t: &Target| {
            let (w, h) = t.size();
            let px = t.sample_grid(gl, &[(w / 2, h / 2)]);
            format!("{w}x{h}{:?}", px.first().copied().unwrap_or([0; 4]))
        };
        format!(
            "composite {} | horizontal {} | blur_out {}",
            probe(&self.composite),
            probe(self.blur_chain.across()),
            probe(&self.blur_out),
        )
    }

    /// The raw mpv output, before the border pass. Diagnostics only.
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
        // Every target here is the window's own size; the blur's levels size
        // themselves.
        let ok = [
            &mut self.video,
            &mut self.extended,
            &mut self.composite,
            &mut self.glassed,
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
        frame: &Frame,
    ) -> Result<bool, String> {
        let Frame {
            width: w,
            height: h,
            new_frame,
            params_dirty,
            rect,
            panels,
            shaded,
        } = *frame;
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

        // Also on resize, not only on a new frame: the target was just
        // cleared, so a paused player would show black and the border would
        // grow out of it. mpv happily re-renders the frame it holds.
        //
        // With nothing loaded and a backdrop on hand, that stands in for the
        // frame instead: drawn when it arrives, while it fades up, and after a
        // resize. mpv's `new_frame` is ignored meanwhile, since with no file it
        // would only render black over it.
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
        // reasons, a resize drag above all, and re-running a 256-tap border
        // pass on each of those was most of the resize lag.
        let t1 = mark(gl);
        let panels_changed = self.panels != panels || self.shaded != shaded;
        // The border is drawn from `rect`, which can change on its own: mpv
        // reports `osd-dimensions` asynchronously, so the rect for a new size
        // often arrives a frame or two after the resize. Paused, no next frame
        // would redraw the border, which would keep the old geometry.
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

        // A panel can move without a new video frame, and only the glass pass
        // depends on that, so the border and blur above are skipped then.
        self.panels.clear();
        self.panels.extend_from_slice(panels);
        self.shaded.clear();
        self.shaded.extend_from_slice(shaded);
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

    /// The composite, blurred by the Blur slider, into `blur_out`.
    fn run_blur(&mut self, gl: &glow::Context) {
        let sigma = self.glass.blur_sigma;
        self.blur_chain
            .run(gl, &self.quad, &self.composite, &self.blur_out, sigma);
    }

    /// Composite the glass panels over the finished frame.
    fn run_glass(&mut self, gl: &glow::Context, w: u32, h: u32) {
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
    #[allow(clippy::too_many_arguments)]
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
            .set_vec2(gl, "u_size", size.0 as f32, size.1 as f32);

        let (base_scale, blur_scale) = (base.scale(), blur.scale());
        self.prog_glass
            .set_vec2(gl, "u_base_scale", base_scale.0, base_scale.1);
        self.prog_glass
            .set_vec2(gl, "u_blur_scale", blur_scale.0, blur_scale.1);

        // Uniform arrays are uploaded whole; unused slots are simply never
        // read, since the shader loop stops at `u_panel_count`.
        let (ceiling, ring_ceiling) = if self.legible {
            (
                legible_ceiling(self.text_floor, 0.0, LEGIBLE),
                legible_ceiling(1.0, self.ring_wash, LEGIBLE),
            )
        } else {
            (0.0, 0.0)
        };
        let count = panels.len().min(MAX_PANELS);
        let mut rects = [0.0f32; MAX_PANELS * 4];
        let mut styles = [0.0f32; MAX_PANELS * 4];
        let mut lights = [0.0f32; MAX_PANELS * 4];
        for (i, panel) in panels.iter().take(count).enumerate() {
            rects[i * 4..i * 4 + 4].copy_from_slice(&panel.rect);
            styles[i * 4] = panel.radius;
            styles[i * 4 + 1] = panel.tint_alpha;
            styles[i * 4 + 2] = panel.opacity;
            styles[i * 4 + 3] = if panel.nested { 1.0 } else { 0.0 };
            lights[i * 4] = if panel.ring {
                stricter(ceiling, ring_ceiling)
            } else {
                ceiling
            };
            lights[i * 4 + 1] = if panel.ring {
                g.rim.max(RING_RIM)
            } else {
                g.rim
            };
        }
        self.prog_glass.set_i32(gl, "u_panel_count", count as i32);
        self.prog_glass
            .set_vec4_array(gl, "u_panel_rect[0]", &rects);
        self.prog_glass
            .set_vec4_array(gl, "u_panel_style[0]", &styles);
        self.prog_glass
            .set_vec4_array(gl, "u_panel_light[0]", &lights);

        let held = shaded.len().min(MAX_PANELS);
        let mut regions = [0.0f32; MAX_PANELS * 4];
        for (i, region) in shaded.iter().take(held).enumerate() {
            regions[i * 4..i * 4 + 4].copy_from_slice(&region.map(|v| v as f32));
        }
        self.prog_glass.set_i32(gl, "u_shaded_count", held as i32);
        self.prog_glass.set_vec4_array(gl, "u_shaded[0]", &regions);
        self.prog_glass.set_f32(gl, "u_headroom", HEADROOM);

        self.prog_glass.set_f32(gl, "u_bevel", BEVEL);
        self.prog_glass
            .set_f32(gl, "u_corner_softness", CORNER_SOFTNESS as f32);
        self.prog_glass.set_f32(gl, "u_refract", g.refract);
        self.prog_glass.set_f32(gl, "u_ior", IOR);
        self.prog_glass.set_f32(gl, "u_aberration", g.aberration);
        self.prog_glass
            .set_f32(gl, "u_fringe_scale", fringe_scale(g.blur_sigma));
        self.prog_glass.set_vec3(gl, "u_tint", g.tint);
        self.prog_glass.set_f32(gl, "u_tint_amount", g.tint_amount);
        self.prog_glass.set_f32(gl, "u_absorb", g.absorb);
        self.prog_glass.set_f32(gl, "u_shadow", g.shadow);

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
        let [x0, y0, x1, y1] = pane.rect;
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
            rect: [
                x0 - left as f32,
                y0 - top as f32,
                x1 - left as f32,
                y1 - top as f32,
            ],
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
            &mut self.blur_out,
            &mut self.over_base,
            &mut self.over_blur,
            &mut self.over_out,
        ] {
            target.release(gl);
        }
        self.blur_chain.release(gl);
        self.quad.release(gl);
        if let Some(b) = self.backdrop.take() {
            unsafe { gl.delete_texture(b.tex) };
        }
        for p in [
            &self.prog_extend,
            &self.prog_spread,
            &self.prog_glass,
            &self.prog_backdrop,
        ] {
            p.release(gl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fringe_scale, legible_ceiling, luminance, stricter, LEGIBLE};

    #[test]
    fn the_fringe_is_the_glass_own_when_sharp_and_widens_with_the_root_of_the_blur() {
        assert!((fringe_scale(0.0) - 1.0).abs() < 1e-6);
        // Four times the blur, twice the fringe.
        let wide = fringe_scale(99.0) / fringe_scale(24.0);
        assert!((wide - 2.0).abs() < 1e-6, "{wide}");
        // Never narrower than the glass's own.
        assert!((fringe_scale(-1.0) - 1.0).abs() < 1e-6);
    }

    /// The brightest 8-bit grey under a ceiling.
    fn grey_at(ceiling: f32) -> f32 {
        (0..=255)
            .map(|v| v as f32 / 255.0)
            .take_while(|g| luminance(*g) <= ceiling)
            .last()
            .unwrap()
    }

    #[test]
    fn body_text_keeps_its_contrast_on_glass_at_the_ceiling() {
        let body = 224.0 / 255.0;
        let ceiling = legible_ceiling(body, 0.0, LEGIBLE);
        // A mid grey, nowhere near white.
        assert!((0.08..0.16).contains(&ceiling), "{ceiling}");
        let grey = grey_at(ceiling);
        let text = body + (1.0 - body) * grey;
        let contrast = (luminance(text) + 0.05) / (luminance(grey) + 0.05);
        assert!(contrast >= LEGIBLE, "{contrast}");
    }

    #[test]
    fn lit_text_keeps_its_contrast_on_the_ring_wash_at_its_ceiling() {
        let wash = 84.0 / 255.0;
        let ceiling = legible_ceiling(1.0, wash, LEGIBLE);
        // Darker than the glass elsewhere has to be, since the wash lightens it.
        assert!(
            ceiling < legible_ceiling(224.0 / 255.0, 0.0, LEGIBLE),
            "{ceiling}"
        );
        let under = wash + (1.0 - wash) * grey_at(ceiling);
        let contrast = 1.05 / (luminance(under) + 0.05);
        assert!(contrast >= LEGIBLE, "{contrast}");
    }

    #[test]
    fn brighter_text_lets_the_glass_be_brighter() {
        assert!(legible_ceiling(1.0, 0.0, LEGIBLE) > legible_ceiling(224.0 / 255.0, 0.0, LEGIBLE));
    }

    #[test]
    fn text_that_could_never_read_leaves_the_glass_alone() {
        assert_eq!(legible_ceiling(0.0, 0.0, LEGIBLE), 0.0);
        assert_eq!(stricter(0.0, 0.0), 0.0);
        assert_eq!(stricter(0.05, 0.0), 0.05);
        assert_eq!(stricter(0.05, 0.02), 0.02);
    }
}
