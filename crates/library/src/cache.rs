//! Small JSON maps from a path to something known about it, kept in the
//! app's data directory — and folders of files kept the same way, in its
//! cache directory.
//!
//! The maps (lengths, title tags, endings, title sequences and credits,
//! downloaded subtitles) share their whole life: read whole, changed one
//! entry at a time from more than one thread, and written whole, by way of a
//! file beside it, so a crash never leaves half of one. So that is written
//! once, here.
//!
//! Every write re-reads the file first rather than holding the map in memory.
//! The files are a few tens of kilobytes at worst, and reading first merges a
//! file edited by hand, or by another copy of the player, rather than
//! clobbering it.
//!
//! Most are caches of what the player worked out, and an entry doesn't say
//! how it was reached. So each cache carries a version, written into its
//! file, and whoever changes how its entries are worked out bumps the number
//! where it is declared. Entries under any other number are forgotten and
//! worked out again when next asked for. A record, of something that happened
//! and can't be worked out again, has no version and is never forgotten.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

use dbm_mpv::MediaPath;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The version every file written before there were versions is taken to
/// hold. Its entries are kept by a cache still at it, and by every record.
const UNVERSIONED: u32 = 1;

/// One of those files: a JSON object of path to `V`.
pub struct PathMap<V> {
    name: &'static str,
    /// What its entries were reached by; see the module's comment.
    version: u32,
    /// Serialises read-modify-write. Each writer rewrites the whole file from
    /// what it just read, so two overlapping writes would lose one of them.
    write: Mutex<()>,
    /// `fn() -> V` rather than `V`, so the map is `Sync`, and can be a
    /// `static`, whatever `V` is.
    value: PhantomData<fn() -> V>,
}

impl<V> PathMap<V> {
    /// A map of what was worked out, at the `version` of the way it is
    /// worked out. Bump it, with a line on what changed, whenever that way
    /// changes enough that an old answer would now be wrong.
    pub const fn cache(name: &'static str, version: u32) -> Self {
        Self {
            name,
            version,
            write: Mutex::new(()),
            value: PhantomData,
        }
    }

    /// A map of what happened, kept whatever changes.
    pub const fn record(name: &'static str) -> Self {
        Self::cache(name, UNVERSIONED)
    }

    fn file(&self) -> PathBuf {
        crate::paths::app_data_dir().join(self.name)
    }
}

impl<V: Serialize + DeserializeOwned + PartialEq> PathMap<V> {
    /// The whole map.
    ///
    /// **Blocking.** Worker thread only. A missing file, or one kept at
    /// another version, is an empty map rather than an error: nothing is known
    /// yet. So is an unreadable one, which is first moved aside, to
    /// `<name>.unreadable`.
    pub fn load(&self) -> HashMap<MediaPath, V> {
        read(&self.file(), self.version)
    }

    /// Remember one entry, rewriting the file only if it changed.
    ///
    /// **Blocking.** Worker thread only.
    pub fn set(&self, path: &MediaPath, value: V) {
        self.edit(|kept| {
            if kept.get(path) == Some(&value) {
                return false;
            }
            kept.insert(path.clone(), value);
            true
        });
    }

    /// Change the map in one read and one write, with no other write between
    /// them. `change` says whether it changed anything, and the file is
    /// rewritten only if it did.
    ///
    /// **Blocking.** Worker thread only.
    pub fn edit(&self, change: impl FnOnce(&mut HashMap<MediaPath, V>) -> bool) {
        let _guard = self
            .write
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        edit(&self.file(), self.version, change);
    }
}

/// A map as it is written, under the version it was written at.
#[derive(Serialize, Deserialize)]
struct Versioned<V> {
    version: u32,
    entries: HashMap<MediaPath, V>,
}

fn read<V: DeserializeOwned>(file: &Path, version: u32) -> HashMap<MediaPath, V> {
    let Some(text) = std::fs::read_to_string(file).ok() else {
        return HashMap::new();
    };
    if let Ok(kept) = serde_json::from_str::<Versioned<V>>(&text) {
        return if kept.version == version {
            kept.entries
        } else {
            HashMap::new()
        };
    }
    // The bare map every file was before it had a version.
    if version == UNVERSIONED
        && let Ok(entries) = serde_json::from_str(&text)
    {
        return entries;
    }
    // Another version's entries may have had another shape, and are
    // forgotten whatever it was.
    if written_at(&text).is_some_and(|kept| kept != u64::from(version)) {
        return HashMap::new();
    }
    if !text.trim().is_empty() {
        crate::paths::set_aside(file);
    }
    HashMap::new()
}

/// The version a map was written at, judged from its shape alone: `None`
/// where it is not a map at all.
fn written_at(text: &str) -> Option<u64> {
    match serde_json::from_str(text).ok()? {
        serde_json::Value::Object(map) => match map.get("version") {
            Some(version) => version.as_u64(),
            None => Some(u64::from(UNVERSIONED)),
        },
        _ => None,
    }
}

fn edit<V: Serialize + DeserializeOwned>(
    file: &Path,
    version: u32,
    change: impl FnOnce(&mut HashMap<MediaPath, V>) -> bool,
) {
    let mut entries = read(file, version);
    if !change(&mut entries) {
        return;
    }
    let Ok(json) = serde_json::to_string(&Versioned { version, entries }) else {
        return;
    };
    if let Err(e) = crate::paths::write_atomic(file, json) {
        log::warn!("could not write {}: {e}", file.display());
    }
}

/// A folder of files worked out from others, in the app's cache directory,
/// versioned as a cache map is.
///
/// The version is a file of its own in the folder. The first time the folder
/// is asked for in a run, a different version there (or none, where the cache
/// is past its first) empties it of everything else.
pub struct Folder {
    name: &'static str,
    version: u32,
    checked: Once,
}

impl Folder {
    pub const fn cache(name: &'static str, version: u32) -> Self {
        Self {
            name,
            version,
            checked: Once::new(),
        }
    }

    /// Where it is, emptied first if what is in it was made another way.
    ///
    /// **Blocking.** Worker thread only, and only the first time per run.
    pub fn path(&self) -> PathBuf {
        let dir = crate::paths::cache_dir().join(self.name);
        self.checked.call_once(|| {
            gather(&crate::paths::app_data_dir().join(self.name), &dir);
            settle(&dir, self.version);
        });
        dir
    }
}

/// Bring over to `dir` the same folder from beside the settings, where
/// earlier versions of the player kept it. Where it can't be moved it is
/// dropped, since all it holds can be made again.
fn gather(old: &Path, dir: &Path) {
    if old == dir || !old.is_dir() {
        return;
    }
    if !dir.exists() && std::fs::rename(old, dir).is_ok() {
        log::info!("{} moved to {}", old.display(), dir.display());
    } else {
        let _ = std::fs::remove_dir_all(old);
        log::info!("{} dropped", old.display());
    }
}

/// The file in a cache folder that says which version made the rest.
const VERSION_FILE: &str = "version";

/// Make `dir` hold only what `version` made, saying so.
fn settle(dir: &Path, version: u32) {
    let say = || {
        if std::fs::create_dir_all(dir).is_ok() {
            let _ = std::fs::write(dir.join(VERSION_FILE), version.to_string());
        }
    };
    if !dir.is_dir() {
        return say();
    }
    let kept = std::fs::read_to_string(dir.join(VERSION_FILE))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(UNVERSIONED);
    if kept == version {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let _ = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
        }
    }
    log::warn!(
        "{} emptied: made by version {kept}, now {version}",
        dir.display()
    );
    say();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(file: &Path, version: u32, path: &MediaPath, value: f64) {
        edit(file, version, |kept| {
            kept.insert(path.clone(), value) != Some(value)
        });
    }

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dbm-cache-{name}-{}", std::process::id()))
    }

    #[test]
    fn an_entry_is_merged_into_what_is_on_disk() {
        let file = scratch("merge.json");
        // Written by someone else: a hand edit, another copy of the player.
        std::fs::write(
            &file,
            r#"{"version":3,"entries":{"C:/films/a.mkv":1200.0}}"#,
        )
        .unwrap();

        update(&file, 3, &"C:/films/b.mkv".into(), 600.0);
        let map: HashMap<MediaPath, f64> = read(&file, 3);
        assert_eq!(map.get("C:/films/a.mkv"), Some(&1200.0));
        assert_eq!(map.get("C:/films/b.mkv"), Some(&600.0));

        // Missing is nothing known, not an error.
        std::fs::remove_file(&file).ok();
        assert!(read::<f64>(&file, 3).is_empty());
    }

    #[test]
    fn an_unreadable_record_survives_the_next_write() {
        let file = scratch("unreadable.json");
        let aside = PathBuf::from(format!("{}.unreadable", file.display()));
        // Cut short by a crash, by a player that wrote in place.
        std::fs::write(&file, r#"{"C:/films/a.mkv":tr"#).unwrap();

        assert!(read::<bool>(&file, UNVERSIONED).is_empty());
        edit(&file, UNVERSIONED, |kept| {
            kept.insert("C:/films/b.mkv".into(), true).is_none()
        });
        assert_eq!(
            std::fs::read_to_string(&aside).unwrap(),
            r#"{"C:/films/a.mkv":tr"#
        );
        let map: HashMap<MediaPath, bool> = read(&file, UNVERSIONED);
        assert_eq!(map.get("C:/films/b.mkv"), Some(&true));
        std::fs::remove_file(&file).ok();
        std::fs::remove_file(&aside).ok();
    }

    #[test]
    fn another_versions_entries_are_forgotten_whatever_their_shape() {
        let file = scratch("reshaped.json");
        std::fs::write(
            &file,
            r#"{"version":1,"entries":{"C:/films/a.mkv":"once a word"}}"#,
        )
        .unwrap();
        assert!(read::<f64>(&file, 2).is_empty());
        assert!(!PathBuf::from(format!("{}.unreadable", file.display())).exists());
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn what_another_version_kept_is_forgotten() {
        let file = scratch("version.json");
        std::fs::write(
            &file,
            r#"{"version":1,"entries":{"C:/films/a.mkv":1200.0}}"#,
        )
        .unwrap();
        assert!(read::<f64>(&file, 2).is_empty());

        // And the next write starts over, at the version that wrote it.
        update(&file, 2, &"C:/films/b.mkv".into(), 600.0);
        let map: HashMap<MediaPath, f64> = read(&file, 2);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("C:/films/b.mkv"), Some(&600.0));
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn a_file_from_before_versions_is_the_first() {
        let file = scratch("bare.json");
        std::fs::write(&file, r#"{"C:/films/a.mkv":1200.0}"#).unwrap();
        // Kept by a record, and by a cache that has not changed since.
        let map: HashMap<MediaPath, f64> = read(&file, UNVERSIONED);
        assert_eq!(map.get("C:/films/a.mkv"), Some(&1200.0));
        // Forgotten by one that has.
        assert!(read::<f64>(&file, 2).is_empty());
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn a_folder_made_another_way_is_emptied() {
        let dir = scratch("folder");
        std::fs::create_dir_all(dir.join("half-built.tiles")).unwrap();
        std::fs::write(dir.join("a.jpg"), "old").unwrap();

        // Unversioned, and still at the first: left alone.
        settle(&dir, UNVERSIONED);
        assert!(dir.join("a.jpg").is_file());

        settle(&dir, 2);
        assert!(!dir.join("a.jpg").exists());
        assert!(!dir.join("half-built.tiles").exists());
        assert_eq!(
            std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap(),
            "2"
        );

        // Made by this version since: left alone.
        std::fs::write(dir.join("b.jpg"), "new").unwrap();
        settle(&dir, 2);
        assert!(dir.join("b.jpg").is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_folder_beside_the_settings_is_brought_over_once() {
        let (old, dir) = (scratch("beside"), scratch("cached"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("a.jpg"), "kept").unwrap();
        gather(&old, &dir);
        assert!(!old.exists());
        assert_eq!(std::fs::read_to_string(dir.join("a.jpg")).unwrap(), "kept");

        // One made again beside the settings, by an older player, is dropped
        // rather than mixed in.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("b.jpg"), "stale").unwrap();
        gather(&old, &dir);
        assert!(!old.exists());
        assert!(!dir.join("b.jpg").exists());

        // The same place both ways is left alone.
        gather(&dir, &dir);
        assert!(dir.join("a.jpg").is_file());
        std::fs::remove_dir_all(&dir).ok();
    }
}
