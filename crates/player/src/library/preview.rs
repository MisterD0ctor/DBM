//! Thumbnails for the seek preview.
//!
//! One sprite atlas per file: a `GRID`×`GRID` grid of frames in a single
//! JPEG, so hovering the timeline costs an image lookup and a `source-clip`
//! rather than a decode. The UI is handed a picture and two offsets and knows
//! nothing about how either was produced.
//!
//! Built by ffmpeg; with none on the machine there are no thumbnails, and
//! the preview falls back to the timestamp alone (see `ffmpeg`).
//!
//! Every frame is extracted by its own `-ss` seek rather than by scanning.
//! A linear pass over a two-hour film to collect 64 frames costs minutes; 64
//! seeks into the container index cost seconds, and they parallelise.
//!
//! Each build gets its own thread rather than the shared worker, since it is
//! 64 ffmpeg spawns and the worker also keeps the track lists current, which
//! must not wait for thumbnails. Its ffmpeg runs behind the player (see
//! `ffmpeg::spawn_behind`), as builds mostly happen while a film is starting,
//! when the player is busiest.
//!
//! A generation counter, bumped on every request, stops an abandoned file's
//! ffmpeg processes from outliving it.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::library::cache::Folder;
use crate::library::ffmpeg;
use crate::playback::tracks::Chapter;
use crate::worker::{Completion, Reporter};

/// Tiles across and down. 64 frames is roughly one per two minutes of a
/// feature: close enough to land on the right scene, cheap enough to build
/// while the film is starting.
const GRID: u32 = 8;
/// Tile height in pixels; the width follows the video's aspect. Even, for
/// yuv420p chroma.
const TILE_H: u32 = 108;
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

/// A finished atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub path: PathBuf,
    pub tile_w: u32,
    pub tile_h: u32,
    pub grid: u32,
}

impl Sprite {
    /// Top-left corner of the tile covering `fraction` of the file.
    pub fn tile_at(&self, fraction: f32) -> (u32, u32) {
        let total = (self.grid * self.grid).max(1);
        let index = ((fraction.clamp(0.0, 1.0) * total as f32) as u32).min(total - 1);
        (
            (index % self.grid) * self.tile_w,
            (index / self.grid) * self.tile_h,
        )
    }
}

/// Bumped on every request. A worker captures the value it started with and
/// stops as soon as the global one moves past it.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Abandon any atlas still being built, and return the generation a new one
/// should run under. Called on the UI thread when the file changes.
pub fn supersede() -> u64 {
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

fn current(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) == generation
}

/// Build the atlas for one video off the UI thread, and report it like any
/// other background result when it is ready.
///
/// Returns immediately. A build that produces nothing says nothing: see
/// [`build`] for why that is the ordinary case rather than an error.
pub fn spawn(video: PathBuf, reporter: Reporter) {
    let generation = supersede();
    std::thread::spawn(move || {
        let Some(sprite) = build(&video, generation) else {
            return;
        };
        // Decoded here rather than on the UI thread, where it costs most of
        // a frame and shows as a hitch.
        let Some(pixels) = decode(&sprite) else {
            return;
        };
        if current(generation) {
            reporter.send(Completion::Preview(Atlas { sprite, pixels }));
        }
    });
}

/// A finished atlas with its picture already decoded, ready to be shown.
pub struct Atlas {
    pub sprite: Sprite,
    pub pixels: slint::SharedPixelBuffer<slint::Rgba8Pixel>,
}

/// The whole atlas as pixels. **Blocking**: runs ffmpeg once.
fn decode(sprite: &Sprite) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let ffmpeg = ffmpeg::path()?;
    let (width, height) = (sprite.tile_w * sprite.grid, sprite.tile_h * sprite.grid);
    let mut pixels = slint::SharedPixelBuffer::new(width, height);
    cut_into(
        &ffmpeg,
        sprite,
        (0, 0),
        (width, height),
        pixels.make_mut_bytes(),
    )?;
    Some(pixels)
}

/// A rectangle of the atlas as RGBA, top row first. **Blocking**: runs
/// ffmpeg once (see [`cut_into`]).
fn cut(ffmpeg: &Path, sprite: &Sprite, at: (u32, u32), size: (u32, u32)) -> Option<Vec<u8>> {
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
    let mut child = cmd
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
    let mut stdout = child.stdout.take()?;
    let filled = stdout.read_exact(rgba).is_ok();
    // A byte more would be a picture of another size than the one asked for.
    let over = stdout.read(&mut [0]).map_or(true, |n| n > 0);
    // Closed before the wait, so an ffmpeg with more to write stops.
    drop(stdout);
    let succeeded = child.wait().is_ok_and(|status| status.success());
    (filled && !over && succeeded).then_some(())
}

/// Build, or find, the atlas for one video. Runs off the UI thread.
///
/// Returns `None` for every ordinary reason a thumbnail might not exist (no
/// ffmpeg, an unreadable file, a superseded request), since none is worth
/// interrupting playback over.
pub fn build(video: &Path, generation: u64) -> Option<Sprite> {
    if let Some(cached) = cached(video) {
        return Some(cached);
    }
    let ffmpeg = ffmpeg::path()?;
    ffmpeg::announce(&ffmpeg);
    if !current(generation) {
        return None;
    }

    let (duration, aspect) = probe(&ffmpeg, video)?;
    if duration <= 0.0 {
        return None;
    }
    let tile_w = match aspect {
        Some(ratio) if ratio > 0.0 => {
            (((TILE_H as f64 * ratio).round() as u32 + 1) & !1).clamp(48, 480)
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

    if !extract(&ffmpeg, video, &tiles, duration, tile_w, generation) {
        return None;
    }
    if !assemble(&ffmpeg, &tiles, &sprite, tile_w, generation) {
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

// ---------------------------------------------------------------------------
// Reading a file
// ---------------------------------------------------------------------------

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
    // `Video: h264 (High), yuv420p(tv), 1920x1080 [SAR 1:1 DAR 16:9], ...`
    let rest = text.split("Video:").nth(1)?;
    for token in rest.split([',', ' ']) {
        let token = token.trim();
        let Some((w, h)) = token.split_once('x') else {
            continue;
        };
        let (Ok(w), Ok(h)) = (w.parse::<f64>(), h.parse::<f64>()) else {
            continue;
        };
        if w > 0.0 && h > 0.0 {
            return Some(w / h);
        }
    }
    None
}

/// One frame per tile, each by its own seek, `PARALLEL` at a time.
fn extract(
    ffmpeg: &Path,
    video: &Path,
    into: &Path,
    duration: f64,
    tile_w: u32,
    generation: u64,
) -> bool {
    let total = GRID * GRID;
    // Sampled at the midpoint of each slice, so the first tile is not the
    // black frame every film opens on and the last is not past the end.
    let step = duration / total as f64;
    let keyframes = step >= KEYFRAME_SLICE;
    let mut pending = Vec::new();
    for index in 0..total {
        if !current(generation) {
            return false;
        }
        let at = step * (index as f64 + 0.5);
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
            for mut child in pending.drain(..) {
                let _ = child.wait();
            }
        }
    }
    for mut child in pending {
        let _ = child.wait();
    }
    current(generation)
}

/// Stitch the tiles into one atlas.
///
/// A frame missing from the middle would shift every later tile into the
/// wrong slot, so a gap is filled with its predecessor: a repeated thumbnail
/// is a smaller lie than an atlas offset by one.
fn assemble(ffmpeg: &Path, tiles: &Path, sprite: &Path, tile_w: u32, generation: u64) -> bool {
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
    if !current(generation) {
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
    ok && current(generation) && sprite.is_file()
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

/// Every atlas built. Versioned as caches are (see `cache`), and bumped
/// whenever an atlas would come out differently: its grid, its tiles' size,
/// the frames it picks.
///
/// 1. `GRID`×`GRID` frames, `TILE_H` high, each the middle of its share.
static PREVIEWS: Folder = Folder::cache("previews", 1);

/// Where a video's atlas lives. Named for the path so the same file found
/// twice is built once, and checked against its modification time so an
/// edited file is not shown yesterday's frames.
fn sprite_path(video: &Path) -> PathBuf {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in video.to_string_lossy().as_bytes() {
        hash ^= *byte as u64;
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
fn write_meta(sprite: &Sprite, video: &Path) {
    let Some(mtime) = modified(video) else { return };
    let text = format!(
        "{mtime} {} {} {}",
        sprite.tile_w, sprite.tile_h, sprite.grid
    );
    let _ = std::fs::write(meta_path(&sprite.path), text);
}

fn cached(video: &Path) -> Option<Sprite> {
    let sprite = sprite_path(video);
    if !sprite.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(meta_path(&sprite)).ok()?;
    let mut parts = text.split_whitespace();
    let stamp: u64 = parts.next()?.parse().ok()?;
    if modified(video)? != stamp {
        return None;
    }
    Some(Sprite {
        tile_w: parts.next()?.parse().ok()?,
        tile_h: parts.next()?.parse().ok()?,
        grid: parts.next()?.parse().ok()?,
        path: sprite,
    })
}

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
            .map(|px| 2126 * px[0] as u64 + 7152 * px[1] as u64 + 722 * px[2] as u64)
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

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// What answering a hover needs, shared between the render loop that learns
/// it and the action that uses it.
///
/// A hover must not reach mpv, since the pointer moves at whatever rate the
/// mouse reports, and a blocking read there would stall the frame.
#[derive(Default)]
pub struct Preview {
    duration: std::cell::Cell<f64>,
    sprite: std::cell::RefCell<Option<Sprite>>,
    /// The file the current atlas belongs to, so a request is made once per
    /// file rather than once per frame.
    source: std::cell::RefCell<Option<PathBuf>>,
    /// Where the pointer last asked about, so an atlas that arrives while the
    /// timeline is already hovered can fill in the tile straight away instead
    /// of waiting for the pointer to move.
    last: std::cell::Cell<f32>,
    /// The file's chapters, to name the one under the pointer.
    chapters: std::cell::RefCell<Vec<Chapter>>,
}

/// What the preview says for a place on the timeline.
pub struct Answer {
    pub time: String,
    /// The chapter it is in, where the file names it; empty otherwise.
    pub chapter: String,
    /// The tile's corner in the atlas.
    pub x: i32,
    pub y: i32,
}

impl Preview {
    pub fn set_duration(&self, seconds: f64) {
        self.duration.set(seconds);
    }

    /// Only copied when they differ, since this is asked every frame.
    pub fn set_chapters(&self, chapters: &[Chapter]) {
        if *self.chapters.borrow() != chapters {
            *self.chapters.borrow_mut() = chapters.to_vec();
        }
    }

    pub fn set_sprite(&self, sprite: Option<Sprite>) {
        *self.sprite.borrow_mut() = sprite;
    }

    /// Whether this path is new, claiming it if so. The caller starts a build
    /// on `true` and does nothing on `false`.
    pub fn claim(&self, path: &Path) -> bool {
        let mut source = self.source.borrow_mut();
        if source.as_deref() == Some(path) {
            return false;
        }
        *source = Some(path.to_path_buf());
        true
    }

    /// The answer for wherever the pointer last was. For when what is known
    /// changes rather than where the pointer is.
    pub fn again(&self) -> Answer {
        self.answer(self.last.get())
    }

    /// The timestamp under the pointer, the chapter it is in, and the tile
    /// to show with them.
    pub fn answer(&self, fraction: f32) -> Answer {
        self.last.set(fraction);
        let seconds = self.duration.get() * fraction.clamp(0.0, 1.0) as f64;
        let (x, y) = match self.sprite.borrow().as_ref() {
            Some(sprite) => sprite.tile_at(fraction),
            None => (0, 0),
        };
        Answer {
            time: crate::interface::format::time(seconds),
            chapter: named_chapter(&self.chapters.borrow(), seconds),
            x: x as i32,
            y: y as i32,
        }
    }
}

/// The title of the chapter `seconds` falls in, unless it is only a
/// placeholder a muxer wrote, its number or its start time, which says
/// nothing the time beside it doesn't.
fn named_chapter(chapters: &[Chapter], seconds: f64) -> String {
    chapters
        .iter()
        .take_while(|c| c.time <= seconds + 1e-3)
        .last()
        .and_then(|c| c.title.as_deref())
        .filter(|title| !crate::library::naming::is_unnamed_chapter(title))
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite() -> Sprite {
        Sprite {
            path: PathBuf::new(),
            tile_w: 192,
            tile_h: 108,
            grid: 8,
        }
    }

    #[test]
    fn tiles_run_left_to_right_then_down() {
        let s = sprite();
        assert_eq!(s.tile_at(0.0), (0, 0));
        // Second tile of the first row.
        assert_eq!(s.tile_at(1.0 / 64.0), (192, 0));
        // First tile of the second row.
        assert_eq!(s.tile_at(8.0 / 64.0), (0, 108));
    }

    #[test]
    fn the_end_lands_on_the_last_tile_not_past_it() {
        let s = sprite();
        assert_eq!(s.tile_at(1.0), (7 * 192, 7 * 108));
        assert_eq!(s.tile_at(2.0), (7 * 192, 7 * 108));
        assert_eq!(s.tile_at(-1.0), (0, 0));
    }

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

    #[test]
    fn a_chapter_is_named_only_where_the_file_names_it() {
        let chapter = |time, title: Option<&str>| Chapter {
            time,
            title: title.map(str::to_string),
        };
        let chapters = [
            chapter(0.0, Some("Prologue")),
            chapter(90.0, Some("Chapter 2")),
            chapter(300.0, Some("The Heist")),
            chapter(900.0, None),
        ];
        assert_eq!(named_chapter(&chapters, 30.0), "Prologue");
        assert_eq!(named_chapter(&chapters, 120.0), "");
        assert_eq!(named_chapter(&chapters, 300.0), "The Heist");
        assert_eq!(named_chapter(&chapters, 1000.0), "");
        assert_eq!(named_chapter(&[], 10.0), "");
    }
}
