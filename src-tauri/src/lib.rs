mod mpv;
mod playlist;
mod preview;
mod smtc;

use std::{path::PathBuf, sync::Arc};
use tauri::{AppHandle, Emitter, Manager};

use mpv::MpvPlayer;
use smtc::SmtcState;

// --- Commands ----------------------------------------------------------------

#[tauri::command]
fn load_video(player: tauri::State<Arc<MpvPlayer>>, path: String) -> Result<(), String> {
    let video_path = PathBuf::from(&path);
    if !video_path.is_file() || !playlist::is_video_file(&video_path) {
        return Err("Not a valid video file".into());
    }
    playlist::load_video(&player, &video_path)
}

#[tauri::command]
fn open_video_dialog(app: AppHandle, player: tauri::State<Arc<MpvPlayer>>) -> Result<(), String> {
    log::info!("Opened video selector");

    let player = Arc::clone(&player);

    let window = app
        .get_webview_window("main")
        .ok_or("window 'main' not found")?;

    tauri_plugin_dialog::DialogExt::dialog(&app)
        .file()
        .set_parent(&window)
        .add_filter("Video Files", playlist::video_extensions())
        .pick_file(move |picked| {
            let path = match picked {
                Some(file_path) => match file_path.into_path() {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("Failed to resolve path: {e}");
                        return;
                    }
                },
                None => {
                    log::info!("No video selected");
                    return;
                }
            };

            if path.is_file() && playlist::is_video_file(&path) {
                if let Err(e) = playlist::load_video(&player, &path) {
                    log::error!("Failed to load video: {e}");
                }
            } else {
                log::error!("Selected path is not a video file");
            }
        });

    Ok(())
}

#[tauri::command]
fn open_folder_dialog(app: AppHandle, player: tauri::State<Arc<MpvPlayer>>) -> Result<(), String> {
    log::info!("Opened folder selector");

    let player = Arc::clone(&player);

    let window = app
        .get_webview_window("main")
        .ok_or("window 'main' not found")?;

    tauri_plugin_dialog::DialogExt::dialog(&app)
        .file()
        .set_parent(&window)
        .pick_folder(move |picked| {
            let path = match picked {
                Some(folder_path) => match folder_path.into_path() {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("Failed to resolve path: {e}");
                        return;
                    }
                },
                None => {
                    log::info!("No folder selected");
                    return;
                }
            };

            if path.is_dir() {
                if let Err(e) = playlist::load_folder(&player, &path) {
                    log::error!("Failed to load folder: {e}");
                }
            } else {
                log::error!("Selected path is not a folder");
            }
        });

    Ok(())
}

/// Saved progress info for a single file.
#[derive(serde::Serialize, Clone)]
struct WatchProgress {
    start: f64,
    duration: f64,
}

/// Look up saved watch-later positions for a list of file paths.
/// Returns a map of path → { start, duration }.
#[tauri::command]
fn get_watch_later_positions(
    paths: Vec<String>,
) -> std::collections::HashMap<String, WatchProgress> {
    use md5::Digest;

    let watch_dir = mpv::app_data_dir().join("watch_later");
    let durations = mpv::load_duration_cache();
    let mut result = std::collections::HashMap::new();

    for path in &paths {
        let hash = md5::Md5::digest(path.as_bytes());
        let filename = hash
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();
        let file_path = watch_dir.join(&filename);

        if let Ok(contents) = std::fs::read_to_string(&file_path) {
            for line in contents.lines() {
                if let Some(val) = line.strip_prefix("start=") {
                    if let Ok(start) = val.parse::<f64>() {
                        let duration = durations.get(path).copied().unwrap_or(0.0);
                        if duration > 0.0 {
                            result.insert(path.clone(), WatchProgress { start, duration });
                        }
                    }
                    break;
                }
            }
        }
    }

    result
}

/// Load a shader from the bundled `shaders/` resource dir and set it as mpv's
/// border-background shader. `name` is the basename without extension, e.g.
/// "ambient-border" → `shaders/ambient-border.glsl`.
fn apply_border_shader(app: &AppHandle, player: &MpvPlayer, name: &str) -> Result<(), String> {
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("invalid shader name: {name}"));
    }

    let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
    let path = resource_dir.join("shaders").join(format!("{name}.glsl"));
    if !path.is_file() {
        return Err(format!("shader not found: {}", path.display()));
    }

    player
        .set_property_raw("border-background-shader", &path.to_string_lossy())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_preview(path: String) -> Option<preview::PreviewReady> {
    preview::cached_preview(std::path::Path::new(&path))
}

#[tauri::command]
fn set_border_shader_options(
    player: tauri::State<Arc<MpvPlayer>>,
    opts: String,
) -> Result<(), String> {
    player
        .set_property_raw("border-background-shader-opts", &opts)
        .map_err(|e| e.to_string())
}

fn ambient_params_path() -> PathBuf {
    mpv::app_data_dir().join("ambient-params.json")
}

#[tauri::command]
fn save_ambient_params(values: serde_json::Value) -> Result<(), String> {
    let path = ambient_params_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&values).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_ambient_params() -> Option<serde_json::Value> {
    let path = ambient_params_path();
    let json = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}

// --- Startup (Rust-side, runs in .setup()) ------------------------------------

fn startup(app: &AppHandle) {
    let player = app.state::<Arc<MpvPlayer>>();
    if let Err(e) = player.init(app) {
        log::error!("Failed to initialize mpv: {}", e);
        return;
    }

    if let Err(e) = apply_border_shader(app, &player, "ambient-border") {
        log::warn!("Failed to apply border shader: {}", e);
    }

    // Periodically save watch-later so playback position survives crashes
    let player_ref = Arc::clone(&player);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if player_ref.is_file_loaded() {
            let _ = player_ref.write_watch_later();
        }
    });

    // CLI argument takes precedence — treat it as a single-file open.
    if let Some(arg_path) = std::env::args().nth(1) {
        let video_path = PathBuf::from(&arg_path);
        if video_path.is_file() && playlist::is_video_file(&video_path) {
            if let Err(e) = playlist::load_video(&player, &video_path) {
                log::warn!("Failed to load startup file: {}", e);
            }
            return;
        }
    }

    // Otherwise, restore the last playlist + position. Fall back to the single
    // last-played file if the playlist is missing or doesn't contain it.
    let last_file = mpv::load_last_session();
    if let Some(paths) = mpv::load_last_playlist() {
        let index = last_file
            .as_ref()
            .and_then(|p| paths.iter().position(|v| v.to_string_lossy() == *p))
            .unwrap_or(0);
        if let Err(e) = playlist::load_playlist(&player, &paths, index) {
            log::warn!("Failed to restore last playlist: {}", e);
        }
        return;
    }
    if let Some(path) = last_file {
        let video_path = PathBuf::from(&path);
        if video_path.is_file() && playlist::is_video_file(&video_path) {
            if let Err(e) = playlist::load_video(&player, &video_path) {
                log::warn!("Failed to load last session: {}", e);
            }
        }
    }
}

// --- App entry point ---------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    unsafe {
        let locale = std::ffi::CString::new("C").unwrap();
        libc::setlocale(libc::LC_NUMERIC, locale.as_ptr());
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(path) = args.get(1) {
                app.emit("open-file", path).ok();
            }
            if let Some(window) = app.get_webview_window("main") {
                window.set_focus().ok();
            }
        }))
        .manage(SmtcState(std::sync::Mutex::new(None)))
        .manage(Arc::new(MpvPlayer::new()))
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                startup(&handle);
                smtc::setup(&handle);
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();
                preview::shutdown();
                smtc::teardown(app);
                if let Some(player) = window.try_state::<Arc<MpvPlayer>>() {
                    if let Err(e) = player.destroy() {
                        log::error!("Failed to destroy mpv on close: {}", e);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_video,
            open_video_dialog,
            open_folder_dialog,
            get_watch_later_positions,
            set_border_shader_options,
            save_ambient_params,
            load_ambient_params,
            get_preview,
            mpv::commands::play,
            mpv::commands::pause,
            mpv::commands::toggle_pause,
            mpv::commands::seek,
            mpv::commands::playlist_play_index,
            mpv::commands::playlist_prev,
            mpv::commands::playlist_next,
            mpv::commands::set_property,
            mpv::commands::get_property,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
