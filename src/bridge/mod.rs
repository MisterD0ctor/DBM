//! Frontend ↔ backend boundary. The only place that talks to Tauri.
//!
//! - [`commands`]: typed `invoke()` wrappers — one fn per `#[tauri::command]`.
//! - [`events`]: installs the property / event listeners and writes mpv
//!   updates straight into [`crate::state::PlayerState`].

pub mod commands;
pub mod events;
