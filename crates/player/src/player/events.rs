//! Step 2 of a run: draining mpv's event queue.
//!
//! Only ever on the UI thread, which keeps `mpv_wait_event`, which isn't
//! safe to call concurrently, on one thread.

use dbm_mpv::Event;
use slint::ComponentHandle;

use super::Player;
use crate::interface::{format, sync, unplayable, warn};
use crate::playback::commands::Reply;
use crate::playback::session;
use crate::{MainWindow, Playback};

impl Player {
    /// Drain mpv's events into the mirrored state, and push out what moved.
    /// Returns whether any of it did.
    pub(super) fn take_events(&mut self, ui: &MainWindow) -> bool {
        let mut moved = false;
        let mut replies = Vec::new();
        let mut notices = Vec::new();
        let mut played_out = None;
        // SAFETY: the one place the queue is read, and only on the UI thread.
        while let Some(event) = unsafe { self.app.mpv.poll_event() } {
            match &event {
                // Not state: each belongs to whoever sent the command, so they
                // are routed once the state is up to date.
                Event::Reply { id, failed } => {
                    let reply = Reply::of(*id);
                    // A move or a list that went nowhere opens nothing, which
                    // is settled here, in its place among the events.
                    if *failed && matches!(reply, Some(Reply::Move | Reply::LoadList)) {
                        session::settled();
                    }
                    replies.push(reply);
                    continue;
                }
                // A file that stopped because it couldn't be played, said in
                // mpv's own words, which beat anything invented here. Which
                // file it was is asked of the list, by the entry's id: the
                // player's own `path` is the file before it, or nothing.
                Event::EndFile {
                    failure: Some(reason),
                    entry,
                    ..
                } => {
                    let path = self
                        .state
                        .playlist
                        .iter()
                        .find(|e| e.id == *entry)
                        .map(|e| e.filename.clone());
                    notices.push(unplayable::could_not_play(reason, path.as_ref()));
                    // So its row keeps saying so after the notice has gone.
                    self.state.found.failed.insert(*entry, reason.clone());
                }
                Event::EndFile {
                    eof: true, entry, ..
                } => played_out = Some(*entry),
                // Saving the resume position waits out a file opening.
                Event::StartFile => session::opening(),
                Event::FileLoaded => session::settled(),
                _ => {}
            }
            // The watchdog reads the same stream for the device list and the
            // audio track, neither of which belongs in `PlayerState`.
            self.app.audio.on_event(&event);
            moved |= self.state.apply(&event);
        }

        let failed = !notices.is_empty();
        if failed {
            self.opening_failed(ui);
        }
        for notice in notices {
            warn(ui, notice);
        }
        if moved {
            sync::push_scalars(ui, &self.state);
        }
        if let Some(entry) = played_out {
            self.played_out(entry);
        }
        self.follow_arrival(ui);
        self.follow_opening(ui);
        for reply in replies {
            match reply {
                Some(Reply::Scrub) => self.app.scrubber.on_reply(&self.app.mpv),
                Some(Reply::LoadList) => self.list_answered(),
                Some(Reply::Move | Reply::Ignored) | None => {}
            }
        }
        // After the replies, so a list's own start can't undo it.
        if failed {
            self.hold_after_failure();
        }
        moved
    }

    /// Autoplay moving on from a file that played to its end: the next one
    /// starts over, with its saved place offered for a while.
    fn played_out(&self, entry: i64) {
        let playlist = &self.state.playlist;
        let next = playlist
            .iter()
            .position(|e| e.id == entry)
            .and_then(|row| playlist.get(row + 1))
            .map(|e| e.filename.clone());
        if let Some(next) = next {
            self.app.arrival.played_out(&self.app.mpv, &next);
        }
    }

    /// Arriving at a file with a saved place, and the offers in the corner
    /// that come with it, then the skips, which wait for the corner to clear.
    fn follow_arrival(&mut self, ui: &MainWindow) {
        let playback = ui.global::<Playback>();
        let (path, time_pos, paused) = (
            self.state.path.as_ref(),
            self.state.time_pos,
            self.state.paused,
        );
        // A file held on its saved place, for the centre pill to ask about.
        self.app.arrival.follow(
            &self.app.mpv,
            self.state.playlist_pos,
            paused,
            self.state.loads,
            path,
        );
        let held = self.app.arrival.asking(path, time_pos, paused);
        playback.set_arriving(held.is_some());
        if let Some(from) = held {
            playback.set_arrival_from(format::time(from).into());
        }
        let offered = self.app.arrival.offered(path, time_pos);
        playback.set_resume_offer(offered.map(format::time).unwrap_or_default().into());
        let back = self.app.retrace.offered(path, time_pos);
        playback.set_back_offer(back.map(format::time).unwrap_or_default().into());
        let corner_free =
            held.is_none() && offered.is_none() && back.is_none() && !self.app.arrival.holding();
        self.skip(corner_free);
        playback.set_credits_count(self.skips.counted(&self.state));
    }
}
