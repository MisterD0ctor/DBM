//! The black bars a film carries inside its own frame, and the picture
//! between them.
//!
//! mpv takes bars encoded into the frame for picture, so the ambient border
//! grows out of black and Fill the window fills the window with them. Handed
//! the picture's own rectangle as `video-crop`, mpv lays out the picture
//! instead, and both follow by themselves.
//!
//! ffmpeg's `cropdetect` finds the lit rectangle of one frame, which in a
//! dark scene is smaller than the picture, and in a frame with burnt-in text
//! in a bar is larger. So a film is sampled across its length and only edges
//! that several frames agree on are believed (see [`settle`]); anything less
//! certain leaves the frame whole, as it would be without this.

use std::path::Path;
use std::process::Command;

use dbm_mpv::MediaPath;
use serde::{Deserialize, Serialize};

use crate::cache::PathMap;
use crate::ffmpeg;
use crate::threads::{Reporter, Supersede, Ticket};

/// Keyframes looked at across the film, enough that some of them light the
/// picture to its edges.
const SAMPLES: u32 = 16;
/// The stretch of the film they are taken from, as shares of its length:
/// past any logos it opens on, and short of its closing credits, which often
/// run across the whole frame, bars and all.
const BODY: (f64, f64) = (0.05, 0.9);
/// How many ffmpeg processes at once, as for the preview atlas.
const PARALLEL: usize = 8;
/// How many frames have to reach the narrowest bar seen for it to be the
/// picture's edge. Every well-lit frame stops there; reached by fewer, it is
/// something lit in the bar, or a film too dark to show where it ends.
const AGREE: usize = 3;
/// How close to the widest frame's edge another has to come to agree with
/// it, as a share of the frame.
const NEAR: f64 = 0.005;
/// The narrowest bar worth cropping, as a share of the frame. A 1.85 film in
/// a 16:9 frame has bars of 2%, and many encodes darken a row or two at the
/// edge, which is not a bar.
const NARROWEST: f64 = 0.01;
/// How far the two bars on one axis can differ, as a share of the frame.
/// Bars of different widths mean something lit in one of them, which a crop
/// would cut off.
const UNEVEN: f64 = 0.01;
/// Pixels taken off the picture at each bar, so the darkened row where it
/// meets the bar stays out of it: the ambient border is drawn from the
/// picture's outermost rows.
const INSET: u32 = 2;

/// What was found for each file: its picture, or `None` where it has no
/// bars to crop. Versioned as caches are (see `cache`), and bumped whenever
/// a film would be cropped differently.
///
/// 1. Sixteen keyframes from 5% to 90% of the film, edges where three agree,
///    even bars over 1%, two pixels in.
static FOUND: PathMap<Option<Crop>> = PathMap::cache("bars.json", 1);

/// The picture inside a frame, in the frame's own pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// One file's answer.
pub struct Found {
    pub path: MediaPath,
    pub crop: Option<Crop>,
}

/// Called on every run.
static RUNS: Supersede = Supersede::new();

/// Find the picture in each of `paths`, in order, and report each one, from
/// what was found before where there is an answer kept.
///
/// Returns immediately. A file that cannot be looked at (not on disk, no
/// ffmpeg, a run called off) is not reported, and is looked at again when
/// next asked for.
pub fn spawn(paths: Vec<MediaPath>, reporter: Reporter<Found>) {
    let ticket = RUNS.next();
    crate::threads::spawn("dbm-bars", move || {
        let known = FOUND.load();
        for path in paths {
            if !ticket.current() {
                return;
            }
            let crop = match known.get(&path) {
                Some(crop) => *crop,
                None => {
                    let Some(crop) = look(&path, ticket) else {
                        continue;
                    };
                    log::info!("bars in {path}: {crop:?}");
                    FOUND.set(&path, crop);
                    crop
                }
            };
            if !reporter.send(Found { path, crop }) {
                return;
            }
        }
    });
}

/// The picture in one file, `Some(None)` for a file without bars, or `None`
/// where it could not be looked at. **Blocking**: runs ffmpeg.
fn look(path: &MediaPath, ticket: Ticket) -> Option<Option<Crop>> {
    if !path.as_path().is_file() {
        return None;
    }
    let ffmpeg = ffmpeg::path()?;
    ffmpeg::announce(&ffmpeg);
    let report = ffmpeg::describe(&ffmpeg, path.as_path())?;
    // ffmpeg finds bars in the frame turned upright, and it is not certain
    // that mpv crops it the same way up.
    if turned(&report) {
        return Some(None);
    }
    let frame = ffmpeg::parse_size(&report)?;
    let duration = ffmpeg::parse_duration(&report).filter(|d| *d > 0.0)?;
    let seen = sample(&ffmpeg, path.as_path(), duration, ticket)?;
    // Too few frames decoded to agree on anything, as when this ffmpeg lacks
    // the codec, which is no reason to remember the file as having no bars.
    if seen.len() < AGREE {
        return None;
    }
    Some(settle(frame, &seen))
}

/// The lit rectangle of a keyframe at the middle of each of `SAMPLES` equal
/// slices of the film's `BODY`, `PARALLEL` at a time. `None` if the run was
/// called off.
fn sample(ffmpeg: &Path, video: &Path, duration: f64, ticket: Ticket) -> Option<Vec<Crop>> {
    let start = duration * BODY.0;
    let step = duration * (BODY.1 - BODY.0) / f64::from(SAMPLES);
    let mut seen = Vec::new();
    let mut pending = ffmpeg::Children::default();
    for index in 0..SAMPLES {
        if !ticket.current() {
            return None;
        }
        let at = start + step * (f64::from(index) + 0.5);
        if let Ok(child) = ffmpeg::spawn_behind(&mut detect(ffmpeg, video, at)) {
            pending.push(child);
        }
        if pending.len() >= PARALLEL || index + 1 == SAMPLES {
            pending.wait_each(|child| {
                if let Ok(out) = child.wait_with_output() {
                    seen.extend(parse_crop(&String::from_utf8_lossy(&out.stderr)));
                }
            });
        }
    }
    ticket.current().then_some(seen)
}

/// `cropdetect` on the one keyframe at or before `at`.
fn detect(ffmpeg: &Path, video: &Path, at: f64) -> Command {
    let mut command = ffmpeg::command(ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-nostdin")
        .arg("-nostats")
        // One thread each, as for the preview atlas, so eight at once leave
        // the player its cores.
        .arg("-threads")
        .arg("1")
        .arg("-filter_threads")
        .arg("1")
        .arg("-skip_frame")
        .arg("nokey")
        // Or the keyframe before `at` is decoded and thrown away, and a file
        // with none after it gives nothing.
        .arg("-noaccurate_seek")
        .arg("-ss")
        .arg(format!("{at:.3}"))
        .arg("-i")
        .arg(video)
        .arg("-map")
        .arg("0:v:0")
        .arg("-frames:v")
        .arg("1")
        // `round=2` rather than the default 16, which rounds the picture's
        // size down and would crop into it.
        .arg("-vf")
        .arg("cropdetect=round=2:reset=0:skip=0")
        .arg("-f")
        .arg("null")
        .arg("-");
    command
}

/// The last `crop=w:h:x:y` in ffmpeg's log. An all-black frame comes out
/// with negative numbers, and says nothing.
fn parse_crop(log: &str) -> Option<Crop> {
    let (_, spec) = log.rsplit_once("crop=")?;
    let mut numbers = spec
        .split_whitespace()
        .next()?
        .split(':')
        .map(|n| n.parse::<u32>().ok());
    let mut next = || numbers.next().flatten();
    let (w, h, x, y) = (next()?, next()?, next()?, next()?);
    (w > 0 && h > 0).then_some(Crop { x, y, w, h })
}

/// Whether the file asks to be turned as it plays, as a phone's video does:
/// `displaymatrix: rotation of -90.00 degrees`.
fn turned(report: &str) -> bool {
    report.split("rotation of ").skip(1).any(|rest| {
        rest.split_whitespace()
            .next()
            .and_then(|angle| angle.parse::<f64>().ok())
            .is_some_and(|angle| angle.rem_euclid(360.0) != 0.0)
    })
}

/// The picture every sample agrees on inside a `width` by `height` frame, or
/// `None` to leave the frame whole.
///
/// Each axis is settled on its own, so a bar that can't be trusted on one
/// leaves only that one whole.
fn settle((width, height): (u32, u32), seen: &[Crop]) -> Option<Crop> {
    let across = axis(
        width,
        seen.iter()
            .map(|c| (c.x, width.saturating_sub(c.x + c.w)))
            .unzip(),
    );
    let down = axis(
        height,
        seen.iter()
            .map(|c| (c.y, height.saturating_sub(c.y + c.h)))
            .unzip(),
    );
    if across == (0, width) && down == (0, height) {
        return None;
    }
    Some(Crop {
        x: across.0,
        y: down.0,
        w: across.1 - across.0,
        h: down.1 - down.0,
    })
}

/// Where the picture starts and ends along an axis `size` long, from each
/// sample's bar at its near and far ends: the whole axis unless both bars
/// are believed, wide enough and alike.
fn axis(size: u32, (near, far): (Vec<u32>, Vec<u32>)) -> (u32, u32) {
    let whole = (0, size);
    let (Some(near), Some(far)) = (edge(size, &near), edge(size, &far)) else {
        return whole;
    };
    let narrowest = share(size, NARROWEST);
    if near < narrowest || far < narrowest || near.abs_diff(far) > share(size, UNEVEN) {
        return whole;
    }
    // Even, as 4:2:0 chroma covers two pixels at a time.
    let start = (near + INSET).next_multiple_of(2);
    let end = size.saturating_sub(far + INSET) & !1;
    if start >= end {
        return whole;
    }
    (start, end)
}

/// The bar at one end of an axis: the narrowest any sample shows, if at
/// least `AGREE` of them come near it.
fn edge(size: u32, bars: &[u32]) -> Option<u32> {
    let narrowest = *bars.iter().min()?;
    let near = narrowest + share(size, NEAR);
    let agreeing = bars.iter().filter(|bar| **bar <= near).count();
    (agreeing >= AGREE).then_some(narrowest)
}

fn share(size: u32, fraction: f64) -> u32 {
    (f64::from(size) * fraction).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: (u32, u32) = (1920, 1080);

    fn crop(w: u32, h: u32, x: u32, y: u32) -> Crop {
        Crop { x, y, w, h }
    }

    /// A 2.39 film in a 1080p frame, the way an episode of Fallout is
    /// encoded: most frames lit to the picture's edges, some darker.
    fn scope() -> Vec<Crop> {
        let mut seen = vec![crop(1920, 804, 0, 138); 10];
        seen.extend([
            crop(1824, 772, 96, 170),
            crop(1726, 796, 186, 140),
            crop(1020, 404, 900, 376),
        ]);
        seen
    }

    #[test]
    fn a_letterboxed_film_is_cropped_just_inside_its_picture() {
        assert_eq!(settle(HD, &scope()), Some(crop(1920, 800, 0, 140)));
    }

    #[test]
    fn a_film_lit_to_its_frame_is_left_whole() {
        let mut seen = vec![crop(1920, 1080, 0, 0); 6];
        seen.push(crop(1020, 704, 900, 376));
        assert_eq!(settle(HD, &seen), None);
    }

    #[test]
    fn a_dark_row_or_two_at_the_edge_is_not_a_bar() {
        let seen = vec![crop(1480, 1076, 0, 2); 8];
        assert_eq!(settle((1480, 1080), &seen), None);
    }

    #[test]
    fn a_pillarboxed_film_is_cropped_at_the_sides() {
        let seen = vec![crop(1440, 1080, 240, 0); 8];
        assert_eq!(settle(HD, &seen), Some(crop(1436, 1080, 242, 0)));
    }

    #[test]
    fn a_film_too_dark_to_agree_on_its_edges_is_left_whole() {
        let seen = [
            crop(1920, 804, 0, 138),
            crop(1700, 700, 110, 190),
            crop(1500, 600, 210, 240),
            crop(1300, 500, 310, 290),
        ];
        assert_eq!(settle(HD, &seen), None);
    }

    #[test]
    fn text_burnt_into_a_bar_leaves_that_axis_whole() {
        // Lines low in the bottom bar in several frames: the two bars
        // no longer match, and cropping would cut the lines off.
        let mut seen = scope();
        seen.extend(vec![crop(1200, 890, 360, 138); 4]);
        assert_eq!(settle(HD, &seen), None);
    }

    #[test]
    fn one_frame_lit_into_a_bar_leaves_that_axis_whole() {
        // A line or a logo in a single frame would be cut off with the bar.
        let mut seen = scope();
        seen.push(crop(1920, 900, 0, 138));
        assert_eq!(settle(HD, &seen), None);
    }

    #[test]
    fn a_bar_on_one_side_only_is_left_alone() {
        let seen = vec![crop(1920, 942, 0, 138); 8];
        assert_eq!(settle(HD, &seen), None);
    }

    #[test]
    fn cropdetect_is_read_off_its_last_line() {
        let log = "[Parsed_cropdetect_0 @ 0x5] x1:0 x2:1919 y1:138 y2:941 w:1920 h:804 \
                   x:0 y:138 pts:0 t:0.000000 limit:0.094118 crop=1920:804:0:138\n\
                   [out#0/null @ 0x6] video:1kB audio:0kB";
        assert_eq!(parse_crop(log), Some(crop(1920, 804, 0, 138)));
    }

    #[test]
    fn an_all_black_frame_says_nothing() {
        assert_eq!(
            parse_crop("limit:0.094118 crop=-1918:-1078:1920:1080"),
            None
        );
        assert_eq!(parse_crop("no frames decoded"), None);
    }

    #[test]
    fn only_a_real_turn_counts() {
        assert!(turned("displaymatrix: rotation of -90.00 degrees"));
        assert!(!turned("displaymatrix: rotation of -0.00 degrees"));
        assert!(!turned("Stream #0:0: Video: h264, 1920x1080"));
    }
}
