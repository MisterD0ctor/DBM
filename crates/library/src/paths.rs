//! Where the app keeps its own files.
//!
//! Matches the Tauri build's location on Windows so a future migration can
//! find watch-later data and settings where it left them.

use std::path::{Path, PathBuf};

const APP_DIR: &str = "Death by MPV";

/// Per-user application data directory, created on demand.
///
/// Falls back to the temp directory rather than failing: a missing config
/// directory should cost a saved position, not the ability to play a file.
pub fn app_data_dir() -> PathBuf {
    // `APPDATA` wins on every platform, not just Windows, so a harness run can
    // be pointed at a scratch directory wherever it runs.
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| xdg("XDG_CONFIG_HOME", ".config"));
    #[cfg(test)]
    let base = TEST_BASE.with_borrow(Clone::clone).or(base);
    made(base.unwrap_or_else(std::env::temp_dir).join(APP_DIR))
}

/// Per-user directory for what the player made and can make again, which a
/// person may empty to win back the space. On Linux that is under
/// `XDG_CACHE_HOME`. Elsewhere, and wherever `APPDATA` has put everything in
/// one place, it is [`app_data_dir`].
pub fn cache_dir() -> PathBuf {
    let apart = std::env::var_os("APPDATA").is_none();
    #[cfg(test)]
    let apart = apart && TEST_BASE.with_borrow(Option::is_none);
    match apart.then(|| xdg("XDG_CACHE_HOME", ".cache")).flatten() {
        Some(base) => made(base.join(APP_DIR)),
        None => app_data_dir(),
    }
}

/// An XDG base directory: `var`, or `fallback` in the home directory where
/// it is unset. `None` on Windows, which has none.
fn xdg(var: &str, fallback: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    std::env::var_os(var)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
}

/// `dir`, made if it was missing.
fn made(dir: PathBuf) -> PathBuf {
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Write `bytes` to `path` whole or not at all.
///
/// They go to a file beside it first, which is then renamed over it, so a
/// crash or power cut mid-write leaves the old file rather than half of the
/// new one. The scratch name holds the process and a count, so two writers
/// never share one.
pub fn write_atomic(path: &Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNT: AtomicU32 = AtomicU32::new(0);

    let bytes = bytes.as_ref();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    let scratch = path.with_file_name(name);
    let written = std::fs::File::create(&scratch).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    let renamed = written.and_then(|()| std::fs::rename(&scratch, path));
    let Err(e) = renamed else {
        return Ok(());
    };
    let _ = std::fs::remove_file(&scratch);
    // Windows refuses to replace a file another program holds open, such as
    // an editor. Better a plain write than none.
    if cfg!(windows) && path.is_file() {
        log::warn!(
            "could not replace {} ({e}), writing it in place",
            path.display()
        );
        return std::fs::write(path, bytes);
    }
    Err(e)
}

#[cfg(test)]
thread_local! {
    /// A test's stand-in for `APPDATA`. Per thread, so tests running in
    /// parallel never see each other's, and none has to change the process
    /// environment under the others.
    static TEST_BASE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Point this thread's `app_data_dir` at `base`, as `APPDATA` would.
#[cfg(test)]
pub fn set_test_base(base: &Path) {
    TEST_BASE.set(Some(base.to_path_buf()));
}

/// Where mpv keeps its per-title resume files.
///
/// Ours rather than mpv's default, so the player does not inherit or
/// pollute the resume state of a separately installed mpv.
pub fn watch_later_dir() -> PathBuf {
    made(app_data_dir().join("watch_later"))
}

/// The first of `names` that exists beside the executable or in the player's
/// `vendor` directory.
///
/// Both, because a shipped build puts its binaries next to the executable,
/// while `cargo run` copies nothing, so a checkout reaches into the source
/// tree: the player's crate, which the binaries ship with. `None` is a normal
/// answer; the callers fall back to the system.
pub fn vendored(names: &[&str]) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        roots.push(dir);
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../player/vendor"));

    roots
        .iter()
        .flat_map(|root| names.iter().map(move |name| root.join(name)))
        .find(|candidate| candidate.is_file())
}

/// Scratch file the current playlist is written to for mpv's `loadlist`.
pub fn playlist_m3u() -> PathBuf {
    app_data_dir().join("playlist.m3u")
}

/// The shader mpv marks subtitles with, written out for it to load (see
/// `session::mask_subtitles`).
pub fn subtitle_mask() -> PathBuf {
    app_data_dir().join("subtitle-mask.glsl")
}

/// Move a file the player can't read out of the way, to `<name>.unreadable`,
/// so the next write can't erase what a person might still recover from it
/// by hand.
pub fn set_aside(file: &Path) {
    let mut aside = file.as_os_str().to_os_string();
    aside.push(".unreadable");
    let aside = PathBuf::from(aside);
    match std::fs::rename(file, &aside) {
        Ok(()) => log::warn!(
            "{} could not be read, moved to {}",
            file.display(),
            aside.display()
        ),
        Err(e) => log::warn!("{} could not be read nor moved: {e}", file.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_write_replaces_the_file_and_leaves_nothing_beside_it() {
        let dir = std::env::temp_dir().join(format!("dbm-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("finished.json");
        std::fs::write(&file, "old").unwrap();

        write_atomic(&file, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["finished.json"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
