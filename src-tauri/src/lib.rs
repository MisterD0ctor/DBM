mod mpv;
mod playlist;
mod smtc;

use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

use mpv::MpvPlayer;
use smtc::SmtcState;

// --- Commands ----------------------------------------------------------------

#[tauri::command]
fn load_video(player: tauri::State<MpvPlayer>, path: String) -> Result<(), String> {
    let video_path = PathBuf::from(&path);
    if !video_path.is_file() || !playlist::is_video_file(&video_path) {
        return Err("Not a valid video file".into());
    }
    playlist::load_video(&player, &video_path)
}

#[tauri::command]
fn open_video_dialog(app: AppHandle, player: tauri::State<MpvPlayer>) -> Result<(), String> {
    let picked = tauri_plugin_dialog::DialogExt::dialog(&app)
        .file()
        .add_filter("Video Files", playlist::video_extensions())
        .blocking_pick_file();

    let path = match picked {
        Some(file_path) => file_path.into_path().map_err(|e| e.to_string())?,
        None => return Ok(()),
    };

    if path.is_file() && playlist::is_video_file(&path) {
        playlist::load_video(&player, &path)
    } else {
        Err("Selected path is not a video file".into())
    }
}

// --- Startup (Rust-side, runs in .setup()) ------------------------------------

fn startup(app: &AppHandle) {
    let player = app.state::<MpvPlayer>();
    if let Err(e) = player.init(app) {
        log::error!("Failed to initialize mpv: {}", e);
        return;
    }

    let startup_path = std::env::args().nth(1).or_else(mpv::load_last_session);

    if let Some(path) = startup_path {
        let video_path = PathBuf::from(&path);
        if video_path.is_file() && playlist::is_video_file(&video_path) {
            if let Err(e) = playlist::load_video(&player, &video_path) {
                log::warn!("Failed to load startup file: {}", e);
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
        .manage(MpvPlayer::new())
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
                smtc::teardown(app);
                if let Some(player) = window.try_state::<MpvPlayer>() {
                    if let Err(e) = player.destroy() {
                        log::error!("Failed to destroy mpv on close: {}", e);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_video,
            open_video_dialog,
            mpv::commands::play,
            mpv::commands::pause,
            mpv::commands::toggle_pause,
            mpv::commands::seek,
            mpv::commands::set_volume,
            mpv::commands::set_speed,
            mpv::commands::playlist_play_index,
            mpv::commands::playlist_prev,
            mpv::commands::playlist_next,
            mpv::commands::set_property,
            mpv::commands::get_property,
            mpv::commands::get_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
