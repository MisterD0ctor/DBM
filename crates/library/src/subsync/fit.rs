//! Putting the two together: every delay and speed tried, and the best kept
//! only if it stands out from the rest.

use super::Fit;
use super::cues::Cue;
use super::listen::PER_SECOND;

/// The furthest delay looked for, either way. Past a minute the subtitles
/// are for another cut of the film, which one number cannot fix.
const REACH: f64 = 60.0;

/// How finely delays are tried.
const STEP: f64 = 0.01;

/// What a delay is rounded to, as steps to the second: tenths, the step the
/// timing row moves by. The search is finer than this, but a subtitle's
/// out-time is set by reading speed and not by the voice, so the best delay
/// is a plateau a few tenths wide and its hundredths mean nothing.
const GRAINS: f64 = 10.0;

/// The speeds a subtitle file can be out by: its own, and each pairing of
/// the three frame rates films are mastered and broadcast at. A file timed
/// for a 25 frames a second transfer runs 4% fast against one at 23.976, and
/// is a minute and a half out by the end of an episode.
const SPEEDS: [f64; 7] = [
    1.0,
    25.0 / 23.976,
    23.976 / 25.0,
    24.0 / 23.976,
    23.976 / 24.0,
    25.0 / 24.0,
    24.0 / 25.0,
];

/// How much better another speed has to score before the subtitles' own is
/// given up. The two a tenth of a per cent either side of it come within a
/// few per cent of it on subtitles that are simply right, and a speed is
/// not a thing to change on a few per cent.
const FASTER_BY: f64 = 1.1;

/// How far the best delay has to stand above the rest, in their standard
/// deviations, to be believed. Subtitles that belonged to the audio came out
/// between 5.5 and 8 on the episodes this was tried on — the 5.5 from the
/// first five minutes of lines alone — and subtitles that did not, another
/// episode's or the right ones at a wrong speed, between 2 and 4.
const SURE: f64 = 4.5;

/// Fewer spans than this is a track of signs and songs, or a clip: too few
/// to tell one delay from the next.
const FEWEST: usize = 10;

/// The best timing a search found, and how far it stood above the others.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Best {
    pub(super) delay: f64,
    pub(super) speed: f64,
    score: f64,
    /// How many standard deviations the score is above the mean of every
    /// delay tried at this speed.
    pub(super) clear: f64,
}

impl Best {
    /// The timing to use, if it stood out enough to be one.
    pub(super) fn believed(self) -> Option<Fit> {
        (self.clear >= SURE).then(|| Fit {
            // Plus nothing, which is what turns a rounded -0.0 into 0.0.
            delay: (self.delay * GRAINS).round() / GRAINS + 0.0,
            speed: self.speed,
        })
    }
}

/// Try every timing and return the best, or `None` with too little to try.
///
/// The score for one timing is the speech inside the spans less what chance
/// would put there, which, with the film's average taken off every frame, is
/// the sum inside them. A running total makes that two lookups a span, so a
/// timing costs a step per line rather than per frame: the search is a
/// hundred million lookups, where a cross-correlation would be tens of
/// billions of multiplications.
pub(super) fn search(speech: &[f32], cues: &[Cue]) -> Option<Best> {
    if cues.len() < FEWEST || speech.is_empty() {
        return None;
    }
    let total = Running::over(speech);
    let steps = (2.0 * REACH / STEP).round() as usize;

    let tried: Vec<Best> = SPEEDS
        .iter()
        .map(|&speed| {
            let mut best = Best {
                delay: 0.0,
                speed,
                score: f64::MIN,
                clear: 0.0,
            };
            let (mut sum, mut squares) = (0.0, 0.0);
            for step in 0..=steps {
                let delay = step as f64 * STEP - REACH;
                // A greater speed stretches every span, and so its sum, for
                // the same fit; divided back out, the speeds can be compared.
                let score = cues
                    .iter()
                    .map(|cue| total.between(cue.start * speed + delay, cue.end * speed + delay))
                    .sum::<f64>()
                    / speed;
                sum += score;
                squares += score * score;
                if score > best.score {
                    (best.score, best.delay) = (score, delay);
                }
            }
            let count = (steps + 1) as f64;
            let mean = sum / count;
            let spread = (squares / count - mean * mean).max(0.0).sqrt();
            if spread > 0.0 {
                best.clear = (best.score - mean) / spread;
            }
            best
        })
        .collect();

    let own = tried[0];
    let best = tried
        .iter()
        .copied()
        .max_by(|a, b| a.score.total_cmp(&b.score))
        .unwrap_or(own);
    Some(if best.score > own.score * FASTER_BY {
        best
    } else {
        own
    })
}

/// A running total of the speech, with the film's average taken off every
/// frame, so the sum over any stretch is two lookups.
struct Running(Vec<f64>);

impl Running {
    fn over(speech: &[f32]) -> Self {
        let mean = speech.iter().map(|&s| f64::from(s)).sum::<f64>() / speech.len() as f64;
        let mut total = Vec::with_capacity(speech.len() + 1);
        let mut sum = 0.0;
        total.push(0.0);
        for &s in speech {
            sum += f64::from(s) - mean;
            total.push(sum);
        }
        Self(total)
    }

    /// The total up to a time in seconds. Before the film and after it there
    /// is nothing to add, which scores a span hanging off either end as the
    /// average — neither for a timing nor against it.
    fn until(&self, seconds: f64) -> f64 {
        let last = self.0.len() - 1;
        let at = (seconds * PER_SECOND).clamp(0.0, last as f64);
        let frame = at as usize;
        match self.0.get(frame + 1) {
            Some(next) => self.0[frame] + (next - self.0[frame]) * (at - frame as f64),
            None => self.0[last],
        }
    }

    fn between(&self, start: f64, end: f64) -> f64 {
        self.until(end) - self.until(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start: f64, end: f64) -> Cue {
        Cue { start, end }
    }

    /// The timing a search settles on, if it settles on one.
    fn fit(speech: &[f32], cues: &[Cue]) -> Option<Fit> {
        search(speech, cues)?.believed()
    }

    /// Lines of uneven length at uneven gaps, the way dialogue comes, for
    /// twenty minutes. Evenly spaced ones would fit at every multiple of
    /// their spacing and prove nothing.
    fn dialogue() -> Vec<Cue> {
        let mut cues = Vec::new();
        let (mut at, mut seed) = (20.0, 12345u64);
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        while at < 1180.0 {
            let length = 1.0 + 3.0 * next();
            cues.push(cue(at, at + length));
            at += length + 0.5 + 6.0 * next();
        }
        cues
    }

    /// Speech where the lines say there is some, once they are played at
    /// `speed` and `delay`.
    fn spoken(cues: &[Cue], delay: f64, speed: f64) -> Vec<f32> {
        let mut speech = vec![0.0f32; 120_000];
        for cue in cues {
            let from = ((cue.start * speed + delay) * PER_SECOND).max(0.0) as usize;
            let to = ((cue.end * speed + delay) * PER_SECOND).max(0.0) as usize;
            for frame in speech.iter_mut().take(to).skip(from) {
                *frame = 1.0;
            }
        }
        speech
    }

    #[test]
    fn subtitles_in_step_are_left_where_they_are() {
        let cues = dialogue();
        let found = fit(&spoken(&cues, 0.0, 1.0), &cues).unwrap();
        assert_eq!(found.speed, 1.0);
        assert!(found.delay.abs() < 0.05, "{found:?}");
    }

    #[test]
    fn a_delay_is_found_either_way() {
        let cues = dialogue();
        for delay in [-5.3, 12.7] {
            let found = fit(&spoken(&cues, delay, 1.0), &cues).unwrap();
            assert_eq!(found.speed, 1.0);
            assert!((found.delay - delay).abs() < 0.051, "{delay}: {found:?}");
        }
    }

    #[test]
    fn another_frame_rate_is_found_with_its_delay() {
        let cues = dialogue();
        let speed = 25.0 / 23.976;
        let found = fit(&spoken(&cues, 2.0, speed), &cues).unwrap();
        assert_eq!(found.speed, speed);
        assert!((found.delay - 2.0).abs() < 0.051, "{found:?}");
    }

    #[test]
    fn subtitles_for_something_else_are_not_guessed_at() {
        let cues = dialogue();
        // Speech that comes and goes on a clock of its own.
        let speech: Vec<f32> = (0..120_000)
            .map(|i| if (i / 170) % 3 == 0 { 1.0 } else { 0.0 })
            .collect();
        assert_eq!(fit(&speech, &cues), None);
    }

    #[test]
    fn a_handful_of_lines_is_too_few_to_go_on() {
        let cues: Vec<Cue> = dialogue().into_iter().take(FEWEST - 1).collect();
        assert_eq!(fit(&spoken(&cues, 3.0, 1.0), &cues), None);
    }
}
