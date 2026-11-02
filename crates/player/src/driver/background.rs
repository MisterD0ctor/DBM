use dbm_lang::{fill, tr};
use slint::ComponentHandle;

use super::{Driver, Gpu};
use crate::interface::{format, sync};
use crate::worker::Completion;
use crate::{Find, MainWindow, Playback, Subtitles};

impl Driver {
    /// Ask for the background work the player's state calls for, and take
    /// delivery of whatever finished.
    pub(super) fn background(&mut self, ui: &MainWindow, gpu: &mut Gpu) {
        // Opening the playlist is the cue to go and look at the resume
        // positions again: they are written to disk behind our back, and this
        // is the moment somebody cares what they say.
        let open = ui.get_playlist_open();
        if open && !self.playlist_open {
            self.feeds.lists.refresh();
            self.feeds.neighbours.refresh();
        }
        self.playlist_open = open;

        // Each of these asks only when what it follows has moved; the work
        // itself is on the worker.
        self.feeds.lists.poll(&self.app.worker, &self.player);
        self.feeds.durations.poll(&self.app.worker, &self.player);
        self.poll_preview(ui);

        // Several probe results can land in one drain, and seasons found
        // beside the list with them; the playlist is laid out once for all.
        // The worker's own handle, so the drain does not hold the driver
        // borrowed while each result is taken in.
        let worker = self.app.worker.clone();
        let mut playlist_moved = false;
        for completion in worker.drain() {
            playlist_moved |= self.receive(ui, gpu, completion);
        }
        if playlist_moved {
            self.panel.request(&self.player, &self.app.worker);
        }

        self.look_onward(ui);
        self.light_the_end();
        self.listen_for_the_titles();
        self.put_back_downloads(ui);
        self.say_it_again(ui);
    }

    /// Put back the subtitles fetched for this file on an earlier visit,
    /// once it has opened, and close a search still open for the last one.
    fn put_back_downloads(&mut self, ui: &MainWindow) {
        if self.player.path == self.per_file.kept_asked {
            return;
        }
        ui.set_find_open(false);
        let find = ui.global::<Find>();
        find.set_state(0);
        find.set_fetching(-1);
        // Not until it has opened: a track added before then is lost with
        // the load it raced.
        let Some(path) = self
            .player
            .path
            .clone()
            .filter(|_| self.player.duration > 0.0)
        else {
            return;
        };
        self.per_file.kept_asked = Some(path.clone());
        self.app.worker.submit(move |_mpv| {
            let files = dbm_library::opensubtitles::kept(&path);
            (!files.is_empty()).then_some(Completion::Kept { path, files })
        });
    }

    /// Say everything the Rust side has handed the interface over again when
    /// the language changes: track names, the chapter, the playlist's parts,
    /// what is left of the film on the way in. The interface's own words
    /// follow by themselves; these were sentences by the time it got them.
    fn say_it_again(&mut self, ui: &MainWindow) {
        let changes = dbm_lang::changes();
        if changes == self.language_seen {
            return;
        }
        self.language_seen = changes;
        sync::push_lists(ui, &self.player);
        sync::push_scalars(ui, &self.player);
        self.panel.request(&self.player, &self.app.worker);
        // Every panel's words have changed for ones it may not have drawn.
        for panel in crate::interface::warm::ALL {
            self.warm.ask(panel);
        }
        if let Some(seconds) = self.resume_left {
            ui.global::<Playback>()
                .set_resume_left(format::left(seconds).into());
        }
    }

    /// Ask for a thumbnail atlas when the playing file changes.
    ///
    /// Driven off the mirrored `path` rather than off opening a playlist: what
    /// wants thumbnails is whatever is on screen now, and that changes at the
    /// end of every file as well as when someone picks one.
    fn poll_preview(&self, ui: &MainWindow) {
        let playback = ui.global::<Playback>();
        let preview = &self.app.preview;
        preview.set_duration(self.player.duration);
        preview.set_chapters(&self.player.chapters);
        let Some(path) = self.player.path.clone() else {
            return;
        };
        if !preview.claim(&path) {
            return;
        }
        // The old atlas belongs to the old file. Cleared now rather than left
        // under the pointer showing the previous film for however long the new
        // one takes to build.
        preview.set_sprite(None);
        playback.set_preview_tile_w(0);
        playback.set_preview_tile_h(0);
        // A sync still listening is listening to the old film, and one that
        // could be undone was this film's to undo.
        dbm_library::subsync::supersede();
        ui.global::<Subtitles>().set_sync_state(0);
        // The old file's decoders are gone by now, so what they held can go back.
        self.app
            .worker
            .run(|_| crate::platform::memory::give_back());

        // Copied into Slint's buffer as it is reported, so still on the
        // build's own thread rather than this one.
        let shown = |atlas: dbm_library::preview::Atlas| Completion::Preview {
            pixels: slint::SharedPixelBuffer::clone_from_slice(
                &atlas.rgba,
                atlas.width(),
                atlas.height(),
            ),
            sprite: atlas.sprite,
        };
        dbm_library::preview::spawn(path, self.app.worker.reporter(shown));
    }

    /// Look for the way on at the end of the last file, once per file — or in
    /// its closing credits, where the credits pill makes the same offer before
    /// the file is over. The offer belongs to the file that asked for it, so
    /// a new one takes it back down.
    fn look_onward(&mut self, ui: &MainWindow) {
        let playback = ui.global::<Playback>();
        let ending =
            (self.player.eof_reached || self.player.credits_rolling()) && self.player.at_last();
        if self.player.path != self.per_file.onward_asked
            && self.per_file.onward_asked.take().is_some()
        {
            playback.set_onward_path("".into());
            playback.set_onward_label("".into());
            playback.set_onward_season(false);
            playback.set_season_done("".into());
        }
        if !ending || self.per_file.onward_asked.is_some() {
            return;
        }
        let Some(path) = self.player.path.clone() else {
            return;
        };
        self.per_file.onward_asked = Some(path.clone());
        // Season 0 is the specials, which no one finishes as a season.
        if let dbm_naming::Media::Episode {
            season: Some(season @ 1..),
            ..
        } = dbm_naming::parse(path.as_str())
        {
            playback.set_season_done(fill(tr("Season {} finished"), &[&season]).into());
        }
        self.app.worker.submit(move |_mpv| {
            let to = dbm_library::onward::after(&path);
            Some(Completion::Onward { from: path, to })
        });
    }

    /// Light the end of a file as the empty window is lit, once per file, so
    /// the glass has a picture to bend where the film has left it black: with
    /// the most lit frame of the file itself, before its credits. Never of
    /// what comes next, whose frame would show a scene before it is watched.
    fn light_the_end(&mut self) {
        let Some(path) = self.player.path.clone() else {
            return;
        };
        if !self.player.eof_reached || self.per_file.end_light_asked.as_ref() == Some(&path) {
            return;
        }
        self.per_file.end_light_asked = Some(path.clone());
        self.app.worker.submit(move |_mpv| {
            let still = dbm_library::preview::lit_still(&path, 0.9);
            Some(Completion::EndLight { from: path, still })
        });
    }

    /// Have an episode's title sequence and closing credits listened for,
    /// once per file, whichever of them its chapters do not already mark.
    ///
    /// Not until the lists have been read for this file and hold it: the
    /// episodes beside it are what it is compared with, and the chapters
    /// that could make the whole thing unnecessary arrive in the same read.
    fn listen_for_the_titles(&mut self) {
        use dbm_library::titles::{self, End};
        let Some(path) = self.player.path.as_ref() else {
            return;
        };
        if self.per_file.titles_asked.as_ref() == Some(path)
            || self.player.duration <= 0.0
            || !self.feeds.lists.settled(&self.player)
            || !self.player.playlist.iter().any(|e| e.filename == *path)
        {
            return;
        }
        self.per_file.titles_asked = Some(path.clone());
        // The opening first: it is the one that is about to play.
        let mut ends = Vec::new();
        if self.player.intro_named().is_none() {
            ends.push(End::Opening);
        }
        if !self.player.credits_marked() {
            ends.push(End::Closing);
        }
        if ends.is_empty() {
            return;
        }
        let paths = self.player.playlist_paths();
        let beside = titles::beside(path, &paths);
        if beside.is_empty() {
            return;
        }
        titles::spawn(
            path.clone(),
            ends,
            beside,
            self.player.duration,
            self.app.worker.reporter(Completion::Titles),
        );
    }
}
