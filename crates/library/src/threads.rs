//! Threads of the player's own: each named, so a debugger's thread list says
//! which job it is doing, and called off through a [`Supersede`] when a newer
//! request makes its work pointless.

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;

use dbm_mpv::MediaPath;

/// `std::thread::spawn`, under a name. Linux keeps only its first 15 bytes.
///
/// Panics where `std::thread::spawn` does: when the system has no thread to
/// give.
pub fn spawn<F, T>(name: &str, f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(f)
        .expect("failed to spawn thread")
}

/// Lets a newer request call off background work begun for an older one.
///
/// Each request takes a [`Ticket`], and a run checks between its steps that
/// its ticket is still the newest, stopping when it is not. A `static`, so
/// the threads it governs need nothing handed to them but their ticket,
/// which is why `new` is `const`.
#[derive(Default)]
pub struct Supersede(AtomicU64);

impl Supersede {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Call off every run in progress, and return the ticket for the next.
    pub fn next(&'static self) -> Ticket {
        Ticket {
            of: self,
            number: self.0.fetch_add(1, Ordering::SeqCst) + 1,
        }
    }
}

/// One request's place in its [`Supersede`].
#[derive(Clone, Copy)]
pub struct Ticket {
    of: &'static Supersede,
    number: u64,
}

impl Ticket {
    /// Whether no newer request has been made since this one.
    pub fn current(self) -> bool {
        self.of.0.load(Ordering::SeqCst) == self.number
    }

    /// Unique among its `Supersede`'s tickets this run.
    pub fn number(self) -> u64 {
        self.number
    }
}

/// A job's answer about one file, which only means anything while that file
/// plays.
///
/// The answer is only handed over against the file playing now, so one that
/// arrives after the file has changed can't be taken for the next one's.
#[derive(Debug, Clone, PartialEq)]
pub struct ForFile<T> {
    path: MediaPath,
    answer: T,
}

impl<T> ForFile<T> {
    pub fn new(path: MediaPath, answer: T) -> Self {
        Self { path, answer }
    }

    pub fn path(&self) -> &MediaPath {
        &self.path
    }

    /// The answer, if `playing` is still the file it is about.
    pub fn if_playing(self, playing: Option<&MediaPath>) -> Option<T> {
        (playing == Some(&self.path)).then_some(self.answer)
    }
}

/// Where a job on a thread of its own hands back what it found.
///
/// The job needs to know nothing of who listens: the worker builds each one
/// to wrap the value into the message the frame drains, and to wake a paused
/// loop so it is noticed.
pub struct Reporter<T>(Box<dyn Fn(T) -> bool + Send>);

impl<T> Reporter<T> {
    pub fn new(deliver: impl Fn(T) -> bool + Send + 'static) -> Self {
        Self(Box::new(deliver))
    }

    /// Deliver. `false` once nobody is listening, as when the player is
    /// shutting down, which is the job's cue to stop.
    pub fn send(&self, value: T) -> bool {
        (self.0)(value)
    }
}
