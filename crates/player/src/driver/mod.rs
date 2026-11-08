//! Driving the GPU pipeline from Slint's rendering notifier.
//!
//! This is the only place that touches GL, and it runs entirely inside
//! Slint's own context during `RenderingSetup` / `BeforeRendering` /
//! `AfterRendering`. What is playing, and everything that follows from it,
//! is the [`Player`]'s; the driver runs it at the start of each frame and
//! draws from what it holds.
//!
//! The per-frame order matters and is not arbitrary — see [`Driver::frame`]:
//!
//!   1. run the player, so the frame draws from what mpv and the worker just
//!      said (see `player`)
//!   2. read panel geometry back out of the UI
//!   3. run the pipeline, which needs (2) to know where the glass goes, and
//!      publish the finished texture — see `draw`
//!
//! Steps 1 and 2 straddle the UI deliberately: the panel rectangles depend on
//! layout that depends on the state just pushed, and reading them in the same
//! frame is what stops the glass lagging a frame behind its widget.

mod draw;

use std::cell::RefCell;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Mutex;

use slint::{ComponentHandle, GraphicsAPI, RenderingState};

use crate::app::App;
use crate::interface::{fades, glass};
use crate::player::Player;
use crate::{MainWindow, diagnostics};
use dbm_gpu::Overlays;
use dbm_gpu::pipeline::{GlassPanel, Pipeline};
use dbm_gpu::rect::Region;
use dbm_gpu::size::Size;
use dbm_lang::{fill, marked, tr};

/// Identity of what a Slint image property points at.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Published {
    id: NonZeroU32,
    /// The active image, and the texture allotted for it.
    used: Size,
    alloc: Size,
}

/// Everything that only exists once the GL context does.
struct Gpu {
    ctx: dbm_mpv::RenderContext,
    gl: glow::Context,
    pipeline: Pipeline,
    overlays: Overlays,
    published: Option<Published>,
}

/// Whether this run can show video at all.
///
/// One value rather than a context and a failure beside each other, so that
/// the two can never both be on hand: a run that has said why it cannot draw
/// has nothing left to draw with, and the compiler says so.
///
/// The file the command line named waits in the first case rather than
/// beside these, since it is only ever waiting for the context: opening it
/// earlier starts it against a video output that can never present one.
enum Video {
    /// Slint has not asked for a context yet.
    Waiting { file: Option<std::path::PathBuf> },
    /// The context, and everything drawn through it.
    ///
    /// Boxed because a GL context is several kilobytes and the other two
    /// states are a pointer and a path: without it every `Video` would be
    /// that wide, for a value that is made once and read every frame.
    Live(Box<Gpu>),
    /// No context this run, and what to say instead of a film.
    Failed(Failure),
}

/// The buffers each frame fills, kept between frames so the frame path
/// does not allocate.
#[derive(Default)]
struct Scratch {
    panels: Vec<GlassPanel>,
    /// Where the shadow under the interface is cast this frame, which the
    /// glass pass holds under white for it (see `shadow::regions`).
    shaded: Vec<Region>,
    /// The ends of scrolling lists to soften this frame. Reused, like the
    /// panels, to keep the frame path from allocating.
    fades: Vec<dbm_gpu::fade::Strip>,
    /// The angles pointing lists back to their current rows, reused the same way.
    ways: Vec<dbm_gpu::way::Way>,
}

pub struct Driver {
    ui: slint::Weak<MainWindow>,
    app: App,
    /// The context, or why there is none — see [`Video`].
    video: Video,
    player: Rc<RefCell<Player>>,
    /// `lang::changes` as of the last frame, so a failure is said again in a
    /// language just chosen.
    language_seen: usize,
    diag: diagnostics::Probe,
    /// A picture of a finished frame, when one is asked for.
    capture: diagnostics::Capture,
    scratch: Scratch,
}

/// The sentence every `Failure` is said under.
const NO_VIDEO: &str = marked("The player cannot show video on this computer.");

/// What `Driver::fatal` puts on the way in, kept in English so it can be said
/// again in a language chosen after it.
struct Failure {
    /// What went wrong, with a blank for `cause`: the driver's or mpv's own
    /// words, which are shown as they came.
    detail: &'static str,
    cause: String,
    /// What usually puts it right.
    note: &'static str,
}

impl Driver {
    pub fn new(
        ui: slint::Weak<MainWindow>,
        app: App,
        file: Option<std::path::PathBuf>,
        player: Rc<RefCell<Player>>,
    ) -> Self {
        let capture = diagnostics::Capture::new(ui.clone());
        Self {
            ui,
            app,
            video: Video::Waiting { file },
            player,
            language_seen: 0,
            diag: diagnostics::Probe::new(),
            capture,
            scratch: Scratch::default(),
        }
    }

    pub fn on(&mut self, state: RenderingState, api: &GraphicsAPI) {
        match state {
            RenderingState::RenderingSetup => self.setup(api),
            RenderingState::BeforeRendering => self.frame(),
            RenderingState::AfterRendering => self.after(),
            RenderingState::RenderingTeardown => self.teardown(),
            _ => {}
        }
    }

    /// What is drawn over the frame Slint has just drawn, and the swap that
    /// tells mpv it reached the screen.
    ///
    /// The context is taken out for the length of this, as in `frame`, so
    /// each pass can borrow the rest of the driver while it runs.
    fn after(&mut self) {
        self.diag.drawn();
        let mut video = std::mem::replace(&mut self.video, Video::Waiting { file: None });
        if let Video::Live(gpu) = &mut video {
            // Tells mpv the frame reached the screen, which is what keeps
            // its frame timing honest. Slint keeps the context current
            // through its notifier.
            unsafe { gpu.ctx.report_swap() };
            // Here rather than in `frame`, because here the interface is
            // already drawn over the video and the picture is the whole
            // thing. Does nothing unless asked.
            if let Some(ui) = self.ui.upgrade() {
                let window = ui.window().size();
                let size = Size::new(window.width, window.height);
                // Before the picture is taken, so all of it is in it. First
                // the soft ends of any list with rows past them, on the
                // frame Slint has just drawn...
                fades::collect(&ui, &mut self.scratch.fades, &mut self.scratch.ways);
                if let Some(fader) = gpu.overlays.fader.as_mut() {
                    fader.run(&gpu.gl, gpu.pipeline.output(), size, &self.scratch.fades);
                }
                // ...then the angle back to a list's current row, whole over
                // the edge it sits in, on its glass disc if it has one.
                // Nothing Slint draws moves with its circle, so this asks for
                // the frames it eases in over.
                if dbm_gpu::way::GLASS {
                    let panes: Vec<_> = self
                        .scratch
                        .ways
                        .iter()
                        .filter(|w| w.on > 0.004)
                        .map(dbm_gpu::way::Way::pane)
                        .collect();
                    gpu.pipeline.glass_over(&gpu.gl, size, &panes);
                }
                if let Some(way) = gpu.overlays.way.as_mut() {
                    way.run(&gpu.gl, size, &self.scratch.ways);
                    if self.scratch.ways.iter().any(|w| w.lit > 0.0 && w.lit < 1.0) {
                        ui.window().request_redraw();
                    }
                }
                // ...then the shadow, cast by what is left, so a row cut off
                // at a list's edge casts nothing past it...
                if let Some(shadow) = gpu.overlays.shadow.as_mut() {
                    shadow.run(
                        &gpu.gl,
                        gpu.pipeline.output(),
                        size,
                        &self.scratch.shaded,
                        ui.window().scale_factor(),
                    );
                }
                // ...and last the window's corners, cut from all of it.
                if let Some(corners) = gpu.overlays.corners.as_mut() {
                    // At the system's size, not the interface's, since they
                    // are the desktop's corners.
                    let radius = crate::platform::window::corner_radius(ui.window());
                    corners.run(&gpu.gl, size, radius * self.app.scale.system(&ui));
                }
                self.capture.maybe(&gpu.gl, size.width, size.height);
                // Its fade is drawn here rather than by Slint, so nothing
                // else knows to ask for the frames.
                if gpu.pipeline.backdrop_arriving() {
                    ui.window().request_redraw();
                }
            }
        }
        self.video = video;
        self.keep_the_interface_moving();
    }

    /// Give back everything the context made, and go back to waiting for one.
    fn teardown(&mut self) {
        // The image property is left alone here: this callback runs while
        // Slint holds the window's adapter borrow, and setting a property
        // that asks for a redraw would panic inside it. The next
        // BeforeRendering publishes or clears it before Slint draws anything.
        let video = std::mem::replace(&mut self.video, Video::Waiting { file: None });
        match video {
            Video::Live(gpu) => {
                gpu.pipeline.release(&gpu.gl);
                gpu.overlays.release(&gpu.gl);
            }
            // A failure is not the context's to take with it: the window
            // would go back to being silently black.
            other => self.video = other,
        }
    }

    /// Say on screen that there will be no video this run.
    ///
    /// Every caller leaves the player without a render context: no film, no
    /// glass, and so no chrome, since the glass pass draws every surface. A
    /// packaged build has no console, so without this the window would just
    /// sit black. The way in says it, and drops its rows, which could only
    /// open something that plays invisibly.
    ///
    /// The note on what usually puts it right differs: a context the driver
    /// wouldn't give, or mpv couldn't use, is the driver's, but refused
    /// shaders may be a fault in the player's own GLSL.
    fn fatal(&mut self, failure: Failure) {
        log::error!("{} — {}", NO_VIDEO, fill(failure.detail, &[&failure.cause]));
        self.video = Video::Failed(failure);
        if let Some(ui) = self.ui.upgrade() {
            // Nothing will render through this context, so what the last one
            // left must not stay in the image property.
            draw::clear_image(&ui);
            if let Video::Failed(failure) = &self.video {
                self.say_failure(&ui, failure);
            }
        }
    }

    /// Put the failure on the way in, in the language being spoken.
    fn say_failure(&self, ui: &MainWindow, failure: &Failure) {
        // Nothing renders after a failure, so the ring has only its wash.
        ui.global::<crate::Paint>().set_glass(false);
        ui.set_fatal(tr(NO_VIDEO).into());
        ui.set_fatal_detail(fill(tr(failure.detail), &[&failure.cause]).into());
        ui.set_fatal_note(tr(failure.note).into());
    }

    fn setup(&mut self, api: &GraphicsAPI) {
        // Taken out here, and opened only at the end: with `vo=libmpv` there
        // is no video output until this context exists, so a file loaded
        // before it starts against one that can never present a frame.
        let file = match std::mem::replace(&mut self.video, Video::Waiting { file: None }) {
            Video::Waiting { file } => file,
            // Slint asks once per window. A second ask over a live context
            // has nothing to do, and one over a failure keeps the failure:
            // whatever went wrong is still true.
            other => {
                self.video = other;
                return;
            }
        };
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            self.fatal(Failure {
                detail: marked(
                    "Slint gave the player a renderer that is not OpenGL. The film \
                     and the glass are both drawn through OpenGL, so neither can be \
                     drawn without it.",
                ),
                cause: String::new(),
                note: marked("Updating the graphics driver is the usual fix."),
            });
            return;
        };
        let gl = unsafe { glow::Context::from_loader_function_cstr(|s| get_proc_address(s)) };
        self.diag.gl_info(&gl);

        let mut pipeline = match Pipeline::new(&gl) {
            Ok(p) => p,
            Err(e) => {
                self.fatal(Failure {
                    detail: marked("The graphics driver would not build the player's shaders. {}"),
                    cause: e.to_string(),
                    note: marked("If the driver is current, this is a bug in the player."),
                });
                return;
            }
        };
        pipeline.clear_surround = self.app.mpv.clear_surround();

        // mpv signals new frames from its own thread. `Weak` is Send but not
        // Sync, and mpv holds these callbacks by shared reference, so a Mutex
        // supplies the Sync half.
        let notify = self.waker();

        match unsafe { dbm_mpv::RenderContext::new(self.app.mpv.clone(), get_proc_address, notify) }
        {
            Ok(ctx) => {
                log::info!("mpv render context created on Slint's GL context");
                self.player.borrow_mut().start(file);
                self.video = Video::Live(Box::new(Gpu {
                    overlays: Overlays::new(&gl),
                    ctx,
                    gl,
                    pipeline,
                    published: None,
                }));
            }
            Err(e) => self.fatal(Failure {
                detail: marked("mpv could not attach to the player's graphics context. {}"),
                cause: e.to_string(),
                note: marked("Updating the graphics driver is the usual fix."),
            }),
        }
    }

    /// Ask for another frame while a hand is on the controls.
    ///
    /// Otherwise only mpv asks, and whatever follows the pointer (a hover, a
    /// slider, the seek preview) steps at the film's frame rate. Slint asks
    /// for its own animation frames, but not for something that moved between
    /// pointer events. So this follows whether the pointer moved this
    /// quarter-second rather than whether the bar is up: the bar stays three
    /// seconds after the hand stops, and redrawing all of that cost half the
    /// player's CPU at 165Hz.
    fn keep_the_interface_moving(&self) {
        // No second test against `Chrome::idle`: a hand that moved within the
        // quarter-second cannot also have been still for three, so the chrome
        // is up by construction whenever this asks for anything.
        if !self.app.activity.stirring() {
            return;
        }
        if let Some(ui) = self.ui.upgrade() {
            ui.window().request_redraw();
        }
    }

    fn waker(&self) -> dbm_mpv::Wake {
        let weak = Mutex::new(self.ui.clone());
        Box::new(move || {
            if let Ok(w) = weak.lock() {
                let _ = w.upgrade_in_event_loop(|ui| ui.window().request_redraw());
            }
        })
    }

    /// One frame, in the order the module doc gives.
    ///
    /// The video state is taken out of the driver for the length of it and
    /// put back at the end, so each step can borrow the rest of the driver
    /// freely while the context is held.
    fn frame(&mut self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let mut video = std::mem::replace(&mut self.video, Video::Waiting { file: None });
        match &mut video {
            // Nothing has asked for a context yet, so there is no frame to
            // draw and nothing gone wrong to explain.
            Video::Waiting { .. } => {}
            Video::Failed(failure) => {
                // The failure is all there is to say again in a language
                // just chosen.
                let changes = dbm_lang::changes();
                if changes != self.language_seen {
                    self.language_seen = changes;
                    self.say_failure(&ui, failure);
                }
            }
            Video::Live(gpu) => {
                // A fresh context starts with no published target. Drop
                // whatever image the old one left, before Slint can draw it
                // against names that mean something else here.
                if gpu.published.is_none() {
                    draw::clear_image(&ui);
                }
                let player = self.player.clone();
                let mut player = player.borrow_mut();

                let t = self.diag.begin();
                player.hear(&ui);
                let t = self.diag.mark_events(t);
                player.background(&ui);
                if let Some(still) = player.take_backdrop() {
                    gpu.pipeline.set_backdrop(
                        &gpu.gl,
                        &still.rgba,
                        Size::new(still.width, still.height),
                    );
                }
                let t = self.diag.mark_lists(t);

                glass::collect(&ui, &mut self.scratch.panels);
                let t = self.diag.mark_panels(t);
                self.diag.panels_changed(&self.scratch.panels);
                player.warm(&ui);
                self.app.scale.frame(&ui, self.app.settings.scale());

                self.draw(&ui, gpu, &player, t);
            }
        }
        self.video = video;
    }
}
