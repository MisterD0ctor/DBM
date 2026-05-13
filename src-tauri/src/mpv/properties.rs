use super::MpvPlayer;

/// Typed convenience methods for frequently-accessed mpv properties.
impl MpvPlayer {
    /// Returns true when mpv has a file loaded (path property is non-null).
    pub fn is_file_loaded(&self) -> bool {
        self.get_property("path", "string")
            .ok()
            .and_then(|v| v.as_str().map(|s| !s.is_empty()))
            .unwrap_or(false)
    }
}
