//! mpv's list-shaped properties, and what to make of their entries.
//!
//! `track-list`, `chapter-list`, `playlist` and `audio-device-list` are node
//! trees. Asked for as a string, mpv prints a node as JSON, so each list is
//! one read and a parse, where reading every field as a property of its own
//! (`track-list/2/lang`) would wait on mpv's core once per field of every
//! entry.
//!
//! The lists are read on demand rather than observed: the counts are
//! observed, and a change there is what triggers a re-read. The device list
//! is the exception, since only observing it keeps it current.

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::MediaPath;
use crate::value::{Format, FromMpv, Value};

/// mpv's id for a track, which `vid`, `aid` and `sid` select it by. Tracks of
/// different kinds can share one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct TrackId(pub i64);

/// As mpv's commands take it.
impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// mpv's id for a playlist entry, which stays with it however the list is
/// reordered, and is what an ended file is named by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct EntryId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
    Sub,
    #[serde(other)]
    Other,
}

/// One entry of `track-list`, by the names mpv gives its fields. A field mpv
/// leaves out reads as `None` or `false`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Track {
    pub id: TrackId,
    #[serde(rename = "type")]
    pub kind: TrackKind,
    #[serde(default, deserialize_with = "non_empty")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "non_empty")]
    pub lang: Option<String>,
    #[serde(default)]
    pub selected: bool,
    /// The file it was loaded from, where that is one of its own rather than
    /// the film's.
    #[serde(rename = "external-filename", default, deserialize_with = "non_empty")]
    pub file: Option<String>,
    /// What it is encoded as, in mpv's words: `aac`, `ass`, `subrip`.
    #[serde(default, deserialize_with = "non_empty")]
    pub codec: Option<String>,
    /// How many channels an audio track carries, where the container says.
    #[serde(rename = "demux-channel-count", default, deserialize_with = "positive")]
    pub channels: Option<i64>,
    /// Its stream's index in the file it comes from, as ffmpeg counts them.
    #[serde(rename = "ff-index", default)]
    pub ff_index: Option<i64>,
}

/// A chapter mark: where it starts, and what it is called if anything.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Chapter {
    pub time: f64,
    #[serde(default, deserialize_with = "non_empty")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlaylistEntry {
    /// Where it is in the list, which mpv says by the order alone.
    #[serde(skip)]
    pub index: usize,
    pub id: EntryId,
    pub filename: MediaPath,
    #[serde(default, deserialize_with = "non_empty")]
    pub title: Option<String>,
    /// Left out by mpv but for the one entry that is.
    #[serde(default)]
    pub current: bool,
}

impl Track {
    /// Loaded from a file of its own, rather than carried in the film.
    pub fn external(&self) -> bool {
        self.file.is_some()
    }
}

/// One entry, as `current-tracks/sub` and its kin report the selected one.
impl FromMpv for Track {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        from_json(value)
    }
}

impl FromMpv for Vec<Track> {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        from_json(value)
    }
}

impl FromMpv for Vec<Chapter> {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        from_json(value)
    }
}

impl FromMpv for Vec<PlaylistEntry> {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        let mut entries: Self = from_json(value)?;
        for (index, entry) in entries.iter_mut().enumerate() {
            entry.index = index;
        }
        Some(entries)
    }
}

/// A value printed as JSON, or `None` when it isn't the shape asked for,
/// which is said in the log.
fn from_json<T: DeserializeOwned>(value: Value) -> Option<T> {
    let Value::Str(json) = value else {
        return None;
    };
    serde_json::from_str(&json)
        .map_err(|e| log::warn!("mpv's {} didn't read ({e})", std::any::type_name::<T>()))
        .ok()
}

/// One entry of `audio-device-list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AudioDevice {
    /// mpv's name for it, which `audio-device` takes: `pipewire/…`, `wasapi/…`.
    pub name: String,
    #[serde(default)]
    pub description: String,
}

impl FromMpv for Vec<AudioDevice> {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        from_json(value)
    }
}

/// Tracks of one kind, in list order.
pub fn of_kind(tracks: &[Track], kind: TrackKind) -> Vec<&Track> {
    tracks.iter().filter(|t| t.kind == kind).collect()
}

/// The selected track of a kind, if any.
pub fn selected(tracks: &[Track], kind: TrackKind) -> Option<&Track> {
    tracks.iter().find(|t| t.kind == kind && t.selected)
}

/// Whether a subtitle codec, in mpv's name for it, is bitmaps.
pub fn is_pictures(codec: &str) -> bool {
    ["pgs", "dvd", "dvb", "xsub", "vobsub"]
        .iter()
        .any(|kind| codec.contains(kind))
}

/// Whether a subtitle codec is a styled script (ASS or SSA), whose lines and
/// signs are placed against the whole frame rather than left to the player.
pub fn is_styled(codec: &str) -> bool {
    matches!(codec, "ass" | "ssa")
}

/// A string, but `None` where it is empty, as mpv's never-set fields can be.
fn non_empty<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|s| !s.is_empty()))
}

/// A count, but `None` where it is not one.
fn positive<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Ok(Option::<i64>::deserialize(d)?.filter(|n| *n > 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_subtitles_are_told_from_text() {
        for codec in ["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"] {
            assert!(is_pictures(codec), "{codec}");
        }
        for codec in ["subrip", "ass", "webvtt", "mov_text"] {
            assert!(!is_pictures(codec), "{codec}");
        }
    }

    #[test]
    fn styled_subtitles_are_told_from_plain_text() {
        assert!(is_styled("ass") && is_styled("ssa"));
        for codec in ["subrip", "webvtt", "mov_text", "hdmv_pgs_subtitle"] {
            assert!(!is_styled(codec), "{codec}");
        }
    }

    #[test]
    fn tracks_read_from_what_mpv_prints() {
        let printed = r#"[
            {"id":1,"type":"video","src-id":0,"albumart":false,"default":true,
             "external":false,"selected":true,"codec":"hevc","demux-w":1920},
            {"id":1,"type":"audio","title":"English: Stereo","lang":"eng",
             "external":false,"selected":true,"codec":"ac3","demux-channel-count":2,
             "ff-index":1},
            {"id":3,"type":"sub","title":"","external":true,"selected":false,
             "codec":"subrip","external-filename":"/a.srt"},
            {"id":4,"type":"attachment"}
        ]"#;
        let tracks: Vec<Track> = serde_json::from_str(printed).unwrap();
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[0].kind, TrackKind::Video);
        assert_eq!(tracks[0].channels, None);
        assert_eq!(tracks[1].title.as_deref(), Some("English: Stereo"));
        assert_eq!(tracks[1].channels, Some(2));
        assert_eq!(tracks[1].ff_index, Some(1));
        assert!(!tracks[1].external(), "carried in the film");
        assert_eq!(tracks[2].file.as_deref(), Some("/a.srt"));
        assert!(tracks[2].external() && !tracks[2].selected);
        assert_eq!(tracks[2].title, None, "an empty title is no title");
        assert_eq!(tracks[2].lang, None);
        assert_eq!(tracks[3].kind, TrackKind::Other);
    }

    #[test]
    fn only_the_current_entry_says_so() {
        let printed = r#"[
            {"filename":"/a.mkv","current":true,"playing":true,"id":1},
            {"filename":"/b.mkv","title":"B","id":2}
        ]"#;
        let entries: Vec<PlaylistEntry> = serde_json::from_str(printed).unwrap();
        assert!(entries[0].current && !entries[1].current);
        assert_eq!(entries[1].title.as_deref(), Some("B"));
        assert_eq!(entries[1].id, EntryId(2));
    }

    #[test]
    fn devices_read_from_what_mpv_prints() {
        let printed = r#"[{"name":"auto","description":"Autoselect device"},
            {"name":"pulse/Kopfh\u00f6rer","description":"name"}]"#;
        let devices = Vec::<AudioDevice>::from_mpv(Value::Str(printed.into())).unwrap();
        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["auto", "pulse/Kopfhörer"]);
        for not_a_list in ["", "null", r#"[{"name":"#] {
            assert_eq!(
                Vec::<AudioDevice>::from_mpv(Value::Str(not_a_list.into())),
                None
            );
        }
    }

    #[test]
    fn playlist_entries_are_numbered_in_list_order() {
        let printed = r#"[{"filename":"/a.mkv","id":5},{"filename":"/b.mkv","id":9}]"#;
        let entries = Vec::<PlaylistEntry>::from_mpv(Value::Str(printed.into())).unwrap();
        let indices: Vec<usize> = entries.iter().map(|e| e.index).collect();
        assert_eq!(indices, [0, 1]);
    }
}
