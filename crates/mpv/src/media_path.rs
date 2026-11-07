//! What a file to play is called, the one way.
//!
//! A file reached two ways, through a link or by its real path, would be two
//! files to everything that remembers one, so a [`MediaPath`] is only made
//! from a [`RealPath`], whose every link has been followed, or from mpv
//! reporting back one it was given.

use std::borrow::Borrow;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
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

    /// One mpv reported, which is one the player gave it.
    pub(crate) fn reported(text: String) -> Self {
        Self(text.into())
    }

    /// Any text, for a test that needs a file it never reads.
    #[cfg(any(test, feature = "testing"))]
    pub fn for_test(text: &str) -> Self {
        Self(text.into())
    }
}

/// As a playlist written for mpv spells it: a name that isn't Unicode loses
/// the bytes that aren't.
impl From<&RealPath> for MediaPath {
    fn from(path: &RealPath) -> Self {
        Self(path.0.to_string_lossy().into())
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

/// Read back as it was written, which was from a [`MediaPath`]: the
/// player's own records, and mpv's lists of what it was given.
impl<'de> Deserialize<'de> for MediaPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|text| Self(text.into()))
    }
}

/// A path with every link in it followed, so one file has one.
///
/// A path a person hands the player goes through
/// `dbm_library::playlist::prepare`, which asks the document portal where it
/// really is before following it here.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RealPath(PathBuf);

impl RealPath {
    /// `path` with every link in it followed. Fails where nothing is there.
    /// **Blocking**: it asks the disk about every part of the path.
    pub fn of(path: &Path) -> io::Result<Self> {
        fs::canonicalize(path).map(|real| Self(plain(real)))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// The files and folders directly in this folder, each at its real path.
    ///
    /// Only a link costs a lookup of its own, since an entry that isn't one
    /// is really where this folder really is, so a folder of thousands on a
    /// network share, or on Windows, where following a path opens it, costs
    /// none. A dangling link, and anything that is neither a file nor a
    /// folder, is passed over.
    pub fn entries(&self) -> io::Result<impl Iterator<Item = Entry> + '_> {
        Ok(fs::read_dir(&self.0)?
            .filter_map(Result::ok)
            .filter_map(|entry| self.entry(&entry)))
    }

    fn entry(&self, entry: &fs::DirEntry) -> Option<Entry> {
        let name = entry.file_name();
        let kind = entry.file_type().ok()?;
        let (real, kind) = if kind.is_symlink() {
            let real = Self::of(&self.0.join(&name)).ok()?;
            let kind = fs::metadata(&real.0).ok()?.file_type();
            (real, kind)
        } else {
            (Self(self.0.join(&name)), kind)
        };
        (kind.is_dir() || kind.is_file()).then_some(Entry {
            name,
            real,
            folder: kind.is_dir(),
        })
    }
}

/// One file or folder in a folder listed by [`RealPath::entries`].
pub struct Entry {
    /// What it is called in the folder, which a link can differ from what it
    /// points at.
    pub name: OsString,
    pub real: RealPath,
    pub folder: bool,
}

/// A path without the verbatim prefix `canonicalize` puts on every path on
/// Windows (`\\?\C:\…`, `\\?\UNC\server\share\…`), which mpv and the interface
/// would show as it is. Other verbatim paths, such as a volume's GUID, mean
/// nothing without it and are left whole.
fn plain(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{share}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(drive) if drive.as_bytes().get(1) == Some(&b':') => PathBuf::from(drive),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_keyed_by_it_reads_and_writes_as_plain_json() {
        let json = r#"{"/films/a.mkv":1.5}"#;
        let map: std::collections::HashMap<MediaPath, f64> = serde_json::from_str(json).unwrap();
        assert_eq!(map[&MediaPath::for_test("/films/a.mkv")], 1.5);
        assert_eq!(serde_json::to_string(&map).unwrap(), json);
    }

    #[test]
    fn a_windows_verbatim_path_is_written_as_a_person_would() {
        let plain_text = |text: &str| plain(PathBuf::from(text)).to_string_lossy().into_owned();
        assert_eq!(plain_text(r"\\?\C:\Films\a.mkv"), r"C:\Films\a.mkv");
        assert_eq!(plain_text(r"\\?\UNC\nas\share\a.mkv"), r"\\nas\share\a.mkv");
        // Meaningless without the prefix.
        let volume = r"\\?\Volume{0b1c}\a.mkv";
        assert_eq!(plain_text(volume), volume);
        assert_eq!(plain_text("/films/a.mkv"), "/films/a.mkv");
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_lists_its_entries_where_they_really_are() {
        let root = std::env::temp_dir().join(format!("dbm-real-entries-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (films, elsewhere) = (root.join("films"), root.join("elsewhere"));
        fs::create_dir_all(&films).unwrap();
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(films.join("a.mkv"), b"").unwrap();
        fs::write(elsewhere.join("b.mkv"), b"").unwrap();
        std::os::unix::fs::symlink(elsewhere.join("b.mkv"), films.join("link.mkv")).unwrap();
        std::os::unix::fs::symlink(root.join("gone.mkv"), films.join("dangling.mkv")).unwrap();
        // The folder itself reached through a link.
        std::os::unix::fs::symlink(&films, root.join("shelf")).unwrap();

        let shelf = RealPath::of(&root.join("shelf")).unwrap();
        assert_eq!(shelf, RealPath::of(&films).unwrap());
        let mut found: Vec<(String, PathBuf)> = shelf
            .entries()
            .unwrap()
            .map(|e| (e.name.to_string_lossy().into_owned(), e.real.0))
            .collect();
        found.sort();
        let real_root = fs::canonicalize(&root).unwrap();
        assert_eq!(
            found,
            [
                ("a.mkv".into(), real_root.join("films/a.mkv")),
                ("link.mkv".into(), real_root.join("elsewhere/b.mkv")),
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }
}
