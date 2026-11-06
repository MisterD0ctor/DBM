//! What the player knows about what is playing.
//!
//! Three parts. Most fields mirror the mpv properties the interface cares
//! about: mpv's event queue is drained on the UI thread straight into them
//! (see [`PlayerState::apply`]). Then mpv's lists, read on the worker when a
//! count moves. And [`Found`]: what the player found out for itself, about
//! the files and the playlist, which `apply` never touches.
//!
//! Plain fields rather than signals, since Slint's properties provide the
//! reactivity, and duplicating it here would make two sources of truth.

use std::collections::HashMap;

use dbm_gpu::rect::{Normalised, Rect};
use dbm_lang::fill;
use dbm_library::durations::Progress;
use dbm_library::threads::ForFile;
use dbm_library::titles::Span;
use dbm_mpv::prop::{self, TrackChoice};
use dbm_mpv::tracks::{AudioDevice, EntryId, PlaylistEntry, Track};
use dbm_mpv::{AnyProperty, ErrorCode, Event, MediaPath, Mpv, Value};

/// Declares [`Prop`] and [`Observed`] from one list, so each property's id,
/// its place in [`Prop::ALL`], the mpv property it mirrors and the type it
/// arrives as can't drift apart: a type that isn't the property's own
/// doesn't compile.
macro_rules! observed {
    ($($(#[$doc:meta])* $variant:ident: $value:ty = $property:path,)*) => {
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

        /// A change mpv reported to one of them, as its property's type.
        /// `None` is no value right now, like `time-pos` before anything is
        /// loaded.
        #[derive(Clone, Debug, PartialEq)]
        pub enum Observed {
            $($variant(Option<$value>),)*
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

        impl Observed {
            /// What an [`Event::Property`] says, read as the type of the
            /// property its id names, or `None` for an id that names none.
            pub fn of(id: u64, value: Option<&Value>) -> Option<Self> {
                Some(match Prop::of(id)? {
                    $(Prop::$variant => Self::$variant($property.decode(value)),)*
                })
            }
        }
    };
}

observed! {
    Filename: String = prop::FILENAME,
    MediaTitle: String = prop::MEDIA_TITLE,
    Path: MediaPath = prop::PATH,
    Duration: f64 = prop::DURATION,
    TimePos: f64 = prop::TIME_POS,
    PercentPos: f64 = prop::PERCENT_POS,
    Pause: bool = prop::PAUSE,
    Mute: bool = prop::MUTE,
    Volume: f64 = prop::VOLUME,
    EofReached: bool = prop::EOF_REACHED,
    IdleActive: bool = prop::IDLE_ACTIVE,
    PlaylistPos: i64 = prop::PLAYLIST_POS,
    PlaylistCount: i64 = prop::PLAYLIST_COUNT,
    SubVisibility: bool = prop::SUB_VISIBILITY,
    SubDelay: f64 = prop::SUB_DELAY,
    SubSpeed: f64 = prop::SUB_SPEED,
    SubScale: f64 = prop::SUB_SCALE,
    Panscan: f64 = prop::PANSCAN,
    Speed: f64 = prop::SPEED,
    Chapter: i64 = prop::CHAPTER,
    ChapterCount: i64 = prop::CHAPTER_COUNT,
    TrackCount: i64 = prop::TRACK_COUNT,
    /// Selecting a different track does not change the list's length, so
    /// this and `Aid` are what keep the `selected` flags from going stale.
    Sid: TrackChoice = prop::SID,
    Aid: TrackChoice = prop::AID,
    /// The video's own size after aspect correction. Enough, with `panscan`,
    /// to work out where a frame will land without waiting to be told.
    Dwidth: f64 = prop::DWIDTH,
    Dheight: f64 = prop::DHEIGHT,
    /// Where mpv actually laid the video out. Observed rather than polled,
    /// since a blocking read on the render path cost 220ms a frame during a
    /// resize, when mpv's core lock is contended.
    OsdW: f64 = prop::OSD_W,
    OsdH: f64 = prop::OSD_H,
    OsdMl: f64 = prop::OSD_ML,
    OsdMt: f64 = prop::OSD_MT,
    OsdMr: f64 = prop::OSD_MR,
    OsdMb: f64 = prop::OSD_MB,
    /// Never shown: observed because observing it is what starts mpv's
    /// hotplug monitor, which the audio watchdog needs to put sound back
    /// when a device returns (see `audio`).
    AudioDeviceList: Vec<AudioDevice> = prop::AUDIO_DEVICE_LIST,
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
/// is cropped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OsdDims {
    pub w: f64,
    pub h: f64,
    pub ml: f64,
    pub mt: f64,
    pub mr: f64,
    pub mb: f64,
}

/// The six parts of that layout as mpv reports them, one at a time.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct OsdParts {
    w: Option<f64>,
    h: Option<f64>,
    ml: Option<f64>,
    mt: Option<f64>,
    mr: Option<f64>,
    mb: Option<f64>,
}

impl OsdParts {
    /// The layout these parts describe, once every one of them is known.
    /// Until then there is no layout to draw from, which is not the same as
    /// a layout with every number zero.
    fn build(self) -> Option<OsdDims> {
        let dims = OsdDims {
            w: self.w?,
            h: self.h?,
            ml: self.ml?,
            mt: self.mt?,
            mr: self.mr?,
            mb: self.mb?,
        };
        (dims.w > 0.0 && dims.h > 0.0).then_some(dims)
    }
}

/// Track, chapter and playlist contents, read together because they are
/// invalidated together and a single job means a single round trip.
pub struct Lists {
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// Each playlist entry's length and resume point, by path. Read here
    /// rather than in a job of its own, because the lookup needs the
    /// playlist's paths, which are read here. `None` where it was not read
    /// again, the list and the file playing being the same as when it last
    /// was (see `ListSync`).
    pub progress: Option<HashMap<MediaPath, Progress>>,
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
    /// How long the file is, in seconds, once mpv knows, which is a moment
    /// after it opens.
    pub duration: Option<f64>,
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
    /// The speed subtitles run at against the film.
    pub sub_speed: Rate,
    /// Size multiplier. Mirrored rather than read from the saved settings, so
    /// the readout shows what mpv settled on, including its own clamping.
    ///
    /// Placement isn't mirrored, since while the bar is up mpv holds a lifted
    /// line, and the readout shows where the person put it (see `subline`).
    pub sub_scale: f64,
    /// 0 letterboxes to fit, 1 crops to fill.
    pub panscan: f64,
    pub speed: Rate,
    /// The chapter playing, from 0, where the file has one and it has begun.
    pub chapter: Option<usize>,
    /// Read on the worker with the tracks (see `worker`). Its length is the
    /// chapter count, so there is no separate field to drift from it.
    pub chapters: Vec<dbm_mpv::tracks::Chapter>,
    pub track_count: i64,
    pub sid: Option<TrackChoice>,
    pub aid: Option<TrackChoice>,
    /// Bumped whenever the track list may have changed, so the lists below
    /// know to re-pull. Covers both the list mutating and the selection
    /// moving within it.
    pub tracks_generation: u64,
    /// How many files mpv has finished opening this run, which tells the
    /// player when an open is over: when one loads, not when one exists.
    pub loads: u64,
    /// mpv's layout, once all six of its parts have been reported.
    pub osd: Option<OsdDims>,
    pub(super) osd_parts: OsdParts,
    /// Display size of the video itself, aspect already applied.
    pub dwidth: f64,
    pub dheight: f64,

    /// Read on the worker thread (see `worker`). Not written by `apply`.
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    pub found: Found,
}

/// A speed against the film's own, where 1 is as timed: playback's, or the
/// subtitles' against the film. 1 until mpv says otherwise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rate(f64);

impl Default for Rate {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Rate {
    /// What mpv reported, where that is a speed at all.
    fn of(reported: Option<f64>) -> Self {
        number(reported)
            .filter(|n| *n > 0.0)
            .map_or_else(Self::default, Self)
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// What the player found out for itself about the files playing and the
/// playlist, beside what mpv reports.
#[derive(Debug, Clone, Default)]
pub struct Found {
    /// How far into each playlist entry its saved place is, by path, so a
    /// list that has moved since can't give a row another file's.
    pub progress: HashMap<MediaPath, Progress>,
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
    /// Playlist entries that wouldn't play, by mpv's id, with mpv's reason,
    /// so their rows can say it again after the notice has gone (see
    /// `unplayable`). By id rather than path, since a file that fails at once
    /// has no path reported yet.
    pub failed: std::collections::HashMap<EntryId, ErrorCode>,
    /// Where an episode's title sequence was heard to be, and the file that
    /// is true of (see `titles`). Kept with its path, so a new file needs
    /// nothing cleared.
    pub intro_heard: Option<ForFile<Span>>,
    /// Where its closing credits were heard to be, kept the same way: in
    /// pieces, with a scene of the file's own between two, or after the last
    /// where it stops short of the end.
    pub credits_heard: Option<ForFile<Vec<Span>>>,
}

impl PlayerState {
    /// Fold one event in. Returns whether anything the UI shows changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::Property { id, value } => {
                Observed::of(*id, value.as_ref()).is_some_and(|change| self.observe(change))
            }
            // A new file invalidates everything list-shaped.
            Event::FileLoaded => {
                self.loads = self.loads.wrapping_add(1);
                self.lists_moved(true)
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

    /// Fold in one property's change. Returns whether anything the UI shows
    /// changed.
    ///
    /// No value is normal (`time-pos` has none until something plays), so it
    /// clears the field rather than being an error.
    pub fn observe(&mut self, change: Observed) -> bool {
        match change {
            Observed::Filename(name) => set(&mut self.filename, text(name)),
            Observed::MediaTitle(title) => set(&mut self.media_title, text(title)),
            Observed::Path(path) => {
                let moved = set(&mut self.path, path.clone());
                let forgot = path.as_ref().is_some_and(|path| self.forget_failure(path));
                moved || forgot
            }
            Observed::Duration(n) => set(&mut self.duration, number(n).filter(|n| *n > 0.0)),
            Observed::TimePos(n) => set(&mut self.time_pos, number(n).unwrap_or(0.0)),
            Observed::PercentPos(n) => set(&mut self.percent_pos, number(n).unwrap_or(0.0)),
            Observed::Pause(on) => set(&mut self.paused, on.unwrap_or(false)),
            Observed::Mute(on) => set(&mut self.muted, on.unwrap_or(false)),
            Observed::Volume(n) => set(&mut self.volume, number(n).unwrap_or(0.0)),
            Observed::EofReached(on) => set(&mut self.eof_reached, on.unwrap_or(false)),
            Observed::IdleActive(on) => set(&mut self.idle, on.unwrap_or(false)),
            Observed::PlaylistPos(pos) => set(&mut self.playlist_pos, pos.and_then(index)),
            Observed::PlaylistCount(count) => {
                let moved = set(&mut self.playlist_count, count.and_then(index).unwrap_or(0));
                self.lists_moved(moved)
            }
            Observed::SubVisibility(on) => set(&mut self.sub_visibility, on.unwrap_or(false)),
            Observed::SubDelay(n) => set(&mut self.sub_delay, number(n).unwrap_or(0.0)),
            Observed::SubSpeed(n) => set(&mut self.sub_speed, Rate::of(n)),
            Observed::SubScale(n) => set(&mut self.sub_scale, number(n).unwrap_or(1.0)),
            Observed::Panscan(n) => set(&mut self.panscan, number(n).unwrap_or(0.0)),
            Observed::Speed(n) => set(&mut self.speed, Rate::of(n)),
            Observed::Chapter(chapter) => set(&mut self.chapter, chapter.and_then(index)),
            // The count alone is not kept: `chapters.len()` is the count, and
            // a change here is the cue to read the marks themselves.
            Observed::ChapterCount(_) => self.lists_moved(true),
            Observed::TrackCount(count) => {
                let moved = set(&mut self.track_count, count.unwrap_or(0));
                self.lists_moved(moved)
            }
            Observed::Sid(choice) => {
                let moved = set(&mut self.sid, choice);
                self.lists_moved(moved)
            }
            Observed::Aid(choice) => {
                let moved = set(&mut self.aid, choice);
                self.lists_moved(moved)
            }
            Observed::Dwidth(n) => set(&mut self.dwidth, number(n).unwrap_or(0.0)),
            Observed::Dheight(n) => set(&mut self.dheight, number(n).unwrap_or(0.0)),
            Observed::OsdW(n) => self.set_osd(|parts| &mut parts.w, n),
            Observed::OsdH(n) => self.set_osd(|parts| &mut parts.h, n),
            Observed::OsdMl(n) => self.set_osd(|parts| &mut parts.ml, n),
            Observed::OsdMt(n) => self.set_osd(|parts| &mut parts.mt, n),
            Observed::OsdMr(n) => self.set_osd(|parts| &mut parts.mr, n),
            Observed::OsdMb(n) => self.set_osd(|parts| &mut parts.mb, n),
            // The audio watchdog's, not the interface's.
            Observed::AudioDeviceList(_) => false,
        }
    }

    /// Keep one part of mpv's layout, rebuilding the whole from it when all
    /// six are known.
    fn set_osd(
        &mut self,
        part: impl FnOnce(&mut OsdParts) -> &mut Option<f64>,
        value: Option<f64>,
    ) -> bool {
        let slot = part(&mut self.osd_parts);
        let moved = set(slot, number(value));
        if moved {
            self.osd = self.osd_parts.build();
        }
        moved
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
        // The entry playing now, with the path mpv reported for it. A file
        // that failed and is then retried keeps its reason until it loads
        // again; this is the read that load asks for, so it is where the
        // reason goes.
        let playing = lists
            .playlist
            .iter()
            .find(|entry| entry.current)
            .map(|entry| entry.filename.clone());
        let listed: std::collections::HashSet<EntryId> =
            lists.playlist.iter().map(|entry| entry.id).collect();
        let mut changed = set(&mut self.tracks, lists.tracks);
        changed |= set(&mut self.playlist, lists.playlist);
        if let Some(path) = playing
            && self.path.as_ref() == Some(&path)
        {
            changed |= self.forget_failure(&path);
        }
        // A failure belongs to an entry of the list that reported it. Once
        // that entry is gone, so is anything its row would have said.
        let before = self.found.failed.len();
        self.found.failed.retain(|id, _| listed.contains(id));
        changed |= self.found.failed.len() != before;
        if let Some(progress) = lists.progress {
            changed |= set(&mut self.found.progress, progress);
        }
        changed |= set(&mut self.chapters, lists.chapters);
        changed
    }

    /// Forget a failure recorded for the file at `path`, now that it has
    /// loaded. Returns whether there was one to forget.
    fn forget_failure(&mut self, path: &MediaPath) -> bool {
        let Some(entry) = self.playlist.iter().find(|entry| entry.filename == *path) else {
            return false;
        };
        self.found.failed.remove(&entry.id).is_some()
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
        chapter
            .title
            .clone()
            .unwrap_or_else(|| fill!("Chapter {} of {}", index + 1, self.chapters.len()))
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
    pub fn video_rect(&self, width: u32, height: u32) -> Rect<Normalised> {
        let Some(OsdDims {
            w: ow,
            h: oh,
            ml,
            mt,
            mr,
            mb,
        }) = self.osd
        else {
            return self.fitted_rect(width, height);
        };
        if ow > 0.0 && oh > 0.0 && ow as u32 == width && oh as u32 == height {
            return Rect::normalised(
                (ml / ow) as f32,
                (mt / oh) as f32,
                ((ow - mr) / ow) as f32,
                ((oh - mb) / oh) as f32,
            );
        }
        self.fitted_rect(width, height)
    }

    /// The fit mpv is about to compute, computed here.
    ///
    /// `panscan` slides between fitting the video inside the window and
    /// filling the window with it, which is mpv's own definition; at 1 the
    /// video covers the window and there is no border to draw.
    pub fn fitted_rect(&self, width: u32, height: u32) -> Rect<Normalised> {
        let (vw, vh) = (self.dwidth, self.dheight);
        let (ww, wh) = (f64::from(width), f64::from(height));
        if vw <= 0.0 || vh <= 0.0 || ww <= 0.0 || wh <= 0.0 {
            return Rect::FULL;
        }
        let inside = (ww / vw).min(wh / vh);
        let outside = (ww / vw).max(wh / vh);
        let scale = inside + (outside - inside) * self.panscan.clamp(0.0, 1.0);
        // Centred, which is mpv's default and the only alignment reachable
        // from here, so both margins on an axis are the same.
        let x0 = (((ww - vw * scale) * 0.5) / ww).clamp(0.0, 1.0);
        let y0 = (((wh - vh * scale) * 0.5) / wh).clamp(0.0, 1.0);
        Rect::normalised(x0 as f32, y0 as f32, 1.0 - x0 as f32, 1.0 - y0 as f32)
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
    pub fn progress_of(&self, row: usize) -> Progress {
        self.playlist
            .get(row)
            .and_then(|entry| self.found.progress.get(&entry.filename))
            .copied()
            .unwrap_or_default()
    }

    /// Take delivery of described files. Returns whether any row changes.
    pub fn apply_probed(&mut self, found: Vec<dbm_library::probe::Found>) -> bool {
        let mut changed = false;
        for f in found {
            if self.found.probed.get(&f.path) == Some(&f) {
                continue;
            }
            changed |= self.playlist.iter().any(|e| e.filename == f.path);
            self.found.probed.insert(f.path.clone(), f);
        }
        changed
    }

    /// [`progress_of`](Self::progress_of), with a probed length filling in
    /// for one mpv has never reported.
    pub fn known_progress(&self, row: usize) -> Progress {
        let mut progress = self.progress_of(row);
        if progress.seconds.is_none()
            && let Some(found) = self.probed_for(row)
            && found.seconds > 0.0
        {
            progress.seconds = Some(found.seconds);
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
        self.found.probed.get(&self.playlist.get(row)?.filename)
    }

    /// Playback progress in 0..1, preferring the elapsed/duration ratio so it
    /// stays smooth; `percent-pos` updates less often.
    pub fn progress(&self) -> f32 {
        self.duration.map_or(0.0, |length| {
            (self.time_pos / length).clamp(0.0, 1.0) as f32
        })
    }
}

/// A number mpv reported, where it is one: a NaN or infinity would poison
/// every sum it went into.
fn number(n: Option<f64>) -> Option<f64> {
    n.filter(|n| n.is_finite())
}

/// Text, where there is any.
fn text(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
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
    fn a_layout_needs_all_six_parts_and_a_real_window() {
        assert_eq!(OsdParts::default().build(), None);
        let mut parts = OsdParts {
            w: Some(1280.0),
            h: Some(720.0),
            ml: Some(0.0),
            mt: Some(0.0),
            mr: Some(0.0),
            mb: Some(0.0),
        };
        assert_eq!(
            parts.build(),
            Some(OsdDims {
                w: 1280.0,
                h: 720.0,
                ml: 0.0,
                mt: 0.0,
                mr: 0.0,
                mb: 0.0,
            })
        );
        parts.h = Some(0.0);
        assert_eq!(parts.build(), None);
    }

    #[test]
    fn every_property_is_observed_under_its_own_index() {
        for (index, &prop) in Prop::ALL.iter().enumerate() {
            assert_eq!(prop.id(), index as u64, "{prop:?}");
            assert_eq!(Prop::of(prop.id()), Some(prop));
        }
    }

    #[test]
    fn a_change_is_read_as_its_own_property_type() {
        let read = |prop: Prop, value| Observed::of(prop.id(), Some(&value));
        assert_eq!(
            read(Prop::Pause, Value::Flag(true)),
            Some(Observed::Pause(Some(true)))
        );
        assert_eq!(
            read(Prop::Sid, Value::Str("no".into())),
            Some(Observed::Sid(Some(TrackChoice::Off)))
        );
        // Another shape is no value, rather than a misreading.
        assert_eq!(
            read(Prop::Pause, Value::Double(1.0)),
            Some(Observed::Pause(None))
        );
        assert_eq!(Observed::of(Prop::ALL.len() as u64, None), None);
    }

    fn entry(id: i64, path: &str, current: bool) -> PlaylistEntry {
        PlaylistEntry {
            index: 0,
            id: EntryId(id),
            filename: path.into(),
            title: None,
            current,
        }
    }

    #[test]
    fn a_file_that_loads_again_stops_saying_it_failed() {
        let mut state = PlayerState {
            path: Some("a.mkv".into()),
            playlist: vec![entry(7, "a.mkv", true)],
            found: Found {
                failed: HashMap::from([(EntryId(7), ErrorCode::LoadingFailed)]),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(state.observe(Observed::Path(Some("a.mkv".into()))));
        assert!(state.found.failed.is_empty());
    }

    #[test]
    fn a_failure_for_a_file_no_longer_in_the_list_is_dropped() {
        let mut state = PlayerState {
            path: Some("a.mkv".into()),
            found: Found {
                failed: HashMap::from([(EntryId(9), ErrorCode::LoadingFailed)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let lists = Lists {
            tracks: Vec::new(),
            playlist: vec![entry(7, "a.mkv", true)],
            progress: None,
            playing: Some("a.mkv".into()),
            chapters: Vec::new(),
            generation: 0,
        };
        assert!(state.apply_lists(lists));
        assert!(state.found.failed.is_empty());
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
