//! The commands the player sends mpv, with their arguments typed.

use std::borrow::Cow;
use std::path::Path;

use crate::prop::{Property, TrackChoice};
use crate::tracks::TrackId;

#[derive(Clone, Copy, Debug)]
pub enum Command<'a> {
    /// `exact` lands on the very frame, decoding from the keyframe before it.
    /// Otherwise mpv's `hr-seek` option decides.
    Seek {
        to: SeekTarget,
        exact: bool,
    },
    /// One frame. Forward pauses on arrival; back always seeks.
    FrameStep(Direction),
    /// Back from a little way into a chapter returns to its start first.
    ChapterStep(Direction),
    Add(Property<f64>, f64),
    Cycle(Property<bool>),
    /// The next or previous track of a kind, passing through none.
    CycleTrack(Property<TrackChoice>, Direction),
    /// The next of `values` after the one the property holds.
    CycleValues(Property<f64>, &'a [f64]),
    /// Replace the playlist with an m3u's entries.
    LoadList(&'a Path),
    /// Play an entry of the playlist, by its index from 0.
    PlaylistPlayIndex(usize),
    PlaylistNext,
    PlaylistPrev,
    /// Load a subtitle file. Selected when `select` is set; otherwise added
    /// to the list as the file's own tracks are.
    SubAdd {
        file: &'a Path,
        select: bool,
        title: Option<&'a str>,
        lang: Option<&'a str>,
    },
    /// Take out a track loaded from a file of its own, by its id.
    SubRemove(TrackId),
    AudioRemove(TrackId),
    /// Save where the playing file is, for mpv to resume it from.
    WriteWatchLaterConfig,
    /// Run a GLSL shader on the picture, after those already running.
    AppendShader(&'a Path),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SeekTarget {
    /// Seconds from where playback is.
    By(f64),
    /// Seconds from the start.
    To(f64),
    /// 0 to 100 of the file.
    Percent(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Back,
}

impl<'a> Command<'a> {
    /// The words mpv's command parser takes, command name first.
    pub(crate) fn args(self) -> Vec<Cow<'a, str>> {
        use Cow::Borrowed as B;
        let forward = |d| d == Direction::Forward;
        match self {
            Self::Seek { to, exact } => {
                let (amount, from) = match to {
                    SeekTarget::By(s) => (s, "relative"),
                    SeekTarget::To(s) => (s, "absolute"),
                    SeekTarget::Percent(p) => (p, "absolute-percent"),
                };
                let flags = if exact {
                    format!("{from}+exact").into()
                } else {
                    B(from)
                };
                vec![B("seek"), number(amount), flags]
            }
            Self::FrameStep(d) => vec![B(if forward(d) {
                "frame-step"
            } else {
                "frame-back-step"
            })],
            Self::ChapterStep(d) => {
                vec![
                    B("add"),
                    B("chapter"),
                    B(if forward(d) { "1" } else { "-1" }),
                ]
            }
            Self::Add(property, by) => vec![B("add"), B(property.name()), number(by)],
            Self::Cycle(property) => vec![B("cycle"), B(property.name())],
            Self::CycleTrack(property, d) => vec![
                B("cycle"),
                B(property.name()),
                B(if forward(d) { "up" } else { "down" }),
            ],
            Self::CycleValues(property, values) => [B("cycle-values"), B(property.name())]
                .into_iter()
                .chain(values.iter().copied().map(number))
                .collect(),
            Self::LoadList(list) => vec![B("loadlist"), list.to_string_lossy(), B("replace")],
            Self::PlaylistPlayIndex(index) => {
                vec![B("playlist-play-index"), index.to_string().into()]
            }
            Self::PlaylistNext => vec![B("playlist-next")],
            Self::PlaylistPrev => vec![B("playlist-prev")],
            Self::SubAdd {
                file,
                select,
                title,
                lang,
            } => {
                let flag = if select { "select" } else { "auto" };
                let mut args = vec![B("sub-add"), file.to_string_lossy(), B(flag)];
                // Positional, so a language needs a title before it.
                if title.is_some() || lang.is_some() {
                    args.push(B(title.unwrap_or_default()));
                }
                args.extend(lang.map(B));
                args
            }
            Self::SubRemove(id) => vec![B("sub-remove"), id.to_string().into()],
            Self::AudioRemove(id) => vec![B("audio-remove"), id.to_string().into()],
            Self::WriteWatchLaterConfig => vec![B("write-watch-later-config")],
            // A command rather than setting the option, which would split
            // the path on the list separator.
            Self::AppendShader(shader) => vec![
                B("change-list"),
                B("glsl-shaders"),
                B("append"),
                shader.to_string_lossy(),
            ],
        }
    }
}

/// Rust prints a float without an exponent and without regard to locale,
/// which is what mpv's parser takes.
fn number<'a>(n: f64) -> Cow<'a, str> {
    n.to_string().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prop;

    fn words(command: Command<'_>) -> Vec<String> {
        command.args().into_iter().map(Cow::into_owned).collect()
    }

    #[test]
    fn seeks_say_where_from_and_how_exactly() {
        let seek = |to, exact| words(Command::Seek { to, exact });
        assert_eq!(
            seek(SeekTarget::By(-10.0), true),
            ["seek", "-10", "relative+exact"]
        );
        assert_eq!(
            seek(SeekTarget::Percent(30.5), false),
            ["seek", "30.5", "absolute-percent"]
        );
        assert_eq!(
            seek(SeekTarget::To(0.0), true),
            ["seek", "0", "absolute+exact"]
        );
    }

    #[test]
    fn small_numbers_have_no_exponent() {
        assert_eq!(
            words(Command::Add(prop::SUB_DELAY, 1e-7)),
            ["add", "sub-delay", "0.0000001"]
        );
    }

    #[test]
    fn a_language_keeps_its_place_after_the_title() {
        let file = Path::new("/a.srt");
        let add = |title, lang| {
            words(Command::SubAdd {
                file,
                select: true,
                title,
                lang,
            })
        };
        assert_eq!(add(None, None), ["sub-add", "/a.srt", "select"]);
        assert_eq!(
            add(None, Some("en")),
            ["sub-add", "/a.srt", "select", "", "en"]
        );
        assert_eq!(
            add(Some("OpenSubtitles"), Some("en")),
            ["sub-add", "/a.srt", "select", "OpenSubtitles", "en"]
        );
    }

    #[test]
    fn tracks_cycle_by_their_selection_property() {
        assert_eq!(
            words(Command::CycleTrack(prop::SID, Direction::Back)),
            ["cycle", "sid", "down"]
        );
        assert_eq!(
            words(Command::CycleValues(prop::PANSCAN, &[1.0, 0.0])),
            ["cycle-values", "panscan", "1", "0"]
        );
    }
}
