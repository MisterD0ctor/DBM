//! What to call a subtitle or audio track.

use std::collections::HashMap;

use super::clean::{clean_separators, strip_extension, strip_metadata, strip_path};
use super::language::{base_code, language_name, same_language};

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
            Self::Forced => "Forced",
            Self::Sdh => "SDH",
            Self::Commentary => "Commentary",
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
/// mpv uses the filename as the title of an external subtitle, so this has to
/// cope with `Movie.2019.1080p.WEB-DL.forced.eng.srt` as readily as with
/// `Director's commentary`. The flags are the part worth keeping: whether a
/// subtitle is forced decides whether you want it, and no language code says
/// so.
pub fn parse_track_name(title: &str, language: Option<&str>) -> TrackName {
    let stem = strip_extension(strip_path(title));
    let cleaned = clean_separators(stem);

    // A title someone typed reads as a sentence; a filename reads as tokens.
    // The difference matters: pulling "commentary" out of `Director's
    // commentary` as a flag leaves "Director's", which is not a name for
    // anything. So words are only taken out of something that arrived as a
    // filename — dot- or underscore-separated, or a single token.
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
        // The language is already shown beside the name; repeating it in the
        // name too is how you get "English — English".
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
    /// Only for the last-resort label. mpv numbers tracks within a type, so
    /// an id is not unique across a file and cannot key anything.
    pub id: i64,
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
/// One list at a time — the subtitles, or the audio, not both. The right
/// label depends on the company a track keeps: with one English track
/// "English" is the whole label, and with three it is not a label at all. Two
/// English subtitles and two English audio tracks are four tracks but two
/// lists, and numbering them 1-4 would be answering a question nobody asked.
///
/// Returns labels positionally, in step with the input, for the same reason
/// the id is only a fallback.
///
/// Mirrors what the Tauri build did, minus the region qualifiers — telling
/// `es-419` from `es-ES` needs a real locale database.
pub fn track_labels(tracks: &[TrackInfo]) -> Vec<String> {
    let key = |t: &TrackInfo| t.language.map(|l| base_code(l).unwrap_or(l).to_string());

    let mut per_language: HashMap<String, usize> = HashMap::new();
    for t in tracks {
        if let Some(k) = key(t) {
            *per_language.entry(k).or_insert(0) += 1;
        }
    }

    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(tracks.len());
    for t in tracks {
        let parsed = t
            .title
            .map(|title| parse_track_name(title, t.language))
            .unwrap_or_default();

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
            // No language and no title: named by what it technically is.
            // "AAC 5.1" is true and tells two tracks apart; "Track 1" said
            // only that a track existed, and read the same in both lists.
            (None, None) => by_format(t).unwrap_or_else(|| format!("Track {}", t.id)),
        };

        for flag in &parsed.flags {
            label.push_str(" · ");
            label.push_str(flag.label());
        }
        if t.external {
            label.push_str(" · external");
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
        "mov_text" => "Timed text".into(),
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
    fn track(
        id: i64,
        title: Option<&'static str>,
        lang: Option<&'static str>,
    ) -> TrackInfo<'static> {
        TrackInfo {
            id,
            title,
            language: lang,
            external: false,
            codec: None,
            channels: None,
        }
    }
    #[test]
    fn a_track_that_says_nothing_is_named_by_its_format() {
        let bare = |codec, channels| TrackInfo {
            id: 1,
            title: None,
            language: None,
            external: false,
            codec: Some(codec),
            channels,
        };
        assert_eq!(track_labels(&[bare("aac", Some(6))]), ["AAC 5.1"]);
        assert_eq!(track_labels(&[bare("subrip", None)]), ["SRT"]);
        assert_eq!(track_labels(&[bare("eac3", Some(2))]), ["E-AC-3 stereo"]);
    }
    #[test]
    fn one_track_per_language_is_not_numbered() {
        let labels = track_labels(&[track(1, None, Some("eng")), track(2, None, Some("swe"))]);
        assert_eq!(labels, ["English", "Svenska"]);
    }
    #[test]
    fn several_of_one_language_are_numbered() {
        let labels = track_labels(&[track(1, None, Some("eng")), track(2, None, Some("en-US"))]);
        assert_eq!(labels, ["English 1", "English 2"]);
    }
    #[test]
    fn ids_repeat_across_a_file_and_must_not_key_anything() {
        // mpv numbers within a type, so the subtitle list has its own id 1.
        // Labels come back positionally for exactly this reason.
        let labels = track_labels(&[track(1, None, Some("eng")), track(1, None, Some("swe"))]);
        assert_eq!(labels, ["English", "Svenska"]);
    }
    #[test]
    fn a_title_keeps_its_own_words() {
        let labels = track_labels(&[track(1, Some("Director's commentary"), Some("eng"))]);
        assert_eq!(labels, ["English — Director's commentary"]);
    }
    #[test]
    fn flags_ride_along() {
        let labels = track_labels(&[TrackInfo {
            id: 1,
            title: Some("forced"),
            language: Some("eng"),
            external: true,
            codec: None,
            channels: None,
        }]);
        assert_eq!(labels, ["English · Forced · external"]);
    }
}
