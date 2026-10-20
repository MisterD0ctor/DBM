//! Turning file, track and chapter names into something worth reading.
//!
//! All of it guesswork over conventions nobody agreed on:
//!
//! * A filename like `Show.Name.S01E02.Episode.Title.1080p.WEB-DL.x264-GRP`
//!   has a show, a season, an episode and a title in it, wrapped in release
//!   metadata that means nothing once the file is playing. Fansub releases
//!   often have no season at all (`[Group] Show - 01 (1080p)`) and number a
//!   show's episodes straight through. See `media`.
//! * A track carries a language code, sometimes a title, the same release
//!   junk again for external subtitles (where mpv uses the filename as the
//!   title), and the flags that matter: forced, SDH, commentary. See `tracks`
//!   and `language`.
//! * A chapter is called `End Credits`, or `Outro`, or its own start time.
//!   See `chapters`.
//!
//! `clean` holds what all of them share. Everything here is a pure function
//! over strings, with no mpv, UI or I/O, because guesswork wants tests, which
//! are easiest with nothing to set up.

mod chapters;
mod clean;
mod language;
mod media;
mod tracks;

pub use chapters::{is_credits, is_intro, is_unnamed_chapter};
pub use clean::strip_path;
pub use language::language_name;
pub use media::{described, listing, parse, titled, without_season, Media};
pub use tracks::{track_labels, TrackInfo};
