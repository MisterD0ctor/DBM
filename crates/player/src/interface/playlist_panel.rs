//! The playlist panel's models: the parts of the list, and what the way on is
//! called.
//!
//! What the parts are is `shelf`'s business; this turns its answer into the
//! structs the panel's rows bind to.

use slint::{Model, ModelRc, VecModel};

use crate::interface::format;
use crate::library::shelf::{self, Entry, Group, Shelf};
use crate::playback::state::PlayerState;
use crate::{MainWindow, PlaylistGroup, PlaylistItem};

/// Hand the playlist alone to the UI.
///
/// Separate so a scan result, which changes nothing about the tracks, does
/// not rebuild the subtitle and audio menus as well — and so a row the
/// pointer is resting on is recreated no more often than it has to be.
pub fn push(ui: &MainWindow, player: &PlayerState) {
    let items: Vec<shelf::Item> = player
        .playlist
        .iter()
        .enumerate()
        .map(|(row, e)| shelf::Item {
            path: &e.filename,
            title: player.known_title(row),
            index: e.index,
            current: e.current,
            progress: player.known_progress(row),
            failed: player.failed.contains(&e.filename),
        })
        .collect();
    let shelf = shelf::arrange(&items, &player.beside);

    ui.set_next_label(next_label(&shelf).into());
    ui.set_playlist_named(shelf.heading.is_some());
    ui.set_playlist_heading(match shelf.title() {
        Some(title) => title.into(),
        None => slint::SharedString::from("PLAYLIST"),
    });
    // Where the panel opens: the part holding the file playing, on its row.
    let current = shelf.groups.iter().position(|g| g.current().is_some());
    ui.set_playlist_current_group(current.map_or(-1, |g| g as i32));
    ui.set_playlist_drills(shelf.groups.iter().any(|g| !g.leaf));

    // The page on show is kept by its name, not its place. Seasons found
    // beside this one arrive a moment after it and go in around it, and a
    // page held by number would turn into another season under the reader.
    let shown = usize::try_from(ui.get_playlist_page())
        .ok()
        .and_then(|page| ui.get_playlist_groups().row_data(page))
        .map(|group| group.label);
    let groups: Vec<PlaylistGroup> = shelf.groups.iter().map(group_item).collect();
    if let Some(label) = shown {
        ui.set_playlist_page(
            groups
                .iter()
                .position(|g| g.label == label)
                .map_or(-1, |page| page as i32),
        );
    }
    ui.set_playlist_groups(ModelRc::new(VecModel::from(groups)));
}

/// What the way on is called: the file after this one, as its row names it.
///
/// The row's own words rather than the file's title, because they are
/// already in the form the panel says them — "E04 · Slabtown", with the show
/// and season said once in the heading over the rows. The end pill has no
/// heading over it, but it has the film that just ended, which is the same
/// show and season, so the row's words still read whole there. Across the
/// boundary into another part they would not: an E01 could be anybody's, so
/// the part goes first — "Season 6 · E01 · No Way Out". A part that is one
/// film is named by the film, and its row would only say it again.
///
/// Empty at the end of the list, and for a file playing that the list does
/// not hold; the pills fall back to their own words.
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

/// One part of the list, as the panel draws it.
fn group_item(group: &Group) -> PlaylistGroup {
    let count = group.entries.len();
    PlaylistGroup {
        label: group.label.as_str().into(),
        // A heading shouts the interface's own words and never the user's:
        // SEASON 7, but a show's name in its own case.
        heading: (if group.named {
            group.label.clone()
        } else {
            group.label.to_uppercase()
        })
        .into(),
        titular: group.named,
        detail: if count == 1 {
            "1 episode".into()
        } else {
            format!("{count} episodes").into()
        },
        current: group.current().is_some(),
        leaf: group.leaf,
        open_row: group.open_row() as i32,
        entries: ModelRc::new(VecModel::from(
            group.entries.iter().map(entry_item).collect::<Vec<_>>(),
        )),
    }
}

fn entry_item(entry: &Entry) -> PlaylistItem {
    PlaylistItem {
        index: entry.index.map_or(-1, |i| i as i32),
        path: entry.path.as_str().into(),
        label: entry.label.as_str().into(),
        current: entry.current,
        // A file never played here has no length to show and no progress to
        // draw; an empty string and a zero say so, and the row leaves both
        // out rather than printing "0:00".
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
    use crate::library::durations::Progress;

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
