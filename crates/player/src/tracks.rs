//! Reading mpv's list-shaped properties.
//!
//! `track-list` and `playlist` are node trees, and observing them as nodes
//! would mean marshalling the whole tree across FFI every time one changed.
//! mpv also exposes every leaf as its own scalar property though —
//! `track-list/2/lang`, `playlist/0/filename` — so these are read field by
//! field instead. Slightly chattier, but each call is a plain string and no
//! node decoding is needed anywhere in the app.
//!
//! Both lists are pulled on demand rather than observed: the counts are
//! observed, and a change there is what triggers a re-read.

use crate::mpv::Mpv;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Sub,
    Other,
}

impl TrackKind {
    fn parse(s: &str) -> Self {
        match s {
            "video" => Self::Video,
            "audio" => Self::Audio,
            "sub" => Self::Sub,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: i64,
    pub kind: TrackKind,
    pub title: Option<String>,
    pub lang: Option<String>,
    pub selected: bool,
    pub external: bool,
}

/// The tracks of one kind, with what to call each of them.
///
/// One kind at a time: the right label depends on the company a track keeps,
/// and a subtitle track keeps company with the other subtitles, not with the
/// audio. See [`crate::naming::track_labels`].
pub fn labelled(tracks: &[Track], kind: TrackKind) -> Vec<(&Track, String)> {
    let of_kind = of_kind(tracks, kind);
    let infos: Vec<crate::naming::TrackInfo> = of_kind
        .iter()
        .map(|t| crate::naming::TrackInfo {
            id: t.id,
            title: t.title.as_deref(),
            language: t.lang.as_deref(),
            external: t.external,
        })
        .collect();
    of_kind
        .into_iter()
        .zip(crate::naming::track_labels(&infos))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistEntry {
    pub index: i64,
    pub filename: String,
    pub title: Option<String>,
    pub current: bool,
}

impl PlaylistEntry {
    /// Prefer embedded metadata, then the file name read for what it says.
    ///
    /// A full path in a menu row is noise, and so is the half of a scene
    /// filename that describes the encode rather than the film — see
    /// [`crate::naming`].
    /// The container's own title, where mpv has one for this entry and it is
    /// not simply the file name it fell back to.
    pub fn embedded_title(&self) -> Option<&str> {
        self.title
            .as_deref()
            .filter(|t| *t != crate::naming::strip_path(&self.filename))
    }
}

/// Read the whole track list. Empty when nothing is loaded.
pub fn read_tracks(mpv: &Mpv) -> Vec<Track> {
    let count = mpv.get_f64("track-list/count").unwrap_or(0.0) as i64;
    (0..count.max(0))
        .filter_map(|i| {
            // Without an id there is nothing to select, so the entry is
            // useless to us even if mpv listed it.
            let id = mpv.get_f64(&format!("track-list/{i}/id"))? as i64;
            Some(Track {
                id,
                kind: TrackKind::parse(
                    &mpv.get_property(&format!("track-list/{i}/type"))
                        .unwrap_or_default(),
                ),
                title: non_empty(mpv.get_property(&format!("track-list/{i}/title"))),
                lang: non_empty(mpv.get_property(&format!("track-list/{i}/lang"))),
                selected: mpv.get_bool(&format!("track-list/{i}/selected")),
                external: mpv.get_bool(&format!("track-list/{i}/external")),
            })
        })
        .collect()
}

pub fn read_playlist(mpv: &Mpv) -> Vec<PlaylistEntry> {
    let count = mpv.get_f64("playlist/count").unwrap_or(0.0) as i64;
    (0..count.max(0))
        .map(|i| PlaylistEntry {
            index: i,
            filename: mpv
                .get_property(&format!("playlist/{i}/filename"))
                .unwrap_or_default(),
            title: non_empty(mpv.get_property(&format!("playlist/{i}/title"))),
            current: mpv.get_bool(&format!("playlist/{i}/current")),
        })
        .collect()
}

/// Tracks of one kind, in list order.
pub fn of_kind(tracks: &[Track], kind: TrackKind) -> Vec<&Track> {
    tracks.iter().filter(|t| t.kind == kind).collect()
}

/// The selected track of a kind, if any.
pub fn selected(tracks: &[Track], kind: TrackKind) -> Option<&Track> {
    tracks.iter().find(|t| t.kind == kind && t.selected)
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}
