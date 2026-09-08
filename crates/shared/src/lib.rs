//! Types shared between the Tauri backend and the Leptos frontend.
//!
//! Both sides import the same structs / enums so the IPC boundary is
//! typed end-to-end. No Tauri or WASM deps live here — keep it that way.
//!
//! Naming on the wire:
//! - Types our backend owns use snake_case (serde default).
//! - Property-change events use kebab-case to match mpv's vocabulary
//!   (`time-pos`, `playlist-pos`, …) so the existing event-name strings
//!   keep working.

use serde::{Deserialize, Serialize};

// ============================================================================
// Property change events  (Tauri channel: `mpv://property`)
// ============================================================================

/// One mpv property update. Backend builds this from the observed-property
/// callback; frontend matches on it once and writes the corresponding signal.
///
/// Serializes as `{ "name": "<kebab-case>", "data": <value> }` — same shape
/// as the old untyped `PropertyChangeEvent`, so emit code is a one-line change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name", content = "data", rename_all = "kebab-case")]
pub enum MpvProperty {
    TimePos(Option<f64>),
    Duration(Option<f64>),
    PercentPos(Option<f64>),
    Filename(Option<String>),
    Path(Option<String>),
    Pause(bool),
    Mute(bool),
    Volume(f64),
    Panscan(f64),
    Sid(Option<String>),
    Aid(Option<String>),
    SubVisibility(bool),
    /// Seconds the subtitles are shifted by. Positive shows them later.
    SubDelay(f64),
    /// Font size multiplier; 1.0 is the source's own size.
    SubScale(f64),
    /// Vertical placement, 0 (top) to 150; 100 is the default bottom.
    SubPos(f64),
    BorderBackground(String),
    EofReached(bool),
    PlaylistPos(Option<i64>),
    PlaylistCount(i64),
    /// Fires when the track list mutates — frontend re-fetches `tracks()`.
    #[serde(rename = "track-list/count")]
    TrackListCount(i64),
}

// ============================================================================
// Non-property mpv events  (Tauri channel: `mpv://event`)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum MpvEvent {
    FileLoaded,
    StartFile,
    Seek,
    EndFile { reason: FileEndReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileEndReason {
    Eof,
    Stop,
    Quit,
    Error,
    Redirect,
    Unknown,
}

// ============================================================================
// Startup snapshot  (one-shot `snapshot` command)
// ============================================================================

/// Filled in by the backend with `MpvPlayer::get_property` for each observed
/// property + a `tracks()`/`playlist()` pull. Frontend hydrates its signals
/// from this on mount, then listens for `MpvProperty` deltas.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub time_pos: Option<f64>,
    pub duration: Option<f64>,
    pub percent_pos: Option<f64>,
    pub paused: bool,
    pub muted: bool,
    pub volume: f64,
    pub panscan: f64,
    pub filename: Option<String>,
    pub path: Option<String>,
    pub sid: Option<String>,
    pub aid: Option<String>,
    pub sub_visibility: bool,
    pub sub_delay: f64,
    pub sub_scale: f64,
    pub sub_pos: f64,
    pub border_background: String,
    pub eof_reached: bool,
    pub playlist_pos: Option<i64>,
    pub playlist: Vec<PlaylistEntry>,
    pub tracks: Vec<Track>,
}

// ============================================================================
// Tracks (subtitle / audio / video)
// ============================================================================

/// Our shape, NOT mpv's wire shape. Backend translates from the raw
/// `track-list` JSON in `MpvPlayer::tracks()` so the frontend never sees the
/// long tail of mpv-specific fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: u32,
    pub kind: TrackKind,
    pub selected: bool,
    pub default: bool,
    pub title: Option<String>,
    pub lang: Option<String>,
    pub codec: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
    Sub,
}

/// What to set `sid`/`aid` to. mpv accepts numeric IDs or the sentinels
/// `"no"` / `"auto"`; the untagged repr serializes each variant naturally.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TrackSelection {
    Id(u32),
    Special(SpecialTrack),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpecialTrack {
    No,
    Auto,
}

// ============================================================================
// Playlist
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistEntry {
    pub filename: String,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub playing: bool,
    pub title: Option<String>,
}

/// Saved watch-later progress for a single file, used to draw the partial
/// progress bar on each playlist item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchProgress {
    pub start: f64,
    pub duration: f64,
}

// ============================================================================
// Seek args
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SeekMode {
    Absolute,
    Relative,
    AbsolutePercent,
    RelativePercent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeekPrecision {
    Keyframes,
    Exact,
}

impl SeekMode {
    pub fn as_mpv_str(self) -> &'static str {
        match self {
            Self::Absolute => "absolute",
            Self::Relative => "relative",
            Self::AbsolutePercent => "absolute-percent",
            Self::RelativePercent => "relative-percent",
        }
    }
}

impl SeekPrecision {
    pub fn as_mpv_str(self) -> &'static str {
        match self {
            Self::Keyframes => "keyframes",
            Self::Exact => "exact",
        }
    }
}

// ============================================================================
// Ambient (border shader)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientParam {
    pub name: String,
    pub value: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AmbientParams {
    pub enabled: bool,
    pub params: Vec<AmbientParam>,
}

// ============================================================================
// Seek preview sprite
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewReady {
    pub path: String,
    pub sprite: String,
    pub grid: u32,
    pub tile_w: u32,
    pub tile_h: u32,
}

// ============================================================================
// Error type for the IPC boundary
// ============================================================================

/// The backend's `MpvError` carries `libloading::Error`, `serde_json::Error`,
/// etc. — types that can't cross the boundary. The Tauri command layer maps
/// each variant into one of these so the frontend has a `Result<T, MpvErrorDto>`
/// it can actually use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpvErrorDto {
    pub kind: MpvErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MpvErrorKind {
    NotInitialized,
    Command,
    SetProperty,
    GetProperty,
    Io,
    Ffi,
    Other,
}

impl std::fmt::Display for MpvErrorDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for MpvErrorDto {}
