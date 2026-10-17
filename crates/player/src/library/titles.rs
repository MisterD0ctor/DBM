//! Finding an episode's title sequence and its closing credits by listening
//! for them.
//!
//! Where a file's chapters mark them — `Opening`, `End Credits`, a short
//! last chapter — nothing here is needed; see `PlayerState::intro` and
//! `PlayerState::credits_rolling`. Most do not. A broadcast episode is
//! split at its act breaks and the titles fall somewhere inside the first
//! of them, after a cold open of any length; a download from a streaming
//! service has no chapters at all, and nothing says where its credits
//! start.
//!
//! What every episode of a season does have is the same titles and the
//! same music under its credits, so this listens to one end of the episode
//! playing and of one beside it in the playlist, and takes the stretch of
//! sound the two share — see `fingerprint`. That answers for both of them,
//! and both answers are kept: watching a season through, every other
//! episode has already been heard by the time it starts.
//!
//! Like the subtitle sync, a run has a thread of its own and an ffmpeg per
//! file behind it, and the player goes on playing while it listens.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::library::cache::PathMap;
use crate::library::ffmpeg;
use crate::library::fingerprint::{self, Print};
use crate::library::naming::{self, Media};
use crate::worker::{Completion, Reporter};

/// How many episodes beside this one are tried before it is taken to have
/// nothing to find. More than one because a season has its odd episode — a
/// special, a finale that opens cold and never stops for the titles, or
/// ends on a song of its own.
const TRIES: usize = 2;
/// A sequence found starting this close to the start of the file starts
/// with it, in seconds: the first look has nothing before it to compare.
const FROM_THE_TOP: f64 = 2.0;
/// How near the end of the file closing credits have to end, in seconds.
/// The music under them is followed by a vanity card or two and at most a
/// look at next week; a stretch two episodes share that stops well short of
/// that is something else — a recurring cue, a "next time" sting — and the
/// episode is not over when it plays.
const NEAR_THE_END: f64 = 300.0;

/// Which end of an episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// The title sequence, somewhere in the first half.
    Opening,
    /// The closing credits, somewhere in the second.
    Closing,
}

impl End {
    /// How much of the episode is listened to for it, in seconds, and never
    /// more than half. A cold open runs to a quarter of an hour at the
    /// outside, and credits with everything after them to a few minutes.
    /// The half keeps each end's music — both the same in every episode —
    /// from being taken for the other's.
    fn reach(self) -> f64 {
        match self {
            End::Opening => 1200.0,
            End::Closing => 600.0,
        }
    }

    /// What listening for it found, by file: where it is, or that there is
    /// none.
    ///
    /// Both are kept. An episode with no sequence is as much work to hear
    /// as one with, and without the entry it would be heard again every
    /// time it was played.
    fn heard(self) -> &'static PathMap<Option<Span>> {
        static OPENINGS: PathMap<Option<Span>> = PathMap::cache("intros.json", HEARD);
        static CLOSINGS: PathMap<Option<Span>> = PathMap::cache("outros.json", HEARD);
        match self {
            End::Opening => &OPENINGS,
            End::Closing => &CLOSINGS,
        }
    }
}

/// Where a sequence starts and ends, in seconds.
pub type Span = (f64, f64);

/// The version of the way a sequence is heard, which what was heard is kept
/// under — see `cache`. Bumped whenever a change would hear one differently,
/// so what the old way heard is heard again.
///
/// 1. Compared with the two episodes either side; titles and credits both
///    fifteen seconds or more.
const HEARD: u32 = 1;

/// A run's answer: the file it listened to, which end of it, and what it
/// found there.
pub struct Outcome {
    pub path: String,
    pub end: End,
    pub span: Option<Span>,
}

/// Bumped by every run. One that finds the number has moved on stops.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn current(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) == generation
}

/// The episodes worth comparing `path` with: others of the same show and
/// season in the playlist, nearest first, the one after before the one
/// before. Empty for a film, which shares neither end with anything.
///
/// The same season because a show's titles change between them, and
/// nearest because they also change within one — a cast member gone, a
/// second cut from the half-way point.
pub fn beside(path: &str, playlist: &[String]) -> Vec<String> {
    let Media::Episode { show, season, .. } = naming::parse(path) else {
        return Vec::new();
    };
    let Some(here) = playlist.iter().position(|p| p == path) else {
        return Vec::new();
    };
    let same = |other: &String| {
        matches!(
            naming::parse(other),
            Media::Episode { show: s, season: n, .. }
                if n == season && s.eq_ignore_ascii_case(&show)
        )
    };
    let mut near: Vec<(usize, &String)> = playlist
        .iter()
        .enumerate()
        .filter(|(at, other)| *at != here && same(other))
        .map(|(at, other)| (at.abs_diff(here) * 2 + usize::from(at < here), other))
        .collect();
    near.sort();
    near.into_iter()
        .take(TRIES)
        .map(|(_, other)| other.clone())
        .collect()
}

/// Find each of `ends` in `path`, off the UI thread, and report them like
/// any other background result, one at a time and in the order given.
/// `duration` is the file's length and `beside` what [`beside`] said to
/// compare it with.
///
/// Returns immediately, and supersedes any run still going: that one was
/// for a file no longer playing. A superseded run reports nothing, and
/// neither does one that could not listen at all — no ffmpeg, nothing
/// beside it — because nothing is waiting on the answer.
pub fn spawn(path: String, ends: Vec<End>, beside: Vec<String>, duration: f64, reporter: Reporter) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        for end in ends {
            let found = find(&path, end, &beside, duration, generation);
            if !current(generation) {
                return;
            }
            let Some(span) = found else {
                continue;
            };
            let what = match end {
                End::Opening => "title sequence",
                End::Closing => "closing credits",
            };
            match span {
                Some((start, to)) => eprintln!("dbm: {what} from {start:.1} s to {to:.1} s"),
                None => eprintln!("dbm: no {what} heard"),
            }
            let path = path.clone();
            reporter.send(Completion::Titles(Outcome { path, end, span }));
        }
    });
}

/// `None` when there is no answer; `Some(None)` when the answer is that
/// there is no sequence.
fn find(
    path: &str,
    end: End,
    beside: &[String],
    duration: f64,
    generation: u64,
) -> Option<Option<Span>> {
    let heard = end.heard();
    if let Some(known) = heard.load().remove(path) {
        return Some(known);
    }
    if beside.is_empty() || duration <= 0.0 {
        return None;
    }
    let ffmpeg = ffmpeg::path()?;
    ffmpeg::announce(&ffmpeg);
    let reach = end.reach().min(duration / 2.0);
    let from = |length: f64| match end {
        End::Opening => 0.0,
        End::Closing => (length - reach).max(0.0),
    };
    let ours = listen(&ffmpeg, Path::new(path), from(duration), reach, generation)?;

    let mut compared = false;
    for other in beside {
        // The other file's length is not known without asking, and its end
        // cannot be found without it.
        let theirs_from = match end {
            End::Opening => Some(0.0),
            End::Closing => length(&ffmpeg, Path::new(other)).map(from),
        };
        // A file that will not decode is not an episode without titles.
        let theirs = theirs_from
            .and_then(|at| listen(&ffmpeg, Path::new(other), at, reach, generation).zip(Some(at)));
        let Some((theirs, theirs_from)) = theirs else {
            if !current(generation) {
                return None;
            }
            continue;
        };
        compared = true;
        let Some(shared) = fingerprint::shared(&ours, &theirs) else {
            continue;
        };
        let here = tidy(shared.first, from(duration));
        let there = tidy(shared.second, theirs_from);
        if end == End::Closing && here.1 < duration - NEAR_THE_END {
            continue;
        }
        heard.set(path, Some(here));
        // The other half of the same answer. Not written over what that
        // file was found to hold when it was the one playing.
        if !heard.load().contains_key(other) {
            heard.set(other, Some(there));
        }
        return Some(Some(here));
    }
    if !compared {
        return None;
    }
    heard.set(path, None);
    Some(None)
}

/// A stretch found in a recording that began `from` seconds into its file,
/// as times in the file, to a tenth of a second.
fn tidy((start, end): Span, from: f64) -> Span {
    let round = |seconds: f64| (seconds * 10.0).round() / 10.0;
    let (start, end) = (start + from, end + from);
    (
        if start < FROM_THE_TOP {
            0.0
        } else {
            round(start)
        },
        round(end),
    )
}

/// How long a file is, in seconds, as its header says. **Blocking.**
fn length(ffmpeg: &Path, file: &Path) -> Option<f64> {
    if !file.is_file() {
        return None;
    }
    ffmpeg::parse_duration(&ffmpeg::describe(ffmpeg, file)?)
}

/// Decode `reach` seconds of a file's first audio track, starting `from`
/// seconds in, and take their print.
///
/// The first track whichever is playing: the music is the same under every
/// dub, and the neighbour's tracks are not known without asking it. `None`
/// for a run that was superseded, an ffmpeg that would not start, or a file
/// with nothing to hear.
fn listen(ffmpeg: &Path, file: &Path, from: f64, reach: f64, generation: u64) -> Option<Print> {
    // A playlist can hold addresses as well as files, and those are not
    // downloaded twice over for the sake of a pill.
    if !file.is_file() {
        return None;
    }
    let mut command = ffmpeg::command(ffmpeg);
    command
        // One thread, as the preview's are: the player is decoding the same
        // film, and this is the one of the two nobody is watching.
        .args(["-v", "error", "-nostdin", "-threads", "1", "-ss"])
        .arg(format!("{from:.3}"))
        .arg("-t")
        .arg(format!("{reach:.3}"))
        .arg("-i")
        .arg(file)
        .args(["-map", "0:a:0", "-vn", "-sn", "-dn", "-ac", "1", "-ar"])
        .arg(fingerprint::RATE.to_string())
        .args(["-f", "s16le", "pipe:1"]);
    // Nothing reads its complaints, and a pipe nobody reads fills.
    command.stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = ffmpeg::spawn_behind(&mut command).ok()?;
    let mut audio = child.stdout.take()?;
    let mut samples = Samples::default();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        if !current(generation) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        match audio.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => samples.feed(&buffer[..n]),
        }
    }
    let _ = child.wait();
    (!samples.heard.is_empty()).then(|| Print::of(&samples.heard))
}

/// Puts samples back together from the bytes of a pipe, which ends a read
/// wherever it likes.
#[derive(Default)]
struct Samples {
    heard: Vec<f32>,
    /// The half of a sample a read ended on.
    odd: Option<u8>,
}

impl Samples {
    fn feed(&mut self, bytes: &[u8]) {
        let mut bytes = bytes.iter().copied();
        loop {
            let Some(low) = self.odd.take().or_else(|| bytes.next()) else {
                return;
            };
            let Some(high) = bytes.next() else {
                self.odd = Some(low);
                return;
            };
            self.heard
                .push(f32::from(i16::from_le_bytes([low, high])) / 32768.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn the_episodes_either_side_are_the_ones_compared() {
        let season = list(&[
            "Show.S02E01.mkv",
            "Show.S02E02.mkv",
            "Show.S02E03.mkv",
            "Show.S02E04.mkv",
        ]);
        assert_eq!(
            beside("Show.S02E02.mkv", &season),
            list(&["Show.S02E03.mkv", "Show.S02E01.mkv"])
        );
        // The last has nothing after it, so the two before.
        assert_eq!(
            beside("Show.S02E04.mkv", &season),
            list(&["Show.S02E03.mkv", "Show.S02E02.mkv"])
        );
    }

    #[test]
    fn another_season_or_another_show_is_not_compared() {
        let mixed = list(&[
            "Show.S01E09.mkv",
            "Show.S02E01.mkv",
            "Other.Show.S02E02.mkv",
            "Show.S02E03.mkv",
        ]);
        assert_eq!(
            beside("Show.S02E01.mkv", &mixed),
            list(&["Show.S02E03.mkv"])
        );
    }

    #[test]
    fn a_film_or_a_lone_episode_has_nothing_beside_it() {
        let films = list(&["Some Film (2019).mkv", "Another Film (2020).mkv"]);
        assert!(beside("Some Film (2019).mkv", &films).is_empty());
        assert!(beside("Show.S02E01.mkv", &list(&["Show.S02E01.mkv"])).is_empty());
        assert!(beside("Show.S02E01.mkv", &[]).is_empty());
    }

    #[test]
    fn samples_split_across_reads_are_put_back_together() {
        let mut samples = Samples::default();
        samples.feed(&[0x00, 0x40, 0x00]);
        samples.feed(&[0xC0]);
        assert_eq!(samples.heard, vec![0.5, -0.5]);
    }

    #[test]
    fn a_sequence_at_the_very_start_starts_at_nothing() {
        assert_eq!(tidy((0.656, 24.856), 0.0), (0.0, 24.9));
        assert_eq!(tidy((274.106, 308.856), 0.0), (274.1, 308.9));
    }

    #[test]
    fn credits_are_placed_in_the_file_and_not_in_what_was_listened_to() {
        // The last ten minutes of 2584 seconds, and 34 seconds at their end.
        assert_eq!(tidy((563.756, 598.156), 1984.145), (2547.9, 2582.3));
    }
}
