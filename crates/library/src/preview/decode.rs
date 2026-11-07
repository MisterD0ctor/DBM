//! Reading an atlas back as pixels: the whole grid for the timeline, or one
//! tile of it.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use super::Sprite;
use crate::ffmpeg;

/// The whole atlas as pixels. **Blocking**: runs ffmpeg once.
pub(super) fn decode(sprite: &Sprite) -> Option<Vec<u8>> {
    let ffmpeg = ffmpeg::path()?;
    let size = (sprite.tile_w * sprite.grid, sprite.tile_h * sprite.grid);
    cut(&ffmpeg, sprite, (0, 0), size)
}

/// A rectangle of the atlas as RGBA, top row first. **Blocking**: runs
/// ffmpeg once (see [`cut_into`]).
pub(super) fn cut(
    ffmpeg: &Path,
    sprite: &Sprite,
    at: (u32, u32),
    size: (u32, u32),
) -> Option<Vec<u8>> {
    let mut rgba = vec![0; (size.0 * size.1 * 4) as usize];
    cut_into(ffmpeg, sprite, at, size, &mut rgba)?;
    Some(rgba)
}

/// Cut a rectangle out of the atlas's JPEG into `rgba`, which is its size.
/// **Blocking**: runs ffmpeg once, onto stdout.
///
/// Read straight into place, since the whole atlas is megabytes: gathered as
/// it comes and copied after, it would take three times its size at the
/// peak, which the allocator keeps (see `platform::memory`).
fn cut_into(
    ffmpeg: &Path,
    sprite: &Sprite,
    at: (u32, u32),
    size: (u32, u32),
    rgba: &mut [u8],
) -> Option<()> {
    let ((x, y), (width, height)) = (at, size);
    let mut cmd = ffmpeg::command(ffmpeg);
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(&sprite.path)
        .arg("-vf")
        .arg(format!("crop={width}:{height}:{x}:{y}"))
        .arg("-frames:v")
        .arg("1")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-")
        .spawn()
        .ok()?;
    let mut child = ffmpeg::Running::new(child);
    let mut stdout = child.stdout()?;
    let filled = stdout.read_exact(rgba).is_ok();
    // A byte more would be a picture of another size than the one asked for.
    let over = stdout.read(&mut [0]).map_or(true, |n| n > 0);
    // Closed before the wait, so an ffmpeg with more to write stops.
    drop(stdout);
    let succeeded = child.wait().is_ok_and(|status| status.success());
    (filled && !over && succeeded).then_some(())
}
