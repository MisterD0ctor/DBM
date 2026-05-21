//! `PlayerState` — the single reactive state struct.
//!
//! One `RwSignal` per observed mpv property + the playlist/tracks lists +
//! a few UI-local bits (fullscreen, menu visibility). Provided once via
//! `provide_context` in [`App`]; every component reads the signals it
//! cares about and writes them only through user actions (which then call
//! commands; the event stream brings the truth back).

use std::collections::HashMap;

use leptos::prelude::*;
use shared::{PlayerSnapshot, PlaylistEntry, PreviewReady, Track, WatchProgress};

#[derive(Clone, Copy)]
pub struct PlayerState {
    // mpv-observed properties
    pub time_pos: RwSignal<f64>,
    pub duration: RwSignal<f64>,
    pub percent_pos: RwSignal<f64>,
    pub paused: RwSignal<bool>,
    pub muted: RwSignal<bool>,
    pub volume: RwSignal<f64>,
    pub panscan: RwSignal<f64>,
    pub filename: RwSignal<Option<String>>,
    pub path: RwSignal<Option<String>>,
    pub sid: RwSignal<Option<String>>,
    pub aid: RwSignal<Option<String>>,
    pub sub_visibility: RwSignal<bool>,
    pub border_background: RwSignal<String>,
    pub eof_reached: RwSignal<bool>,
    pub playlist_pos: RwSignal<Option<i64>>,
    pub playlist_count: RwSignal<i64>,
    pub playlist: RwSignal<Vec<PlaylistEntry>>,
    pub tracks: RwSignal<Vec<Track>>,
    /// path → saved resume position. Populated after playlist changes; drives
    /// the partial progress bar on each playlist menu item.
    pub playlist_progress: RwSignal<HashMap<String, WatchProgress>>,
    /// Sprite atlas for the currently-loaded video. `None` means no preview
    /// is available (yet); when ffmpeg finishes generation, the
    /// `preview://ready` listener fills it in.
    pub preview_sprite: RwSignal<Option<PreviewReady>>,

    // UI-local (not mirrored from mpv)
    pub fullscreen: RwSignal<bool>,
    /// Snapshot of the OS window's maximized state captured *at the moment we
    /// entered fullscreen*. The toolbar fullscreen button cycles
    /// normal → maximized → fullscreen → previous, so on exit we need to know
    /// whether to unmaximize the window or leave it maximized.
    pub pre_fullscreen_maximized: RwSignal<bool>,
    /// `true` when the controls panel + cursor should be visible. The
    /// overlay module toggles this on mouse activity. (Wired in a later step.)
    #[allow(dead_code)]
    pub controls_visible: RwSignal<bool>,
}

impl PlayerState {
    pub fn new() -> Self {
        Self {
            time_pos: RwSignal::new(0.0),
            duration: RwSignal::new(0.0),
            percent_pos: RwSignal::new(0.0),
            paused: RwSignal::new(true),
            muted: RwSignal::new(false),
            volume: RwSignal::new(100.0),
            panscan: RwSignal::new(0.0),
            filename: RwSignal::new(None),
            path: RwSignal::new(None),
            sid: RwSignal::new(None),
            aid: RwSignal::new(None),
            sub_visibility: RwSignal::new(true),
            border_background: RwSignal::new(String::new()),
            eof_reached: RwSignal::new(false),
            playlist_pos: RwSignal::new(None),
            playlist_count: RwSignal::new(0),
            playlist: RwSignal::new(Vec::new()),
            tracks: RwSignal::new(Vec::new()),
            playlist_progress: RwSignal::new(HashMap::new()),
            preview_sprite: RwSignal::new(None),
            fullscreen: RwSignal::new(false),
            pre_fullscreen_maximized: RwSignal::new(false),
            controls_visible: RwSignal::new(true),
        }
    }

    /// Populate every mpv-driven signal from a backend `snapshot()` response.
    /// UI-local signals are not touched.
    pub fn hydrate(&self, snap: PlayerSnapshot) {
        self.time_pos.set(snap.time_pos.unwrap_or(0.0));
        self.duration.set(snap.duration.unwrap_or(0.0));
        self.percent_pos.set(snap.percent_pos.unwrap_or(0.0));
        self.paused.set(snap.paused);
        self.muted.set(snap.muted);
        self.volume.set(snap.volume);
        self.panscan.set(snap.panscan);
        self.filename.set(snap.filename);
        self.path.set(snap.path);
        self.sid.set(snap.sid);
        self.aid.set(snap.aid);
        self.sub_visibility.set(snap.sub_visibility);
        self.border_background.set(snap.border_background);
        self.eof_reached.set(snap.eof_reached);
        self.playlist_pos.set(snap.playlist_pos);
        self.playlist_count.set(snap.playlist.len() as i64);
        self.playlist.set(snap.playlist);
        self.tracks.set(snap.tracks);
    }

    /// `true` if the playlist is loaded and we're on its last entry.
    pub fn is_last_video(&self) -> bool {
        let pos = self.playlist_pos.get().unwrap_or(0);
        let count = self.playlist_count.get();
        count > 0 && pos >= count - 1
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new()
    }
}
