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
    // `APPDATA` wins outright on every platform, not just Windows — the
    // `durations` test redirects it to a scratch directory, and that has to
    // work the same way wherever the suite runs or the test quietly starts
    // reading and writing the real one.
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        if cfg!(windows) {
            None
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        }
    });
    let dir = base
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR);
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
/// Both are checked because both happen: a shipped build puts its binaries
/// next to the `.exe`, and `cargo run` puts them nowhere at all — it never
/// copies anything, so a checkout has to reach into the source tree. Returning
/// `None` is a normal answer; the callers fall back to the system.
pub fn vendored(names: &[&str]) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))
    {
        roots.push(dir);
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor"));

    roots
        .iter()
        .flat_map(|root| names.iter().map(move |name| root.join(name)))
        .find(|candidate| candidate.is_file())
}

/// Tunable material parameters, saved between runs.
///
/// Plain text on purpose: it is a short list of numbers, and being able to
/// open it in an editor while tuning is worth more than a compact format.
pub fn settings_file() -> PathBuf {
    // A harness that moves settings writes to its own file. Relying on the
    // caller to redirect `APPDATA` was the arrangement before, and it cost a
    // real settings file: one run without the prefix is all it takes, and
    // nothing about the run says it happened.
    // Any harness at all, rather than a list of the ones believed to write
    // settings: that list was wrong within a day of being written, because
    // `DBM_KEY_TEST` presses B and B is the ambience switch. A test run has
    // no business in the real file whatever it thinks it is doing.
    let harnessed = std::env::vars_os().any(|(k, _)| {
        let k = k.to_string_lossy();
        k.starts_with("DBM_") && k.ends_with("_TEST")
    });
    let name = if harnessed {
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
