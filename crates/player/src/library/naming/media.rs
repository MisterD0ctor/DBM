//! What a filename turned out to be, and the line it is shown as.

use super::clean::{
    authored, clean_separators, strip_extension, strip_group, strip_metadata, strip_path, trim_junk,
};

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
    let described: Vec<(Option<String>, String)> = entries
        .map(|(path, embedded)| described(path, embedded))
        .collect();

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
        .all(|(show, _)| {
            show.as_ref()
                .is_some_and(|s| s.eq_ignore_ascii_case(&first))
        })
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
                if n == episode && chars.get(end).is_some_and(|c| !c.is_alphanumeric()) =>
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
    while chars
        .get(j)
        .is_some_and(|c| matches!(c, ' ' | '.' | '_' | '-'))
    {
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
        let (show, s, e, _, title) =
            episode("Show.Name.S01E02.Episode.Title.1080p.WEB-DL.x264-GRP.mkv");
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
        let (show, s, e, end, title) =
            episode(r"C:\Users\k\Videos\[EMBER] Sousou no Frieren S2 - 01.mkv");
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
        assert!(matches!(
            parse("Some Show - 1080p.mkv"),
            Media::Movie { .. }
        ));
        assert!(matches!(
            parse("Some Show - 1080 WEB.mkv"),
            Media::Movie { .. }
        ));
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
            titled(
                "[Grp] Wandering Road - 01.mkv",
                Some("01 - A Journey Begins")
            ),
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
        assert_eq!(
            strip_group("[EMBER] Sousou no Frieren"),
            "Sousou no Frieren"
        );
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
            titled(
                "blade.runner.2049.2017.1080p.mkv",
                Some("Blade Runner 2049")
            ),
            "Blade Runner 2049 (2017)"
        );
    }
    #[test]
    fn one_show_becomes_the_heading() {
        let list = listing(
            [
                (
                    "[EMBER] Sousou no Frieren S2 - 01.mkv",
                    Some("The Chosen One"),
                ),
                ("[EMBER] Sousou no Frieren S2 - 02.mkv", None),
            ]
            .into_iter(),
        );
        assert_eq!(list.heading.as_deref(), Some("Sousou no Frieren"));
        assert_eq!(list.rows, ["S02E01 \u{b7} The Chosen One", "S02E02"]);
    }
    #[test]
    fn a_mixed_list_keeps_the_show_on_every_row() {
        let list = listing([("Show.A.S01E01.mkv", None), ("Show.B.S01E01.mkv", None)].into_iter());
        assert_eq!(list.heading, None);
        assert_eq!(list.rows, ["Show A \u{b7} S01E01", "Show B \u{b7} S01E01"]);
    }
    #[test]
    fn a_film_among_the_episodes_is_enough_to_stop_it() {
        let list =
            listing([("Show.A.S01E01.mkv", None), ("Some.Movie.2019.mkv", None)].into_iter());
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
        assert_eq!(
            titled("Some.Movie.2019.1080p.mkv", None),
            "Some Movie (2019)"
        );
    }
}
