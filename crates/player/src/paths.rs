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
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        if cfg!(windows) {
            None
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        }
    });
    #[cfg(test)]
    let base = TEST_BASE.with_borrow(Clone::clone).or(base);
    let dir = base.unwrap_or_else(std::env::temp_dir).join(APP_DIR);
    let _ = std::fs::create_dir_all(&dir);
    dir
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
    let dir = app_data_dir().join("watch_later");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The first of `names` that exists beside the executable or in the crate's
/// `vendor` directory.
///
/// Both, because a shipped build puts its binaries next to the executable,
/// while `cargo run` copies nothing, so a checkout reaches into the source
/// tree. `None` is a normal answer; the callers fall back to the system.
pub fn vendored(names: &[&str]) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        roots.push(dir);
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor"));

    roots
        .iter()
        .flat_map(|root| names.iter().map(move |name| root.join(name)))
        .find(|candidate| candidate.is_file())
}

/// The saved settings (see `settings`).
///
/// Plain text on purpose: it is a short list of numbers, and being able to
/// open it in an editor while tuning is worth more than a compact format.
pub fn settings_file() -> PathBuf {
    // A run nobody is watching (an exercise, a showcase or a capture) keeps to
    // its own file, so it neither touches the real settings nor looks like
    // whatever they were last set to while trying something.
    let name = if crate::run_mode::unattended() {
        log::info!("unattended - settings go to settings.test.conf");
        "settings.test.conf"
    } else {
        "settings.conf"
    };
    app_data_dir().join(name)
}

/// What the player said this run (see `logging`). A run nobody is watching
/// keeps to its own, as it does for the settings.
pub fn log_file() -> PathBuf {
    let name = if crate::run_mode::unattended() {
        "dbm.test.log"
    } else {
        "dbm.log"
    };
    app_data_dir().join(name)
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
