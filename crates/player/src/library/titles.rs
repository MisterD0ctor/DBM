//! Finding an episode's title sequence and its closing credits by listening
//! for them.
//!
//! Where a file's chapters mark them (`Opening`, `End Credits`, a short last
//! chapter), nothing here is needed; see `PlayerState::intro` and
//! `PlayerState::credits_rolling`. Most don't: a broadcast episode is split
//! at act breaks, with the titles inside the first after a cold open of any
//! length, and a streaming download has no chapters at all.
//!
//! What a season's episodes do share is the same titles and the same music
//! under their credits, so this listens to one end of the episode playing
//! and of another of its season, and takes the stretch of sound the two share
//! (see `fingerprint`). Both answers are kept, so watching a season through,
//! every other episode has already been heard by the time it starts.
//!
//! Like the subtitle sync, a run has a thread of its own and an ffmpeg per
//! file behind it, and the player goes on playing while it listens.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::library::cache::PathMap;
use crate::library::ffmpeg;
use crate::library::fingerprint::{self, Print};
use crate::library::naming::{self, Media};
use crate::worker::{Completion, Reporter};

/// How many episodes of the season are tried before this one is taken to
/// have nothing to find.
///
/// A season has its odd episodes (a special, a finale that never stops for
/// the titles, one that ends on a song of its own), and a show made in
/// eleven-minute halves has titles on only every other episode, so the two
/// either side may both have none.
const TRIES: usize = 4;
/// How far either side of where an episode is known to hold its sequence it
/// is listened to, in seconds, when it is the one compared with. More would
/// only give the comparison something else to agree on.
const AROUND: f64 = 5.0;
/// The shortest title sequence, in seconds. A network's ident, or a
/// "previously on" over the same sting, is shorter, and not worth skipping.
const SHORTEST: f64 = 15.0;
/// The shortest closing credits, in seconds. A short show's run under ten:
/// one card and a few bars. That is short enough to be a cue that comes
/// round in every episode, so credits this short have more to prove; see
/// [`credits`].
const SHORTEST_CREDITS: f64 = 8.0;
/// A sequence found starting this close to the start of the file starts
/// with it, in seconds: the first look has nothing before it to compare.
const FROM_THE_TOP: f64 = 2.0;
/// How near the end of the file closing credits have to end, in seconds,
/// since after them come only a vanity card or two and at most a look at next
/// week. A shared stretch well short of that is a recurring cue or a "next
/// time" sting.
const NEAR_THE_END: f64 = 300.0;
/// How near the end of a file what ends it has to end, in seconds: the few
/// it takes to go dark.
const AT_THE_END: f64 = 5.0;

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

    /// The shortest stretch two episodes share that can be this end.
    fn shortest(self) -> f64 {
        match self {
            End::Opening => SHORTEST,
            End::Closing => SHORTEST_CREDITS,
        }
    }

    /// What listening for it found, by file: where it is, or that there is
    /// none.
    ///
    /// Both are kept, since an episode with no sequence is as much work to
    /// hear as one with, and would otherwise be heard again every time.
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

/// The version of how a sequence is heard, which what was heard is kept under
/// (see `cache`). Bumped whenever a change would hear one differently.
///
/// 1. Compared with the two episodes either side; titles and credits both
///    fifteen seconds or more.
/// 2. With up to four of the season, those known to hold the sequence
///    first; credits from eight seconds, if they end an episode.
/// 3. The nearest of the season first, known to hold the sequence or not.
const HEARD: u32 = 3;

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

/// The episodes `path` could be compared with, and how many places from it
/// each is: others of the same show and season in the playlist, nearest
/// first, the one after before the one before. Empty for a film, which
/// shares neither end with anything.
///
/// The same season because a show's titles change between seasons, and
/// nearest because they also change within one (a cast member gone, a second
/// cut from the halfway point). Which are tried, and in what order, depends
/// on what is already known of them; see [`worth_trying`].
pub fn beside(path: &str, playlist: &[String]) -> Vec<(String, usize)> {
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
    let mut near: Vec<(usize, usize, &String)> = playlist
        .iter()
        .enumerate()
        .filter(|(at, other)| *at != here && same(other))
        .map(|(at, other)| (at.abs_diff(here), usize::from(at < here), other))
        .collect();
    near.sort();
    near.into_iter()
        .map(|(away, _, other)| (other.clone(), away))
        .collect()
}

/// Which of the episodes [`beside`] gave to compare with, in order, and what
/// each was already heard to hold at this end.
///
/// Nearest first, since titles can change partway through a season, and
/// then every episode known to hold them may hold the old ones. Of two as
/// near, one known to hold the sequence goes first, being the cheapest to
/// listen to. Those heard to hold none go last, worth a try only in case
/// that was wrong.
fn worth_trying<'a>(
    beside: &'a [(String, usize)],
    known: &HashMap<String, Option<Span>>,
) -> Vec<(&'a str, Option<Option<Span>>)> {
    let mut ranked: Vec<(usize, &(String, usize))> = beside.iter().enumerate().collect();
    ranked.sort_by_key(|&(near, (other, away))| {
        let held = known.get(other);
        let holds = matches!(held, Some(Some(_)));
        let holds_none = matches!(held, Some(None));
        (holds_none, *away, !holds, near)
    });
    ranked
        .into_iter()
        .take(TRIES)
        .map(|(_, (other, _))| (other.as_str(), known.get(other).copied()))
        .collect()
}

/// Find each of `ends` in `path`, off the UI thread, and report them like
/// any other background result, one at a time and in the order given.
/// `duration` is the file's length and `beside` what [`beside`] said to
/// compare it with.
///
/// Returns immediately, superseding any run still going for a file no longer
/// playing. A superseded run reports nothing, and neither does one that
/// couldn't listen at all (no ffmpeg, nothing beside it), since nothing is
/// waiting on the answer.
pub fn spawn(
    path: String,
    ends: Vec<End>,
    beside: Vec<(String, usize)>,
    duration: f64,
    reporter: Reporter,
) {
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
    beside: &[(String, usize)],
    duration: f64,
    generation: u64,
) -> Option<Option<Span>> {
    let heard = end.heard();
    let known = heard.load();
    if let Some(found) = known.get(path) {
        return Some(*found);
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
    // A match shorter than a title sequence is held while the rest are
    // tried, in case one shares more: credits that are a short cut of
    // another episode's share only the cut, and an episode heard to hold
    // only part of its credits offers only that part.
    let mut short: Option<(Span, &str, Span)> = None;
    for (other, held) in worth_trying(beside, &known) {
        // Where in the other file to listen, and for how long: around what
        // it is known to hold, or else the same end as this one's. Its
        // length is not known without asking, and its end cannot be found
        // without it.
        let known = matches!(held, Some(Some(_)));
        let (window, length_there) = match (held, end) {
            (Some(Some((start, to))), _) => {
                let at = (start - AROUND).max(0.0);
                (Some((at, to + AROUND - at)), None)
            }
            (_, End::Opening) => (Some((0.0, reach)), None),
            (_, End::Closing) => {
                let length = length(&ffmpeg, Path::new(other));
                (length.map(|l| (from(l), reach)), length)
            }
        };
        // A file that will not decode is not an episode without titles.
        let theirs = window.and_then(|(at, long)| {
            listen(&ffmpeg, Path::new(other), at, long, generation).zip(Some(at))
        });
        let Some((theirs, theirs_from)) = theirs else {
            if !current(generation) {
                return None;
            }
            continue;
        };
        compared = true;
        let Some(shared) = fingerprint::shared(&ours, &theirs, end.shortest()) else {
            continue;
        };
        let here = tidy(shared.first, from(duration));
        let there = tidy(shared.second, theirs_from);
        if end == End::Closing && !credits(here, duration, there, length_there, known) {
            continue;
        }
        let lasts = |(start, to): Span| to - start;
        if lasts(here) < SHORTEST {
            if short.is_none_or(|(kept, ..)| lasts(here) > lasts(kept)) {
                short = Some((here, other, there));
            }
            continue;
        }
        keep(heard, path, here, other, there);
        return Some(Some(here));
    }
    if let Some((here, other, there)) = short {
        keep(heard, path, here, other, there);
        return Some(Some(here));
    }
    if !compared {
        return None;
    }
    heard.set(path, None);
    Some(None)
}

/// Remember where a stretch two files share lies in each: `here` in `path`,
/// the one playing, and `there` in `other`.
///
/// The other file's half doesn't replace what it was heard to hold when it
/// was the one playing, only a finding of none, which that run must have
/// missed.
fn keep(heard: &PathMap<Option<Span>>, path: &str, here: Span, other: &str, there: Span) {
    heard.set(path, Some(here));
    if !matches!(heard.load().get(other), Some(Some(_))) {
        heard.set(other, Some(there));
    }
}

/// Whether a stretch two episodes share near their ends is where this one's
/// credits roll.
///
/// `here` and `there` are where it lies in each, `duration` how long this one
/// is, and `length_there` the other, where it was asked. One that stops well
/// short of this episode's end is something else. One shorter than a title
/// sequence must also be plainly the credits: the very end of one of the two,
/// or a match for what is known to be the other's credits (`known`). Not
/// necessarily this one's end, since where one episode's credits are a short
/// cut of another's, the shared stretch ends the longer one's early.
fn credits(here: Span, duration: f64, there: Span, length_there: Option<f64>, known: bool) -> bool {
    if here.1 < duration - NEAR_THE_END {
        return false;
    }
    if known || here.1 - here.0 >= SHORTEST {
        return true;
    }
    let last = |(_, to): Span, length: f64| to >= length - AT_THE_END;
    last(here, duration) || length_there.is_some_and(|length| last(there, length))
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
/// The first track whichever is playing, since the music is the same under
/// every dub and the neighbour's tracks aren't known without asking. `None`
/// for a run that was superseded, an ffmpeg that wouldn't start, or a file
/// with nothing to hear.
fn listen(ffmpeg: &Path, file: &Path, from: f64, reach: f64, generation: u64) -> Option<Print> {
    // A playlist can hold addresses as well as files, which aren't worth
    // downloading again for a pill.
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

    fn away(names: &[(&str, usize)]) -> Vec<(String, usize)> {
        names.iter().map(|&(n, a)| (n.to_string(), a)).collect()
    }

    fn tried<'a>(
        near: &'a [(String, usize)],
        known: &HashMap<String, Option<Span>>,
    ) -> Vec<&'a str> {
        worth_trying(near, known)
            .into_iter()
            .map(|(other, _)| other)
            .collect()
    }

    #[test]
    fn the_nearest_episodes_come_first() {
        let season = list(&[
            "Show.S02E01.mkv",
            "Show.S02E02.mkv",
            "Show.S02E03.mkv",
            "Show.S02E04.mkv",
        ]);
        assert_eq!(
            beside("Show.S02E02.mkv", &season),
            away(&[
                ("Show.S02E03.mkv", 1),
                ("Show.S02E01.mkv", 1),
                ("Show.S02E04.mkv", 2)
            ])
        );
        // The last has nothing after it, so those before, nearest first.
        assert_eq!(
            beside("Show.S02E04.mkv", &season),
            away(&[
                ("Show.S02E03.mkv", 1),
                ("Show.S02E02.mkv", 2),
                ("Show.S02E01.mkv", 3)
            ])
        );
    }

    #[test]
    fn of_two_as_near_one_known_to_hold_the_titles_goes_first() {
        // A show in eleven-minute halves: the titles on every other one.
        let season: Vec<String> = (1..=9).map(|e| format!("Show.S07E{e:02}.mkv")).collect();
        let near = beside("Show.S07E05.mkv", &season);
        let mut known = HashMap::new();
        known.insert("Show.S07E06.mkv".to_string(), None);
        known.insert("Show.S07E04.mkv".to_string(), None);
        known.insert("Show.S07E03.mkv".to_string(), Some((0.0, 25.0)));
        known.insert("Show.S07E09.mkv".to_string(), Some((0.0, 25.0)));
        // The one that holds them ahead of the one as near not yet heard,
        // then the next nearest, the later of two as near. The further one
        // that holds them is not reached, and nor are the two either side,
        // heard to have none.
        assert_eq!(
            tried(&near, &known),
            [
                "Show.S07E03.mkv",
                "Show.S07E07.mkv",
                "Show.S07E08.mkv",
                "Show.S07E02.mkv"
            ]
        );
    }

    #[test]
    fn titles_that_change_partway_are_found_after_the_change() {
        // A season whose second half has titles of its own, and the first
        // half already heard.
        let season: Vec<String> = (1..=22).map(|e| format!("Show.S01E{e:02}.mkv")).collect();
        let near = beside("Show.S01E12.mkv", &season);
        let known = (1..=11)
            .map(|e| (format!("Show.S01E{e:02}.mkv"), Some((50.0, 140.0))))
            .collect();
        // Outward from the change, so the second half's own are reached
        // rather than all of the first half's.
        assert_eq!(
            tried(&near, &known),
            [
                "Show.S01E11.mkv",
                "Show.S01E13.mkv",
                "Show.S01E10.mkv",
                "Show.S01E14.mkv"
            ]
        );
    }

    #[test]
    fn short_credits_have_to_end_one_of_the_two() {
        // Ten seconds of music that stops a second before this file does.
        assert!(credits(
            (675.4, 684.8),
            686.0,
            (658.0, 667.4),
            Some(690.0),
            false
        ));
        // Or before the other does: here it is the start of longer credits.
        assert!(credits(
            (649.9, 659.3),
            669.2,
            (673.3, 682.8),
            Some(683.3),
            false
        ));
        // The same ten seconds a minute short of both is a cue.
        assert!(!credits(
            (615.4, 624.8),
            686.0,
            (598.0, 607.4),
            Some(669.2),
            false
        ));
        // Unless what it matched is known to be the other's credits.
        assert!(credits((649.9, 659.3), 669.2, (673.3, 682.8), None, true));
        // Never far from the end, known or not.
        assert!(!credits((100.0, 110.0), 686.0, (673.3, 682.8), None, true));
    }

    #[test]
    fn credits_as_long_as_titles_may_be_followed_by_more() {
        let far = (0.0, 0.0);
        assert!(credits((2547.9, 2582.3), 2700.0, far, None, false));
        assert!(!credits((1000.0, 1040.0), 2700.0, far, None, false));
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
            away(&[("Show.S02E03.mkv", 2)])
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
