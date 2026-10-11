//! Finding an episode's title sequence by listening for it.
//!
//! Where a file's chapters name the sequence — `Opening`, `Intro`, `OP` —
//! nothing here is needed; see `PlayerState::intro`. Most do not: a
//! broadcast episode is split at its act breaks and the titles fall
//! somewhere inside the first of them, after a cold open of any length.
//!
//! What every episode of a season does have is the same sequence, so this
//! listens to the start of the episode playing and of one beside it in the
//! playlist, and takes the stretch of sound the two share — see
//! `fingerprint`. That answers for both of them, and both answers are kept:
//! watching a season through, every other episode has already been heard
//! by the time it starts.
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

/// How much of an episode is listened to, in seconds, and never more than
/// its first half. A cold open runs to a quarter of an hour at the outside;
/// the half keeps a short episode's closing music — the same in every
/// episode too — from being taken for its opening.
const REACH: f64 = 1200.0;
/// How many episodes beside this one are tried before it is taken to have
/// no sequence. More than one because a season has its odd episode — a
/// special, a finale that opens cold and never stops for the titles.
const TRIES: usize = 2;
/// A sequence found starting this close to the start of the file starts
/// with it, in seconds: the first look has nothing before it to compare.
const FROM_THE_TOP: f64 = 2.0;

/// Where a title sequence starts and ends, in seconds.
pub type Span = (f64, f64);

/// What listening to a file found: its sequence, or that it has none.
///
/// Both are kept. An episode with no sequence is as much work to hear as
/// one with, and without the entry it would be heard again every time it
/// was played.
static HEARD: PathMap<Option<Span>> = PathMap::new("intros.json");

/// A run's answer: the file it listened to and what it found there.
pub struct Outcome {
    pub path: String,
    pub span: Option<Span>,
}

/// Bumped by every run. One that finds the number has moved on stops.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn current(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) == generation
}

/// The episodes worth comparing `path` with: others of the same show and
/// season in the playlist, nearest first, the one after before the one
/// before. Empty for a film, which has no sequence to share with anything.
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

/// Find `path`'s title sequence, off the UI thread, and report it like any
/// other background result. `duration` is the file's length and `beside`
/// what [`beside`] said to compare it with.
///
/// Returns immediately, and supersedes any run still going: that one was
/// for a file no longer playing. A superseded run reports nothing, and
/// neither does one that could not listen at all — no ffmpeg, nothing
/// beside it — because nothing is waiting on the answer.
pub fn spawn(path: String, beside: Vec<String>, duration: f64, reporter: Reporter) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        let Some(span) = find(&path, &beside, duration, generation) else {
            return;
        };
        if !current(generation) {
            return;
        }
        match span {
            Some((start, end)) => {
                eprintln!("dbm: title sequence from {start:.1} s to {end:.1} s")
            }
            None => eprintln!("dbm: no title sequence heard"),
        }
        reporter.send(Completion::Intro(Outcome { path, span }));
    });
}

/// `None` when there is no answer; `Some(None)` when the answer is that
/// there is no sequence.
fn find(path: &str, beside: &[String], duration: f64, generation: u64) -> Option<Option<Span>> {
    if let Some(known) = HEARD.load().remove(path) {
        return Some(known);
    }
    if beside.is_empty() || duration <= 0.0 {
        return None;
    }
    let ffmpeg = ffmpeg::path()?;
    ffmpeg::announce(&ffmpeg);
    let reach = REACH.min(duration / 2.0);
    let ours = listen(&ffmpeg, Path::new(path), reach, generation)?;

    let mut compared = false;
    for other in beside {
        // A file that will not decode is not an episode without titles.
        let Some(theirs) = listen(&ffmpeg, Path::new(other), reach, generation) else {
            if !current(generation) {
                return None;
            }
            continue;
        };
        compared = true;
        let Some(shared) = fingerprint::shared(&ours, &theirs) else {
            continue;
        };
        let (here, there) = (tidy(shared.first), tidy(shared.second));
        HEARD.set(path, Some(here));
        // The other half of the same answer. Not written over what that
        // file was found to hold when it was the one playing.
        if !HEARD.load().contains_key(other) {
            HEARD.set(other, Some(there));
        }
        return Some(Some(here));
    }
    if !compared {
        return None;
    }
    HEARD.set(path, None);
    Some(None)
}

fn tidy((start, end): Span) -> Span {
    let round = |seconds: f64| (seconds * 10.0).round() / 10.0;
    (
        if start < FROM_THE_TOP {
            0.0
        } else {
            round(start)
        },
        round(end),
    )
}

/// Decode the first `reach` seconds of a file's first audio track and take
/// their print.
///
/// The first track whichever is playing: the music is the same under every
/// dub, and the neighbour's tracks are not known without asking it. `None`
/// for a run that was superseded, an ffmpeg that would not start, or a file
/// with nothing to hear.
fn listen(ffmpeg: &Path, file: &Path, reach: f64, generation: u64) -> Option<Print> {
    // A playlist can hold addresses as well as files, and those are not
    // downloaded twice over for the sake of a pill.
    if !file.is_file() {
        return None;
    }
    let mut command = ffmpeg::command(ffmpeg);
    command
        // One thread, as the preview's are: the player is decoding the same
        // film, and this is the one of the two nobody is watching.
        .args(["-v", "error", "-nostdin", "-threads", "1", "-t"])
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
        assert_eq!(tidy((0.656, 24.856)), (0.0, 24.9));
        assert_eq!(tidy((274.106, 308.856)), (274.1, 308.9));
    }
}
