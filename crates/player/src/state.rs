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
    ("speed", FORMAT_DOUBLE),
    ("chapter", FORMAT_DOUBLE),
    ("chapter-list/count", FORMAT_DOUBLE),
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
    /// Playback speed. Zero until mpv first reports it, which reads as 1.
    pub speed: f64,
    /// The chapter playing, from 0, or -1 before the first mark.
    pub chapter: i64,
    pub chapter_count: i64,
    /// Read on the worker with the tracks — see `worker`.
    pub chapters: Vec<crate::tracks::Chapter>,
    /// Files that would not play this run, by path, so their playlist rows
    /// can say so after the notice has gone.
    pub failed: std::collections::HashSet<String>,
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
    /// What `probe` found, by path.
    ///
    /// Only ever added to, and read *over* `playlist` and `progress` rather
    /// than written into them: those are replaced wholesale by every list
    /// read, and a read that left the worker before a probe landed would
    /// otherwise put the row back the way it was.
    pub probed: std::collections::HashMap<String, crate::probe::Found>,
    /// Seasons of the show found beside a playlist that is one season of it —
    /// see [`crate::shelf`]. Not queued: shown, and opened when picked.
    pub beside: Vec<crate::shelf::Beside>,
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
            Event::StartFile
            | Event::EndFile { .. }
            | Event::Seek
            | Event::PlaybackRestart => true,
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
            "speed" => set(&mut self.speed, num.unwrap_or(1.0)),
            "chapter" => set(&mut self.chapter, num.map_or(-1, |n| n as i64)),
            "chapter-list/count" => {
                if set(&mut self.chapter_count, num.unwrap_or(0.0) as i64) {
                    self.tracks_generation = self.tracks_generation.wrapping_add(1);
                    return true;
                }
                false
            }
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
        changed |= set(&mut self.chapters, lists.chapters);
        changed
    }

    /// What the chapter playing is called: its own title where the file has
    /// one, and its number out of how many where it does not. Empty with no
    /// chapters.
    pub fn chapter_label(&self) -> String {
        let Ok(index) = usize::try_from(self.chapter) else {
            return String::new();
        };
        let Some(chapter) = self.chapters.get(index) else {
            return String::new();
        };
        chapter.title.clone().unwrap_or_else(|| {
            format!("Chapter {} of {}", index + 1, self.chapters.len())
        })
    }

    /// Whether an episode's closing credits are playing: the chapter playing
    /// is the one they start at — see [`credits_chapter`](Self::credits_chapter)
    /// — or one after it.
    pub fn credits_rolling(&self) -> bool {
        let Ok(current) = usize::try_from(self.chapter) else {
            return false;
        };
        let Some(start) = self.credits_chapter() else {
            return false;
        };
        // Last, being the one that parses a name.
        current >= start
            && self.path.as_deref().is_some_and(|path| {
                matches!(crate::naming::parse(path), crate::naming::Media::Episode { .. })
            })
    }

    /// The chapter the closing credits start at, where the chapters say.
    ///
    /// By name first: the first chapter in the file's second half called
    /// something like `End Credits`. The half keeps a cold open's `Credits`
    /// from counting.
    ///
    /// Plenty of releases name nothing — every chapter titled with its own
    /// start time, or not at all — and a broadcast episode is split at its
    /// act breaks, the last of which is the credits. So where no chapter has
    /// a real name, the last one is the credits if it is short: a final act
    /// runs minutes, and a closing sequence well under `CREDITS_TAIL`.
    fn credits_chapter(&self) -> Option<usize> {
        if self.duration <= 0.0 {
            return None;
        }
        let named = self.chapters.iter().position(|c| {
            c.time >= self.duration / 2.0
                && c.title.as_deref().is_some_and(crate::naming::is_credits)
        });
        if named.is_some() {
            return named;
        }
        let unnamed = self
            .chapters
            .iter()
            .all(|c| c.title.as_deref().map_or(true, crate::naming::is_unnamed_chapter));
        let last = self.chapters.len().checked_sub(1)?;
        let tail = self.duration - self.chapters[last].time;
        (unnamed && last > 0 && tail > 0.0 && tail <= CREDITS_TAIL).then_some(last)
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

    /// Take delivery of described files. Returns whether any row changes.
    pub fn apply_probed(&mut self, found: Vec<crate::probe::Found>) -> bool {
        let mut changed = false;
        for f in found {
            if self.probed.get(&f.path) == Some(&f) {
                continue;
            }
            changed |= self.playlist.iter().any(|e| e.filename == f.path);
            self.probed.insert(f.path.clone(), f);
        }
        changed
    }

    /// [`progress_of`](Self::progress_of), with a probed length filling in
    /// for one mpv has never reported.
    pub fn known_progress(&self, row: usize) -> crate::durations::Progress {
        let mut progress = self.progress_of(row);
        if progress.seconds > 0.0 {
            return progress;
        }
        let Some(found) = self.probed_for(row) else {
            return progress;
        };
        if found.seconds > 0.0 {
            progress.seconds = found.seconds;
            if progress.start > 0.0 {
                progress.fraction = (progress.start / found.seconds).clamp(0.0, 1.0) as f32;
            }
        }
        progress
    }

    /// A row's embedded title: mpv's where mpv has opened the file, the
    /// probe's otherwise. mpv's wins because it is the same tag read by the
    /// thing actually playing it.
    pub fn known_title(&self, row: usize) -> Option<&str> {
        let entry = self.playlist.get(row)?;
        entry
            .embedded_title()
            .or_else(|| self.probed_for(row)?.title.as_deref())
    }

    fn probed_for(&self, row: usize) -> Option<&crate::probe::Found> {
        self.probed.get(&self.playlist.get(row)?.filename)
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

/// The longest last chapter that is taken for credits when no chapter is
/// named, in seconds.
const CREDITS_TAIL: f64 = 180.0;

/// Assign and report whether the value actually moved.
fn set<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

/// How much of a film is left, the way a person says it: "43 min left",
/// "1 h 12 min left". Minutes are rounded up, so a row never claims nothing
/// is left of something that has not ended.
pub fn format_left(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 60.0 {
        return "under a minute left".into();
    }
    let minutes = (seconds / 60.0).ceil() as u64;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min left"),
        (h, 0) => format!("{h} h left"),
        (h, m) => format!("{h} h {m} min left"),
    }
}

/// A speed as a person writes it: `1×`, `1.5×`, `1.25×`.
pub fn format_speed(speed: f64) -> String {
    let speed = if speed > 0.0 { speed } else { 1.0 };
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}×")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::Chapter;

    fn playing(path: &str, chapter: i64, marks: &[(f64, &str)]) -> PlayerState {
        PlayerState {
            path: Some(path.into()),
            duration: 3600.0,
            chapter,
            chapters: marks
                .iter()
                .map(|(time, title)| Chapter {
                    time: *time,
                    title: Some(title.to_string()),
                })
                .collect(),
            ..Default::default()
        }
    }

    const EPISODE: &str = "Show.Name.S01E02.1080p.mkv";
    const MARKS: &[(f64, &str)] = &[(0.0, "Episode"), (3400.0, "End Credits"), (3550.0, "Preview")];

    #[test]
    fn the_credits_roll_from_their_chapter_to_the_end() {
        assert!(!playing(EPISODE, 0, MARKS).credits_rolling());
        assert!(playing(EPISODE, 1, MARKS).credits_rolling());
        // A preview of the next episode after the credits is still past them.
        assert!(playing(EPISODE, 2, MARKS).credits_rolling());
    }

    #[test]
    fn a_film_has_no_next_episode_to_offer() {
        assert!(!playing("Some Film (2019).mkv", 1, MARKS).credits_rolling());
    }

    #[test]
    fn credits_at_the_start_are_an_opening() {
        let marks = &[(0.0, "Credits"), (90.0, "Episode")];
        assert!(!playing(EPISODE, 0, marks).credits_rolling());
        assert!(!playing(EPISODE, 1, marks).credits_rolling());
    }

    /// The Walking Dead as released: seven act breaks, each titled with its
    /// start time, and the last half-minute the credits.
    const ACTS: &[(f64, &str)] = &[
        (0.0, "00:00:00.000"),
        (141.3, "00:02:21.141"),
        (871.1, "00:14:26.407"),
        (1296.1, "00:21:32.583"),
        (1606.5, "00:26:44.186"),
        (2059.8, "00:34:16.346"),
        (3566.9, "00:59:26.900"),
    ];

    #[test]
    fn unnamed_chapters_end_in_short_credits() {
        assert!(!playing(EPISODE, 5, ACTS).credits_rolling());
        assert!(playing(EPISODE, 6, ACTS).credits_rolling());
    }

    #[test]
    fn a_long_last_act_is_not_credits() {
        let acts = &[(0.0, "Chapter 01"), (1800.0, "Chapter 02"), (3000.0, "Chapter 03")];
        assert!(!playing(EPISODE, 2, acts).credits_rolling());
    }

    #[test]
    fn named_chapters_are_taken_at_their_word() {
        // Short, last, and called something else: not a guess worth making.
        let marks = &[(0.0, "Episode"), (3500.0, "Preview")];
        assert!(!playing(EPISODE, 1, marks).credits_rolling());
    }

    #[test]
    fn nothing_rolls_before_the_length_is_known() {
        let mut state = playing(EPISODE, 1, MARKS);
        state.duration = 0.0;
        assert!(!state.credits_rolling());
    }
}
