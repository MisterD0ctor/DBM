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
//!      `background` and `completions`
//!   5. read panel geometry back out of the UI
//!   6. run the pipeline, which needs (5) to know where the glass goes, and
//!      publish the finished texture — see `draw`
//!
//! Steps 2 and 5 straddle the UI deliberately: the panel rectangles depend on
//! layout that depends on the state just pushed, and reading them in the same
//! frame is what stops the glass lagging a frame behind its widget.

mod background;
mod completions;
mod draw;
mod events;
mod feeds;

use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::Instant;

use slint::{ComponentHandle, GraphicsAPI, RenderingState};

use crate::app::App;
use crate::interface::subline::Picture;
use crate::interface::{fades, format, glass, playlist_panel, sync, warn};
use crate::platform::modal_loop;
use crate::playback::bars::Bars;
use crate::playback::commands;
use crate::playback::skips::{Skip, Skips, Wanted};
use crate::playback::state::PlayerState;
use crate::settings::Toggle;
use crate::worker::Completion;
use crate::{MainWindow, Playback, Playlist, Subtitles, diagnostics};
use dbm_gpu::Overlays;
use dbm_gpu::pipeline::{GlassPanel, Pipeline};
use dbm_lang::{fill, marked, tr};
use dbm_library::playlist;
use dbm_mpv::MediaPath;
use dbm_mpv::tracks::{self, TrackKind};

/// Identity of what a Slint image property points at: texture id, active
/// image size, allocated texture size.
type Published = (NonZeroU32, u32, u32, u32, u32);

/// Everything that only exists once the GL context does.
struct Gpu {
    ctx: dbm_mpv::RenderContext,
    gl: glow::Context,
    pipeline: Pipeline,
    overlays: Overlays,
    published: Option<Published>,
}

impl Hooks {
    /// Install whichever hooks are not up yet. Each needs the native window,
    /// which does not exist until the loop has turned, so each retries a
    /// frame until it takes.
    fn install(&mut self, ui: &MainWindow) {
        self.modal.poll(ui.window());
        self.drops.poll(ui);
        self.rounding.poll(ui.window());
    }

    /// Tell the media controls and the wake lock what is playing.
    ///
    /// Every frame, not only the ones that moved: registration needs a window
    /// handle that does not exist yet on the first of them, and the update
    /// itself is skipped unless something changed.
    fn follow(&mut self, ui: &MainWindow, player: &PlayerState, moved: bool) {
        self.media_keys.publish(ui.window(), player, moved);
        self.awake.follow(player);
    }
}

/// The buffers each frame fills, kept between frames so the frame path
/// does not allocate.
#[derive(Default)]
struct Scratch {
    panels: Vec<GlassPanel>,
    /// Where the shadow under the interface is cast this frame, which the
    /// glass pass holds under white for it (see `shadow::regions`).
    shaded: Vec<[i32; 4]>,
    /// The ends of scrolling lists to soften this frame. Reused, like the
    /// panels, to keep the frame path from allocating.
    fades: Vec<dbm_gpu::fade::Strip>,
    /// The angles pointing lists back to their current rows, reused the same way.
    ways: Vec<dbm_gpu::way::Way>,
    /// Reply ids seen this frame, routed below. Reused to avoid allocating
    /// on the frame path.
    replies: Vec<u64>,
    /// What the interface should say out loud this frame. Same reuse.
    notices: Vec<String>,
}

/// What each operating system is told, or asked, as the player runs.
struct Hooks {
    modal: modal_loop::Hook,
    /// Installed the same way and for the same reason as the modal hook.
    drops: crate::platform::dropped::Accepting,
    /// Asked for the same way.
    rounding: crate::platform::window::Rounding,
    /// The media keys and the desktop's media widget, told what is playing
    /// when it changes.
    media_keys: crate::platform::media_keys::Controls,
    /// Holds the display on while something plays.
    awake: crate::platform::awake::Awake,
}

/// What follows the playing list and asks the worker for what it lacks.
#[derive(Default)]
struct Feeds {
    lists: sync::ListSync,
    /// Keeps the duration cache the playlist reads from up to date.
    durations: feeds::Recorder,
    /// Describes the playlist's other files without playing them.
    scan: feeds::Scan,
    /// Looks for the seasons beside a playlist that is one season of a show.
    neighbours: feeds::Neighbours,
}

/// What was done once for the file playing, so it is not done again, and
/// is done afresh for the next.
#[derive(Default)]
struct PerFile {
    /// The file the way on was last looked for on behalf of.
    onward_asked: Option<MediaPath>,
    /// The file whose end was last given a light, and the file whose end has
    /// one now (see `light_the_end`).
    end_light_asked: Option<MediaPath>,
    end_lit: Option<MediaPath>,
    /// Whether that light is the mark, for want of a frame of the file's own.
    end_marked: bool,
    /// The file whose title sequence and credits were last looked for, so
    /// each is looked for once.
    titles_asked: Option<MediaPath>,
    /// The file subtitles fetched before were last put back for, and the
    /// search's page last closed for: both once a file.
    kept_asked: Option<MediaPath>,
    /// The file whose frame was last looked at for bars.
    bars_asked: Option<MediaPath>,
}

pub struct Driver {
    ui: slint::Weak<MainWindow>,
    app: App,
    /// Loaded once the render context exists — see `RenderingSetup`.
    file: Option<std::path::PathBuf>,
    gpu: Option<Gpu>,
    player: PlayerState,
    /// Has the playlist laid out on the worker, and shows what comes back.
    panel: playlist_panel::Panel,
    /// `lang::changes` as of the last frame, so words already handed to the
    /// interface are said again in a language just chosen.
    language_seen: usize,
    /// How much is left of the film the way in offers, kept so that line can
    /// be said again too.
    resume_left: Option<f64>,
    /// The panels to draw unseen before they are opened.
    warm: crate::interface::warm::WarmUp,
    /// Last seen state of the playlist panel, so its opening can be noticed.
    playlist_open: bool,
    /// Whether a `loadlist` is on its way, so that its reply knows to start
    /// playback.
    loading_list: bool,
    /// The files of the lists given to `loadlist` and not yet answered, in
    /// the order they were given, each deleted when its answer comes, by
    /// when mpv has read it.
    lists_loading: std::collections::VecDeque<std::path::PathBuf>,
    /// Whether an open came to nothing since a file last loaded, so that if
    /// nothing plays now, the empty window looks for what to offer as it
    /// does at launch.
    stumbled: bool,
    /// `PlayerState::loads` as of the last frame — see where it is compared.
    loads_seen: u64,
    diag: diagnostics::Probe,
    /// A picture of a finished frame, when one is asked for.
    capture: diagnostics::Capture,
    /// Why there is no video this run, if there isn't.
    failure: Option<Failure>,
    scratch: Scratch,
    hooks: Hooks,
    feeds: Feeds,
    per_file: PerFile,
    skips: Skips,
    bars: Bars,
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
        media_keys: crate::platform::media_keys::Controls,
    ) -> Self {
        Self {
            ui,
            app,
            file,
            gpu: None,
            player: PlayerState::default(),
            panel: playlist_panel::Panel::default(),
            language_seen: 0,
            resume_left: None,
            warm: crate::interface::warm::WarmUp::default(),
            playlist_open: false,
            loading_list: false,
            lists_loading: std::collections::VecDeque::new(),
            stumbled: false,
            loads_seen: 0,
            diag: diagnostics::Probe::new(),
            capture: diagnostics::Capture::new(),
            failure: None,
            scratch: Scratch::default(),
            hooks: Hooks {
                modal: modal_loop::Hook::new(),
                drops: crate::platform::dropped::Accepting::new(),
                rounding: crate::platform::window::Rounding::new(),
                media_keys,
                awake: crate::platform::awake::Awake::default(),
            },
            feeds: Feeds::default(),
            per_file: PerFile::default(),
            skips: Skips::default(),
            bars: Bars::default(),
        }
    }

    pub fn on(&mut self, state: RenderingState, api: &GraphicsAPI) {
        match state {
            RenderingState::RenderingSetup => self.setup(api),
            RenderingState::BeforeRendering => self.frame(),
            RenderingState::AfterRendering => {
                self.diag.drawn();
                if let Some(gpu) = self.gpu.as_mut() {
                    // Tells mpv the frame reached the screen, which is what
                    // keeps its frame timing honest. Slint keeps the context
                    // current through its notifier.
                    unsafe { gpu.ctx.report_swap() };
                    // Here rather than in `frame`, because here the interface
                    // is already drawn over the video and the picture is the
                    // whole thing. Does nothing unless asked.
                    if let Some(ui) = self.ui.upgrade() {
                        let size = ui.window().size();
                        // Before the picture is taken, so all of it is in it.
                        // First the soft ends of any list with rows past them,
                        // on the frame Slint has just drawn...
                        fades::collect(&ui, &mut self.scratch.fades, &mut self.scratch.ways);
                        if let Some(fader) = gpu.overlays.fader.as_mut() {
                            fader.run(
                                &gpu.gl,
                                gpu.pipeline.output(),
                                (size.width, size.height),
                                &self.scratch.fades,
                            );
                        }
                        // ...then the angle back to a list's current row, whole
                        // over the edge it sits in, on its glass disc if it has
                        // one. Nothing Slint draws moves with its circle, so
                        // this asks for the frames it eases in over.
                        if dbm_gpu::way::GLASS {
                            let panes: Vec<_> = self
                                .scratch
                                .ways
                                .iter()
                                .filter(|w| w.on > 0.004)
                                .map(dbm_gpu::way::Way::pane)
                                .collect();
                            gpu.pipeline
                                .glass_over(&gpu.gl, (size.width, size.height), &panes);
                        }
                        if let Some(way) = gpu.overlays.way.as_mut() {
                            way.run(&gpu.gl, (size.width, size.height), &self.scratch.ways);
                            if self.scratch.ways.iter().any(|w| w.lit > 0.0 && w.lit < 1.0) {
                                ui.window().request_redraw();
                            }
                        }
                        // ...then the shadow, cast by what is left, so a row
                        // cut off at a list's edge casts nothing past it...
                        if let Some(shadow) = gpu.overlays.shadow.as_mut() {
                            shadow.run(
                                &gpu.gl,
                                gpu.pipeline.output(),
                                (size.width, size.height),
                                &self.scratch.shaded,
                                ui.window().scale_factor(),
                            );
                        }
                        // ...and last the window's corners, cut from all of it.
                        if let Some(corners) = gpu.overlays.corners.as_mut() {
                            // At the system's size, not the interface's, since
                            // they are the desktop's corners.
                            let radius = crate::platform::window::corner_radius(ui.window());
                            corners.run(
                                &gpu.gl,
                                (size.width, size.height),
                                radius * self.app.scale.system(&ui),
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
                    gpu.pipeline.release(&gpu.gl);
                    gpu.overlays.release(&gpu.gl);
                }
            }
            _ => {}
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
        self.failure = Some(failure);
        if let Some(ui) = self.ui.upgrade() {
            self.say_failure(&ui);
        }
    }

    /// Put the failure on the way in, in the language being spoken.
    fn say_failure(&self, ui: &MainWindow) {
        let Some(failure) = &self.failure else {
            return;
        };
        // Nothing renders after a failure, so the ring has only its wash.
        ui.global::<crate::Paint>().set_glass(false);
        ui.set_fatal(tr(NO_VIDEO).into());
        ui.set_fatal_detail(fill(tr(failure.detail), &[&failure.cause]).into());
        ui.set_fatal_note(tr(failure.note).into());
    }

    fn setup(&mut self, api: &GraphicsAPI) {
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
                    cause: e,
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
        // Events are drained on the UI thread at video rate; this wakeup only
        // has to cover the paused case, where no frames are coming and an
        // event would otherwise sit unnoticed.
        self.app.mpv.set_wakeup(self.waker());

        match unsafe { dbm_mpv::RenderContext::new(self.app.mpv.clone(), get_proc_address, notify) }
        {
            Ok(ctx) => {
                log::info!("mpv render context created on Slint's GL context");
                // The two panels whose words are the player's own, drawn
                // unseen once now there is something to draw them with.
                self.warm.ask(crate::Panel::Settings);
                self.warm.ask(crate::Panel::Files);
                // Open only now. With `vo=libmpv` there is no video output
                // until the render context exists, so a file loaded earlier
                // starts against a VO that cannot present and never produces
                // a frame.
                //
                // The scan runs on the worker: a directory of thousands, or
                // one on a network share, would otherwise stall startup.
                if let Some(path) = self.file.take() {
                    self.app.worker.submit(move |_mpv| {
                        let path = crate::platform::real_place(&path);
                        Some(Completion::Opened(playlist::prepare(&path)))
                    });
                } else {
                    // Nothing named, so the empty window is what is coming.
                    completions::look_for_resume(&self.app.worker);
                }
                self.gpu = Some(Gpu {
                    overlays: Overlays::new(&gl),
                    ctx,
                    gl,
                    pipeline,
                    published: None,
                });
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
    /// The GPU state is taken out of the driver for the length of it and put
    /// back at the end, so each step can borrow the rest of the driver freely
    /// while the pipeline is held.
    fn frame(&mut self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let Some(mut gpu) = self.gpu.take() else {
            // With no render context, the failure is all there is to say
            // again in a language just chosen.
            let changes = dbm_lang::changes();
            if changes != self.language_seen {
                self.language_seen = changes;
                self.say_failure(&ui);
            }
            return;
        };
        self.hooks.install(&ui);

        let t = self.diag.begin();
        let moved = self.take_events(&ui);
        self.follow(&ui, moved);
        let t = self.diag.mark_events(t);

        self.background(&ui, &mut gpu);
        let t = self.diag.mark_lists(t);

        glass::collect(&ui, &mut self.scratch.panels);
        let t = self.diag.mark_panels(t);
        self.diag.panels_changed(&self.scratch.panels);
        self.warm.frame(&ui);
        self.app.scale.frame(&ui, self.app.settings.scale());

        self.draw(&ui, &mut gpu, t);
        self.gpu = Some(gpu);
    }

    /// Drain mpv's events into the mirrored state, and push out what moved.
    /// Returns whether any of it did.
    fn take_events(&mut self, ui: &MainWindow) -> bool {
        let playback = ui.global::<Playback>();
        let moved = events::drain(
            &self.app.mpv,
            &mut self.player,
            &mut self.scratch.replies,
            &mut self.scratch.notices,
            &self.app.audio,
        );
        // A failure marks its playlist row, which the list otherwise only
        // hears about on the next read.
        let failed = !self.scratch.notices.is_empty();
        if failed {
            self.stumbled = true;
            self.panel.request(&self.player, &self.app.worker);
        }
        for notice in std::mem::take(&mut self.scratch.notices) {
            // A notice means something concluded, which is the backstop for
            // an open that never produces a file: a container mpv cannot
            // read raises its `EndFile` failure here and `has_file` never
            // turns true, so without this the line would say "Opening…"
            // until the next successful one.
            ui.global::<Playlist>().set_opening(false);
            warn(ui, notice);
        }
        if moved {
            sync::push_scalars(ui, &self.player);
        }
        // Autoplay moving on from a file that played to its end: the next
        // one starts over, with its saved place offered for a while.
        if let Some(entry) = self.player.played_out.take() {
            let next = self
                .player
                .playlist
                .iter()
                .position(|e| e.id == entry)
                .and_then(|row| self.player.playlist.get(row + 1))
                .map(|e| e.filename.clone());
            if let Some(next) = next {
                self.app.arrival.played_out(&self.app.mpv, &next);
            }
        }
        // A file held on its saved place, for the centre pill to ask about.
        self.app.arrival.follow(
            &self.app.mpv,
            self.player.playlist_pos,
            self.player.paused,
            self.player.loads,
            self.player.path.as_ref(),
        );
        let held = self.app.arrival.asking(
            self.player.path.as_ref(),
            self.player.time_pos,
            self.player.paused,
        );
        playback.set_arriving(held.is_some());
        if let Some(from) = held {
            playback.set_arrival_from(format::time(from).into());
        }
        let offered = self
            .app
            .arrival
            .offered(self.player.path.as_ref(), self.player.time_pos);
        playback.set_resume_offer(offered.map(format::time).unwrap_or_default().into());
        let back = self
            .app
            .retrace
            .offered(self.player.path.as_ref(), self.player.time_pos);
        playback.set_back_offer(back.map(format::time).unwrap_or_default().into());
        let corner_free =
            held.is_none() && offered.is_none() && back.is_none() && !self.app.arrival.holding();
        self.skip(corner_free);
        playback.set_credits_count(self.skips.counted(&self.player));
        // An open is over when mpv finishes opening a file, not when it has
        // one, which is already true while a film plays. Failures end it their
        // own ways: a scan that found nothing where the scan reports back, and
        // a file mpv couldn't play by its notice, above.
        if self.player.loads != self.loads_seen {
            self.loads_seen = self.player.loads;
            self.stumbled = false;
            ui.global::<Playlist>().set_opening(false);
        }
        // Back on the empty window after an open that came to nothing, which
        // offers what it would have at launch rather than the bare fill.
        if self.stumbled && self.player.idle && self.player.path.is_none() {
            self.stumbled = false;
            completions::look_for_resume(&self.app.worker);
        }
        for id in std::mem::take(&mut self.scratch.replies) {
            if id == commands::REPLY_LOADLIST
                && let Some(m3u) = self.lists_loading.pop_front()
            {
                self.delete_list(m3u);
            }
            match id {
                commands::REPLY_SCRUB => self.app.scrubber.on_reply(&self.app.mpv),
                // The list exists now, and which entry plays was settled
                // before it loaded (see `commands::load_list`), so this only
                // makes sure it plays. Unless it is to be held on its saved
                // place or started over, which is settled once it is in.
                commands::REPLY_LOADLIST if self.loading_list => {
                    self.loading_list = false;
                    if !self.app.arrival.holding() {
                        commands::set_pause(&self.app.mpv, false);
                    }
                }
                _ => {}
            }
        }
        // mpv goes on from a file that wouldn't play as from one that ended,
        // but with autoplay off nothing starts that wasn't asked for: the
        // next file waits, paused, under the notice saying why. After the
        // replies, so a list's own start can't undo it.
        if failed && !self.app.settings.is_on(Toggle::Autoplay) {
            self.loading_list = false;
            commands::set_pause(&self.app.mpv, true);
        }
        moved
    }

    /// Skip the titles or the credits, where the person asked for that and
    /// one is playing (see `skips`).
    fn skip(&mut self, corner_free: bool) {
        let settings = &self.app.settings;
        let wanted = Wanted {
            intros: settings.is_on(Toggle::SkipIntros),
            credits: settings.is_on(Toggle::SkipCredits) && settings.is_on(Toggle::Autoplay),
        };
        let mpv = &self.app.mpv;
        match self
            .skips
            .follow(&self.player, wanted, corner_free, Instant::now())
        {
            Some(Skip::Intro { from, to }) => {
                commands::seek_to(mpv, to);
                if let Some(path) = &self.player.path {
                    self.app.retrace.skipped(path, from, to);
                }
            }
            Some(Skip::Scene(at)) => commands::seek_to(mpv, at),
            Some(Skip::Next) => self.app.arrival.go_on(mpv),
            None => {}
        }
    }

    /// Bring everything that follows the player up to date with it: the OS
    /// media controls, the display's wake lock, and the subtitle line.
    fn follow(&mut self, ui: &MainWindow, moved: bool) {
        self.hooks.follow(ui, &self.player, moved);
        // A few reads and a comparison while the bar is still; a command a
        // frame only while it fades. See `subline`.
        let bitmaps = tracks::selected(&self.player.tracks, TrackKind::Sub)
            .and_then(|track| track.codec.as_deref())
            .is_some_and(tracks::is_pictures);
        self.app.subline.follow(
            &self.app.mpv,
            Picture::from_osd(self.player.osd),
            bitmaps,
            f64::from(ui.get_subtitle_floor()),
            f64::from(ui.get_subtitle_lift()),
        );
        if let Some(own) = self.app.subline.readout() {
            ui.global::<Subtitles>()
                .set_pos_text(format::position(own).into());
        }
    }
}
