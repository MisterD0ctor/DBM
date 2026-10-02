//! Driving the GPU pipeline from Slint's rendering notifier.
//!
//! This is the only place that touches GL, and it runs entirely inside
//! Slint's own context during `RenderingSetup` / `BeforeRendering` /
//! `AfterRendering`. Everything else in the app is free of graphics concerns.
//!
//! The per-frame order matters and is not arbitrary — see [`Driver::frame`]:
//!
//!   1. install the window hooks that are not up yet
//!   2. drain mpv's events into the mirrored state, and push what moved
//!   3. bring what follows the player up to date: media keys, wake lock,
//!      the subtitle line
//!   4. ask for background work, and take delivery of what finished — see
//!      `completions`
//!   5. read panel geometry back out of the UI
//!   6. run the pipeline, which needs (5) to know where the glass goes, and
//!      publish the finished texture
//!
//! Steps 2 and 5 straddle the UI deliberately: the panel rectangles depend on
//! layout that depends on the state just pushed, and reading them in the same
//! frame is what stops the glass lagging a frame behind its widget.

mod completions;
mod events;

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use slint::{
    private_unstable_api::re_exports::IntSize, BorrowedOpenGLTextureBuilder,
    BorrowedOpenGLTextureOrigin, ComponentHandle, GraphicsAPI, RenderingState,
};

use crate::app::App;
use crate::gpu::gfx::Target;
use crate::gpu::pipeline::{GlassPanel, Pipeline};
use crate::interface::{format, glass, playlist_panel, sync};
use crate::library::playlist;
use crate::platform::modal_loop;
use crate::playback::commands;
use crate::playback::mpv::{self, Mpv};
use crate::playback::state::PlayerState;
use crate::worker::Completion;
use crate::{diagnostics, MainWindow};

/// Identity of what a Slint image property points at: texture id, active
/// image size, allocated texture size.
type Published = (NonZeroU32, u32, u32, u32, u32);

/// Everything that only exists once the GL context does.
struct Gpu {
    // Declaration order is drop order: the render context must die before the
    // `Mpv` whose symbol table it points into. Holding the `Rc` here makes
    // that hold even if the closure is torn down without a teardown notify.
    ctx: mpv::RenderContext,
    gl: glow::Context,
    pipeline: Pipeline,
    published: Option<Published>,
    _mpv: Arc<Mpv>,
}

/// Tracks whether the Windows modal-resize hook is installed yet.
///
/// It cannot go in before the loop starts, because the window handle only
/// exists once the window manager has made the window — so it is retried from
/// the frame path until it takes.
struct ModalHook {
    done: bool,
    attempts: u32,
}

impl ModalHook {
    const GIVE_UP_AFTER: u32 = 120;

    fn new() -> Self {
        Self {
            done: std::env::var_os("DBM_NO_MODAL_HOOK").is_some(),
            attempts: 0,
        }
    }

    fn poll(&mut self, ui: &MainWindow) {
        if self.done {
            return;
        }
        match modal_loop::keep_rendering_during_modal_loop(ui.window()) {
            Ok(()) => self.done = true,
            Err(e) => {
                self.attempts += 1;
                if self.attempts == Self::GIVE_UP_AFTER {
                    self.done = true;
                    eprintln!(
                        "dbm: modal-loop hook unavailable ({e}); holding a window edge \
                         will stall rendering"
                    );
                }
            }
        }
    }
}

pub struct Driver {
    ui: slint::Weak<MainWindow>,
    app: App,
    /// Loaded once the render context exists — see `RenderingSetup`.
    file: Option<String>,
    gpu: Option<Gpu>,
    player: PlayerState,
    panels: Vec<GlassPanel>,
    modal: ModalHook,
    /// Installed the same way and for the same reason as the modal hook.
    drops: crate::platform::dropped::Accepting,
    lists: sync::ListSync,
    /// Keeps the duration cache the playlist reads from up to date.
    durations: crate::library::durations::Recorder,
    /// Describes the playlist's other files without playing them.
    scan: crate::library::probe::Scan,
    /// Looks for the seasons beside a playlist that is one season of a show.
    neighbours: crate::library::shelf::Neighbours,
    /// Last seen state of the playlist panel, so its opening can be noticed.
    playlist_open: bool,
    /// The file the way on was last looked for on behalf of.
    onward_asked: Option<String>,
    /// Reply ids seen this frame, routed below. Reused to avoid allocating
    /// on the frame path.
    replies: Vec<u64>,
    /// What the interface should say out loud this frame. Same reuse.
    notices: Vec<String>,
    /// Which playlist entry to start on once `loadlist` reports done.
    pending_start: Option<usize>,
    /// `PlayerState::loads` as of the last frame — see where it is compared.
    loads_seen: u64,
    /// The OS media overlay, told what is playing when it changes.
    smtc: crate::platform::smtc::Controls,
    /// Holds the display on while something plays.
    awake: crate::platform::awake::Awake,
    diag: diagnostics::Probe,
    /// A picture of a finished frame, when one is asked for.
    capture: diagnostics::Capture,
}

impl Driver {
    pub fn new(
        ui: slint::Weak<MainWindow>,
        app: App,
        file: Option<String>,
        smtc: crate::platform::smtc::Controls,
    ) -> Self {
        Self {
            ui,
            app,
            file,
            gpu: None,
            player: PlayerState::default(),
            panels: Vec::new(),
            modal: ModalHook::new(),
            drops: crate::platform::dropped::Accepting::default(),
            lists: sync::ListSync::default(),
            durations: crate::library::durations::Recorder::default(),
            scan: crate::library::probe::Scan::default(),
            neighbours: crate::library::shelf::Neighbours::default(),
            playlist_open: false,
            onward_asked: None,
            replies: Vec::new(),
            notices: Vec::new(),
            pending_start: None,
            loads_seen: 0,
            smtc,
            awake: crate::platform::awake::Awake::default(),
            diag: diagnostics::Probe::new(),
            capture: diagnostics::Capture::new(),
        }
    }

    pub fn on(&mut self, state: RenderingState, api: &GraphicsAPI) {
        match state {
            RenderingState::RenderingSetup => self.setup(api),
            RenderingState::BeforeRendering => self.frame(),
            RenderingState::AfterRendering => {
                if let Some(gpu) = self.gpu.as_ref() {
                    // Tells mpv the frame reached the screen, which is what
                    // keeps its frame timing honest.
                    gpu.ctx.report_swap();
                    // Here rather than in `frame`, because here the interface
                    // is already drawn over the video and the picture is the
                    // whole thing. Does nothing unless asked.
                    if let Some(ui) = self.ui.upgrade() {
                        let size = ui.window().size();
                        self.capture.maybe(&gpu.gl, size.width, size.height);
                        // Its fade is drawn here rather than by Slint, so
                        // nothing else knows to ask for the frames.
                        if gpu.pipeline.backdrop_arriving() {
                            ui.window().request_redraw();
                        }
                    }
                }
                self.keep_the_interface_moving();
            }
            RenderingState::RenderingTeardown => {
                if let Some(mut gpu) = self.gpu.take() {
                    let Gpu { gl, pipeline, .. } = &mut gpu;
                    pipeline.release(gl);
                }
            }
            _ => {}
        }
    }

    /// Say, on screen, that there will be no video this run.
    ///
    /// All three callers leave the player without a render context, which
    /// means no film, no glass, and — since the glass pass is what draws every
    /// surface in the interface — no chrome either. `eprintln!` was the only
    /// report any of them made, and a packaged build has no console to make it
    /// to, so the window simply sat there black and a person could reasonably
    /// conclude the player was broken rather than the driver.
    ///
    /// The way in says it, because the way in is what is on screen when no
    /// file is loaded and no file can now be loaded. It drops its rows: they
    /// offer to open something that could only play invisibly.
    ///
    /// `note` is what usually puts it right, and it is not the same for all
    /// three. A context the driver would not give, or mpv could not use, is
    /// the driver's; shaders it refused may be an old driver or a fault in the
    /// player's own GLSL, and telling everyone to update a driver sends the
    /// second kind looking in the wrong place.
    fn fatal(&self, reason: &str, detail: String, note: &str) {
        eprintln!("dbm: {reason} — {detail}");
        if let Some(ui) = self.ui.upgrade() {
            ui.set_fatal(reason.into());
            ui.set_fatal_detail(detail.into());
            ui.set_fatal_note(note.into());
        }
    }

    fn setup(&mut self, api: &GraphicsAPI) {
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            self.fatal(
                "The player cannot show video on this computer.",
                "Slint gave the player a renderer that is not OpenGL. The film \
                 and the glass are both drawn through OpenGL, so neither can be \
                 drawn without it."
                    .into(),
                "Updating the graphics driver is the usual fix.",
            );
            return;
        };
        let gl = unsafe { glow::Context::from_loader_function_cstr(|s| get_proc_address(s)) };
        self.diag.gl_info(&gl);

        let pipeline = match Pipeline::new(&gl) {
            Ok(p) => p,
            Err(e) => {
                self.fatal(
                    "The player cannot show video on this computer.",
                    format!("The graphics driver would not build the player's shaders. {e}"),
                    "If the driver is current, this is a bug in the player.",
                );
                return;
            }
        };

        // mpv signals new frames from its own thread. `Weak` is Send but not
        // Sync, and mpv holds these callbacks by shared reference, so a Mutex
        // supplies the Sync half.
        let notify = self.waker();
        // Events are drained on the UI thread at video rate; this wakeup only
        // has to cover the paused case, where no frames are coming and an
        // event would otherwise sit unnoticed.
        self.app.mpv.set_wakeup(self.waker());

        match unsafe { mpv::RenderContext::new(&self.app.mpv, get_proc_address, notify) } {
            Ok(ctx) => {
                eprintln!("dbm: mpv render context created on Slint's GL context");
                // Open only now. With `vo=libmpv` there is no video output
                // until the render context exists, so a file loaded earlier
                // starts against a VO that cannot present and never produces
                // a frame.
                //
                // The scan runs on the worker: a directory of thousands, or
                // one on a network share, would otherwise stall startup.
                if let Some(f) = self.file.take() {
                    let path = std::path::PathBuf::from(f);
                    self.app
                        .worker
                        .submit(move |_mpv| Some(Completion::Opened(playlist::prepare(&path))));
                } else {
                    // Nothing named, so the empty window is what is coming:
                    // find what it can offer to pick back up. A stat per
                    // played file and one ffmpeg run, so on the worker.
                    self.app.worker.submit(|_mpv| {
                        Some(Completion::Resume(crate::library::resume::last_watched()))
                    });
                }
                self.gpu = Some(Gpu {
                    ctx,
                    gl,
                    pipeline,
                    published: None,
                    _mpv: self.app.mpv.clone(),
                });
            }
            Err(e) => self.fatal(
                "The player cannot show video on this computer.",
                format!("mpv could not attach to the player's graphics context. {e}"),
                "Updating the graphics driver is the usual fix.",
            ),
        }
    }

    /// Ask for another frame while a hand is on the controls.
    ///
    /// Otherwise the only thing asking is mpv, and what follows the pointer
    /// can only move as often as the film does — a hover, a slider, the seek
    /// preview tracking the timeline, all of them stepping at 24 or 30 to a
    /// second on a display capable of five times that.
    ///
    /// Tied to the hand rather than to the chrome, and the difference is most
    /// of the player's CPU. Slint asks for its own frames: an animation runs
    /// at the display's rate with nothing here helping it, measured at 160
    /// draws a second against 165 with this on. What it cannot ask for is a
    /// frame for something that moved *between* pointer events, which is the
    /// whole of the list above and nothing else. So the question worth asking
    /// is whether the pointer is live this quarter-second, not whether the bar
    /// is up — the bar is up for three seconds after the hand stops, and every
    /// frame in that window was a full composite of a picture that had not
    /// changed. At 165Hz with a panel open that was half the CPU the player
    /// used, spent on nothing: 21% of a core against 10% with it gone.
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

    fn waker(&self) -> Box<dyn Fn() + Send + Sync> {
        let weak = Mutex::new(self.ui.clone());
        Box::new(move || {
            if let Ok(w) = weak.lock() {
                let _ = w.upgrade_in_event_loop(|ui| ui.window().request_redraw());
            }
        })
    }

    /// One frame, in the order the module doc gives.
    ///
    /// The GPU state is taken out of the driver for the length of it and put
    /// back at the end, so each step can borrow the rest of the driver freely
    /// while the pipeline is held.
    fn frame(&mut self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let Some(mut gpu) = self.gpu.take() else {
            return;
        };
        self.modal.poll(&ui);
        self.drops.poll(&ui);

        let t = self.diag.begin();
        let moved = self.take_events(&ui);
        self.follow(&ui, moved);
        let t = self.diag.mark_events(t);

        self.background(&ui, &mut gpu);
        let t = self.diag.mark_lists(t);

        glass::collect(&ui, &mut self.panels);
        let t = self.diag.mark_panels(t);
        self.diag.panels_changed(&self.panels);

        self.draw(&ui, &mut gpu, t);
        self.gpu = Some(gpu);
    }

    /// Drain mpv's events into the mirrored state, and push out what moved.
    /// Returns whether any of it did.
    fn take_events(&mut self, ui: &MainWindow) -> bool {
        let moved = events::drain(
            &self.app.mpv,
            &mut self.player,
            &mut self.replies,
            &mut self.notices,
            &self.app.audio,
        );
        // A failure marks its playlist row, which the list otherwise only
        // hears about on the next read.
        if !self.notices.is_empty() {
            playlist_panel::push(ui, &self.player);
        }
        for notice in std::mem::take(&mut self.notices) {
            // A notice means something concluded, which is the backstop for
            // an open that never produces a file: a container mpv cannot
            // read raises its `EndFile` failure here and `has_file` never
            // turns true, so without this the line would say "Opening…"
            // until the next successful one.
            ui.set_opening(false);
            say(ui, notice);
        }
        if moved {
            sync::push_scalars(ui, &self.player);
        }
        // An open is over when mpv finishes opening a file — not when it has
        // one. This used to ask the second question, and with a film already
        // playing the answer was yes on the very next frame: "Opening …" over
        // a playing film was cleared before it was ever drawn, so the slow
        // case it exists for showed nothing at all. The failures end it by
        // their own routes: a scan that found nothing clears it where the
        // scan reports back, and a file mpv could not play raises a notice,
        // which clears it above.
        if self.player.loads != self.loads_seen {
            self.loads_seen = self.player.loads;
            ui.set_opening(false);
        }
        for id in std::mem::take(&mut self.replies) {
            match id {
                commands::REPLY_SCRUB => self.app.scrubber.on_reply(&self.app.mpv),
                // The list exists now. Which entry plays was settled
                // before it loaded — see `commands::load_list` — so all this
                // has to do is make sure it is playing.
                commands::REPLY_LOADLIST => {
                    if self.pending_start.take().is_some() {
                        commands::set_pause(&self.app.mpv, false);
                    }
                }
                _ => {}
            }
        }
        moved
    }

    /// Bring everything that follows the player up to date with it: the OS
    /// media controls, the display's wake lock, and the subtitle line.
    fn follow(&mut self, ui: &MainWindow, moved: bool) {
        // Every frame, not only the ones that moved: registration needs a
        // window handle that does not exist yet on the first of them, and the
        // update itself is skipped unless something changed.
        self.smtc.publish(ui.window(), &self.player, moved);
        self.awake.follow(&self.player);
        // Two reads and a comparison while the bar is still; a command a
        // frame only while it fades. See `subline`.
        self.app.subline.follow(
            &self.app.mpv,
            ui.get_subtitle_ceiling() as f64,
            ui.get_subtitle_lift() as f64,
        );
        if let Some(own) = self.app.subline.readout() {
            ui.set_sub_pos_text(format::position(own).into());
        }
    }

    /// Ask for the background work the player's state calls for, and take
    /// delivery of whatever finished.
    fn background(&mut self, ui: &MainWindow, gpu: &mut Gpu) {
        // Opening the playlist is the cue to go and look at the resume
        // positions again: they are written to disk behind our back, and this
        // is the moment somebody cares what they say.
        let open = ui.get_playlist_open();
        if open && !self.playlist_open {
            self.lists.refresh();
            self.neighbours.refresh();
        }
        self.playlist_open = open;

        // Each of these asks only when what it follows has moved; the work
        // itself is on the worker.
        self.lists.poll(&self.app.worker, &self.player);
        self.durations.poll(&self.app.worker, &self.player);
        self.poll_preview(ui);

        // Several probe results can land in one drain, and seasons found
        // beside the list with them; the playlist is rebuilt once for all.
        // The worker's own handle, so the drain does not hold the driver
        // borrowed while each result is taken in.
        let worker = self.app.worker.clone();
        let mut playlist_moved = false;
        for completion in worker.drain() {
            playlist_moved |= self.receive(ui, gpu, completion);
        }
        if playlist_moved {
            playlist_panel::push(ui, &self.player);
        }

        self.look_onward(ui);
    }

    /// Ask for a thumbnail atlas when the playing file changes.
    ///
    /// Driven off the mirrored `path` rather than off opening a playlist: what
    /// wants thumbnails is whatever is on screen now, and that changes at the
    /// end of every file as well as when someone picks one.
    fn poll_preview(&self, ui: &MainWindow) {
        let preview = &self.app.preview;
        preview.set_duration(self.player.duration);
        let Some(path) = self.player.path.as_deref() else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        if !preview.claim(&path) {
            return;
        }
        // The old atlas belongs to the old file. Cleared now rather than left
        // under the pointer showing the previous film for however long the new
        // one takes to build.
        preview.set_sprite(None);
        ui.set_preview_tile_w(0);
        ui.set_preview_tile_h(0);

        crate::library::preview::spawn(path, self.ui.clone());
    }

    /// Look for the way on at the end of the last file, once per file — or in
    /// its closing credits, where the credits pill makes the same offer before
    /// the file is over. The offer belongs to the file that asked for it, so
    /// a new one takes it back down.
    fn look_onward(&mut self, ui: &MainWindow) {
        let ending = (self.player.eof_reached || self.player.credits_rolling())
            && self.player.playlist_pos + 1 >= self.player.playlist_count;
        if self.player.path != self.onward_asked && self.onward_asked.take().is_some() {
            ui.set_onward_path("".into());
            ui.set_onward_label("".into());
            ui.set_onward_season(false);
        }
        if !ending || self.onward_asked.is_some() {
            return;
        }
        let Some(path) = self.player.path.clone() else {
            return;
        };
        self.onward_asked = Some(path.clone());
        self.app.worker.submit(move |_mpv| {
            let to = crate::library::onward::after(&path);
            Some(Completion::Onward { from: path, to })
        });
    }

    /// Run the pipeline and hand Slint the result.
    fn draw(&mut self, ui: &MainWindow, gpu: &mut Gpu, t: std::time::Instant) {
        // Material parameters, if a slider moved. Coalesced in the store,
        // so a drag costs one update per frame however fast it moves.
        let mut params_dirty = false;
        if let Some((glass, border)) = self.app.settings.take_changes() {
            gpu.pipeline.glass = glass;
            gpu.pipeline.params = border;
            gpu.pipeline.border_enabled = self.app.settings.ambience_on();
            gpu.pipeline.glass_enabled = self.app.settings.glass_on();
            params_dirty = true;
        }

        // The backdrop stands in only while nothing is loaded.
        gpu.pipeline.show_backdrop = self.player.path.is_none();

        let size = ui.window().size();
        // For the size being rendered at, not the one mpv last reported on.
        let rect = self.player.video_rect(size.width, size.height);
        // Poll exactly once: reading the update flag also clears it, so a
        // second call would swallow the frame it reported.
        let new_frame = gpu.ctx.wants_redraw();

        let rendered = match gpu.pipeline.render(
            &gpu.gl,
            &gpu.ctx,
            size.width,
            size.height,
            new_frame,
            params_dirty,
            rect,
            &self.panels,
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("dbm: pipeline: {e}");
                false
            }
        };
        self.diag.mark_render(t);
        if !rendered {
            return;
        }

        self.diag.frame_done(
            new_frame,
            &gpu.pipeline,
            &gpu.gl,
            &self.player,
            size.width,
            size.height,
        );

        publish(gpu.pipeline.output(), &mut gpu.published, |img, w, h| {
            ui.set_video_frame(img);
            ui.set_video_clip_w(w);
            ui.set_video_clip_h(h);
        });
    }
}

/// Put one sentence on screen, in the capsule the action flash already uses.
///
/// The alternative this replaces was `eprintln!` — which in a packaged build
/// goes to a console that does not exist, so every failure was indistinguishable
/// from the player ignoring you.
fn say(ui: &MainWindow, text: String) {
    ui.global::<crate::Flash>().invoke_notice(text.into());
}

/// Point a Slint image property at a target, but only when something actually
/// changed — `Property::set` skips equal values, so a no-op costs nothing but
/// also invalidates nothing.
///
/// The `Image` covers the whole texture, which is generally larger than the
/// active image because targets grow only. `set` receives the active size so
/// the UI can clip the source rect down to it.
fn publish(
    target: &Target,
    slot: &mut Option<Published>,
    set: impl FnOnce(slint::Image, i32, i32),
) {
    let Some(id) = target.texture_id() else {
        return;
    };
    let (uw, uh) = target.size();
    let (aw, ah) = target.alloc();
    let key = (id, uw, uh, aw, ah);
    if *slot == Some(key) {
        return;
    }
    let image =
        unsafe { BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(id, IntSize::new(aw, ah)) }
            // mpv renders unflipped and every pass preserves that, so the
            // whole chain reads top-left like the rest of the app.
            .origin(BorrowedOpenGLTextureOrigin::TopLeft)
            .build();
    set(image, uw as i32, uh as i32);
    *slot = Some(key);
}
