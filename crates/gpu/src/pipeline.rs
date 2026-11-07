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

use crate::blur::Blur;
use crate::gfx::{Program, ScreenQuad, Target, program, program_with, saved_draw_fbo};
use crate::rect::{Normalised, Rect, Region};
use crate::size::Size;

mod backdrop;
mod border;
mod glass;
mod params;

use backdrop::Picture;
pub use params::{
    BEVEL, BorderParams, CORNER_SOFTNESS, GlassOver, GlassPanel, GlassParams, IOR, LEGIBLE,
    MAX_PANELS, OPEN_MILK, RING_MILK, RING_RIM, fringe_scale, legible_ceiling,
};

/// Why the frame could not be drawn.
#[derive(Debug)]
pub enum Error {
    /// The caller's film callback failed, with its own error. The pipeline
    /// does not know what draws the film, so it keeps the cause rather than
    /// flattening it to a sentence.
    Film(Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    pub fn film(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Film(Box::new(source))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Film(source) => write!(f, "could not draw the film: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Film(source) => Some(&**source),
        }
    }
}

/// Whether the next frame's allocation is to be refused, and how many have
/// been. Only when the harness is built in.
#[cfg(feature = "harness")]
static REFUSE_NEXT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "harness")]
static REFUSED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Ask the next frame's allocation to fail, as a driver with no room left
/// would, so the harness can watch what the player does about it.
///
/// Only the next one; the frame after allocates normally.
#[cfg(feature = "harness")]
pub fn refuse_next_allocation() {
    REFUSE_NEXT.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// How many allocations have been refused, so an exercise can tell that one
/// was rather than only that it was asked for.
#[cfg(feature = "harness")]
pub fn refusals() -> u32 {
    REFUSED.load(std::sync::atomic::Ordering::SeqCst)
}

/// How the backdrop stands in for mpv's picture this frame.
///
/// Three states rather than a "shown" and an "over the film" beside it,
/// which left a fourth: over a film that is not being shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    /// Not at all: mpv's own frame is drawn.
    Off,
    /// Alone, with nothing playing.
    Alone,
    /// Over the frame mpv still holds, at the end of a file.
    OverFilm,
}

/// How long the last frame's three stretches took.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// mpv rendering the frame.
    pub mpv: std::time::Duration,
    /// The border and blur.
    pub border: std::time::Duration,
    /// The glass.
    pub glass: std::time::Duration,
}

/// How long the last border frame's three parts took.
#[derive(Clone, Copy, Debug, Default)]
pub struct Split {
    /// Reading the video rectangle and the frame's other inputs.
    pub update_rect: std::time::Duration,
    /// The border pass.
    pub border: std::time::Duration,
    /// The blur.
    pub blur: std::time::Duration,
}

/// What allocating the frame's targets came to.
///
/// Two states rather than an `ok` and a `resized` beside it: a resize that
/// could not be given its room has no meaningful answer to "did anything
/// move", since the next frame has to run the whole chain whatever it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sizes {
    /// Every target the pass needs exists. `resized` says whether any of
    /// them had to move to get there, so its contents must be drawn again.
    /// Not just the composite: a failed grow may have moved some targets and
    /// not others.
    Allocated { resized: bool },
    /// One could not be given the room it asked for. The rest were still
    /// tried, so the next frame starts from one size rather than a mix.
    Refused,
}

/// What one frame is drawn from.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    /// The window, in physical pixels.
    pub size: Size,
    /// mpv has a new picture to render.
    pub new_frame: bool,
    /// A material parameter changed, so the passes must run again even with
    /// nothing else moving, or a slider would do nothing on a paused frame.
    pub params_dirty: bool,
    /// The video's rect within the window, normalised. From observed state
    /// rather than read from mpv here (see `state::OBSERVED`).
    pub rect: Rect<Normalised>,
    /// The surfaces that want glass under them.
    pub panels: &'a [GlassPanel],
    /// Where the shadow pass will look, from `shadow::regions`, which the
    /// glass pass holds under white for it.
    pub shaded: &'a [Region],
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
    shaded: Vec<Region>,
    pub glass: GlassParams,

    /// The last frame's three stretches, when `DBM_GPU_TIME` is on. Zero
    /// otherwise.
    pub timings: Timings,
    /// The last border frame's three parts, for the same frame.
    pub split: Split,
    gpu_time: bool,
    pub params: BorderParams,
    /// Video rect within the window, normalised (x0, y0, x1, y1).
    rect: Rect<Normalised>,
    /// `border` off means skip both border passes and show the video alone.
    pub border_enabled: bool,
    /// Whether mpv draws over a clear surround (see `Mpv::clear_surround`),
    /// so what it puts outside the picture can go over the border.
    pub clear_surround: bool,
    /// Glass off is expressed as zero panels rather than a shader branch:
    /// the pass already short-circuits on an empty list, so it costs one
    /// full-screen copy and no special case.
    pub glass_enabled: bool,
    /// The alpha of the lowest step of text, `Paint.body`, which the glass
    /// never grows too bright for; see [`legible_ceiling`].
    pub text_floor: f32,
    /// Whether it is held to that at all: off, the glass is only what its
    /// sliders make it.
    pub legible: bool,

    prog_backdrop: Program,
    /// A frame of the last film left unfinished, or at the end of a file of
    /// what comes next (see [`Pipeline::set_backdrop`]).
    backdrop: Option<Picture>,
    /// How that frame stands in for mpv's picture, if it does at all (see
    /// [`Pipeline::show_backdrop`]).
    backdrop_shown: Backdrop,
    /// The backdrop has just gone, so mpv's frame is drawn again whether or
    /// not it has a new one.
    film_wanted: bool,
    /// A frame failed after its targets had been touched, so the next one
    /// must run the whole chain even if every size now looks current.
    needs_redraw: bool,
    /// Whether the backdrop fades up or is simply there: the Animations
    /// setting, for the one movement the pipeline draws itself.
    pub animate: bool,
}

impl Pipeline {
    pub fn new(gl: &glow::Context) -> Result<Self, crate::gfx::Error> {
        Ok(Self {
            quad: ScreenQuad::new(gl)?,
            prog_extend: program!(gl, "border_extend")?,
            prog_spread: program_with!(gl, "border_spread", "dither")?,
            video: Target::default(),
            extended: Target::default(),
            composite: Target::default(),
            blur_out: Target::default(),
            glassed: Target::default(),
            prog_glass: program_with!(gl, "glass", "dither")?,
            blur_chain: Blur::new(gl)?,
            over_base: Target::default(),
            over_blur: Target::default(),
            over_out: Target::default(),
            panels: Vec::new(),
            shaded: Vec::new(),
            glass: GlassParams::default(),
            timings: Default::default(),
            split: Default::default(),
            gpu_time: std::env::var_os("DBM_GPU_TIME").is_some(),
            params: BorderParams::default(),
            rect: Rect::FULL,
            border_enabled: true,
            clear_surround: false,
            glass_enabled: true,
            text_floor: 0.0,
            legible: true,
            prog_backdrop: program!(gl, "backdrop")?,
            backdrop: None,
            backdrop_shown: Backdrop::Off,
            film_wanted: false,
            needs_redraw: false,
            animate: true,
        })
    }

    /// The film's picture, into the video target. mpv re-renders the frame
    /// it holds when asked, so a paused film is drawn again as well as a
    /// playing one.
    fn render_film(
        &self,
        gl: &glow::Context,
        film: &impl Fn(u32, i32, i32) -> Result<(), Error>,
        size: Size,
    ) -> Result<bool, Error> {
        let Some(fbo) = self.video.fbo() else {
            return Ok(false);
        };
        let saved = saved_draw_fbo(gl);
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            let mut viewport = [0i32; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            let res = film(fbo.0.get(), size.width as i32, size.height as i32);
            gl.bind_framebuffer(glow::FRAMEBUFFER, saved);
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            res?;
        }
        Ok(true)
    }

    /// Draw the film, remembering a failure that leaves the targets needing
    /// the whole chain again.
    fn film_frame(
        &mut self,
        gl: &glow::Context,
        film: &impl Fn(u32, i32, i32) -> Result<(), Error>,
        size: Size,
    ) -> Result<bool, Error> {
        match self.render_film(gl, film, size) {
            Ok(true) => Ok(true),
            Ok(false) => {
                self.needs_redraw = true;
                Ok(false)
            }
            Err(e) => {
                self.needs_redraw = true;
                Err(e)
            }
        }
    }

    /// The image Slint draws: video, ambient border and glass panels, all
    /// composited. Slint draws only panel *content* over this.
    pub fn output(&self) -> &Target {
        &self.glassed
    }

    /// The video rect the border was last drawn from. For diagnostics that
    /// need to compare it against what mpv currently reports.
    pub fn border_rect(&self) -> Rect<Normalised> {
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
            let size = t.size();
            let px = t.sample_grid(gl, &[(size.width / 2, size.height / 2)]);
            format!(
                "{}x{}{:?}",
                size.width,
                size.height,
                px.first().copied().unwrap_or([0; 4])
            )
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
    /// A resize discards every target's contents, so the caller has to
    /// re-run the chain even without a new frame from mpv. "Resized" means
    /// any target's active size differs, not one target's: a failed grow
    /// may have moved some of them and not others.
    fn ensure_sizes(&mut self, gl: &glow::Context, size: Size) -> Sizes {
        // The harness's one way to reach the refusal a driver would give on
        // a frame it has no room for, which no window can be asked to cause.
        #[cfg(feature = "harness")]
        if REFUSE_NEXT.swap(false, std::sync::atomic::Ordering::SeqCst) {
            REFUSED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Sizes::Refused;
        }
        let resized = [
            &self.video,
            &self.extended,
            &self.composite,
            &self.glassed,
            &self.blur_out,
        ]
        .into_iter()
        .any(|target| target.size() != size);
        // Every target here is the window's own size; the blur's levels size
        // themselves. All are tried, so a failure in one doesn't leave the
        // rest on an old size for the next frame to trip over.
        let mut ok = true;
        for target in [
            &mut self.video,
            &mut self.extended,
            &mut self.composite,
            &mut self.glassed,
            &mut self.blur_out,
        ] {
            if !target.ensure_size(gl, size) {
                ok = false;
            }
        }
        if ok {
            Sizes::Allocated { resized }
        } else {
            Sizes::Refused
        }
    }

    /// Pull the film's current frame and run the whole chain.
    ///
    /// `film` draws the picture into the framebuffer it is handed, at the
    /// width and height given: mpv's render context, in the player.
    ///
    /// Returns false if nothing could be rendered (a zero-sized window, or a
    /// failed allocation). `output()` then holds either the previous target
    /// or a cleared replacement, never a deleted one.
    pub fn render(
        &mut self,
        gl: &glow::Context,
        film: impl Fn(u32, i32, i32) -> Result<(), Error>,
        frame: &Frame,
    ) -> Result<bool, Error> {
        let Frame {
            size,
            new_frame,
            params_dirty,
            rect,
            panels,
            shaded,
        } = *frame;
        let resized = match self.ensure_sizes(gl, size) {
            Sizes::Allocated { resized } => resized,
            Sizes::Refused => {
                self.needs_redraw = true;
                return Ok(false);
            }
        };
        let resized = resized || std::mem::take(&mut self.needs_redraw);
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
        // With a backdrop shown and on hand, that stands in for the frame
        // instead: drawn when it arrives, while it fades up, and after a
        // resize. mpv's `new_frame` is ignored meanwhile, since with no file it
        // would only render black over it, and at a file's end it has nothing
        // new. Over a film the held frame is drawn under it each time, for it
        // to arrive over.
        let mut drew_backdrop = false;
        let mut drew_film_again = false;
        if self.backdrop_shown != Backdrop::Off && self.backdrop.is_some() {
            if resized || self.backdrop_arriving() {
                if self.backdrop_shown == Backdrop::OverFilm && !self.film_frame(gl, &film, size)? {
                    return Ok(false);
                }
                self.draw_backdrop(gl, size);
                drew_backdrop = true;
            }
        } else if new_frame || resized || self.film_wanted {
            if !self.film_frame(gl, &film, size)? {
                return Ok(false);
            }
            drew_film_again = std::mem::take(&mut self.film_wanted);
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
        let full = new_frame
            || resized
            || params_dirty
            || rect_changed
            || drew_backdrop
            || drew_film_again;
        if !full && !panels_changed {
            return Ok(true);
        }

        if full {
            self.rect = rect;
            let ta = mark(gl);
            self.run_border(gl, size);
            let tb = mark(gl);
            self.run_blur(gl);
            let tc = mark(gl);
            if gpu_time {
                self.split = Split {
                    update_rect: ta.duration_since(t1),
                    border: tb.duration_since(ta),
                    blur: tc.duration_since(tb),
                };
            }
        }
        let t2 = mark(gl);

        // A panel can move without a new video frame, and only the glass pass
        // depends on that, so the border and blur above are skipped then.
        self.panels.clear();
        self.panels.extend_from_slice(panels);
        self.shaded.clear();
        self.shaded.extend_from_slice(shaded);
        self.run_glass(gl, size);
        let t3 = mark(gl);
        if gpu_time {
            self.timings = Timings {
                mpv: t1.duration_since(t0),
                border: t2.duration_since(t1),
                glass: t3.duration_since(t2),
            };
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

    pub fn release(mut self, gl: &glow::Context) {
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
            self.prog_extend,
            self.prog_spread,
            self.prog_glass,
            self.prog_backdrop,
        ] {
            p.release(gl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Refused;

    impl std::fmt::Display for Refused {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("refused")
        }
    }

    impl std::error::Error for Refused {}

    #[test]
    fn a_film_error_keeps_the_cause_it_was_handed() {
        let error = Error::film(Refused);
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.to_string().contains("refused"));
    }
}
