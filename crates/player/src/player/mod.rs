//! What is playing, and everything that follows from it.
//!
//! mpv's events and the worker's answers come in here, and what the
//! interface, the media keys and the wake lock hear about playback goes out
//! from here. It runs whenever either has something to say (see [`wake`]),
//! not only when a frame is drawn: once mpv has nothing loaded it draws no
//! frames, and a redraw asked for then may never come. The driver runs it at
//! the start of every frame as well, which keeps it going while Windows holds
//! the event loop during a resize, and draws from what it holds.
//!
//! One run, in this order:
//!
//!   1. install the window hooks that are not up yet
//!   2. drain mpv's events into the mirrored state, and push what moved — see
//!      `events`
//!   3. bring what follows the player up to date: media keys, wake lock, the
//!      subtitle line
//!   4. ask for background work, and take delivery of what finished — see
//!      `background` and `completions`

mod background;
mod completions;
mod events;
mod feeds;
mod opening;

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use dbm_library::preview::Still;
use dbm_mpv::tracks::{self, TrackKind};
use dbm_mpv::{Events, MediaPath};
use slint::ComponentHandle;

use crate::app::App;
use crate::interface::subline::Picture;
use crate::interface::{format, playlist_panel, sync};
use crate::platform::modal_loop;
use crate::playback::bars::Bars;
use crate::playback::commands;
use crate::playback::skips::{Skip, Skips, Wanted};
use crate::playback::state::PlayerState;
use crate::settings::Toggle;
use crate::worker::{Completion, Opened};
use crate::{MainWindow, Playback, Subtitles};

use opening::Opening;

thread_local! {
    /// The player, for a wakeup from another thread to reach once it is on
    /// this one: what crosses threads has to be `Send`, which an `Rc` isn't.
    static PLAYER: RefCell<Weak<RefCell<Player>>> = const { RefCell::new(Weak::new()) };
}

/// Whether a run is already on its way to the UI thread, so a burst of
/// wakeups costs one.
static WOKEN: AtomicBool = AtomicBool::new(false);

/// Have the player take in what mpv or the worker has for it.
///
/// From any thread, mpv's own included, since all it does there is post to
/// the event loop.
pub fn wake() {
    if WOKEN.swap(true, Ordering::AcqRel) {
        return;
    }
    let posted = slint::invoke_from_event_loop(|| {
        WOKEN.store(false, Ordering::Release);
        let Some(player) = PLAYER.with(|player| player.borrow().upgrade()) else {
            return;
        };
        // Held only by a frame, which takes everything in itself.
        if let Ok(mut player) = player.try_borrow_mut() {
            player.pump();
        }
    });
    // Only once the loop has gone, when there is nothing left to tell; but
    // left set, it would swallow every wakeup after.
    if posted.is_err() {
        WOKEN.store(false, Ordering::Release);
    }
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

impl Hooks {
    /// Install whichever hooks are not up yet. Each needs the native window,
    /// which does not exist until the loop has turned, so each retries a
    /// run until it takes.
    fn install(&mut self, ui: &MainWindow) {
        self.modal.poll(ui.window());
        self.drops.poll(ui);
        self.rounding.poll(ui.window());
    }

    /// Tell the media controls and the wake lock what is playing.
    ///
    /// Every run, not only the ones that moved: registration needs a window
    /// handle that does not exist yet on the first of them, and the update
    /// itself is skipped unless something changed.
    fn follow(&mut self, ui: &MainWindow, player: &PlayerState, moved: bool) {
        self.media_keys.publish(ui.window(), player, moved);
        self.awake.follow(player);
    }
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

/// What was done once for the file playing, so it is not done again. Made
/// afresh whenever the file changes, nothing playing included (see
/// `follow_file`), so none of it can be about another file.
#[derive(Default)]
struct PerFile {
    /// The file it is all about, or `None` with nothing playing.
    path: Option<MediaPath>,
    /// Whether the way on was looked for, at its end or in its credits.
    onward_asked: bool,
    /// Whether its end was asked to be given a light (see `light_the_end`),
    /// and what lights it once one came.
    end_light_asked: bool,
    end_light: Option<EndLight>,
    /// Whether its title sequence and credits were listened for.
    titles_asked: bool,
    /// Whether the subtitles fetched for it before were put back, once it
    /// had opened.
    kept_asked: bool,
    /// Whether its frame was looked at for bars.
    bars_asked: bool,
}

/// What lights the end of a file, behind the glass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndLight {
    /// The most lit frame of the file itself.
    Frame,
    /// The player's mark, for want of a frame of the file's own.
    Mark,
}

pub struct Player {
    ui: slint::Weak<MainWindow>,
    app: App,
    /// mpv's event queue, which nothing else reads.
    events: Events,
    state: PlayerState,
    /// Has the playlist laid out on the worker, and shows what comes back.
    panel: playlist_panel::Panel,
    /// `lang::changes` as of the last run, so words already handed to the
    /// interface are said again in a language just chosen.
    language_seen: usize,
    /// How much is left of the film the way in offers, kept so that line can
    /// be said again too.
    resume_left: Option<f64>,
    /// The panels to draw unseen before they are opened.
    warm: crate::interface::warm::WarmUp,
    /// Last seen state of the playlist panel, so its opening can be noticed.
    playlist_open: bool,
    opening: Opening,
    hooks: Hooks,
    feeds: Feeds,
    per_file: PerFile,
    skips: Skips,
    bars: Bars,
    /// A picture for behind the glass, waiting for the driver to hand it to
    /// the GPU, which only it holds.
    backdrop: Option<Still>,
}

impl Player {
    /// Make the player, and have mpv's wakeups reach it.
    pub fn new(
        ui: slint::Weak<MainWindow>,
        app: App,
        events: Events,
        media_keys: crate::platform::media_keys::Controls,
    ) -> Rc<RefCell<Self>> {
        app.mpv.set_wakeup(Box::new(wake));
        let player = Rc::new(RefCell::new(Self {
            ui,
            app,
            events,
            state: PlayerState::default(),
            panel: playlist_panel::Panel::default(),
            language_seen: 0,
            resume_left: None,
            warm: crate::interface::warm::WarmUp::default(),
            playlist_open: false,
            opening: Opening::default(),
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
            backdrop: None,
        }));
        PLAYER.with(|slot| *slot.borrow_mut() = Rc::downgrade(&player));
        player
    }

    /// Begin, now video can be shown: open what was named on the command
    /// line, or look for what the empty window can offer.
    ///
    /// Not before. With `vo=libmpv` there is no video output until the render
    /// context exists, so a file loaded earlier starts against a VO that
    /// cannot present and never produces a frame.
    pub fn start(&mut self, file: Option<std::path::PathBuf>) {
        // The two panels whose words are the player's own, drawn unseen once
        // now there is something to draw them with.
        self.warm.ask(crate::Panel::Settings);
        self.warm.ask(crate::Panel::Files);
        match file {
            // The scan runs on the worker: a directory of thousands, or one on
            // a network share, would otherwise stall startup.
            Some(path) => {
                self.app.worker.submit(move |_mpv| {
                    let path = crate::platform::real_place(&path);
                    Some(Completion::Opened(
                        dbm_library::playlist::prepare(&path)
                            .map(|list| Opened { list, ask: true }),
                    ))
                });
            }
            None => completions::look_for_resume(&self.app.worker),
        }
    }

    /// One whole run, for a wakeup. A frame runs the halves itself, timing
    /// each.
    fn pump(&mut self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.hear(&ui);
        self.background(&ui);
        // What was taken in may change what the frame draws: the video's
        // place, the backdrop, the light at the end.
        ui.window().request_redraw();
    }

    /// Steps 1 to 3: the hooks, mpv's events, and what follows the player.
    pub fn hear(&mut self, ui: &MainWindow) {
        self.hooks.install(ui);
        crate::interface::actions::follow_fullscreen(ui);
        let moved = self.take_events(ui);
        self.follow(ui, moved);
    }

    /// Write the playing file's place and wait for mpv to have, as the
    /// player quits (see `session`).
    pub fn save_before_quitting(&mut self) {
        crate::playback::session::save_before_quitting(&mut self.events, &self.app.mpv);
    }

    /// What the frame draws from.
    pub fn state(&self) -> &PlayerState {
        &self.state
    }

    /// A picture for behind the glass, if one has come since the last frame.
    pub fn take_backdrop(&mut self) -> Option<Still> {
        self.backdrop.take()
    }

    /// Whether the file has ended and its end has been given a light, which
    /// the backdrop shows in place of the film's black.
    pub fn end_lit(&self) -> bool {
        self.state.eof_reached
            && self.per_file.path == self.state.path
            && self.per_file.end_light.is_some()
    }

    /// Draw the panels waiting to be drawn unseen. A frame's work, since
    /// drawing them is what warms them.
    pub fn warm(&mut self, ui: &MainWindow) {
        self.warm.frame(ui);
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
            .follow(&self.state, wanted, corner_free, Instant::now())
        {
            Some(Skip::Intro { from, to }) => {
                commands::seek_to(mpv, to);
                if let Some(path) = &self.state.path {
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
        self.hooks.follow(ui, &self.state, moved);
        // A few reads and a comparison while the bar is still; a command a
        // run only while it fades. See `subline`.
        let bitmaps = tracks::selected(&self.state.tracks, TrackKind::Sub)
            .and_then(|track| track.codec.as_deref())
            .is_some_and(tracks::is_pictures);
        self.app.subline.follow(
            &self.app.mpv,
            Picture::from_osd(self.state.osd),
            bitmaps,
            f64::from(ui.get_subtitle_floor()),
            f64::from(ui.get_subtitle_lift()),
        );
        if let Some(own) = self.app.subline.readout() {
            ui.global::<Subtitles>()
                .set_pos_text(format::position(own).into());
        }
        ui.global::<Playback>().set_end_lit(self.end_lit());
    }
}
