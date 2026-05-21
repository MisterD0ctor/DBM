//! Windows System Media Transport Controls (SMTC) integration.
//!
//! Hooks the OS-level media overlay (volume flyout, lock screen,
//! Bluetooth headset transport buttons) into our mpv instance:
//! - SMTC button presses (Play / Pause / Next / Previous) drive mpv
//! - mpv's `pause` and `filename` property changes are pushed back to
//!   SMTC so the OS overlay stays in sync
//!
//! Lives behind `cfg(windows)` — gracefully no-ops elsewhere.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use log::{info, warn};
use tauri::{AppHandle, Listener, Manager};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

use crate::mpv::MpvPlayer;

// ---------------------------------------------------------------------------
// Managed state — `tauri::Builder::manage(SmtcState::default())`
// ---------------------------------------------------------------------------

#[derive(Default)]
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

    let hwnd = match window.hwnd() {
        Ok(h) => h.0 as *mut c_void,
        Err(e) => {
            warn!("SMTC setup: HWND unavailable ({e})");
            return;
        }
    };

    let interop = match windows::core::factory::<
        SystemMediaTransportControls,
        ISystemMediaTransportControlsInterop,
    >() {
        Ok(i) => i,
        Err(e) => {
            warn!("SMTC setup: factory failed ({e})");
            return;
        }
    };

    let controls: SystemMediaTransportControls =
        match unsafe { interop.GetForWindow(windows::Win32::Foundation::HWND(hwnd)) } {
            Ok(c) => c,
            Err(e) => {
                warn!("SMTC setup: GetForWindow failed ({e})");
                return;
            }
        };

    if let Err(e) = (|| -> windows::core::Result<()> {
        controls.SetIsEnabled(true)?;
        controls.SetIsPlayEnabled(true)?;
        controls.SetIsPauseEnabled(true)?;
        controls.SetIsNextEnabled(true)?;
        controls.SetIsPreviousEnabled(true)?;
        controls.SetPlaybackStatus(MediaPlaybackStatus::Playing)?;
        Ok(())
    })() {
        warn!("SMTC setup: configuring failed ({e})");
        return;
    }

    // SMTC button → mpv command. Runs on the SMTC event thread; spawn a
    // worker for the playlist nav cases so we don't block it.
    let app_for_buttons = app.clone();
    if let Err(e) = controls.ButtonPressed(
        &windows::Foundation::TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_, args| {
            if let Some(args) = args.as_ref() {
                if let Ok(button) = args.Button() {
                    on_button_pressed(&app_for_buttons, button);
                }
            }
            Ok(())
        }),
    ) {
        warn!("SMTC setup: ButtonPressed registration failed ({e})");
    }

    // mpv → SMTC sync. One Rust-side listener pulls typed property events
    // and forwards the relevant ones to the OS overlay.
    let app_for_listener = app.clone();
    app.clone().listen("mpv://property", move |event| {
        let prop: shared::MpvProperty = match serde_json::from_str(event.payload()) {
            Ok(p) => p,
            Err(_) => return,
        };
        match prop {
            shared::MpvProperty::Pause(paused) => {
                update_playback(&app_for_listener, !paused);
            }
            shared::MpvProperty::Filename(Some(name)) => {
                // Strip extension for a cleaner OS title.
                let title = match name.rfind('.') {
                    Some(idx) if idx > 0 => &name[..idx],
                    _ => name.as_str(),
                };
                update_metadata(&app_for_listener, title);
            }
            _ => {}
        }
    });

    let state = app.state::<SmtcState>();
    *state.0.lock().unwrap() = Some(controls);

    info!("SMTC initialized");
}

fn on_button_pressed(app: &AppHandle, button: SystemMediaTransportControlsButton) {
    let player = app.state::<Arc<MpvPlayer>>();

    match button {
        SystemMediaTransportControlsButton::Play => {
            if let Err(e) = player.set_property_raw("pause", "no") {
                warn!("SMTC play failed: {e}");
            }
        }
        SystemMediaTransportControlsButton::Pause => {
            if let Err(e) = player.set_property_raw("pause", "yes") {
                warn!("SMTC pause failed: {e}");
            }
        }
        SystemMediaTransportControlsButton::Next => spawn_playlist_nav(app, "playlist-next"),
        SystemMediaTransportControlsButton::Previous => spawn_playlist_nav(app, "playlist-prev"),
        _ => {}
    }
}

fn spawn_playlist_nav(app: &AppHandle, command: &'static str) {
    let app = app.clone();
    std::thread::spawn(move || {
        let player = app.state::<Arc<MpvPlayer>>();
        let _ = player.command(command, &[]);
        // mpv needs a moment after a playlist nav to actually start the new
        // file before we can unpause it.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = player.set_property_raw("pause", "no");
    });
}

// ---------------------------------------------------------------------------
// Updates from mpv property events
// ---------------------------------------------------------------------------

pub fn update_playback(app: &AppHandle, playing: bool) {
    let state = app.state::<SmtcState>();
    let guard = state.0.lock().unwrap();
    let Some(controls) = guard.as_ref() else {
        return;
    };
    let status = if playing {
        MediaPlaybackStatus::Playing
    } else {
        MediaPlaybackStatus::Paused
    };
    if let Err(e) = controls.SetPlaybackStatus(status) {
        warn!("SMTC playback update failed: {e}");
    }
}

pub fn update_metadata(app: &AppHandle, title: &str) {
    let state = app.state::<SmtcState>();
    let guard = state.0.lock().unwrap();
    let Some(controls) = guard.as_ref() else {
        return;
    };
    let updater = match controls.DisplayUpdater() {
        Ok(u) => u,
        Err(e) => {
            warn!("SMTC DisplayUpdater failed: {e}");
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

// ---------------------------------------------------------------------------
// Teardown
// ---------------------------------------------------------------------------

pub fn teardown(app: &AppHandle) {
    let state = app.state::<SmtcState>();
    *state.0.lock().unwrap() = None;
    info!("SMTC torn down");
}
