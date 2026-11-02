//! Reading mpv's list-shaped properties.
//!
//! `track-list`, `chapter-list` and `playlist` are node trees. Asked for as a
//! string, mpv prints a node as JSON, so each list is one read and a parse,
//! where reading every field as a property of its own (`track-list/2/lang`)
//! would wait on mpv's core once per field of every entry.
//!
//! The lists are pulled on demand rather than observed: the counts are
//! observed, and a change there is what triggers a re-read.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::Mpv;

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
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: TrackKind,
    #[serde(default, deserialize_with = "non_empty")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "non_empty")]
    pub lang: Option<String>,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub external: bool,
    /// What it is encoded as, in mpv's words: `aac`, `ass`, `subrip`.
    #[serde(default, deserialize_with = "non_empty")]
    pub codec: Option<String>,
    /// How many channels an audio track carries, where the container says.
    #[serde(rename = "demux-channel-count", default, deserialize_with = "positive")]
    pub channels: Option<i64>,
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
    pub index: i64,
    /// mpv's own id for the entry, which stays with it however the list is
    /// reordered, and is what an ended file is named by.
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub filename: String,
    #[serde(default, deserialize_with = "non_empty")]
    pub title: Option<String>,
    /// Left out by mpv but for the one entry that is.
    #[serde(default)]
    pub current: bool,
}

/// Read the whole track list. Empty when nothing is loaded.
pub fn read_tracks(mpv: &Mpv) -> Vec<Track> {
    read_list(mpv, "track-list")
}

/// Read the chapter list. Empty for a file without chapters, which is most.
pub fn read_chapters(mpv: &Mpv) -> Vec<Chapter> {
    read_list(mpv, "chapter-list")
}

pub fn read_playlist(mpv: &Mpv) -> Vec<PlaylistEntry> {
    let mut entries: Vec<PlaylistEntry> = read_list(mpv, "playlist");
    for (index, entry) in (0..).zip(&mut entries) {
        entry.index = index;
    }
    entries
}

/// Read the list property `name` as JSON. Empty when mpv has none, and when
/// it isn't the shape asked for, which is said in the log.
fn read_list<T: DeserializeOwned>(mpv: &Mpv, name: &str) -> Vec<T> {
    let Some(json) = mpv.get_property(name) else {
        return Vec::new();
    };
    serde_json::from_str(&json).unwrap_or_else(|e| {
        log::warn!("mpv's {name} didn't read ({e})");
        Vec::new()
    })
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
    fn tracks_read_from_what_mpv_prints() {
        let printed = r#"[
            {"id":1,"type":"video","src-id":0,"albumart":false,"default":true,
             "external":false,"selected":true,"codec":"hevc","demux-w":1920},
            {"id":1,"type":"audio","title":"English: Stereo","lang":"eng",
             "external":false,"selected":true,"codec":"ac3","demux-channel-count":2},
            {"id":3,"type":"sub","title":"","external":true,"selected":false,
             "codec":"subrip"},
            {"id":4,"type":"attachment"}
        ]"#;
        let tracks: Vec<Track> = serde_json::from_str(printed).unwrap();
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[0].kind, TrackKind::Video);
        assert_eq!(tracks[0].channels, None);
        assert_eq!(tracks[1].title.as_deref(), Some("English: Stereo"));
        assert_eq!(tracks[1].channels, Some(2));
        assert!(tracks[2].external && !tracks[2].selected);
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
        assert_eq!(entries[1].id, 2);
    }
}
