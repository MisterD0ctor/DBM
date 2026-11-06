//! mpv's properties, each named once with the type of its value.
//!
//! Options are properties too, so the ones only set before `mpv_initialize`
//! are here as well. A property mpv only reports is marked [`ReadOnly`],
//! which [`Mpv::set`](crate::Mpv::set) refuses at compile time.

use std::fmt;
use std::marker::PhantomData;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::MediaPath;
use crate::tracks::{AudioDevice, Chapter, PlaylistEntry, Track, TrackId};
use crate::value::{Format, FromMpv, ToMpv, Value};

/// One of mpv's properties: its name, the type of its value, and whether a
/// client may set it.
pub struct Property<T, A = Writable> {
    name: &'static str,
    _value: PhantomData<fn() -> (T, A)>,
}

/// Marks a property a client may set.
pub enum Writable {}

/// Marks a property mpv only reports.
pub enum ReadOnly {}

impl<T, A> Property<T, A> {
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            _value: PhantomData,
        }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    /// What an [`Event::Property`](crate::Event::Property) for this property
    /// says, as its type. `None` where there is no value, or not one of these.
    pub fn decode(self, value: Option<&Value>) -> Option<T>
    where
        T: FromMpv,
    {
        value.cloned().and_then(T::from_mpv)
    }
}

// By hand, since deriving would ask the same of `T` and `A`.
impl<T, A> Clone for Property<T, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, A> Copy for Property<T, A> {}

impl<T, A> fmt::Debug for Property<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// A property with its type left behind, for a table of mixed ones such as
/// the set the player observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnyProperty {
    name: &'static str,
    format: Format,
}

impl AnyProperty {
    pub const fn name(self) -> &'static str {
        self.name
    }

    pub(crate) const fn format(self) -> Format {
        self.format
    }
}

impl<T: FromMpv, A> From<Property<T, A>> for AnyProperty {
    fn from(property: Property<T, A>) -> Self {
        Self {
            name: property.name,
            format: T::FORMAT,
        }
    }
}

// ---------------------------------------------------------------------------
// Values particular to a property
// ---------------------------------------------------------------------------

/// Which track of a kind plays, as `vid`, `aid` and `sid` take it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackChoice {
    /// None, mpv's `no`.
    Off,
    /// Whichever mpv picks by language and default flags.
    Auto,
    Id(TrackId),
}

impl TrackChoice {
    pub fn id(self) -> Option<TrackId> {
        match self {
            Self::Id(id) => Some(id),
            Self::Off | Self::Auto => None,
        }
    }
}

impl FromMpv for TrackChoice {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        match String::from_mpv(value)?.as_str() {
            "no" => Some(Self::Off),
            "auto" => Some(Self::Auto),
            id => id.parse().ok().map(|id| Self::Id(TrackId(id))),
        }
    }
}

impl ToMpv for TrackChoice {
    fn to_mpv(&self) -> Value {
        Value::Str(self.to_string())
    }
}

/// As mpv spells it.
impl fmt::Display for TrackChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("no"),
            Self::Auto => f.write_str("auto"),
            Self::Id(id) => write!(f, "{id}"),
        }
    }
}

/// What mpv does when a file ends (`keep-open`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepOpen {
    /// Go on to the next entry, and close after the last.
    No,
    /// Go on to the next entry, but hold the last one open at its end.
    Yes,
    /// Hold every file open at its end, so nothing advances by itself.
    Always,
}

impl FromMpv for KeepOpen {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        match String::from_mpv(value)?.as_str() {
            "no" => Some(Self::No),
            "yes" => Some(Self::Yes),
            "always" => Some(Self::Always),
            _ => None,
        }
    }
}

impl ToMpv for KeepOpen {
    fn to_mpv(&self) -> Value {
        Value::Str(
            match self {
                Self::No => "no",
                Self::Yes => "yes",
                Self::Always => "always",
            }
            .into(),
        )
    }
}

/// Whether mpv reads ahead into memory (`cache`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cache {
    /// Only for what mpv takes to be a network stream.
    Auto,
    Yes,
    No,
}

impl ToMpv for Cache {
    fn to_mpv(&self) -> Value {
        Value::Str(
            match self {
                Self::Auto => "auto",
                Self::Yes => "yes",
                Self::No => "no",
            }
            .into(),
        )
    }
}

/// A part of the video frame, in its own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// `None` is the whole frame.
impl ToMpv for Option<Rect> {
    fn to_mpv(&self) -> Value {
        Value::Str(self.map_or_else(String::new, |r| format!("{}x{}+{}+{}", r.w, r.h, r.x, r.y)))
    }
}

/// Property names, as the options that list some take them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names(Vec<&'static str>);

impl Names {
    pub fn with<T, A>(mut self, property: Property<T, A>) -> Self {
        self.0.push(property.name);
        self
    }
}

impl ToMpv for Names {
    fn to_mpv(&self) -> Value {
        Value::Str(self.0.join(","))
    }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

pub const PAUSE: Property<bool> = Property::new("pause");
pub const SPEED: Property<f64> = Property::new("speed");
/// 0 to 100 is the source's own level; above that is amplified.
pub const VOLUME: Property<f64> = Property::new("volume");
pub const MUTE: Property<bool> = Property::new("mute");
pub const KEEP_OPEN: Property<KeepOpen> = Property::new("keep-open");
pub const CACHE: Property<Cache> = Property::new("cache");
/// The playlist entry a list loaded next starts on, from 0.
pub const PLAYLIST_START: Property<i64> = Property::new("playlist-start");

// ---------------------------------------------------------------------------
// Where playback is
// ---------------------------------------------------------------------------

pub const TIME_POS: Property<f64> = Property::new("time-pos");
pub const PERCENT_POS: Property<f64> = Property::new("percent-pos");
pub const DURATION: Property<f64, ReadOnly> = Property::new("duration");
/// From 0, and unset in a file without chapters.
pub const CHAPTER: Property<i64> = Property::new("chapter");
pub const CHAPTER_COUNT: Property<i64, ReadOnly> = Property::new("chapter-list/count");
pub const CHAPTER_LIST: Property<Vec<Chapter>, ReadOnly> = Property::new("chapter-list");
pub const EOF_REACHED: Property<bool, ReadOnly> = Property::new("eof-reached");
/// Nothing loaded and nothing on its way.
pub const IDLE_ACTIVE: Property<bool, ReadOnly> = Property::new("idle-active");

// ---------------------------------------------------------------------------
// The file and the playlist
// ---------------------------------------------------------------------------

pub const FILENAME: Property<String, ReadOnly> = Property::new("filename");
/// The file's own title where it has one, else its name.
pub const MEDIA_TITLE: Property<String, ReadOnly> = Property::new("media-title");
pub const PATH: Property<MediaPath, ReadOnly> = Property::new("path");
pub const PLAYLIST: Property<Vec<PlaylistEntry>, ReadOnly> = Property::new("playlist");
/// From 0, and -1 with nothing playing.
pub const PLAYLIST_POS: Property<i64> = Property::new("playlist-pos");
pub const PLAYLIST_COUNT: Property<i64, ReadOnly> = Property::new("playlist-count");

// ---------------------------------------------------------------------------
// Tracks
// ---------------------------------------------------------------------------

pub const TRACK_LIST: Property<Vec<Track>, ReadOnly> = Property::new("track-list");
pub const TRACK_COUNT: Property<i64, ReadOnly> = Property::new("track-list/count");
pub const VID: Property<TrackChoice> = Property::new("vid");
pub const AID: Property<TrackChoice> = Property::new("aid");
pub const SID: Property<TrackChoice> = Property::new("sid");
/// The audio track playing, unset when none is.
pub const CURRENT_AUDIO: Property<Track, ReadOnly> = Property::new("current-tracks/audio");
/// The subtitle track selected, unset when none is.
pub const CURRENT_SUB: Property<Track, ReadOnly> = Property::new("current-tracks/sub");
pub const AUDIO_DEVICE: Property<String> = Property::new("audio-device");
/// Every output device. Observing it is what starts mpv's hotplug monitor,
/// so it only stays current while observed.
pub const AUDIO_DEVICE_LIST: Property<Vec<AudioDevice>, ReadOnly> =
    Property::new("audio-device-list");
/// Added at the end of the audio chain, in mpv's filter syntax.
pub const AF: Property<String> = Property::new("af");

// ---------------------------------------------------------------------------
// Subtitles
// ---------------------------------------------------------------------------

pub const SUB_VISIBILITY: Property<bool> = Property::new("sub-visibility");
/// In seconds; positive shows them later.
pub const SUB_DELAY: Property<f64> = Property::new("sub-delay");
/// How fast they run against the film, 1 for as timed.
pub const SUB_SPEED: Property<f64> = Property::new("sub-speed");
pub const SUB_SCALE: Property<f64> = Property::new("sub-scale");
/// 0 at the top of the picture, 100 at its foot, past 100 into the letterbox.
pub const SUB_POS: Property<f64> = Property::new("sub-pos");
/// Whether text subtitles lay out against the whole window.
pub const SUB_USE_MARGINS: Property<bool> = Property::new("sub-use-margins");
/// Whether ASS subtitles lay out against the whole window.
pub const SUB_ASS_FORCE_MARGINS: Property<bool> = Property::new("sub-ass-force-margins");
/// The line on screen, as plain text.
pub const SUB_TEXT: Property<String, ReadOnly> = Property::new("sub-text");
/// An ffmetadata file whose chapters replace the file's own.
pub const CHAPTERS_FILE: Property<PathBuf> = Property::new("chapters-file");

// ---------------------------------------------------------------------------
// The picture
// ---------------------------------------------------------------------------

/// -100 to 100, 0 untouched, like the three after it.
pub const BRIGHTNESS: Property<i64> = Property::new("brightness");
pub const CONTRAST: Property<i64> = Property::new("contrast");
pub const SATURATION: Property<i64> = Property::new("saturation");
pub const GAMMA: Property<i64> = Property::new("gamma");
/// 0 fits the picture inside the window, 1 fills it by cropping.
pub const PANSCAN: Property<f64> = Property::new("panscan");
/// The part of the frame shown, for the playing file only: mpv puts the
/// whole frame back when the file ends.
pub const FILE_LOCAL_VIDEO_CROP: Property<Option<Rect>> =
    Property::new("file-local-options/video-crop");
/// The picture's size after aspect correction.
pub const DWIDTH: Property<f64, ReadOnly> = Property::new("dwidth");
pub const DHEIGHT: Property<f64, ReadOnly> = Property::new("dheight");
/// Where mpv laid the video out: the window's size, then the margins round
/// the picture, negative where it is cropped.
pub const OSD_W: Property<f64, ReadOnly> = Property::new("osd-dimensions/w");
pub const OSD_H: Property<f64, ReadOnly> = Property::new("osd-dimensions/h");
pub const OSD_ML: Property<f64, ReadOnly> = Property::new("osd-dimensions/ml");
pub const OSD_MT: Property<f64, ReadOnly> = Property::new("osd-dimensions/mt");
pub const OSD_MR: Property<f64, ReadOnly> = Property::new("osd-dimensions/mr");
pub const OSD_MB: Property<f64, ReadOnly> = Property::new("osd-dimensions/mb");

// ---------------------------------------------------------------------------
// Resuming
// ---------------------------------------------------------------------------

pub const WATCH_LATER_DIRECTORY: Property<PathBuf> = Property::new("watch-later-directory");
pub const SAVE_POSITION_ON_QUIT: Property<bool> = Property::new("save-position-on-quit");
/// What a watch-later entry records. `start` is the position.
pub const WATCH_LATER_OPTIONS: Property<Names> = Property::new("watch-later-options");
/// What goes back to its default as each file starts.
pub const RESET_ON_NEXT_FILE: Property<Names> = Property::new("reset-on-next-file");
/// Where a file starts, as a time.
pub const START: Property<String> = Property::new("start");

// ---------------------------------------------------------------------------
// Set up by `Mpv::new` alone
// ---------------------------------------------------------------------------

pub(crate) const VO: Property<String> = Property::new("vo");
pub(crate) const HWDEC: Property<String> = Property::new("hwdec");
pub(crate) const DEBAND: Property<bool> = Property::new("deband");
pub(crate) const DEBAND_ITERATIONS: Property<i64> = Property::new("deband-iterations");
/// The colour round the picture, as `r/g/b/a`.
pub(crate) const BACKGROUND_COLOR: Property<String> = Property::new("background-color");
/// What `BACKGROUND_COLOR` was called before mpv 0.38.
pub(crate) const BACKGROUND: Property<String> = Property::new("background");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_choices_read_and_write_as_mpv_spells_them() {
        for (said, choice) in [
            ("no", TrackChoice::Off),
            ("auto", TrackChoice::Auto),
            ("3", TrackChoice::Id(TrackId(3))),
        ] {
            assert_eq!(TrackChoice::from_mpv(Value::Str(said.into())), Some(choice));
            assert_eq!(choice.to_mpv(), Value::Str(said.into()));
        }
        assert_eq!(TrackChoice::from_mpv(Value::Str("banana".into())), None);
    }

    #[test]
    fn a_crop_is_written_size_first_and_none_is_the_whole_frame() {
        let rect = Rect {
            x: 10,
            y: 20,
            w: 320,
            h: 200,
        };
        assert_eq!(Some(rect).to_mpv(), Value::Str("320x200+10+20".into()));
        assert_eq!(None::<Rect>.to_mpv(), Value::Str(String::new()));
    }

    #[test]
    fn names_join_as_a_list_option_takes_them() {
        let names = Names::default().with(SUB_DELAY).with(SUB_SPEED);
        assert_eq!(names.to_mpv(), Value::Str("sub-delay,sub-speed".into()));
    }
}
