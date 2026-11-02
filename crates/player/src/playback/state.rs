//! Mirror of the mpv properties the UI cares about.
//!
//! mpv's typed event queue is drained on the UI thread straight into this
//! struct, which is then pushed into Slint properties.
//!
//! Plain fields rather than signals, since Slint's properties provide the
//! reactivity, and duplicating it here would make two sources of truth.

use dbm_lang::{fill, tr};
use dbm_mpv::tracks::{PlaylistEntry, Track};
use dbm_mpv::{Event, Format, Mpv, Value};

/// Every property the player observes. Its position in [`Prop::ALL`] is the
/// id mpv reports it under.
///
/// List-shaped properties (`track-list`, `playlist`) are absent, since
/// observing them as nodes would marshal a tree across FFI on every change.
/// Their counts are observed instead, and the list is pulled when one moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Filename,
    MediaTitle,
    Path,
    Duration,
    TimePos,
    PercentPos,
    Pause,
    Mute,
    Volume,
    EofReached,
    IdleActive,
    PlaylistPos,
    PlaylistCount,
    SubVisibility,
    SubDelay,
    SubSpeed,
    SubScale,
    Panscan,
    Speed,
    Chapter,
    ChapterCount,
    TrackCount,
    /// Selecting a different track does not change the list's length, so
    /// this and `Aid` are what keep the `selected` flags from going stale.
    Sid,
    Aid,
    /// The video's own size after aspect correction. Enough, with `panscan`,
    /// to work out where a frame will land without waiting to be told.
    Dwidth,
    Dheight,
    /// Where mpv actually laid the video out. Observed rather than polled,
    /// since a blocking read on the render path cost 220ms a frame during a
    /// resize, when mpv's core lock is contended.
    OsdW,
    OsdH,
    OsdMl,
    OsdMt,
    OsdMr,
    OsdMb,
    /// Never shown: observed because observing it is what starts mpv's
    /// hotplug monitor, which the audio watchdog needs to put sound back
    /// when a device returns (see `audio`).
    AudioDeviceList,
}

impl Prop {
    /// In declaration order, which makes each one's index its id.
    pub const ALL: [Self; 33] = [
        Self::Filename,
        Self::MediaTitle,
        Self::Path,
        Self::Duration,
        Self::TimePos,
        Self::PercentPos,
        Self::Pause,
        Self::Mute,
        Self::Volume,
        Self::EofReached,
        Self::IdleActive,
        Self::PlaylistPos,
        Self::PlaylistCount,
        Self::SubVisibility,
        Self::SubDelay,
        Self::SubSpeed,
        Self::SubScale,
        Self::Panscan,
        Self::Speed,
        Self::Chapter,
        Self::ChapterCount,
        Self::TrackCount,
        Self::Sid,
        Self::Aid,
        Self::Dwidth,
        Self::Dheight,
        Self::OsdW,
        Self::OsdH,
        Self::OsdMl,
        Self::OsdMt,
        Self::OsdMr,
        Self::OsdMb,
        Self::AudioDeviceList,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Filename => "filename",
            Self::MediaTitle => "media-title",
            Self::Path => "path",
            Self::Duration => "duration",
            Self::TimePos => "time-pos",
            Self::PercentPos => "percent-pos",
            Self::Pause => "pause",
            Self::Mute => "mute",
            Self::Volume => "volume",
            Self::EofReached => "eof-reached",
            Self::IdleActive => "idle-active",
            Self::PlaylistPos => "playlist-pos",
            Self::PlaylistCount => "playlist-count",
            Self::SubVisibility => "sub-visibility",
            Self::SubDelay => "sub-delay",
            Self::SubSpeed => "sub-speed",
            Self::SubScale => "sub-scale",
            Self::Panscan => "panscan",
            Self::Speed => "speed",
            Self::Chapter => "chapter",
            Self::ChapterCount => "chapter-list/count",
            Self::TrackCount => "track-list/count",
            Self::Sid => "sid",
            Self::Aid => "aid",
            Self::Dwidth => "dwidth",
            Self::Dheight => "dheight",
            Self::OsdW => "osd-dimensions/w",
            Self::OsdH => "osd-dimensions/h",
            Self::OsdMl => "osd-dimensions/ml",
            Self::OsdMt => "osd-dimensions/mt",
            Self::OsdMr => "osd-dimensions/mr",
            Self::OsdMb => "osd-dimensions/mb",
            Self::AudioDeviceList => "audio-device-list",
        }
    }

    const fn format(self) -> Format {
        match self {
            Self::Filename
            | Self::MediaTitle
            | Self::Path
            | Self::Sid
            | Self::Aid
            | Self::AudioDeviceList => Format::String,
            Self::Duration
            | Self::TimePos
            | Self::PercentPos
            | Self::Volume
            | Self::PlaylistPos
            | Self::PlaylistCount
            | Self::SubDelay
            | Self::SubSpeed
            | Self::SubScale
            | Self::Panscan
            | Self::Speed
            | Self::Chapter
            | Self::ChapterCount
            | Self::TrackCount
            | Self::Dwidth
            | Self::Dheight
            | Self::OsdW
            | Self::OsdH
            | Self::OsdMl
            | Self::OsdMt
            | Self::OsdMr
            | Self::OsdMb => Format::Double,
            Self::Pause
            | Self::Mute
            | Self::EofReached
            | Self::IdleActive
            | Self::SubVisibility => Format::Flag,
        }
    }

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
    for prop in Prop::ALL {
        if let Err(e) = mpv.observe(prop.id(), prop.name(), prop.format()) {
            log::warn!("cannot observe {}: {e}", prop.name());
        }
    }
}

/// Track, chapter and playlist contents, read together because they are
/// invalidated together and a single job means a single round trip.
pub struct Lists {
    pub tracks: Vec<Track>,
    pub playlist: Vec<PlaylistEntry>,
    /// One per playlist entry, in the same order: how long it is and how far
    /// in the resume point sits. Read here rather than in a job of its own,
    /// because the lookup needs the playlist's paths, which are read here.
    pub progress: Vec<dbm_library::durations::Progress>,
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
    pub path: Option<String>,
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
    pub playlist_pos: i64,
    pub playlist_count: i64,
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
    /// The chapter playing, from 0, or -1 before the first mark.
    pub chapter: i64,
    pub chapter_count: i64,
    /// Read on the worker with the tracks (see `worker`).
    pub chapters: Vec<dbm_mpv::tracks::Chapter>,
    /// Where an episode's title sequence was heard to be, in seconds, and the
    /// file that is true of (see `titles`). Kept with its path, so a new file
    /// needs nothing cleared.
    pub intro_heard: Option<(String, (f64, f64))>,
    /// Where its closing credits were heard to be, kept the same way: in
    /// pieces, with a scene of the file's own between two, or after the last
    /// where it stops short of the end.
    pub credits_heard: Option<(String, Vec<(f64, f64)>)>,
    /// Playlist entries that wouldn't play, by mpv's id, with the notice
    /// that said so, so their rows can say it again after it has gone. By id
    /// rather than path, since a file that fails at once has no path reported
    /// yet.
    pub failed: std::collections::HashMap<i64, String>,
    /// The entry that last played to its end, by mpv's id, for the frame to
    /// take: what autoplay is moving on from (see `arrival`).
    pub played_out: Option<i64>,
    pub track_count: i64,
    pub sid: Option<String>,
    pub aid: Option<String>,
    /// Bumped whenever the track list may have changed, so the lists below
    /// know to re-pull. Covers both the list mutating and the selection
    /// moving within it.
    pub tracks_generation: u64,
    /// How many files mpv has finished opening this run, which tells the
    /// driver when an open is over: when one loads, not when one exists.
    pub loads: u64,
    /// mpv's own layout: OSD size and the margins around the video.
    pub osd: [f64; 6],
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
    pub probed: std::collections::HashMap<String, dbm_library::probe::Found>,
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
                Prop::of(*id).is_some_and(|prop| self.apply_property(prop, value))
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
            Event::StartFile | Event::EndFile { .. } | Event::Seek | Event::PlaybackRestart => true,
            // Filtered out by the event drain before reaching here: command
            // completions are routed to whoever issued them.
            Event::CommandReply { .. } | Event::Shutdown => false,
        }
    }

    fn apply_property(&mut self, prop: Prop, value: &Value) -> bool {
        // `Unset` is normal (`time-pos` has no value until something plays),
        // so it clears the field rather than being an error.
        let num = match value {
            Value::Double(d) if d.is_finite() => Some(*d),
            _ => None,
        };
        let flag = matches!(value, Value::Flag(true));
        let text = match value {
            Value::Str(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };

        match prop {
            Prop::Filename => set(&mut self.filename, text),
            Prop::MediaTitle => set(&mut self.media_title, text),
            Prop::Path => set(&mut self.path, text),
            Prop::Duration => set(&mut self.duration, num.unwrap_or(0.0)),
            Prop::TimePos => set(&mut self.time_pos, num.unwrap_or(0.0)),
            Prop::PercentPos => set(&mut self.percent_pos, num.unwrap_or(0.0)),
            Prop::Pause => set(&mut self.paused, flag),
            Prop::Mute => set(&mut self.muted, flag),
            Prop::Volume => set(&mut self.volume, num.unwrap_or(0.0)),
            Prop::EofReached => set(&mut self.eof_reached, flag),
            Prop::IdleActive => set(&mut self.idle, flag),
            Prop::PlaylistPos => set(&mut self.playlist_pos, num.unwrap_or(-1.0) as i64),
            Prop::PlaylistCount => {
                let moved = set(&mut self.playlist_count, num.unwrap_or(0.0) as i64);
                self.lists_moved(moved)
            }
            Prop::SubVisibility => set(&mut self.sub_visibility, flag),
            Prop::SubDelay => set(&mut self.sub_delay, num.unwrap_or(0.0)),
            Prop::SubSpeed => set(&mut self.sub_speed, num.unwrap_or(1.0)),
            Prop::SubScale => set(&mut self.sub_scale, num.unwrap_or(1.0)),
            Prop::Panscan => set(&mut self.panscan, num.unwrap_or(0.0)),
            Prop::Speed => set(&mut self.speed, num.unwrap_or(1.0)),
            Prop::Chapter => set(&mut self.chapter, num.map_or(-1, |n| n as i64)),
            Prop::ChapterCount => {
                let moved = set(&mut self.chapter_count, num.unwrap_or(0.0) as i64);
                self.lists_moved(moved)
            }
            Prop::Sid => {
                let moved = set(&mut self.sid, text);
                self.lists_moved(moved)
            }
            Prop::Aid => {
                let moved = set(&mut self.aid, text);
                self.lists_moved(moved)
            }
            Prop::Dwidth => set(&mut self.dwidth, num.unwrap_or(0.0)),
            Prop::Dheight => set(&mut self.dheight, num.unwrap_or(0.0)),
            Prop::OsdW => set(&mut self.osd[0], num.unwrap_or(0.0)),
            Prop::OsdH => set(&mut self.osd[1], num.unwrap_or(0.0)),
            Prop::OsdMl => set(&mut self.osd[2], num.unwrap_or(0.0)),
            Prop::OsdMt => set(&mut self.osd[3], num.unwrap_or(0.0)),
            Prop::OsdMr => set(&mut self.osd[4], num.unwrap_or(0.0)),
            Prop::OsdMb => set(&mut self.osd[5], num.unwrap_or(0.0)),
            Prop::TrackCount => {
                let n = num.unwrap_or(0.0) as i64;
                let moved = set(&mut self.track_count, n);
                self.lists_moved(moved)
            }
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
    pub fn playlist_paths(&self) -> Vec<String> {
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
            fill(
                tr("Chapter {} of {}"),
                &[&(index + 1), &self.chapters.len()],
            )
        })
    }

    /// Whether the closing credits are playing: the chapter playing is the
    /// one they start at (see [`credits_chapter`](Self::credits_chapter)) or
    /// more of them after it. Where the chapters don't say, the credits are
    /// where they were heard, if they were.
    ///
    /// An episode's, which can go on to the next; and a film's only where a
    /// scene follows them, which is somewhere to skip to that is the film's
    /// own.
    pub fn credits_rolling(&self) -> bool {
        match self.credits_playing() {
            Some(scene) => scene.is_some() || self.is_episode(),
            None => false,
        }
    }

    /// Where the scene after the credits playing starts, where one does:
    /// what skipping them lands on, whether they stop for it and go on
    /// after, or end with it.
    pub fn after_credits(&self) -> Option<f64> {
        self.credits_playing().flatten()
    }

    /// Whether credits are playing, and if so where they next stop for a
    /// scene of the file's own: `Some(None)` where they run to the end.
    ///
    /// By chapters, the credits are their first chapter and any after it
    /// named for more of them or for a look at the next episode, which is
    /// more of the way on; any other is a scene. Heard, they are the pieces
    /// of music heard (see `titles`), and the skip lands `SCENE_LEAD` before
    /// a piece stops, since a scene can start under the song's last bars.
    fn credits_playing(&self) -> Option<Option<f64>> {
        if let Some(first) = self.credits_chapter() {
            let ending = |at: usize| {
                at == first
                    || self.chapters[at].title.as_deref().is_some_and(|title| {
                        dbm_naming::is_credits(title) || dbm_naming::is_preview(title)
                    })
            };
            let current = usize::try_from(self.chapter).ok()?;
            if current < first || current >= self.chapters.len() || !ending(current) {
                return None;
            }
            let scene = (current + 1..self.chapters.len()).find(|&at| !ending(at));
            return Some(scene.map(|at| self.chapters[at].time));
        }
        let pieces = self.credits_heard_pieces()?;
        let last = pieces.len() - 1;
        pieces.iter().enumerate().find_map(|(at, &(start, end))| {
            let followed = at < last || end + LENGTH_SLACK < self.duration;
            let scene = followed.then_some(end - SCENE_LEAD);
            (self.time_pos >= start && scene.is_none_or(|scene| self.time_pos < scene))
                .then_some(scene)
        })
    }

    fn is_episode(&self) -> bool {
        self.path.as_deref().is_some_and(|path| {
            matches!(dbm_naming::parse(path), dbm_naming::Media::Episode { .. })
        })
    }

    /// Whether the chapters say where the credits are, by either of the
    /// ways [`credits_chapter`](Self::credits_chapter) reads them. Where
    /// they do there is nothing to listen for.
    pub fn credits_marked(&self) -> bool {
        self.credits_chapter().is_some()
    }

    /// Where this file's credits were heard to be, in pieces. In its second
    /// half, like everything else that is taken for an ending.
    fn credits_heard_pieces(&self) -> Option<&[(f64, f64)]> {
        let path = self.path.as_deref()?;
        let (of, pieces) = self.credits_heard.as_ref()?;
        let starts = pieces.first()?.0;
        (of == path && self.duration > 0.0 && starts >= self.duration / 2.0)
            .then_some(pieces.as_slice())
    }

    /// The chapter the closing credits start at, where the chapters say.
    ///
    /// By name first: the first chapter in the file's second half called
    /// something like `End Credits`. The half keeps a cold open's `Credits`
    /// from counting.
    ///
    /// Plenty of releases name nothing, and a broadcast episode is split at
    /// its act breaks, the last of which is the credits. So where no chapter
    /// has a real name, the last is the credits if it is short: a final act
    /// runs minutes, a closing sequence well under `CREDITS_TAIL`.
    fn credits_chapter(&self) -> Option<usize> {
        if self.duration <= 0.0 {
            return None;
        }
        let named = self.chapters.iter().position(|c| {
            c.time >= self.duration / 2.0 && c.title.as_deref().is_some_and(dbm_naming::is_credits)
        });
        if named.is_some() {
            return named;
        }
        let unnamed = self.chapters.iter().all(|c| {
            c.title
                .as_deref()
                .is_none_or(dbm_naming::is_unnamed_chapter)
        });
        let last = self.chapters.len().checked_sub(1)?;
        let tail = self.duration - self.chapters[last].time;
        (unnamed && last > 0 && tail > 0.0 && tail <= CREDITS_TAIL).then_some(last)
    }

    /// Where the playing episode's title sequence starts and ends, in
    /// seconds: as its chapters name it, and otherwise as it was heard.
    ///
    /// An episode's only, since a film's opening titles are part of the film,
    /// as its credits are (see [`credits_rolling`](Self::credits_rolling)).
    pub fn intro(&self) -> Option<(f64, f64)> {
        let path = self.path.as_deref()?;
        let span = self.intro_named().or_else(|| {
            self.intro_heard
                .as_ref()
                .filter(|(of, _)| of == path)
                .map(|(_, span)| *span)
        })?;
        // Last, being the one that parses a name.
        matches!(dbm_naming::parse(path), dbm_naming::Media::Episode { .. }).then_some(span)
    }

    /// The title sequence where a chapter is named for it: from that
    /// chapter's mark to the next one.
    ///
    /// In the file's first half, which keeps an oddly named `Ending Theme`
    /// out, and no longer than `INTRO_LONGEST`, since an `Intro` that runs
    /// ten minutes is an introduction, not a title sequence. The last chapter
    /// has no next mark to end at, so isn't one either.
    pub fn intro_named(&self) -> Option<(f64, f64)> {
        if self.duration <= 0.0 {
            return None;
        }
        let at = self.chapters.iter().position(|c| {
            c.time < self.duration / 2.0 && c.title.as_deref().is_some_and(dbm_naming::is_intro)
        })?;
        let (start, end) = (self.chapters[at].time, self.chapters.get(at + 1)?.time);
        (end > start && end - start <= INTRO_LONGEST).then_some((start, end))
    }

    /// Whether the title sequence is playing, with enough of it left to be
    /// worth skipping.
    pub fn intro_rolling(&self) -> bool {
        self.intro()
            .is_some_and(|(start, end)| self.time_pos >= start && self.time_pos < end - INTRO_TAIL)
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
            .as_deref()
            .or(self.filename.as_deref())
            .unwrap_or("")
    }

    /// The container's own title, where it has one.
    ///
    /// mpv reports the file name in `media-title` when the container has no
    /// title of its own, and that fallback isn't a title.
    fn embedded_title(&self) -> Option<&str> {
        let title = self.media_title.as_deref()?;
        let name = dbm_naming::strip_path(self.path.as_deref().unwrap_or(""));
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

/// The longest last chapter that is taken for credits when no chapter is
/// named, in seconds.
const CREDITS_TAIL: f64 = 180.0;
/// How far before the music under heard credits stops a skip past them
/// lands, in seconds. A scene after the credits can start under the song's
/// last bars, and a moment more of the credits costs less than its start.
const SCENE_LEAD: f64 = 3.0;
/// How far short of the file's end heard credits have to stop to be taken
/// for stopping there, in seconds: the length of a file read once by ffmpeg
/// and once by mpv can differ by a little. Credits that stop for a scene
/// stop well short (see `titles`).
const LENGTH_SLACK: f64 = 1.0;

/// The longest chapter named for a title sequence that is taken for one,
/// in seconds.
const INTRO_LONGEST: f64 = 300.0;
/// How much of a title sequence has to be left for skipping it to be
/// offered, in seconds. Less, and it is over before the offer is read.
const INTRO_TAIL: f64 = 3.0;

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
        for (index, prop) in Prop::ALL.into_iter().enumerate() {
            assert_eq!(prop.id(), index as u64, "{prop:?}");
            assert_eq!(Prop::of(prop.id()), Some(prop));
        }
    }
    use dbm_mpv::tracks::Chapter;

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
    const MARKS: &[(f64, &str)] = &[
        (0.0, "Episode"),
        (3400.0, "End Credits"),
        (3550.0, "Preview"),
    ];

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
        let acts = &[
            (0.0, "Chapter 01"),
            (1800.0, "Chapter 02"),
            (3000.0, "Chapter 03"),
        ];
        assert!(!playing(EPISODE, 2, acts).credits_rolling());
    }

    #[test]
    fn named_chapters_are_taken_at_their_word() {
        // Short, last, and called something else: not a guess worth making.
        let marks = &[(0.0, "Episode"), (3500.0, "Preview")];
        assert!(!playing(EPISODE, 1, marks).credits_rolling());
    }

    const OPENED: &[(f64, &str)] = &[
        (0.0, "Cold Open"),
        (95.0, "Opening"),
        (185.0, "Part A"),
        (3400.0, "Ending"),
    ];

    fn at(mut state: PlayerState, seconds: f64) -> PlayerState {
        state.time_pos = seconds;
        state
    }

    #[test]
    fn a_chapter_named_for_the_titles_is_the_title_sequence() {
        let state = playing(EPISODE, 1, OPENED);
        assert_eq!(state.intro(), Some((95.0, 185.0)));
        assert!(!at(playing(EPISODE, 0, OPENED), 94.0).intro_rolling());
        assert!(at(playing(EPISODE, 1, OPENED), 95.0).intro_rolling());
        assert!(at(playing(EPISODE, 1, OPENED), 181.0).intro_rolling());
        // Three seconds from its end there is nothing left to skip.
        assert!(!at(playing(EPISODE, 1, OPENED), 183.0).intro_rolling());
        assert!(!at(playing(EPISODE, 2, OPENED), 185.0).intro_rolling());
    }

    #[test]
    fn a_film_keeps_its_opening_titles() {
        assert_eq!(playing("Some Film (2019).mkv", 1, OPENED).intro(), None);
    }

    #[test]
    fn an_introduction_is_not_a_title_sequence() {
        let marks = &[(0.0, "Intro"), (600.0, "Act One")];
        assert_eq!(playing(EPISODE, 0, marks).intro(), None);
        // Nor is the last chapter, which has nowhere to end.
        let marks = &[(0.0, "Episode"), (60.0, "Opening")];
        assert_eq!(playing(EPISODE, 1, marks).intro(), None);
    }

    #[test]
    fn a_sequence_that_was_heard_stands_in_where_no_chapter_names_one() {
        let mut state = at(playing(EPISODE, 1, ACTS), 280.0);
        assert!(!state.intro_rolling());
        state.intro_heard = Some((EPISODE.into(), (274.1, 308.9)));
        assert!(state.intro_rolling());
        // Heard in another file, it says nothing about this one.
        state.intro_heard = Some(("Show.Name.S01E03.1080p.mkv".into(), (274.1, 308.9)));
        assert!(!state.intro_rolling());
    }

    #[test]
    fn chapters_are_believed_over_what_was_heard() {
        let mut state = playing(EPISODE, 1, OPENED);
        state.intro_heard = Some((EPISODE.into(), (10.0, 40.0)));
        assert_eq!(state.intro(), Some((95.0, 185.0)));
    }

    #[test]
    fn credits_that_were_heard_roll_where_no_chapter_marks_them() {
        let mut state = at(playing(EPISODE, -1, &[]), 3500.0);
        assert!(!state.credits_rolling());
        state.credits_heard = Some((EPISODE.into(), vec![(3480.0, 3600.0)]));
        assert!(state.credits_rolling());
        assert!(!at(state.clone(), 3470.0).credits_rolling());
        // Heard in another file, or in a film, they say nothing here.
        state.credits_heard = Some(("Show.Name.S01E03.1080p.mkv".into(), vec![(3480.0, 3600.0)]));
        assert!(!state.credits_rolling());
        let mut film = at(playing("Some Film (2019).mkv", -1, &[]), 3500.0);
        film.credits_heard = Some(("Some Film (2019).mkv".into(), vec![(3480.0, 3600.0)]));
        assert!(!film.credits_rolling());
    }

    #[test]
    fn a_scene_heard_after_the_credits_is_skipped_to() {
        let mut state = at(playing(EPISODE, -1, &[]), 3500.0);
        state.credits_heard = Some((EPISODE.into(), vec![(3480.0, 3560.0)]));
        // A moment before the music stops, which the scene may start under.
        assert_eq!(state.after_credits(), Some(3557.0));
        assert!(state.credits_rolling());
        // From there the scene is the episode's, and nothing is offered.
        assert!(!at(state.clone(), 3557.0).credits_rolling());
        // Credits that run to the end have nothing after them, though the
        // length they were heard against is a little off mpv's.
        state.credits_heard = Some((EPISODE.into(), vec![(3480.0, 3599.6)]));
        assert_eq!(state.after_credits(), None);
        assert!(at(state, 3590.0).credits_rolling());
    }

    #[test]
    fn credits_heard_to_stop_for_a_scene_skip_to_it_and_go_on_after() {
        let mut state = at(playing(EPISODE, -1, &[]), 3320.0);
        state.credits_heard = Some((EPISODE.into(), vec![(3300.0, 3350.0), (3440.0, 3600.0)]));
        assert_eq!(state.after_credits(), Some(3347.0));
        assert!(state.credits_rolling());
        // The scene is the episode's.
        assert!(!at(state.clone(), 3400.0).credits_rolling());
        // The rest of the credits run to the end, and go on from there.
        let resumed = at(state, 3450.0);
        assert!(resumed.credits_rolling());
        assert_eq!(resumed.after_credits(), None);
    }

    /// Credits, a scene after them, and a look at the next episode.
    const SCENE: &[(f64, &str)] = &[
        (0.0, "Episode"),
        (3400.0, "End Credits"),
        (3500.0, "Post-Credits Scene"),
        (3550.0, "Preview"),
    ];

    #[test]
    fn a_chapter_after_the_credits_is_a_scene_to_skip_to() {
        let state = playing(EPISODE, 1, SCENE);
        assert!(state.credits_rolling());
        assert_eq!(state.after_credits(), Some(3500.0));
        assert!(!playing(EPISODE, 2, SCENE).credits_rolling());
        // The preview after the scene is the way on again.
        let preview = playing(EPISODE, 3, SCENE);
        assert!(preview.credits_rolling());
        assert_eq!(preview.after_credits(), None);
        // A preview straight after the credits is more of the way on.
        assert_eq!(playing(EPISODE, 1, MARKS).after_credits(), None);
    }

    #[test]
    fn a_film_has_its_credits_skipped_only_to_a_scene_after_them() {
        assert!(playing("Some Film (2019).mkv", 1, SCENE).credits_rolling());
        assert!(!playing("Some Film (2019).mkv", 1, MARKS).credits_rolling());
    }

    #[test]
    fn chapters_are_believed_over_credits_that_were_heard() {
        let mut state = at(playing(EPISODE, 0, MARKS), 3300.0);
        state.credits_heard = Some((EPISODE.into(), vec![(3200.0, 3600.0)]));
        assert!(state.credits_marked());
        assert!(!state.credits_rolling());
    }

    #[test]
    fn nothing_rolls_before_the_length_is_known() {
        let mut state = playing(EPISODE, 1, MARKS);
        state.duration = 0.0;
        assert!(!state.credits_rolling());
    }
}
