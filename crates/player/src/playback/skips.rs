//! Skipping an episode's title sequence and its closing credits without
//! being asked, for someone who turned it on.
//!
//! Where either is comes from a guess as often as from a chapter (see
//! `ends`), so neither goes unseen. The title sequence goes at once and the
//! corner pill then offers the way back (see `retrace`). The credits go on
//! only after their pill has counted down, in seconds of the film, so a
//! pause holds the count and a seek stops it.
//!
//! This decides; the frame loop carries it out.

use std::time::Instant;

use super::state::PlayerState;

/// How long the credits pill counts down before going on, in seconds of
/// the film.
const COUNTDOWN: f64 = 5.0;

/// A seek moves the position more than this from one frame to the next, and
/// more than this away from where playing would have taken it in the time
/// between. A window that stalls while the film plays on moves it as far as
/// the clock, which is no seek.
const JUMP: f64 = 2.0;

/// Which skips the person asked for.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wanted {
    pub intros: bool,
    /// Going on from the credits, which goes no further than autoplay would,
    /// so is only wanted while autoplay is on.
    pub credits: bool,
}

/// What to do this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Skip {
    /// Past the title sequence, from where the film is to where it ends.
    Intro { from: f64, to: f64 },
    /// Past the credits to the scene after them.
    Scene(f64),
    /// Past the credits to the next file.
    Next,
}

#[derive(Default)]
pub struct Skips {
    /// The load, the position and the time on the last frame, so a seek can
    /// be told from playing.
    last: Option<LastFrame>,
    /// The load whose title sequence is done with: skipped, or landed in by a
    /// seek, which is wanting to see it.
    intro_done: Option<u64>,
    count: Option<Count>,
}

#[derive(Clone, Copy)]
struct LastFrame {
    loads: u64,
    time_pos: f64,
    at: Instant,
}

struct Count {
    loads: u64,
    /// Where the film was when the count began.
    from: f64,
    state: Counting,
}

#[derive(Clone, Copy, PartialEq)]
enum Counting {
    Running,
    /// A seek stopped it, and the credits roll.
    Stopped,
    /// It ran out and went on, which takes a few frames to land.
    Went,
}

impl Skips {
    /// What to skip this frame, the one drawn at `now`, if anything.
    /// `corner_free` is whether the corner pill is the skips' to use: no file
    /// held or on its way in to be held, and no other offer in it.
    pub fn follow(
        &mut self,
        player: &PlayerState,
        wanted: Wanted,
        corner_free: bool,
        now: Instant,
    ) -> Option<Skip> {
        let playing = player.path.is_some() && !player.paused && !player.eof_reached;
        let seeked = self.last.is_some_and(|last| {
            let moved = player.time_pos - last.time_pos;
            let played = if playing {
                now.saturating_duration_since(last.at).as_secs_f64() * player.speed.get()
            } else {
                0.0
            };
            last.loads == player.loads && moved.abs() > JUMP && (moved - played).abs() > JUMP
        });
        self.last = Some(LastFrame {
            loads: player.loads,
            time_pos: player.time_pos,
            at: now,
        });
        self.intro(player, wanted.intros && playing && corner_free, seeked)
            .or_else(|| self.credits(player, wanted.credits && corner_free, seeked))
    }

    fn intro(&mut self, player: &PlayerState, may: bool, seeked: bool) -> Option<Skip> {
        if self.intro_done == Some(player.loads) || !player.intro_rolling() {
            return None;
        }
        let span = player.intro()?;
        // Landing at its start is how a file starts over, which is no sign of
        // wanting to see the sequence.
        if seeked && player.time_pos > span.start + JUMP {
            self.intro_done = Some(player.loads);
            return None;
        }
        if !may {
            return None;
        }
        self.intro_done = Some(player.loads);
        Some(Skip::Intro {
            from: player.time_pos,
            to: span.end,
        })
    }

    /// Counts down while the credits pill offers the way on, and goes there
    /// when the count runs out. Only to where autoplay would go: the next
    /// file, never the next season; or a scene after the credits.
    fn credits(&mut self, player: &PlayerState, may: bool, seeked: bool) -> Option<Skip> {
        let onward = player
            .after_credits()
            .map(Skip::Scene)
            .or_else(|| (!player.at_last()).then_some(Skip::Next))
            .filter(|_| may && !player.eof_reached && player.credits_rolling());
        let Some(onward) = onward else {
            self.count = None;
            return None;
        };
        if self
            .count
            .as_ref()
            .is_some_and(|count| count.loads != player.loads)
        {
            self.count = None;
        }
        let count = self.count.get_or_insert(Count {
            loads: player.loads,
            from: player.time_pos,
            state: Counting::Running,
        });
        if count.state != Counting::Running {
            return None;
        }
        if seeked {
            count.state = Counting::Stopped;
            return None;
        }
        if player.paused || player.time_pos - count.from < COUNTDOWN {
            return None;
        }
        count.state = Counting::Went;
        Some(onward)
    }

    /// How far the credits pill has counted, 0 to 1, for its fill: 0 with
    /// no count running, and 1 once it has gone on, until the pill goes.
    pub fn counted(&self, player: &PlayerState) -> f32 {
        let Some(count) = self.count.as_ref().filter(|c| c.loads == player.loads) else {
            return 0.0;
        };
        match count.state {
            Counting::Running => {
                ((player.time_pos - count.from) / COUNTDOWN).clamp(0.0, 1.0) as f32
            }
            Counting::Stopped => 0.0,
            Counting::Went => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dbm_mpv::tracks::Chapter;

    use super::*;

    const EPISODE: &str = "Show.Name.S01E02.1080p.mkv";
    const ALL: Wanted = Wanted {
        intros: true,
        credits: true,
    };

    /// An episode with a cold open, its titles from 95 s to 185 s, and
    /// credits over its last three minutes; second of three in its list.
    fn episode() -> PlayerState {
        PlayerState {
            path: Some(EPISODE.into()),
            duration: Some(3600.0),
            loads: 1,
            playlist_pos: Some(1),
            playlist_count: 3,
            chapters: [
                (0.0, "Cold Open"),
                (95.0, "Opening"),
                (185.0, "Part A"),
                (3420.0, "End Credits"),
            ]
            .iter()
            .map(|&(time, title)| Chapter {
                time,
                title: Some(title.to_string()),
            })
            .collect(),
            ..Default::default()
        }
    }

    /// Plays `player` on from where it is to `to`, a frame at a time, and
    /// says what was skipped on the way.
    fn play(skips: &mut Skips, player: &mut PlayerState, to: f64, wanted: Wanted) -> Vec<Skip> {
        play_beside(skips, player, to, wanted, true)
    }

    /// The same, with the corner pill free or not.
    fn play_beside(
        skips: &mut Skips,
        player: &mut PlayerState,
        to: f64,
        wanted: Wanted,
        corner_free: bool,
    ) -> Vec<Skip> {
        let mut done = Vec::new();
        while player.time_pos < to {
            let next = (player.time_pos + 0.25).min(to);
            at(player, next);
            done.extend(skips.follow(player, wanted, corner_free, Instant::now()));
        }
        done
    }

    fn at(player: &mut PlayerState, seconds: f64) {
        player.time_pos = seconds;
        player.chapter = player.chapters.iter().rposition(|c| c.time <= seconds);
    }

    #[test]
    fn the_titles_are_skipped_as_they_start_and_offered_back_from_there() {
        let mut skips = Skips::default();
        let mut player = episode();
        let skipped = play(&mut skips, &mut player, 100.0, ALL);
        assert_eq!(
            skipped,
            [Skip::Intro {
                from: 95.0,
                to: 185.0
            }]
        );
    }

    #[test]
    fn nothing_is_skipped_that_was_not_asked_for() {
        let mut skips = Skips::default();
        let mut player = episode();
        assert!(play(&mut skips, &mut player, 3600.0, Wanted::default()).is_empty());
    }

    #[test]
    fn going_back_into_the_titles_is_wanting_to_see_them() {
        let mut skips = Skips::default();
        let mut player = episode();
        play(&mut skips, &mut player, 100.0, ALL);
        // The way back lands where the skip left.
        at(&mut player, 95.0);
        assert_eq!(skips.follow(&player, ALL, true, Instant::now()), None);
        assert!(play(&mut skips, &mut player, 190.0, ALL).is_empty());
    }

    #[test]
    fn a_window_that_stalls_into_the_titles_still_skips_them() {
        let mut skips = Skips::default();
        let mut player = episode();
        let then = Instant::now();
        at(&mut player, 94.0);
        skips.follow(&player, ALL, true, then);
        // Four seconds without a frame, while the film played on.
        at(&mut player, 98.0);
        assert_eq!(
            skips.follow(&player, ALL, true, then + Duration::from_secs(4)),
            Some(Skip::Intro {
                from: 98.0,
                to: 185.0
            })
        );
    }

    #[test]
    fn a_seek_into_the_middle_of_the_titles_is_left_to_play() {
        let mut skips = Skips::default();
        let mut player = episode();
        at(&mut player, 1800.0);
        skips.follow(&player, ALL, true, Instant::now());
        at(&mut player, 140.0);
        assert!(play(&mut skips, &mut player, 190.0, ALL).is_empty());
    }

    #[test]
    fn starting_over_at_the_titles_still_skips_them() {
        let mut skips = Skips::default();
        let mut player = episode();
        player.chapters.remove(0);
        player.chapters[0].time = 0.0;
        // Opened at its saved place, then started over from the top.
        at(&mut player, 1800.0);
        skips.follow(&player, ALL, true, Instant::now());
        at(&mut player, 0.0);
        assert!(matches!(
            skips.follow(&player, ALL, true, Instant::now()),
            Some(Skip::Intro { .. })
        ));
    }

    #[test]
    fn the_titles_wait_while_the_corner_offers_something_else() {
        let mut skips = Skips::default();
        let mut player = episode();
        assert!(play_beside(&mut skips, &mut player, 106.0, ALL, false).is_empty());
        player.time_pos = 106.25;
        assert_eq!(
            skips.follow(&player, ALL, true, Instant::now()),
            Some(Skip::Intro {
                from: 106.25,
                to: 185.0
            })
        );
    }

    #[test]
    fn a_paused_film_skips_nothing() {
        let mut skips = Skips::default();
        let mut player = episode();
        at(&mut player, 96.0);
        player.paused = true;
        assert_eq!(skips.follow(&player, ALL, true, Instant::now()), None);
    }

    #[test]
    fn the_credits_count_down_and_then_go_on() {
        let mut skips = Skips::default();
        let mut player = episode();
        at(&mut player, 3400.0);
        assert!(play(&mut skips, &mut player, 3424.75, ALL).is_empty());
        assert_eq!(play(&mut skips, &mut player, 3425.0, ALL), [Skip::Next]);
        // Not again while the next file is on its way.
        assert!(play(&mut skips, &mut player, 3430.0, ALL).is_empty());
        assert_eq!(skips.counted(&player), 1.0);
    }

    #[test]
    fn the_count_fills_as_the_film_plays() {
        let mut skips = Skips::default();
        let mut player = episode();
        at(&mut player, 3420.0);
        skips.follow(&player, ALL, true, Instant::now());
        assert_eq!(skips.counted(&player), 0.0);
        play(&mut skips, &mut player, 3422.5, ALL);
        assert_eq!(skips.counted(&player), 0.5);
    }

    #[test]
    fn a_pause_holds_the_count() {
        let mut skips = Skips::default();
        let mut player = episode();
        at(&mut player, 3420.0);
        skips.follow(&player, ALL, true, Instant::now());
        player.paused = true;
        for _ in 0..1000 {
            assert_eq!(skips.follow(&player, ALL, true, Instant::now()), None);
        }
        player.paused = false;
        assert_eq!(play(&mut skips, &mut player, 3426.0, ALL), [Skip::Next]);
    }

    #[test]
    fn a_seek_lets_the_credits_roll() {
        let mut skips = Skips::default();
        let mut player = episode();
        at(&mut player, 3420.0);
        play(&mut skips, &mut player, 3423.0, ALL);
        // Back to read a name.
        at(&mut player, 3420.5);
        assert_eq!(skips.follow(&player, ALL, true, Instant::now()), None);
        assert_eq!(skips.counted(&player), 0.0);
        assert!(play(&mut skips, &mut player, 3599.0, ALL).is_empty());
    }

    #[test]
    fn the_last_episode_counts_down_to_nothing() {
        let mut skips = Skips::default();
        let mut player = episode();
        player.playlist_pos = Some(2);
        at(&mut player, 3400.0);
        assert!(play(&mut skips, &mut player, 3599.0, ALL).is_empty());
        assert_eq!(skips.counted(&player), 0.0);
    }

    #[test]
    fn a_scene_after_the_credits_is_counted_down_to() {
        let mut skips = Skips::default();
        let mut player = episode();
        player.playlist_pos = Some(2);
        player.chapters.push(Chapter {
            time: 3500.0,
            title: Some("Post-Credits Scene".into()),
        });
        at(&mut player, 3415.0);
        assert_eq!(
            play(&mut skips, &mut player, 3430.0, ALL),
            [Skip::Scene(3500.0)]
        );
    }

    #[test]
    fn the_credits_wait_while_the_corner_offers_something_else() {
        let mut skips = Skips::default();
        let mut player = episode();
        at(&mut player, 3420.0);
        for _ in 0..40 {
            player.time_pos += 0.25;
            assert_eq!(skips.follow(&player, ALL, false, Instant::now()), None);
            assert_eq!(skips.counted(&player), 0.0);
        }
        // The count starts once the corner is the credits pill's.
        assert_eq!(play(&mut skips, &mut player, 3434.0, ALL), []);
        assert_eq!(play(&mut skips, &mut player, 3436.0, ALL), [Skip::Next]);
    }
}
