//! Background thumbnail-sprite generation for the seek tooltip.
//!
//! On file load we spawn ffmpeg in a worker thread to extract a GRID×GRID
//! tile atlas of preview frames. When finished, an event `preview://ready`
//! is emitted so the frontend can attach the sprite to its seek tooltip.
//!
//! Sprites are cached in the app data dir under `previews/<md5>.jpg` and
//! keyed off the video's path + mtime so edits invalidate old sprites.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use shared::PreviewReady;
use tauri::{AppHandle, Emitter, Manager};

pub const GRID: u32 = 8;
/// Must be even (yuv420p chroma subsampling).
pub const TILE_H: u32 = 120;
/// Fallback width if probe fails to detect the source aspect ratio.
const TILE_W_FALLBACK: u32 = 214;
const FRAMES: u32 = GRID * GRID;

/// Concurrent ffmpeg processes during sprite generation. More processes ≠
/// linearly faster: each one re-opens the file, and disk seek bandwidth
/// caps the gain. 8 is a good default for SSDs.
const TILE_PARALLELISM: usize = 8;

static ACTIVE_JOB: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static ACTIVE_CHILDREN: Lazy<Mutex<Vec<Child>>> = Lazy::new(|| Mutex::new(Vec::new()));
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Bumped on every `request_preview` call. Worker threads capture their own
/// generation at spawn time and check `is_alive(my_gen)` at every step —
/// once the global generation moves past, the worker bails so it doesn't
/// keep spawning ffmpegs for a video the user is no longer interested in.
static JOB_GENERATION: AtomicU64 = AtomicU64::new(0);

fn is_alive(my_gen: u64) -> bool {
    !SHUTTING_DOWN.load(Ordering::Relaxed) && JOB_GENERATION.load(Ordering::Relaxed) == my_gen
}

/// Kill every ffmpeg child currently tracked. Used both when a new job
/// supersedes an in-flight one, and at app shutdown.
fn kill_active_children() {
    let mut children = std::mem::take(&mut *ACTIVE_CHILDREN.lock().unwrap());
    for child in &mut children {
        let _ = child.kill();
    }
}

fn previews_dir() -> PathBuf {
    crate::mpv::app_data_dir().join("previews")
}

fn sprite_path_for(video: &Path) -> PathBuf {
    use md5::Digest;
    let key = video.to_string_lossy();
    let digest = md5::Md5::digest(key.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    previews_dir().join(format!("{hex}.jpg"))
}

fn meta_path_for(sprite: &Path) -> PathBuf {
    sprite.with_extension("json")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PreviewMeta {
    tile_w: u32,
    tile_h: u32,
}

fn write_meta(sprite: &Path, meta: &PreviewMeta) -> Result<(), String> {
    let path = meta_path_for(sprite);
    let json = serde_json::to_string(meta).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

fn read_meta(sprite: &Path) -> Option<PreviewMeta> {
    let json = std::fs::read_to_string(meta_path_for(sprite)).ok()?;
    serde_json::from_str(&json).ok()
}

fn is_cache_fresh(video: &Path, sprite: &Path) -> bool {
    let Ok(v) = std::fs::metadata(video) else {
        return false;
    };
    let Ok(s) = std::fs::metadata(sprite) else {
        return false;
    };
    match (v.modified(), s.modified()) {
        (Ok(vm), Ok(sm)) => sm >= vm,
        _ => false,
    }
}

/// Look up an existing cached sprite for a video, if one is valid.
pub fn cached_preview(video: &Path) -> Option<PreviewReady> {
    let sprite = sprite_path_for(video);
    if !is_cache_fresh(video, &sprite) {
        return None;
    }
    let meta = read_meta(&sprite)?;
    Some(PreviewReady {
        path: video.to_string_lossy().into_owned(),
        sprite: sprite.to_string_lossy().into_owned(),
        grid: GRID,
        tile_w: meta.tile_w,
        tile_h: meta.tile_h,
    })
}

/// Resolve the ffmpeg sidecar binary. In a bundled app the sidecar sits
/// next to the executable; in dev builds tauri-build copies it into
/// `target/*/`.
fn ffmpeg_path(app: &AppHandle) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidates = [
        exe_dir.join("ffmpeg-x86_64-pc-windows-msvc.exe"),
        exe_dir.join("ffmpeg.exe"),
        app.path()
            .resource_dir()
            .ok()
            .map(|d| d.join("ffmpeg-x86_64-pc-windows-msvc.exe"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn parse_duration_seconds(stderr: &str) -> Option<f64> {
    let key = "Duration:";
    let idx = stderr.find(key)?;
    let rest = &stderr[idx + key.len()..];
    let end = rest.find(',')?;
    let s = rest[..end].trim();
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].parse().ok()?;
    let m: f64 = parts[1].parse().ok()?;
    let sec: f64 = parts[2].parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Pull the source's display aspect ratio out of ffmpeg's probe stderr.
/// Prefers an explicit `DAR a:b`, falls back to storage WxH.
fn parse_display_aspect(stderr: &str) -> Option<f64> {
    if let Some(dar_idx) = stderr.find("DAR ") {
        let after = &stderr[dar_idx + 4..];
        let end = after.find(|c: char| c == ']' || c == ',' || c == ' ')?;
        let dar = &after[..end];
        let mut parts = dar.split(':');
        let n: f64 = parts.next()?.parse().ok()?;
        let d: f64 = parts.next()?.parse().ok()?;
        if d > 0.0 {
            return Some(n / d);
        }
    }
    let video_idx = stderr.find("Video:")?;
    let tail = &stderr[video_idx..];
    for token in tail.split(|c: char| c == ' ' || c == ',') {
        if let Some(x_pos) = token.find('x') {
            let (w_str, h_str) = token.split_at(x_pos);
            let h_str = &h_str[1..];
            if let (Ok(w), Ok(h)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                if w > 0 && h > 0 {
                    return Some(w as f64 / h as f64);
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn cmd_no_window(path: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(path);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

#[cfg(not(windows))]
fn cmd_no_window(path: &Path) -> Command {
    Command::new(path)
}

/// Spawn ffmpeg, register it in ACTIVE_CHILDREN so shutdown() or a
/// superseding request can kill it, then poll-wait. Multiple workers can
/// coexist because we don't hold the children-lock across the wait.
fn run_tracked(
    mut cmd: Command,
    my_gen: u64,
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    if !is_alive(my_gen) {
        return Err("superseded".into());
    }

    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    let stderr_pipe = child.stderr.take();
    let pid = child.id();

    ACTIVE_CHILDREN.lock().unwrap().push(child);

    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut s) = stderr_pipe {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });

    let status = loop {
        if !is_alive(my_gen) {
            return Err("superseded".into());
        }
        let poll = {
            let mut guard = ACTIVE_CHILDREN.lock().unwrap();
            let Some(c) = guard.iter_mut().find(|c| c.id() == pid) else {
                return Err("child was killed".into());
            };
            c.try_wait().map_err(|e| format!("wait: {e}"))?
        };
        match poll {
            Some(s) => break s,
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    };
    ACTIVE_CHILDREN.lock().unwrap().retain(|c| c.id() != pid);

    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    Ok((status, stderr_bytes))
}

/// Extract one tile by seeking to `timestamp` and grabbing a single frame.
/// `-ss` BEFORE `-i` is the key trick: it uses the container index to jump
/// directly without scanning preceding bytes.
fn extract_tile(
    ffmpeg: &Path,
    video: &Path,
    timestamp: f64,
    tile_w: u32,
    tile_h: u32,
    out: &Path,
    my_gen: u64,
) -> Result<(), String> {
    // Tile dimensions match the source DAR exactly — no padding, no bars.
    // yuvj420p (full-range) avoids the "Non full-range YUV is non-standard"
    // mjpeg encoder error on sources tagged with full range.
    let vf = format!("scale={tile_w}:{tile_h},format=yuvj420p");

    let mut cmd = cmd_no_window(ffmpeg);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-threads")
        .arg("1")
        .arg("-ss")
        .arg(format!("{timestamp:.3}"))
        .arg("-an")
        .arg("-sn")
        .arg("-i")
        .arg(video)
        .arg("-vf")
        .arg(&vf)
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("5")
        .arg(out);

    let (status, stderr) = run_tracked(cmd, my_gen)?;
    if !status.success() {
        let msg = String::from_utf8_lossy(&stderr);
        return Err(format!(
            "ffmpeg tile @ {timestamp:.3}s -> {} exited with {status}: {}",
            out.display(),
            msg.trim()
        ));
    }
    Ok(())
}

fn run_ffmpeg(ffmpeg: &Path, video: &Path, sprite: &Path, my_gen: u64) -> Result<u32, String> {
    // 1. Probe duration + display aspect ratio.
    let mut probe_cmd = cmd_no_window(ffmpeg);
    probe_cmd.arg("-hide_banner").arg("-i").arg(video);
    let (_status, probe_stderr) = run_tracked(probe_cmd, my_gen)?;
    let stderr = String::from_utf8_lossy(&probe_stderr);
    let duration = parse_duration_seconds(&stderr)
        .ok_or_else(|| format!("could not parse duration: {stderr}"))?;
    if duration <= 0.0 {
        return Err("video has zero duration".into());
    }

    // Derive tile width from the source DAR. Round to even for yuv420p
    // chroma alignment. Fall back to a sane default if probe didn't surface
    // DAR.
    let tile_w = match parse_display_aspect(&stderr) {
        Some(dar) if dar > 0.0 => {
            let raw = (TILE_H as f64 * dar).round() as u32;
            let even = (raw + 1) & !1;
            even.clamp(60, 600)
        }
        _ => TILE_W_FALLBACK,
    };

    // 2. Extract FRAMES tiles via -ss seek per process. For long files this
    //    is dramatically faster than a linear scan because each ffmpeg
    //    invocation jumps directly via the container index.
    let tile_dir = sprite.with_extension("tiles");
    std::fs::create_dir_all(&tile_dir).map_err(|e| format!("create tile dir: {e}"))?;
    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _guard = TempDir(tile_dir.clone());

    let step = duration / FRAMES as f64;
    let timestamps: Vec<(u32, f64)> = (0..FRAMES)
        // Sample at the midpoint of each segment so first/last tiles are
        // representative rather than dead-frame title cards / black.
        .map(|i| (i, (i as f64 + 0.5) * step))
        .collect();

    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel::<(u32, f64)>();
    let rx = std::sync::Arc::new(Mutex::new(rx));

    let workers: Vec<_> = (0..TILE_PARALLELISM)
        .map(|_| {
            let rx = rx.clone();
            let ffmpeg = ffmpeg.to_path_buf();
            let video = video.to_path_buf();
            let tile_dir = tile_dir.clone();
            std::thread::spawn(move || -> Result<(), String> {
                loop {
                    if !is_alive(my_gen) {
                        return Err("superseded".into());
                    }
                    let next = rx.lock().unwrap().recv();
                    let Ok((idx, ts)) = next else { return Ok(()) };
                    let out = tile_dir.join(format!("tile-{idx:04}.jpg"));
                    extract_tile(&ffmpeg, &video, ts, tile_w, TILE_H, &out, my_gen)?;
                }
            })
        })
        .collect();

    for ts in timestamps {
        if tx.send(ts).is_err() {
            break;
        }
    }
    drop(tx);

    for worker in workers {
        match worker.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("tile worker panicked".into()),
        }
    }

    // 3. Composite tiles into the final sprite. concat demuxer feeds the
    //    image sequence to the tile filter — no random access to the
    //    source video, just the small intermediates.
    if let Some(parent) = sprite.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let pattern = tile_dir.join("tile-%04d.jpg");
    let mut compose_cmd = cmd_no_window(ffmpeg);
    compose_cmd
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-threads")
        .arg("2")
        .arg("-i")
        .arg(&pattern)
        .arg("-vf")
        .arg(format!("tile={g}x{g}", g = GRID))
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("5")
        .arg(sprite);

    let (status, _stderr) = run_tracked(compose_cmd, my_gen)?;
    if !status.success() {
        let _ = std::fs::remove_file(sprite);
        return Err(format!("ffmpeg compose exited with {status}"));
    }

    if let Err(e) = write_meta(
        sprite,
        &PreviewMeta {
            tile_w,
            tile_h: TILE_H,
        },
    ) {
        log::warn!("failed to write preview meta: {e}");
    }

    Ok(tile_w)
}

/// Kill any running ffmpeg children. Called from the app's close handler.
pub fn shutdown() {
    SHUTTING_DOWN.store(true, Ordering::Relaxed);
    kill_active_children();
    let mut children = std::mem::take(&mut *ACTIVE_CHILDREN.lock().unwrap());
    for child in &mut children {
        let _ = child.wait();
    }
}

/// Queue a preview-generation job for `video_path`. If a sprite is already
/// cached, emit the ready event immediately. Otherwise spawn a worker
/// thread; switching videos fast cancels the previous job via
/// JOB_GENERATION + child kill.
pub fn request_preview(app: &AppHandle, video_path: &Path) {
    if SHUTTING_DOWN.load(Ordering::Relaxed) {
        return;
    }

    let path_str = video_path.to_string_lossy().into_owned();
    let sprite = sprite_path_for(video_path);

    if let Some(ready) = cached_preview(video_path) {
        let _ = app.emit("preview://ready", ready);
        return;
    }

    {
        let mut active = ACTIVE_JOB.lock().unwrap();
        if active.as_deref() == Some(path_str.as_str()) {
            return; // already running for this path
        }
        *active = Some(path_str.clone());
    }

    let my_gen = JOB_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    kill_active_children();

    let Some(ffmpeg) = ffmpeg_path(app) else {
        log::warn!("ffmpeg sidecar not found, skipping preview generation");
        *ACTIVE_JOB.lock().unwrap() = None;
        return;
    };

    let app = app.clone();
    let video = video_path.to_path_buf();
    std::thread::spawn(move || {
        let result = run_ffmpeg(&ffmpeg, &video, &sprite, my_gen);
        match result {
            Ok(tile_w) if is_alive(my_gen) => {
                let _ = app.emit(
                    "preview://ready",
                    PreviewReady {
                        path: path_str.clone(),
                        sprite: sprite.to_string_lossy().into_owned(),
                        grid: GRID,
                        tile_w,
                        tile_h: TILE_H,
                    },
                );
            }
            Ok(_) => { /* superseded — sprite is cached for next time */ }
            Err(e) => log::debug!("preview stopped for {}: {}", video.display(), e),
        }
        let mut active = ACTIVE_JOB.lock().unwrap();
        if active.as_deref() == Some(path_str.as_str()) {
            *active = None;
        }
    });
}
