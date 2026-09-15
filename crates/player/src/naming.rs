//! Turning file and track names into something worth reading.
//!
//! Two jobs, both of them guesswork over conventions nobody agreed on:
//!
//! * A filename like `Show.Name.S01E02.Episode.Title.1080p.WEB-DL.x264-GRP`
//!   has a show, a season, an episode and a title in it, wrapped in release
//!   metadata that means nothing once the file is playing. Fansub releases
//!   often have no season at all — `[Group] Show - 01 (1080p)` — and number
//!   a show's episodes straight through.
//! * A track carries a language code, sometimes a title, and — for external
//!   subtitles, where mpv uses the filename as the title — the same release
//!   junk again, plus the flags that actually matter: forced, SDH, commentary.
//!
//! Everything here is a pure function over strings. No mpv, no UI, no I/O:
//! the parsing is all guesswork and guesswork wants tests, which is easiest
//! when there is nothing to set up first.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Cleaners
// ---------------------------------------------------------------------------

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
                    j.and_then(|j| chars.get(j)).is_some_and(|c| c.is_ascii_digit())
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
fn authored(embedded: &str, path: &str) -> bool {
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
fn trim_junk(s: &str) -> String {
    const JUNK: [char; 12] = ['-', '–', '.', ',', '(', ')', '[', ']', '{', '}', '_', ' '];
    s.trim().trim_matches(JUNK).trim().to_string()
}

// ---------------------------------------------------------------------------
// What a filename turned out to be
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Media {
    Episode {
        show: String,
        /// `None` for a release numbered straight through, as fansubs
        /// usually are: `Show - 01`.
        season: Option<u32>,
        episode: u32,
        /// A double episode: `S01E02-E03`.
        episode_end: Option<u32>,
        title: Option<String>,
    },
    Movie {
        title: String,
        year: Option<u32>,
    },
}

/// Work out what a path is a name for.
///
/// Never fails: anything that is not recognisably an episode is treated as a
/// film, and a film that is only a cleaned-up filename is still better than
/// the filename.
pub fn parse(path: &str) -> Media {
    let stem = strip_group(strip_extension(strip_path(path)));
    if let Some(episode) = parse_episode(stem) {
        return episode;
    }
    // Only once there is no marker. `S2 - 01` has the same dash and number as
    // `Show - 01`, and the season in front of it is the part worth having.
    if let Some(episode) = parse_absolute(stem) {
        return episode;
    }
    let (title, year) = split_year(&clean_separators(stem));
    Media::Movie {
        title: strip_metadata(&title),
        year,
    }
}

/// One line for a bar or a menu row, given what the container says about
/// itself.
///
/// A file name and an embedded title are two descriptions of one thing, and
/// they are not interchangeable: the name knows the show and the numbering,
/// the container usually knows only the episode's own title. Showing one and
/// then the other — which is what happens when the metadata arrives a moment
/// after the file opens — reads as the name changing under you.
///
/// So they are combined instead. The name provides the shape and the
/// container fills in the episode title, which is the one part someone
/// actually typed. What arrives late then adds to the line rather than
/// replacing it.
pub fn titled(path: &str, embedded: Option<&str>) -> String {
    match described(path, embedded) {
        (Some(show), rest) => format!("{show} · {rest}"),
        (None, rest) => rest,
    }
}

/// One entry, split into the show it belongs to and everything else.
///
/// Split rather than formatted because a list of episodes from one show says
/// that show's name on every row, and a panel 380px wide has better uses for
/// the space. Whoever is building the list decides whether the show is worth
/// repeating; see [`listing`].
pub fn described(path: &str, embedded: Option<&str>) -> (Option<String>, String) {
    let embedded = embedded
        .map(str::trim)
        .filter(|t| !t.is_empty())
        // Both the bar and the playlist rows arrive here, so a release name
        // in the title tag is refused once for both.
        .filter(|t| authored(t, path));
    match (parse(path), embedded) {
        (
            Media::Episode {
                show,
                season,
                episode,
                episode_end,
                title: from_name,
            },
            Some(title),
        ) => {
            let title = episode_title(title, episode);
            (
                Some(show),
                episode_line(
                    season,
                    episode,
                    episode_end,
                    title.as_deref().or(from_name.as_deref()),
                ),
            )
        }
        // A film's embedded title is the film's name, so it replaces rather
        // than adds to — but the year, which containers rarely carry, is
        // still worth keeping from the name.
        (Media::Movie { year, .. }, Some(title)) => (None, movie_line(title, year)),
        (media, None) => line(media),
    }
}

/// A list of entries, and what to call the list itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// The show they all belong to, if they all belong to one.
    pub heading: Option<String>,
    /// One label per entry, in order. The show is left out of these wherever
    /// `heading` carries it.
    pub rows: Vec<String>,
}

/// Label a whole list at once.
///
/// A season's worth of episodes writes the show's name down the left margin
/// of the panel and then runs out of room for the part that differs — which
/// is the part being read. So when every entry belongs to the same show, the
/// show becomes the heading and the rows keep only what distinguishes them.
///
/// Anything else — a film among the episodes, two different shows, a folder
/// of unparseable names — and each row says what it is on its own.
pub fn listing<'a>(entries: impl Iterator<Item = (&'a str, Option<&'a str>)>) -> Listing {
    let described: Vec<(Option<String>, String)> =
        entries.map(|(path, embedded)| described(path, embedded)).collect();

    let heading = common_show(&described);
    let rows = described
        .into_iter()
        .map(|(show, rest)| match (&heading, show) {
            // The heading is saying it, so the row need not.
            (Some(_), Some(_)) => rest,
            (_, Some(show)) => format!("{show} · {rest}"),
            (_, None) => rest,
        })
        .collect();
    Listing { heading, rows }
}

/// The one show every entry belongs to, if there is one.
fn common_show(described: &[(Option<String>, String)]) -> Option<String> {
    let mut entries = described.iter();
    let first = entries.next()?.0.clone()?;
    // Case-insensitively: one release naming it `Frieren` and the next
    // `frieren` is still one show, and the first spelling is as good as any.
    entries
        .all(|(show, _)| show.as_ref().is_some_and(|s| s.eq_ignore_ascii_case(&first)))
        .then_some(first)
}

fn line(media: Media) -> (Option<String>, String) {
    match media {
        Media::Episode {
            show,
            season,
            episode,
            episode_end,
            title,
        } => (
            Some(show),
            episode_line(season, episode, episode_end, title.as_deref()),
        ),
        Media::Movie { title, year } => (None, movie_line(&title, year)),
    }
}

/// A container's own title, with any numbering it repeats taken off the
/// front.
///
/// Containers are routinely tagged `S02E03-Somewhere She'd Like`, or just
/// `03 - Somewhere She'd Like`. The line already says which episode this is,
/// so leaving that in produces `S02E03 · S02E03-Somewhere She'd Like`.
///
/// Only a *leading* marker, and a bare number only when it is the episode we
/// already know about — a title is allowed to be about a number, and nothing
/// here should turn `2001 - A Space Odyssey` into `A Space Odyssey`.
/// Deliberately no metadata stripping either: this is a line someone typed,
/// not a file name, and a word like `Extended` in it is part of the title.
fn episode_title(embedded: &str, episode: u32) -> Option<String> {
    let chars: Vec<char> = embedded.chars().collect();
    let cut = match match_marker(&chars, 0) {
        Some((_, _, _, end)) => Some(end),
        // A bare number, and only this episode's.
        None => match take_number(&chars, 0, 3) {
            Some((n, end))
                if n == episode
                    && chars
                        .get(end)
                        .is_some_and(|c| !c.is_alphanumeric()) =>
            {
                Some(end)
            }
            _ => None,
        },
    };
    let rest: String = match cut {
        Some(end) => chars[end..].iter().collect(),
        None => embedded.to_string(),
    };
    let rest = trim_junk(&rest);
    (!rest.is_empty()).then_some(rest)
}

fn episode_line(
    season: Option<u32>,
    episode: u32,
    episode_end: Option<u32>,
    title: Option<&str>,
) -> String {
    // `E01` on its own where there is no season, in the same vocabulary as
    // `S02E01` — a number that reads as an episode rather than as a count.
    let season = season.map(|s| format!("S{s:02}")).unwrap_or_default();
    let number = match episode_end {
        Some(end) => format!("{season}E{episode:02}-E{end:02}"),
        None => format!("{season}E{episode:02}"),
    };
    match title {
        Some(t) => format!("{number} · {t}"),
        None => number,
    }
}

/// An episode's line with its season taken off the front, for a list that
/// already says which season it is: `S08E03 · The Long Night` under SEASON 8
/// is `E03 · The Long Night`. Only this season's own marker, written as
/// [`episode_line`] writes it; any other line comes back as it was.
pub fn without_season(line: &str, season: u32) -> String {
    match line.strip_prefix(&format!("S{season:02}")) {
        Some(rest) if rest.starts_with('E') => rest.to_string(),
        _ => line.to_string(),
    }
}

fn movie_line(title: &str, year: Option<u32>) -> String {
    // Not twice: a container title of "Blade Runner 2049" plus a year read
    // off the name is one film, not a film and a date.
    match year.filter(|y| !title.contains(&y.to_string())) {
        Some(y) => format!("{title} ({y})"),
        None => title.to_string(),
    }
}

/// Find `S01E02`, `s1e2`, or `1x02`, and split the name around it.
fn parse_episode(stem: &str) -> Option<Media> {
    let chars: Vec<char> = stem.chars().collect();
    for i in 0..chars.len() {
        let Some((season, episode, episode_end, end)) = match_marker(&chars, i) else {
            continue;
        };
        let show = trim_junk(&clean_separators(&chars[..i].iter().collect::<String>()));
        // A marker at the very start leaves no show name, which means this
        // was a bare `S01E02 - Title` — common enough inside a show folder.
        let rest: String = chars[end..].iter().collect();
        let title = strip_metadata(&trim_junk(&clean_separators(&rest)));
        return Some(Media::Episode {
            show,
            season: Some(season),
            episode,
            episode_end,
            title: (!title.is_empty()).then_some(title),
        });
    }
    None
}

/// `Show - 01`: an episode numbered without a season.
///
/// The fansub convention, and one the season markers miss entirely. Without
/// it a folder of `[Group] Show - 01.mkv` was a folder of films called
/// `Show - 01` — and once each file's own title was read ahead of time the
/// numbers went too, leaving episode titles in no visible order under a
/// heading that said PLAYLIST.
///
/// Strict about what counts, because a dash and a number turn up in names
/// that are not episodes. The dash has to stand between gaps, so
/// `Spider-Man 2` is safe. The number has to be a whole token, so `1080p` is
/// not episode 108; and it must be neither a plausible year nor a bare
/// resolution, so `Some Film - 2019` stays a film.
fn parse_absolute(stem: &str) -> Option<Media> {
    let chars: Vec<char> = stem.chars().collect();
    let gap = |c: Option<&char>| c.is_some_and(|c| matches!(c, ' ' | '_'));
    for i in 1..chars.len() {
        if chars[i] != '-' || !gap(chars.get(i - 1)) || !gap(chars.get(i + 1)) {
            continue;
        }
        let start = skip_gap(&chars, i + 1);
        let Some((episode, mut end)) = take_number(&chars, start, 4) else {
            continue;
        };
        let digits = end - start;
        // A revision, `03v2`, is still episode 3.
        if chars.get(end).is_some_and(|c| c.eq_ignore_ascii_case(&'v'))
            && chars.get(end + 1).is_some_and(|c| c.is_ascii_digit())
        {
            end += 1;
            while chars.get(end).is_some_and(|c| c.is_ascii_digit()) {
                end += 1;
            }
        }
        if chars.get(end).is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        if digits == 4 && (1900..=2099).contains(&episode) {
            continue;
        }
        if matches!(episode, 480 | 576 | 720 | 1080 | 1440 | 2160 | 4320) {
            continue;
        }
        let show = trim_junk(&clean_separators(&chars[..i].iter().collect::<String>()));
        if show.is_empty() {
            continue;
        }
        // Bracketed groups after the number belong to the release, not the
        // episode. `- 01 [ABCD1234]` is a checksum, and a checksum is not
        // release vocabulary `strip_metadata` would know to cut.
        let rest = without_brackets(&chars[end..].iter().collect::<String>());
        let title = strip_metadata(&trim_junk(&clean_separators(&rest)));
        return Some(Media::Episode {
            show,
            season: None,
            episode,
            episode_end: None,
            title: (!title.is_empty()).then_some(title),
        });
    }
    None
}

fn skip_gap(chars: &[char], i: usize) -> usize {
    let mut j = i;
    while chars.get(j).is_some_and(|c| matches!(c, ' ' | '_')) {
        j += 1;
    }
    j
}

/// A string with every `[...]` group taken out.
fn without_brackets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The marker at `i`, and where it ends.
///
/// Handles `S01E02`, `S01E02E03`, `S01E02-E03`, `1x02` — and `S2 - 01`, which
/// carries no `E` at all and is how most of anime is named.
fn match_marker(chars: &[char], i: usize) -> Option<(u32, u32, Option<u32>, usize)> {
    // A marker has to start a word; otherwise `Class01e02` would match.
    if i > 0 && chars[i - 1].is_alphanumeric() {
        return None;
    }
    let mut j = i;
    let season;
    if chars.get(j).is_some_and(|c| c.eq_ignore_ascii_case(&'s')) {
        j += 1;
        let (value, next) = take_number(chars, j, 2)?;
        season = value;
        j = next;
        // `S01 E02` and `S01.E02` both occur.
        let before = j;
        j = skip_separators(chars, j);
        let separated = j > before;
        if chars.get(j).is_some_and(|c| c.eq_ignore_ascii_case(&'e')) {
            j += 1;
        } else if !separated {
            // `S01` with the episode run straight on would be `S0102`, which
            // nobody writes and everybody would misread.
            return None;
        }
    } else {
        let (value, next) = take_number(chars, i, 2)?;
        season = value;
        j = next;
        if !chars.get(j).is_some_and(|c| c.eq_ignore_ascii_case(&'x')) {
            return None;
        }
        j += 1;
    }
    let (episode, next) = take_number(chars, j, 3)?;
    // The number has to be the whole token. Without this `Show S2 1080p`
    // reads `108` as an episode and throws the rest away.
    if chars.get(next).is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    j = next;

    // A second episode number, with or without its own `E`.
    let mut episode_end = None;
    let mut k = skip_separators(chars, j);
    if chars.get(k).is_some_and(|c| c.eq_ignore_ascii_case(&'e')) {
        if let Some((end, next)) = take_number(chars, k + 1, 3) {
            episode_end = Some(end);
            k = next;
            j = k;
        }
    }
    Some((season, episode, episode_end, j))
}

/// Up to `max` digits from `i`, and where they end.
fn take_number(chars: &[char], i: usize, max: usize) -> Option<(u32, usize)> {
    let mut j = i;
    let mut value: u32 = 0;
    while j < chars.len() && j - i < max && chars[j].is_ascii_digit() {
        value = value * 10 + chars[j].to_digit(10)?;
        j += 1;
    }
    (j > i).then_some((value, j))
}

fn skip_separators(chars: &[char], i: usize) -> usize {
    let mut j = i;
    while chars.get(j).is_some_and(|c| matches!(c, ' ' | '.' | '_' | '-')) {
        j += 1;
    }
    j
}

/// A four-digit year in parentheses or standing alone, and the name without
/// it. Only plausible years: `1080` is a resolution and `2160` is a bigger
/// one, and both turn up in the same names.
fn split_year(name: &str) -> (String, Option<u32>) {
    let words: Vec<&str> = name.split(' ').collect();
    for (i, w) in words.iter().enumerate().rev() {
        let bare = w.trim_matches(|c: char| !c.is_ascii_digit());
        if bare.len() != 4 {
            continue;
        }
        let Ok(year) = bare.parse::<u32>() else {
            continue;
        };
        if !(1900..=2099).contains(&year) {
            continue;
        }
        // A year at the very front is part of the title, as in `1917`.
        if i == 0 {
            continue;
        }
        return (words[..i].join(" "), Some(year));
    }
    (name.to_string(), None)
}

// ---------------------------------------------------------------------------
// Track names
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Languages
// ---------------------------------------------------------------------------

/// Endonyms: a language written the way it writes itself, which is what
/// someone looking for their own language is scanning the list for.
///
/// Keyed by ISO 639-1. Not every language — the long tail falls back to the
/// raw code, and the point where that starts to matter is the point to bring
/// in a real CLDR table rather than to keep extending this one.
const LANGUAGES: &[(&str, &str)] = &[
    ("ar", "العربية"),
    ("bg", "Български"),
    ("bn", "বাংলা"),
    ("cs", "Čeština"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("el", "Ελληνικά"),
    ("en", "English"),
    ("es", "Español"),
    ("et", "Eesti"),
    ("fa", "فارسی"),
    ("fi", "Suomi"),
    ("fr", "Français"),
    ("he", "עברית"),
    ("hi", "हिन्दी"),
    ("hr", "Hrvatski"),
    ("hu", "Magyar"),
    ("id", "Indonesia"),
    ("is", "Íslenska"),
    ("it", "Italiano"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("lt", "Lietuvių"),
    ("lv", "Latviešu"),
    ("ms", "Melayu"),
    ("nb", "Norsk bokmål"),
    ("nl", "Nederlands"),
    ("nn", "Norsk nynorsk"),
    ("no", "Norsk"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("ro", "Română"),
    ("ru", "Русский"),
    ("sk", "Slovenčina"),
    ("sl", "Slovenščina"),
    ("sr", "Српски"),
    ("sv", "Svenska"),
    ("th", "ไทย"),
    ("tr", "Türkçe"),
    ("uk", "Українська"),
    ("vi", "Tiếng Việt"),
    ("zh", "中文"),
];

/// Three-letter codes onto their two-letter equivalents. Both the
/// bibliographic and terminological forms, since files carry either.
const ALPHA3: &[(&str, &str)] = &[
    ("ara", "ar"), ("bul", "bg"), ("ben", "bn"), ("cze", "cs"), ("ces", "cs"),
    ("dan", "da"), ("ger", "de"), ("deu", "de"), ("gre", "el"), ("ell", "el"),
    ("eng", "en"), ("spa", "es"), ("est", "et"), ("per", "fa"), ("fas", "fa"),
    ("fin", "fi"), ("fre", "fr"), ("fra", "fr"), ("heb", "he"), ("hin", "hi"),
    ("hrv", "hr"), ("hun", "hu"), ("ind", "id"), ("ice", "is"), ("isl", "is"),
    ("ita", "it"), ("jpn", "ja"), ("kor", "ko"), ("lit", "lt"), ("lav", "lv"),
    ("may", "ms"), ("msa", "ms"), ("nob", "nb"), ("dut", "nl"), ("nld", "nl"),
    ("nno", "nn"), ("nor", "no"), ("pol", "pl"), ("por", "pt"), ("rum", "ro"),
    ("ron", "ro"), ("rus", "ru"), ("slo", "sk"), ("slk", "sk"), ("slv", "sl"),
    ("srp", "sr"), ("swe", "sv"), ("tha", "th"), ("tur", "tr"), ("ukr", "uk"),
    ("vie", "vi"), ("chi", "zh"), ("zho", "zh"),
];

/// The two-letter code a tag reduces to: `en-US` and `eng` both give `en`.
///
/// Whoever muxed the file picked from `en`, `eng` and `en-US` more or less at
/// random, so nothing that compares languages can compare the raw strings.
pub fn base_code(tag: &str) -> Option<&'static str> {
    let tag = tag.trim().to_ascii_lowercase();
    let head = tag.split(['-', '_']).next().unwrap_or(&tag);
    if let Some((code, _)) = LANGUAGES.iter().find(|(c, _)| *c == head) {
        return Some(code);
    }
    ALPHA3
        .iter()
        .find(|(three, _)| *three == head)
        .map(|(_, two)| *two)
}

/// How to write a language code for a reader. Unknown codes come back
/// upper-cased, which at least reads as a code rather than as a word.
pub fn language_name(tag: &str) -> String {
    match base_code(tag).and_then(|c| LANGUAGES.iter().find(|(code, _)| *code == c)) {
        Some((_, name)) => (*name).to_string(),
        None => tag.trim().to_ascii_uppercase(),
    }
}

/// Do two tags name the same language?
pub fn same_language(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    match (base_code(a), base_code(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Track labels
// ---------------------------------------------------------------------------

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

/// Whether a chapter's title marks closing credits.
///
/// Chapter names are structural rather than written — `End Credits`,
/// `Outro`, `ED` — so whole words are matched, and a title about the start
/// (`Opening Credits`, `Intro`, `OP`) is refused whatever else it says.
pub fn is_credits(title: &str) -> bool {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let has = |word: &str| words.iter().any(|w| w == word);
    if ["opening", "intro", "op", "recap", "previously", "cold"]
        .iter()
        .any(|w| has(w))
    {
        return false;
    }
    ["credits", "credit", "endcredits", "outro", "ending", "ed", "closing"]
        .iter()
        .any(|w| has(w))
        || (has("end") && (words.len() == 1 || has("titles")))
}

/// Whether a chapter's title is only a placeholder — its own start time,
/// `00:42:22.832`, or `Chapter 7` — which is what a muxer writes when nobody
/// named anything.
pub fn is_unnamed_chapter(title: &str) -> bool {
    let title = title.trim();
    if let Some(rest) = title
        .strip_prefix("Chapter")
        .or_else(|| title.strip_prefix("chapter"))
    {
        let rest = rest.trim_start();
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
    }
    title.chars().any(|c| c.is_ascii_digit())
        && title.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.')
}

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

    fn episode(name: &str) -> (String, Option<u32>, u32, Option<u32>, Option<String>) {
        match parse(name) {
            Media::Episode {
                show,
                season,
                episode,
                episode_end,
                title,
            } => (show, season, episode, episode_end, title),
            other => panic!("expected an episode, got {other:?}"),
        }
    }

    #[test]
    fn dashed_episode() {
        let (show, s, e, end, title) = episode("Show Name - S01E02 - Episode Title.mkv");
        assert_eq!(show, "Show Name");
        assert_eq!((s, e, end), (Some(1), 2, None));
        assert_eq!(title.as_deref(), Some("Episode Title"));
    }

    #[test]
    fn dotted_episode() {
        let (show, s, e, _, title) = episode("Show.Name.S01E02.Episode.Title.1080p.WEB-DL.x264-GRP.mkv");
        assert_eq!(show, "Show Name");
        assert_eq!((s, e), (Some(1), 2));
        assert_eq!(title.as_deref(), Some("Episode Title"));
    }

    #[test]
    fn double_episode() {
        let (_, _, e, end, title) = episode("Show - S01E02-E03 - Two Parter.mkv");
        assert_eq!((e, end), (2, Some(3)));
        assert_eq!(title.as_deref(), Some("Two Parter"));
    }

    #[test]
    fn alternate_marker() {
        let (show, s, e, _, _) = episode("Show Name 1x02 Title.mkv");
        assert_eq!(show, "Show Name");
        assert_eq!((s, e), (Some(1), 2));
    }

    #[test]
    fn spaced_marker() {
        let (_, s, e, _, _) = episode("Show - S01 E02 - Title.mkv");
        assert_eq!((s, e), (Some(1), 2));
    }

    #[test]
    fn a_marker_must_start_a_word() {
        // Otherwise "Class" would be read as a season marker.
        assert!(matches!(parse("Class1x02 thing.mkv"), Media::Movie { .. }));
    }

    #[test]
    fn anime_season_and_episode_without_an_e() {
        let (show, s, e, end, title) = episode(r"C:\Users\k\Videos\[EMBER] Sousou no Frieren S2 - 01.mkv");
        assert_eq!(show, "Sousou no Frieren");
        assert_eq!((s, e, end), (Some(2), 1, None));
        assert_eq!(title, None);
    }

    #[test]
    fn fansub_numbering_without_a_season() {
        let (show, s, e, end, title) =
            episode("[SubsPlease] Wandering Road - 01 (1080p) [ABCD1234].mkv");
        assert_eq!(show, "Wandering Road");
        assert_eq!((s, e, end), (None, 1, None));
        assert_eq!(title, None);
    }

    #[test]
    fn a_checksum_alone_is_not_a_title() {
        let (_, _, e, _, title) = episode("[Grp] Wandering Road - 04 [ABCD1234].mkv");
        assert_eq!(e, 4);
        assert_eq!(title, None);
    }

    #[test]
    fn a_title_after_the_number_is_kept() {
        let (show, _, e, _, title) = episode("Wandering Road - 03 - Old Friends.mkv");
        assert_eq!((show.as_str(), e), ("Wandering Road", 3));
        assert_eq!(title.as_deref(), Some("Old Friends"));
    }

    #[test]
    fn a_revision_is_the_same_episode() {
        let (_, _, e, _, title) = episode("[Grp] Wandering Road - 03v2 [720p].mkv");
        assert_eq!(e, 3);
        assert_eq!(title, None);
    }

    #[test]
    fn a_year_after_a_dash_is_a_film() {
        assert!(matches!(parse("Some Film - 2019.mkv"), Media::Movie { .. }));
    }

    #[test]
    fn a_resolution_after_a_dash_is_not_an_episode() {
        assert!(matches!(parse("Some Show - 1080p.mkv"), Media::Movie { .. }));
        assert!(matches!(parse("Some Show - 1080 WEB.mkv"), Media::Movie { .. }));
    }

    #[test]
    fn a_hyphen_inside_a_name_is_not_a_dash() {
        assert!(matches!(parse("Spider-Man 2.mkv"), Media::Movie { .. }));
    }

    #[test]
    fn a_marked_season_wins_over_the_dash() {
        // `S2 - 01` has the same dash and number, and the season is worth having.
        let (_, s, e, _, _) = episode("[EMBER] Sousou no Frieren S2 - 01.mkv");
        assert_eq!((s, e), (Some(2), 1));
    }

    #[test]
    fn an_episode_without_a_season_is_numbered_on_its_own() {
        assert_eq!(
            titled("[Grp] Wandering Road - 01.mkv", Some("A Journey Begins")),
            "Wandering Road \u{b7} E01 \u{b7} A Journey Begins"
        );
        // A tag that repeats the bare number says it once, as with a season.
        assert_eq!(
            titled("[Grp] Wandering Road - 01.mkv", Some("01 - A Journey Begins")),
            "Wandering Road \u{b7} E01 \u{b7} A Journey Begins"
        );
    }

    #[test]
    fn a_fansub_folder_becomes_one_show() {
        let list = listing(
            [
                ("[Grp] Wandering Road - 01.mkv", Some("A Journey Begins")),
                ("[Grp] Wandering Road - 02.mkv", None),
            ]
            .into_iter(),
        );
        assert_eq!(list.heading.as_deref(), Some("Wandering Road"));
        assert_eq!(list.rows, ["E01 \u{b7} A Journey Begins", "E02"]);
    }

    #[test]
    fn a_release_group_is_not_the_show() {
        assert_eq!(strip_group("[EMBER] Sousou no Frieren"), "Sousou no Frieren");
        assert_eq!(strip_group("(Group) Show"), "Show");
        assert_eq!(strip_group("Show [Group]"), "Show [Group]");
        // An unclosed bracket is not a tag; leave the name alone.
        assert_eq!(strip_group("[weird name"), "[weird name");
    }

    #[test]
    fn a_resolution_is_not_an_episode() {
        // `S2 1080p` must not read 108 as the episode number.
        match parse("Show S2 1080p WEB-DL.mkv") {
            Media::Movie { title, .. } => assert_eq!(title, "Show S2"),
            other => panic!("expected a movie, got {other:?}"),
        }
    }

    #[test]
    fn the_container_title_fills_the_episode_slot() {
        // The name knows the show and the numbering; the container knows the
        // episode's own title. Combined, not swapped — otherwise the line
        // changes shape the moment the metadata arrives.
        let path = "[EMBER] Sousou no Frieren S2 - 01.mkv";
        assert_eq!(titled(path, None), "Sousou no Frieren \u{b7} S02E01");
        assert_eq!(
            titled(path, Some("The Chosen One")),
            "Sousou no Frieren \u{b7} S02E01 \u{b7} The Chosen One"
        );
    }

    #[test]
    fn a_container_that_repeats_the_numbering_says_it_once() {
        let path = "[EMBER] Sousou no Frieren S2 - 03.mkv";
        assert_eq!(
            titled(path, Some("S02E03-Somewhere She'd Like")),
            "Sousou no Frieren \u{b7} S02E03 \u{b7} Somewhere She'd Like"
        );
        // The bare-number form of the same habit.
        assert_eq!(
            titled(path, Some("03 - Somewhere She'd Like")),
            "Sousou no Frieren \u{b7} S02E03 \u{b7} Somewhere She'd Like"
        );
    }

    #[test]
    fn a_title_may_be_about_a_number() {
        // 2001 is not episode 3, so it stays where it is.
        let path = "[EMBER] Sousou no Frieren S2 - 03.mkv";
        assert_eq!(
            titled(path, Some("2001 - A Space Odyssey")),
            "Sousou no Frieren \u{b7} S02E03 \u{b7} 2001 - A Space Odyssey"
        );
    }

    #[test]
    fn a_container_title_is_not_a_file_name() {
        // No metadata stripping: someone typed this one.
        let path = "Show.S01E01.mkv";
        assert_eq!(
            titled(path, Some("The Extended Cut")),
            "Show \u{b7} S01E01 \u{b7} The Extended Cut"
        );
    }

    #[test]
    fn an_advertisement_in_the_title_tag_is_not_a_title() {
        // Verbatim from a real file. The tag is the release name with a site
        // name bolted on the front, so it is not equal to the file name and
        // the old exact-match test let it through whole — advertisement,
        // resolution, codec and release group — into the slot meant for the
        // episode's own title.
        let path = r"C:\Users\me\Game.of.Thrones.S07E01.Dragonstone.1080p.10bit.BluRay.6CH.x265.HEVC-PSA.mkv";
        let tag = concat!(
            "PSArips.com | Game.of.Thrones.S07E01.Dragonstone",
            ".1080p.10bit.BluRay.6CH.x265.HEVC-PSA"
        );
        assert_eq!(
            titled(path, Some(tag)),
            "Game of Thrones \u{b7} S07E01 \u{b7} Dragonstone"
        );
    }

    #[test]
    fn a_title_tag_that_swallowed_the_file_name_is_refused() {
        // The same shape with no technical marker to give it away, so the one
        // thing that catches it is that the tag contains the file's own stem.
        // A renamed download looks like this.
        let path = "Show.S01E02.The.Reckoning.mkv";
        assert_eq!(
            titled(path, Some("some-tracker.org | Show.S01E02.The.Reckoning")),
            "Show \u{b7} S01E02 \u{b7} The Reckoning"
        );
    }

    #[test]
    fn a_container_title_of_only_numbering_adds_nothing() {
        let path = "Show.S01E01.mkv";
        assert_eq!(titled(path, Some("S01E01")), "Show \u{b7} S01E01");
    }

    #[test]
    fn a_container_title_replaces_a_film_name_but_keeps_the_year() {
        assert_eq!(
            titled("some.ripped.name.2019.1080p.mkv", Some("A Proper Title")),
            "A Proper Title (2019)"
        );
        // And does not say the year twice.
        assert_eq!(
            titled("blade.runner.2049.2017.1080p.mkv", Some("Blade Runner 2049")),
            "Blade Runner 2049 (2017)"
        );
    }

    #[test]
    fn one_show_becomes_the_heading() {
        let list = listing(
            [
                ("[EMBER] Sousou no Frieren S2 - 01.mkv", Some("The Chosen One")),
                ("[EMBER] Sousou no Frieren S2 - 02.mkv", None),
            ]
            .into_iter(),
        );
        assert_eq!(list.heading.as_deref(), Some("Sousou no Frieren"));
        assert_eq!(
            list.rows,
            ["S02E01 \u{b7} The Chosen One", "S02E02"]
        );
    }

    #[test]
    fn a_mixed_list_keeps_the_show_on_every_row() {
        let list = listing(
            [
                ("Show.A.S01E01.mkv", None),
                ("Show.B.S01E01.mkv", None),
            ]
            .into_iter(),
        );
        assert_eq!(list.heading, None);
        assert_eq!(
            list.rows,
            ["Show A \u{b7} S01E01", "Show B \u{b7} S01E01"]
        );
    }

    #[test]
    fn a_film_among_the_episodes_is_enough_to_stop_it() {
        let list = listing(
            [
                ("Show.A.S01E01.mkv", None),
                ("Some.Movie.2019.mkv", None),
            ]
            .into_iter(),
        );
        assert_eq!(list.heading, None);
        assert_eq!(list.rows, ["Show A \u{b7} S01E01", "Some Movie (2019)"]);
    }

    #[test]
    fn an_empty_list_has_no_show() {
        let list = listing([].into_iter());
        assert_eq!(list.heading, None);
        assert!(list.rows.is_empty());
    }

    #[test]
    fn movie_with_year() {
        match parse("Some.Movie.2019.1080p.BluRay.x264.mkv") {
            Media::Movie { title, year } => {
                assert_eq!(title, "Some Movie");
                assert_eq!(year, Some(2019));
            }
            other => panic!("expected a movie, got {other:?}"),
        }
    }

    #[test]
    fn resolution_is_not_a_year() {
        match parse("Some Movie 2160p.mkv") {
            Media::Movie { title, year } => {
                assert_eq!(title, "Some Movie");
                assert_eq!(year, None);
            }
            other => panic!("expected a movie, got {other:?}"),
        }
    }

    #[test]
    fn a_leading_year_is_the_title() {
        match parse("1917.2019.1080p.mkv") {
            Media::Movie { title, year } => {
                assert_eq!(title, "1917");
                assert_eq!(year, Some(2019));
            }
            other => panic!("expected a movie, got {other:?}"),
        }
    }

    #[test]
    fn plain_name_survives() {
        match parse("holiday clip.mp4") {
            Media::Movie { title, year } => {
                assert_eq!(title, "holiday clip");
                assert_eq!(year, None);
            }
            other => panic!("expected a movie, got {other:?}"),
        }
    }

    #[test]
    fn display_lines() {
        assert_eq!(
            titled("Show.Name.S01E02.The.Title.1080p.mkv", None),
            "Show Name · S01E02 · The Title"
        );
        assert_eq!(titled("Some.Movie.2019.1080p.mkv", None), "Some Movie (2019)");
    }

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
    fn credits_chapters_are_recognised() {
        for title in [
            "End Credits",
            "Credits",
            "Closing Credits",
            "Outro",
            "ED",
            "Ending",
            "Ending Theme",
            "End",
            "End Titles",
            "credits_roll",
        ] {
            assert!(is_credits(title), "{title}");
        }
        for title in [
            "Opening Credits",
            "Intro",
            "OP",
            "Chapter 12",
            "Edward's Return",
            "Previously On",
            "The End of the Beginning",
            "Preview",
        ] {
            assert!(!is_credits(title), "{title}");
        }
    }

    #[test]
    fn placeholder_chapter_titles_are_unnamed() {
        for title in ["00:42:22.832", "00:00:00.000", "Chapter 07", "chapter 3", "01"] {
            assert!(is_unnamed_chapter(title), "{title}");
        }
        for title in ["End Credits", "Chapter One", "Chapter", "Act 3", "", "2001: A Space Odyssey"] {
            assert!(!is_unnamed_chapter(title), "{title}");
        }
    }

    #[test]
    fn paths_are_stripped() {
        assert_eq!(strip_path(r"C:\videos\show.mkv"), "show.mkv");
        assert_eq!(strip_path("/videos/show.mkv"), "show.mkv");
    }

    #[test]
    fn language_codes_fold_together() {
        assert!(same_language("eng", "en-US"));
        assert!(same_language("SWE", "sv"));
        assert!(!same_language("en", "sv"));
        assert_eq!(language_name("swe"), "Svenska");
        assert_eq!(language_name("ja"), "日本語");
        assert_eq!(language_name("qqq"), "QQQ");
    }

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

    fn track(id: i64, title: Option<&'static str>, lang: Option<&'static str>) -> TrackInfo<'static> {
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

