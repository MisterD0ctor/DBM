use std::path::Path;

use super::cache::cached;
use super::decode::cut;
use crate::ffmpeg;

/// One frame of a film, as plain pixels.
pub struct Still {
    /// RGBA, top row first.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The tile of an already-built atlas nearest `fraction` of the way in.
///
/// **Blocking.** Worker thread only: it runs ffmpeg once (see [`cut`]).
///
/// Only from the cache, never by building, since a frame for the empty window
/// isn't worth 64 ffmpeg spawns at startup.
pub fn still(video: &Path, fraction: f32) -> Option<Still> {
    let sprite = cached(video)?;
    let ffmpeg = ffmpeg::path()?;
    let (width, height) = (sprite.tile_w, sprite.tile_h);
    let rgba = cut(&ffmpeg, &sprite, sprite.tile_at(fraction), (width, height))?;
    Some(Still {
        rgba,
        width,
        height,
    })
}

/// The most lit tile of an already-built atlas, among those in the first
/// `within` of the file: the light of a film rather than of a black opening
/// or of its credits.
///
/// **Blocking.** Worker thread only: it runs ffmpeg once, for the whole atlas,
/// and from the cache only, as [`still`] does.
pub fn lit_still(video: &Path, within: f32) -> Option<Still> {
    let sprite = cached(video)?;
    let ffmpeg = ffmpeg::path()?;
    let (width, height, grid) = (sprite.tile_w, sprite.tile_h, sprite.grid);
    let atlas = cut(&ffmpeg, &sprite, (0, 0), (width * grid, height * grid))?;
    let stride = (width * grid * 4) as usize;
    let corner = |index: u32| ((index % grid) * width, (index / grid) * height);
    // Rec. 709's weights over every pixel of the tile, in whole numbers.
    let light = |index: u32| -> u64 {
        let (x, y) = corner(index);
        (y..y + height)
            .flat_map(|row| {
                let start = row as usize * stride + x as usize * 4;
                atlas[start..start + width as usize * 4].as_chunks::<4>().0
            })
            .map(|px| 2126 * u64::from(px[0]) + 7152 * u64::from(px[1]) + 722 * u64::from(px[2]))
            .sum()
    };
    let tiles = ((grid * grid) as f32 * within.clamp(0.0, 1.0))
        .ceil()
        .max(1.0) as u32;
    let (x, y) = corner((0..tiles).max_by_key(|&index| light(index))?);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in y..y + height {
        let start = row as usize * stride + x as usize * 4;
        rgba.extend_from_slice(&atlas[start..start + width as usize * 4]);
    }
    Some(Still {
        rgba,
        width,
        height,
    })
}
