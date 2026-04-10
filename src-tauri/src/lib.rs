mod mpv;
mod playlist;
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

// --- Startup (Rust-side, runs in .setup()) ------------------------------------

fn startup(app: &AppHandle) {
    let player = app.state::<Arc<MpvPlayer>>();
    if let Err(e) = player.init(app) {
        log::error!("Failed to initialize mpv: {}", e);
        return;
    }

    // Periodically save watch-later so playback position survives crashes
    let player_ref = Arc::clone(&player);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if player_ref.is_file_loaded() {
            let _ = player_ref.write_watch_later();
        }
    });

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
