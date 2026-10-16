//! Files on disk, and what to call them.
//!
//! What opening a path means — a folder, a season, the files beside one —
//! and what each file is: its name read for the show, season and episode in
//! it, how long it is and how far in you got, its title tag, and a sheet of
//! its frames for the seek preview. And at either end of watching, what to
//! offer: the film left unfinished, and where to go when a list runs out.
//! In between, what can be heard in it: the timing its subtitles want, and
//! where an episode's title sequence and closing credits are. And from
//! outside: subtitles fetched from OpenSubtitles for a film without any.
//!
//! Nothing here touches the interface; most of it touches the disk, and runs
//! off the UI thread.

pub mod cache;
pub mod durations;
pub mod ffmpeg;
pub mod fingerprint;
pub mod naming;
pub mod onward;
pub mod opensubtitles;
pub mod playlist;
pub mod preview;
pub mod probe;
pub mod resume;
pub mod shelf;
pub mod subsync;
pub mod titles;
