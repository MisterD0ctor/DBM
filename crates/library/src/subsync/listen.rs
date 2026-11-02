//! The audio's side: how much speech there is in every hundredth of a
//! second, as ffmpeg decodes it.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use super::Asked;
use crate::ffmpeg;
use crate::threads::Ticket;

/// What ffmpeg is asked for: mono at this rate, which keeps everything a
/// voice has below 4 kHz and is an eighth of the samples a film carries.
const RATE: usize = 8000;

/// Samples to a frame: a hundredth of a second, which is finer than anyone
/// can time a subtitle by eye.
const FRAME: usize = RATE / 100;

/// Frames to a second.
pub(super) const PER_SECOND: f64 = (RATE / FRAME) as f64;

/// Decode the film's audio and return how loud each frame of it is, in
/// decibels. `stream` asks for a subtitle track to be written out as SubRip
/// along the way.
///
/// `None` for a run that was superseded or an ffmpeg that would not start.
/// One that starts and produces nothing returns an empty list, which the
/// caller reads as silence.
pub(super) fn listen(
    ffmpeg: &Path,
    asked: &Asked,
    centre: bool,
    stream: Option<(i64, &Path)>,
    ticket: Ticket,
) -> Option<Vec<f32>> {
    let mut command = ffmpeg::command(ffmpeg);
    command
        // One thread, as the preview's are: the player is decoding the same
        // film, and this is the one of the two nobody is watching.
        .args(["-v", "error", "-nostdin", "-threads", "1", "-i"])
        .arg(&asked.video)
        .arg("-map")
        .arg(match asked.audio {
            Some(index) => format!("0:{index}"),
            None => "0:a:0".into(),
        })
        .args(["-vn", "-sn", "-dn"]);
    if centre {
        command.args(["-af", "pan=mono|c0=FC"]);
    }
    command
        .args(["-ac", "1", "-ar"])
        .arg(RATE.to_string())
        .args(["-f", "s16le", "pipe:1"]);
    if let Some((index, to)) = stream {
        command
            .arg("-map")
            .arg(format!("0:{index}"))
            .args(["-f", "srt", "-y"])
            .arg(to);
    }
    // Nothing reads its complaints, and a pipe nobody reads fills: a damaged
    // file would stop ffmpeg dead on its own error messages.
    command.stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = ffmpeg::spawn_behind(&mut command).ok()?;
    let mut audio = child.stdout.take()?;
    let mut loudness = Loudness::default();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        if !ticket.current() {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        match audio.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => loudness.feed(&buffer[..n]),
        }
    }
    let _ = child.wait();
    Some(loudness.frames)
}

/// Turns samples into one loudness a frame, as they arrive.
///
/// The samples themselves are never kept: a feature at this rate is over a
/// hundred megabytes of them, and all that is wanted is their size.
#[derive(Default)]
struct Loudness {
    frames: Vec<f32>,
    /// The half of a sample a read ended on.
    odd: Option<u8>,
    last: f32,
    sum: f64,
    count: usize,
}

impl Loudness {
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
            self.sample(f32::from(i16::from_le_bytes([low, high])));
        }
    }

    fn sample(&mut self, x: f32) {
        // Each sample less most of the one before: the pre-emphasis speech
        // coding has always used. It takes out the rumble under a scene —
        // traffic, wind, a score's low end — and leaves the consonants.
        let y = x - 0.95 * self.last;
        self.last = x;
        self.sum += f64::from(y * y);
        self.count += 1;
        if self.count == FRAME {
            // The one inside the logarithm puts digital silence at zero.
            self.frames
                .push((10.0 * (self.sum / FRAME as f64 + 1.0).log10()) as f32);
            self.sum = 0.0;
            self.count = 0;
        }
    }
}

/// Whether there is nothing in a track to measure: none of it, or none of it
/// above what dither and a codec's rounding leave in an empty channel.
pub(super) fn is_silent(loudness: &[f32]) -> bool {
    loudness.iter().all(|&db| db < 10.0)
}

/// How much each frame sounds like someone talking, from 0 to 1.
///
/// Loudness against its surroundings, not against the film: a whisper in a
/// quiet room and a shout over an engine are both the loud part of their
/// scene, and a threshold for the whole film would hear only the second.
/// So each frame is placed between the quiet and the loud of the half minute
/// around it.
pub(super) fn speech(loudness: &[f32]) -> Vec<f32> {
    let n = loudness.len();
    if n == 0 {
        return Vec::new();
    }
    // A voice is syllables, and at a frame apiece the gaps between them
    // would count against a line that is being spoken straight through.
    const SMOOTH: usize = 2;
    let smooth: Vec<f32> = (0..n)
        .map(|i| {
            let run = &loudness[i.saturating_sub(SMOOTH)..(i + SMOOTH + 1).min(n)];
            run.iter().sum::<f32>() / run.len() as f32
        })
        .collect();

    // The surroundings are measured every five seconds over thirty, and
    // read off between the measurements.
    const AROUND: usize = 3000;
    const EVERY: usize = 500;
    let marks: Vec<(f32, f32)> = (0..n)
        .step_by(EVERY)
        .map(|centre| {
            let mut around =
                smooth[centre.saturating_sub(AROUND / 2)..(centre + AROUND / 2).min(n)].to_vec();
            around.sort_by(f32::total_cmp);
            let at = |part: f64| around[((around.len() - 1) as f64 * part) as usize];
            // The quiet is the level a third of the frames are under and the
            // loud the level a tenth are over. At least 6 dB apart, or a
            // stretch of steady noise would be stretched into speech.
            let quiet = at(0.3);
            (quiet, (at(0.9) - quiet).max(6.0))
        })
        .collect();

    (0..n)
        .map(|i| {
            let (a, b) = (i / EVERY, (i / EVERY + 1).min(marks.len() - 1));
            let t = (i % EVERY) as f32 / EVERY as f32;
            let quiet = marks[a].0 + (marks[b].0 - marks[a].0) * t;
            let range = marks[a].1 + (marks[b].1 - marks[a].1) * t;
            ((smooth[i] - quiet) / range).clamp(0.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_split_across_reads_are_put_back_together() {
        let samples: Vec<u8> = (0..FRAME * 3)
            .flat_map(|i| (((i * 37) % 2000) as i16 - 1000).to_le_bytes())
            .collect();
        let mut whole = Loudness::default();
        whole.feed(&samples);
        let mut pieces = Loudness::default();
        for piece in samples.chunks(7) {
            pieces.feed(piece);
        }
        assert_eq!(whole.frames.len(), 3);
        assert_eq!(whole.frames, pieces.frames);
    }
}
