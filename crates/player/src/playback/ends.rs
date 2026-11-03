//! Where a file's title sequence and closing credits are: by its chapters'
//! names first, and otherwise where they were heard (see `titles`). What the
//! skips offer and where they land.

use super::state::PlayerState;

/// The longest last chapter that is taken for credits when no chapter is
/// named, in seconds.
const CREDITS_TAIL: f64 = 180.0;

/// How far before the music under heard credits stops a skip past them
/// lands, in seconds. A scene after the credits can start under the song's
/// last bars, and a moment more of the credits costs less than its start.
const SCENE_LEAD: f64 = 3.0;

/// How far short of the file's end heard credits have to stop to be taken
/// for stopping there, in seconds: the length of a file read once by ffmpeg
/// and once by mpv can differ by a little. Credits that stop for a scene
/// stop well short (see `titles`).
const LENGTH_SLACK: f64 = 1.0;

/// The longest chapter named for a title sequence that is taken for one,
/// in seconds.
const INTRO_LONGEST: f64 = 300.0;

/// How much of a title sequence has to be left for skipping it to be
/// offered, in seconds. Less, and it is over before the offer is read.
const INTRO_TAIL: f64 = 3.0;

impl PlayerState {
    /// Whether the closing credits are playing: the chapter playing is the
    /// one they start at (see [`credits_chapter`](Self::credits_chapter)) or
    /// more of them after it. Where the chapters don't say, the credits are
    /// where they were heard, if they were.
    ///
    /// An episode's, which can go on to the next; and a film's only where a
    /// scene follows them, which is somewhere to skip to that is the film's
    /// own.
    pub fn credits_rolling(&self) -> bool {
        match self.credits_playing() {
            Some(scene) => scene.is_some() || self.is_episode(),
            None => false,
        }
    }

    /// Where the scene after the credits playing starts, where one does:
    /// what skipping them lands on, whether they stop for it and go on
    /// after, or end with it.
    pub fn after_credits(&self) -> Option<f64> {
        self.credits_playing().flatten()
    }

    /// Whether credits are playing, and if so where they next stop for a
    /// scene of the file's own: `Some(None)` where they run to the end.
    ///
    /// By chapters, the credits are their first chapter and any after it
    /// named for more of them or for a look at the next episode, which is
    /// more of the way on; any other is a scene. Heard, they are the pieces
    /// of music heard (see `titles`), and the skip lands `SCENE_LEAD` before
    /// a piece stops, since a scene can start under the song's last bars.
    fn credits_playing(&self) -> Option<Option<f64>> {
        if let Some(first) = self.credits_chapter() {
            let ending = |at: usize| {
                at == first
                    || self.chapters[at].title.as_deref().is_some_and(|title| {
                        dbm_naming::is_credits(title) || dbm_naming::is_preview(title)
                    })
            };
            let current = self.chapter?;
            if current < first || current >= self.chapters.len() || !ending(current) {
                return None;
            }
            let scene = (current + 1..self.chapters.len()).find(|&at| !ending(at));
            return Some(scene.map(|at| self.chapters[at].time));
        }
        let pieces = self.credits_heard_pieces()?;
        let last = pieces.len() - 1;
        pieces.iter().enumerate().find_map(|(at, &(start, end))| {
            let followed = at < last || end + LENGTH_SLACK < self.duration;
            let scene = followed.then_some(end - SCENE_LEAD);
            (self.time_pos >= start && scene.is_none_or(|scene| self.time_pos < scene))
                .then_some(scene)
        })
    }

    fn is_episode(&self) -> bool {
        self.path.as_ref().is_some_and(|path| {
            matches!(
                dbm_naming::parse(path.as_str()),
                dbm_naming::Media::Episode { .. }
            )
        })
    }

    /// Whether the chapters say where the credits are, by either of the
    /// ways [`credits_chapter`](Self::credits_chapter) reads them. Where
    /// they do there is nothing to listen for.
    pub fn credits_marked(&self) -> bool {
        self.credits_chapter().is_some()
    }

    /// Where this file's credits were heard to be, in pieces. In its second
    /// half, like everything else that is taken for an ending.
    fn credits_heard_pieces(&self) -> Option<&[(f64, f64)]> {
        let path = self.path.as_ref()?;
        let (of, pieces) = self.credits_heard.as_ref()?;
        let starts = pieces.first()?.0;
        (of == path && self.duration > 0.0 && starts >= self.duration / 2.0)
            .then_some(pieces.as_slice())
    }

    /// The chapter the closing credits start at, where the chapters say.
    ///
    /// By name first: the first chapter in the file's second half called
    /// something like `End Credits`. The half keeps a cold open's `Credits`
    /// from counting.
    ///
    /// Plenty of releases name nothing, and a broadcast episode is split at
    /// its act breaks, the last of which is the credits. So where no chapter
    /// has a real name, the last is the credits if it is short: a final act
    /// runs minutes, a closing sequence well under `CREDITS_TAIL`.
    fn credits_chapter(&self) -> Option<usize> {
        if self.duration <= 0.0 {
            return None;
        }
        let named = self.chapters.iter().position(|c| {
            c.time >= self.duration / 2.0 && c.title.as_deref().is_some_and(dbm_naming::is_credits)
        });
        if named.is_some() {
            return named;
        }
        let unnamed = self.chapters.iter().all(|c| {
            c.title
                .as_deref()
                .is_none_or(dbm_naming::is_unnamed_chapter)
        });
        let last = self.chapters.len().checked_sub(1)?;
        let tail = self.duration - self.chapters[last].time;
        (unnamed && last > 0 && tail > 0.0 && tail <= CREDITS_TAIL).then_some(last)
    }

    /// Where the playing episode's title sequence starts and ends, in
    /// seconds: as its chapters name it, and otherwise as it was heard.
    ///
    /// An episode's only, since a film's opening titles are part of the film,
    /// as its credits are (see [`credits_rolling`](Self::credits_rolling)).
    pub fn intro(&self) -> Option<(f64, f64)> {
        let path = self.path.as_ref()?;
        let span = self.intro_named().or_else(|| {
            self.intro_heard
                .as_ref()
                .filter(|(of, _)| of == path)
                .map(|(_, span)| *span)
        })?;
        // Last, being the one that parses a name.
        matches!(
            dbm_naming::parse(path.as_str()),
            dbm_naming::Media::Episode { .. }
        )
        .then_some(span)
    }

    /// The title sequence where a chapter is named for it: from that
    /// chapter's mark to the next one.
    ///
    /// In the file's first half, which keeps an oddly named `Ending Theme`
    /// out, and no longer than `INTRO_LONGEST`, since an `Intro` that runs
    /// ten minutes is an introduction, not a title sequence. The last chapter
    /// has no next mark to end at, so isn't one either.
    pub fn intro_named(&self) -> Option<(f64, f64)> {
        if self.duration <= 0.0 {
            return None;
        }
        let at = self.chapters.iter().position(|c| {
            c.time < self.duration / 2.0 && c.title.as_deref().is_some_and(dbm_naming::is_intro)
        })?;
        let (start, end) = (self.chapters[at].time, self.chapters.get(at + 1)?.time);
        (end > start && end - start <= INTRO_LONGEST).then_some((start, end))
    }

    /// Whether the title sequence is playing, with enough of it left to be
    /// worth skipping.
    pub fn intro_rolling(&self) -> bool {
        self.intro()
            .is_some_and(|(start, end)| self.time_pos >= start && self.time_pos < end - INTRO_TAIL)
    }
}

#[cfg(test)]
mod tests {
    use dbm_mpv::tracks::Chapter;

    use super::*;

    const EPISODE: &str = "Show.Name.S01E02.1080p.mkv";
    const MARKS: &[(f64, &str)] = &[
        (0.0, "Episode"),
        (3400.0, "End Credits"),
        (3550.0, "Preview"),
    ];

    /// The Walking Dead as released: seven act breaks, each titled with its
    /// start time, and the last half-minute the credits.
    const ACTS: &[(f64, &str)] = &[
        (0.0, "00:00:00.000"),
        (141.3, "00:02:21.141"),
        (871.1, "00:14:26.407"),
        (1296.1, "00:21:32.583"),
        (1606.5, "00:26:44.186"),
        (2059.8, "00:34:16.346"),
        (3566.9, "00:59:26.900"),
    ];

    const OPENED: &[(f64, &str)] = &[
        (0.0, "Cold Open"),
        (95.0, "Opening"),
        (185.0, "Part A"),
        (3400.0, "Ending"),
    ];

    /// Credits, a scene after them, and a look at the next episode.
    const SCENE: &[(f64, &str)] = &[
        (0.0, "Episode"),
        (3400.0, "End Credits"),
        (3500.0, "Post-Credits Scene"),
        (3550.0, "Preview"),
    ];

    fn playing(path: &str, chapter: i64, marks: &[(f64, &str)]) -> PlayerState {
        // -1 for none, as mpv says it.
        PlayerState {
            path: Some(path.into()),
            duration: 3600.0,
            chapter: usize::try_from(chapter).ok(),
            chapters: marks
                .iter()
                .map(|(time, title)| Chapter {
                    time: *time,
                    title: Some(title.to_string()),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_new_file_reads_nothing_from_the_last_ones_chapters() {
        // Arrived inside the last file's titles and its credits, before the
        // new file's chapters are read.
        let mut state = playing("Show.Name.S01E03.1080p.mkv", 1, OPENED);
        state.apply(&dbm_mpv::Event::StartFile);
        state.time_pos = 120.0;
        assert_eq!(state.intro(), None);
        assert!(!state.intro_rolling());
        state.chapter = Some(3);
        state.time_pos = 3450.0;
        assert!(!state.credits_rolling());
    }

    #[test]
    fn the_credits_roll_from_their_chapter_to_the_end() {
        assert!(!playing(EPISODE, 0, MARKS).credits_rolling());
        assert!(playing(EPISODE, 1, MARKS).credits_rolling());
        // A preview of the next episode after the credits is still past them.
        assert!(playing(EPISODE, 2, MARKS).credits_rolling());
    }

    #[test]
    fn a_film_has_no_next_episode_to_offer() {
        assert!(!playing("Some Film (2019).mkv", 1, MARKS).credits_rolling());
    }

    #[test]
    fn credits_at_the_start_are_an_opening() {
        let marks = &[(0.0, "Credits"), (90.0, "Episode")];
        assert!(!playing(EPISODE, 0, marks).credits_rolling());
        assert!(!playing(EPISODE, 1, marks).credits_rolling());
    }

    #[test]
    fn unnamed_chapters_end_in_short_credits() {
        assert!(!playing(EPISODE, 5, ACTS).credits_rolling());
        assert!(playing(EPISODE, 6, ACTS).credits_rolling());
    }

    #[test]
    fn a_long_last_act_is_not_credits() {
        let acts = &[
            (0.0, "Chapter 01"),
            (1800.0, "Chapter 02"),
            (3000.0, "Chapter 03"),
        ];
        assert!(!playing(EPISODE, 2, acts).credits_rolling());
    }

    #[test]
    fn named_chapters_are_taken_at_their_word() {
        // Short, last, and called something else: not a guess worth making.
        let marks = &[(0.0, "Episode"), (3500.0, "Preview")];
        assert!(!playing(EPISODE, 1, marks).credits_rolling());
    }

    fn at(mut state: PlayerState, seconds: f64) -> PlayerState {
        state.time_pos = seconds;
        state
    }

    #[test]
    fn a_chapter_named_for_the_titles_is_the_title_sequence() {
        let state = playing(EPISODE, 1, OPENED);
        assert_eq!(state.intro(), Some((95.0, 185.0)));
        assert!(!at(playing(EPISODE, 0, OPENED), 94.0).intro_rolling());
        assert!(at(playing(EPISODE, 1, OPENED), 95.0).intro_rolling());
        assert!(at(playing(EPISODE, 1, OPENED), 181.0).intro_rolling());
        // Three seconds from its end there is nothing left to skip.
        assert!(!at(playing(EPISODE, 1, OPENED), 183.0).intro_rolling());
        assert!(!at(playing(EPISODE, 2, OPENED), 185.0).intro_rolling());
    }

    #[test]
    fn a_film_keeps_its_opening_titles() {
        assert_eq!(playing("Some Film (2019).mkv", 1, OPENED).intro(), None);
    }

    #[test]
    fn an_introduction_is_not_a_title_sequence() {
        let marks = &[(0.0, "Intro"), (600.0, "Act One")];
        assert_eq!(playing(EPISODE, 0, marks).intro(), None);
        // Nor is the last chapter, which has nowhere to end.
        let marks = &[(0.0, "Episode"), (60.0, "Opening")];
        assert_eq!(playing(EPISODE, 1, marks).intro(), None);
    }

    #[test]
    fn a_sequence_that_was_heard_stands_in_where_no_chapter_names_one() {
        let mut state = at(playing(EPISODE, 1, ACTS), 280.0);
        assert!(!state.intro_rolling());
        state.intro_heard = Some((EPISODE.into(), (274.1, 308.9)));
        assert!(state.intro_rolling());
        // Heard in another file, it says nothing about this one.
        state.intro_heard = Some(("Show.Name.S01E03.1080p.mkv".into(), (274.1, 308.9)));
        assert!(!state.intro_rolling());
    }

    #[test]
    fn chapters_are_believed_over_what_was_heard() {
        let mut state = playing(EPISODE, 1, OPENED);
        state.intro_heard = Some((EPISODE.into(), (10.0, 40.0)));
        assert_eq!(state.intro(), Some((95.0, 185.0)));
    }

    #[test]
    fn credits_that_were_heard_roll_where_no_chapter_marks_them() {
        let mut state = at(playing(EPISODE, -1, &[]), 3500.0);
        assert!(!state.credits_rolling());
        state.credits_heard = Some((EPISODE.into(), vec![(3480.0, 3600.0)]));
        assert!(state.credits_rolling());
        assert!(!at(state.clone(), 3470.0).credits_rolling());
        // Heard in another file, or in a film, they say nothing here.
        state.credits_heard = Some(("Show.Name.S01E03.1080p.mkv".into(), vec![(3480.0, 3600.0)]));
        assert!(!state.credits_rolling());
        let mut film = at(playing("Some Film (2019).mkv", -1, &[]), 3500.0);
        film.credits_heard = Some(("Some Film (2019).mkv".into(), vec![(3480.0, 3600.0)]));
        assert!(!film.credits_rolling());
    }

    #[test]
    fn a_scene_heard_after_the_credits_is_skipped_to() {
        let mut state = at(playing(EPISODE, -1, &[]), 3500.0);
        state.credits_heard = Some((EPISODE.into(), vec![(3480.0, 3560.0)]));
        // A moment before the music stops, which the scene may start under.
        assert_eq!(state.after_credits(), Some(3557.0));
        assert!(state.credits_rolling());
        // From there the scene is the episode's, and nothing is offered.
        assert!(!at(state.clone(), 3557.0).credits_rolling());
        // Credits that run to the end have nothing after them, though the
        // length they were heard against is a little off mpv's.
        state.credits_heard = Some((EPISODE.into(), vec![(3480.0, 3599.6)]));
        assert_eq!(state.after_credits(), None);
        assert!(at(state, 3590.0).credits_rolling());
    }

    #[test]
    fn credits_heard_to_stop_for_a_scene_skip_to_it_and_go_on_after() {
        let mut state = at(playing(EPISODE, -1, &[]), 3320.0);
        state.credits_heard = Some((EPISODE.into(), vec![(3300.0, 3350.0), (3440.0, 3600.0)]));
        assert_eq!(state.after_credits(), Some(3347.0));
        assert!(state.credits_rolling());
        // The scene is the episode's.
        assert!(!at(state.clone(), 3400.0).credits_rolling());
        // The rest of the credits run to the end, and go on from there.
        let resumed = at(state, 3450.0);
        assert!(resumed.credits_rolling());
        assert_eq!(resumed.after_credits(), None);
    }

    #[test]
    fn a_chapter_after_the_credits_is_a_scene_to_skip_to() {
        let state = playing(EPISODE, 1, SCENE);
        assert!(state.credits_rolling());
        assert_eq!(state.after_credits(), Some(3500.0));
        assert!(!playing(EPISODE, 2, SCENE).credits_rolling());
        // The preview after the scene is the way on again.
        let preview = playing(EPISODE, 3, SCENE);
        assert!(preview.credits_rolling());
        assert_eq!(preview.after_credits(), None);
        // A preview straight after the credits is more of the way on.
        assert_eq!(playing(EPISODE, 1, MARKS).after_credits(), None);
    }

    #[test]
    fn a_film_has_its_credits_skipped_only_to_a_scene_after_them() {
        assert!(playing("Some Film (2019).mkv", 1, SCENE).credits_rolling());
        assert!(!playing("Some Film (2019).mkv", 1, MARKS).credits_rolling());
    }

    #[test]
    fn chapters_are_believed_over_credits_that_were_heard() {
        let mut state = at(playing(EPISODE, 0, MARKS), 3300.0);
        state.credits_heard = Some((EPISODE.into(), vec![(3200.0, 3600.0)]));
        assert!(state.credits_marked());
        assert!(!state.credits_rolling());
    }

    #[test]
    fn nothing_rolls_before_the_length_is_known() {
        let mut state = playing(EPISODE, 1, MARKS);
        state.duration = 0.0;
        assert!(!state.credits_rolling());
    }
}
