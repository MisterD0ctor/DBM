//! One path for each file, however it was reached.
//!
//! A resume point, a finished mark and every cache are filed under the path
//! a file was opened by, so two paths to one file are two files to the
//! player. Fedora's atomic desktops keep home in `/var/home`, with `/home` a
//! link to it, and a link of the user's own makes another, so a path is
//! followed to its end before anything is filed under it.
//!
//! Not on Windows, where the end is a `\\?\` path, which mpv and the
//! interface would show as it is.

use std::path::{Path, PathBuf};

/// `path` with every link in it followed, or as it is when it can't be
/// (it doesn't exist, say, which opening it will then report).
#[cfg(not(windows))]
pub fn resolved(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(end) if end != path => {
            eprintln!("dbm: {} is {}", path.display(), end.display());
            end
        }
        _ => path.to_path_buf(),
    }
}

#[cfg(windows)]
pub fn resolved(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn a_file_reached_through_a_link_is_filed_under_its_real_path() {
        let root = std::env::temp_dir().join(format!("dbm-links-{}", std::process::id()));
        let season = root.join("Season 1");
        std::fs::create_dir_all(&season).unwrap();
        std::fs::write(season.join("a.mkv"), b"").unwrap();
        std::os::unix::fs::symlink(&season, root.join("Shows")).unwrap();

        let real = std::fs::canonicalize(season.join("a.mkv")).unwrap();
        assert_eq!(resolved(&root.join("Shows/a.mkv")), real);
        assert_eq!(resolved(&real), real);
        let missing = root.join("Shows/b.mkv");
        assert_eq!(resolved(&missing), missing);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
