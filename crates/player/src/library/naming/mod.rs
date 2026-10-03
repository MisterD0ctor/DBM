//! Turning file, track and chapter names into something worth reading.
//!
//! All of it guesswork over conventions nobody agreed on:
//!
//! * A filename like `Show.Name.S01E02.Episode.Title.1080p.WEB-DL.x264-GRP`
//!   has a show, a season, an episode and a title in it, wrapped in release
//!   metadata that means nothing once the file is playing. Fansub releases
//!   often have no season at all — `[Group] Show - 01 (1080p)` — and number
//!   a show's episodes straight through. See `media`.
//! * A track carries a language code, sometimes a title, and — for external
//!   subtitles, where mpv uses the filename as the title — the same release
//!   junk again, plus the flags that actually matter: forced, SDH,
//!   commentary. See `tracks` and `language`.
//! * A chapter is called `End Credits`, or `Outro`, or its own start time.
//!   See `chapters`.
//!
//! `clean` holds what all of them share. Everything here is a pure function
//! over strings. No mpv, no UI, no I/O: the parsing is all guesswork and
//! guesswork wants tests, which is easiest when there is nothing to set up
//! first.

mod chapters;
mod clean;
mod language;
mod media;
mod tracks;

pub use chapters::{is_credits, is_unnamed_chapter};
pub use clean::strip_path;
pub use media::{described, listing, parse, titled, without_season, Media};
pub use tracks::{track_labels, TrackInfo};
