//! Small JSON maps from a path to something known about it, kept in the
//! app's data directory.
//!
//! Three of these exist — how long each file is, its title tag, and where
//! each finished file's ending began — and they share their whole life: read
//! whole, changed one entry at a time, from more than one thread, and written
//! whole. So that life is written once, here.
//!
//! Every write re-reads the file first rather than holding the map in
//! memory. The files are a few tens of kilobytes at worst, and reading first
//! means a file edited by hand, or by another copy of the player, is merged
//! rather than clobbered.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// One of those files: a flat JSON object of path to `V`.
pub struct PathMap<V> {
    name: &'static str,
    /// Serialises read-modify-write. Each writer rewrites the whole file from
    /// what it just read, so two overlapping writes would lose one of them.
    write: Mutex<()>,
    /// `fn() -> V` rather than `V`, so the map is `Sync` — and can be a
    /// `static` — whatever `V` is.
    value: PhantomData<fn() -> V>,
}

impl<V> PathMap<V> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            write: Mutex::new(()),
            value: PhantomData,
        }
    }

    fn file(&self) -> PathBuf {
        crate::paths::app_data_dir().join(self.name)
    }
}

impl<V: Serialize + DeserializeOwned + PartialEq> PathMap<V> {
    /// The whole map.
    ///
    /// **Blocking.** Worker thread only. A missing or unreadable file is an
    /// empty map, not an error: the first run of the player has none, and
    /// neither does one whose file somebody deleted. Both mean the same thing
    /// — nothing is known yet.
    pub fn load(&self) -> HashMap<String, V> {
        read(&self.file())
    }

    /// Remember one entry, rewriting the file only if it changed.
    ///
    /// **Blocking.** Worker thread only.
    pub fn set(&self, path: &str, value: V) {
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        update(&self.file(), path, value);
    }
}

fn read<V: DeserializeOwned>(file: &Path) -> HashMap<String, V> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn update<V: Serialize + DeserializeOwned + PartialEq>(file: &Path, path: &str, value: V) {
    let mut map = read(file);
    if map.get(path) == Some(&value) {
        return;
    }
    map.insert(path.to_string(), value);
    let Ok(json) = serde_json::to_string(&map) else {
        return;
    };
    if let Err(e) = std::fs::write(file, json) {
        eprintln!("dbm: could not write {}: {e}", file.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_merged_into_what_is_on_disk() {
        let file = std::env::temp_dir().join(format!("dbm-cache-{}.json", std::process::id()));
        // Written by someone else — a hand edit, another copy of the player.
        std::fs::write(&file, r#"{"C:/films/a.mkv":1200.0}"#).unwrap();

        update(&file, "C:/films/b.mkv", 600.0);
        let map: HashMap<String, f64> = read(&file);
        assert_eq!(map.get("C:/films/a.mkv"), Some(&1200.0));
        assert_eq!(map.get("C:/films/b.mkv"), Some(&600.0));

        // Missing or not JSON is nothing known, not an error.
        std::fs::write(&file, "not json").unwrap();
        assert!(read::<f64>(&file).is_empty());
        std::fs::remove_file(&file).ok();
        assert!(read::<f64>(&file).is_empty());
    }
}
