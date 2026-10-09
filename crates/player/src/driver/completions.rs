//! What each kind of finished background work does to the player.
//!
//! Every variant of [`Completion`] is answered here, in one `match`, so
//! adding a kind of background job is a compile error until its result has
//! somewhere to go — see `worker`.

use super::{say, Driver, Gpu};
use crate::interface::format;
use crate::library::preview::Sprite;
use crate::library::resume::Resume;
use crate::library::subsync::{Failure, Outcome};
use crate::playback::commands;
use crate::worker::{Completion, Worker};
use crate::MainWindow;

impl Driver {
    /// Take delivery of one finished job. Returns whether the playlist panel
    /// needs rebuilding, which the caller does once however many asked.
    pub(super) fn receive(
        &mut self,
        ui: &MainWindow,
        gpu: &mut Gpu,
        completion: Completion,
    ) -> bool {
        match completion {
            Completion::Lists(lists) => {
                self.lists.apply(ui, &mut self.player, lists);
                // Here because this is where the playlist's paths become
                // known. The scan compares them with the last request and
                // does nothing when only the tracks moved.
                let paths: Vec<String> = self
                    .player
                    .playlist
                    .iter()
                    .map(|e| e.filename.clone())
                    .collect();
                self.scan.request(&paths, &self.app.worker);
                // What was found beside the old list is put away rather than
                // shown around another show.
                if self.neighbours.request(&paths, &self.app.worker)
                    && !self.player.beside.is_empty()
                {
                    self.player.beside.clear();
                    return true;
                }
                false
            }
            Completion::Opened(Ok(list)) => {
                eprintln!(
                    "dbm: opened {} file(s), starting at {}",
                    list.count, list.start
                );
                self.loading_list = true;
                commands::load_list(&self.app.mpv, &list.m3u, list.start);
                false
            }
            Completion::Opened(Err(e)) => {
                eprintln!("dbm: cannot open: {e}");
                // Nothing was found to load, so no file is coming.
                ui.set_opening(false);
                say(ui, e);
                false
            }
            Completion::Notice(text) => {
                say(ui, text);
                false
            }
            Completion::Probed(found) => self.player.apply_probed(found),
            Completion::Beside { paths, seasons } => {
                // For the list it was looked for beside, or for nothing.
                let playing = self.player.playlist.iter().map(|e| e.filename.as_str());
                if playing.eq(paths.iter().map(String::as_str)) && self.player.beside != seasons {
                    self.player.beside = seasons;
                    return true;
                }
                false
            }
            Completion::Resume(Some(resume)) => {
                self.offer_resume(ui, gpu, resume);
                false
            }
            // Nothing to continue, which on a fresh install is every time:
            // the window is lit by the player's own mark instead.
            Completion::Resume(None) => {
                light_with_the_mark(&self.app.worker);
                false
            }
            Completion::Preview(sprite) => {
                self.show_atlas(ui, sprite);
                false
            }
            Completion::Mark(still) => {
                gpu.pipeline.set_backdrop(&gpu.gl, &still);
                ui.set_backdrop(true);
                false
            }
            Completion::Synced(outcome) => {
                self.synced(ui, outcome);
                false
            }
            Completion::Onward { from, to } => {
                // An answer for a file that has since been replaced is dropped.
                if self.player.path.as_deref() == Some(from.as_str()) {
                    if let Some(onward) = to {
                        ui.set_onward_path(onward.path.as_str().into());
                        ui.set_onward_label(onward.label.as_str().into());
                        ui.set_onward_season(onward.season);
                    }
                }
                false
            }
        }
    }

    /// Play the subtitles the way a sync found, and say what came of it.
    ///
    /// Every outcome is said. The row that asked has been reading
    /// "Listening" for some seconds, and the two answers that change nothing
    /// on screen — already in step, and no answer at all — would otherwise
    /// be indistinguishable from a button that did nothing.
    fn synced(&self, ui: &MainWindow, outcome: Outcome) {
        ui.set_sync_state(0);
        // For the film and the track it listened to, or for nothing: the
        // timing belongs to that pair and would be wrong for any other.
        if self.player.path.as_deref() != Some(outcome.path.as_str())
            || self.player.sid != outcome.sid
        {
            return;
        }
        let fit = match outcome.result {
            Ok(fit) => fit,
            Err(why) => {
                say(ui, unsynced(why).into());
                return;
            }
        };
        let was = (
            self.player.sub_delay,
            if self.player.sub_speed > 0.0 {
                self.player.sub_speed
            } else {
                1.0
            },
        );
        let moved = fit.delay - was.0;
        let same_speed = (fit.speed - was.1).abs() < 1e-4;
        if same_speed && moved.abs() < 0.05 {
            say(ui, "Subtitles already in sync".into());
            return;
        }
        commands::set_sub_delay(&self.app.mpv, fit.delay);
        commands::set_sub_speed(&self.app.mpv, fit.speed);
        // What stood before, for the row to offer back.
        ui.set_sync_was_delay(was.0 as f32);
        ui.set_sync_was_speed(was.1 as f32);
        ui.set_sync_state(2);
        say(
            ui,
            if same_speed {
                format!(
                    "Subtitles moved {:.1} s {}",
                    moved.abs(),
                    if moved < 0.0 { "earlier" } else { "later" }
                )
            } else {
                // The delay alone would be a number that means nothing: at
                // another speed the subtitles have moved by a different
                // amount at every point in the film.
                "Subtitles matched to the film's speed".into()
            },
        );
    }

    /// Hand the seek preview a finished atlas.
    fn show_atlas(&self, ui: &MainWindow, sprite: Sprite) {
        let image = match slint::Image::load_from_path(&sprite.path) {
            Ok(image) => image,
            Err(e) => {
                eprintln!("dbm: preview atlas unreadable: {e:?}");
                return;
            }
        };
        eprintln!(
            "dbm: preview atlas {g}x{g} tiles of {}x{}",
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
        let (_, x, y) = self.app.preview.again();
        ui.set_preview_clip_x(x);
        ui.set_preview_clip_y(y);
    }

    /// Put the film left unfinished on the way in, and its frame behind it.
    fn offer_resume(&self, ui: &MainWindow, gpu: &mut Gpu, resume: Resume) {
        eprintln!(
            "dbm: last unfinished {} at {:.0}%{}",
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
        match &resume.still {
            Some(still) => {
                gpu.pipeline.set_backdrop(&gpu.gl, still);
                // The interface needs to know, because with no backdrop and no
                // film there is no picture for the way in's glass to bend and
                // it gives itself a ground instead. Set only where one was
                // actually drawn.
                ui.set_backdrop(true);
            }
            None => light_with_the_mark(&self.app.worker),
        }
    }
}

/// Why a sync changed nothing, for the person who asked for it.
fn unsynced(why: Failure) -> &'static str {
    match why {
        Failure::NoSubtitles => "No subtitles to sync",
        Failure::Pictures => "Only text subtitles can be synced",
        Failure::NoFfmpeg => "Syncing subtitles needs ffmpeg",
        Failure::Unread => "Could not read these subtitles",
        Failure::Silent => "Could not read this film's audio",
        Failure::Unsure => "Could not match these subtitles to the speech",
    }
}

/// Draw the mark for the backdrop, on the worker.
///
/// Parsing an SVG and rasterising it is a millisecond of arithmetic rather
/// than a wait, but the frame path's rule is that nothing it does can stall,
/// and the cheapest way to keep a rule is not to argue about its edges.
fn light_with_the_mark(worker: &Worker) {
    worker.submit(|_mpv| crate::gpu::mark::still().map(Completion::Mark));
}
