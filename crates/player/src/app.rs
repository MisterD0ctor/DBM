//! The handles the parts of the player share.
//!
//! One value rather than a parameter apiece, since the interface's callbacks,
//! the frame loop and the harness each need several. Every field is a
//! reference-counted handle, so cloning an `App` is cheap and a closure can
//! take the whole thing.
//!
//! UI thread only, like everything that holds one: the `Rc`s are not `Send`.
//! The worker and the bus threads are given the `Arc<Mpv>` alone.

use std::rc::Rc;
use std::sync::Arc;

use crate::interface::chrome::Activity;
use crate::interface::subline::Subline;
use crate::library::opensubtitles::Hit;
use crate::library::preview::Preview;
use crate::playback::arrival::Arrival;
use crate::playback::audio::Watchdog;
use crate::playback::mpv::Mpv;
use crate::playback::scrub::Scrubber;
use crate::settings::Store;
use crate::worker::Worker;

#[derive(Clone)]
pub struct App {
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
    pub scrubber: Rc<Scrubber>,
    /// What answering a hover over the timeline needs: the frame loop learns
    /// it, the callbacks answer the pointer from it.
    pub preview: Rc<Preview>,
    /// Puts audio back when its device returns. Hears every mpv event, and
    /// about the one choice it must not undo.
    pub audio: Rc<Watchdog>,
    /// Where subtitles sit. The callbacks move the line when the person
    /// does, the frame loop when the bar does, and both go through here.
    pub subline: Rc<Subline>,
    /// What the last search on OpenSubtitles found, in the order its rows
    /// are drawn: the frame loop puts it here, a pressed row reads it back.
    pub found: Rc<std::cell::RefCell<Vec<Hit>>>,
    /// Arriving at a file with a place saved in it: the callbacks ask for
    /// files through it, the frame loop tells it what the playlist knows and
    /// when a file has loaded.
    pub arrival: Rc<Arrival>,
}
