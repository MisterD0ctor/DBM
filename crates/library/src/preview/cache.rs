//! Where each atlas is kept, and whether the one kept is still the film's.

use std::path::{Path, PathBuf};

use dbm_mpv::MediaPath;

use super::Sprite;
use crate::cache::Folder;

/// Every atlas built. Versioned as caches are (see `cache`), and bumped
/// whenever an atlas would come out differently: its grid, its tiles' size,
/// the frames it picks.
///
/// 1. `GRID`×`GRID` frames, `TILE_H` high, each the middle of its share.
static PREVIEWS: Folder = Folder::cache("previews", 1);

/// Where a video's atlas lives. Named for the path so the same file found
/// twice is built once, and checked against its modification time so an
/// edited file is not shown yesterday's frames.
pub(super) fn sprite_path(video: &MediaPath) -> PathBuf {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in video.as_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    PREVIEWS.path().join(format!("{hash:016x}.jpg"))
}

fn meta_path(sprite: &Path) -> PathBuf {
    sprite.with_extension("meta")
}

fn modified(path: &Path) -> Option<u64> {
    let stamp = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(stamp.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
}

/// `<mtime> <tile_w> <tile_h> <grid>`: four numbers, so reading it back
/// needs no real parser.
pub(super) fn write_meta(sprite: &Sprite, video: &MediaPath) {
    let Some(mtime) = modified(video.as_path()) else {
        return;
    };
    let text = format!(
        "{mtime} {} {} {}",
        sprite.tile_w, sprite.tile_h, sprite.grid
    );
    let _ = crate::paths::write_atomic(&meta_path(&sprite.path), text);
}

pub(super) fn cached(video: &MediaPath) -> Option<Sprite> {
    let sprite = sprite_path(video);
    if !sprite.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(meta_path(&sprite)).ok()?;
    let mut parts = text.split_whitespace();
    let stamp: u64 = parts.next()?.parse().ok()?;
    if modified(video.as_path())? != stamp {
        return None;
    }
    Some(Sprite {
        tile_w: parts.next()?.parse().ok()?,
        tile_h: parts.next()?.parse().ok()?,
        grid: parts.next()?.parse().ok()?,
        path: sprite,
    })
}
