use std::ffi::c_void;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
];

// --- Playlists ----------------------------------------------------------------

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
    let mut writer = BufWriter::new(fs::File::create(&playlist_path).map_err(|e| e.to_string())?);
    writeln!(writer, "#EXTM3U").map_err(|e| e.to_string())?;
    for video in &videos {
        writeln!(writer, "{}", video.display()).map_err(|e| e.to_string())?;
    }
    Ok(playlist_path)
}

#[tauri::command]
fn playlist_from_directory(path: &str) -> Result<(String, usize), String> {
    let dir = PathBuf::from(path);
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

// --- Windows System Media Transport Controls (SMTC) ---------------------------

// Managed state wrapper
pub struct SmtcState(pub Mutex<Option<SystemMediaTransportControls>>);

fn get_smtc_for_hwnd(hwnd: *mut c_void) -> windows::core::Result<SystemMediaTransportControls> {
    let interop = windows::core::factory::<
        SystemMediaTransportControls,
        ISystemMediaTransportControlsInterop,
    >()?;
    unsafe { interop.GetForWindow(windows::Win32::Foundation::HWND(hwnd as *mut c_void)) }
}

#[tauri::command]
fn setup_smtc(app: AppHandle, state: State<SmtcState>) {
    let window = app.get_webview_window("main").unwrap();
    let hwnd = window.hwnd().unwrap().0 as *mut c_void;

    let controls = get_smtc_for_hwnd(hwnd).expect("Failed to get SMTC for window");

    controls.SetIsEnabled(true).unwrap();
    controls.SetIsPlayEnabled(true).unwrap();
    controls.SetIsPauseEnabled(true).unwrap();
    controls.SetIsNextEnabled(true).unwrap();
    controls.SetIsPreviousEnabled(true).unwrap();
    controls
        .SetPlaybackStatus(MediaPlaybackStatus::Playing)
        .unwrap();

    let app_clone = app.clone();
    controls
        .ButtonPressed(&windows::Foundation::TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_, args| {
            let button = args.as_ref().unwrap().Button().unwrap();
            let cmd = match button {
                SystemMediaTransportControlsButton::Play => "play",
                SystemMediaTransportControlsButton::Pause => "pause",
                SystemMediaTransportControlsButton::Next => "next",
                SystemMediaTransportControlsButton::Previous => "previous",
                _ => return Ok(()),
            };
            app_clone.emit("smtc-command", cmd).ok();
            Ok(())
        }))
        .unwrap();

    // Store the instance in managed state
    *state.0.lock().unwrap() = Some(controls);
}

#[tauri::command]
fn update_smtc_playback(state: State<SmtcState>, playing: bool) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let controls = guard
        .as_ref()
        .ok_or("SMTC not initialized — call setup_smtc first")?;

    let status = if playing {
        MediaPlaybackStatus::Playing
    } else {
        MediaPlaybackStatus::Paused
    };

    controls
        .SetPlaybackStatus(status)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_smtc_metadata(
    state: State<SmtcState>,
    title: String,
    subtitle: String,
) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let controls = guard
        .as_ref()
        .ok_or("SMTC not initialized — call setup_smtc first")?;

    let updater = controls.DisplayUpdater().map_err(|e| e.to_string())?;
    updater
        .SetType(MediaPlaybackType::Video)
        .map_err(|e| e.to_string())?;

    let props = updater.VideoProperties().map_err(|e| e.to_string())?;

    // Bind HSTRINGs before passing references — avoids use-after-free
    let title_hs: windows::core::HSTRING = title.into();
    let subtitle_hs: windows::core::HSTRING = subtitle.into();
    props.SetTitle(&title_hs).map_err(|e| e.to_string())?;
    props.SetSubtitle(&subtitle_hs).map_err(|e| e.to_string())?;

    updater.Update().map_err(|e| e.to_string())
}

// Optional: update both at once to keep state consistent
#[tauri::command]
fn update_smtc(
    state: State<SmtcState>,
    playing: bool,
    title: String,
    subtitle: String,
) -> Result<(), String> {
    update_smtc_playback(state.clone(), playing)?;
    update_smtc_metadata(state, title, subtitle)
}

// Call this on app exit or when tearing down the player
#[tauri::command]
fn teardown_smtc(state: State<SmtcState>) {
    *state.0.lock().unwrap() = None;
}

// --- Startup file -------------------------------------------------------------

pub struct StartupFile(pub Mutex<Option<String>>);

#[tauri::command]
fn get_startup_file(state: State<StartupFile>) -> Option<String> {
    state.0.lock().unwrap().take() // take() so it's only returned once
}

// ─── App entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_file = std::env::args().nth(1);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Fired when a second instance is launched while app is already running
            if let Some(path) = args.get(1) {
                app.emit("open-file", path).ok();
            }

            // Also bring the window to focus
            if let Some(window) = app.get_webview_window("main") {
                window.set_focus().ok();
            }
        }))
        .manage(StartupFile(Mutex::new(startup_file)))
        .manage(SmtcState(Mutex::new(None)))
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_libmpv::init())
        .invoke_handler(tauri::generate_handler![
            playlist_from_directory,
            playlist_from_video,
            playlist_from_path_dialog,
            setup_smtc,
            update_smtc_playback,
            update_smtc_metadata,
            update_smtc,
            teardown_smtc,
            get_startup_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
