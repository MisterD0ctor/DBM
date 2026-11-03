//! Files on disk, and what to call them.
//!
//! What opening a path means — a folder, a season, the files beside one —
//! and what each file is: its name read for the show, season and episode in
//! it (with `dbm_naming`), how long it is and how far in you got, its title
//! tag, a sheet of its frames for the seek preview, and the black bars its
//! frame carries around the picture (`bars`). And at either end of
//! watching, what to offer: the film left unfinished, and where to go when a
//! list runs out, and a still to light the window with, from the film or from
//! the player's own logo (`mark`).
//! In between, what can be heard in it: the timing its subtitles want, and
//! where an episode's title sequence and closing credits are. And from
//! outside: subtitles fetched from OpenSubtitles for a film without any.
//!
//! Nothing here touches the interface; most of it touches the disk, and runs
//! off the UI thread, on threads of its own (`threads`). Where on disk the
//! player keeps what it learns is `paths`.

pub mod bars;
pub mod cache;
pub mod durations;
pub mod ffmpeg;
pub mod fingerprint;
pub mod mark;
pub mod onward;
pub mod opensubtitles;
pub mod paths;
pub mod playlist;
pub mod preview;
pub mod probe;
pub mod real_paths;
pub mod resume;
pub mod shelf;
pub mod subsync;
pub mod threads;
pub mod titles;
