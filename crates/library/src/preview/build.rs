//! Building an atlas: the film's length and shape read off ffmpeg, a frame
//! cut at each tile's moment, and the tiles put together into one JPEG.

use std::path::{Path, PathBuf};

use dbm_mpv::MediaPath;

use super::cache::{cached, sprite_path, write_meta};
use super::{GRID, Sprite, TILE_H};
use crate::ffmpeg;
use crate::threads::Ticket;

/// Used when the aspect cannot be read.
const TILE_W_FALLBACK: u32 = 192;

/// How many ffmpeg processes at once. Past this the disk is the limit, not
/// the CPU, and each process re-opens the file.
const PARALLEL: usize = 8;

/// From this much film per tile up, a tile is the keyframe nearest its time
/// rather than the exact frame.
///
/// An exact frame decodes everything from the keyframe before it, hundreds
/// of frames a tile in a long-GOP encode, which held a 16-thread machine near
/// 90% for eight seconds and made the player drop frames. A keyframe is one
/// decoded frame, and the atlas takes two seconds.
///
/// Twenty seconds because encoders put a keyframe at least every ten or so
/// (x264 and x265 default to 250 frames), so the keyframe at or before a
/// slice's midpoint is still inside it. That is a film of 21 minutes or more;
/// shorter clips, where ten seconds early would show, keep exact frames.
const KEYFRAME_SLICE: f64 = 20.0;

/// Build, or find, the atlas for one video. Runs off the UI thread.
///
/// Returns `None` for every ordinary reason a thumbnail might not exist (no
/// ffmpeg, an unreadable file, a superseded request), since none is worth
/// interrupting playback over.
pub(super) fn build(video: &MediaPath, ticket: Ticket) -> Option<Sprite> {
    if let Some(cached) = cached(video) {
        return Some(cached);
    }
    let ffmpeg = ffmpeg::path()?;
    ffmpeg::announce(&ffmpeg);
    if !ticket.current() {
        return None;
    }

    let (duration, aspect) = probe(&ffmpeg, video.as_path())?;
    if duration <= 0.0 {
        return None;
    }
    let tile_w = match aspect {
        Some(ratio) if ratio > 0.0 => {
            (((f64::from(TILE_H) * ratio).round() as u32 + 1) & !1).clamp(48, 480)
        }
        _ => TILE_W_FALLBACK,
    };

    let sprite = sprite_path(video);
    if let Some(parent) = sprite.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    let tiles = sprite.with_extension("tiles");
    std::fs::create_dir_all(&tiles).ok()?;
    let _cleanup = Cleanup(tiles.clone());

    if !extract(&ffmpeg, video.as_path(), &tiles, duration, tile_w, ticket) {
        return None;
    }
    if !assemble(&ffmpeg, &tiles, &sprite, tile_w, ticket) {
        return None;
    }

    let meta = Sprite {
        path: sprite,
        tile_w,
        tile_h: TILE_H,
        grid: GRID,
    };
    write_meta(&meta, video);
    Some(meta)
}

/// Removes the scratch tiles whichever way the build ends.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Duration in seconds and display aspect ratio, read off ffmpeg's own
/// report. `-i` with no output makes ffmpeg describe the file and exit.
fn probe(ffmpeg: &Path, video: &Path) -> Option<(f64, Option<f64>)> {
    let text = ffmpeg::describe(ffmpeg, video)?;
    Some((ffmpeg::parse_duration(&text)?, parse_aspect(&text)))
}

/// `DAR 16:9` where present, else the coded `1920x1080`.
fn parse_aspect(text: &str) -> Option<f64> {
    if let Some(rest) = text.split("DAR ").nth(1) {
        let spec = rest.split(']').next()?.trim();
        if let Some((w, h)) = spec.split_once(':') {
            let (w, h): (f64, f64) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
            if h > 0.0 {
                return Some(w / h);
            }
        }
    }
    let frame = ffmpeg::parse_size(text)?;
    Some(f64::from(frame.width) / f64::from(frame.height))
}

/// One frame per tile, each by its own seek, `PARALLEL` at a time.
fn extract(
    ffmpeg: &Path,
    video: &Path,
    into: &Path,
    duration: f64,
    tile_w: u32,
    ticket: Ticket,
) -> bool {
    let total = GRID * GRID;
    // Sampled at the midpoint of each slice, so the first tile is not the
    // black frame every film opens on and the last is not past the end.
    let step = duration / f64::from(total);
    let keyframes = step >= KEYFRAME_SLICE;
    let mut pending = ffmpeg::Children::default();
    for index in 0..total {
        if !ticket.current() {
            return false;
        }
        let at = step * (f64::from(index) + 0.5);
        let out = into.join(format!("{:03}.jpg", index + 1));
        let mut command = ffmpeg::command(ffmpeg);
        command
            .arg("-nostdin")
            .arg("-y")
            // One thread each, since a decoder's default is a pool the size
            // of the machine, and eight at once would starve the player.
            .arg("-threads")
            .arg("1")
            .arg("-filter_threads")
            .arg("1");
        if keyframes {
            // Decode the keyframe the seek lands on and nothing after it.
            command
                .arg("-skip_frame")
                .arg("nokey")
                .arg("-noaccurate_seek");
        }
        command
            // Before `-i`, so ffmpeg seeks the container to the keyframe
            // before the time rather than reading the file up to it. With
            // exact frames it still decodes forward from there.
            .arg("-ss")
            .arg(format!("{at:.3}"))
            .arg("-i")
            .arg(video)
            .arg("-frames:v")
            .arg("1")
            .arg("-vf")
            .arg(format!("scale={tile_w}:{TILE_H}"))
            .arg("-q:v")
            .arg("5")
            .arg(&out);
        match ffmpeg::spawn_behind(&mut command) {
            Ok(child) => pending.push(child),
            Err(_) => return false,
        }
        if pending.len() >= PARALLEL {
            pending.wait_each(|mut child| {
                let _ = child.wait();
            });
        }
    }
    pending.wait_each(|mut child| {
        let _ = child.wait();
    });
    ticket.current()
}

/// Stitch the tiles into one atlas.
///
/// A frame missing from the middle would shift every later tile into the
/// wrong slot, so a gap is filled with its predecessor: a repeated thumbnail
/// is a smaller lie than an atlas offset by one.
fn assemble(ffmpeg: &Path, tiles: &Path, sprite: &Path, tile_w: u32, ticket: Ticket) -> bool {
    let mut last: Option<PathBuf> = None;
    for index in 0..GRID * GRID {
        let path = tiles.join(format!("{:03}.jpg", index + 1));
        if path.is_file() {
            last = Some(path);
            continue;
        }
        match &last {
            Some(previous) => {
                if std::fs::copy(previous, &path).is_err() {
                    return false;
                }
            }
            // Nothing before it either: the run produced no frames at all.
            None => return false,
        }
    }
    if !ticket.current() {
        return false;
    }
    let mut command = ffmpeg::command(ffmpeg);
    command
        .arg("-nostdin")
        .arg("-y")
        .arg("-i")
        .arg(tiles.join("%03d.jpg"))
        .arg("-vf")
        .arg(format!("scale={tile_w}:{TILE_H},tile={GRID}x{GRID}"))
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("4")
        .arg(sprite);
    let ok = ffmpeg::spawn_behind(&mut command)
        .and_then(|mut child| child.wait())
        .is_ok_and(|s| s.success());
    ok && ticket.current() && sprite.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_prefers_the_declared_ratio() {
        let text = "Video: h264, yuv420p, 1920x816 [SAR 1:1 DAR 40:17], 24 fps";
        let ratio = parse_aspect(text).unwrap();
        assert!((ratio - 40.0 / 17.0).abs() < 0.001, "{ratio}");
    }

    #[test]
    fn aspect_falls_back_to_the_coded_size() {
        let text = "Video: h264, yuv420p, 960x400, 30 fps";
        let ratio = parse_aspect(text).unwrap();
        assert!((ratio - 2.4).abs() < 0.001, "{ratio}");
    }
}
