//! Background thumbnail-sprite generation for the seek tooltip.
//!
//! On file load we spawn ffmpeg in a worker thread to extract a GRID×GRID tile
//! atlas of preview frames. When finished, an event `preview://ready` is
//! emitted so the frontend can attach the sprite to its seek tooltip.
//!
//! Sprites are cached in the app data dir under `previews/<md5>.jpg` and keyed
//! off the video's path + mtime so edits invalidate old sprites.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter, Manager};

pub const GRID: u32 = 12;
pub const TILE_W: u32 = 160;
pub const TILE_H: u32 = 90;
const FRAMES: u32 = GRID * GRID;

/// Concurrent ffmpeg processes during sprite generation. More processes ≠
/// linearly faster: each one re-opens the file, and disk seek bandwidth caps
/// the gain. 4 is a good balance for SSDs; bump higher for NVMe if needed.
const TILE_PARALLELISM: usize = 8;

static ACTIVE_JOB: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static ACTIVE_CHILDREN: Lazy<Mutex<Vec<Child>>> = Lazy::new(|| Mutex::new(Vec::new()));
static SHUTTING_DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(serde::Serialize, Clone)]
pub struct PreviewReady {
    pub path: String,
    pub sprite: String,
    pub grid: u32,
    pub tile_w: u32,
    pub tile_h: u32,
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
    if is_cache_fresh(video, &sprite) {
        Some(PreviewReady {
            path: video.to_string_lossy().into_owned(),
            sprite: sprite.to_string_lossy().into_owned(),
            grid: GRID,
            tile_w: TILE_W,
            tile_h: TILE_H,
        })
    } else {
        None
    }
}

/// Resolve the ffmpeg sidecar binary. In a bundled app the sidecar sits next
/// to the executable; in dev builds tauri-build copies it into `target/*/`.
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
    // ffmpeg prints e.g. "Duration: 01:23:45.67, start: ..."
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

/// Spawn ffmpeg, register it in ACTIVE_CHILDREN so shutdown() can kill it,
/// then wait. Returns captured stderr + exit status. Poll-based so shutdown's
/// kill isn't blocked on a long wait, and so multiple workers can coexist.
fn run_tracked(mut cmd: Command) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    if SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("shutting down".into());
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

    // Poll try_wait so we don't hold the mutex across a blocking wait — other
    // workers and shutdown() need access to the children vec.
    let status = loop {
        if SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("shutting down".into());
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
fn extract_tile(ffmpeg: &Path, video: &Path, timestamp: f64, out: &Path) -> Result<(), String> {
    // yuvj420p (full-range JPEG variant) avoids the "Non full-range YUV is
    // non-standard" mjpeg encoder error on sources tagged with pc/full range.
    let vf = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,\
         pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,\
         format=yuvj420p",
        w = TILE_W,
        h = TILE_H,
    );

    let mut cmd = cmd_no_window(ffmpeg);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        // Single-threaded per process — we get parallelism from running
        // multiple processes, not from per-process threads.
        .arg("-threads")
        .arg("1")
        // Input-level fast seek: jumps via the container index, no byte scan.
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

    let (status, stderr) = run_tracked(cmd)?;
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

fn run_ffmpeg(ffmpeg: &Path, video: &Path, sprite: &Path) -> Result<(), String> {
    // 1. Probe duration.
    let mut probe_cmd = cmd_no_window(ffmpeg);
    probe_cmd.arg("-hide_banner").arg("-i").arg(video);
    let (_status, probe_stderr) = run_tracked(probe_cmd)?;
    let stderr = String::from_utf8_lossy(&probe_stderr);
    let duration = parse_duration_seconds(&stderr)
        .ok_or_else(|| format!("could not parse duration from ffmpeg output: {stderr}"))?;
    if duration <= 0.0 {
        return Err("video has zero duration".into());
    }

    // 2. Extract FRAMES individual tiles via -ss seek per process. For a long
    //    file this is dramatically faster than a single linear scan because
    //    each ffmpeg invocation jumps directly to its timestamp via the
    //    container index instead of demuxing the whole bitstream.
    let tile_dir = sprite.with_extension("tiles");
    std::fs::create_dir_all(&tile_dir).map_err(|e| format!("create tile dir: {e}"))?;
    // Cleanup guard so partial runs don't leave intermediates lying around.
    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _guard = TempDir(tile_dir.clone());

    let step = duration / FRAMES as f64;
    let timestamps: Vec<(u32, f64)> = (0..FRAMES)
        // Sample at the midpoint of each segment so the first/last tiles are
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
                    if SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err("shutting down".into());
                    }
                    let next = rx.lock().unwrap().recv();
                    let Ok((idx, ts)) = next else { return Ok(()) };
                    let out = tile_dir.join(format!("tile-{idx:04}.jpg"));
                    extract_tile(&ffmpeg, &video, ts, &out)?;
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
    //    image sequence to the tile filter — no random access to the source
    //    video, just the small intermediates.
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

    let (status, _stderr) = run_tracked(compose_cmd)?;
    if !status.success() {
        let _ = std::fs::remove_file(sprite);
        return Err(format!("ffmpeg compose exited with {status}"));
    }
    Ok(())
}

/// Kill any running ffmpeg children. Called from the app's close handler so
/// background preview jobs don't outlive the process.
pub fn shutdown() {
    SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut children = std::mem::take(&mut *ACTIVE_CHILDREN.lock().unwrap());
    for child in &mut children {
        let _ = child.kill();
    }
    for child in &mut children {
        let _ = child.wait();
    }
}

/// Queue a preview-generation job for `video_path`. If a sprite is already
/// cached, we emit the ready event immediately and skip ffmpeg. Otherwise we
/// spawn a worker thread. Only one job runs at a time; newer requests
/// supersede older ones (the older job still finishes but its result is
/// ignored by the frontend because the event carries the file path).
pub fn request_preview(app: &AppHandle, video_path: &Path) {
    if SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
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

    let Some(ffmpeg) = ffmpeg_path(app) else {
        log::warn!("ffmpeg sidecar not found, skipping preview generation");
        *ACTIVE_JOB.lock().unwrap() = None;
        return;
    };

    let app = app.clone();
    let video = video_path.to_path_buf();
    std::thread::spawn(move || {
        let result = run_ffmpeg(&ffmpeg, &video, &sprite);
        match result {
            Ok(()) => {
                let _ = app.emit(
                    "preview://ready",
                    PreviewReady {
                        path: path_str.clone(),
                        sprite: sprite.to_string_lossy().into_owned(),
                        grid: GRID,
                        tile_w: TILE_W,
                        tile_h: TILE_H,
                    },
                );
            }
            Err(e) => log::warn!("preview generation failed for {}: {}", video.display(), e),
        }
        let mut active = ACTIVE_JOB.lock().unwrap();
        if active.as_deref() == Some(path_str.as_str()) {
            *active = None;
        }
    });
}
