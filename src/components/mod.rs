//! Reusable UI primitives + player components.

#![allow(dead_code, unused_imports)] // exposed surface; consumers grow into it

pub mod action_overlay;
pub mod ambient_menu;
pub mod end_of_playback;
pub mod icon_button;
pub mod media_title;
pub mod menu;
pub mod playlist_menu;
pub mod seek_preview;
pub mod timeline;
pub mod toolbar;
pub mod tracks_menu;
pub mod video_surface;
pub mod volume_group;

pub use action_overlay::{ActionFeedback, ActionKind, ActionOverlay};
pub use ambient_menu::AmbientAnchor;
pub use end_of_playback::EndOfPlayback;
pub use icon_button::IconButton;
pub use media_title::MediaTitle;
pub use menu::Menu;
pub use playlist_menu::PlaylistAnchor;
pub use seek_preview::SeekPreview;
pub use timeline::Timeline;
pub use toolbar::{Reflow, ToolbarMainRow, ToolbarOverflowRow};
pub use tracks_menu::TracksAnchor;
pub use video_surface::VideoSurface;
pub use volume_group::VolumeGroup;
