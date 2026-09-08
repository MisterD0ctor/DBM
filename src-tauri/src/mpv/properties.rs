//! Typed property accessors on top of `MpvPlayer::get_property`.
//!
//! Two kinds of methods live here:
//! - Fast single-property reads (`is_paused`, `volume`, …) used by commands.
//! - Aggregate fetches (`tracks`, `playlist`, `snapshot`) used by the
//!   frontend to hydrate state on mount.

use serde::Deserialize;
use shared::{PlayerSnapshot, PlaylistEntry, Track, TrackKind};

use super::{MpvPlayer, MpvResult};

#[allow(dead_code)]
impl MpvPlayer {
    // --- File state ----------------------------------------------------------

    pub fn is_file_loaded(&self) -> bool {
        self.get_property("path", "string")
            .ok()
            .and_then(|v| v.as_str().map(|s| !s.is_empty()))
            .unwrap_or(false)
    }

    // --- Single-property reads ----------------------------------------------

    pub fn is_paused(&self) -> MpvResult<bool> {
        self.get_property("pause", "flag")
            .map(|v| v.as_bool().unwrap_or(true))
    }

    pub fn is_muted(&self) -> MpvResult<bool> {
        self.get_property("mute", "flag")
            .map(|v| v.as_bool().unwrap_or(false))
    }

    pub fn time_pos(&self) -> MpvResult<f64> {
        self.get_property("time-pos", "double")
            .map(|v| v.as_f64().unwrap_or(0.0))
    }

    pub fn duration(&self) -> MpvResult<f64> {
        self.get_property("duration", "double")
            .map(|v| v.as_f64().unwrap_or(0.0))
    }

    pub fn percent_pos(&self) -> MpvResult<f64> {
        self.get_property("percent-pos", "double")
            .map(|v| v.as_f64().unwrap_or(0.0))
    }

    pub fn volume(&self) -> MpvResult<f64> {
        self.get_property("volume", "double")
            .map(|v| v.as_f64().unwrap_or(100.0))
    }

    pub fn panscan(&self) -> MpvResult<f64> {
        self.get_property("panscan", "double")
            .map(|v| v.as_f64().unwrap_or(0.0))
    }

    // --- Aggregate fetches --------------------------------------------------

    pub fn tracks(&self) -> MpvResult<Vec<Track>> {
        let raw = self.get_property("track-list", "string")?;
        // mpv returns track-list as a JSON string when fetched in "string"
        // format; it can also come back as a structured Value depending on
        // the wrapper. Handle both.
        let raws: Vec<RawTrack> = match raw {
            serde_json::Value::String(s) => serde_json::from_str(&s)?,
            other => serde_json::from_value(other)?,
        };
        Ok(raws.into_iter().filter_map(RawTrack::into_track).collect())
    }

    pub fn playlist(&self) -> MpvResult<Vec<PlaylistEntry>> {
        let raw = self.get_property("playlist", "string")?;
        let entries: Vec<PlaylistEntry> = match raw {
            serde_json::Value::String(s) => serde_json::from_str(&s)?,
            other => serde_json::from_value(other)?,
        };
        Ok(entries)
    }

    /// One-shot hydration for the frontend. Each field falls back to its
    /// default on read failure — mpv may not have every property populated
    /// before a file is loaded.
    pub fn snapshot(&self) -> MpvResult<PlayerSnapshot> {
        Ok(PlayerSnapshot {
            time_pos: self.get_property("time-pos", "double").ok().and_then(as_f64),
            duration: self.get_property("duration", "double").ok().and_then(as_f64),
            percent_pos: self
                .get_property("percent-pos", "double")
                .ok()
                .and_then(as_f64),
            paused: self.is_paused().unwrap_or(true),
            muted: self.is_muted().unwrap_or(false),
            volume: self.volume().unwrap_or(100.0),
            panscan: self.panscan().unwrap_or(0.0),
            filename: self
                .get_property("filename", "string")
                .ok()
                .and_then(as_string),
            path: self.get_property("path", "string").ok().and_then(as_string),
            sid: self.get_property("sid", "string").ok().and_then(as_string),
            aid: self.get_property("aid", "string").ok().and_then(as_string),
            sub_visibility: self
                .get_property("sub-visibility", "flag")
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            sub_delay: self
                .get_property("sub-delay", "double")
                .ok()
                .and_then(as_f64)
                .unwrap_or(0.0),
            sub_scale: self
                .get_property("sub-scale", "double")
                .ok()
                .and_then(as_f64)
                .unwrap_or(1.0),
            sub_pos: self
                .get_property("sub-pos", "double")
                .ok()
                .and_then(as_f64)
                .unwrap_or(100.0),
            border_background: self
                .get_property("border-background", "string")
                .ok()
                .and_then(as_string)
                .unwrap_or_default(),
            eof_reached: self
                .get_property("eof-reached", "flag")
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            playlist_pos: self
                .get_property("playlist-pos", "double")
                .ok()
                .and_then(|v| v.as_f64().map(|n| n as i64)),
            playlist: self.playlist().unwrap_or_default(),
            tracks: self.tracks().unwrap_or_default(),
        })
    }
}

// ---------------------------------------------------------------------------
// Raw mpv track shape (private) → `shared::Track`
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawTrack {
    id: Option<u64>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    selected: bool,
    #[serde(default)]
    default: bool,
    title: Option<String>,
    lang: Option<String>,
    codec: Option<String>,
}

impl RawTrack {
    fn into_track(self) -> Option<Track> {
        let id = self.id.and_then(|n| u32::try_from(n).ok())?;
        let kind = match self.kind.as_deref()? {
            "video" => TrackKind::Video,
            "audio" => TrackKind::Audio,
            "sub" => TrackKind::Sub,
            _ => return None,
        };
        Some(Track {
            id,
            kind,
            selected: self.selected,
            default: self.default,
            title: self.title,
            lang: self.lang,
            codec: self.codec,
        })
    }
}

fn as_f64(v: serde_json::Value) -> Option<f64> {
    v.as_f64()
}

fn as_string(v: serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}
