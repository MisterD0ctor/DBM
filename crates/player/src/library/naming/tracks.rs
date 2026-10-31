//! What to call a subtitle or audio track.

use std::collections::HashMap;

use super::clean::{clean_separators, strip_extension, strip_metadata, strip_path};
use super::language::{base_code, language_name, same_language};
use crate::lang::{fill, tr};

/// A flag a subtitle track carries about itself, rather than a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// Only the signs and the foreign dialogue.
    Forced,
    /// Includes sound effects and speaker labels.
    Sdh,
    /// A commentary track rather than the film's own dialogue.
    Commentary,
}

impl Flag {
    fn label(self) -> &'static str {
        match self {
            Self::Forced => tr("Forced"),
            Self::Sdh => "SDH",
            Self::Commentary => tr("Commentary"),
        }
    }

    fn detect(word: &str) -> Option<Self> {
        match word {
            "forced" | "foreign" => Some(Self::Forced),
            "sdh" | "cc" | "hearingimpaired" | "hearing" => Some(Self::Sdh),
            "commentary" | "comm" => Some(Self::Commentary),
            _ => None,
        }
    }
}

/// What a track's own title turned out to say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackName {
    /// Whatever is left once the flags and the release junk are removed.
    pub text: Option<String>,
    pub flags: Vec<Flag>,
}

/// Read a track title.
///
/// mpv uses the filename as an external subtitle's title, so this copes with
/// `Movie.2019.1080p.WEB-DL.forced.eng.srt` as well as `Director's
/// commentary`. The flags are worth keeping, since whether a subtitle is
/// forced decides whether you want it, and no language code says so.
pub fn parse_track_name(title: &str, language: Option<&str>) -> TrackName {
    let stem = strip_extension(strip_path(title));
    let cleaned = clean_separators(stem);

    // Words are only taken out of something that arrived as a filename
    // (dot- or underscore-separated, or a single token), since taking
    // "commentary" out of `Director's commentary` would leave "Director's".
    let written = stem.contains(' ') && !stem.contains('.') && !stem.contains('_');

    let mut flags = Vec::new();
    let mut kept: Vec<&str> = Vec::new();

    for word in cleaned.split(' ') {
        let bare: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if bare.is_empty() {
            continue;
        }
        if let Some(flag) = Flag::detect(&bare) {
            if !flags.contains(&flag) {
                flags.push(flag);
            }
            if !written {
                continue;
            }
        }
        // The language is already shown beside the name, so it isn't
        // repeated in it.
        if language.is_some_and(|l| same_language(&bare, l)) || base_code(&bare).is_some() {
            continue;
        }
        kept.push(word);
    }

    let text = strip_metadata(&kept.join(" "));
    // A flag the text already says out loud does not need saying twice.
    let lowered = text.to_ascii_lowercase();
    flags.retain(|f| !lowered.contains(&f.label().to_ascii_lowercase()));
    TrackName {
        text: (!text.is_empty()).then_some(text),
        flags,
    }
}

/// One track, as much of it as naming a track needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackInfo<'a> {
    pub title: Option<&'a str>,
    pub language: Option<&'a str>,
    pub external: bool,
    /// mpv's codec name, for a track that says nothing else about itself.
    pub codec: Option<&'a str>,
    /// How many channels, for the same.
    pub channels: Option<i64>,
}

/// What to call each track, given all the others it sits with.
///
/// One list at a time, the subtitles or the audio, since the right label
/// depends on a track's company: with one English track "English" is the
/// whole label, and with three it isn't a label at all.
///
/// Returns labels positionally, in step with the input, since ids aren't
/// unique across lists. Region qualifiers are left out, because telling
/// `es-419` from `es-ES` needs a real locale database.
pub fn track_labels(tracks: &[TrackInfo]) -> Vec<String> {
    let key = |t: &TrackInfo| t.language.map(|l| base_code(l).unwrap_or(l).to_string());

    let mut per_language: HashMap<String, usize> = HashMap::new();
    for t in tracks {
        if let Some(k) = key(t) {
            *per_language.entry(k).or_insert(0) += 1;
        }
    }

    let parsed: Vec<TrackName> = tracks
        .iter()
        .map(|t| {
            t.title
                .map(|title| parse_track_name(title, t.language))
                .unwrap_or_default()
        })
        .collect();
    let untitled = tracks
        .iter()
        .zip(&parsed)
        .filter(|(t, p)| key(t).is_none() && p.text.is_none())
        .count();

    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut unnamed = 0;
    let mut out = Vec::with_capacity(tracks.len());
    for (t, parsed) in tracks.iter().zip(&parsed) {
        let mut label = match (key(t), &parsed.text) {
            (Some(l), Some(text)) => format!("{} — {text}", language_name(&l)),
            (Some(l), None) => {
                let name = language_name(&l);
                // Only number them when the language alone cannot tell them
                // apart; a lone Swedish track is "Svenska", not "Svenska 1".
                if per_language.get(&l).copied().unwrap_or(0) > 1 {
                    let n = seen.entry(l).and_modify(|c| *c += 1).or_insert(1);
                    format!("{name} {n}")
                } else {
                    name
                }
            }
            (None, Some(text)) => text.clone(),
            // No language and no title: said to have none, since a format
            // alone, "SRT", reads as a name the track was given. Numbered
            // among the others like it, and the format after, which is what
            // tells "AAC 5.1" from "AC-3 stereo".
            (None, None) => {
                unnamed += 1;
                let mut label = if untitled > 1 {
                    fill(tr("Untitled {}"), &[&unnamed])
                } else {
                    tr("Untitled").to_string()
                };
                if let Some(format) = by_format(t) {
                    label.push_str(" · ");
                    label.push_str(&format);
                }
                label
            }
        };

        for flag in &parsed.flags {
            label.push_str(" · ");
            label.push_str(flag.label());
        }
        if t.external {
            label.push_str(" · ");
            label.push_str(tr("external"));
        }
        out.push(label);
    }
    out
}

/// A track's format as a person would name it: "AAC 5.1", "E-AC-3 stereo",
/// "ASS". `None` when mpv did not say what the codec is.
fn by_format(t: &TrackInfo) -> Option<String> {
    let codec = codec_name(t.codec?);
    Some(match t.channels.and_then(channel_layout) {
        Some(layout) => format!("{codec} {layout}"),
        None => codec,
    })
}

/// mpv reports FFmpeg's internal codec names, some of which are not what
/// anyone calls the format.
fn codec_name(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "subrip" => "SRT".into(),
        "hdmv_pgs_subtitle" => "PGS".into(),
        "dvd_subtitle" => "VobSub".into(),
        "mov_text" => tr("Timed text").into(),
        "webvtt" => "WebVTT".into(),
        "eac3" => "E-AC-3".into(),
        "ac3" => "AC-3".into(),
        "truehd" => "TrueHD".into(),
        "opus" => "Opus".into(),
        "vorbis" => "Vorbis".into(),
        other => other.to_ascii_uppercase(),
    }
}

/// How a channel count is said, for the few layouts that have a name.
fn channel_layout(channels: i64) -> Option<&'static str> {
    match channels {
        1 => Some("mono"),
        2 => Some("stereo"),
        6 => Some("5.1"),
        8 => Some("7.1"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_filenames_become_flags() {
        let parsed = parse_track_name("Movie.2019.1080p.WEB-DL.forced.eng.srt", Some("eng"));
        assert_eq!(parsed.flags, vec![Flag::Forced]);
        assert_eq!(parsed.text.as_deref(), Some("Movie 2019"));
    }
    #[test]
    fn a_written_title_keeps_its_words() {
        // The flag is recognised but not repeated: the title already says it.
        let parsed = parse_track_name("Director's commentary", Some("eng"));
        assert!(parsed.flags.is_empty());
        assert_eq!(parsed.text.as_deref(), Some("Director's commentary"));
    }
    #[test]
    fn a_bare_flag_is_only_a_flag() {
        let parsed = parse_track_name("forced", Some("eng"));
        assert_eq!(parsed.flags, vec![Flag::Forced]);
        assert_eq!(parsed.text, None);
    }
    fn track(title: Option<&'static str>, lang: Option<&'static str>) -> TrackInfo<'static> {
        TrackInfo {
            title,
            language: lang,
            external: false,
            codec: None,
            channels: None,
        }
    }
    #[test]
    fn a_track_that_says_nothing_is_untitled_in_its_format() {
        let bare = |codec, channels| TrackInfo {
            title: None,
            language: None,
            external: false,
            codec: Some(codec),
            channels,
        };
        assert_eq!(
            track_labels(&[bare("aac", Some(6))]),
            ["Untitled · AAC 5.1"]
        );
        assert_eq!(track_labels(&[bare("subrip", None)]), ["Untitled · SRT"]);
        assert_eq!(
            track_labels(&[bare("subrip", None), bare("ass", None)]),
            ["Untitled 1 · SRT", "Untitled 2 · ASS"]
        );
        assert_eq!(track_labels(&[track(None, None)]), ["Untitled"]);
    }
    #[test]
    fn one_track_per_language_is_not_numbered() {
        let labels = track_labels(&[track(None, Some("eng")), track(None, Some("swe"))]);
        assert_eq!(labels, ["English", "Svenska"]);
    }
    #[test]
    fn several_of_one_language_are_numbered() {
        let labels = track_labels(&[track(None, Some("eng")), track(None, Some("en-US"))]);
        assert_eq!(labels, ["English 1", "English 2"]);
    }
    #[test]
    fn a_title_keeps_its_own_words() {
        let labels = track_labels(&[track(Some("Director's commentary"), Some("eng"))]);
        assert_eq!(labels, ["English — Director's commentary"]);
    }
    #[test]
    fn flags_ride_along() {
        let labels = track_labels(&[TrackInfo {
            title: Some("forced"),
            language: Some("eng"),
            external: true,
            codec: None,
            channels: None,
        }]);
        assert_eq!(labels, ["English · Forced · external"]);
    }
}
