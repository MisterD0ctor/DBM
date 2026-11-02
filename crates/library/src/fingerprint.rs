//! Hearing that two recordings share a stretch of sound.
//!
//! An episode's title sequence is the same sound in every episode of a
//! season, and nothing else in two different episodes is — not for long.
//! So the sequence can be found without knowing anything about it: listen
//! to two episodes and look for the longest stretch they have in common.
//! The music under the closing credits is found the same way, at the other
//! end.
//!
//! The samples themselves cannot be compared. Two encodes of one recording
//! differ in every sample, and the sequence does not start on the same
//! sample of both. What survives is the *shape* of the sound: which bands
//! of pitch are getting louder against their neighbours, twenty times a
//! second. That is reduced to 32 bits a moment — the scheme is Haitsma and
//! Kalker's, from *A Highly Robust Audio Fingerprinting System* — and two
//! moments are the same sound when few of those bits differ.
//!
//! Nothing here touches a file, a process or the player: samples in, times
//! out, so it can be reasoned about, and tested, on its own.

/// Samples a second this expects. The bands end at 1800Hz, so anything
/// faster is detail it would not look at.
pub const RATE: usize = 4000;
/// Samples under one look: half a second, which is long enough that a
/// sequence starting part-way between two looks still mostly fills one.
const WINDOW: usize = 2048;
/// Samples from one look to the next.
const HOP: usize = 200;
/// Looks a second.
pub const PER_SECOND: f64 = (RATE / HOP) as f64;
/// One more than the bits in a print: each bit is the step between two
/// neighbouring bands.
const BANDS: usize = 33;
/// The pitches the bands cover, low to high, in hertz. Speech and music
/// both live here, and it is where a lossy encode changes least.
const LOW: f64 = 300.0;
const HIGH: f64 = 1800.0;
/// Mean square of a look below which it is silence: -70dB of full scale.
/// Silence has no shape, and two silences would otherwise be a perfect
/// match — every fade to black in one episode with every one in the other.
const QUIET: f32 = 1e-7;

/// How many of a print's 32 bits may differ for two looks to be one sound.
/// Unrelated sound differs in 16, give or take 3; the same recording
/// through two encodes, a few.
const NEAR: u32 = 9;
/// What a look that does not match costs a stretch, against the 1 a match
/// earns it. At 1 a stretch survives being wrong as often as right, which
/// is far more than the same sound ever is and far less than chance.
const MISS: i32 = 1;
/// The longest, in seconds. Two files that agree for longer than any title
/// sequence runs are the same episode twice.
const LONGEST: f64 = 300.0;
/// How much of a stretch has to match, net of what does not, for it to be
/// believed: three looks in four.
const SOLID: f64 = 0.5;

/// What a recording sounds like, a look at a time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Print {
    looks: Vec<u32>,
    /// Whether each look heard anything.
    heard: Vec<bool>,
}

impl Print {
    /// Take a recording's print. `samples` are mono at [`RATE`], within ±1.
    pub fn of(samples: &[f32]) -> Self {
        let mut print = Self::default();
        if samples.len() < WINDOW {
            return print;
        }
        let spectrum = Spectrum::new();
        let mut before = [0.0f32; BANDS];
        let mut now = [0.0f32; BANDS];
        let mut scratch = (vec![0.0f32; WINDOW], vec![0.0f32; WINDOW]);
        for (index, look) in samples.windows(WINDOW).step_by(HOP).enumerate() {
            let loud = look.iter().map(|x| x * x).sum::<f32>() / WINDOW as f32;
            spectrum.bands(look, &mut scratch, &mut now);
            let mut bits = 0u32;
            for band in 0..BANDS - 1 {
                // Whether this band gained on the one above it since the
                // last look. A difference of differences, so neither a
                // louder mix nor a tilted one changes the answer.
                let step = (now[band] - now[band + 1]) - (before[band] - before[band + 1]);
                bits = bits << 1 | u32::from(step > 0.0);
            }
            print.looks.push(bits);
            // The first has nothing before it to have changed from.
            print.heard.push(index > 0 && loud > QUIET);
            before = now;
        }
        print
    }

    /// The part of the recording from `from` seconds into it to `to`, as if
    /// it had been recorded from `from`; `to` may run past its end.
    pub fn part(&self, from: f64, to: f64) -> Self {
        let look = |seconds: f64| (seconds * PER_SECOND).round().max(0.0) as usize;
        let start = look(from).min(self.looks.len());
        let end = look(to).clamp(start, self.looks.len());
        Self {
            looks: self.looks[start..end].to_vec(),
            heard: self.heard[start..end].to_vec(),
        }
    }

    /// How long the recording was, to the last whole look.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.looks.len()
    }
}

/// Where the stretch two recordings share lies in each, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shared {
    pub first: (f64, f64),
    pub second: (f64, f64),
}

/// The longest stretch of the same sound in both, if there is one solid
/// enough to believe and at least `shortest` seconds long — which is for the
/// caller to say, since a title sequence and the music under a short show's
/// credits are not the same length.
///
/// Every way of laying one recording over the other is tried: the sequence
/// follows a cold open of any length in either, so it can be anywhere in
/// both. Along each, a stretch gains for every look that matches and loses
/// for every one that does not, and never falls below nothing — so it
/// starts over after anything that is not the same sound, and rides through
/// the odd look an encoder smeared.
pub fn shared(first: &Print, second: &Print, shortest: f64) -> Option<Shared> {
    let (a, b) = (first, second);
    let least = (shortest * PER_SECOND) as usize;
    if a.looks.len() < least || b.looks.len() < least {
        return None;
    }
    // (score, where it starts in `a`, where it peaks in `a`, how far along
    // `b` is ahead of `a`)
    let mut best: (i32, usize, usize, isize) = (0, 0, 0, 0);
    let lead = b.looks.len() as isize - least as isize;
    let lag = a.looks.len() as isize - least as isize;
    for ahead in -lag..=lead {
        let from = (-ahead).max(0) as usize;
        let until = a.looks.len().min((b.looks.len() as isize - ahead) as usize);
        let offset = (from as isize + ahead) as usize;
        let ours = &a.looks[from..until];
        let theirs = &b.looks[offset..offset + ours.len()];
        let heard = a.heard[from..until]
            .iter()
            .zip(&b.heard[offset..offset + ours.len()]);
        let (mut score, mut start) = (0i32, 0usize);
        for (at, ((x, y), (hx, hy))) in ours.iter().zip(theirs).zip(heard).enumerate() {
            if *hx && *hy && (x ^ y).count_ones() <= NEAR {
                if score == 0 {
                    start = at;
                }
                score += 1;
                if score > best.0 {
                    best = (score, from + start, from + at, ahead);
                }
            } else {
                score = (score - MISS).max(0);
            }
        }
    }
    let (score, start, peak, ahead) = best;
    let looks = peak + 1 - start.min(peak);
    let seconds = looks as f64 / PER_SECOND;
    if score == 0 || seconds < shortest || seconds > LONGEST {
        return None;
    }
    if f64::from(score) < SOLID * looks as f64 {
        return None;
    }
    // A look is a window, not an instant, and its time is its middle: the
    // first to match is the first the sequence mostly fills, and the last
    // the last, so the sequence's own edges are within a hop of both.
    let at = |look: usize| look as f64 / PER_SECOND + HALF;
    let there = |look: usize| (look as isize + ahead) as usize;
    Some(Shared {
        first: (at(start), at(peak)),
        second: (at(there(start)), at(there(peak))),
    })
}

/// Half a look, in seconds.
const HALF: f64 = WINDOW as f64 / RATE as f64 / 2.0;

/// The loudness of each band of one look, by a Fourier transform written
/// out here because it is thirty lines and the alternative is a dependency.
struct Spectrum {
    /// The taper a look is given so its edges do not ring.
    taper: Vec<f32>,
    /// One turn of the circle in `WINDOW` steps: (cos, sin).
    turn: Vec<(f32, f32)>,
    /// Which bins of the transform each band runs between.
    edges: [usize; BANDS + 1],
}

impl Spectrum {
    fn new() -> Self {
        let around = |i: usize| 2.0 * std::f64::consts::PI * i as f64 / WINDOW as f64;
        let mut edges = [0usize; BANDS + 1];
        for (band, edge) in edges.iter_mut().enumerate() {
            // Evenly spaced in pitch, which is to say in the logarithm.
            let hertz = LOW * (HIGH / LOW).powf(band as f64 / BANDS as f64);
            *edge = (hertz * WINDOW as f64 / RATE as f64).round() as usize;
        }
        Self {
            taper: (0..WINDOW)
                .map(|i| (0.5 - 0.5 * around(i).cos()) as f32)
                .collect(),
            turn: (0..WINDOW / 2)
                .map(|i| (around(i).cos() as f32, -around(i).sin() as f32))
                .collect(),
            edges,
        }
    }

    fn bands(&self, look: &[f32], scratch: &mut (Vec<f32>, Vec<f32>), out: &mut [f32; BANDS]) {
        let (re, im) = scratch;
        let bits = WINDOW.trailing_zeros();
        for (i, (x, taper)) in look.iter().zip(&self.taper).enumerate() {
            re[i.reverse_bits() >> (usize::BITS - bits)] = x * taper;
        }
        im.fill(0.0);
        let mut half = 1;
        while half < WINDOW {
            let stride = WINDOW / (2 * half);
            for base in (0..WINDOW).step_by(2 * half) {
                for k in 0..half {
                    let (cos, sin) = self.turn[k * stride];
                    let (i, j) = (base + k, base + k + half);
                    let (tr, ti) = (re[j] * cos - im[j] * sin, re[j] * sin + im[j] * cos);
                    (re[j], im[j]) = (re[i] - tr, im[i] - ti);
                    (re[i], im[i]) = (re[i] + tr, im[i] + ti);
                }
            }
            half *= 2;
        }
        for (band, energy) in out.iter_mut().enumerate() {
            *energy = (self.edges[band]..self.edges[band + 1])
                .map(|bin| re[bin] * re[bin] + im[bin] * im[bin])
                .sum();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A title sequence's shortest, as `titles` asks for it.
    const TITLES: f64 = 15.0;

    fn shared(first: &Print, second: &Print) -> Option<Shared> {
        super::shared(first, second, TITLES)
    }

    /// Noise that is the same every time it is asked for, and different for
    /// every seed: a small generator, so the tests carry no recordings.
    fn noise(seed: u64, seconds: f64) -> Vec<f32> {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..(seconds * RATE as f64) as usize)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 40) as f32 / (1u64 << 23) as f32 - 1.0
            })
            .collect()
    }

    /// Something with a shape to it: noise whose colour keeps changing, as
    /// music's does. Plain noise looks the same at every moment.
    fn music(seed: u64, seconds: f64) -> Vec<f32> {
        let raw = noise(seed, seconds);
        let pace = noise(seed ^ 0xABCD, seconds);
        let mut held = 0.0f32;
        let mut memory = 0.5f32;
        raw.iter()
            .enumerate()
            .map(|(i, x)| {
                if i % 600 == 0 {
                    memory = 0.05 + 0.9 * pace[i].abs();
                }
                held += memory * (x - held);
                held
            })
            .collect()
    }

    fn episode(cold: u64, cold_for: f64, theme: &[f32], rest: u64) -> Vec<f32> {
        let mut all = music(cold, cold_for);
        all.extend_from_slice(theme);
        all.extend(music(rest, 40.0));
        all
    }

    #[test]
    fn a_tone_lands_in_the_band_that_holds_it() {
        let spectrum = Spectrum::new();
        let tone: Vec<f32> = (0..WINDOW)
            .map(|i| (2.0 * std::f32::consts::PI * 700.0 * i as f32 / RATE as f32).sin())
            .collect();
        let mut out = [0.0f32; BANDS];
        spectrum.bands(&tone, &mut (vec![0.0; WINDOW], vec![0.0; WINDOW]), &mut out);
        let loudest = (0..BANDS)
            .max_by(|a, b| out[*a].total_cmp(&out[*b]))
            .unwrap();
        let bin = 700 * WINDOW / RATE;
        assert!(
            (spectrum.edges[loudest]..spectrum.edges[loudest + 1]).contains(&bin),
            "band {loudest}"
        );
    }

    #[test]
    fn the_stretch_two_episodes_share_is_found_in_both() {
        let theme = music(1, 30.0);
        let a = episode(2, 20.0, &theme, 3);
        let b = episode(4, 55.0, &theme, 5);
        let found = shared(&Print::of(&a), &Print::of(&b)).expect("a shared stretch");
        for (was, want) in [
            (found.first.0, 20.0),
            (found.first.1, 50.0),
            (found.second.0, 55.0),
            (found.second.1, 85.0),
        ] {
            assert!((was - want).abs() < 1.0, "{found:?}");
        }
    }

    #[test]
    fn it_is_found_through_a_different_encode() {
        let theme = music(1, 30.0);
        let a = episode(2, 20.0, &theme, 3);
        // Quieter, with something else laid over it, and starting between
        // two of the other's looks.
        let hiss = noise(9, 200.0);
        let mut b = music(4, 33.0);
        b.extend(std::iter::repeat_n(0.0, HOP / 2));
        b.extend(theme.iter().zip(&hiss).map(|(x, n)| 0.5 * x + 0.01 * n));
        b.extend(music(5, 40.0));
        let found = shared(&Print::of(&a), &Print::of(&b)).expect("a shared stretch");
        assert!((found.first.0 - 20.0).abs() < 1.0, "{found:?}");
        assert!((found.second.1 - 63.0).abs() < 1.0, "{found:?}");
    }

    #[test]
    fn a_part_is_heard_as_if_recorded_from_where_it_was_cut() {
        let theme = music(1, 30.0);
        let a = Print::of(&episode(2, 20.0, &theme, 3));
        let b = Print::of(&episode(4, 55.0, &theme, 5));
        let found =
            shared(&a.part(10.0, f64::INFINITY), &b.part(40.0, 90.0)).expect("a shared stretch");
        for (was, want) in [
            (found.first.0, 10.0),
            (found.first.1, 40.0),
            (found.second.0, 15.0),
            (found.second.1, 45.0),
        ] {
            assert!((was - want).abs() < 1.0, "{found:?}");
        }
    }

    #[test]
    fn episodes_with_nothing_in_common_share_nothing() {
        let a = Print::of(&music(2, 120.0));
        let b = Print::of(&music(3, 120.0));
        assert_eq!(shared(&a, &b), None);
    }

    #[test]
    fn a_sting_is_too_short_to_be_a_title_sequence() {
        let sting = music(1, 6.0);
        let a = episode(2, 20.0, &sting, 3);
        let b = episode(4, 30.0, &sting, 5);
        assert_eq!(shared(&Print::of(&a), &Print::of(&b)), None);
    }

    #[test]
    fn two_silences_are_not_the_same_sound() {
        let mut a = music(2, 30.0);
        a.extend(vec![0.0; 40 * RATE]);
        let mut b = music(3, 50.0);
        b.extend(vec![0.0; 40 * RATE]);
        assert_eq!(shared(&Print::of(&a), &Print::of(&b)), None);
    }

    #[test]
    fn the_same_file_twice_is_not_an_episode_and_its_neighbour() {
        let a = Print::of(&music(2, LONGEST + 60.0));
        assert_eq!(shared(&a, &a.clone()), None);
    }

    #[test]
    fn a_recording_shorter_than_a_look_has_no_print() {
        assert_eq!(Print::of(&[0.0; 100]).len(), 0);
    }
}
