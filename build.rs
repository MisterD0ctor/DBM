//! Stamps a content hash of `public/` into the binary as `ASSET_VERSION`.
//!
//! Icon URLs are stable (`public/icons/foo.svg`), and neither trunk's dev
//! server nor Tauri's asset protocol sends `Cache-Control` or an `ETag` —
//! only `Last-Modified`. With no explicit freshness, Chromium falls back to
//! a heuristic of roughly 10% of the file's age, so editing an icon that had
//! been sitting untouched for months leaves the WebView convinced its copy
//! is good for weeks. A rebuild doesn't help: the URL never changes, so it
//! never asks.
//!
//! Appending this hash as `?v=` gives each icon a new URL exactly when its
//! bytes change — and, just as importantly, the *same* URL when they don't,
//! so ordinary caching still works.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

fn main() {
    // Cargo rescans a directory target recursively, so any icon edit re-runs
    // this script — and only then.
    println!("cargo:rerun-if-changed=public");

    let mut files = Vec::new();
    collect(Path::new("public"), &mut files);
    files.sort();

    let mut hasher = DefaultHasher::new();
    for file in &files {
        file.to_string_lossy().hash(&mut hasher);
        if let Ok(bytes) = std::fs::read(file) {
            bytes.hash(&mut hasher);
        }
    }
    println!("cargo:rustc-env=ASSET_VERSION={:x}", hasher.finish());
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}
