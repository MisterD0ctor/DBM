mod mpv;
mod persistence;
mod playlist;
mod preview;
#[cfg(windows)]
mod smtc;

use std::path::PathBuf;
use std::sync::Arc;

use mpv::{MpvPlayer, MpvResult};
use tauri::{AppHandle, Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // mpv parses option values with the C locale; force it so decimal
    // separators ("0.5") aren't reinterpreted on systems with comma locales.
    unsafe {
        let locale = std::ffi::CString::new("C").unwrap();
        libc::setlocale(libc::LC_NUMERIC, locale.as_ptr());
    }

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Second launch with a file argument — hand it to the running instance.
            if let Some(path) = args.get(1) {
                let _ = app.emit("open-file", path);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .manage(Arc::new(MpvPlayer::new()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(windows)]
    {
        builder = builder.manage(smtc::SmtcState::default());
    }

    builder
        .setup(|app| {
            let handle = app.handle().clone();
            // Initialize mpv off the main thread — `init` blocks on dll load
            // and window-handle resolution; doing it on `setup`'s thread
            // would delay the window appearing.
            std::thread::spawn(move || startup(&handle));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                #[cfg(windows)]
                smtc::teardown(window.app_handle());
                preview::shutdown();
                if let Some(player) = window.try_state::<Arc<MpvPlayer>>() {
                    if let Err(e) = player.destroy() {
                        log::error!("Failed to destroy mpv on close: {e}");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            mpv::commands::snapshot,
            mpv::commands::tracks,
            mpv::commands::playlist,
            mpv::commands::play,
            mpv::commands::pause,
            mpv::commands::set_pause,
            mpv::commands::seek,
            mpv::commands::set_mute,
            mpv::commands::set_volume,
            mpv::commands::set_panscan,
            mpv::commands::set_subtitle_track,
            mpv::commands::set_audio_track,
            mpv::commands::set_sub_visibility,
            mpv::commands::set_ambient_enabled,
            mpv::commands::apply_ambient_params,
            mpv::commands::load_ambient_params,
            mpv::commands::load_video,
            mpv::commands::load_folder,
            mpv::commands::open_video_dialog,
            mpv::commands::open_folder_dialog,
            mpv::commands::open_subtitle_dialog,
            mpv::commands::get_watch_later_positions,
            mpv::commands::get_preview,
            mpv::commands::show_snap_layouts,
            mpv::commands::playlist_play_index,
            mpv::commands::playlist_prev,
            mpv::commands::playlist_next,
            mpv::commands::set_property,
            mpv::commands::get_property,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn startup(app: &AppHandle) {
    let player = app.state::<Arc<MpvPlayer>>();
    if let Err(e) = player.init(app) {
        log::error!("Failed to initialize mpv: {e}");
        return;
    }
    if let Err(e) = apply_border_shader(app, &player, "ambient-border") {
        log::warn!("Failed to apply border shader: {e}");
    }

    // App-level concerns the wrapper deliberately stays out of.
    persistence::install_property_listener(app);
    persistence::spawn_watch_later_writer(app);

    #[cfg(windows)]
    smtc::setup(app);

    // What to load on launch, in priority order:
    //   1. CLI arg (file associations + first launch from explorer)
    //   2. Last playlist (everything we played last session)
    //   3. Last single file (if no playlist saved)
    //   4. Nothing — mpv idles
    if let Some(arg) = std::env::args().nth(1) {
        let path = PathBuf::from(&arg);
        if path.is_file() && playlist::is_video_file(&path) {
            if let Err(e) = playlist::load_video(&player, &path) {
                log::warn!("Failed to load CLI arg: {e}");
            }
            return;
        }
    }

    let last_file = persistence::load_last_session();
    if let Some(paths) = persistence::load_last_playlist() {
        // Resume on the file we left off on; falls back to index 0 if
        // last_session no longer matches anything in the saved playlist.
        let index = last_file
            .as_ref()
            .and_then(|p| paths.iter().position(|v| v.to_string_lossy() == *p))
            .unwrap_or(0);
        if let Err(e) = playlist::load_playlist(&player, &paths, index) {
            log::warn!("Failed to restore last playlist: {e}");
        }
        return;
    }
    if let Some(path) = last_file {
        let video_path = PathBuf::from(&path);
        if video_path.is_file() && playlist::is_video_file(&video_path) {
            if let Err(e) = playlist::load_video(&player, &video_path) {
                log::warn!("Failed to load last session: {e}");
            }
        }
    }
}

/// Load `shaders/<name>.glsl` from the bundled resource dir and tell mpv to
/// use it for the border-background region. `name` is validated to prevent
/// path traversal.
fn apply_border_shader(app: &AppHandle, player: &MpvPlayer, name: &str) -> MpvResult<()> {
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(mpv::MpvError::Ffi(format!("invalid shader name: {name}")));
    }
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| mpv::MpvError::Ffi(format!("resource_dir: {e}")))?;
    let path = resource_dir.join("shaders").join(format!("{name}.glsl"));
    if !path.is_file() {
        return Err(mpv::MpvError::Ffi(format!(
            "shader not found: {}",
            path.display()
        )));
    }
    player.set_property_raw("border-background-shader", &path.to_string_lossy())
}
