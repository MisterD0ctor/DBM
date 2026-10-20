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
    // `APPDATA` wins on every platform, not just Windows, so the `durations`
    // test can redirect it to a scratch directory wherever the suite runs.
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        if cfg!(windows) {
            None
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        }
    });
    let dir = base.unwrap_or_else(std::env::temp_dir).join(APP_DIR);
    let _ = std::fs::create_dir_all(&dir);
    dir
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
    // A harness writes to its own file, so a test run never touches the real
    // settings, however it was started.
    let name = if crate::harness::armed() {
        eprintln!("dbm: harness armed - settings go to settings.test.conf");
        "settings.test.conf"
    } else {
        "settings.conf"
    };
    app_data_dir().join(name)
}

/// Scratch file the current playlist is written to for mpv's `loadlist`.
pub fn playlist_m3u() -> PathBuf {
    app_data_dir().join("playlist.m3u")
}
