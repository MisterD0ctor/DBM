//! What each kind of finished background work does to the player.
//!
//! Every variant of [`Completion`] is answered here, in one `match`, so
//! adding a kind of background job is a compile error until its result has
//! somewhere to go (see `worker`).

use super::{say, warn, Driver, Gpu};
use crate::interface::format;
use crate::lang::{fill, tr};
use crate::library::opensubtitles::account::Account;
use crate::library::opensubtitles::{self, Fetched, Found, Hit};
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
                // A read the lists have since moved past is thrown away, so the
                // newer one is asked for now: the poll in the frame has already
                // run, and a paused film may draw no other.
                self.lists.poll(&self.app.worker, &self.player);
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
                if list.ask {
                    self.app
                        .arrival
                        .open_list(&self.app.mpv, list.first.clone(), list.saved);
                }
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
                // An end reached before its atlas was built had no frame to be
                // lit with, and now may.
                if self.end_lit != self.player.path {
                    self.end_light_asked = None;
                }
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
                if let Some(pieces) = heard.pieces {
                    match heard.end {
                        // A title sequence is heard as the one piece.
                        End::Opening => {
                            self.player.intro_heard =
                                pieces.first().map(|&span| (heard.path, span));
                        }
                        End::Closing => self.player.credits_heard = Some((heard.path, pieces)),
                    }
                    // Pushed here, since a paused film reports nothing else
                    // that would.
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
            Completion::SignedIn(result) => {
                self.signed_in(ui, result);
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
            Completion::EndLight { from, still } => {
                if let (Some(still), true) =
                    (still, self.player.path.as_deref() == Some(from.as_str()))
                {
                    gpu.pipeline.set_backdrop(&gpu.gl, &still);
                    self.end_lit = Some(from);
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
    /// Every outcome is said: the row has read "Listening" for seconds, and
    /// the two answers that change nothing on screen (already in step, and no
    /// answer) would otherwise look like a button that did nothing.
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
        // Saved now, since the timing is this file's alone, and moving on
        // before the next checkpoint would lose it.
        crate::playback::session::checkpoint(&self.app.mpv);
        // What stood before, for the row to offer back.
        ui.set_sync_was_delay(was.0 as f32);
        ui.set_sync_was_speed(was.1 as f32);
        ui.set_sync_state(2);
        say(
            ui,
            if same_speed {
                // Two sentences rather than one with a word swapped in, since
                // the word doesn't sit in the same place in every language.
                fill(
                    if moved < 0.0 {
                        tr("Subtitles moved {} s earlier")
                    } else {
                        tr("Subtitles moved {} s later")
                    },
                    &[&format!("{:.1}", moved.abs())],
                )
            } else {
                // No figure, since at another speed the subtitles have moved
                // by a different amount at every point in the film.
                tr("Subtitles matched to the film's speed").into()
            },
        );
    }

    /// Fill the search's page with what OpenSubtitles found, or say why not.
    fn found(&self, ui: &MainWindow, found: Found) {
        show_account(ui, found.account.as_ref());
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
        show_account(ui, fetched.account.as_ref());
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
            Err(why) => {
                // Kept to be fetched once the person has signed in, which is
                // the way past it, so the menu goes straight to where that is
                // done, if it is still on the page that asked.
                if why.wants_account() {
                    let row = self.row_of(&fetched.hit);
                    *self.app.waiting.borrow_mut() = Some((fetched.path, fetched.hit));
                    if ui.invoke_offer_sign_in(row, why.sentence().into()) {
                        return;
                    }
                }
                warn(ui, why.sentence());
            }
        }
    }

    /// Go on with the download that asked for an account, now there is one,
    /// or say on the sign-in page why there isn't.
    fn signed_in(&self, ui: &MainWindow, result: Result<Account, opensubtitles::Failure>) {
        ui.set_signing_in(false);
        match result {
            Ok(account) => {
                show_account(ui, Some(&account));
                ui.set_sign_in_said(Default::default());
                // Back to the search, on the row whose download goes on, if
                // the sign-in's page is still the one showing.
                if ui.get_menu_open() && ui.get_account_open() {
                    ui.invoke_close_account();
                }
                let waiting = self.app.waiting.borrow_mut().take();
                let Some((path, hit)) = waiting else {
                    return;
                };
                if self.player.path.as_deref() != Some(path.as_str()) {
                    return;
                }
                ui.set_find_fetching(self.row_of(&hit));
                opensubtitles::fetch(path, hit, self.app.worker.reporter());
            }
            Err(why) => ui.set_sign_in_said(why.sentence().into()),
        }
    }

    /// Where `hit` is in the search's results, or -1 once a search since has
    /// replaced them.
    fn row_of(&self, hit: &Hit) -> i32 {
        let found = self.app.found.borrow();
        found
            .iter()
            .position(|h| h == hit)
            .map_or(-1, |row| row as i32)
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
                // With no backdrop the way in's glass has nothing to bend and
                // gives itself a ground, so this is set only where one was
                // drawn.
                ui.set_backdrop(true);
            }
            None => light_with_the_mark(&self.app.worker),
        }
    }
}

/// One result of a search, as its row says it: the release it was timed
/// against, and beside it the language and whatever sets it apart.
/// Who is signed in to OpenSubtitles, on the search's page.
fn show_account(ui: &MainWindow, account: Option<&Account>) {
    ui.set_find_account(account.map(|a| a.name.as_str()).unwrap_or_default().into());
    ui.set_find_left(
        account
            .and_then(|a| a.left)
            .map_or(-1, |left| left.clamp(0, i32::MAX as i64) as i32),
    );
}

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
/// Parsing and rasterising an SVG only takes a millisecond, but the frame
/// path must never stall, and the rule is easiest kept without exceptions.
fn light_with_the_mark(worker: &Worker) {
    worker.submit(|_mpv| crate::gpu::mark::still().map(Completion::Mark));
}
