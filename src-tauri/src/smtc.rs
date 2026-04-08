use std::ffi::c_void;
use std::sync::Mutex;

use log::{info, warn};
use tauri::{AppHandle, Manager};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

use crate::mpv::MpvPlayer;

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

pub struct SmtcState(pub Mutex<Option<SystemMediaTransportControls>>);

// ---------------------------------------------------------------------------
// Setup — call once after the window is created
// ---------------------------------------------------------------------------

pub fn setup(app: &AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => {
            warn!("SMTC setup: window 'main' not found, skipping");
            return;
        }
    };

    let hwnd = window.hwnd().expect("Failed to get HWND").0 as *mut c_void;

    let interop = windows::core::factory::<
        SystemMediaTransportControls,
        ISystemMediaTransportControlsInterop,
    >()
    .expect("Failed to get SMTC interop");

    let controls: SystemMediaTransportControls =
        unsafe { interop.GetForWindow(windows::Win32::Foundation::HWND(hwnd)) }
            .expect("Failed to get SMTC for window");

    controls.SetIsEnabled(true).unwrap();
    controls.SetIsPlayEnabled(true).unwrap();
    controls.SetIsPauseEnabled(true).unwrap();
    controls.SetIsNextEnabled(true).unwrap();
    controls.SetIsPreviousEnabled(true).unwrap();
    controls
        .SetPlaybackStatus(MediaPlaybackStatus::Playing)
        .unwrap();

    // Button presses → direct mpv commands (no JS round-trip)
    let app_clone = app.clone();
    controls
        .ButtonPressed(
            &windows::Foundation::TypedEventHandler::<
                SystemMediaTransportControls,
                SystemMediaTransportControlsButtonPressedEventArgs,
            >::new(move |_, args| {
                let button = args.as_ref().unwrap().Button().unwrap();
                on_button_pressed(&app_clone, button);
                Ok(())
            }),
        )
        .unwrap();

    let state = app.state::<SmtcState>();
    *state.0.lock().unwrap() = Some(controls);

    info!("SMTC initialized");
}

fn on_button_pressed(app: &AppHandle, button: SystemMediaTransportControlsButton) {
    let player = app.state::<std::sync::Arc<MpvPlayer>>();

    match button {
        SystemMediaTransportControlsButton::Play => {
            if let Err(e) = player.set_property_raw("pause", "no") {
                warn!("SMTC play failed: {}", e);
            }
        }
        SystemMediaTransportControlsButton::Pause => {
            if let Err(e) = player.set_property_raw("pause", "yes") {
                warn!("SMTC pause failed: {}", e);
            }
        }
        SystemMediaTransportControlsButton::Next => {
            let app = app.clone();
            std::thread::spawn(move || {
                let player = app.state::<std::sync::Arc<MpvPlayer>>();
                let _ = player.command("playlist-next", &[]);
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = player.set_property_raw("pause", "no");
            });
        }
        SystemMediaTransportControlsButton::Previous => {
            let app = app.clone();
            std::thread::spawn(move || {
                let player = app.state::<std::sync::Arc<MpvPlayer>>();
                let _ = player.command("playlist-prev", &[]);
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = player.set_property_raw("pause", "no");
            });
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Updates — called from the mpv event callback
// ---------------------------------------------------------------------------

pub fn update_playback(app: &AppHandle, playing: bool) {
    let state = app.state::<SmtcState>();
    let guard = state.0.lock().unwrap();
    if let Some(controls) = guard.as_ref() {
        let status = if playing {
            MediaPlaybackStatus::Playing
        } else {
            MediaPlaybackStatus::Paused
        };
        if let Err(e) = controls.SetPlaybackStatus(status) {
            warn!("SMTC playback update failed: {}", e);
        }
    }
}

pub fn update_metadata(app: &AppHandle, title: &str) {
    let state = app.state::<SmtcState>();
    let guard = state.0.lock().unwrap();
    if let Some(controls) = guard.as_ref() {
        let updater = match controls.DisplayUpdater() {
            Ok(u) => u,
            Err(e) => {
                warn!("SMTC DisplayUpdater failed: {}", e);
                return;
            }
        };

        let _ = updater.SetType(MediaPlaybackType::Video);

        if let Ok(props) = updater.VideoProperties() {
            let title_hs: windows::core::HSTRING = title.into();
            let _ = props.SetTitle(&title_hs);
        }

        let _ = updater.Update();
    }
}

// ---------------------------------------------------------------------------
// Teardown
// ---------------------------------------------------------------------------

pub fn teardown(app: &AppHandle) {
    let state = app.state::<SmtcState>();
    *state.0.lock().unwrap() = None;
    info!("SMTC torn down");
}
