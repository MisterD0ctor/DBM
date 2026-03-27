use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::net::UnixStream;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
];

fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| VIDEO_EXTENSIONS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
        .unwrap_or(false)
}

fn video_files_in_directory(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut videos: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory: {}", e))?
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_file(p))
        .collect();

    if videos.is_empty() {
        return Err("No video files found in the directory".to_string());
    }

    videos.sort();
    Ok(videos)
}

fn playlist_from_video_files(videos: Vec<PathBuf>) -> Result<PathBuf, String> {
    let playlist_path = std::env::temp_dir().join("tauri_playlist.m3u");
    let mut writer = BufWriter::new(File::create(&playlist_path).map_err(|e| e.to_string())?);
    writeln!(writer, "#EXTM3U").map_err(|e| e.to_string())?;
    for video in &videos {
        writeln!(writer, "{}", video.display()).map_err(|e| e.to_string())?;
    }
    Ok(playlist_path)
}

#[tauri::command]
fn playlist_from_directory(dir_path: &str) -> Result<(String, usize), String> {
    let dir = PathBuf::from(dir_path);
    if !dir.is_dir() {
        return Err("Provided path is not a directory".to_string());
    }
    let videos = video_files_in_directory(&dir)?;
    let playlist_path = playlist_from_video_files(videos)?;
    Ok((playlist_path.to_string_lossy().to_string(), 0))
}

#[tauri::command]
fn playlist_from_video(video_path: &str) -> Result<(String, usize), String> {
    let video_path = PathBuf::from(video_path);
    let dir = video_path
        .parent()
        .ok_or("Failed to get parent directory")?;
    let videos = video_files_in_directory(dir)?;
    let index = videos
        .iter()
        .position(|p| p == &video_path)
        .ok_or("Original video not found in directory")?;
    let playlist_path = playlist_from_video_files(videos)?;
    Ok((playlist_path.to_string_lossy().to_string(), index))
}

#[tauri::command]
fn playlist_from_path_dialog(app: tauri::AppHandle) -> Result<(String, usize), String> {
    let picked = tauri_plugin_dialog::DialogExt::dialog(&app)
        .file()
        .add_filter("Video Files", &VIDEO_EXTENSIONS)
        .blocking_pick_file();

    let path = match picked {
        Some(file_path) => file_path.into_path().map_err(|e| e.to_string())?,
        None => return Err("No file or folder selected".to_string()),
    };

    let path_str = path.to_string_lossy().to_string();
    if path.is_dir() {
        playlist_from_directory(&path_str)
    } else if path.is_file() && is_video_file(&path) {
        playlist_from_video(&path_str)
    } else {
        Err("Selected path is not a video file or directory".to_string())
    }
}

// ─── Ambient color sampling ───────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct AmbientColors {
    top: [u8; 3],
    bottom: [u8; 3],
    left: [u8; 3],
    right: [u8; 3],
}

/// Average the RGB values of a horizontal strip `strip_h` pixels tall at `y_start`.
fn avg_row_strip(img: &image::RgbImage, y_start: u32, strip_h: u32) -> [u8; 3] {
    let (w, h) = img.dimensions();
    let y_end = (y_start + strip_h).min(h);
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for y in y_start..y_end {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            r += px[0] as u64;
            g += px[1] as u64;
            b += px[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return [0, 0, 0];
    }
    [(r / n) as u8, (g / n) as u8, (b / n) as u8]
}

/// Average the RGB values of a vertical strip `strip_w` pixels wide at `x_start`.
fn avg_col_strip(img: &image::RgbImage, x_start: u32, strip_w: u32) -> [u8; 3] {
    let (w, h) = img.dimensions();
    let x_end = (x_start + strip_w).min(w);
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for y in 0..h {
        for x in x_start..x_end {
            let px = img.get_pixel(x, y);
            r += px[0] as u64;
            g += px[1] as u64;
            b += px[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return [0, 0, 0];
    }
    [(r / n) as u8, (g / n) as u8, (b / n) as u8]
}

#[cfg(windows)]
fn mpv_ipc_screenshot_raw(socket_path: &str) -> Result<Vec<u8>, String> {
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};

    // mpv on Windows expects the path as \\.\pipe\<name>
    // If the caller already passes the full pipe path, use it directly.
    // Otherwise, treat socket_path as just the pipe name and prefix it.
    let pipe_path = if socket_path.starts_with(r"\\.\pipe\") {
        socket_path.to_string()
    } else {
        format!(r"\\.\pipe\{}", socket_path)
    };

    // Named pipes on Windows are opened like regular files
    let pipe = (0..10)
        .find_map(
            |i| match OpenOptions::new().read(true).write(true).open(&pipe_path) {
                Ok(f) => Some(Ok(f)),
                Err(e) if i < 9 => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    None
                }
                Err(e) => Some(Err(format!("Pipe connect failed after retries: {e}"))),
            },
        )
        .unwrap_or_else(|| Err("Pipe not available".to_string()))?;

    let cmd = r#"{"command":["screenshot-raw","video"]}"#;
    let mut writer = BufWriter::new(&pipe);
    writer
        .write_all(format!("{cmd}\n").as_bytes())
        .map_err(|e| format!("Pipe write failed: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("Pipe flush failed: {e}"))?;

    let mut reader = BufReader::new(&pipe);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("Pipe read failed: {e}"))?;

    let v: serde_json::Value =
        serde_json::from_str(&line).map_err(|e| format!("JSON parse failed: {e}"))?;

    if v["error"].as_str() != Some("success") {
        return Err(format!("mpv error: {}", v["error"]));
    }

    let b64 = v["data"]["data"]
        .as_str()
        .ok_or("Missing data field in mpv response")?;

    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("Base64 decode failed: {e}"))
}

/// Sample ambient colors from the current video frame via mpv's IPC socket.
///
/// `socket_path` is the path passed to mpv's `--input-ipc-server` option.
/// Returns averaged RGB for the top, bottom, left, and right edge strips.
#[tauri::command]
fn sample_video_colors(socket_path: String) -> Result<AmbientColors, String> {
    let raw_ppm = mpv_ipc_screenshot_raw(&socket_path)?;

    // The raw bytes are a PPM image; load it then scale down for fast sampling
    let img = image::load_from_memory(&raw_ppm)
        .map_err(|e| format!("Image decode failed: {e}"))?
        .resize_exact(128, 72, image::imageops::FilterType::Nearest)
        .to_rgb8();

    let (_, h) = img.dimensions();
    let strip = (h / 8).max(1); // sample top/bottom 12.5% of height

    Ok(AmbientColors {
        top: avg_row_strip(&img, 0, strip),
        bottom: avg_row_strip(&img, h - strip, strip),
        left: avg_col_strip(&img, 0, 16), // leftmost 16 columns of 128
        right: avg_col_strip(&img, 112, 16), // rightmost 16 columns of 128
    })
}

// ─── App entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_libmpv::init())
        .invoke_handler(tauri::generate_handler![
            playlist_from_directory,
            playlist_from_video,
            playlist_from_path_dialog,
            sample_video_colors,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
