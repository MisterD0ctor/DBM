//! Somewhere to put work that must not touch the UI thread.
//!
//! The rule this exists to enforce: **nothing on the frame path may block.**
//! A blocking mpv property read can cost 200ms during a resize, an exact seek
//! as much per drag update, and scanning a folder far more.
//!
//! So: one background thread, a queue of jobs in, a queue of completions out.
//! Jobs get a `&Mpv` and may block as long as they like. Completions are
//! drained on the UI thread once per frame, and the loop is woken so a paused
//! player notices them promptly rather than waiting for the next video frame.
//!
//! Completions are an enum rather than a boxed `Any` so that every kind of
//! background work is visible in one place, and adding one is a compile
//! error until it is handled.

use std::sync::mpsc::{self, Receiver, Sender, TryIter};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::playback::mpv::Mpv;
use crate::MainWindow;

/// Work that finished. Add a variant per kind of background job.
pub enum Completion {
    Lists(crate::interface::sync::Lists),
    /// A path opened and scanned into a playlist, or why it could not be.
    Opened(Result<crate::library::playlist::Prepared, String>),
    /// Something the person watching should be told, raised from a background
    /// job, such as the audio watchdog saying sound is back.
    Notice(String),
    /// Something that did not happen, raised from a background job: said the
    /// same way, with the warning mark before it.
    Warning(String),
    /// Playlist entries described by `probe` without being played: their
    /// lengths and titles. Several at once when they came from the cache.
    Probed(Vec<crate::library::probe::Found>),
    /// What the empty window can offer to pick back up, looked for once at
    /// startup when nothing was named on the command line.
    Resume(Option<crate::library::resume::Resume>),
    /// A seek-preview atlas, built or found for the file playing, and already
    /// decoded. Reported from the build's own thread (see `preview`).
    Preview(crate::library::preview::Atlas),
    /// The light behind the empty window when no film can give it one (see
    /// `mark`).
    Mark(crate::library::preview::Still),
    /// The light the end of `from` stands in, where its atlas has a frame to
    /// give it (see `preview::lit_still`).
    EndLight {
        from: String,
        still: Option<crate::library::preview::Still>,
    },
    /// Where the way on goes once a list has run out, already named. `from`
    /// is the file that asked, so an answer that arrives after something
    /// else started is dropped, and `None` means there is nowhere to go.
    Onward {
        from: String,
        to: Option<crate::library::onward::Onward>,
    },
    /// The timing that puts the selected subtitles over the film's speech,
    /// or why there is none. Reported from the run's own thread (see
    /// `subsync`).
    Synced(crate::library::subsync::Outcome),
    /// Where the playing episode's title sequence is, or its closing
    /// credits, found by listening or remembered from the last time.
    /// Reported from the run's own thread (see `titles`).
    Titles(crate::library::titles::Outcome),
    /// The playlist laid out in its parts, for the panel to show (see
    /// `playlist_panel`).
    Shelved(crate::library::shelf::Shelf),
    /// What OpenSubtitles holds for a file, or why it couldn't say. From the
    /// search's own thread (see `opensubtitles`).
    Found(crate::library::opensubtitles::Found),
    /// A subtitle fetched from there and kept, or why it was not.
    Fetched(crate::library::opensubtitles::Fetched),
    /// Who signed in to OpenSubtitles, or why they could not. From the
    /// sign-in's own thread (see `opensubtitles::account`).
    SignedIn(
        Result<
            crate::library::opensubtitles::account::Account,
            crate::library::opensubtitles::Failure,
        >,
    ),
    /// Subtitles fetched for a file on an earlier day, to put back as it
    /// opens. `path` is the file, so an answer for one since replaced is
    /// dropped.
    Kept {
        path: String,
        files: Vec<(std::path::PathBuf, String)>,
    },
    /// Seasons of the playing show found in folders beside its own, for the
    /// playlist panel. `paths` is the list they were looked for beside, so an
    /// answer for a list since replaced is dropped.
    Beside {
        paths: Vec<String>,
        seasons: Vec<crate::library::shelf::Beside>,
    },
}

type Job = Box<dyn FnOnce(&Mpv) -> Option<Completion> + Send + 'static>;

pub struct Worker {
    jobs: Option<Sender<Job>>,
    results: Receiver<Completion>,
    thread: Option<JoinHandle<()>>,
    /// Kept so other threads can report through the same queue (see
    /// [`Reporter`]).
    reply: Sender<Completion>,
    waker: slint::Weak<MainWindow>,
}

/// A way for a thread of its own to hand results back as the worker does.
///
/// Some background work shouldn't queue behind the worker (a large folder's
/// scan would hold the track menu empty until it finished), but its results
/// still belong in the one enum drained each frame.
#[derive(Clone)]
pub struct Reporter {
    reply: Sender<Completion>,
    waker: slint::Weak<MainWindow>,
}

impl Reporter {
    /// Deliver, and wake a paused loop so it is noticed. Returns `false` once
    /// the player is shutting down, which is the sender's cue to stop.
    pub fn send(&self, completion: Completion) -> bool {
        if self.reply.send(completion).is_err() {
            return false;
        }
        let _ = self
            .waker
            .upgrade_in_event_loop(|ui| slint::ComponentHandle::window(&ui).request_redraw());
        true
    }
}

impl Worker {
    pub fn spawn(mpv: Arc<Mpv>, ui: slint::Weak<MainWindow>) -> Self {
        let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
        let (results_tx, results_rx) = mpsc::channel::<Completion>();
        let reply = results_tx.clone();
        let waker = ui.clone();

        let thread = std::thread::Builder::new()
            .name("dbm-worker".into())
            .spawn(move || {
                // Ends when the sender is dropped, which is how `Drop` below
                // asks the thread to stop.
                for job in jobs_rx {
                    let Some(completion) = job(&mpv) else {
                        continue;
                    };
                    if results_tx.send(completion).is_err() {
                        break;
                    }
                    // Nudge the loop, since while paused there are no frames
                    // to drain the completion.
                    let _ = ui.upgrade_in_event_loop(|ui| {
                        slint::ComponentHandle::window(&ui).request_redraw()
                    });
                }
            })
            .expect("spawning the worker thread");

        Self {
            jobs: Some(jobs_tx),
            results: results_rx,
            thread: Some(thread),
            reply,
            waker,
        }
    }

    /// Queue work with no result to report, such as a blocking read followed
    /// by commands.
    pub fn run(&self, job: impl FnOnce(&Mpv) + Send + 'static) -> bool {
        self.submit(move |mpv| {
            job(mpv);
            None
        })
    }

    /// Queue work. Returns whether it was accepted; `false` means the worker
    /// is shutting down.
    pub fn submit(&self, job: impl FnOnce(&Mpv) -> Option<Completion> + Send + 'static) -> bool {
        self.jobs
            .as_ref()
            .is_some_and(|tx| tx.send(Box::new(job)).is_ok())
    }

    /// Everything that finished since the last call. Never blocks.
    pub fn drain(&self) -> TryIter<'_, Completion> {
        self.results.try_iter()
    }

    /// A handle another thread can report through.
    pub fn reporter(&self) -> Reporter {
        Reporter {
            reply: self.reply.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Closing the job channel is what ends the loop in the thread.
        self.jobs = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
