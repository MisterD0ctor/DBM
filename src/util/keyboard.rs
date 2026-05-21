//! Window-level keydown handler.
//!
//! One function installs every shortcut. Mirrors the switch statement in
//! [controls.js], but each branch reads state + fires a command +
//! optionally flashes the action overlay.

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{SeekMode, SeekPrecision};

use crate::bridge::commands;
use crate::components::{ActionFeedback, ActionKind};
use crate::state::PlayerState;
use crate::util::window::{enter_fullscreen, exit_fullscreen};

const SEEK_STEP: f64 = 10.0;
const VOLUME_STEP: f64 = 2.0;

pub fn install_shortcuts(state: PlayerState, fb: ActionFeedback) {
    let handle = window_event_listener(ev::keydown, move |ev: ev::KeyboardEvent| {
        // Don't capture typing in form controls.
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
                let tag = el.tag_name().to_ascii_lowercase();
                if matches!(tag.as_str(), "input" | "textarea" | "select") {
                    return;
                }
            }
        }

        match ev.code().as_str() {
            "Escape" => {
                if state.fullscreen.get_untracked() {
                    spawn_local(async move {
                        exit_fullscreen(state).await;
                    });
                }
            }
            "Space" => {
                if state.eof_reached.get_untracked() {
                    spawn_local(async move {
                        let _ = commands::seek(0.0, SeekMode::Absolute, SeekPrecision::Exact).await;
                        let _ = commands::play().await;
                    });
                    fb.show(ActionKind::Play, None, None);
                } else {
                    let next = !state.paused.get_untracked();
                    spawn_local(async move {
                        let _ = commands::set_pause(next).await;
                    });
                    fb.show(
                        if next { ActionKind::Pause } else { ActionKind::Play },
                        None,
                        None,
                    );
                }
            }
            "F11" | "KeyF" => {
                let next = !state.fullscreen.get_untracked();
                spawn_local(async move {
                    if next {
                        enter_fullscreen(state).await;
                    } else {
                        exit_fullscreen(state).await;
                    }
                });
                fb.show(
                    if next { ActionKind::FullscreenOn } else { ActionKind::FullscreenOff },
                    None,
                    None,
                );
            }
            "KeyM" => {
                let next = !state.muted.get_untracked();
                spawn_local(async move {
                    let _ = commands::set_mute(next).await;
                });
                fb.show(
                    if next { ActionKind::MuteOn } else { ActionKind::MuteOff },
                    None,
                    None,
                );
            }
            "KeyT" => {
                let cur = state.panscan.get_untracked();
                let next = if cur > 0.5 { 0.0 } else { 1.0 };
                spawn_local(async move {
                    let _ = commands::set_panscan(next).await;
                });
                fb.show(
                    if next > 0.5 { ActionKind::PanscanOn } else { ActionKind::PanscanOff },
                    None,
                    None,
                );
            }
            "KeyC" => {
                let next = !state.sub_visibility.get_untracked();
                spawn_local(async move {
                    let _ = commands::set_sub_visibility(next).await;
                });
                fb.show(
                    if next { ActionKind::SubtitlesOn } else { ActionKind::SubtitlesOff },
                    None,
                    None,
                );
            }
            "KeyB" => {
                let enabled = state.border_background.with_untracked(|b| b == "shader");
                let next = !enabled;
                spawn_local(async move {
                    let _ = commands::set_ambient_enabled(next).await;
                });
                fb.show(
                    if next { ActionKind::AmbientOn } else { ActionKind::AmbientOff },
                    None,
                    None,
                );
            }
            "ArrowUp" => {
                bump_volume(state, fb, VOLUME_STEP);
            }
            "ArrowDown" => {
                bump_volume(state, fb, -VOLUME_STEP);
            }
            "Home" => {
                spawn_local(async move {
                    let _ = commands::seek(0.0, SeekMode::Absolute, SeekPrecision::Exact).await;
                    let _ = commands::play().await;
                });
                fb.show(ActionKind::Rewind, None, None);
            }
            "KeyO" if ev.ctrl_key() => {
                if ev.shift_key() {
                    spawn_local(async move {
                        let _ = commands::open_folder_dialog().await;
                    });
                } else {
                    spawn_local(async move {
                        let _ = commands::open_video_dialog().await;
                    });
                }
            }
            "ArrowRight" => {
                if state.eof_reached.get_untracked() {
                    if state.is_last_video() {
                        spawn_local(async move {
                            let _ = commands::seek(0.0, SeekMode::Absolute, SeekPrecision::Exact).await;
                            let _ = commands::play().await;
                        });
                        fb.show(ActionKind::Rewind, None, None);
                    } else {
                        spawn_local(async move {
                            let _ = commands::playlist_next().await;
                            let _ = commands::play().await;
                        });
                        fb.show(ActionKind::Next, None, None);
                    }
                } else if ev.ctrl_key() {
                    spawn_local(async move {
                        let _ = commands::playlist_next().await;
                        let _ = commands::play().await;
                    });
                    fb.show(ActionKind::Next, None, None);
                } else {
                    spawn_local(async move {
                        let _ = commands::seek(SEEK_STEP, SeekMode::Relative, SeekPrecision::Exact).await;
                    });
                    fb.show(ActionKind::SeekForward, None, Some(80.0));
                }
            }
            "ArrowLeft" => {
                if ev.ctrl_key() {
                    spawn_local(async move {
                        let _ = commands::playlist_prev().await;
                        let _ = commands::play().await;
                    });
                    fb.show(ActionKind::Previous, None, None);
                } else {
                    spawn_local(async move {
                        let _ = commands::seek(-SEEK_STEP, SeekMode::Relative, SeekPrecision::Exact).await;
                    });
                    fb.show(ActionKind::SeekBackward, None, Some(20.0));
                }
            }
            _ => {}
        }
    });
    on_cleanup(move || handle.remove());

    // Browsers leave a clicked `<button>` focused, so a subsequent
    // Space/Enter re-triggers it on top of our shortcut handler — e.g.
    // clicking Play, then pressing Space pauses *and* re-plays. Blur the
    // active element after every click so keyboard shortcuts go straight
    // to the window handler instead.
    let click_handle = window_event_listener(ev::click, |_: ev::MouseEvent| {
        let Some(active) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.active_element())
        else {
            return;
        };
        if !active.tag_name().eq_ignore_ascii_case("button") {
            return;
        }
        if let Ok(el) = active.dyn_into::<web_sys::HtmlElement>() {
            let _ = el.blur();
        }
    });
    on_cleanup(move || click_handle.remove());
}

fn bump_volume(state: PlayerState, fb: ActionFeedback, delta: f64) {
    let next = (state.volume.get_untracked() + delta).clamp(0.0, 150.0);
    spawn_local(async move {
        let _ = commands::set_volume(next).await;
    });
    fb.show(ActionKind::Volume, Some(format!("{:.0}%", next)), None);
}

use wasm_bindgen::JsCast;
