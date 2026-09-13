//! Thumbnails for the seek preview.
//!
//! One sprite atlas per file: a `GRID`×`GRID` grid of frames in a single
//! JPEG, so hovering the timeline costs an image lookup and a `source-clip`
//! rather than a decode. The UI is handed a picture and two offsets and knows
//! nothing about how either was produced.
//!
//! Built by ffmpeg, which is not libmpv and not a dependency of this crate:
//! if it is not on the machine there are no thumbnails and the preview falls
//! back to the timestamp alone. That is a feature degrading, not a failure,
//! and nothing here reports it as one after the first mention.
//!
//! Every frame is extracted by its own `-ss` seek rather than by scanning.
//! A linear pass over a two-hour film to collect 64 frames costs minutes; 64
//! seeks into the container index cost seconds, and they parallelise.
//!
//! Each build gets its own thread, not the shared worker. A feature-length
//! film is 64 ffmpeg spawns and takes as long as it takes; the worker is also
//! what keeps the track lists current, and a menu that stays empty until the
//! thumbnails finish would be a worse bug than having no thumbnails at all.
//!
//! A generation counter, bumped on every request, is what stops an abandoned
//! file's ffmpeg processes from outliving it — the same coalescing shape as
//! the scrubber and the list reads.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Tiles across and down. 64 frames is roughly one per two minutes of a
/// feature — close enough to land on the right scene, cheap enough to build
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
        ((index % self.grid) * self.tile_w, (index / self.grid) * self.tile_h)
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

/// Build the atlas for one video off the UI thread, and hand it back through
/// the event loop when it is ready.
///
/// Returns immediately. A build that produces nothing says nothing: see
/// [`build`] for why that is the ordinary case rather than an error.
pub fn spawn(video: PathBuf, ui: slint::Weak<crate::MainWindow>) {
    let generation = supersede();
    std::thread::spawn(move || {
        let Some(sprite) = build(&video, generation) else {
            return;
        };
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui.upgrade() {
                ui.invoke_preview_ready(
                    sprite.path.to_string_lossy().as_ref().into(),
                    sprite.tile_w as i32,
                    sprite.tile_h as i32,
                    sprite.grid as i32,
                );
            }
        });
    });
}

/// Build, or find, the atlas for one video. Runs off the UI thread.
///
/// Returns `None` for every ordinary reason a thumbnail might not exist —
/// no ffmpeg, an unreadable file, a superseded request — because none of
/// them is worth interrupting playback over.
pub fn build(video: &Path, generation: u64) -> Option<Sprite> {
    if let Some(cached) = cached(video) {
        return Some(cached);
    }
    let ffmpeg = ffmpeg_path()?;
    announce(&ffmpeg);
    if !current(generation) {
        return None;
    }

    let (duration, aspect) = probe(&ffmpeg, video)?;
    if duration <= 0.0 {
        return None;
    }
    let tile_w = match aspect {
        Some(ratio) if ratio > 0.0 => (((TILE_H as f64 * ratio).round() as u32 + 1) & !1)
            .clamp(48, 480),
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
// ffmpeg
// ---------------------------------------------------------------------------

/// Where ffmpeg is, if anywhere.
///
/// `DBM_FFMPEG` overrides; then the copy this app ships with; then whatever
/// the `PATH` turns up. The shipped copy has to come before `PATH` or a
/// machine with its own ffmpeg would quietly use that one instead — a
/// different build, possibly a different decoder set, for no reason.
///
/// Two names because the vendored file keeps the target-triple suffix Tauri's
/// sidecar mechanism requires, while a packaged build places it plainly.
pub(crate) fn ffmpeg_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("DBM_FFMPEG") {
        let path = PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }
    let names: &[&str] = if cfg!(windows) {
        &["ffmpeg.exe", "ffmpeg-x86_64-pc-windows-msvc.exe"]
    } else {
        &["ffmpeg", "ffmpeg-x86_64-unknown-linux-gnu"]
    };
    if let Some(shipped) = crate::paths::vendored(names) {
        return Some(shipped);
    }
    let name = names[0];
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(if cfg!(windows) { ';' } else { ':' })
        .map(|dir| Path::new(dir).join(name))
        .find(|p| p.is_file())
}

/// Name the ffmpeg actually in use, once per run.
///
/// Four things can supply it — the override, the copy beside the executable,
/// this crate's vendor directory, the `PATH` — so "the thumbnails are missing"
/// is otherwise a question with no way to answer it.
pub(crate) fn announce(ffmpeg: &Path) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| eprintln!("dbm: ffmpeg at {}", ffmpeg.display()));
}

pub(crate) fn command(ffmpeg: &Path) -> Command {
    let mut cmd = Command::new(ffmpeg);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // Without this every ffmpeg flashes a console window on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Duration in seconds and display aspect ratio, read off ffmpeg's own
/// report. `-i` with no output makes ffmpeg describe the file and exit.
fn probe(ffmpeg: &Path, video: &Path) -> Option<(f64, Option<f64>)> {
    let out = command(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(video)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    Some((parse_duration(&text)?, parse_aspect(&text)))
}

/// `  Duration: 01:23:45.67, start: ...`
pub(crate) fn parse_duration(text: &str) -> Option<f64> {
    let rest = text.split("Duration:").nth(1)?.trim_start();
    let clock = rest.split(',').next()?.trim();
    let mut parts = clock.split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let s: f64 = parts.next()?.trim().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
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
    let mut pending = Vec::new();
    for index in 0..total {
        if !current(generation) {
            return false;
        }
        let at = step * (index as f64 + 0.5);
        let out = into.join(format!("{:03}.jpg", index + 1));
        let child = command(ffmpeg)
            .arg("-nostdin")
            .arg("-y")
            // Before `-i`, so ffmpeg seeks the container rather than
            // decoding up to the timestamp. This is the whole reason the
            // build finishes in seconds.
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
            .arg(&out)
            .spawn();
        match child {
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
/// wrong slot, so any gap is filled with its predecessor before stitching:
/// a repeated thumbnail is a far smaller lie than an atlas offset by one.
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
    let ok = command(ffmpeg)
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
        .arg(sprite)
        .status()
        .is_ok_and(|s| s.success());
    ok && current(generation) && sprite.is_file()
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

/// Where a video's atlas lives. Named for the path so the same file found
/// twice is built once, and checked against its modification time so an
/// edited file is not shown yesterday's frames.
fn sprite_path(video: &Path) -> PathBuf {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in video.to_string_lossy().as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    crate::paths::app_data_dir()
        .join("previews")
        .join(format!("{hash:016x}.jpg"))
}

fn meta_path(sprite: &Path) -> PathBuf {
    sprite.with_extension("meta")
}

fn modified(path: &Path) -> Option<u64> {
    let stamp = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(
        stamp
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs(),
    )
}

/// `<mtime> <tile_w> <tile_h> <grid>` — four numbers, so reading it back
/// needs no parser worth the name.
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
    fn duration_is_read_off_ffmpeg_report() {
        let text = "  Duration: 01:23:45.67, start: 0.000000, bitrate: 1234 kb/s";
        let seconds = parse_duration(text).unwrap();
        assert!((seconds - 5025.67).abs() < 0.01, "{seconds}");
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
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// What answering a hover needs, shared between the render loop that learns
/// it and the action that uses it.
///
/// A hover must not reach mpv: the pointer moves at whatever rate the mouse
/// reports, and a blocking property read on that path is the mistake this
/// codebase keeps having to unlearn. Everything here is already known.
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
}

impl Preview {
    pub fn set_duration(&self, seconds: f64) {
        self.duration.set(seconds);
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
    pub fn again(&self) -> (String, i32, i32) {
        self.answer(self.last.get())
    }

    /// The timestamp under the pointer, and the tile to show with it.
    pub fn answer(&self, fraction: f32) -> (String, i32, i32) {
        self.last.set(fraction);
        let seconds = self.duration.get() * fraction.clamp(0.0, 1.0) as f64;
        let (x, y) = match self.sprite.borrow().as_ref() {
            Some(sprite) => sprite.tile_at(fraction),
            None => (0, 0),
        };
        (crate::state::format_time(seconds), x as i32, y as i32)
    }
}
