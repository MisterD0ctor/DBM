//! Small JSON maps from a path to something known about it, kept in the
//! app's data directory — and folders of files kept the same way.
//!
//! The maps (lengths, title tags, endings, title sequences and credits,
//! downloaded subtitles) share their whole life: read whole, changed one
//! entry at a time from more than one thread, and written whole. So that is
//! written once, here.
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
    /// **Blocking.** Worker thread only. A missing or unreadable file, or one
    /// kept at another version, is an empty map rather than an error: nothing
    /// is known yet.
    pub fn load(&self) -> HashMap<String, V> {
        read(&self.file(), self.version)
    }

    /// Remember one entry, rewriting the file only if it changed.
    ///
    /// **Blocking.** Worker thread only.
    pub fn set(&self, path: &str, value: V) {
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        update(&self.file(), self.version, path, value);
    }
}

/// A map as it is written, under the version it was written at.
#[derive(Serialize, Deserialize)]
struct Versioned<V> {
    version: u32,
    entries: HashMap<String, V>,
}

fn read<V: DeserializeOwned>(file: &Path, version: u32) -> HashMap<String, V> {
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
    match serde_json::from_str(&text) {
        Ok(entries) if version == UNVERSIONED => entries,
        _ => HashMap::new(),
    }
}

fn update<V: Serialize + DeserializeOwned + PartialEq>(
    file: &Path,
    version: u32,
    path: &str,
    value: V,
) {
    let mut entries = read(file, version);
    if entries.get(path) == Some(&value) {
        return;
    }
    entries.insert(path.to_string(), value);
    let Ok(json) = serde_json::to_string(&Versioned { version, entries }) else {
        return;
    };
    if let Err(e) = std::fs::write(file, json) {
        eprintln!("dbm: could not write {}: {e}", file.display());
    }
}

/// A folder of files worked out from others, in the app's data directory,
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
        let dir = crate::paths::app_data_dir().join(self.name);
        self.checked.call_once(|| settle(&dir, self.version));
        dir
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
    eprintln!(
        "dbm: {} emptied: made by version {kept}, now {version}",
        dir.display()
    );
    say();
}

#[cfg(test)]
mod tests {
    use super::*;

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

        update(&file, 3, "C:/films/b.mkv", 600.0);
        let map: HashMap<String, f64> = read(&file, 3);
        assert_eq!(map.get("C:/films/a.mkv"), Some(&1200.0));
        assert_eq!(map.get("C:/films/b.mkv"), Some(&600.0));

        // Missing or not JSON is nothing known, not an error.
        std::fs::write(&file, "not json").unwrap();
        assert!(read::<f64>(&file, 3).is_empty());
        std::fs::remove_file(&file).ok();
        assert!(read::<f64>(&file, 3).is_empty());
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
        update(&file, 2, "C:/films/b.mkv", 600.0);
        let map: HashMap<String, f64> = read(&file, 2);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("C:/films/b.mkv"), Some(&600.0));
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn a_file_from_before_versions_is_the_first() {
        let file = scratch("bare.json");
        std::fs::write(&file, r#"{"C:/films/a.mkv":1200.0}"#).unwrap();
        // Kept by a record, and by a cache that has not changed since.
        let map: HashMap<String, f64> = read(&file, UNVERSIONED);
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
}
