//! Lining subtitles up with the speech they belong to.
//!
//! Subtitles that came from somewhere else — a download, or a track muxed in
//! from another release — run early or late by a fixed amount, and sometimes
//! at the speed of another frame rate. Nothing in the subtitles says so. The
//! audio does: people talk while lines are on screen and mostly not between
//! them, so the right timing is the one that puts the lines over the talking.
//!
//! No speech is recognised, and the language of either side does not matter.
//! The audio is reduced to one number every hundredth of a second — how much
//! sound there is against what surrounds it — and the subtitles to the spans
//! they are on screen for. Every delay within a minute either way is then
//! tried, at each of the speeds a frame-rate mismatch produces, and scored by
//! how much of the sound falls inside the spans.
//!
//! ffmpeg does the decoding, so with none on the machine there is no sync —
//! see `ffmpeg`. It reads the film once: the audio comes back down a pipe,
//! and a subtitle track inside the same file is written out beside it in the
//! same pass, since getting at either means reading the whole container.
//!
//! Its own thread, like the seek preview and for its reason: a feature is a
//! quarter of a minute of decoding, and the shared worker is what keeps the
//! track lists current. A generation counter abandons a run whose film has
//! been replaced or which has been asked for again.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::library::ffmpeg;
use crate::playback::mpv::Mpv;
use crate::worker::{Completion, Reporter};

/// What ffmpeg is asked for: mono at this rate, which keeps everything a
/// voice has below 4 kHz and is an eighth of the samples a film carries.
const RATE: usize = 8000;
/// Samples to a frame: a hundredth of a second, which is finer than anyone
/// can time a subtitle by eye.
const FRAME: usize = RATE / 100;
/// Frames to a second.
const PER_SECOND: f64 = (RATE / FRAME) as f64;

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
/// The longest a line counts for. A caption left up for half a minute is a
/// sign or a lyric, not somebody talking for that long.
const LONGEST: f64 = 10.0;

/// A stretch of time with a subtitle on screen, in the subtitles' own clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
}

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
    /// The track is pictures — a disc's own subtitles — and has no text to
    /// take timings from. Those are cut with the film and are in step.
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

/// A finished run, and what it was a run for — so an answer that arrives
/// after the film or the track has changed can be dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub path: String,
    pub sid: Option<String>,
    pub result: Result<Fit, Failure>,
}

/// Bumped on every request and on every new file. A run captures the value it
/// started with and stops once the global one has moved past it.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Abandon any run in progress, and return the generation a new one should
/// carry. Called when the file changes and when a sync is asked for.
pub fn supersede() -> u64 {
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

fn current(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) == generation
}

/// Sync the selected subtitles to the playing film's speech, off the UI
/// thread, and report the answer like any other background result.
///
/// Returns immediately. A run that is superseded reports nothing; every
/// other one reports, because the interface is showing that it is listening
/// and has to be told when to stop.
pub fn spawn(mpv: Arc<Mpv>, reporter: Reporter) {
    let generation = supersede();
    std::thread::spawn(move || {
        // Read here rather than handed over: these block on mpv's core
        // lock, which is exactly what the UI thread may not do.
        let path = mpv.get_property("path").unwrap_or_default();
        let sid = mpv.get_property("sid");
        let result = ask(&mpv, &path).and_then(|asked| run(&asked, generation));
        if !current(generation) {
            return;
        }
        match &result {
            Ok(fit) => eprintln!(
                "dbm: subtitles fit at {:+.1} s, speed {:.4}",
                fit.delay, fit.speed
            ),
            Err(why) => eprintln!("dbm: subtitles not synced: {why:?}"),
        }
        reporter.send(Completion::Synced(Outcome { path, sid, result }));
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
    if is_pictures(&codec) {
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

/// Whether a subtitle codec, in mpv's name for it, is bitmaps.
fn is_pictures(codec: &str) -> bool {
    ["pgs", "dvd", "dvb", "xsub", "vobsub"]
        .iter()
        .any(|kind| codec.contains(kind))
}

/// Whether a track with this many channels can be relied on to have a centre.
///
/// Five and up always do — 5.0, 5.1, 6.1, 7.1. Three and four may not: 2.1
/// and quad are both real layouts, and asking ffmpeg for a channel that is
/// not there gets silence rather than an error.
fn has_centre(channels: i64) -> bool {
    channels >= 5
}

fn run(asked: &Asked, generation: u64) -> Result<Fit, Failure> {
    let ffmpeg = ffmpeg::path().ok_or(Failure::NoFfmpeg)?;
    ffmpeg::announce(&ffmpeg);

    // A track inside the film is written out by the same ffmpeg that decodes
    // the audio, to a file that is gone again when this returns.
    let scratch = match asked.subtitles {
        Source::Stream(_) => Some(Scratch::new(generation)),
        Source::File(_) => None,
    };
    let stream = match (&asked.subtitles, &scratch) {
        (Source::Stream(index), Some(scratch)) => Some((*index, scratch.0.as_path())),
        _ => None,
    };

    let mut loudness = listen(&ffmpeg, asked, asked.centre, stream, generation);
    // The centre channel is where the voices are when there is one, and with
    // the music and the effects left in the others it is most of what makes
    // this work on a film. If it came back empty the track did not have one
    // after all, and everything mixed down is the fallback.
    if asked.centre && loudness.as_deref().is_some_and(is_silent) {
        loudness = listen(&ffmpeg, asked, false, stream, generation);
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
    // Said whether or not it is believed: "it would not sync" is otherwise a
    // report with nothing in it to go on.
    eprintln!(
        "dbm: {} subtitle spans against {:.0} s of audio: best at {:+.2} s, \
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
    fn new(generation: u64) -> Self {
        Self(std::env::temp_dir().join(format!(
            "dbm-subtitles-{}-{generation}.srt",
            std::process::id()
        )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Decode the film's audio and return how loud each frame of it is, in
/// decibels. `stream` asks for a subtitle track to be written out as SubRip
/// along the way.
///
/// `None` for a run that was superseded or an ffmpeg that would not start.
/// One that starts and produces nothing returns an empty list, which the
/// caller reads as silence.
fn listen(
    ffmpeg: &Path,
    asked: &Asked,
    centre: bool,
    stream: Option<(i64, &Path)>,
    generation: u64,
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
        if !current(generation) {
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
            self.sample(i16::from_le_bytes([low, high]) as f32);
        }
    }

    fn sample(&mut self, x: f32) {
        // Each sample less most of the one before: the pre-emphasis speech
        // coding has always used. It takes out the rumble under a scene —
        // traffic, wind, a score's low end — and leaves the consonants.
        let y = x - 0.95 * self.last;
        self.last = x;
        self.sum += (y * y) as f64;
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
fn is_silent(loudness: &[f32]) -> bool {
    loudness.iter().all(|&db| db < 10.0)
}

/// How much each frame sounds like someone talking, from 0 to 1.
///
/// Loudness against its surroundings, not against the film: a whisper in a
/// quiet room and a shout over an engine are both the loud part of their
/// scene, and a threshold for the whole film would hear only the second.
/// So each frame is placed between the quiet and the loud of the half minute
/// around it.
fn speech(loudness: &[f32]) -> Vec<f32> {
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

/// Read a subtitle file as text, whatever it was saved as.
///
/// Only the timings are wanted and they are digits and punctuation, which
/// every eight-bit encoding agrees with UTF-8 about — so a Latin-1 file
/// reads correctly here with its accents mangled, and nothing needs to know
/// which encoding it was. UTF-16 is the exception, and says so up front.
fn read_text(file: &Path) -> Option<String> {
    let bytes = std::fs::read(file).ok()?;
    let wide = |unit: fn([u8; 2]) -> u16| {
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        char::decode_utf16(pairs.iter().map(|&pair| unit(pair)))
            .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect::<String>()
    };
    Some(match bytes.as_slice() {
        [0xff, 0xfe, ..] => wide(u16::from_le_bytes),
        [0xfe, 0xff, ..] => wide(u16::from_be_bytes),
        _ => String::from_utf8_lossy(&bytes).into_owned(),
    })
}

/// Have ffmpeg turn a subtitle file into SubRip, for the formats `cues` does
/// not read itself. **Blocking**, but on a file of a few hundred lines.
fn convert(ffmpeg: &Path, file: &Path) -> Option<String> {
    let out = ffmpeg::command(ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(file)
        .args(["-f", "srt", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The times lines are on screen, from SubRip, WebVTT or Advanced SubStation
/// text. In order, and with lines that overlap or touch run together: what
/// is wanted is when anything is on screen, not how many things.
pub fn cues(text: &str) -> Vec<Cue> {
    let mut found: Vec<Cue> = text
        .lines()
        .filter_map(|line| {
            let (start, end) = match line.split_once("-->") {
                // SubRip and WebVTT: `00:01:02,500 --> 00:01:04,000`, with
                // WebVTT free to put layout after the second time.
                Some((start, rest)) => (start, rest.split_whitespace().next()?),
                // Advanced SubStation: `Dialogue: 0,0:01:02.50,0:01:04.00,...`
                None => {
                    let mut fields = line.strip_prefix("Dialogue:")?.split(',');
                    fields.next()?;
                    (fields.next()?, fields.next()?)
                }
            };
            let (start, end) = (clock(start)?, clock(end)?);
            (end > start).then(|| Cue {
                start,
                end: end.min(start + LONGEST),
            })
        })
        .collect();
    found.sort_by(|a, b| a.start.total_cmp(&b.start));

    let mut spans: Vec<Cue> = Vec::with_capacity(found.len());
    for cue in found {
        match spans.last_mut() {
            Some(last) if cue.start <= last.end => last.end = last.end.max(cue.end),
            _ => spans.push(cue),
        }
    }
    spans
}

/// `1:02:03.456`, `01:02:03,456` or WebVTT's hourless `02:03.456`, in seconds.
fn clock(text: &str) -> Option<f64> {
    let mut seconds = 0.0;
    let mut parts = 0;
    for part in text.trim().split(':') {
        let value: f64 = part.trim().replace(',', ".").parse().ok()?;
        if value < 0.0 {
            return None;
        }
        seconds = seconds * 60.0 + value;
        parts += 1;
    }
    (2..=3).contains(&parts).then_some(seconds)
}

/// The best timing a search found, and how far it stood above the others.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Best {
    delay: f64,
    speed: f64,
    score: f64,
    /// How many standard deviations the score is above the mean of every
    /// delay tried at this speed.
    clear: f64,
}

impl Best {
    /// The timing to use, if it stood out enough to be one.
    fn believed(self) -> Option<Fit> {
        (self.clear >= SURE).then(|| Fit {
            // Plus nothing, which is what turns a rounded -0.0 into 0.0.
            delay: (self.delay * GRAINS).round() / GRAINS + 0.0,
            speed: self.speed,
        })
    }
}

/// Try every timing and return the best, or `None` with too little to try.
///
/// The score for one timing is the speech inside the spans less the speech
/// that would be there by chance — which, with the film's average taken off
/// every frame first, is simply the sum inside them. A running total of the
/// frames makes that two lookups a span, so a timing costs as many steps as
/// there are lines and not as many as there are frames: the whole search is
/// a hundred million lookups, where sliding one signal along the other would
/// be tens of billions of multiplications.
fn search(speech: &[f32], cues: &[Cue]) -> Option<Best> {
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
        let mean = speech.iter().map(|&s| s as f64).sum::<f64>() / speech.len() as f64;
        let mut total = Vec::with_capacity(speech.len() + 1);
        let mut sum = 0.0;
        total.push(0.0);
        for &s in speech {
            sum += s as f64 - mean;
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

    #[test]
    fn subrip_timings_are_read() {
        let text = "1\n00:00:23,223 --> 00:00:25,723\nIt's probably stupid.\n\n\
                    2\n01:02:03,500 --> 01:02:05,000\nDangerous.\n";
        assert_eq!(cues(text), [cue(23.223, 25.723), cue(3723.5, 3725.0)]);
    }

    #[test]
    fn webvtt_timings_are_read_with_or_without_hours() {
        let text = "WEBVTT\n\n00:01.000 --> 00:02.500 align:start position:10%\nHello\n\n\
                    00:01:10.000 --> 00:01:12.000\nAgain\n";
        assert_eq!(cues(text), [cue(1.0, 2.5), cue(70.0, 72.0)]);
    }

    #[test]
    fn substation_timings_are_read() {
        let text = "[Events]\nFormat: Layer, Start, End, Style, Text\n\
                    Dialogue: 0,0:00:17.90,0:00:20.74,Default,,0,0,0,,Gareth: It's stupid.\n\
                    Comment: 0,0:00:30.00,0:00:31.00,Default,,0,0,0,,not a line\n";
        assert_eq!(cues(text), [cue(17.9, 20.74)]);
    }

    #[test]
    fn lines_that_overlap_become_one_span_and_come_out_in_order() {
        let text = "00:00:10,000 --> 00:00:12,000\n\
                    00:00:05,000 --> 00:00:06,000\n\
                    00:00:11,000 --> 00:00:13,000\n";
        assert_eq!(cues(text), [cue(5.0, 6.0), cue(10.0, 13.0)]);
    }

    #[test]
    fn a_caption_left_up_counts_for_only_so_long() {
        let text = "00:00:10,000 --> 00:01:10,000\n";
        assert_eq!(cues(text), [cue(10.0, 10.0 + LONGEST)]);
    }

    #[test]
    fn lines_that_are_not_timings_are_passed_over() {
        let text = "1\nHe said --> and left.\n00:00:02,000 --> 00:00:01,000\n";
        assert!(cues(text).is_empty());
    }

    #[test]
    fn a_subtitle_file_saved_as_utf_16_is_still_read() {
        let text = "1\r\n00:00:01,000 --> 00:00:02,000\r\nHall\u{e5}\r\n";
        let file = std::env::temp_dir().join(format!("dbm-utf16-{}.srt", std::process::id()));
        for big in [false, true] {
            let mut bytes: Vec<u8> = if big {
                vec![0xfe, 0xff]
            } else {
                vec![0xff, 0xfe]
            };
            for unit in text.encode_utf16() {
                bytes.extend(if big {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                });
            }
            std::fs::write(&file, bytes).unwrap();
            assert_eq!(cues(&read_text(&file).unwrap()), [cue(1.0, 2.0)]);
        }
        // And one that is not Unicode at all: only the timings are wanted.
        std::fs::write(&file, b"00:00:03,000 --> 00:00:04,000\nHall\xe5\n").unwrap();
        assert_eq!(cues(&read_text(&file).unwrap()), [cue(3.0, 4.0)]);
        let _ = std::fs::remove_file(&file);
    }

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

    #[test]
    fn picture_subtitles_are_told_from_text() {
        for codec in ["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"] {
            assert!(is_pictures(codec), "{codec}");
        }
        for codec in ["subrip", "ass", "webvtt", "mov_text"] {
            assert!(!is_pictures(codec), "{codec}");
        }
    }
}
