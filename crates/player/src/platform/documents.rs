//! Where the files the document portal hands over really are.
//!
//! A Flatpak is given what is opened in it through the desktop's document
//! portal whenever the portal grants more than the app can reach on its own.
//! GNOME's file chooser asks for writing as well as reading, which the
//! player's read-only view of the disk doesn't cover, so what is chosen there
//! arrives under `/run/flatpak/doc`, in a folder of its own. A video opened
//! that way has no season beside it, and its resume point is filed under a
//! path no other way in shares.
//!
//! The portal says where each of its documents is, and the player can read
//! them there, so that is where they are played from. Windows has no portal;
//! everything here leaves a path as it is there.

use std::path::{Path, PathBuf};

/// The document a portal path is in, and the path within it: the document's
/// own name, then anything below it when it is a folder. `None` for any path
/// outside the portal.
fn document(path: &Path) -> Option<(&str, PathBuf)> {
    let inside = path.strip_prefix("/run/flatpak/doc").ok().or_else(|| {
        let rest = path.strip_prefix("/run/user").ok()?;
        let mut parts = rest.components();
        parts.next()?;
        (parts.next()?.as_os_str() == "doc").then_some(parts.as_path())
    })?;
    let mut parts = inside.components();
    let id = parts.next()?.as_os_str().to_str()?;
    // The document's own name, which its real path already ends in.
    parts.next()?;
    Some((id, parts.as_path().to_path_buf()))
}

/// Whether `path` is one the document portal handed over.
pub fn is_portal(path: &Path) -> bool {
    document(path).is_some()
}

/// Where `path` really is, when the portal handed it over and the player can
/// read it there; otherwise `path` as it is. **Blocking**: asks the portal
/// over D-Bus.
pub fn real(path: &Path) -> PathBuf {
    let Some((id, within)) = document(path) else {
        return path.to_path_buf();
    };
    match host_path(id) {
        Ok(Some(host)) => {
            let real = if within.as_os_str().is_empty() {
                host
            } else {
                host.join(within)
            };
            if real.exists() {
                eprintln!("dbm: {} is {}", path.display(), real.display());
                return real;
            }
            eprintln!("dbm: {} is out of reach", real.display());
        }
        Ok(None) => eprintln!("dbm: the portal has no {id}"),
        Err(e) => eprintln!("dbm: cannot ask the portal where {id} is: {e}"),
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn host_path(id: &str) -> zbus::Result<Option<PathBuf>> {
    use std::collections::HashMap;
    use std::os::unix::ffi::OsStringExt;

    let conn = zbus::blocking::Connection::session()?;
    let documents = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Documents",
        "/org/freedesktop/portal/documents",
        "org.freedesktop.portal.Documents",
    )?;
    let mut paths: HashMap<String, Vec<u8>> = documents.call("GetHostPaths", &(vec![id],))?;
    Ok(paths.remove(id).map(|mut bytes| {
        // A bytestring, so it ends in a NUL.
        if bytes.last() == Some(&0) {
            bytes.pop();
        }
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }))
}

#[cfg(windows)]
fn host_path(_id: &str) -> std::io::Result<Option<PathBuf>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(path: &str) -> Option<(String, PathBuf)> {
        document(Path::new(path)).map(|(id, within)| (id.to_owned(), within))
    }

    #[test]
    fn a_portal_path_is_split_at_its_document() {
        let folder = split("/run/flatpak/doc/PXD5/Video/Shows/a.mkv");
        assert_eq!(folder, Some(("PXD5".into(), "Shows/a.mkv".into())));
        let file = split("/run/user/1000/doc/vA_2/a.mkv");
        assert_eq!(file, Some(("vA_2".into(), PathBuf::new())));
    }

    #[test]
    fn other_paths_are_not_the_portals() {
        assert!(!is_portal(Path::new("/var/home/kasper/Video/Shows/a.mkv")));
        assert!(!is_portal(Path::new("/run/user/1000/gvfs/smb-share/a.mkv")));
        assert!(!is_portal(Path::new("/run/media/kasper/doc/a.mkv")));
        assert!(!is_portal(Path::new("/run/flatpak/doc")));
    }
}
