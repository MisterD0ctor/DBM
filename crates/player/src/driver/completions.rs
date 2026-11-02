//! What each kind of finished background work does to the player.
//!
//! Every variant of [`Completion`] is answered here, in one `match`, so
//! adding a kind of background job is a compile error until its result has
//! somewhere to go (see `worker`). Each arm hands its result to whatever owns
//! what it changes: the search to `interface::find`, a sync to
//! `interface::subsync`, and what the driver holds itself to a method below.

use dbm_library::onward::Onward;
use dbm_library::playlist::Prepared;
use dbm_library::preview::{Sprite, Still};
use dbm_library::resume::Resume;
use dbm_library::shelf::{Beside, Shelf};
use dbm_library::titles::{self, End};
use slint::ComponentHandle;

use super::{Driver, Gpu};
use crate::MainWindow;
use crate::Playlist;
use crate::interface::{format, say, subsync, warn};
use crate::playback::commands;
use crate::playback::state::Lists;
use crate::worker::{Completion, Worker};

impl Driver {
    /// Take delivery of one finished job. Returns whether the playlist needs
    /// laying out again, which the caller asks for once however many did.
    pub(super) fn receive(
        &mut self,
        ui: &MainWindow,
        gpu: &mut Gpu,
        completion: Completion,
    ) -> bool {
        let playing = self.player.path.as_deref();
        match completion {
            // The three that can move the playlist's rows.
            Completion::Lists(lists) => return self.take_lists(ui, lists),
            Completion::Probed(found) => return self.player.apply_probed(found),
            Completion::Beside { paths, seasons } => return self.take_beside(&paths, seasons),

            Completion::Shelved(shelf) => self.take_shelf(ui, shelf),
            Completion::Opened(Ok(list)) => self.load(list),
            Completion::Opened(Err(e)) => self.not_opened(ui, e),
            Completion::Notice(text) => say(ui, text),
            Completion::Warning(text) => warn(ui, text),
            Completion::Resume(Some(resume)) => self.offer_resume(ui, gpu, resume),
            // Nothing to continue, which on a fresh install is every time:
            // the window is lit by the player's own mark instead.
            Completion::Resume(None) => light_with_the_mark(&self.app.worker),
            Completion::Preview { sprite, pixels } => self.show_atlas(ui, sprite, pixels),
            Completion::Mark(still) => {
                show_backdrop(gpu, &still);
                ui.set_backdrop(true);
            }
            Completion::Titles(heard) => self.take_titles(ui, heard),
            Completion::Synced(outcome) => {
                subsync::answer(ui, &self.app.mpv, &self.player, outcome);
            }
            Completion::Found(found) => self.app.search.found(ui, playing, found),
            Completion::Fetched(fetched) => {
                self.app.search.fetched(ui, &self.app.mpv, playing, fetched);
            }
            Completion::SignedIn(result) => {
                self.app
                    .search
                    .signed_in(ui, &self.app.worker, playing, result);
            }
            Completion::Kept { path, files } => self.put_back(&path, files),
            Completion::EndLight { from, still } => self.light_the_end_with(gpu, from, still),
            Completion::Onward { from, to } => self.offer_onward(ui, &from, to),
        }
        false
    }

    /// The track list, the playlist and the chapters, read again: put in the
    /// mirrored state, and the work that follows from the playlist's paths
    /// asked for.
    fn take_lists(&mut self, ui: &MainWindow, lists: Lists) -> bool {
        let mut moved = self.feeds.lists.apply(ui, &mut self.player, lists);
        if moved {
            self.warm.ask(crate::interface::warm::MENU);
            self.warm.ask(crate::interface::warm::AUDIO);
        }
        // A read the lists have since moved past is thrown away, so the
        // newer one is asked for now: the poll in the frame has already
        // run, and a paused film may draw no other.
        self.feeds.lists.poll(&self.app.worker, &self.player);
        // What is saved for each entry, for choosing one on purpose.
        self.app.arrival.learn(
            (0..self.player.playlist.len())
                .map(|row| {
                    (
                        self.player.playlist[row].filename.clone(),
                        self.player.known_progress(row),
                    )
                })
                .collect(),
        );
        // Here because this is where the playlist's paths become
        // known. The scan compares them with the last request and
        // does nothing when only the tracks moved.
        let paths = self.player.playlist_paths();
        self.feeds.scan.request(&paths, &self.app.worker);
        // What was found beside the old list is put away rather than
        // shown around another show.
        if self.feeds.neighbours.request(&paths, &self.app.worker) && !self.player.beside.is_empty()
        {
            self.player.beside.clear();
            moved = true;
        }
        moved
    }

    /// The seasons found beside the playlist, for the list it was looked for
    /// beside, or for nothing.
    fn take_beside(&mut self, paths: &[String], seasons: Vec<Beside>) -> bool {
        let playing = self.player.playlist.iter().map(|e| e.filename.as_str());
        if playing.eq(paths.iter().map(String::as_str)) && self.player.beside != seasons {
            self.player.beside = seasons;
            return true;
        }
        false
    }

    fn take_shelf(&mut self, ui: &MainWindow, shelf: Shelf) {
        if self.panel.show(ui, shelf) {
            self.warm.ask(crate::interface::warm::PLAYLIST);
        }
    }

    /// Load what an open found.
    fn load(&mut self, list: Prepared) {
        log::info!("opened {} file(s), starting at {}", list.count, list.start);
        self.loading_list = true;
        if list.ask {
            self.app
                .arrival
                .open_list(&self.app.mpv, list.first.clone(), list.saved);
        }
        commands::load_list(&self.app.mpv, &list.m3u, list.start, &list.first);
    }

    fn not_opened(&mut self, ui: &MainWindow, e: String) {
        log::warn!("cannot open: {e}");
        // Nothing was found to load, so no file is coming.
        ui.global::<Playlist>().set_opening(false);
        self.stumbled = true;
        warn(ui, e);
    }

    /// Where the title sequence or the credits were heard. Kept with its
    /// path, so an answer for a file that has since been replaced is simply
    /// about something else.
    fn take_titles(&mut self, ui: &MainWindow, heard: titles::Outcome) {
        let Some(pieces) = heard.pieces else {
            return;
        };
        match heard.end {
            // A title sequence is heard as the one piece.
            End::Opening => {
                self.player.intro_heard = pieces.first().map(|&span| (heard.path, span));
            }
            End::Closing => self.player.credits_heard = Some((heard.path, pieces)),
        }
        // Pushed here, since a paused film reports nothing else that would.
        crate::interface::sync::push_scalars(ui, &self.player);
    }

    /// Subtitles fetched for this film before, back in the list as the
    /// film's own tracks are, without taking the place of whichever one it
    /// opened on.
    fn put_back(&self, path: &str, files: Vec<(std::path::PathBuf, String)>) {
        if self.player.path.as_deref() == Some(path) {
            for (file, language) in files {
                commands::add_subtitle(&self.app.mpv, &file, &language, false);
            }
        }
    }

    /// Light the end of `from` with its last frame. With no frame of its
    /// own, the end is lit by the mark, as a fresh install's empty window
    /// is: it still dims a last frame that may be a white one, and the
    /// season's line still has that to read on.
    fn light_the_end_with(&mut self, gpu: &mut Gpu, from: String, still: Option<Still>) {
        if self.player.path.as_deref() != Some(from.as_str()) {
            return;
        }
        self.per_file.end_marked = still.is_none();
        if let Some(still) = still.or_else(dbm_library::mark::still) {
            show_backdrop(gpu, &still);
            self.per_file.end_lit = Some(from);
        }
    }

    /// What to go on to after `from`. An answer for a file that has since
    /// been replaced is dropped.
    fn offer_onward(&self, ui: &MainWindow, from: &str, to: Option<Onward>) {
        if self.player.path.as_deref() == Some(from)
            && let Some(onward) = to
        {
            ui.set_onward_path(onward.path.as_str().into());
            ui.set_onward_label(onward.label.as_str().into());
            ui.set_onward_season(onward.season);
        }
    }

    /// Hand the seek preview a finished atlas. Its picture comes decoded, so
    /// this is the cost of wrapping it and nothing more.
    fn show_atlas(
        &mut self,
        ui: &MainWindow,
        sprite: Sprite,
        pixels: slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    ) {
        let image = slint::Image::from_rgba8(pixels);
        log::info!(
            "preview atlas {g}x{g} tiles of {}x{}",
            sprite.tile_w,
            sprite.tile_h,
            g = sprite.grid
        );
        ui.set_preview_sprite(image);
        ui.set_preview_tile_w(sprite.tile_w as i32);
        ui.set_preview_tile_h(sprite.tile_h as i32);
        self.app.preview.set_sprite(Some(sprite));
        // The pointer may already be on the timeline, in which case it is
        // showing a timestamp and an empty frame.
        let answer = self.app.preview.again();
        ui.set_preview_clip_x(answer.x);
        ui.set_preview_clip_y(answer.y);
        // An end reached before its atlas was built had no frame of its
        // own to be lit with, and now may.
        if self.per_file.end_lit != self.player.path || self.per_file.end_marked {
            self.per_file.end_light_asked = None;
        }
    }

    /// Put the film left unfinished on the way in, and its frame behind it.
    fn offer_resume(&mut self, ui: &MainWindow, gpu: &mut Gpu, resume: Resume) {
        log::info!(
            "last unfinished {} at {:.0}%{}",
            resume.path,
            resume.fraction * 100.0,
            if resume.still.is_some() {
                ", with a frame"
            } else {
                ""
            }
        );
        ui.set_resume_path(resume.path.into());
        ui.set_resume_show(resume.show.unwrap_or_default().into());
        ui.set_resume_title(resume.title.into());
        ui.set_resume_progress(resume.fraction);
        ui.set_resume_left(format::left(resume.seconds_left).into());
        self.resume_left = Some(resume.seconds_left);
        match &resume.still {
            Some(still) => {
                show_backdrop(gpu, still);
                // With no backdrop the way in's glass has nothing to bend and
                // gives itself a ground, so this is set only where one was
                // drawn.
                ui.set_backdrop(true);
            }
            None => light_with_the_mark(&self.app.worker),
        }
    }
}

fn show_backdrop(gpu: &mut Gpu, still: &Still) {
    gpu.pipeline
        .set_backdrop(&gpu.gl, &still.rgba, still.width, still.height);
}

/// Find what the empty window can offer to pick back up. A stat per played
/// file and one ffmpeg run, so on the worker.
pub(super) fn look_for_resume(worker: &Worker) {
    worker.submit(|_mpv| Some(Completion::Resume(dbm_library::resume::last_watched())));
}

/// Draw the mark for the backdrop, on the worker.
///
/// Parsing and rasterising an SVG only takes a millisecond, but the frame
/// path must never stall, and the rule is easiest kept without exceptions.
fn light_with_the_mark(worker: &Worker) {
    worker.submit(|_mpv| dbm_library::mark::still().map(Completion::Mark));
}
