//! The handles the parts of the player share.
//!
//! One value rather than a parameter apiece, since the interface's callbacks,
//! the frame loop and the harness each need several. It is one `Rc`, so
//! cloning an `App` is cheap and a closure can take the whole thing. The few
//! fields handed out on their own are reference-counted themselves.
//!
//! UI thread only, like everything that holds one: the `Rc`s are not `Send`.
//! The worker and the bus threads are given the `Arc<Mpv>` alone.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use dbm_library::preview::Preview;
use dbm_mpv::{MediaPath, Mpv};

use crate::interface::chrome::Activity;
use crate::interface::find::Search;
use crate::interface::scale::Scale;
use crate::interface::subline::Subline;
use crate::playback::arrival::Arrival;
use crate::playback::audio::Watchdog;
use crate::playback::retrace::Retrace;
use crate::playback::scrub::Scrubber;
use crate::settings::Store;
use crate::worker::Worker;

#[derive(Clone)]
pub struct App(Rc<Shared>);

impl App {
    pub fn new(shared: Shared) -> Self {
        Self(Rc::new(shared))
    }
}

impl std::ops::Deref for App {
    type Target = Shared;

    fn deref(&self) -> &Shared {
        &self.0
    }
}

pub struct Shared {
    /// `Arc` for two reasons: its address must stay put, because the render
    /// context keeps a raw pointer to the symbol table inside it; and the
    /// worker thread needs a share of it.
    pub mpv: Arc<Mpv>,
    /// Anything that would block the frame path goes here.
    pub worker: Rc<Worker>,
    /// What the person has set: read by the frame loop, written by the
    /// settings page, saved by its `Persister`.
    pub settings: Rc<Store>,
    /// When a hand was last on the controls.
    pub activity: Activity,
    /// Timeline drags: the callbacks feed it positions, the frame loop the
    /// replies that let the next seek go out.
    pub scrubber: Scrubber,
    /// What answering a hover over the timeline needs: the frame loop learns
    /// it, the callbacks answer the pointer from it.
    pub preview: Preview,
    /// Puts audio back when its device returns. Hears every mpv event, and
    /// about the one choice it must not undo.
    pub audio: Rc<Watchdog>,
    /// Where subtitles sit. The callbacks move the line when the person
    /// does, the frame loop when the bar does, and both go through here.
    pub subline: Subline,
    /// The search for subtitles: the callbacks start it and read a pressed
    /// row back, the frame loop hands it the answers.
    pub search: Search,
    /// Arriving at a file with a place saved in it: the callbacks ask for
    /// files through it, the frame loop tells it what the playlist knows and
    /// when a file has loaded.
    pub arrival: Arrival,
    /// A seek the pointer made: the callbacks tell it where the film was and
    /// where it went, the frame loop asks whether the way back is offered.
    pub retrace: Retrace,
    /// The interface's size: the settings choose it, the frame loop keeps it
    /// on top of whatever the system sets.
    pub scale: Scale,
    /// The file playing: the player sets it as it follows a new one, the
    /// callbacks read it.
    pub playing: Playing,
}

/// The file playing, as mpv reported it, for a callback that has no player
/// to ask. Kept as the path itself rather than read back from the
/// interface's text, which would make a path out of any string.
#[derive(Default)]
pub struct Playing(RefCell<Option<MediaPath>>);

impl Playing {
    pub fn get(&self) -> Option<MediaPath> {
        self.0.borrow().clone()
    }

    pub fn set(&self, path: Option<MediaPath>) {
        *self.0.borrow_mut() = path;
    }
}
