//! Somewhere to put work that must not touch the UI thread.
//!
//! The rule this exists to enforce: **nothing on the frame path may block.**
//! A blocking mpv property read can cost 200ms during a resize, an exact seek
//! as much per drag update, and scanning a folder far more.
//!
//! So: one background thread, a queue of jobs in, a queue of completions out.
//! Jobs get a `&Reader`, the handle to mpv whose reads wait on its core,
//! which the UI thread is never given, and may block as long as they like. Each completion
//! wakes the player, which takes it in on the UI thread whether or not a
//! frame is coming (see `player::wake`).
//!
//! Completions are an enum rather than a boxed `Any` so that every kind of
//! background work is visible in one place, and adding one is a compile
//! error until it is handled.

use std::sync::mpsc::{self, Receiver, Sender, TryIter};
use std::thread::JoinHandle;

use dbm_library::threads::Reporter;
use dbm_mpv::{MediaPath, Reader};

/// Work that finished. Add a variant per kind of background job.
pub enum Completion {
    Lists(crate::playback::state::Lists),
    /// A path opened and scanned into a playlist, or why it could not be.
    Opened(Result<dbm_library::playlist::Prepared, String>),
    /// Something the person watching should be told, raised from a background
    /// job, such as the audio watchdog saying sound is back.
    Notice(String),
    /// Something that did not happen, raised from a background job: said the
    /// same way, with the warning mark before it.
    Warning(String),
    /// Playlist entries described by `probe` without being played: their
    /// lengths and titles. Several at once when they came from the cache.
    Probed(Vec<dbm_library::probe::Found>),
    /// What the empty window can offer to pick back up, looked for once at
    /// startup when nothing was named on the command line.
    Resume(Option<dbm_library::resume::Resume>),
    /// A seek-preview atlas, built or found for the file playing, and already
    /// decoded into Slint's pixels. Reported from the build's own thread (see
    /// `preview`).
    Preview {
        sprite: dbm_library::preview::Sprite,
        pixels: slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    },
    /// The light behind the empty window when no film can give it one (see
    /// `mark`).
    Mark(dbm_library::preview::Still),
    /// The light the end of `from` stands in, where its atlas has a frame to
    /// give it (see `preview::lit_still`).
    EndLight {
        from: MediaPath,
        still: Option<dbm_library::preview::Still>,
    },
    /// Where the way on goes once a list has run out, already named. `from`
    /// is the file that asked, so an answer that arrives after something
    /// else started is dropped, and `None` means there is nowhere to go.
    Onward {
        from: MediaPath,
        to: Option<dbm_library::onward::Onward>,
    },
    /// The timing that puts the selected subtitles over the film's speech,
    /// or why there is none. Reported from the run's own thread (see
    /// `subsync`).
    Synced(dbm_library::subsync::Outcome),
    /// Where the playing episode's title sequence is, or its closing
    /// credits, found by listening or remembered from the last time.
    /// Reported from the run's own thread (see `titles`).
    Titles(dbm_library::titles::Outcome),
    /// The picture inside a file's frame, where the frame carries black bars
    /// around it. Reported from the run's own thread (see `bars`).
    Bars(dbm_library::bars::Found),
    /// The playlist laid out in its parts, for the panel to show (see
    /// `playlist_panel`).
    Shelved(dbm_library::shelf::Shelf),
    /// What OpenSubtitles holds for a file, or why it couldn't say. From the
    /// search's own thread (see `opensubtitles`).
    Found(dbm_library::opensubtitles::Found),
    /// A subtitle fetched from there and kept, or why it was not.
    Fetched(dbm_library::opensubtitles::Fetched),
    /// Who signed in to OpenSubtitles, or why they could not. From the
    /// sign-in's own thread (see `opensubtitles::account`).
    SignedIn(
        Result<dbm_library::opensubtitles::account::Account, dbm_library::opensubtitles::Failure>,
    ),
    /// Subtitles fetched for a file on an earlier day, to put back as it
    /// opens. `path` is the file, so an answer for one since replaced is
    /// dropped.
    Kept {
        path: MediaPath,
        files: Vec<(std::path::PathBuf, String)>,
    },
    /// Seasons of the playing show found in folders beside its own, for the
    /// playlist panel. `paths` is the list they were looked for beside, so an
    /// answer for a list since replaced is dropped.
    Beside {
        paths: Vec<MediaPath>,
        seasons: Vec<dbm_library::shelf::Beside>,
    },
}

type Job = Box<dyn FnOnce(&Reader) -> Option<Completion> + Send + 'static>;

pub struct Worker {
    jobs: Option<Sender<Job>>,
    results: Receiver<Completion>,
    thread: Option<JoinHandle<()>>,
    /// Kept so other threads can report through the same queue (see
    /// [`Worker::reporter`]).
    reply: Sender<Completion>,
}

impl Worker {
    pub fn spawn(mpv: Reader) -> Self {
        let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
        let (results_tx, results_rx) = mpsc::channel::<Completion>();
        let reply = results_tx.clone();

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
                    crate::player::wake();
                }
            })
            .expect("spawning the worker thread");

        Self {
            jobs: Some(jobs_tx),
            results: results_rx,
            thread: Some(thread),
            reply,
        }
    }

    /// Queue work with no result to report, such as a blocking read followed
    /// by commands.
    pub fn run(&self, job: impl FnOnce(&Reader) + Send + 'static) -> bool {
        self.submit(move |mpv| {
            job(mpv);
            None
        })
    }

    /// Queue work. Returns whether it was accepted; `false` means the worker
    /// is shutting down.
    pub fn submit(&self, job: impl FnOnce(&Reader) -> Option<Completion> + Send + 'static) -> bool {
        self.jobs
            .as_ref()
            .is_some_and(|tx| tx.send(Box::new(job)).is_ok())
    }

    /// Everything that finished since the last call. Never blocks.
    pub fn drain(&self) -> TryIter<'_, Completion> {
        self.results.try_iter()
    }

    /// A handle another thread can report through.
    ///
    /// Some background work shouldn't queue behind the worker (a large
    /// folder's scan would hold the track menu empty until it finished), but
    /// its results still belong in the one enum the player drains, which
    /// `wrap` puts them in.
    pub fn reporter<T: 'static>(&self, wrap: fn(T) -> Completion) -> Reporter<T> {
        let reply = self.reply.clone();
        Reporter::new(move |value| {
            let delivered = reply.send(wrap(value)).is_ok();
            if delivered {
                crate::player::wake();
            }
            delivered
        })
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
