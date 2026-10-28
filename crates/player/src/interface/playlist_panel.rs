//! The playlist panel's models: the parts of the list, and what the way on is
//! called.
//!
//! What the parts are is `shelf`'s business; this turns its answer into the
//! structs the panel's rows bind to.

use std::rc::Rc;

use slint::{Model, ModelRc, VecModel};

use crate::interface::format;
use crate::lang::{fill, tr};
use crate::library::durations::Progress;
use crate::library::shelf::{self, Beside, Entry, Group, Shelf};
use crate::playback::state::PlayerState;
use crate::worker::{Completion, Worker};
use crate::{MainWindow, PlaylistGroup, PlaylistItem};

/// A playlist row as the player had it when the panel was asked for, owned so
/// it can cross to the worker.
struct Row {
    path: String,
    title: Option<String>,
    index: i64,
    current: bool,
    progress: Progress,
    failed: bool,
}

/// The playlist panel: asks for its layout, and shows the answer.
///
/// Laying a list out means reading every file's name for its show, season
/// and episode, so it is done on the worker and only the finished [`Shelf`]
/// comes back. It is asked for whenever something a row says has moved, and
/// answers arrive in order, so the last one shown is the newest.
#[derive(Default)]
pub struct Panel {
    /// What the panel is showing, to tell what an answer changes.
    shown: Option<Shelf>,
    /// The models the rows are bound to, kept so an answer that only changes
    /// what rows say can be written into them.
    groups: Option<Rc<VecModel<PlaylistGroup>>>,
    pages: Vec<Rc<VecModel<PlaylistItem>>>,
}

impl Panel {
    /// Have the playlist laid out as it stands now.
    pub fn request(&self, player: &PlayerState, worker: &Worker) {
        let rows: Vec<Row> = player
            .playlist
            .iter()
            .enumerate()
            .map(|(row, e)| Row {
                path: e.filename.clone(),
                title: player.known_title(row).map(str::to_owned),
                index: e.index,
                current: e.current,
                progress: player.known_progress(row),
                failed: player.failed.contains(&e.id),
            })
            .collect();
        let beside: Vec<Beside> = player.beside.clone();
        worker.submit(move |_mpv| {
            let items: Vec<shelf::Item> = rows
                .iter()
                .map(|row| shelf::Item {
                    path: &row.path,
                    title: row.title.as_deref(),
                    index: row.index,
                    current: row.current,
                    progress: row.progress,
                    failed: row.failed,
                })
                .collect();
            Some(Completion::Shelved(shelf::arrange(&items, &beside)))
        });
    }

    /// Show a finished layout. Returns whether the rows were built anew,
    /// rather than written into or left alone.
    ///
    /// Rows are written in place when the list is the one on show and only
    /// what they say has moved, such as a length just read. Replacing the
    /// model would rebuild every row and restart whatever was moving, many
    /// times a second while a folder is first described.
    pub fn show(&mut self, ui: &MainWindow, shelf: Shelf) -> bool {
        if self.shown.as_ref() == Some(&shelf) {
            return false;
        }
        ui.set_next_label(next_label(&shelf).into());
        ui.set_playlist_named(shelf.heading.is_some());
        ui.set_playlist_heading(match shelf.title() {
            Some(title) => title.into(),
            None => slint::SharedString::from(tr("PLAYLIST")),
        });
        // Where the panel opens: the part holding the file playing, on its row.
        let current = shelf.groups.iter().position(|g| g.current().is_some());
        ui.set_playlist_current_group(current.map_or(-1, |g| g as i32));
        ui.set_playlist_drills(shelf.groups.iter().any(|g| !g.leaf));

        let rewritten = self.rewrite(&shelf);
        if !rewritten {
            self.replace(ui, &shelf);
        }
        if crate::diagnostics::trace() {
            eprintln!(
                "dbm: playlist {} in {} part(s)",
                if rewritten { "rewritten" } else { "rebuilt" },
                shelf.groups.len()
            );
        }
        self.shown = Some(shelf);
        !rewritten
    }

    /// Write what moved into the rows on show. `false` when the list is not
    /// the one they were built for, and nothing was touched.
    fn rewrite(&self, shelf: &Shelf) -> bool {
        let (Some(shown), Some(groups)) = (&self.shown, &self.groups) else {
            return false;
        };
        if !same_rows(shown, shelf) || self.pages.len() != shelf.groups.len() {
            return false;
        }
        for (g, (was, now)) in shown.groups.iter().zip(&shelf.groups).enumerate() {
            if was == now {
                continue;
            }
            for (row, (old, new)) in was.entries.iter().zip(&now.entries).enumerate() {
                if old != new {
                    self.pages[g].set_row_data(row, entry_item(new));
                }
            }
            if was.current().is_some() != now.current().is_some()
                || was.open_row() != now.open_row()
            {
                groups.set_row_data(g, group_item(now, &self.pages[g]));
            }
        }
        true
    }

    /// Build the rows afresh, for a list that is not the one on show.
    fn replace(&mut self, ui: &MainWindow, shelf: &Shelf) {
        // The page on show is kept by its name, not its place, since seasons
        // found beside this one arrive later and go in around it.
        let shown = usize::try_from(ui.get_playlist_page())
            .ok()
            .and_then(|page| ui.get_playlist_groups().row_data(page))
            .map(|group| group.label);
        self.pages = shelf
            .groups
            .iter()
            .map(|g| {
                Rc::new(VecModel::from(
                    g.entries.iter().map(entry_item).collect::<Vec<_>>(),
                ))
            })
            .collect();
        let groups: Vec<PlaylistGroup> = shelf
            .groups
            .iter()
            .zip(&self.pages)
            .map(|(group, page)| group_item(group, page))
            .collect();
        if let Some(label) = shown {
            ui.set_playlist_page(
                groups
                    .iter()
                    .position(|g| g.label == label)
                    .map_or(-1, |page| page as i32),
            );
        }
        let groups = Rc::new(VecModel::from(groups));
        ui.set_playlist_groups(ModelRc::from(groups.clone()));
        self.groups = Some(groups);
    }
}

/// Whether two layouts are the same files in the same parts under the same
/// names, whatever else about a row differs.
fn same_rows(a: &Shelf, b: &Shelf) -> bool {
    a.groups.len() == b.groups.len()
        && a.groups.iter().zip(&b.groups).all(|(a, b)| {
            a.label == b.label
                && a.named == b.named
                && a.leaf == b.leaf
                && a.entries.len() == b.entries.len()
                && a.entries
                    .iter()
                    .zip(&b.entries)
                    .all(|(a, b)| a.path == b.path)
        })
}

/// What the way on is called: the file after this one, as its row names it.
///
/// The row's own words ("E04 · Slabtown") rather than the file's title. They
/// still read whole on the end pill, over the film that just ended, which is
/// the same show and season. Across into another part an E01 could be
/// anybody's, so the part goes first: "Season 6 · E01 · No Way Out". A part
/// that is one film is named by the film.
///
/// Empty at the end of the list, or for a file the list doesn't hold, and the
/// pills fall back to their own words.
fn next_label(shelf: &Shelf) -> String {
    let rows = || {
        shelf
            .groups
            .iter()
            .enumerate()
            .flat_map(|(part, g)| g.entries.iter().map(move |e| (part, g, e)))
    };
    let Some((part, playing)) = rows()
        .find(|(_, _, e)| e.current)
        .and_then(|(part, _, e)| Some((part, e.index?)))
    else {
        return String::new();
    };
    let Some((next_part, group, entry)) = rows().find(|(_, _, e)| e.index == Some(playing + 1))
    else {
        return String::new();
    };
    if next_part == part {
        entry.label.clone()
    } else if group.leaf {
        group.label.clone()
    } else {
        format!("{} · {}", group.label, entry.label)
    }
}

/// One part of the list, as the panel draws it, over the rows of its page.
fn group_item(group: &Group, page: &Rc<VecModel<PlaylistItem>>) -> PlaylistGroup {
    let count = group.entries.len();
    PlaylistGroup {
        label: group.label.as_str().into(),
        // Capitals for the interface's own words, never the user's: SEASON 7,
        // but a show's name in its own case.
        heading: (if group.named {
            group.label.clone()
        } else {
            group.label.to_uppercase()
        })
        .into(),
        titular: group.named,
        detail: if count == 1 {
            tr("1 episode").into()
        } else {
            fill(tr("{} episodes"), &[&count]).into()
        },
        current: group.current().is_some(),
        leaf: group.leaf,
        open_row: group.open_row() as i32,
        entries: ModelRc::from(page.clone()),
    }
}

fn entry_item(entry: &Entry) -> PlaylistItem {
    PlaylistItem {
        index: entry.index.map_or(-1, |i| i as i32),
        path: entry.path.as_str().into(),
        label: entry.label.as_str().into(),
        current: entry.current,
        // A file never played here has no length or progress, so the row
        // leaves both out rather than printing "0:00".
        length: if entry.progress.seconds > 0.0 {
            format::time(entry.progress.seconds).into()
        } else {
            Default::default()
        },
        progress: entry.progress.fraction,
        finished: entry.progress.finished,
        failed: entry.failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, index: i64, current: bool) -> shelf::Item<'_> {
        shelf::Item {
            path,
            title: None,
            index,
            current,
            progress: Progress::default(),
            failed: false,
        }
    }

    fn next(items: &[shelf::Item]) -> String {
        next_label(&shelf::arrange(items, &[]))
    }

    #[test]
    fn within_a_season_the_next_row_says_it_alone() {
        let items = [
            item("Show.S07E01.mkv", 0, true),
            item("Show.S07E02.Slabtown.mkv", 1, false),
        ];
        assert_eq!(next(&items), "E02 · Slabtown");
    }

    #[test]
    fn at_the_end_of_the_list_there_is_nothing_to_name() {
        let items = [
            item("Show.S07E01.mkv", 0, false),
            item("Show.S07E02.mkv", 1, true),
        ];
        assert_eq!(next(&items), "");
    }

    #[test]
    fn into_another_season_the_season_goes_first() {
        let items = [
            item("Show.S05E16.mkv", 0, true),
            item("Show.S06E01.No.Way.Out.mkv", 1, false),
        ];
        assert_eq!(next(&items), "Season 6 · E01 · No Way Out");
    }

    #[test]
    fn a_film_is_named_by_itself() {
        let items = [
            item("Show.A.S01E01.mkv", 0, false),
            item("Show.A.S01E02.mkv", 1, true),
            item("Alien (1979).mkv", 2, false),
        ];
        assert_eq!(next(&items), "Alien (1979)");
    }
}
