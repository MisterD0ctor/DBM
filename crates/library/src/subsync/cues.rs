//! The subtitles' side: the spans their lines are on screen, read from the
//! file whatever its format and encoding.

use std::path::Path;
use std::process::Stdio;

use crate::ffmpeg;

/// The longest a line counts for. A caption left up for half a minute is a
/// sign or a lyric, not somebody talking for that long.
const LONGEST: f64 = 10.0;

/// A stretch of time with a subtitle on screen, in the subtitles' own clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
}

/// Read a subtitle file as text, whatever it was saved as.
///
/// Only the timings are wanted, which are digits and punctuation that every
/// eight-bit encoding shares with UTF-8, so a Latin-1 file reads correctly
/// here with its accents mangled. UTF-16 is the exception, and says so up
/// front.
pub(super) fn read_text(file: &Path) -> Option<String> {
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
pub(super) fn convert(ffmpeg: &Path, file: &Path) -> Option<String> {
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
        let value = crate::number::finite(&part.replace(',', "."))?;
        if value < 0.0 {
            return None;
        }
        seconds = seconds * 60.0 + value;
        parts += 1;
    }
    (2..=3).contains(&parts).then_some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start: f64, end: f64) -> Cue {
        Cue { start, end }
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
    fn a_timing_that_is_not_a_number_is_no_cue() {
        let text = "1\nnan:00:01,000 --> 00:00:02,000\nOne\n\n\
                    2\n00:00:03,000 --> inf:00:04,000\nTwo\n\n\
                    3\n00:00:05,000 --> 00:00:06,000\nThree\n";
        assert_eq!(cues(text), [cue(5.0, 6.0)]);
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
}
