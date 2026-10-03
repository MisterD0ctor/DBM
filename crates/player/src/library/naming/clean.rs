//! Cleaners: the separators, extensions, release tags and group signatures
//! that wrap the part of a name a person wrote.
//!
//! Shared by everything else here — a filename and an external subtitle's
//! title arrive wrapped in the same junk.

/// Strip leading directory components, for either platform's separator.
pub fn strip_path(name: &str) -> &str {
    match name.rfind(['/', '\\']) {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

/// Strip a trailing extension.
///
/// Only a short one: a dot four characters from the end of
/// `Movie.2019.WEB-DL` is a separator, not an extension, and cutting there
/// would eat half the name.
pub fn strip_extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 5 => &name[..i],
        _ => name,
    }
}

/// Dots and underscores used as word separators become spaces.
///
/// Not every dot: `5.1` and `H.264` are one token each, and splitting them
/// turns a clean name into a scattering of digits. A dot is a separator only
/// when it is not sitting between two digits, or between a single letter and
/// a digit.
pub fn clean_separators(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        let separator = match c {
            '_' => true,
            '.' => {
                let before = i.checked_sub(1).map(|j| chars[j]);
                let after = chars.get(i + 1).copied();
                let digit_at = |j: Option<usize>| {
                    j.and_then(|j| chars.get(j))
                        .is_some_and(|c| c.is_ascii_digit())
                };
                match (before, after) {
                    // 5.1, 7.1, 5.1.2 — a channel layout is single digits on
                    // both sides. `2019.1080p` is digits either side too, and
                    // the run length is the only thing that separates them.
                    (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => {
                        let lone_before = !digit_at(i.checked_sub(2));
                        let lone_after = !digit_at(Some(i + 2));
                        !(lone_before && lone_after)
                    }
                    // H.264, x.265 — but only where the letter stands
                    // alone. In `Movie.2019` the dot is a separator, and the
                    // only thing telling the two apart is what precedes the
                    // letter.
                    (Some(a), Some(b)) if a.is_ascii_alphabetic() && b.is_ascii_digit() => {
                        let lone = i
                            .checked_sub(2)
                            .map(|j| chars[j])
                            .is_none_or(|p| !p.is_alphanumeric());
                        !lone
                    }
                    _ => true,
                }
            }
            _ => false,
        };
        out.push(if separator { ' ' } else { c });
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Words that mean "this is a release", not "this is the film".
#[rustfmt::skip]
const METADATA: &[&str] = &[
    // Resolution
    "480p", "576p", "720p", "1080p", "1080i", "2160p", "4k", "uhd",
    // Source
    "bluray", "blu-ray", "bdrip", "bdremux", "remux", "brrip", "webrip", "web-dl", "webdl",
    "web", "hdtv", "pdtv", "dvdrip", "dvd", "hdrip", "hdcam", "cam", "telesync", "telecine",
    "amzn", "nf", "dsnp", "hmax", "atvp", "pcok", "hulu", "stan", "ip",
    // Video codec
    "x264", "x265", "h264", "h265", "h.264", "h.265", "hevc", "avc", "av1", "vp9", "mpeg2",
    "xvid", "divx", "10bit", "8bit",
    // Dynamic range
    "hdr", "hdr10", "hdr10+", "hdr10plus", "dv", "sdr", "hlg",
    // Audio
    "aac", "aac2", "aac5", "ac3", "eac3", "dts", "dts-hd", "dtshd", "dts-x", "flac", "truehd",
    "atmos", "ddp5", "ddp", "dd5", "dd+", "dd", "lpcm", "mp3", "opus", "2ch", "6ch", "8ch",
    // Edition and packaging
    "proper", "repack", "internal", "limited", "extended", "unrated", "uncut", "imax",
    "hybrid", "multi", "dual",
];

/// Cut the name at the first release tag.
///
/// Truncating rather than removing word by word, because everything after the
/// first tag is metadata too — `1080p WEB-DL x264-GROUP` has no title hiding
/// in the middle of it.
pub fn strip_metadata(s: &str) -> String {
    let words: Vec<&str> = s.split(' ').collect();
    for (i, w) in words.iter().enumerate() {
        if is_metadata(w) {
            return trim_junk(&words[..i].join(" "));
        }
    }
    trim_junk(s)
}

/// Whether one word is a release marker rather than part of a name.
///
/// Split out so the same judgement serves two callers: cutting a name short
/// at the first marker, and deciding whether a container's title tag was
/// written by a person at all.
fn is_metadata(word: &str) -> bool {
    let bare = word
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '+' && c != '.' && c != '-')
        .to_ascii_lowercase();
    if bare.is_empty() {
        return false;
    }
    // A release group trailing the codec, as in `x264-GRP`.
    let head = bare.split('-').next().unwrap_or(&bare);
    METADATA.contains(&bare.as_str()) || METADATA.contains(&head)
}

/// Whether one word could only have come from a release name.
///
/// Narrower than [`is_metadata`] on purpose. Most of `METADATA` is ordinary
/// English — a film can be called *The Extended Cut*, a title can contain
/// *Proper*, and *Stan* is a name — so that list is right for cutting a file
/// name short and wrong for judging whether a person wrote something. What
/// cannot appear in prose is a marker carrying a digit (`1080p`, `x265`,
/// `10bit`, `6ch`) or one of the few spelled-out technical words below.
fn is_release_token(word: &str) -> bool {
    #[rustfmt::skip]
    const NEVER_IN_PROSE: &[&str] = &[
        "bluray", "blu-ray", "bdrip", "bdremux", "brrip", "webrip", "web-dl", "webdl",
        "hdtv", "pdtv", "dvdrip", "hdrip", "hdcam", "telesync", "telecine", "hevc",
        "avc", "xvid", "divx", "truehd", "dtshd", "dts-hd", "dts-x", "flac", "lpcm",
    ];
    let bare = word
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '+' && c != '.' && c != '-')
        .to_ascii_lowercase();
    if bare.is_empty() {
        return false;
    }
    let head = bare.split('-').next().unwrap_or(&bare);
    if NEVER_IN_PROSE.contains(&bare.as_str()) || NEVER_IN_PROSE.contains(&head) {
        return true;
    }
    bare.chars().any(|c| c.is_ascii_digit()) && is_metadata(&bare)
}

/// Whether a container's title tag is a title, or the file name in disguise.
///
/// mpv reports the container's `title` tag, and the rule used to be "if it is
/// not exactly the file name, a person wrote it". Packers are not so tidy. One
/// real file carries
///
/// ```text
/// PSArips.com | Game.of.Thrones.S07E01.Dragonstone.1080p.10bit.BluRay.6CH.x265.HEVC-PSA
/// ```
///
/// which is not equal to the file name, so it passed the old test and landed
/// whole — advertisement included — in the slot meant for the one part of the
/// name a human actually typed.
///
/// Two cheap tests catch it, and both have to pass for the title to be used.
/// Nobody types `1080p` into an episode title; and a title that contains the
/// file's own stem is the file name wearing a hat, however much has been
/// bolted on either side of it.
pub(super) fn authored(embedded: &str, path: &str) -> bool {
    if clean_separators(embedded).split(' ').any(is_release_token) {
        return false;
    }
    let stem = comparable(strip_extension(strip_path(path)));
    !stem.is_empty() && !comparable(embedded).contains(&stem)
}

/// Lower case, separators as spaces, runs of space collapsed — enough to
/// compare a title against a file name without either's punctuation deciding
/// the answer.
fn comparable(s: &str) -> String {
    clean_separators(s)
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drop a bracketed tag from the front of a name.
///
/// A name opening with `[Group]` or `(Group)` is a release group's signature,
/// not part of the title — the convention is near-universal in fansubbing and
/// nowhere does a film actually begin with a bracket.
pub fn strip_group(s: &str) -> &str {
    let s = s.trim_start();
    let close = match s.chars().next() {
        Some('[') => ']',
        Some('(') => ')',
        _ => return s,
    };
    match s.find(close) {
        Some(i) => s[i + 1..].trim_start(),
        None => s,
    }
}

/// Trim the punctuation a cut leaves dangling.
pub(super) fn trim_junk(s: &str) -> String {
    const JUNK: [char; 12] = ['-', '–', '.', ',', '(', ')', '[', ']', '{', '}', '_', ' '];
    s.trim().trim_matches(JUNK).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_keep_numbers_together() {
        assert_eq!(clean_separators("Movie.DD5.1.H.264"), "Movie DD5.1 H.264");
        assert_eq!(clean_separators("Show.Name__Title"), "Show Name Title");
    }
    #[test]
    fn extension_only_when_short() {
        assert_eq!(strip_extension("show.mkv"), "show");
        assert_eq!(strip_extension("Movie.2019.WEB-DL"), "Movie.2019.WEB-DL");
        assert_eq!(strip_extension(".hidden"), ".hidden");
    }
    #[test]
    fn paths_are_stripped() {
        assert_eq!(strip_path(r"C:\videos\show.mkv"), "show.mkv");
        assert_eq!(strip_path("/videos/show.mkv"), "show.mkv");
    }
}
