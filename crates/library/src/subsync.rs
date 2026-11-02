//! Lining subtitles up with the speech they belong to.
//!
//! Subtitles that came from somewhere else (a download, or a track muxed in
//! from another release) run early or late by a fixed amount, and sometimes
//! at another frame rate's speed. The audio says so: people talk while lines
//! are on screen and mostly not between them, so the right timing puts the
//! lines over the talking.
//!
//! No speech is recognised, so the language doesn't matter. The audio is
//! reduced to one number every hundredth of a second (how much sound there is
//! against its surroundings), and the subtitles to the spans they are on
//! screen. Every delay within a minute either way is tried, at each speed a
//! frame-rate mismatch produces, and scored by how much sound falls inside
//! the spans.
//!
//! ffmpeg does the decoding, so without it there is no sync (see `ffmpeg`).
//! It reads the film once: the audio comes back down a pipe, and a subtitle
//! track inside the same file is written out in the same pass, since getting
//! at either means reading the whole container.
//!
//! Its own thread, like the seek preview's, since a feature is a quarter of a
//! minute of decoding and the shared worker keeps the track lists current. A
//! [`Supersede`] abandons a run whose film was replaced or which was asked
//! for again.

use std::path::PathBuf;
use std::sync::Arc;

use dbm_mpv::Mpv;
use dbm_mpv::tracks;

use crate::ffmpeg;
use crate::threads::Reporter;
use crate::threads::{Supersede, Ticket};

mod cues;
mod fit;
mod listen;

use cues::{convert, cues, read_text};
use fit::search;
use listen::{PER_SECOND, is_silent, listen, speech};

/// How to play subtitles so they land on the speech: mpv's `sub-speed` and
/// `sub-delay`, which put a line timed at `t` on screen at `t × speed + delay`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub delay: f64,
    pub speed: f64,
}

/// Why there is no fit. Each is something to tell the person who asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Nothing is selected to sync.
    NoSubtitles,
    /// The track is pictures (a disc's own subtitles), with no text to take
    /// timings from. Those are cut with the film and are in step.
    Pictures,
    NoFfmpeg,
    /// The subtitles gave no timings.
    Unread,
    /// The audio gave nothing to measure against.
    Silent,
    /// No delay stood out from the rest: too little speech, too few lines,
    /// or subtitles for something else.
    Unsure,
}

/// A finished run, and what it was for, so an answer that arrives after the
/// film or the track has changed can be dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub path: String,
    pub sid: Option<String>,
    pub result: Result<Fit, Failure>,
}

/// Called on every request and on every new file.
static RUNS: Supersede = Supersede::new();

/// Abandon any run in progress. Called when the file changes.
pub fn supersede() {
    RUNS.next();
}

/// Sync the selected subtitles to the playing film's speech, off the UI
/// thread, and report the answer like any other background result.
///
/// Returns immediately. A run that is superseded reports nothing; every
/// other one reports, because the interface is showing that it is listening
/// and has to be told when to stop.
pub fn spawn(mpv: Arc<Mpv>, reporter: Reporter<Outcome>) {
    let ticket = RUNS.next();
    crate::threads::spawn("dbm-subsync", move || {
        // Read here rather than handed over, since these block on mpv's core
        // lock, which the UI thread must not.
        let path = mpv.get_property("path").unwrap_or_default();
        let sid = mpv.get_property("sid");
        let result = ask(&mpv, &path).and_then(|asked| run(&asked, ticket));
        if !ticket.current() {
            return;
        }
        match &result {
            Ok(fit) => log::info!(
                "subtitles fit at {:+.1} s, speed {:.4}",
                fit.delay,
                fit.speed
            ),
            Err(why) => log::warn!("subtitles not synced: {why:?}"),
        }
        reporter.send(Outcome { path, sid, result });
    });
}

/// Where the subtitles are.
#[derive(Debug, Clone, PartialEq)]
enum Source {
    /// A file of their own beside the film.
    File(PathBuf),
    /// A stream of the film's own file, by its index there.
    Stream(i64),
}

/// One run's worth of questions, answered by mpv.
#[derive(Debug, Clone, PartialEq)]
struct Asked {
    video: PathBuf,
    /// The audio stream's index in the film's file, or `None` for its first.
    audio: Option<i64>,
    /// Whether that stream has a centre channel to take the voices from.
    centre: bool,
    subtitles: Source,
}

fn ask(mpv: &Mpv, path: &str) -> Result<Asked, Failure> {
    if path.is_empty() || mpv.get_f64("current-tracks/sub/id").is_none() {
        return Err(Failure::NoSubtitles);
    }
    let codec = mpv
        .get_property("current-tracks/sub/codec")
        .unwrap_or_default();
    if tracks::is_pictures(&codec) {
        return Err(Failure::Pictures);
    }
    // A track from outside the film has to say which file it is: its index
    // is an index into that file, and would name some other stream of the
    // film's.
    let subtitles = if mpv.get_bool("current-tracks/sub/external") {
        mpv.get_property("current-tracks/sub/external-filename")
            .filter(|file| !file.is_empty())
            .map(|file| Source::File(PathBuf::from(file)))
            .ok_or(Failure::Unread)?
    } else {
        Source::Stream(
            mpv.get_f64("current-tracks/sub/ff-index")
                .ok_or(Failure::Unread)? as i64,
        )
    };
    // An audio track from a file of its own is not in the film's, so its
    // index means nothing there; the film's first is the next best thing,
    // and is the same performance in another mix or language.
    let own_audio = !mpv.get_bool("current-tracks/audio/external");
    let channels = mpv
        .get_f64("current-tracks/audio/demux-channel-count")
        .unwrap_or(0.0) as i64;
    Ok(Asked {
        video: PathBuf::from(path),
        audio: mpv
            .get_f64("current-tracks/audio/ff-index")
            .filter(|_| own_audio)
            .map(|i| i as i64),
        centre: own_audio && has_centre(channels),
        subtitles,
    })
}

/// Whether a track with this many channels can be relied on to have a centre.
///
/// Five and up always do (5.0, 5.1, 6.1, 7.1). Three and four may not, since
/// 2.1 and quad are real layouts, and asking ffmpeg for a missing channel
/// gets silence rather than an error.
fn has_centre(channels: i64) -> bool {
    channels >= 5
}

fn run(asked: &Asked, ticket: Ticket) -> Result<Fit, Failure> {
    let ffmpeg = ffmpeg::path().ok_or(Failure::NoFfmpeg)?;
    ffmpeg::announce(&ffmpeg);

    // A track inside the film is written out by the same ffmpeg that decodes
    // the audio, to a file that is gone again when this returns.
    let scratch = match asked.subtitles {
        Source::Stream(_) => Some(Scratch::new(ticket)),
        Source::File(_) => None,
    };
    let stream = match (&asked.subtitles, &scratch) {
        (Source::Stream(index), Some(scratch)) => Some((*index, scratch.0.as_path())),
        _ => None,
    };

    let mut loudness = listen(&ffmpeg, asked, asked.centre, stream, ticket);
    // The centre channel is where the voices are when there is one, and with
    // the music and the effects left in the others it is most of what makes
    // this work on a film. If it came back empty the track did not have one
    // after all, and everything mixed down is the fallback.
    if asked.centre && loudness.as_deref().is_some_and(is_silent) {
        loudness = listen(&ffmpeg, asked, false, stream, ticket);
    }
    let loudness = loudness.filter(|l| !is_silent(l)).ok_or(Failure::Silent)?;

    let text = match (&asked.subtitles, &scratch) {
        (Source::File(file), _) => read_text(file)
            .filter(|text| !cues(text).is_empty())
            .or_else(|| convert(&ffmpeg, file)),
        (Source::Stream(_), Some(scratch)) => read_text(&scratch.0),
        _ => None,
    };
    let cues = cues(&text.ok_or(Failure::Unread)?);
    if cues.is_empty() {
        return Err(Failure::Unread);
    }
    let best = search(&speech(&loudness), &cues).ok_or(Failure::Unsure)?;
    // Logged whether or not it is believed, so "it would not sync" has
    // something to go on.
    log::info!(
        "{} subtitle spans against {:.0} s of audio: best at {:+.2} s, \
         speed {:.4}, {:.1} clear of the rest",
        cues.len(),
        loudness.len() as f64 / PER_SECOND,
        best.delay,
        best.speed,
        best.clear,
    );
    best.believed().ok_or(Failure::Unsure)
}

/// A temporary subtitle file, removed when it goes out of scope.
struct Scratch(PathBuf);

impl Scratch {
    fn new(ticket: Ticket) -> Self {
        Self(std::env::temp_dir().join(format!(
            "dbm-subtitles-{}-{}.srt",
            std::process::id(),
            ticket.number()
        )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
