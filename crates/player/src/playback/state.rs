//! Mirror of the mpv properties the UI cares about.
//!
//! mpv's typed event queue is drained on the UI thread straight into this
//! struct, which is then pushed into Slint properties.
//!
//! Plain fields rather than signals, since Slint's properties provide the
//! reactivity, and duplicating it here would make two sources of truth.

use dbm_lang::{fill, tr};
use dbm_mpv::prop::{self, TrackChoice};
use dbm_mpv::tracks::{PlaylistEntry, Track};
use dbm_mpv::{AnyProperty, Event, MediaPath, Mpv, Property, Value};

/// Declares [`Prop`] from one list, so each property's id, its place in
/// [`Prop::ALL`] and the mpv property it mirrors can't drift apart.
macro_rules! observed {
    ($($(#[$doc:meta])* $variant:ident = $property:path,)*) => {
        /// Every property the player observes. Its position in [`Prop::ALL`]
        /// is the id mpv reports it under.
        ///
        /// List-shaped properties (`track-list`, `playlist`) are absent, since
        /// observing them as nodes would marshal a tree across FFI on every
        /// change. Their counts are observed instead, and the list is pulled
        /// when one moves.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Prop {
            $($(#[$doc])* $variant,)*
        }

        impl Prop {
            /// In declaration order, which makes each one's index its id.
            pub const ALL: &[Self] = &[$(Self::$variant,)*];

            /// The mpv property it mirrors, whose type says what it arrives as.
            pub fn property(self) -> AnyProperty {
                match self {
                    $(Self::$variant => $property.into(),)*
                }
            }
        }
    };
}

observed! {
    Filename = prop::FILENAME,
    MediaTitle = prop::MEDIA_TITLE,
    Path = prop::PATH,
    Duration = prop::DURATION,
    TimePos = prop::TIME_POS,
    PercentPos = prop::PERCENT_POS,
    Pause = prop::PAUSE,
    Mute = prop::MUTE,
    Volume = prop::VOLUME,
    EofReached = prop::EOF_REACHED,
    IdleActive = prop::IDLE_ACTIVE,
    PlaylistPos = prop::PLAYLIST_POS,
    PlaylistCount = prop::PLAYLIST_COUNT,
    SubVisibility = prop::SUB_VISIBILITY,
    SubDelay = prop::SUB_DELAY,
    SubSpeed = prop::SUB_SPEED,
    SubScale = prop::SUB_SCALE,
    Panscan = prop::PANSCAN,
    Speed = prop::SPEED,
    Chapter = prop::CHAPTER,
    ChapterCount = prop::CHAPTER_COUNT,
    TrackCount = prop::TRACK_COUNT,
    /// Selecting a different track does not change the list's length, so
    /// this and `Aid` are what keep the `selected` flags from going stale.
    Sid = prop::SID,
    Aid = prop::AID,
    /// The video's own size after aspect correction. Enough, with `panscan`,
    /// to work out where a frame will land without waiting to be told.
    Dwidth = prop::DWIDTH,
    Dheight = prop::DHEIGHT,
    /// Where mpv actually laid the video out. Observed rather than polled,
    /// since a blocking read on the render path cost 220ms a frame during a
    /// resize, when mpv's core lock is contended.
    OsdW = prop::OSD_W,
    OsdH = prop::OSD_H,
    OsdMl = prop::OSD_ML,
    OsdMt = prop::OSD_MT,
    OsdMr = prop::OSD_MR,
    OsdMb = prop::OSD_MB,
    /// Never shown: observed because observing it is what starts mpv's
    /// hotplug monitor, which the audio watchdog needs to put sound back
    /// when a device returns (see `audio`).
    AudioDeviceList = prop::AUDIO_DEVICE_LIST,
}

impl Prop {
    pub const fn id(self) -> u64 {
        self as u64
    }

    /// Which property an [`Event::Property`] is about.
    pub fn of(id: u64) -> Option<Self> {
        usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
    }
}

/// Ask mpv for every [`Prop`], under its id.
pub fn observe(mpv: &Mpv) {
    for &prop in Prop::ALL {
        if let Err(e) = mpv.observe(prop.id(), prop.property()) {
            log::warn!("cannot observe {}: {e}", prop.property().name());
        }
    }
}

/// mpv's own layout, from `osd-dimensions`: the window's size, then the
/// margins round the picture, left, top, right and bottom, negative where it
/// is cropped. All zero before mpv has laid anything out.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OsdDims {
    pub w: f64,
    pub h: f64,
    pub ml: f64,
    pub mt: f64,
    pub mr: f64,
    pub mb: f64,
}

/// Track, chapter and playlist contents, read together because they are
/// invalidated together and a single job means a single round trip.
pub struct Lists {
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// One per playlist entry, in the same order: how long it is and how far
    /// in the resume point sits. Read here rather than in a job of its own,
    /// because the lookup needs the playlist's paths, which are read here.
    /// `None` where it was not read again, the list and the file playing
    /// being the same as when it last was (see `ListSync`).
    pub progress: Option<Vec<dbm_library::durations::Progress>>,
    /// mpv's current entry as this was read, which the progress, where read,
    /// is true as of.
    pub playing: Option<MediaPath>,
    /// The file's chapter marks. Invalidated with the tracks, by a new file.
    pub chapters: Vec<dbm_mpv::tracks::Chapter>,
    /// The generation this was read for, so a stale result can be discarded
    /// if the lists moved again while the job was running.
    pub generation: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub filename: Option<String>,
    pub media_title: Option<String>,
    pub path: Option<MediaPath>,
    pub duration: f64,
    pub time_pos: f64,
    pub percent_pos: f64,
    pub paused: bool,
    pub muted: bool,
    pub volume: f64,
    pub eof_reached: bool,
    /// Nothing loaded and nothing on its way, which is where a file that
    /// wouldn't play leaves mpv when it was the last.
    pub idle: bool,
    /// The entry playing, from 0, where one is.
    pub playlist_pos: Option<usize>,
    pub playlist_count: usize,
    pub sub_visibility: bool,
    pub sub_delay: f64,
    /// The speed subtitles run at against the film. Zero until mpv first
    /// reports it, which reads as 1.
    pub sub_speed: f64,
    /// Size multiplier. Mirrored rather than read from the saved settings, so
    /// the readout shows what mpv settled on, including its own clamping.
    ///
    /// Placement isn't mirrored, since while the bar is up mpv holds a lifted
    /// line, and the readout shows where the person put it (see `subline`).
    pub sub_scale: f64,
    /// 0 letterboxes to fit, 1 crops to fill.
    pub panscan: f64,
    /// Playback speed. Zero until mpv first reports it, which reads as 1.
    pub speed: f64,
    /// The chapter playing, from 0, where the file has one and it has begun.
    pub chapter: Option<usize>,
    pub chapter_count: i64,
    /// Read on the worker with the tracks (see `worker`).
    pub chapters: Vec<dbm_mpv::tracks::Chapter>,
    /// Where an episode's title sequence was heard to be, in seconds, and the
    /// file that is true of (see `titles`). Kept with its path, so a new file
    /// needs nothing cleared.
    pub intro_heard: Option<(MediaPath, (f64, f64))>,
    /// Where its closing credits were heard to be, kept the same way: in
    /// pieces, with a scene of the file's own between two, or after the last
    /// where it stops short of the end.
    pub credits_heard: Option<(MediaPath, Vec<(f64, f64)>)>,
    /// Playlist entries that wouldn't play, by mpv's id, with the notice
    /// that said so, so their rows can say it again after it has gone. By id
    /// rather than path, since a file that fails at once has no path reported
    /// yet.
    pub failed: std::collections::HashMap<i64, String>,
    /// The entry that last played to its end, by mpv's id, for the frame to
    /// take: what autoplay is moving on from (see `arrival`).
    pub played_out: Option<i64>,
    pub track_count: i64,
    pub sid: Option<TrackChoice>,
    pub aid: Option<TrackChoice>,
    /// Bumped whenever the track list may have changed, so the lists below
    /// know to re-pull. Covers both the list mutating and the selection
    /// moving within it.
    pub tracks_generation: u64,
    /// How many files mpv has finished opening this run, which tells the
    /// driver when an open is over: when one loads, not when one exists.
    pub loads: u64,
    pub osd: OsdDims,
    /// Display size of the video itself, aspect already applied.
    pub dwidth: f64,
    pub dheight: f64,

    /// Read on the worker thread (see `worker`). Not written by `apply`.
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// Parallel to `playlist`, and only as long as it: read in the same job,
    /// applied in the same step.
    pub progress: Vec<dbm_library::durations::Progress>,
    /// What `probe` found, by path.
    ///
    /// Only added to, and read over `playlist` and `progress` rather than
    /// written into them, since those are replaced by every list read, and a
    /// read begun before a probe landed would undo it.
    pub probed: std::collections::HashMap<MediaPath, dbm_library::probe::Found>,
    /// Seasons of the show found beside a playlist that is one season of it
    /// (see [`dbm_library::shelf`]). Shown rather than queued, and opened
    /// when picked.
    pub beside: Vec<dbm_library::shelf::Beside>,
}

impl PlayerState {
    /// Fold one event in. Returns whether anything the UI shows changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::Property { id, value } => {
                Prop::of(*id).is_some_and(|prop| self.apply_property(prop, value.as_ref()))
            }
            // A new file invalidates everything list-shaped.
            Event::FileLoaded => {
                self.loads = self.loads.wrapping_add(1);
                self.lists_moved(true)
            }
            Event::EndFile {
                eof: true, entry, ..
            } => {
                self.played_out = Some(*entry);
                true
            }
            // The chapters are the last file's until the new file's are read,
            // and its index, length and time would be read against them. So
            // they go now, and a read still on its way for the last file is
            // let go.
            Event::StartFile => {
                self.chapters.clear();
                self.lists_moved(true)
            }
            Event::EndFile { .. } | Event::Seek | Event::PlaybackRestart => true,
            // Filtered out by the event drain before reaching here: command
            // completions are routed to whoever issued them.
            Event::Reply { .. } | Event::Shutdown => false,
        }
    }

    /// No value is normal (`time-pos` has none until something plays), so it
    /// clears the field rather than being an error.
    fn apply_property(&mut self, which: Prop, value: Option<&Value>) -> bool {
        match which {
            Prop::Filename => set(&mut self.filename, text(prop::FILENAME, value)),
            Prop::MediaTitle => set(&mut self.media_title, text(prop::MEDIA_TITLE, value)),
            Prop::Path => set(&mut self.path, prop::PATH.decode(value)),
            Prop::Duration => set(
                &mut self.duration,
                number(prop::DURATION, value).unwrap_or(0.0),
            ),
            Prop::TimePos => set(
                &mut self.time_pos,
                number(prop::TIME_POS, value).unwrap_or(0.0),
            ),
            Prop::PercentPos => set(
                &mut self.percent_pos,
                number(prop::PERCENT_POS, value).unwrap_or(0.0),
            ),
            Prop::Pause => set(&mut self.paused, flag(prop::PAUSE, value)),
            Prop::Mute => set(&mut self.muted, flag(prop::MUTE, value)),
            Prop::Volume => set(&mut self.volume, number(prop::VOLUME, value).unwrap_or(0.0)),
            Prop::EofReached => set(&mut self.eof_reached, flag(prop::EOF_REACHED, value)),
            Prop::IdleActive => set(&mut self.idle, flag(prop::IDLE_ACTIVE, value)),
            Prop::PlaylistPos => set(
                &mut self.playlist_pos,
                prop::PLAYLIST_POS.decode(value).and_then(index),
            ),
            Prop::PlaylistCount => {
                let count = prop::PLAYLIST_COUNT.decode(value).and_then(index);
                let moved = set(&mut self.playlist_count, count.unwrap_or(0));
                self.lists_moved(moved)
            }
            Prop::SubVisibility => set(&mut self.sub_visibility, flag(prop::SUB_VISIBILITY, value)),
            Prop::SubDelay => set(
                &mut self.sub_delay,
                number(prop::SUB_DELAY, value).unwrap_or(0.0),
            ),
            Prop::SubSpeed => set(
                &mut self.sub_speed,
                number(prop::SUB_SPEED, value).unwrap_or(1.0),
            ),
            Prop::SubScale => set(
                &mut self.sub_scale,
                number(prop::SUB_SCALE, value).unwrap_or(1.0),
            ),
            Prop::Panscan => set(
                &mut self.panscan,
                number(prop::PANSCAN, value).unwrap_or(0.0),
            ),
            Prop::Speed => set(&mut self.speed, number(prop::SPEED, value).unwrap_or(1.0)),
            Prop::Chapter => set(
                &mut self.chapter,
                prop::CHAPTER.decode(value).and_then(index),
            ),
            Prop::ChapterCount => {
                let count = prop::CHAPTER_COUNT.decode(value).unwrap_or(0);
                let moved = set(&mut self.chapter_count, count);
                self.lists_moved(moved)
            }
            Prop::TrackCount => {
                let count = prop::TRACK_COUNT.decode(value).unwrap_or(0);
                let moved = set(&mut self.track_count, count);
                self.lists_moved(moved)
            }
            Prop::Sid => {
                let moved = set(&mut self.sid, prop::SID.decode(value));
                self.lists_moved(moved)
            }
            Prop::Aid => {
                let moved = set(&mut self.aid, prop::AID.decode(value));
                self.lists_moved(moved)
            }
            Prop::Dwidth => set(&mut self.dwidth, number(prop::DWIDTH, value).unwrap_or(0.0)),
            Prop::Dheight => set(
                &mut self.dheight,
                number(prop::DHEIGHT, value).unwrap_or(0.0),
            ),
            Prop::OsdW => set(&mut self.osd.w, number(prop::OSD_W, value).unwrap_or(0.0)),
            Prop::OsdH => set(&mut self.osd.h, number(prop::OSD_H, value).unwrap_or(0.0)),
            Prop::OsdMl => set(&mut self.osd.ml, number(prop::OSD_ML, value).unwrap_or(0.0)),
            Prop::OsdMt => set(&mut self.osd.mt, number(prop::OSD_MT, value).unwrap_or(0.0)),
            Prop::OsdMr => set(&mut self.osd.mr, number(prop::OSD_MR, value).unwrap_or(0.0)),
            Prop::OsdMb => set(&mut self.osd.mb, number(prop::OSD_MB, value).unwrap_or(0.0)),
            // The audio watchdog's, not the interface's.
            Prop::AudioDeviceList => false,
        }
    }

    /// Have the track list, the playlist and the chapters read again when
    /// `moved`, and pass `moved` on.
    fn lists_moved(&mut self, moved: bool) -> bool {
        if moved {
            self.tracks_generation = self.tracks_generation.wrapping_add(1);
        }
        moved
    }

    /// The playlist's files, in order.
    pub fn playlist_paths(&self) -> Vec<MediaPath> {
        self.playlist.iter().map(|e| e.filename.clone()).collect()
    }

    /// Fold in lists read on the worker thread.
    ///
    /// A result is discarded if the lists moved again while it was read,
    /// since the newer request is on its way, and a stale snapshot would
    /// flicker the menu.
    pub fn apply_lists(&mut self, lists: Lists) -> bool {
        if lists.generation != self.tracks_generation {
            return false;
        }
        let mut changed = set(&mut self.tracks, lists.tracks);
        changed |= set(&mut self.playlist, lists.playlist);
        if let Some(progress) = lists.progress {
            changed |= set(&mut self.progress, progress);
        }
        changed |= set(&mut self.chapters, lists.chapters);
        changed
    }

    /// Whether nothing follows the entry playing: it is the last, or there
    /// is no list.
    pub fn at_last(&self) -> bool {
        self.playlist_pos.map_or(0, |pos| pos + 1) >= self.playlist_count
    }

    /// Whether nothing comes before the entry playing.
    pub fn at_first(&self) -> bool {
        self.playlist_pos.is_none_or(|pos| pos == 0)
    }

    /// What the chapter playing is called: its own title where the file has
    /// one, and its number out of how many where it does not. Empty with no
    /// chapters.
    pub fn chapter_label(&self) -> String {
        let Some(index) = self.chapter else {
            return String::new();
        };
        let Some(chapter) = self.chapters.get(index) else {
            return String::new();
        };
        chapter.title.clone().unwrap_or_else(|| {
            fill(
                tr("Chapter {} of {}"),
                &[&(index + 1), &self.chapters.len()],
            )
        })
    }

    /// Where the video sits inside a window of this size, normalised
    /// (x0, y0, x1, y1).
    ///
    /// mpv's own margins wherever they describe this size, since they account
    /// for everything mpv does (panscan, zoom, alignment, crop).
    ///
    /// They arrive asynchronously, with the size they were measured at. During
    /// a resize the margins on hand describe the previous window, and would
    /// put the video's edge where the video isn't, darkening the border. So
    /// when they are a size behind, the fit is worked out here from the
    /// video's dimensions and the panscan, wrong only for a zoom set in mpv's
    /// own config.
    ///
    /// Before anything is laid out, a full-window rect reads as no border.
    pub fn video_rect(&self, width: u32, height: u32) -> [f32; 4] {
        let OsdDims {
            w: ow,
            h: oh,
            ml,
            mt,
            mr,
            mb,
        } = self.osd;
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
        let (ww, wh) = (f64::from(width), f64::from(height));
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

    /// What the bar shows.
    ///
    /// mpv's `media-title` is the container's title where one exists, and the
    /// filename otherwise, which out of a release is mostly not a title. So a
    /// filename is read properly, and a real embedded title left alone.
    pub fn display_title(&self) -> String {
        dbm_naming::titled(self.title_path(), self.embedded_title())
    }

    /// What the bar shows when it has no room for the show's name as well:
    /// the part that tells one episode from the next. Empty for anything
    /// that has no show.
    pub fn display_episode(&self) -> String {
        match dbm_naming::described(self.title_path(), self.embedded_title()) {
            (Some(_), episode) => episode,
            (None, _) => String::new(),
        }
    }

    fn title_path(&self) -> &str {
        self.path
            .as_ref()
            .map(MediaPath::as_str)
            .or(self.filename.as_deref())
            .unwrap_or("")
    }

    /// The container's own title, where it has one.
    ///
    /// mpv reports the file name in `media-title` when the container has no
    /// title of its own, and that fallback isn't a title.
    fn embedded_title(&self) -> Option<&str> {
        let title = self.media_title.as_deref()?;
        let name = dbm_naming::strip_path(self.path.as_ref().map_or("", MediaPath::as_str));
        let bare = self.filename.as_deref().unwrap_or("");
        (!title.is_empty() && title != name && title != bare).then_some(title)
    }

    /// What is known about one playlist row.
    ///
    /// Defaults rather than an `Option`, since a file never played here is
    /// the ordinary case, and its answer (no length, no progress) is the
    /// default.
    pub fn progress_of(&self, row: usize) -> dbm_library::durations::Progress {
        self.progress.get(row).copied().unwrap_or_default()
    }

    /// Take delivery of described files. Returns whether any row changes.
    pub fn apply_probed(&mut self, found: Vec<dbm_library::probe::Found>) -> bool {
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
    pub fn known_progress(&self, row: usize) -> dbm_library::durations::Progress {
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
    /// probe's otherwise, since mpv's is read by what is actually playing it.
    pub fn known_title(&self, row: usize) -> Option<&str> {
        let entry = self.playlist.get(row)?;
        crate::playback::labels::embedded_title(entry)
            .or_else(|| self.probed_for(row)?.title.as_deref())
    }

    fn probed_for(&self, row: usize) -> Option<&dbm_library::probe::Found> {
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

/// A number mpv reported, where it is one: a NaN or infinity would poison
/// every sum it went into.
fn number<A>(property: Property<f64, A>, value: Option<&Value>) -> Option<f64> {
    property.decode(value).filter(|n| n.is_finite())
}

/// A flag, off where mpv has no value for it.
fn flag<A>(property: Property<bool, A>, value: Option<&Value>) -> bool {
    property.decode(value).unwrap_or(false)
}

/// Text, where there is any.
fn text<A>(property: Property<String, A>, value: Option<&Value>) -> Option<String> {
    property.decode(value).filter(|text| !text.is_empty())
}

/// A position as mpv reports one, where -1 is none.
fn index(n: i64) -> Option<usize> {
    usize::try_from(n).ok()
}

/// Assign and report whether the value actually moved.
fn set<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_property_is_observed_under_its_own_index() {
        for (index, &prop) in Prop::ALL.iter().enumerate() {
            assert_eq!(prop.id(), index as u64, "{prop:?}");
            assert_eq!(Prop::of(prop.id()), Some(prop));
        }
    }

    #[test]
    fn the_playlist_ends_are_read_as_mpv_reports_them() {
        let at = |pos, count| PlayerState {
            playlist_pos: pos,
            playlist_count: count,
            ..Default::default()
        };
        assert!(at(None, 0).at_last() && at(None, 0).at_first());
        assert!(
            !at(None, 3).at_last(),
            "nothing playing yet, with more to come"
        );
        assert!(at(Some(0), 3).at_first() && !at(Some(0), 3).at_last());
        assert!(at(Some(2), 3).at_last() && !at(Some(2), 3).at_first());
    }
}
