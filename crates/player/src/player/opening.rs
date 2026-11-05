//! Opening a path: from its scan to the first file mpv loads from it, or to
//! nothing.
//!
//! The interface says "Opening…" from the moment a path is asked for (see
//! `actions::opening`) until this says it is over. Keeping the resume file
//! from being written mid-way is `session`'s, since media keys move between
//! files from a thread of their own.

use std::collections::VecDeque;
use std::path::PathBuf;

use dbm_library::playlist::Prepared;
use slint::ComponentHandle;

use super::{Player, completions};
use crate::interface::warn;
use crate::playback::commands;
use crate::settings::Toggle;
use crate::{MainWindow, Playlist};

#[derive(Default)]
pub(super) struct Opening {
    /// Whether the list on its way is to start playing once it is in.
    starting: bool,
    /// The lists handed to `loadlist` and not yet answered, oldest first,
    /// each deleted when its answer comes, by when mpv has read it.
    lists: VecDeque<PathBuf>,
    /// Whether an open came to nothing since a file last loaded, so that if
    /// nothing plays now, the empty window looks for what to offer as it does
    /// at launch.
    stumbled: bool,
    /// `PlayerState::loads` when last looked at.
    loads_seen: u64,
}

impl Player {
    /// Load what an open found.
    pub(super) fn load(&mut self, list: Prepared) {
        log::info!("opened {} file(s), starting at {}", list.count, list.start);
        self.opening.starting = true;
        if list.ask {
            self.app
                .arrival
                .open_list(&self.app.mpv, list.first.clone(), list.saved);
        }
        if commands::load_list(&self.app.mpv, &list.m3u, list.start, &list.first) {
            self.opening.lists.push_back(list.m3u);
        } else {
            self.delete_list(list.m3u);
        }
    }

    /// The scan found nothing to load, so no file is coming.
    pub(super) fn not_opened(&mut self, ui: &MainWindow, e: String) {
        log::warn!("cannot open: {e}");
        ui.global::<Playlist>().set_opening(false);
        self.opening.stumbled = true;
        warn(ui, e);
    }

    /// A file wouldn't play. Which is the backstop for an open that never
    /// produces a file: a container mpv can't read fails here and no file
    /// ever loads, so without this the line would say "Opening…" until the
    /// next open that worked.
    pub(super) fn opening_failed(&mut self, ui: &MainWindow) {
        ui.global::<Playlist>().set_opening(false);
        self.opening.stumbled = true;
        // Its row says so, which the list otherwise only hears on its next
        // read.
        self.panel.request(&self.state, &self.app.worker);
    }

    /// mpv has answered a `loadlist`. The list exists now, and which entry
    /// plays was settled before it loaded (see `commands::load_list`), so
    /// this only makes sure it plays. Unless it is to be held on its saved
    /// place or started over, which is settled once it is in.
    pub(super) fn list_answered(&mut self) {
        if let Some(m3u) = self.opening.lists.pop_front() {
            self.delete_list(m3u);
        }
        if std::mem::take(&mut self.opening.starting) && !self.app.arrival.holding() {
            commands::set_pause(&self.app.mpv, false);
        }
    }

    /// mpv goes on from a file that wouldn't play as from one that ended, but
    /// with autoplay off nothing starts that wasn't asked for: the next file
    /// waits, paused, under the notice saying why.
    pub(super) fn hold_after_failure(&mut self) {
        if !self.app.settings.is_on(Toggle::Autoplay) {
            self.opening.starting = false;
            commands::set_pause(&self.app.mpv, true);
        }
    }

    /// An open is over when mpv finishes opening a file, not when it has one,
    /// which is already true while a film plays. Failures end it their own
    /// ways: a scan that found nothing in `not_opened`, and a file mpv
    /// couldn't play in `opening_failed`.
    pub(super) fn follow_opening(&mut self, ui: &MainWindow) {
        if self.state.loads != self.opening.loads_seen {
            self.opening.loads_seen = self.state.loads;
            self.opening.stumbled = false;
            ui.global::<Playlist>().set_opening(false);
        }
        // Back on the empty window after an open that came to nothing, which
        // offers what it would have at launch rather than the bare fill.
        if self.opening.stumbled && self.state.idle && self.state.path.is_none() {
            self.opening.stumbled = false;
            completions::look_for_resume(&self.app.worker);
        }
    }

    /// Delete a playlist file mpv is done with, off the UI thread.
    fn delete_list(&self, m3u: PathBuf) {
        self.app.worker.run(move |_mpv| {
            let _ = std::fs::remove_file(m3u);
        });
    }
}
