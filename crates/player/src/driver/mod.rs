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
use crate::gpu::pipeline::{Frame, GlassPanel, Pipeline};
use crate::interface::{fades, format, glass, playlist_panel, sync};
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
    /// Softens the ends of scrolling lists, once Slint has drawn them.
    /// Absent where its shader would not build: the lists are then cut by
    /// their panel as they always were, which is not worth losing the
    /// player over.
    fader: Option<crate::gpu::fade::Fader>,
    /// The angle in a list's end pointing back to its current row, drawn
    /// over the soft edge it sits in. Absent the same way: the lists still
    /// scroll, and nothing points back.
    way: Option<crate::gpu::way::Ways>,
    /// The shadow under the interface's words and marks, the same way.
    shadow: Option<crate::gpu::shadow::Shadow>,
    published: Option<Published>,
    _mpv: Arc<Mpv>,
}

pub struct Driver {
    ui: slint::Weak<MainWindow>,
    app: App,
    /// Loaded once the render context exists — see `RenderingSetup`.
    file: Option<String>,
    gpu: Option<Gpu>,
    player: PlayerState,
    panels: Vec<GlassPanel>,
    /// The ends of scrolling lists to soften this frame. Reused, like the
    /// panels, to keep the frame path from allocating.
    fades: Vec<crate::gpu::fade::Strip>,
    /// The angles pointing lists back to their current rows, reused the same way.
    ways: Vec<crate::gpu::way::Way>,
    modal: modal_loop::Hook,
    /// Installed the same way and for the same reason as the modal hook.
    drops: crate::platform::dropped::Accepting,
    lists: sync::ListSync,
    /// Has the playlist laid out on the worker, and shows what comes back.
    panel: playlist_panel::Panel,
    /// `lang::changes` as of the last frame, so words already handed to the
    /// interface are said again in a language just chosen.
    language_seen: usize,
    /// How much is left of the film the way in offers, kept so that line can
    /// be said again too.
    resume_left: Option<f64>,
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
    /// The file whose title sequence and credits were last looked for, so
    /// each is looked for once.
    titles_asked: Option<String>,
    /// The file subtitles fetched before were last put back for, and the
    /// search's page last closed for: both once a file.
    kept_asked: Option<String>,
    /// Reply ids seen this frame, routed below. Reused to avoid allocating
    /// on the frame path.
    replies: Vec<u64>,
    /// What the interface should say out loud this frame. Same reuse.
    notices: Vec<String>,
    /// Whether a `loadlist` is on its way, so that its reply knows to start
    /// playback.
    loading_list: bool,
    /// `PlayerState::loads` as of the last frame — see where it is compared.
    loads_seen: u64,
    /// The media keys and the desktop's media widget, told what is playing
    /// when it changes.
    media_keys: crate::platform::media_keys::Controls,
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
        media_keys: crate::platform::media_keys::Controls,
    ) -> Self {
        Self {
            ui,
            app,
            file,
            gpu: None,
            player: PlayerState::default(),
            panels: Vec::new(),
            fades: Vec::new(),
            ways: Vec::new(),
            modal: modal_loop::Hook::new(),
            drops: crate::platform::dropped::Accepting::new(),
            lists: sync::ListSync::default(),
            panel: playlist_panel::Panel::default(),
            language_seen: 0,
            resume_left: None,
            durations: crate::library::durations::Recorder::default(),
            scan: crate::library::probe::Scan::default(),
            neighbours: crate::library::shelf::Neighbours::default(),
            playlist_open: false,
            onward_asked: None,
            titles_asked: None,
            kept_asked: None,
            replies: Vec::new(),
            notices: Vec::new(),
            loading_list: false,
            loads_seen: 0,
            media_keys,
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
                if let Some(gpu) = self.gpu.as_mut() {
                    // Tells mpv the frame reached the screen, which is what
                    // keeps its frame timing honest.
                    gpu.ctx.report_swap();
                    // Here rather than in `frame`, because here the interface
                    // is already drawn over the video and the picture is the
                    // whole thing. Does nothing unless asked.
                    if let Some(ui) = self.ui.upgrade() {
                        let size = ui.window().size();
                        // Before the picture is taken, so all of it is in it.
                        // The ends of any list with rows past them first,
                        // softened on the frame Slint has just finished...
                        fades::collect(&ui, &mut self.fades, &mut self.ways);
                        if let Some(fader) = gpu.fader.as_mut() {
                            fader.run(
                                &gpu.gl,
                                gpu.pipeline.output(),
                                (size.width, size.height),
                                &self.fades,
                            );
                        }
                        // ...then the angle pointing back to a list's current
                        // row, whole over the edge it sits in. Its circle
                        // eases in under the pointer, and nothing Slint draws
                        // moves with it to ask for the frames.
                        if let Some(way) = gpu.way.as_mut() {
                            way.run(&gpu.gl, (size.width, size.height), &self.ways);
                            if self.ways.iter().any(|w| w.lit > 0.0 && w.lit < 1.0) {
                                ui.window().request_redraw();
                            }
                        }
                        // ...and then the shadow, cast by what is left. The
                        // other way round, a row cut off at a list's edge
                        // cast its shadow past the edge, where no strip
                        // reaches, and the row thinned away above a dark
                        // smudge that stayed.
                        if let Some(shadow) = gpu.shadow.as_mut() {
                            shadow.run(
                                &gpu.gl,
                                gpu.pipeline.output(),
                                (size.width, size.height),
                                &self.panels,
                                ui.window().scale_factor(),
                            );
                        }
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
                    let Gpu {
                        gl,
                        pipeline,
                        fader,
                        way,
                        shadow,
                        ..
                    } = &mut gpu;
                    pipeline.release(gl);
                    if let Some(fader) = fader {
                        fader.release(gl);
                    }
                    if let Some(way) = way {
                        way.release(gl);
                    }
                    if let Some(shadow) = shadow {
                        shadow.release(gl);
                    }
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
                let fader = crate::gpu::fade::Fader::new(&gl)
                    .inspect_err(|e| eprintln!("dbm: no soft edge on scrolling lists: {e}"))
                    .ok();
                let way = crate::gpu::way::Ways::new(&gl)
                    .inspect_err(|e| eprintln!("dbm: no way back to a list's current row: {e}"))
                    .ok();
                let shadow = crate::gpu::shadow::Shadow::new(&gl)
                    .inspect_err(|e| eprintln!("dbm: no shadow under the interface: {e}"))
                    .ok();
                self.gpu = Some(Gpu {
                    ctx,
                    gl,
                    pipeline,
                    fader,
                    way,
                    shadow,
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
        self.modal.poll(ui.window());
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
            self.panel.request(&self.player, &self.app.worker);
        }
        for notice in std::mem::take(&mut self.notices) {
            // A notice means something concluded, which is the backstop for
            // an open that never produces a file: a container mpv cannot
            // read raises its `EndFile` failure here and `has_file` never
            // turns true, so without this the line would say "Opening…"
            // until the next successful one.
            ui.set_opening(false);
            warn(ui, notice);
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
                commands::REPLY_LOADLIST if self.loading_list => {
                    self.loading_list = false;
                    commands::set_pause(&self.app.mpv, false);
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
        self.media_keys.publish(ui.window(), &self.player, moved);
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
        // beside the list with them; the playlist is laid out once for all.
        // The worker's own handle, so the drain does not hold the driver
        // borrowed while each result is taken in.
        let worker = self.app.worker.clone();
        let mut playlist_moved = false;
        for completion in worker.drain() {
            playlist_moved |= self.receive(ui, gpu, completion);
        }
        if playlist_moved {
            self.panel.request(&self.player, &self.app.worker);
        }

        self.look_onward(ui);
        self.listen_for_the_titles();
        self.put_back_downloads(ui);
        self.say_it_again(ui);
    }

    /// Put back the subtitles fetched for this file on an earlier visit,
    /// once it has opened, and close a search still open for the last one.
    fn put_back_downloads(&mut self, ui: &MainWindow) {
        if self.player.path == self.kept_asked {
            return;
        }
        ui.set_find_open(false);
        ui.set_find_state(0);
        ui.set_find_fetching(-1);
        // Not until it has opened: a track added before then is lost with
        // the load it raced.
        let Some(path) = self
            .player
            .path
            .clone()
            .filter(|_| self.player.duration > 0.0)
        else {
            return;
        };
        self.kept_asked = Some(path.clone());
        self.app.worker.submit(move |_mpv| {
            let files = crate::library::opensubtitles::kept(&path);
            (!files.is_empty()).then_some(Completion::Kept { path, files })
        });
    }

    /// Say everything the Rust side has handed the interface over again when
    /// the language changes: track names, the chapter, the playlist's parts,
    /// what is left of the film on the way in. The interface's own words
    /// follow by themselves; these were sentences by the time it got them.
    fn say_it_again(&mut self, ui: &MainWindow) {
        let changes = crate::lang::changes();
        if changes == self.language_seen {
            return;
        }
        self.language_seen = changes;
        sync::push_lists(ui, &self.player);
        sync::push_scalars(ui, &self.player);
        self.panel.request(&self.player, &self.app.worker);
        if let Some(seconds) = self.resume_left {
            ui.set_resume_left(format::left(seconds).into());
        }
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
        // A sync still listening is listening to the old film, and one that
        // could be undone was this film's to undo.
        crate::library::subsync::supersede();
        ui.set_sync_state(0);

        crate::library::preview::spawn(path, self.app.worker.reporter());
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

    /// Have an episode's title sequence and closing credits listened for,
    /// once per file, whichever of them its chapters do not already mark.
    ///
    /// Not until the lists have been read for this file and hold it: the
    /// episodes beside it are what it is compared with, and the chapters
    /// that could make the whole thing unnecessary arrive in the same read.
    fn listen_for_the_titles(&mut self) {
        use crate::library::titles::{self, End};
        let Some(path) = self.player.path.as_deref() else {
            return;
        };
        if self.titles_asked.as_deref() == Some(path)
            || self.player.duration <= 0.0
            || !self.lists.settled(&self.player)
            || !self.player.playlist.iter().any(|e| e.filename == path)
        {
            return;
        }
        self.titles_asked = Some(path.to_string());
        // The opening first: it is the one that is about to play.
        let mut ends = Vec::new();
        if self.player.intro_named().is_none() {
            ends.push(End::Opening);
        }
        if !self.player.credits_marked() {
            ends.push(End::Closing);
        }
        if ends.is_empty() {
            return;
        }
        let paths: Vec<String> = self
            .player
            .playlist
            .iter()
            .map(|e| e.filename.clone())
            .collect();
        let beside = titles::beside(path, &paths);
        if beside.is_empty() {
            return;
        }
        titles::spawn(
            path.to_string(),
            ends,
            beside,
            self.player.duration,
            self.app.worker.reporter(),
        );
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
            gpu.pipeline.animate = self.app.settings.animations();
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

        let frame = Frame {
            width: size.width,
            height: size.height,
            new_frame,
            params_dirty,
            rect,
            panels: &self.panels,
        };
        let rendered = match gpu.pipeline.render(&gpu.gl, &gpu.ctx, &frame) {
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

/// Say that something did not happen: the same sentence, with the warning
/// mark before it. For a failure, never for news.
fn warn(ui: &MainWindow, text: String) {
    ui.global::<crate::Flash>().invoke_warn(text.into());
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
