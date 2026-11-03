//! Files numbered one after another, which only their folder shows to be a
//! show's.
//!
//! `Fate Zero 01 The Summoning of Heroes` names an episode the way
//! `Toy Story 3` names a film, so a name alone can't say which it is. The
//! folder around it can, so each folder is judged whole when it is listed,
//! and [`parse`](crate::parse) looks up the verdict for a name that doesn't
//! say on its own.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{PoisonError, RwLock};

use super::clean::{
    clean_separators, strip_extension, strip_group, strip_metadata, strip_path, trim_junk,
};
use super::media::{Media, named_episode, split_year, without_brackets, year_of};

/// Fewer files than this could be a film and its sequels.
const FEWEST: usize = 4;

/// The last word of a film's name where the film is split across files:
/// `Film Part 2`, `Film CD 2`.
const PIECE: &[&str] = &["part", "pt", "vol", "volume", "disc", "disk", "cd"];

/// The runs in each folder listed so far.
static FOLDERS: RwLock<BTreeMap<String, Vec<Run>>> = RwLock::new(BTreeMap::new());

/// One show's numbered files in a folder.
#[derive(Debug, Clone, PartialEq)]
struct Run {
    /// The words in front of the number, in lower case: none for files that
    /// are only numbered, as in `01 - Pilot`.
    key: Vec<String>,
    show: String,
    season: Option<u32>,
}

/// Judge each folder these paths are in.
///
/// The paths given for a folder have to be all of its videos, since a run is
/// judged by what else is there. A folder given again is judged again.
pub fn notice<S: AsRef<str>>(paths: &[S]) {
    let mut folders: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in paths {
        if let Some((folder, name)) = split(path.as_ref()) {
            folders.entry(folder).or_default().push(name);
        }
    }
    let judged: Vec<(&str, Vec<Run>)> = folders
        .into_iter()
        .map(|(folder, names)| (folder, runs_in(folder, &names)))
        .collect();
    let mut known = FOLDERS.write().unwrap_or_else(PoisonError::into_inner);
    for (folder, runs) in judged {
        if runs.is_empty() {
            known.remove(folder);
        } else {
            known.insert(folder.to_owned(), runs);
        }
    }
}

/// The episode the file at `path`, whose name is `stem`, is in its folder's
/// runs.
pub(super) fn episode(path: &str, stem: &str) -> Option<Media> {
    let (folder, _) = split(path)?;
    let known = FOLDERS.read().unwrap_or_else(PoisonError::into_inner);
    let words = words(stem);
    known
        .get(folder)?
        .iter()
        .find_map(|run| run.episode(&words))
}

impl Run {
    /// The episode `words` name, where they are this run's key and a number.
    fn episode(&self, words: &[String]) -> Option<Media> {
        let at = self.key.len();
        let key = words.get(..at)?;
        if !key
            .iter()
            .zip(&self.key)
            .all(|(w, k)| w.to_lowercase() == *k)
        {
            return None;
        }
        let episode = number_at(words, at)?;
        let rest = clean_separators(&without_brackets(&words[at + 1..].join(" ")));
        let title = strip_metadata(&trim_junk(&rest));
        Some(Media::Episode {
            show: self.show.clone(),
            season: self.season,
            episode,
            episode_end: None,
            title: (!title.is_empty()).then_some(title),
        })
    }
}

/// The runs among `names`, the videos in `folder`, longest key first so a
/// name is read as the run that says the most of it.
fn runs_in(folder: &str, names: &[&str]) -> Vec<Run> {
    let stems: Vec<&str> = names
        .iter()
        .map(|name| strip_group(strip_extension(name)))
        .collect();
    // Each key's numbers, and the first file's spelling of the key.
    let mut keys: BTreeMap<Vec<String>, (String, BTreeSet<u32>)> = BTreeMap::new();
    for stem in stems.iter().filter(|stem| named_episode(stem).is_none()) {
        let words = words(stem);
        for at in 0..words.len() {
            let Some(number) = number_at(&words, at) else {
                continue;
            };
            let key = words[..at].iter().map(|word| word.to_lowercase()).collect();
            keys.entry(key)
                .or_insert_with(|| (trim_junk(&words[..at].join(" ")), BTreeSet::new()))
                .1
                .insert(number);
        }
    }
    // `Toy Story` beside `Toy Story 2` makes it a sequel.
    let films: BTreeSet<String> = stems
        .iter()
        .map(|stem| strip_metadata(&split_year(&clean_separators(stem)).0).to_lowercase())
        .collect();
    let sequels = |key: &[String]| !key.is_empty() && films.contains(&key.join(" "));
    let (named, season) = named_by(folder);
    let mut runs: Vec<Run> = keys
        .into_iter()
        .filter(|(key, (_, numbers))| a_season(key, numbers) && !in_pieces(key) && !sequels(key))
        .filter_map(|(key, (written, _))| {
            let show = if written.is_empty() {
                named.clone()?
            } else {
                written
            };
            Some(Run { key, show, season })
        })
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.key.len()));
    runs
}

/// Whether the numbers under `key` are a season's episodes: enough of them,
/// and for files that are only numbered, close together, where films that
/// start with a number (`12 Angry Men`, `300`) are far apart.
fn a_season(key: &[String], numbers: &BTreeSet<u32>) -> bool {
    let (Some(first), Some(last)) = (numbers.first(), numbers.last()) else {
        return false;
    };
    numbers.len() >= FEWEST
        && (!key.is_empty()
            || usize::try_from(last - first).is_ok_and(|gap| gap < 2 * numbers.len()))
}

fn in_pieces(key: &[String]) -> bool {
    key.last()
        .is_some_and(|word| PIECE.contains(&word.as_str()))
}

/// The episode number at `at`, where the word there is one and no year comes
/// after it: a year there is how a film is named, as in `Toy Story 3 (2010)`.
fn number_at(words: &[String], at: usize) -> Option<u32> {
    // `720` on its own is a resolution.
    let number = digits(words.get(at)?, 3).filter(|n| !matches!(n, 480 | 576 | 720))?;
    let year_after = words[at + 1..].iter().any(|word| year_of(word).is_some());
    (!year_after).then_some(number)
}

/// The show and season a folder's name says it holds: `Show - Season 2`. A
/// folder named for its season alone is inside the show's, which names the
/// show instead.
fn named_by(folder: &str) -> (Option<String>, Option<u32>) {
    let (show, season) = read_folder(strip_path(folder));
    let show = show.or_else(|| {
        let (above, _) = split(folder).filter(|_| season.is_some())?;
        read_folder(strip_path(above)).0
    });
    (show, season)
}

/// A folder's name, as a show's name and a season.
fn read_folder(name: &str) -> (Option<String>, Option<u32>) {
    let words = words(&strip_metadata(&clean_separators(strip_group(name))));
    let named = (0..words.len()).find_map(|at| Some((at, season_at(&words, at)?)));
    let (show, season) = match named {
        Some((at, season)) => (trim_junk(&words[..at].join(" ")), Some(season)),
        None => (trim_junk(&words.join(" ")), None),
    };
    ((!show.is_empty()).then_some(show), season)
}

/// The season named at `at`: `Season 2`, `Series 2` or `S02`.
fn season_at(words: &[String], at: usize) -> Option<u32> {
    let word = words[at].to_lowercase();
    if word == "season" || word == "series" {
        return digits(words.get(at + 1)?, 2);
    }
    digits(word.strip_prefix('s')?, 2)
}

/// A number written in at most `most` digits and nothing else.
fn digits(word: &str, most: usize) -> Option<u32> {
    let all = (1..=most).contains(&word.len()) && word.bytes().all(|b| b.is_ascii_digit());
    all.then(|| word.parse().ok()).flatten()
}

fn words(stem: &str) -> Vec<String> {
    clean_separators(stem)
        .split(' ')
        .map(str::to_owned)
        .collect()
}

/// A path's folder and the name in it, at either platform's separator.
fn split(path: &str) -> Option<(&str, &str)> {
    let at = path.rfind(['/', '\\'])?;
    Some((&path[..at], &path[at + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    /// The runs among `names`, as the shows and seasons they name.
    fn shows(folder: &str, names: &[&str]) -> Vec<(String, Option<u32>)> {
        runs_in(folder, names)
            .into_iter()
            .map(|run| (run.show, run.season))
            .collect()
    }

    /// `names` as files in `folder`, which is then listed.
    fn listed(folder: &str, names: &[&str]) -> Vec<String> {
        let paths: Vec<String> = names
            .iter()
            .map(|name| format!("{folder}/{name}"))
            .collect();
        notice(&paths);
        paths
    }

    const FATE: &str =
        "/Shows/[anime4life.] Fate Zero 1-25 Complete (BD1080p AC3 10bit) [x265_HEVC] Dual Audio";

    #[test]
    fn a_folder_of_numbered_files_is_a_show() {
        let paths = listed(
            FATE,
            &[
                "Fate Zero 01 The Summoning of Heroes (BD1080p AC3 10bit).mkv",
                "Fate Zero 02 The Fake First Shot (BD1080p AC3 10bit).mkv",
                "Fate Zero 03 Fuyuki City (BD1080p AC3 10bit).mkv",
                "Fate Zero 24 The Last Command Seal (BD1080p AC3 10bit).mkv",
                "Fate Zero 25 Fate Zero (BD1080p AC3 10bit).mkv",
                "Fate Zero 10 Rin's Big Adventure (BD1080p AC3 10bit).mkv",
                "Fate Zero 11 Discussing the Grail (BD1080p AC3 10bit).mkv",
            ],
        );
        assert_eq!(
            parse(&paths[0]),
            Media::Episode {
                show: "Fate Zero".into(),
                season: None,
                episode: 1,
                episode_end: None,
                title: Some("The Summoning of Heroes".into()),
            }
        );
        assert!(matches!(
            parse(&paths[4]),
            Media::Episode { episode: 25, title: Some(ref t), .. } if t == "Fate Zero"
        ));
        // The same file anywhere not listed has nothing to go on.
        let elsewhere = "/Unlisted/Fate Zero 01 The Summoning of Heroes (BD1080p AC3 10bit).mkv";
        assert!(matches!(parse(elsewhere), Media::Movie { .. }));
    }

    #[test]
    fn three_files_are_too_few() {
        assert!(shows("/a", &["Show 01.mkv", "Show 02.mkv", "Show 03.mkv"]).is_empty());
    }

    #[test]
    fn films_are_not_a_show() {
        // Sequels beside the film they follow.
        let toy_story = [
            "Toy Story.mkv",
            "Toy Story 2.mkv",
            "Toy Story 3.mkv",
            "Toy Story 4.mkv",
            "Toy Story 5.mkv",
        ];
        assert!(shows("/Films", &toy_story).is_empty());
        // Sequels with their years.
        let rocky = [
            "Rocky 2 (1979).mkv",
            "Rocky 3 (1982).mkv",
            "Rocky.4.1985.1080p.mkv",
            "Rocky 5 (1990).mkv",
        ];
        assert!(shows("/Films", &rocky).is_empty());
        // One film in pieces.
        let pieces = [
            "Film Part 1.mkv",
            "Film Part 2.mkv",
            "Film Part 3.mkv",
            "Film Part 4.mkv",
        ];
        assert!(shows("/Films", &pieces).is_empty());
        // Films that start with a number, which are far apart.
        let numbered = [
            "10 Cloverfield Lane.mkv",
            "12 Angry Men.mkv",
            "21 Jump Street.mkv",
            "22 Jump Street.mkv",
            "300.mkv",
        ];
        assert!(shows("/Films", &numbered).is_empty());
        let recordings = [
            "Skärminspelning från 2026-10-17 16-25-48.webm",
            "Skärminspelning från 2026-10-18 00-18-22.webm",
            "Skärminspelning från 2026-10-18 00-19-16.webm",
            "Skärminspelning från 2026-10-18 00-20-01.webm",
        ];
        assert!(shows("/Recordings", &recordings).is_empty());
    }

    #[test]
    fn a_number_in_the_show_s_name_is_not_the_episode() {
        let names = [
            "Mob Psycho 100 01.mkv",
            "Mob Psycho 100 02.mkv",
            "Mob Psycho 100 03.mkv",
            "Mob Psycho 100 04.mkv",
        ];
        assert_eq!(
            shows("/a", &names),
            vec![("Mob Psycho 100".to_string(), None)]
        );
    }

    #[test]
    fn files_only_numbered_take_their_folder_s_name() {
        let names = ["01 - Pilot.mkv", "02.mkv", "03 Three.mkv", "04.mkv"];
        let folder = "/Shows/Vinland Saga - Season 1 (2019)";
        assert_eq!(
            shows(folder, &names),
            vec![("Vinland Saga".to_string(), Some(1))]
        );
        let paths = listed(folder, &names);
        assert!(matches!(
            parse(&paths[0]),
            Media::Episode { episode: 1, title: Some(ref t), .. } if t == "Pilot"
        ));
        // A folder named for its season alone is in the show's.
        assert_eq!(
            shows("/Shows/[Grp] Show (2010)/Season 02", &names),
            vec![("Show (2010)".to_string(), Some(2))]
        );
        let release =
            "/Shows/The.Walking.Dead.SEASON.06.S06.COMPLETE.1080p.WEB-DL.6CH.x265.HEVC-PSA";
        assert_eq!(
            shows(release, &names),
            vec![("The Walking Dead".to_string(), Some(6))]
        );
        // Not where they are far apart, as films that start with a number are.
        let far = ["01.mkv", "02.mkv", "03.mkv", "40.mkv"];
        assert!(shows(folder, &far).is_empty());
    }

    #[test]
    fn a_folder_listed_again_is_judged_again() {
        let names = ["Show 01.mkv", "Show 02.mkv", "Show 03.mkv", "Show 04.mkv"];
        let paths = listed("/again", &names);
        assert!(matches!(parse(&paths[0]), Media::Episode { .. }));
        listed("/again", &names[..3]);
        assert!(matches!(parse(&paths[0]), Media::Movie { .. }));
    }
}
