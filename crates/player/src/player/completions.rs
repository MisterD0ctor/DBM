//! What each kind of finished background work does to the player.
//!
//! Every variant of [`Completion`] is answered here, in one `match`, so
//! adding a kind of background job is a compile error until its result has
//! somewhere to go (see `worker`). Each arm hands its result to whatever owns
//! what it changes: the search to `interface::find`, a sync to
//! `interface::subsync`, and what the player holds itself to a method below.

use dbm_library::onward::Onward;
use dbm_library::preview::{Sprite, Still};
use dbm_library::resume::Resume;
use dbm_library::shelf::{Beside, Shelf};
use dbm_library::titles::{self, End};
use dbm_mpv::MediaPath;
use slint::ComponentHandle;

use super::{EndLight, Player};
use crate::interface::{format, say, subsync, warn};
use crate::playback::commands;
use crate::playback::state::Lists;
use crate::worker::{Completion, Worker};
use crate::{MainWindow, Playback};

impl Player {
    /// Take delivery of one finished job. Returns whether the playlist needs
    /// laying out again, which the caller asks for once however many did.
    pub(super) fn receive(&mut self, ui: &MainWindow, completion: Completion) -> bool {
        let playing = self.state.path.as_ref();
        match completion {
            // The three that can move the playlist's rows.
            Completion::Lists(lists) => return self.take_lists(ui, lists),
            Completion::Probed(found) => return self.state.apply_probed(found),
            Completion::Beside { paths, seasons } => return self.take_beside(&paths, seasons),

            Completion::Shelved(shelf) => self.take_shelf(ui, shelf),
            Completion::Opened(Ok(opened)) => self.load(opened.list, opened.ask),
            Completion::Opened(Err(e)) => self.not_opened(ui, e),
            Completion::Notice(text) => say(ui, text),
            Completion::Warning(text) => warn(ui, text),
            Completion::Resume(Some(resume)) => self.offer_resume(ui, resume),
            // Nothing to continue, which on a fresh install is every time:
            // the window is lit by the player's own mark instead.
            Completion::Resume(None) => light_with_the_mark(&self.app.worker),
            Completion::Preview { sprite, pixels } => self.show_atlas(ui, sprite, pixels),
            Completion::Mark(still) => {
                self.backdrop = Some(still);
                ui.set_backdrop(true);
            }
            Completion::Titles(heard) => self.take_titles(ui, heard),
            Completion::Bars(found) => self.bars.take(found, playing),
            Completion::Synced(outcome) => {
                subsync::answer(ui, &self.app.mpv, &self.state, outcome);
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
            Completion::Kept(kept) => {
                if let Some(files) = kept.if_playing(playing) {
                    self.put_back(files);
                }
            }
            Completion::EndLight(light) => {
                if let Some(still) = light.if_playing(playing) {
                    self.light_the_end_with(still);
                }
            }
            Completion::Onward(onward) => {
                if let Some(to) = onward.if_playing(playing) {
                    offer_onward(ui, to);
                }
            }
        }
        false
    }

    /// The track list, the playlist and the chapters, read again: put in the
    /// mirrored state, and the work that follows from the playlist's paths
    /// asked for.
    fn take_lists(&mut self, ui: &MainWindow, lists: Lists) -> bool {
        let mut moved = self.feeds.lists.apply(ui, &mut self.state, lists);
        if moved {
            self.warm.ask(crate::Panel::Subtitles);
            self.warm.ask(crate::Panel::Audio);
        }
        // A read the lists have since moved past is thrown away, so the
        // newer one is asked for now: this run's poll has already been.
        self.feeds.lists.poll(&self.app.worker, &self.state);
        // What is saved for each entry, for choosing one on purpose.
        self.app.arrival.learn(
            (0..self.state.playlist.len())
                .map(|row| {
                    (
                        self.state.playlist[row].filename.clone(),
                        self.state.known_progress(row),
                    )
                })
                .collect(),
        );
        // Here because this is where the playlist's paths become
        // known. The scan compares them with the last request and
        // does nothing when only the tracks moved.
        let paths = self.state.playlist_paths();
        self.feeds.scan.request(&paths, &self.app.worker);
        // What was found beside the old list is put away rather than
        // shown around another show.
        if self.feeds.neighbours.request(&paths, &self.app.worker)
            && !self.state.found.beside.is_empty()
        {
            self.state.found.beside.clear();
            moved = true;
        }
        moved
    }

    /// The seasons found beside the playlist, for the list it was looked for
    /// beside, or for nothing.
    fn take_beside(&mut self, paths: &[MediaPath], seasons: Vec<Beside>) -> bool {
        let playing = self.state.playlist.iter().map(|e| &e.filename);
        if playing.eq(paths) && self.state.found.beside != seasons {
            self.state.found.beside = seasons;
            return true;
        }
        false
    }

    fn take_shelf(&mut self, ui: &MainWindow, shelf: Shelf) {
        if self.panel.show(ui, shelf) {
            self.warm.ask(crate::Panel::Playlist);
        }
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
                self.state.found.intro_heard = pieces.first().map(|&span| (heard.path, span));
            }
            End::Closing => self.state.found.credits_heard = Some((heard.path, pieces)),
        }
        // Pushed here, since a paused film reports nothing else that would.
        crate::interface::sync::push_scalars(ui, &self.state);
    }

    /// Subtitles fetched for this film before, back in the list as the
    /// film's own tracks are, without taking the place of whichever one it
    /// opened on.
    fn put_back(&self, files: Vec<(std::path::PathBuf, String)>) {
        for (file, language) in files {
            commands::add_subtitle(&self.app.mpv, &file, &language, false);
        }
    }

    /// Light the end of the file playing with its last frame. With no frame
    /// of its own, the end is lit by the mark, as a fresh install's empty
    /// window is: it still dims a last frame that may be a white one, and the
    /// season's line still has that to read on.
    fn light_the_end_with(&mut self, still: Option<Still>) {
        let (still, light) = match still {
            Some(still) => (Some(still), EndLight::Frame),
            None => (dbm_library::mark::still(), EndLight::Mark),
        };
        if let Some(still) = still {
            self.backdrop = Some(still);
            self.per_file.end_light = Some(light);
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
        let playback = ui.global::<Playback>();
        let image = slint::Image::from_rgba8(pixels);
        log::info!(
            "preview atlas {g}x{g} tiles of {}x{}",
            sprite.tile_w,
            sprite.tile_h,
            g = sprite.grid
        );
        playback.set_preview_sprite(image);
        playback.set_preview_tile_w(sprite.tile_w as i32);
        playback.set_preview_tile_h(sprite.tile_h as i32);
        self.app.preview.set_sprite(Some(sprite));
        // The pointer may already be on the timeline, in which case it is
        // showing a timestamp and an empty frame.
        let answer = self.app.preview.again();
        playback.set_preview_clip_x(answer.x);
        playback.set_preview_clip_y(answer.y);
        // An end reached before its atlas was built had no frame of its
        // own to be lit with, and now may.
        if self.per_file.end_light != Some(EndLight::Frame) {
            self.per_file.end_light_asked = false;
        }
    }

    /// Put the film left unfinished on the way in, and its frame behind it.
    fn offer_resume(&mut self, ui: &MainWindow, resume: Resume) {
        let playback = ui.global::<Playback>();
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
        playback.set_resume_path(resume.path.as_str().into());
        playback.set_resume_show(resume.show.unwrap_or_default().into());
        playback.set_resume_title(resume.title.into());
        playback.set_resume_progress(resume.fraction);
        playback.set_resume_left(format::left(resume.seconds_left).into());
        self.resume_left = Some(resume.seconds_left);
        match resume.still {
            Some(still) => {
                self.backdrop = Some(still);
                // With no backdrop the way in's glass has nothing to bend and
                // gives itself a ground, so this is set only where one was
                // drawn.
                ui.set_backdrop(true);
            }
            None => light_with_the_mark(&self.app.worker),
        }
    }
}

/// What to go on to after the file playing, where there is anywhere.
fn offer_onward(ui: &MainWindow, to: Option<Onward>) {
    if let Some(onward) = to {
        let playback = ui.global::<Playback>();
        playback.set_onward_path(onward.path.to_string_lossy().as_ref().into());
        playback.set_onward_label(onward.label.as_str().into());
        playback.set_onward_season(onward.season);
    }
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
