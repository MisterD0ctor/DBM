//! What a file to play is called, the one way.

use std::borrow::Borrow;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A file to play, spelled as mpv is given it and reports it back.
///
/// Everything remembered about a file is filed under this spelling, because
/// mpv names its watch-later files by a hash of it, so a file is only ever
/// the same file when the text is the same. Cloning shares the text, which
/// matters for a value copied into every message to the worker.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaPath(Arc<str>);

impl MediaPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&*self.0)
    }
}

impl From<&str> for MediaPath {
    fn from(text: &str) -> Self {
        Self(text.into())
    }
}

impl From<String> for MediaPath {
    fn from(text: String) -> Self {
        Self(text.into())
    }
}

/// As a playlist written for mpv spells it: a name that isn't Unicode loses
/// the bytes that aren't.
impl From<&Path> for MediaPath {
    fn from(path: &Path) -> Self {
        Self(path.to_string_lossy().into())
    }
}

impl AsRef<Path> for MediaPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

/// So a map keyed by it can be asked with the text alone.
impl Borrow<str> for MediaPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MediaPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for MediaPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

/// As the text alone, so a JSON map can be keyed by it.
impl Serialize for MediaPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MediaPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_keyed_by_it_reads_and_writes_as_plain_json() {
        let json = r#"{"/films/a.mkv":1.5}"#;
        let map: std::collections::HashMap<MediaPath, f64> = serde_json::from_str(json).unwrap();
        assert_eq!(map[&MediaPath::from("/films/a.mkv")], 1.5);
        assert_eq!(serde_json::to_string(&map).unwrap(), json);
    }
}
