//! Exercises written as straight-line code, one step after another, on
//! Slint's own event loop.
//!
//! An exercise is an `async` block that `run` hands to `slint::spawn_local`.
//! It waits between steps with a [`Beat`], which keeps the steady period a
//! repeated timer had, or with [`sleep`]. A step can be put in or taken out
//! without renumbering anything.
//!
//! An exercise that knows what it should see keeps a [`Verdict`]. A run with
//! a failed check, or with an exercise that never got to its end, exits with
//! a failure (see [`failed`]), which is what lets a job run them unwatched.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use crate::{MainWindow, Playback};

/// Run `exercise` on the event loop, with the window to drive. It is handed
/// its own strong handle, which it holds until it ends or the run does.
pub fn run<F>(ui: &MainWindow, exercise: impl FnOnce(MainWindow) -> F) -> Script
where
    F: Future<Output = ()> + 'static,
{
    let future = exercise(ui.clone_strong());
    match slint::spawn_local(future) {
        Ok(handle) => Script(Some(handle)),
        Err(e) => {
            log::warn!("harness exercise not started: {e}");
            Script(None)
        }
    }
}

/// An exercise under way, stopped when this is dropped, which is once the
/// window has closed.
pub struct Script(Option<slint::JoinHandle<()>>);

impl Drop for Script {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

/// Wait `duration` on the event loop.
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        until: Instant::now() + duration,
        rung: None,
    }
}

pub struct Sleep {
    until: Instant,
    /// Set once a timer is waiting to wake the script, by that timer.
    rung: Option<Rc<Rung>>,
}

#[derive(Default)]
struct Rung {
    done: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let now = Instant::now();
        if now >= self.until || self.rung.as_ref().is_some_and(|r| r.done.get()) {
            return Poll::Ready(());
        }
        match &self.rung {
            Some(rung) => *rung.waker.borrow_mut() = Some(cx.waker().clone()),
            None => {
                let rung = Rc::new(Rung::default());
                *rung.waker.borrow_mut() = Some(cx.waker().clone());
                let ring = rung.clone();
                slint::Timer::single_shot(self.until - now, move || {
                    ring.done.set(true);
                    if let Some(waker) = ring.waker.borrow_mut().take() {
                        waker.wake();
                    }
                });
                self.rung = Some(rung);
            }
        }
        Poll::Pending
    }
}

/// Wait until the window has the film: its length has come through from mpv.
/// A step taken sooner acts on nothing, and on software GL, at a few frames a
/// second, that can be seconds into the run.
pub async fn loaded(ui: &MainWindow) {
    while ui.global::<Playback>().get_duration() <= 0.0 {
        sleep(Duration::from_millis(100)).await;
    }
}

/// Steps a fixed period apart, counted from when the beat began, as a
/// repeated timer's are: a slow step doesn't push back every one after it.
pub struct Beat {
    period: Duration,
    next: Instant,
}

impl Beat {
    /// The first beat falls one `period` from now.
    pub fn new(period: Duration) -> Self {
        Self {
            period,
            next: Instant::now() + period,
        }
    }

    /// Wait for the next beat. One the loop was too busy to keep comes at
    /// once, and the beat after it a period on, as a timer's does: missed
    /// beats are skipped rather than run back to back.
    pub async fn next(&mut self) {
        let now = Instant::now();
        if self.next > now {
            sleep(self.next - now).await;
            self.next += self.period;
        } else {
            self.next = now + self.period;
        }
    }

    /// Let `beats` go by.
    pub async fn rest(&mut self, beats: u32) {
        for _ in 0..beats {
            self.next().await;
        }
    }
}

/// How long [`Verdict::expect`] waits for a step to come through. On one
/// core and software GL, sharing it with ffmpeg making the previews, a switch
/// of audio track takes over a second and a half to reach the window.
const GRACE: Duration = Duration::from_secs(5);

thread_local! {
    /// Checks failed in the run so far.
    static FAILED: Cell<u32> = const { Cell::new(0) };
    /// Exercises with a verdict that began and have not finished.
    static OPEN: Cell<u32> = const { Cell::new(0) };
}

/// What an exercise that knows what it should see found. Made at its start
/// and finished at its end; one cut off before its end fails the run.
pub struct Verdict {
    name: &'static str,
    failed: Cell<u32>,
}

impl Verdict {
    pub fn new(name: &'static str) -> Self {
        OPEN.set(OPEN.get() + 1);
        Self {
            name,
            failed: Cell::new(0),
        }
    }

    /// Say whether `what` holds, given up to [`GRACE`] to come true: a step
    /// it follows is checked for taking effect, not for how fast it did.
    pub async fn expect(&self, holds: impl Fn() -> bool, what: &str) {
        let until = Instant::now() + GRACE;
        while !holds() && Instant::now() < until {
            sleep(Duration::from_millis(100)).await;
        }
        self.check(holds(), what);
    }

    /// Say whether `what` held.
    pub fn check(&self, held: bool, what: &str) {
        eprintln!("dbm: {} {what}", if held { "pass" } else { "FAIL" });
        if !held {
            self.failed.set(self.failed.get() + 1);
            FAILED.set(FAILED.get() + 1);
        }
    }

    pub fn finish(self) {
        OPEN.set(OPEN.get() - 1);
        eprintln!(
            "dbm: --- {} test done, {} failed ---",
            self.name,
            self.failed.get()
        );
    }
}

/// Whether the run should end in failure: a check failed, or an exercise
/// with a verdict never reached its end.
pub fn failed() -> bool {
    if OPEN.get() > 0 {
        eprintln!("dbm: FAIL an exercise did not reach its end");
    }
    FAILED.get() > 0 || OPEN.get() > 0
}
