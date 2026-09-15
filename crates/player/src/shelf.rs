//! The shape a playlist is shown in.
//!
//! mpv plays one flat list, in the order [`crate::playlist::watching_order`]
//! settles, and Next and autoplay walk that list as they always did. This is
//! only how the panel lays it out. A season of one show is shown as it always
//! was. A list spanning several seasons becomes a list of those seasons, each
//! opening onto its own episodes; a folder of several shows becomes a list of
//! each show's seasons, with anything that is one file on its own — a film, a
//! stray episode — standing in that list as itself.
//!
//! Beside a single season sit the seasons around it on disk. Opening an
//! episode means that season, so they are not queued; they are shown, and an
//! episode picked from one opens its folder at that episode.

use std::path::Path;

use crate::durations::Progress;
use crate::naming::Media;
use crate::worker::{Completion, Worker};

/// A file as the playlist has it.
pub struct Item<'a> {
    pub path: &'a str,
    /// The container's own title, where one is known.
    pub title: Option<&'a str>,
    pub index: i64,
    pub current: bool,
    pub progress: Progress,
    pub failed: bool,
}

/// A season of the playing show, found in a folder beside the playlist's own.
#[derive(Debug, Clone, PartialEq)]
pub struct Beside {
    pub season: u32,
    /// Its files in watching order, and how far into each you got.
    pub files: Vec<(String, Progress)>,
}

/// One row of a part's page.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub label: String,
    pub path: String,
    /// Its place in mpv's playlist, or `None` for a file in a season beside it.
    pub index: Option<i64>,
    pub current: bool,
    pub progress: Progress,
    pub failed: bool,
}

/// One part of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// How a row names it: *Season 7*, *Specials*, *Frieren · Season 2*, or a
    /// film's own name.
    pub label: String,
    /// Whether that name has the user's words in it — a show's, a film's —
    /// rather than only the interface's, which decides whether a heading may
    /// set it in capitals.
    pub named: bool,
    /// The season it is, for putting seasons found beside the list in order
    /// among it.
    season: Option<u32>,
    /// One file, shown in the list of parts as itself and played from there.
    pub leaf: bool,
    pub entries: Vec<Entry>,
}

impl Group {
    /// The row of the file playing, if it is in here.
    pub fn current(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.current)
    }

    /// The row a page of it opens on: the file playing, or else the first not
    /// yet finished — where watching it would carry on.
    pub fn open_row(&self) -> usize {
        self.current()
            .or_else(|| self.entries.iter().position(|e| !e.progress.finished))
            .unwrap_or(0)
    }

}

/// A playlist, in parts.
pub struct Shelf {
    /// The show every file belongs to, if there is one.
    pub heading: Option<String>,
    pub groups: Vec<Group>,
}

impl Shelf {
    /// What the panel's heading says: the show, and its season as well when
    /// the whole list is one season. The rows leave the season off, so with
    /// no page of seasons to say it, the heading has to.
    pub fn title(&self) -> Option<String> {
        let show = self.heading.as_ref()?;
        Some(match self.groups.as_slice() {
            [only] if only.season.is_some() => format!("{show} · {}", only.label),
            _ => show.clone(),
        })
    }
}

/// What a run of files belongs to.
#[derive(PartialEq)]
enum Part {
    Show(String, Option<u32>),
    Alone,
}

/// Lay a playlist out in parts, with any seasons found beside it.
pub fn arrange(items: &[Item], beside: &[Beside]) -> Shelf {
    let heading = crate::naming::listing(items.iter().map(|i| (i.path, i.title))).heading;

    // Runs of consecutive files rather than a gathering by key. The list is in
    // watching order, which already keeps a show's files together; were one
    // ever split, two parts in their places would be truer to what autoplay
    // will do than one part collected from both ends of the queue.
    let mut runs: Vec<(Part, Option<String>, Vec<usize>)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let (part, show) = match crate::naming::parse(item.path) {
            Media::Episode { show, season, .. } => {
                (Part::Show(show.to_lowercase(), season), Some(show))
            }
            Media::Movie { .. } => (Part::Alone, None),
        };
        let joins = matches!(
            (runs.last(), &part),
            (Some((last, _, _)), Part::Show(..)) if *last == part
        );
        match runs.last_mut() {
            Some(run) if joins => run.2.push(i),
            _ => runs.push((part, show, vec![i])),
        }
    }

    let mut groups: Vec<Group> = runs
        .into_iter()
        .map(|(part, show, members)| {
            // Only in a list of several things is one file a thing of its
            // own. In one show's list a season of one episode is a season.
            let leaf = members.len() == 1 && heading.is_none();
            let season = match part {
                Part::Show(_, season) => season,
                Part::Alone => None,
            };
            let labels: Vec<String> = if leaf {
                let item = &items[members[0]];
                vec![crate::naming::titled(item.path, item.title)]
            } else {
                in_season(
                    crate::naming::listing(members.iter().map(|&i| (items[i].path, items[i].title)))
                        .rows,
                    season,
                )
            };
            let (label, named) = if leaf {
                (labels[0].clone(), true)
            } else {
                part_label(season, show.as_deref(), heading.is_some())
            };
            let entries = members
                .iter()
                .zip(labels)
                .map(|(&i, label)| {
                    let item = &items[i];
                    Entry {
                        label,
                        path: item.path.to_string(),
                        index: Some(item.index),
                        current: item.current,
                        progress: item.progress,
                        failed: item.failed,
                    }
                })
                .collect();
            Group {
                label,
                named,
                season,
                leaf,
                entries,
            }
        })
        .collect();

    // Seasons beside a single one, in among it by number. Only then: a list
    // already spanning seasons holds its own, and a mixed one has no one show
    // to look for.
    if heading.is_some() && groups.len() == 1 && !beside.is_empty() {
        for b in beside {
            let labels = in_season(
                crate::naming::listing(b.files.iter().map(|(p, _)| (p.as_str(), None))).rows,
                Some(b.season),
            );
            let (label, named) = part_label(Some(b.season), None, true);
            groups.push(Group {
                label,
                named,
                season: Some(b.season),
                leaf: false,
                entries: b
                    .files
                    .iter()
                    .zip(labels)
                    .map(|((path, progress), label)| Entry {
                        label,
                        path: path.clone(),
                        index: None,
                        current: false,
                        progress: *progress,
                        failed: false,
                    })
                    .collect(),
            });
        }
        groups.sort_by_key(|g| g.season);
    }

    Shelf { heading, groups }
}

/// A part's rows, less the season its heading already says.
fn in_season(rows: Vec<String>, season: Option<u32>) -> Vec<String> {
    match season {
        Some(n) => rows
            .iter()
            .map(|row| crate::naming::without_season(row, n))
            .collect(),
        None => rows,
    }
}

/// What a part is called: its season, and its show's name as well where the
/// list is not all one show's.
fn part_label(season: Option<u32>, show: Option<&str>, one_show: bool) -> (String, bool) {
    let part = match season {
        Some(0) => "Specials".to_string(),
        Some(n) => format!("Season {n}"),
        None => "Episodes".to_string(),
    };
    match show.filter(|_| !one_show) {
        None => (part, false),
        Some(show) if season.is_none() => (show.to_string(), true),
        Some(show) => (format!("{show} · {part}"), true),
    }
}

// ---------------------------------------------------------------------------
// Looking beside the list
// ---------------------------------------------------------------------------

/// Asks for the seasons beside the playlist, once per list.
#[derive(Default)]
pub struct Neighbours {
    asked: Vec<String>,
    /// Ask again for the same list. How far into each file you got is written
    /// to disk behind the player's back, and opening the panel is when that is
    /// worth reading again.
    stale: bool,
}

impl Neighbours {
    pub fn refresh(&mut self) {
        self.stale = true;
    }

    /// Look beside this list if it is a new one, or the last look is stale.
    ///
    /// Returns whether the list itself changed, so what was found beside the
    /// old one can be put away rather than shown around another show.
    pub fn request(&mut self, paths: &[String], worker: &Worker) -> bool {
        let moved = paths != self.asked.as_slice();
        if !moved && !self.stale {
            return false;
        }
        self.stale = false;
        self.asked = paths.to_vec();
        let paths = paths.to_vec();
        worker.submit(move |_mpv| {
            let seasons = look_beside(&paths);
            Some(Completion::Beside { paths, seasons })
        });
        moved
    }
}

/// The other seasons of the show a playlist is one season of.
///
/// **Blocking.** Worker thread only: it lists the folder above the playlist's
/// own, looks in each folder there, and reads every file's resume point.
///
/// Nothing for a list that is not one season of one show in one folder.
pub fn look_beside(paths: &[String]) -> Vec<Beside> {
    let Some(first) = paths.first() else {
        return Vec::new();
    };
    let Media::Episode {
        show,
        season: Some(season),
        ..
    } = crate::naming::parse(first)
    else {
        return Vec::new();
    };
    let folder = Path::new(first).parent();
    let one_season = paths.iter().all(|p| {
        Path::new(p).parent() == folder
            && matches!(
                crate::naming::parse(p),
                Media::Episode { show: ref s, season: Some(n), .. }
                    if n == season && s.eq_ignore_ascii_case(&show)
            )
    });
    if !one_season {
        return Vec::new();
    }
    crate::playlist::seasons_beside(Path::new(first))
        .into_iter()
        .filter_map(|(dir, season)| {
            let files: Vec<String> = crate::playlist::scan_flat(&dir)
                .ok()?
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let progress = crate::durations::of(&files);
            Some(Beside {
                season,
                files: files.into_iter().zip(progress).collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, index: i64) -> Item<'_> {
        Item {
            path,
            title: None,
            index,
            current: false,
            progress: Progress::default(),
            failed: false,
        }
    }

    fn labels(shelf: &Shelf) -> Vec<&str> {
        shelf.groups.iter().map(|g| g.label.as_str()).collect()
    }

    fn done() -> Progress {
        Progress {
            finished: true,
            ..Progress::default()
        }
    }

    #[test]
    fn one_season_is_one_part_as_it_always_was() {
        let mut items = vec![item("Show.S07E01.mkv", 0), item("Show.S07E02.mkv", 1)];
        items[1].current = true;
        let shelf = arrange(&items, &[]);
        assert_eq!(shelf.heading.as_deref(), Some("Show"));
        assert_eq!(labels(&shelf), ["Season 7"]);
        let group = &shelf.groups[0];
        assert!(!group.leaf);
        assert_eq!(group.current(), Some(1));
        // The heading says the season, so the rows need not.
        assert_eq!(shelf.title().as_deref(), Some("Show · Season 7"));
        assert_eq!(group.entries[0].label, "E01");
    }

    #[test]
    fn seasons_beside_one_go_in_around_it() {
        let items = vec![item("Show.S07E01.mkv", 0), item("Show.S07E02.mkv", 1)];
        let beside = vec![
            Beside {
                season: 8,
                files: vec![("Show.S08E01.mkv".into(), done())],
            },
            Beside {
                season: 6,
                files: vec![("Show.S06E01.mkv".into(), Progress::default())],
            },
        ];
        let shelf = arrange(&items, &beside);
        assert_eq!(labels(&shelf), ["Season 6", "Season 7", "Season 8"]);
        assert_eq!(shelf.groups[0].entries[0].index, None);
        assert_eq!(shelf.groups[1].entries[1].index, Some(1));
        assert_eq!(shelf.groups[2].entries[0].label, "E01");
        // A list of seasons says the show; each season's page says the rest.
        assert_eq!(shelf.title().as_deref(), Some("Show"));
    }

    #[test]
    fn a_whole_show_is_its_seasons() {
        let items = vec![
            item("Show.S00E01.mkv", 0),
            item("Show.S01E01.mkv", 1),
            item("Show.S01E02.mkv", 2),
            item("Show.S02E01.mkv", 3),
        ];
        let shelf = arrange(&items, &[]);
        assert_eq!(labels(&shelf), ["Specials", "Season 1", "Season 2"]);
        assert!(shelf.groups.iter().all(|g| !g.leaf && !g.named));
    }

    #[test]
    fn a_mixed_folder_is_its_shows_and_films() {
        let items = vec![
            item("Show.A.S01E01.mkv", 0),
            item("Show.A.S01E02.mkv", 1),
            item("Alien (1979).mkv", 2),
            item("Show.B.S02E05.mkv", 3),
        ];
        let shelf = arrange(&items, &[]);
        assert_eq!(shelf.heading, None);
        assert_eq!(shelf.groups.len(), 3);
        assert_eq!(shelf.groups[0].label, "Show A · Season 1");
        assert!(shelf.groups[0].named && !shelf.groups[0].leaf);
        assert!(shelf.groups[1].leaf);
        // A lone episode among other things names its show on its own row.
        assert!(shelf.groups[2].leaf);
        assert!(shelf.groups[2].entries[0].label.starts_with("Show B"));
    }

    #[test]
    fn a_page_opens_where_watching_carries_on() {
        let entry = |progress: Progress| Entry {
            label: String::new(),
            path: String::new(),
            index: None,
            current: false,
            progress,
            failed: false,
        };
        let half = Progress {
            fraction: 0.5,
            ..Progress::default()
        };
        let mut group = Group {
            label: "Season 1".into(),
            named: false,
            season: Some(1),
            leaf: false,
            entries: vec![entry(done()), entry(done()), entry(half), entry(Progress::default())],
        };
        assert_eq!(group.open_row(), 2);
        group.entries[0].current = true;
        assert_eq!(group.open_row(), 0);
    }
}
