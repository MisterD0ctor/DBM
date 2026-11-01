//! What to call the tracks and the playlist entries mpv lists.
//!
//! Here rather than beside them in `dbm_mpv::tracks`, since the names are
//! `dbm_naming`'s guesswork, which the binding has no use for.

use dbm_mpv::tracks::{PlaylistEntry, Track, TrackKind, of_kind};

/// The tracks of one kind, with what to call each of them.
///
/// One kind at a time: the right label depends on the company a track keeps,
/// and a subtitle track keeps company with the other subtitles, not with the
/// audio. See [`dbm_naming::track_labels`].
pub fn tracks(tracks: &[Track], kind: TrackKind) -> Vec<(&Track, String)> {
    let of_kind = of_kind(tracks, kind);
    let infos: Vec<dbm_naming::TrackInfo> = of_kind
        .iter()
        .map(|t| dbm_naming::TrackInfo {
            title: t.title.as_deref(),
            language: t.lang.as_deref(),
            external: t.external,
            codec: t.codec.as_deref(),
            channels: t.channels,
        })
        .collect();
    of_kind
        .into_iter()
        .zip(dbm_naming::track_labels(&infos))
        .collect()
}

/// The container's own title for an entry, where mpv has one and it is not
/// simply the file name it fell back to.
pub fn embedded_title(entry: &PlaylistEntry) -> Option<&str> {
    entry
        .title
        .as_deref()
        .filter(|t| *t != dbm_naming::strip_path(&entry.filename))
}
