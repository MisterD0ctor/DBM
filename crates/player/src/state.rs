//! Mirror of the mpv properties the UI cares about.
//!
//! The Tauri build round-trips these as JSON over an IPC channel and rebuilds
//! them into reactive signals on the other side. Here there is no boundary to
//! cross: mpv's typed event queue is drained on the UI thread and written
//! straight into this struct, which is then pushed into Slint properties.
//!
//! Everything is a plain field rather than a signal. Slint's own properties
//! provide the reactivity, and duplicating that here would mean two sources
//! of truth for the same value.

use crate::mpv::{Event, Value, FORMAT_DOUBLE, FORMAT_FLAG, FORMAT_STRING};
use crate::tracks::{PlaylistEntry, Track};
use std::ffi::c_int;

/// Properties to observe, and the format to receive each in.
///
/// List-shaped properties (`track-list`, `playlist`) are deliberately absent:
/// observing them as nodes would mean marshalling a tree across FFI on every
/// change. Their counts are observed instead, and the list itself is pulled
/// on demand when a count moves.
pub const OBSERVED: &[(&str, c_int)] = &[
    ("filename", FORMAT_STRING),
    ("media-title", FORMAT_STRING),
    ("path", FORMAT_STRING),
    ("duration", FORMAT_DOUBLE),
    ("time-pos", FORMAT_DOUBLE),
    ("percent-pos", FORMAT_DOUBLE),
    ("pause", FORMAT_FLAG),
    ("mute", FORMAT_FLAG),
    ("volume", FORMAT_DOUBLE),
    ("eof-reached", FORMAT_FLAG),
    ("playlist-pos", FORMAT_DOUBLE),
    ("playlist-count", FORMAT_DOUBLE),
    ("sub-visibility", FORMAT_FLAG),
    ("sub-delay", FORMAT_DOUBLE),
    ("sub-scale", FORMAT_DOUBLE),
    ("sub-pos", FORMAT_DOUBLE),
    ("panscan", FORMAT_DOUBLE),
    ("track-list/count", FORMAT_DOUBLE),
    // Selecting a different track does not change the list's length, so
    // these are what keep the `selected` flags from going stale.
    ("sid", FORMAT_STRING),
    ("aid", FORMAT_STRING),
    // Where mpv actually laid the video out. Observed rather than polled:
    // `mpv_get_property_string` blocks on mpv's core lock, and six of them
    // per frame on the render path cost 220ms each frame during a resize,
    // when that lock is contended. Off the event stream they are free.
    // The video's own size after aspect correction. Enough, with `panscan`,
    // to work out where a frame will land without waiting to be told.
    ("dwidth", FORMAT_DOUBLE),
    ("dheight", FORMAT_DOUBLE),
    ("osd-dimensions/w", FORMAT_DOUBLE),
    ("osd-dimensions/h", FORMAT_DOUBLE),
    ("osd-dimensions/ml", FORMAT_DOUBLE),
    ("osd-dimensions/mt", FORMAT_DOUBLE),
    ("osd-dimensions/mr", FORMAT_DOUBLE),
    ("osd-dimensions/mb", FORMAT_DOUBLE),
];

#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub filename: Option<String>,
    pub media_title: Option<String>,
    pub path: Option<String>,
    pub duration: f64,
    pub time_pos: f64,
    pub percent_pos: f64,
    pub paused: bool,
    pub muted: bool,
    pub volume: f64,
    pub eof_reached: bool,
    pub playlist_pos: i64,
    pub playlist_count: i64,
    pub sub_visibility: bool,
    pub sub_delay: f64,
    /// Size multiplier and vertical placement. Mirrored rather than read from
    /// the saved settings so the readout shows what mpv settled on, including
    /// any clamping it did of its own.
    pub sub_scale: f64,
    pub sub_pos: f64,
    /// 0 letterboxes to fit, 1 crops to fill.
    pub panscan: f64,
    pub track_count: i64,
    pub sid: Option<String>,
    pub aid: Option<String>,
    /// Bumped whenever the track list may have changed, so the lists below
    /// know to re-pull. Covers both the list mutating and the selection
    /// moving within it.
    pub tracks_generation: u64,
    /// mpv's own layout: OSD size and the margins around the video.
    pub osd: [f64; 6],
    /// Display size of the video itself, aspect already applied.
    pub dwidth: f64,
    pub dheight: f64,

    /// Read on the worker thread — see `worker`. Not written by `apply`.
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// Parallel to `playlist`, and only as long as it: read in the same job,
    /// applied in the same step.
    pub progress: Vec<crate::durations::Progress>,
}

impl PlayerState {
    /// Fold one event in. Returns whether anything the UI shows changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::Property { name, value } => self.apply_property(name, value),
            // A new file invalidates everything list-shaped.
            Event::FileLoaded => {
                self.tracks_generation = self.tracks_generation.wrapping_add(1);
                true
            }
            Event::StartFile | Event::EndFile | Event::Seek | Event::PlaybackRestart => true,
            // Filtered out in  before reaching here - command
            // completions are routed to whoever issued them.
            Event::CommandReply { .. } | Event::Shutdown => false,
        }
    }

    fn apply_property(&mut self, name: &str, value: &Value) -> bool {
        // `Unset` is normal rather than exceptional - `time-pos` has no value
        // until something is playing - so it clears the field instead of
        // being treated as an error.
        let num = match value {
            Value::Double(d) if d.is_finite() => Some(*d),
            _ => None,
        };
        let flag = matches!(value, Value::Flag(true));
        let text = match value {
            Value::Str(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };

        match name {
            "filename" => set(&mut self.filename, text),
            "media-title" => set(&mut self.media_title, text),
            "path" => set(&mut self.path, text),
            "duration" => set(&mut self.duration, num.unwrap_or(0.0)),
            "time-pos" => set(&mut self.time_pos, num.unwrap_or(0.0)),
            "percent-pos" => set(&mut self.percent_pos, num.unwrap_or(0.0)),
            "pause" => set(&mut self.paused, flag),
            "mute" => set(&mut self.muted, flag),
            "volume" => set(&mut self.volume, num.unwrap_or(0.0)),
            "eof-reached" => set(&mut self.eof_reached, flag),
            "playlist-pos" => set(&mut self.playlist_pos, num.unwrap_or(-1.0) as i64),
            "playlist-count" => {
                if set(&mut self.playlist_count, num.unwrap_or(0.0) as i64) {
                    self.tracks_generation = self.tracks_generation.wrapping_add(1);
                    return true;
                }
                false
            }
            "sub-visibility" => set(&mut self.sub_visibility, flag),
            "sub-delay" => set(&mut self.sub_delay, num.unwrap_or(0.0)),
            "sub-scale" => set(&mut self.sub_scale, num.unwrap_or(1.0)),
            "sub-pos" => set(&mut self.sub_pos, num.unwrap_or(100.0)),
            "panscan" => set(&mut self.panscan, num.unwrap_or(0.0)),
            "sid" => {
                if set(&mut self.sid, text) {
                    self.tracks_generation = self.tracks_generation.wrapping_add(1);
                    return true;
                }
                false
            }
            "aid" => {
                if set(&mut self.aid, text) {
                    self.tracks_generation = self.tracks_generation.wrapping_add(1);
                    return true;
                }
                false
            }
            "dwidth" => set(&mut self.dwidth, num.unwrap_or(0.0)),
            "dheight" => set(&mut self.dheight, num.unwrap_or(0.0)),
            "osd-dimensions/w" => set(&mut self.osd[0], num.unwrap_or(0.0)),
            "osd-dimensions/h" => set(&mut self.osd[1], num.unwrap_or(0.0)),
            "osd-dimensions/ml" => set(&mut self.osd[2], num.unwrap_or(0.0)),
            "osd-dimensions/mt" => set(&mut self.osd[3], num.unwrap_or(0.0)),
            "osd-dimensions/mr" => set(&mut self.osd[4], num.unwrap_or(0.0)),
            "osd-dimensions/mb" => set(&mut self.osd[5], num.unwrap_or(0.0)),
            "track-list/count" => {
                let n = num.unwrap_or(0.0) as i64;
                if set(&mut self.track_count, n) {
                    self.tracks_generation = self.tracks_generation.wrapping_add(1);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Fold in lists read on the worker thread.
    ///
    /// A result is discarded if the lists moved again while it was being
    /// read — the newer request is already on its way, and applying a stale
    /// snapshot would flicker the menu.
    pub fn apply_lists(&mut self, lists: crate::worker::Lists) -> bool {
        if lists.generation != self.tracks_generation {
            return false;
        }
        let mut changed = set(&mut self.tracks, lists.tracks);
        changed |= set(&mut self.playlist, lists.playlist);
        changed |= set(&mut self.progress, lists.progress);
        changed
    }

    /// Where the video sits inside a window of this size, normalised
    /// (x0, y0, x1, y1).
    ///
    /// mpv's own margins wherever they describe *this* size, because they
    /// account for everything mpv does — panscan, zoom, alignment, crop — and
    /// a locally derived fit is only right for the cases this player knows
    /// about.
    ///
    /// But they arrive asynchronously, and `osd-dimensions` carries the size
    /// it was measured at, which is how a stale one is recognised. During a
    /// resize every frame is rendered at a size mpv has not reported on yet,
    /// so the margins on hand describe the *previous* window. Using them
    /// anyway is what darkens the border: they put the video's edge somewhere
    /// the video is not, and the border pass spreads whatever it finds there,
    /// which after a fresh allocation is black.
    ///
    /// So when they are a size behind, the fit is worked out here instead,
    /// from the video's own dimensions and the panscan — the only two things
    /// this player lets anyone change. Wrong for a zoom set in mpv's own
    /// config; right every frame of a resize, which is the one that shows.
    ///
    /// A full-window rect is the right degenerate answer before anything has
    /// been laid out: it reads as no border.
    pub fn video_rect(&self, width: u32, height: u32) -> [f32; 4] {
        let [ow, oh, ml, mt, mr, mb] = self.osd;
        if ow > 0.0 && oh > 0.0 && ow as u32 == width && oh as u32 == height {
            return [
                (ml / ow) as f32,
                (mt / oh) as f32,
                ((ow - mr) / ow) as f32,
                ((oh - mb) / oh) as f32,
            ];
        }
        self.fitted_rect(width, height)
    }

    /// The fit mpv is about to compute, computed here.
    ///
    /// `panscan` slides between fitting the video inside the window and
    /// filling the window with it, which is mpv's own definition; at 1 the
    /// video covers the window and there is no border to draw.
    pub fn fitted_rect(&self, width: u32, height: u32) -> [f32; 4] {
        let (vw, vh) = (self.dwidth, self.dheight);
        let (ww, wh) = (width as f64, height as f64);
        if vw <= 0.0 || vh <= 0.0 || ww <= 0.0 || wh <= 0.0 {
            return [0.0, 0.0, 1.0, 1.0];
        }
        let inside = (ww / vw).min(wh / vh);
        let outside = (ww / vw).max(wh / vh);
        let scale = inside + (outside - inside) * self.panscan.clamp(0.0, 1.0);
        // Centred, which is mpv's default and the only alignment reachable
        // from here, so both margins on an axis are the same.
        let x0 = (((ww - vw * scale) * 0.5) / ww).clamp(0.0, 1.0);
        let y0 = (((wh - vh * scale) * 0.5) / wh).clamp(0.0, 1.0);
        [x0 as f32, y0 as f32, 1.0 - x0 as f32, 1.0 - y0 as f32]
    }

    /// Best name to show for what is playing. `media-title` carries embedded
    /// metadata where a file has it and falls back to the filename otherwise,
    /// which is what mpv itself displays.
    /// What the bar shows.
    ///
    /// mpv's `media-title` is the container's own title where one exists, and
    /// the filename where it does not — and a filename straight out of a
    /// release is mostly not a title. So anything that looks like it came off
    /// a disk gets read properly; a real embedded title is left alone.
    pub fn display_title(&self) -> String {
        let path = self
            .path
            .as_deref()
            .or(self.filename.as_deref())
            .unwrap_or("");
        crate::naming::titled(path, self.embedded_title())
    }

    /// The container's own title, where it has one.
    ///
    /// mpv reports the file name in `media-title` when the container carries
    /// no title of its own, and that fallback is not a title — passing it on
    /// would mean parsing a name that has already been parsed.
    fn embedded_title(&self) -> Option<&str> {
        let title = self.media_title.as_deref()?;
        let name = crate::naming::strip_path(self.path.as_deref().unwrap_or(""));
        let bare = self.filename.as_deref().unwrap_or("");
        (!title.is_empty() && title != name && title != bare).then_some(title)
    }

    /// What is known about one playlist row.
    ///
    /// Defaults rather than an `Option`: a row whose file has never been
    /// played here is the ordinary case, not a missing value, and the answer
    /// it wants — no length, no progress — is exactly the default.
    pub fn progress_of(&self, row: usize) -> crate::durations::Progress {
        self.progress.get(row).copied().unwrap_or_default()
    }

    /// Playback progress in 0..1, preferring the elapsed/duration ratio so it
    /// stays smooth; `percent-pos` updates less often.
    pub fn progress(&self) -> f32 {
        if self.duration > 0.0 {
            (self.time_pos / self.duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}

/// Assign and report whether the value actually moved.
fn set<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

/// `H:MM:SS`, dropping the hours field when it would be zero — matching how
/// the Tauri build renders timestamps.
pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".into();
    }
    let total = seconds as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
