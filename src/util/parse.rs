//! Port of `parse.js`. Filename → TV-show breakdown + utility cleaners.
//!
//! Differs from the JS version in one respect: `parse_tv_show` does not
//! strip the file extension internally — callers do that explicitly via
//! [`strip_extension`]. The JS version stripped on every dot, which
//! truncated titles containing dots (e.g. `"... Episode.Title"`).

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TvShow {
    pub show: String,
    pub season: u32,
    pub episode: u32,
    pub episode_end: Option<u32>,
    pub title: Option<String>,
}

fn tv_regex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(concat!(
            r"^(?P<show>.+?)",
            r"\s*[-.]?\s*",
            r"[Ss](?P<season>\d{1,2})",
            r"\s*[Ee](?P<episode>\d{1,2})",
            r"(?:\s*[-.]?\s*[Ee](?P<episode_end>\d{1,2}))?",
            r"\s*[-.]?\s*",
            r"(?P<title>.*?)$",
        ))
        .expect("tv regex compiles")
    })
}

fn metadata_tags() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| {
        const TAGS: &[&str] = &[
            // Resolution
            "480p", "576p", "720p", "1080p", "1080i", "2160p", "4k",
            // Source
            "bluray", "bdrip", "brrip", "webrip", "web-dl", "webdl", "web", "hdtv", "dvdrip",
            "hdrip", "hdcam", "cam", "ts", "telesync", "amzn", "nf", "dsnp", "hmax", "atvp",
            "pcok", "hulu", "cr", "it",
            // Video codec
            "x264", "x265", "h264", "h265", "h.264", "h.265", "hevc", "avc", "av1", "vp9",
            "mpeg2", "xvid", "divx",
            // HDR
            "hdr", "hdr10", "hdr10+", "hdr10plus", "dolby vision", "dv", "sdr",
            // Audio codec
            "aac", "aac5", "ac3", "eac3", "dts", "dts-hd", "dtshd", "flac", "truehd", "atmos",
            "ddp5", "ddp", "dd5", "dd", "lpcm", "mp3",
        ];
        TAGS.iter().copied().collect()
    })
}

/// Parse a filename (no extension) like `"Show - S01E02 - Episode Title"`.
/// Returns `None` if the season/episode marker isn't found.
pub fn parse_tv_show(name: &str) -> Option<TvShow> {
    if name.is_empty() {
        return None;
    }
    let caps = tv_regex().captures(name)?;
    let show = clean_separators(caps.name("show")?.as_str());
    let season = caps.name("season")?.as_str().parse().ok()?;
    let episode = caps.name("episode")?.as_str().parse().ok()?;
    let episode_end = caps
        .name("episode_end")
        .and_then(|m| m.as_str().parse().ok());

    let raw_title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
    let cleaned = strip_metadata(&clean_separators(raw_title));
    let title = if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    };

    Some(TvShow {
        show,
        season,
        episode,
        episode_end,
        title,
    })
}

/// Replace dots/underscores used as word separators and trim/collapse spaces.
pub fn clean_separators(s: &str) -> String {
    s.chars()
        .map(|c| if c == '.' || c == '_' { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Truncate at the first word that matches a known release-metadata tag.
pub fn strip_metadata(s: &str) -> String {
    let words: Vec<&str> = s.split(' ').collect();
    let tags = metadata_tags();
    for (i, w) in words.iter().enumerate() {
        let bare: String = w
            .chars()
            .filter(|c| !matches!(c, '(' | ')' | '[' | ']' | '{' | '}'))
            .collect::<String>()
            .to_lowercase();
        if !bare.is_empty() && tags.contains(bare.as_str()) {
            let mut result = words[..i].join(" ");
            // Strip trailing junk left over from the cut point.
            while result.ends_with(['-', ' ', '(']) {
                result.pop();
            }
            return result;
        }
    }
    s.to_string()
}

/// Strip leading directory components. Handles both Windows and POSIX separators.
pub fn strip_path(filename: &str) -> &str {
    let bs = filename.rfind('\\');
    let fs = filename.rfind('/');
    let cut = match (bs, fs) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    };
    match cut {
        Some(idx) if idx > 0 => &filename[idx + 1..],
        _ => filename,
    }
}

/// Strip a trailing extension, if any. Does nothing for hidden-file-like
/// names (`.dotfile`) where the dot is at index 0.
pub fn strip_extension(filename: &str) -> &str {
    match filename.rfind('.') {
        Some(idx) if idx > 0 => &filename[..idx],
        _ => filename,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashed_filename() {
        let tv = parse_tv_show("Show Name - S01E02 - Episode Title").unwrap();
        assert_eq!(tv.show, "Show Name");
        assert_eq!(tv.season, 1);
        assert_eq!(tv.episode, 2);
        assert_eq!(tv.episode_end, None);
        assert_eq!(tv.title.as_deref(), Some("Episode Title"));
    }

    #[test]
    fn dotted_filename() {
        let tv = parse_tv_show("Show.Name.S01E02.Episode.Title").unwrap();
        assert_eq!(tv.show, "Show Name");
        assert_eq!(tv.title.as_deref(), Some("Episode Title"));
    }

    #[test]
    fn double_episode() {
        let tv = parse_tv_show("Show - S01E02-E03 - Two Parter").unwrap();
        assert_eq!(tv.episode, 2);
        assert_eq!(tv.episode_end, Some(3));
    }

    #[test]
    fn strips_metadata_from_title() {
        let tv = parse_tv_show("Show - S01E02 - Title 1080p WEB-DL x264").unwrap();
        assert_eq!(tv.title.as_deref(), Some("Title"));
    }

    #[test]
    fn no_match() {
        assert!(parse_tv_show("just a filename").is_none());
    }

    #[test]
    fn strip_path_windows() {
        assert_eq!(strip_path(r"C:\videos\show.mkv"), "show.mkv");
    }

    #[test]
    fn strip_path_posix() {
        assert_eq!(strip_path("/videos/show.mkv"), "show.mkv");
    }

    #[test]
    fn strip_extension_basic() {
        assert_eq!(strip_extension("show.mkv"), "show");
        assert_eq!(strip_extension("show"), "show");
        assert_eq!(strip_extension(".hidden"), ".hidden");
    }

    #[test]
    fn clean_separators_basic() {
        assert_eq!(clean_separators("Show.Name__Title"), "Show Name Title");
        assert_eq!(clean_separators("  a  b  "), "a b");
    }
}
