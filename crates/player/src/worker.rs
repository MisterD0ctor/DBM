//! Somewhere to put work that must not touch the UI thread.
//!
//! The rule this exists to enforce: **nothing on the frame path may block.**
//! Two separate incidents have come from breaking it — six
//! `mpv_get_property_string` calls per frame cost 220ms each during a resize,
//! and a synchronous exact seek cost 195ms per drag update. Reading the track
//! list is the same shape (~80ms on load), and scanning a folder will be far
//! worse.
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

use crate::mpv::Mpv;
use crate::tracks::{PlaylistEntry, Track};
use crate::MainWindow;

/// Track and playlist contents, read together because they are invalidated
/// together and a single job means a single round trip.
pub struct Lists {
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// One per playlist entry, in the same order: how long it is and how far
    /// into it the resume point sits. Read here rather than in a job of its
    /// own because it is answered *from* the playlist — the paths have to be
    /// known before the lookup can start, and they are known right here.
    pub progress: Vec<crate::durations::Progress>,
    /// The generation this was read for, so a stale result can be discarded
    /// if the lists moved again while the job was running.
    pub generation: u64,
}

/// A playlist that has been scanned and written out, ready to hand to mpv.
pub struct Playlist {
    /// The M3U on disk.
    pub m3u: std::path::PathBuf,
    /// Which entry to start on.
    pub start: usize,
    pub count: usize,
}

/// Work that finished. Add a variant per kind of background job.
pub enum Completion {
    Lists(Lists),
    Opened(Result<Playlist, String>),
    /// Something the person watching should be told, raised from a background
    /// job. The audio watchdog is the first caller: it repairs the audio chain
    /// silently, and silence is exactly what made the original fault so
    /// baffling.
    Notice(String),
    /// Playlist entries described by `probe` without being played: their
    /// lengths and titles. Several at once when they came from the cache.
    Probed(Vec<crate::probe::Found>),
    /// What the empty window can offer to pick back up, looked for once at
    /// startup when nothing was named on the command line.
    Resume(Option<crate::session::Resume>),
}

type Job = Box<dyn FnOnce(&Mpv) -> Option<Completion> + Send + 'static>;

pub struct Worker {
    jobs: Option<Sender<Job>>,
    results: Receiver<Completion>,
    thread: Option<JoinHandle<()>>,
    /// Kept so threads that are not this one can report through the same
    /// queue — see [`Reporter`].
    reply: Sender<Completion>,
    waker: slint::Weak<MainWindow>,
}

/// A way for a thread of its own to hand results back as the worker does.
///
/// Some background work should not queue behind the worker — a scan of a
/// large folder would hold the track menu empty until it finished — but its
/// results still belong in the one enum drained once a frame, where every
/// kind of background work is handled in one place.
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
        let _ = self.waker.upgrade_in_event_loop(|ui| {
            slint::ComponentHandle::window(&ui).request_redraw()
        });
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
                    // Nudge the loop: while paused there are no frames, and
                    // the completion would otherwise sit until something
                    // else caused a redraw.
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

    /// Queue work with no result to report — a blocking read followed by
    /// commands, say. Convenience over `submit` returning `None`.
    pub fn run(&self, job: impl FnOnce(&Mpv) + Send + 'static) -> bool {
        self.submit(move |mpv| {
            job(mpv);
            None
        })
    }

    /// Queue work. Returns whether it was accepted — a `false` means the
    /// worker is shutting down.
    pub fn submit(
        &self,
        job: impl FnOnce(&Mpv) -> Option<Completion> + Send + 'static,
    ) -> bool {
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
