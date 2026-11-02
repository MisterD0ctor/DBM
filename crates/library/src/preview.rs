//! Thumbnails for the seek preview.
//!
//! One sprite atlas per file: a `GRID`×`GRID` grid of frames in a single
//! JPEG, so hovering the timeline costs an image lookup and a `source-clip`
//! rather than a decode. The UI is handed a picture and two offsets and knows
//! nothing about how either was produced.
//!
//! Built by ffmpeg; with none on the machine there are no thumbnails, and
//! the preview falls back to the timestamp alone (see `ffmpeg`).
//!
//! Every frame is extracted by its own `-ss` seek rather than by scanning.
//! A linear pass over a two-hour film to collect 64 frames costs minutes; 64
//! seeks into the container index cost seconds, and they parallelise.
//!
//! Each build gets its own thread rather than the shared worker, since it is
//! 64 ffmpeg spawns and the worker also keeps the track lists current, which
//! must not wait for thumbnails. Its ffmpeg runs behind the player (see
//! `ffmpeg::spawn_behind`), as builds mostly happen while a film is starting,
//! when the player is busiest.
//!
//! A [`Supersede`], called on every request, stops an abandoned file's
//! ffmpeg processes from outliving it.

use std::cell::RefCell;
use std::path::PathBuf;

use dbm_mpv::MediaPath;
use dbm_mpv::tracks::Chapter;

use crate::threads::Reporter;
use crate::threads::Supersede;

mod build;
mod cache;
mod decode;
mod still;

use build::build;
use decode::decode;
pub use still::{Still, lit_still, still};

/// Tiles across and down. 64 frames is roughly one per two minutes of a
/// feature: close enough to land on the right scene, cheap enough to build
/// while the film is starting.
const GRID: u32 = 8;
/// Tile height in pixels; the width follows the video's aspect. Even, for
/// yuv420p chroma.
const TILE_H: u32 = 108;

/// A finished atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub path: PathBuf,
    pub tile_w: u32,
    pub tile_h: u32,
    pub grid: u32,
}

impl Sprite {
    /// Top-left corner of the tile covering `fraction` of the file.
    pub fn tile_at(&self, fraction: f32) -> (u32, u32) {
        let total = (self.grid * self.grid).max(1);
        let index = ((fraction.clamp(0.0, 1.0) * total as f32) as u32).min(total - 1);
        (
            (index % self.grid) * self.tile_w,
            (index / self.grid) * self.tile_h,
        )
    }
}

/// Called on every request.
static BUILDS: Supersede = Supersede::new();

/// Build the atlas for one video off the UI thread, and report it like any
/// other background result when it is ready.
///
/// Returns immediately. A build that produces nothing says nothing: see
/// [`build`] for why that is the ordinary case rather than an error.
pub fn spawn(video: MediaPath, reporter: Reporter<Atlas>) {
    let ticket = BUILDS.next();
    crate::threads::spawn("dbm-preview", move || {
        let Some(sprite) = build(&video, ticket) else {
            return;
        };
        // Decoded here rather than on the UI thread, where it costs most of
        // a frame and shows as a hitch.
        let Some(rgba) = decode(&sprite) else {
            return;
        };
        if ticket.current() {
            reporter.send(Atlas { sprite, rgba });
        }
    });
}

/// A finished atlas with its picture already decoded, ready to be shown.
pub struct Atlas {
    pub sprite: Sprite,
    /// RGBA, top row first, [`Atlas::width`] by [`Atlas::height`].
    pub rgba: Vec<u8>,
}

impl Atlas {
    pub fn width(&self) -> u32 {
        self.sprite.tile_w * self.sprite.grid
    }

    pub fn height(&self) -> u32 {
        self.sprite.tile_h * self.sprite.grid
    }
}

/// What answering a hover needs, shared between the render loop that learns
/// it and the action that uses it.
///
/// A hover must not reach mpv, since the pointer moves at whatever rate the
/// mouse reports, and a blocking read there would stall the frame.
#[derive(Default)]
pub struct Preview(RefCell<Known>);

#[derive(Default)]
struct Known {
    duration: f64,
    sprite: Option<Sprite>,
    /// The file the current atlas belongs to, so a request is made once per
    /// file rather than once per frame.
    source: Option<MediaPath>,
    /// Where the pointer last asked about, so an atlas that arrives while the
    /// timeline is already hovered can fill in the tile straight away instead
    /// of waiting for the pointer to move.
    last: f32,
    /// The file's chapters, to name the one under the pointer.
    chapters: Vec<Chapter>,
}

/// What the preview says for a place on the timeline.
pub struct Answer {
    /// Where under the pointer is, in the film.
    pub seconds: f64,
    /// The chapter it is in, where the file names it; empty otherwise.
    pub chapter: String,
    /// The tile's corner in the atlas.
    pub x: i32,
    pub y: i32,
}

impl Preview {
    pub fn set_duration(&self, seconds: f64) {
        self.0.borrow_mut().duration = seconds;
    }

    /// Only copied when they differ, since this is asked every frame.
    pub fn set_chapters(&self, chapters: &[Chapter]) {
        let mut known = self.0.borrow_mut();
        if known.chapters != chapters {
            known.chapters = chapters.to_vec();
        }
    }

    pub fn set_sprite(&self, sprite: Option<Sprite>) {
        self.0.borrow_mut().sprite = sprite;
    }

    /// Whether this path is new, claiming it if so. The caller starts a build
    /// on `true` and does nothing on `false`.
    pub fn claim(&self, path: &MediaPath) -> bool {
        let mut known = self.0.borrow_mut();
        if known.source.as_ref() == Some(path) {
            return false;
        }
        known.source = Some(path.clone());
        true
    }

    /// The answer for wherever the pointer last was. For when what is known
    /// changes rather than where the pointer is.
    pub fn again(&self) -> Answer {
        let last = self.0.borrow().last;
        self.answer(last)
    }

    /// The timestamp under the pointer, the chapter it is in, and the tile
    /// to show with them.
    pub fn answer(&self, fraction: f32) -> Answer {
        let mut known = self.0.borrow_mut();
        known.last = fraction;
        let seconds = known.duration * f64::from(fraction.clamp(0.0, 1.0));
        let (x, y) = match &known.sprite {
            Some(sprite) => sprite.tile_at(fraction),
            None => (0, 0),
        };
        Answer {
            seconds,
            chapter: named_chapter(&known.chapters, seconds),
            x: x as i32,
            y: y as i32,
        }
    }
}

/// The title of the chapter `seconds` falls in, unless it is only a
/// placeholder a muxer wrote, its number or its start time, which says
/// nothing the time beside it doesn't.
fn named_chapter(chapters: &[Chapter], seconds: f64) -> String {
    chapters
        .iter()
        .take_while(|c| c.time <= seconds + 1e-3)
        .last()
        .and_then(|c| c.title.as_deref())
        .filter(|title| !dbm_naming::is_unnamed_chapter(title))
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite() -> Sprite {
        Sprite {
            path: PathBuf::new(),
            tile_w: 192,
            tile_h: 108,
            grid: 8,
        }
    }

    #[test]
    fn tiles_run_left_to_right_then_down() {
        let s = sprite();
        assert_eq!(s.tile_at(0.0), (0, 0));
        // Second tile of the first row.
        assert_eq!(s.tile_at(1.0 / 64.0), (192, 0));
        // First tile of the second row.
        assert_eq!(s.tile_at(8.0 / 64.0), (0, 108));
    }

    #[test]
    fn the_end_lands_on_the_last_tile_not_past_it() {
        let s = sprite();
        assert_eq!(s.tile_at(1.0), (7 * 192, 7 * 108));
        assert_eq!(s.tile_at(2.0), (7 * 192, 7 * 108));
        assert_eq!(s.tile_at(-1.0), (0, 0));
    }

    #[test]
    fn a_chapter_is_named_only_where_the_file_names_it() {
        let chapter = |time, title: Option<&str>| Chapter {
            time,
            title: title.map(str::to_string),
        };
        let chapters = [
            chapter(0.0, Some("Prologue")),
            chapter(90.0, Some("Chapter 2")),
            chapter(300.0, Some("The Heist")),
            chapter(900.0, None),
        ];
        assert_eq!(named_chapter(&chapters, 30.0), "Prologue");
        assert_eq!(named_chapter(&chapters, 120.0), "");
        assert_eq!(named_chapter(&chapters, 300.0), "The Heist");
        assert_eq!(named_chapter(&chapters, 1000.0), "");
        assert_eq!(named_chapter(&[], 10.0), "");
    }
}
