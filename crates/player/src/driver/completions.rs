//! What each kind of finished background work does to the player.
//!
//! Every variant of [`Completion`] is answered here, in one `match`, so
//! adding a kind of background job is a compile error until its result has
//! somewhere to go — see `worker`.

use super::{say, warn, Driver, Gpu};
use crate::interface::format;
use crate::lang::{fill, tr};
use crate::library::opensubtitles::{Fetched, Found, Hit};
use crate::library::preview::Atlas;
use crate::library::resume::Resume;
use crate::library::subsync::{Failure, Outcome};
use crate::library::titles::End;
use crate::playback::commands;
use crate::worker::{Completion, Worker};
use crate::{FoundItem, MainWindow};
use slint::{ModelRc, VecModel};

impl Driver {
    /// Take delivery of one finished job. Returns whether the playlist needs
    /// laying out again, which the caller asks for once however many did.
    pub(super) fn receive(
        &mut self,
        ui: &MainWindow,
        gpu: &mut Gpu,
        completion: Completion,
    ) -> bool {
        match completion {
            Completion::Lists(lists) => {
                let mut moved = self.lists.apply(ui, &mut self.player, lists);
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
                    moved = true;
                }
                moved
            }
            Completion::Shelved(shelf) => {
                self.panel.show(ui, shelf);
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
                warn(ui, e);
                false
            }
            Completion::Notice(text) => {
                say(ui, text);
                false
            }
            Completion::Warning(text) => {
                warn(ui, text);
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
            Completion::Preview(atlas) => {
                self.show_atlas(ui, atlas);
                false
            }
            Completion::Mark(still) => {
                gpu.pipeline.set_backdrop(&gpu.gl, &still);
                ui.set_backdrop(true);
                false
            }
            Completion::Titles(heard) => {
                // Kept with its path, so an answer for a file that has since
                // been replaced is simply about something else.
                if let Some(span) = heard.span {
                    match heard.end {
                        End::Opening => self.player.intro_heard = Some((heard.path, span)),
                        End::Closing => self.player.credits_heard = Some((heard.path, span.0)),
                    }
                    // Nothing else may have moved — a paused film reports
                    // nothing — and the pill should not wait for it to.
                    crate::interface::sync::push_scalars(ui, &self.player);
                }
                false
            }
            Completion::Synced(outcome) => {
                self.synced(ui, outcome);
                false
            }
            Completion::Found(found) => {
                self.found(ui, found);
                false
            }
            Completion::Fetched(fetched) => {
                self.fetched(ui, fetched);
                false
            }
            Completion::Kept { path, files } => {
                // Back in the list as the film's own tracks are, without
                // taking the place of whichever one it opened on.
                if self.player.path.as_deref() == Some(path.as_str()) {
                    for (file, language) in files {
                        commands::add_subtitle(&self.app.mpv, &file, &language, false);
                    }
                }
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
                warn(ui, unsynced(why).into());
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
            say(ui, tr("Subtitles already in sync").into());
            return;
        }
        commands::set_sub_delay(&self.app.mpv, fit.delay);
        commands::set_sub_speed(&self.app.mpv, fit.speed);
        // Kept now: the timing is this file's alone, and the next file
        // along would otherwise take it away within the ten seconds before
        // the position is next saved.
        crate::playback::session::checkpoint(&self.app.mpv);
        // What stood before, for the row to offer back.
        ui.set_sync_was_delay(was.0 as f32);
        ui.set_sync_was_speed(was.1 as f32);
        ui.set_sync_state(2);
        say(
            ui,
            if same_speed {
                // Two sentences rather than one with a word swapped in: the
                // word does not sit in the same place in every language.
                fill(
                    if moved < 0.0 {
                        tr("Subtitles moved {} s earlier")
                    } else {
                        tr("Subtitles moved {} s later")
                    },
                    &[&format!("{:.1}", moved.abs())],
                )
            } else {
                // The delay alone would be a number that means nothing: at
                // another speed the subtitles have moved by a different
                // amount at every point in the film.
                tr("Subtitles matched to the film's speed").into()
            },
        );
    }

    /// Fill the search's page with what OpenSubtitles found, or say why not.
    fn found(&self, ui: &MainWindow, found: Found) {
        // For the film it was asked about, or for nothing.
        if self.player.path.as_deref() != Some(found.path.as_str()) {
            return;
        }
        match found.hits {
            Ok(hits) => {
                let rows: Vec<FoundItem> = hits.iter().map(found_row).collect();
                ui.set_find_hits(ModelRc::new(VecModel::from(rows)));
                ui.set_find_state(if hits.is_empty() { 3 } else { 2 });
                *self.app.found.borrow_mut() = hits;
            }
            Err(why) => {
                ui.set_find_said(why.sentence().into());
                ui.set_find_state(4);
            }
        }
    }

    /// Load a fetched subtitle and show it, or say why there is none.
    fn fetched(&self, ui: &MainWindow, fetched: Fetched) {
        ui.set_find_fetching(-1);
        if self.player.path.as_deref() != Some(fetched.path.as_str()) {
            return;
        }
        match fetched.result {
            Ok((file, language)) => {
                commands::add_subtitle(&self.app.mpv, &file, &language, true);
                // Back to the tracks, where it now stands lit: the answer
                // is the row it arrived as.
                ui.set_find_open(false);
            }
            Err(why) => warn(ui, why.sentence()),
        }
    }

    /// Hand the seek preview a finished atlas. Its picture comes decoded, so
    /// this is the cost of wrapping it and nothing more.
    fn show_atlas(&self, ui: &MainWindow, atlas: Atlas) {
        let Atlas { sprite, pixels } = atlas;
        let image = slint::Image::from_rgba8(pixels);
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
    fn offer_resume(&mut self, ui: &MainWindow, gpu: &mut Gpu, resume: Resume) {
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
        self.resume_left = Some(resume.seconds_left);
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

/// One result of a search, as its row says it: the release it was timed
/// against, and beside it the language and whatever sets it apart.
fn found_row(hit: &Hit) -> FoundItem {
    let mut detail = vec![crate::library::naming::language_name(&hit.language)];
    if hit.exact {
        detail.push(tr("Exact").to_string());
    }
    if hit.hearing_impaired {
        detail.push("SDH".into());
    }
    if hit.machine {
        detail.push(tr("Machine").to_string());
    }
    FoundItem {
        label: hit.release.as_str().into(),
        detail: detail.join(" · ").into(),
        exact: hit.exact,
    }
}

/// Why a sync changed nothing, for the person who asked for it.
fn unsynced(why: Failure) -> &'static str {
    match why {
        Failure::NoSubtitles => tr("No subtitles to sync"),
        Failure::Pictures => tr("Only text subtitles can be synced"),
        Failure::NoFfmpeg => tr("Syncing subtitles needs ffmpeg"),
        Failure::Unread => tr("Could not read these subtitles"),
        Failure::Silent => tr("Could not read this film's audio"),
        Failure::Unsure => tr("Could not match these subtitles to the speech"),
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
