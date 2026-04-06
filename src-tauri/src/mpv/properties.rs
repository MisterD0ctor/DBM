use super::{MpvPlayer, MpvResult};

/// Typed convenience methods for frequently-accessed mpv properties.
#[allow(dead_code)]
impl MpvPlayer {
    // --- File state ----------------------------------------------------------

    /// Returns true when mpv has a file loaded (path property is non-null).
    pub fn is_file_loaded(&self) -> bool {
        self.get_property("path", "string")
            .ok()
            .and_then(|v| v.as_str().map(|s| !s.is_empty()))
            .unwrap_or(false)
    }

    // --- Playback state ------------------------------------------------------

    pub fn is_paused(&self) -> MpvResult<bool> {
        self.get_property("pause", "flag")
            .map(|v| v.as_bool().unwrap_or(true))
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

    // --- Audio ---------------------------------------------------------------

    pub fn volume(&self) -> MpvResult<f64> {
        self.get_property("volume", "double")
            .map(|v| v.as_f64().unwrap_or(100.0))
    }

    pub fn is_muted(&self) -> MpvResult<bool> {
        self.get_property("mute", "flag")
            .map(|v| v.as_bool().unwrap_or(false))
    }

    // --- Video ---------------------------------------------------------------

    pub fn panscan(&self) -> MpvResult<f64> {
        self.get_property("panscan", "double")
            .map(|v| v.as_f64().unwrap_or(0.0))
    }

    pub fn video_aspect(&self) -> MpvResult<f64> {
        self.get_property("video-params/aspect", "double")
            .map(|v| v.as_f64().unwrap_or(0.0))
    }

    // --- Track list ----------------------------------------------------------

    pub fn track_list_json(&self) -> MpvResult<String> {
        self.get_property("track-list", "string")
            .map(|v| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
    }
}
