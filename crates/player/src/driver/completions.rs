//! What each kind of finished background work does to the player.
//!
//! Every variant of [`Completion`] is answered here, in one `match`, so
//! adding a kind of background job is a compile error until its result has
//! somewhere to go — see `worker`.

use super::{say, Driver, Gpu};
use crate::interface::format;
use crate::library::preview::Sprite;
use crate::library::resume::Resume;
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

/// Draw the mark for the backdrop, on the worker.
///
/// Parsing an SVG and rasterising it is a millisecond of arithmetic rather
/// than a wait, but the frame path's rule is that nothing it does can stall,
/// and the cheapest way to keep a rule is not to argue about its edges.
fn light_with_the_mark(worker: &Worker) {
    worker.submit(|_mpv| crate::gpu::mark::still().map(Completion::Mark));
}
