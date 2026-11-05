//! Exercises of playback: the transport, scrubbing, the fade, the seek
//! preview, the ends of a file, the flash, and resume progress.

use std::time::Duration;

use dbm_mpv::{Command, Mpv, prop};
use slint::{ComponentHandle, Model};

use super::drive::{arrow, chord, hover, wheel};
use super::script::{self, Beat, Script, Verdict};
use super::{playlist_rows, selected_label, short};
use crate::{MainWindow, Playback, Playlist, Settings, Subtitles, Tracks};

fn report(ui: &MainWindow, label: &str) {
    eprintln!(
        "dbm: {label:<22} t={} paused={} vol={} muted={} subs={}/{} audio={}",
        ui.global::<Playback>().get_elapsed(),
        ui.global::<Playback>().get_paused(),
        ui.global::<Playback>().get_volume(),
        ui.global::<Playback>().get_muted(),
        ui.global::<Subtitles>().get_visible(),
        selected_label(&ui.global::<Tracks>().get_subtitles()),
        selected_label(&ui.global::<Tracks>().get_audio()),
    );
}

pub(super) fn input_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let verdict = Verdict::new("input");
        let playback = ui.global::<Playback>();
        let tracks = ui.global::<Tracks>();
        let subtitles = ui.global::<Subtitles>();
        let selected =
            |rows: slint::ModelRc<crate::TrackItem>| rows.iter().find(|t| t.selected).map(|t| t.id);
        script::loaded(&ui).await;
        let mut beat = Beat::new(Duration::from_millis(1500));
        // Alternating act / observe. Each report follows its check, so it
        // shows the state the check waited for rather than one in flight.
        beat.next().await;
        report(&ui, "start");
        beat.next().await;
        playback.invoke_toggle_pause();
        beat.next().await;
        verdict
            .expect(|| playback.get_paused(), "a toggle pauses")
            .await;
        report(&ui, "after toggle-pause");
        beat.next().await;
        playback.invoke_toggle_pause();
        beat.next().await;
        verdict
            .expect(|| !playback.get_paused(), "a second toggle plays on")
            .await;
        report(&ui, "after toggle again");
        beat.next().await;
        playback.invoke_seek_fraction(0.5);
        beat.next().await;
        verdict
            .expect(
                || (playback.get_progress() - 0.5).abs() < 0.1,
                "a seek to half way lands there",
            )
            .await;
        report(&ui, "after seek to 50%");
        beat.next().await;
        let volume = playback.get_volume();
        playback.invoke_nudge_volume(-15.0);
        beat.next().await;
        verdict
            .expect(
                || (playback.get_volume() - (volume - 15.0)).abs() < 0.5,
                "the volume comes down by what was asked",
            )
            .await;
        report(&ui, "after volume -15");
        beat.next().await;
        let progress = playback.get_progress();
        playback.invoke_seek_relative(-3.0);
        beat.next().await;
        verdict
            .expect(
                || playback.get_progress() < progress,
                "a seek back goes back",
            )
            .await;
        report(&ui, "after seek -3s");
        beat.next().await;
        playback.invoke_toggle_mute();
        beat.next().await;
        verdict
            .expect(|| playback.get_muted(), "a toggle mutes")
            .await;
        report(&ui, "after toggle-mute");
        beat.next().await;
        tracks.invoke_select_audio(2);
        beat.next().await;
        verdict
            .expect(
                || selected(tracks.get_audio()) == Some(2),
                "the audio track picked plays",
            )
            .await;
        report(&ui, "after audio -> id 2");
        beat.next().await;
        tracks.invoke_select_subtitle(2);
        beat.next().await;
        report(&ui, "after sub -> id 2");
        beat.next().await;
        tracks.invoke_select_subtitle(-1);
        beat.next().await;
        verdict
            .expect(|| !subtitles.get_visible(), "subtitles turned off are off")
            .await;
        report(&ui, "after subs off");
        beat.next().await;
        subtitles.invoke_toggle();
        beat.next().await;
        verdict
            .expect(
                || subtitles.get_visible() && selected(tracks.get_subtitles()).is_some(),
                "a toggle turns subtitles on, on a track",
            )
            .await;
        report(&ui, "after captions toggle");
        beat.next().await;
        playback.invoke_toggle_panscan();
        beat.next().await;
        verdict
            .expect(|| playback.get_panscan(), "a toggle fills the window")
            .await;
        eprintln!("dbm: panscan now {}", playback.get_panscan());
        beat.next().await;
        playback.invoke_toggle_panscan();
        beat.next().await;
        verdict
            .expect(|| !playback.get_panscan(), "a second toggle fits it again")
            .await;
        eprintln!("dbm: panscan back to {}", playback.get_panscan());
        beat.next().await;
        // The mute is still on. Now the bar is used: the level has to arrive
        // and the mute has to go, since either alone is a volume nobody can
        // hear.
        report(&ui, "still muted, about to use the bar");
        beat.next().await;
        playback.invoke_set_volume(120.0);
        beat.next().await;
        verdict
            .expect(
                || !playback.get_muted() && (playback.get_volume() - 120.0).abs() < 0.5,
                "the bar sets the level and unmutes",
            )
            .await;
        report(&ui, "after the bar was used while muted");
        verdict.finish();
    })
}

/// Drag the timeline the way a hand does: many small position updates,
/// arriving faster than mpv can service them.
pub(super) fn scrub_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let verdict = Verdict::new("scrub");
        let playback = ui.global::<Playback>();
        // Let playback settle before dragging, by the clock rather than in
        // beats: on software GL each beat waits for a frame, a fifth of a second.
        script::loaded(&ui).await;
        script::sleep(Duration::from_secs(2)).await;
        let mut beat = Beat::new(Duration::from_millis(16));
        let started = std::time::Instant::now();
        eprintln!(
            "dbm: --- scrub drag starting, paused={} ---",
            playback.get_paused()
        );
        // Sweep 10% -> 90%, several updates per tick, as a real drag
        // delivers moves faster than a 16ms timer.
        for step in 0..120u32 {
            if step == 60 {
                eprintln!("dbm: mid-drag paused={}", playback.get_paused());
                verdict.check(playback.get_paused(), "a drag holds the film still");
            }
            for sub in 0..4 {
                playback.invoke_seek_scrub(0.1 + 0.8 * ((step * 4 + sub) as f32 / 480.0));
            }
            beat.next().await;
        }
        // Release, so the coalescing report fires.
        playback.invoke_seek_fraction(0.9);
        eprintln!(
            "dbm: --- scrub drag done in {:.0}ms ---",
            started.elapsed().as_secs_f64() * 1000.0
        );
        // Two ticks later, so the resume has round-tripped through mpv.
        beat.rest(2).await;
        eprintln!("dbm: after release paused={}", playback.get_paused());
        // On a busy machine the round trip can take longer than that.
        for _ in 0..60 {
            if !playback.get_paused() {
                break;
            }
            beat.next().await;
        }
        verdict.check(!playback.get_paused(), "the film plays on within a second");
        verdict.finish();
    })
}

/// Drag the timeline slowly, stop, and see what picture is on screen; then
/// drag it fast and stop.
///
/// A slow drag gets exact seeks and a fast one keyframes (see `scrub`), and
/// the difference only shows in the picture. So this compares the position
/// mpv reports with the pointer each time the pointer has stopped, three
/// times along a slow drag and once after a fast one; keyframes alone can
/// leave a stopped pointer up to ten seconds from its frame. Only at rest,
/// since mid-seek mpv reports where it is going.
///
/// Real pointer events, because a stopped pointer is noticed by a Slint timer
/// that only a held button runs. Pair it with `DBM_TRACE=1` for how many of
/// each drag's seeks were exact.
pub(super) fn slow_scrub_test(ui: &MainWindow) -> Script {
    use super::drive::{press, release};

    // Three legs of sixty moves, a rest after each. `leg` is how many moves
    // into the drag a tick is, for the ticks that move.
    let leg = |tick: u32| match tick {
        151..=210 => Some(tick - 150),
        237..=296 => Some(tick - 236 + 60),
        323..=382 => Some(tick - 322 + 120),
        _ => None,
    };
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(16));
        let playback = ui.global::<Playback>();
        for tick in 1..=585u32 {
            beat.next().await;
            let y = ui.get_timeline_y() + 14.0;
            let x = |fraction: f32| ui.get_scrub_x() + ui.get_scrub_w() * fraction;
            let picture =
                || ui.global::<Playback>().get_progress() * ui.global::<Playback>().get_duration();
            let report = |label: &str, fraction: f32| {
                let wanted = fraction * ui.global::<Playback>().get_duration();
                eprintln!(
                    "dbm: scrub {label:<28} pointer at {wanted:8.2} s, picture at {:8.2} s, {:+.2} s out",
                    picture(),
                    picture() - wanted,
                );
            };
            // A sixth of a pixel a tick is ten pixels a second: a hand
            // feeling for a frame, and well under the speed a drag turns
            // fast at for anything longer than a few minutes.
            let slow = |moves: u32| 0.50 + moves as f32 * 0.16 / ui.get_scrub_w();
            match (tick, leg(tick)) {
                (_, Some(moves)) => hover(&ui, x(slow(moves)), y),
                // Let the film start before taking hold of it.
                (150, _) => press(&ui, x(0.50), y),
                // Held still before each of these: nothing is sent from
                // here, and the interface has to notice that by itself.
                (236, _) => report("slow drag, first rest", slow(60)),
                (322, _) => report("slow drag, second rest", slow(120)),
                (408, _) => report("slow drag, third rest", slow(180)),
                (409, _) => release(&ui, x(slow(180)), y),
                (425, _) => report("let go", slow(180)),
                // The second act: a fifth of the film in a sixth of a second.
                (440, _) => press(&ui, x(0.60), y),
                (441..=450, _) => hover(&ui, x(0.60 + 0.02 * (tick - 440) as f32), y),
                (486, _) => report("fast drag, come to rest", 0.80),
                (487, _) => release(&ui, x(0.80), y),
                (505, _) => report("let go", 0.80),
                // The third: a drag that wanders off the track before the
                // button comes up, which is how most of them end.
                (520, _) => {
                    eprintln!(
                        "dbm: scrub before the third drag paused={}",
                        playback.get_paused()
                    );
                    press(&ui, x(0.30), y);
                }
                (521..=530, _) => hover(&ui, x(0.30 + 0.001 * (tick - 520) as f32), y),
                (531, _) => hover(&ui, x(0.31), y - 80.0),
                (532, _) => release(&ui, x(0.31), y - 80.0),
                (560, _) => {
                    report("let go off the track", 0.31);
                    eprintln!("dbm: scrub after it paused={}", playback.get_paused());
                }
                // And no drag at all: a click still has to seek, since the
                // button coming up is what seeks.
                (570, _) => {
                    press(&ui, x(0.70), y);
                    release(&ui, x(0.70), y);
                }
                (585, _) => {
                    report("a click", 0.70);
                    eprintln!("dbm: --- slow scrub test done ---");
                }
                _ => {}
            }
        }
    })
}

/// Watch the bar fade out, in the value the pipeline is actually given.
///
/// Every surface publishes how far it has faded in, so a fade is readable
/// without a capture: a run of values between 1 and 0 is an animation, and a
/// jump to nothing is a cut. Worth checking because the interface only asks
/// for frames while it is on screen, so the fade-out depends on Slint driving
/// its own animation.
pub(super) fn fade_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(16));
        // Every step the bar's opacity takes, until it has been gone a while.
        let mut seen: Vec<f32> = Vec::new();
        let mut settled = 0u32;
        while settled < 12 {
            beat.next().await;
            let Some(bar) = ui.get_glass_rects().row_data(0) else {
                continue;
            };
            let now = if bar.width > 0.0 { bar.opacity } else { 0.0 };
            if seen.last().is_none_or(|last| (last - now).abs() > 0.001) {
                seen.push(now);
            }
            if now == 0.0 {
                settled += 1;
            }
        }
        let steps: Vec<String> = seen.iter().map(|v| format!("{v:.2}")).collect();
        eprintln!(
            "dbm: bar faded through {} value(s): {}",
            seen.len(),
            steps.join(" ")
        );
        eprintln!("dbm: --- fade test done ---");
    })
}

/// Walk the pointer along the timeline and read the preview off the far end.
///
/// A real pointer move, not just the callback, because the preview only
/// appears while the track is hovered. Checks that hovering raises it, and
/// that the timestamp and the tile under the pointer are right.
pub(super) fn preview_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1100));
        beat.next().await;
        report_preview(&ui, "before hovering");
        // Each place reported a beat after the move to it, since a value
        // read in the same tick as its move belongs to the next one.
        for fraction in [0.0f32, 0.25, 0.5, 0.99] {
            let x = ui.get_scrub_x() + ui.get_scrub_w() * fraction;
            let y = ui.get_timeline_y() + 14.0;
            hover(&ui, x, y);
            beat.next().await;
            report_preview(&ui, &format!("at {:.0}%", fraction * 100.0));
        }
        beat.next().await;
        eprintln!("dbm: --- preview test done ---");
    })
}

/// Set the subtitles' timing on one file, go to the next, and come back.
///
/// Timing belongs to the file it was set on: the next should start untimed,
/// and the first should get its own back. Both halves, since either can fail
/// alone. The watch-later file is written by hand before leaving rather than
/// waiting for the ten-second checkpoint.
pub(super) fn timing_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> Script {
    let report = |ui: &MainWindow, mpv: &Mpv, label: &str| {
        eprintln!(
            "dbm: timing {label:<20} {:?} delay={:+.2} speed={:.4}",
            short(&mpv.get(prop::FILENAME).unwrap_or_default().into()),
            ui.global::<Subtitles>().get_delay(),
            ui.global::<Subtitles>().get_speed(),
        );
    };
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(700));
        // Let the film open and its playlist arrive first.
        beat.rest(5).await;
        report(&ui, &mpv, "as it opened");
        beat.next().await;
        ui.global::<Subtitles>().invoke_nudge_delay(-5.0);
        let _ = mpv.set(prop::SUB_SPEED, 1.04);
        beat.next().await;
        report(&ui, &mpv, "set on the first");
        let _ = mpv.run(0, Command::WriteWatchLaterConfig);
        beat.next().await;
        ui.global::<Playlist>().invoke_step(1);
        beat.rest(3).await;
        report(&ui, &mpv, "on the next file");
        beat.next().await;
        ui.global::<Playlist>().invoke_step(-1);
        beat.rest(3).await;
        report(&ui, &mpv, "back on the first");
        beat.next().await;
        eprintln!("dbm: --- timing test done ---");
    })
}

/// Turn the wheel over the picture, then over the volume track.
///
/// The wheel sets the volume only over its own track, so the picture must not
/// answer and the track must, with the level in the hover label at once
/// rather than after a name's wait. The steps are shorter than that wait, so
/// a label up by the next one can't have waited.
pub(super) fn wheel_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(400));
        let playback = ui.global::<Playback>();
        // Where the volume track is, read when it is wanted: the bar lays
        // itself out once the window is up.
        let track = || (ui.get_vol_cx(), ui.get_vol_cy());
        // Let the film start and the bar settle first.
        beat.rest(6).await;
        report_wheel(&ui, "at rest");
        beat.next().await;
        for _ in 0..3 {
            wheel(&ui, 640.0, 300.0, 120.0);
        }
        beat.next().await;
        report_wheel(&ui, "three notches, picture");
        beat.next().await;
        let (x, y) = track();
        eprintln!("dbm: wheel over the volume track at ({x:.0}, {y:.0})");
        wheel(&ui, x, y, 120.0);
        beat.next().await;
        report_wheel(&ui, "one notch up, track");
        beat.next().await;
        let (x, y) = track();
        for _ in 0..3 {
            wheel(&ui, x, y, -120.0);
        }
        beat.next().await;
        report_wheel(&ui, "three notches down");
        beat.next().await;
        playback.invoke_toggle_mute();
        beat.next().await;
        report_wheel(&ui, "muted");
        beat.next().await;
        playback.invoke_toggle_mute();
        beat.next().await;
        hover(&ui, 640.0, 300.0);
        beat.next().await;
        report_wheel(&ui, "pointer gone");
        beat.next().await;
        eprintln!("dbm: --- wheel test done ---");
    })
}

fn report_wheel(ui: &MainWindow, label: &str) {
    let tip = ui.global::<crate::Tip>();
    let glass = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .count();
    eprintln!(
        "dbm: wheel {label:<24} vol={} muted={} tip={:?} {:?} at {:.0} | {glass} glass rect(s)",
        ui.global::<Playback>().get_volume(),
        ui.global::<Playback>().get_muted(),
        tip.get_label(),
        tip.get_keys(),
        tip.get_cx(),
    );
}

fn report_preview(ui: &MainWindow, label: &str) {
    let overlay = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .count();
    eprintln!(
        "dbm: preview {label:<16} time={:?} tile=({},{}) of {}x{} | {overlay} glass rect(s)",
        ui.global::<Playback>().get_preview_time(),
        ui.global::<Playback>().get_preview_clip_x(),
        ui.global::<Playback>().get_preview_clip_y(),
        ui.global::<Playback>().get_preview_tile_w(),
        ui.global::<Playback>().get_preview_tile_h(),
    );
}

/// Sit at the end of a file, where the only thing left on screen is the offer
/// of another one.
///
/// The end is reached for real rather than by setting the flag, since
/// `at-end` comes from mpv's `eof-reached`: autoplay off, so mpv holds the
/// last frame, then a seek to the final moment and a wait. It then stays, so
/// the window can be captured with the bar up and after it has gone.
pub(super) fn end_test(ui: &MainWindow) -> Script {
    // `DBM_END_TEST=enter` goes on past the pill with Enter; anything else
    // holds the pill up to be captured.
    let enter = std::env::var("DBM_END_TEST").is_ok_and(|v| v == "enter");
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1000));
        let settings = ui.global::<Settings>();
        beat.next().await;
        report_end(&ui, "start");
        beat.next().await;
        // Both halves: the callback tells mpv, the property moves the
        // switch, exactly as clicking it does.
        settings.invoke_set_autoplay(false);
        settings.set_autoplay(false);
        beat.next().await;
        ui.global::<Playback>().invoke_seek_fraction(0.999);
        beat.next().await;
        report_end(&ui, "seeking");
        beat.next().await;
        report_end(&ui, "seeking");
        beat.next().await;
        report_end(&ui, "at the end");
        beat.next().await;
        if enter {
            let key = slint::SharedString::from(slint::platform::Key::Return);
            chord(&ui, &[], &key);
        } else {
            eprintln!("dbm: --- end-of-playback test: holding ---");
        }
        beat.next().await;
        if enter {
            report_end(&ui, "after enter");
        }
        beat.next().await;
        if enter {
            report_end(&ui, "after enter");
        }
        beat.next().await;
        if enter {
            eprintln!("dbm: --- end-of-playback test done ---");
        }
    })
}

/// What the player is showing at the end of a file, and whether the pipeline
/// was told to put glass under it.
fn report_end(ui: &MainWindow, label: &str) {
    let rects: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| format!("{}x{}@{},{}", r.width, r.height, r.x, r.y))
        .collect();
    eprintln!(
        "dbm: end {label:<12} at-end={} at-last={} paused={} ring={}@{} t={}/{} {:?} | {} rect(s) {}",
        ui.global::<Playback>().get_at_end(),
        ui.global::<Playback>().get_at_last(),
        ui.global::<Playback>().get_paused(),
        ui.global::<crate::Chrome>().get_focus_visible(),
        ui.get_bar_focus(),
        ui.global::<Playback>().get_elapsed(),
        ui.global::<Playback>().get_total(),
        ui.global::<Playback>()
            .get_path()
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_string(),
        rects.len(),
        rects.join(" "),
    );
}

/// Play into a file's closing credits.
///
/// Few files carry chapters and fewer name their credits, so the chapters
/// are supplied: an ffmetadata file, which mpv takes through `chapters-file`,
/// naming the last two minutes `End Credits`. mpv reads it when a file opens,
/// so the file is opened again, the way the dialog would.
pub(super) fn credits_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> Script {
    let own = std::env::var("DBM_CREDITS_TEST").is_ok_and(|v| v == "own");
    let heard = std::env::var("DBM_CREDITS_TEST").is_ok_and(|v| v == "heard");
    let scene = std::env::var("DBM_CREDITS_TEST").is_ok_and(|v| v == "scene");
    let mid = std::env::var("DBM_CREDITS_TEST").is_ok_and(|v| v == "mid");
    let asleep = std::env::var("DBM_CREDITS_TEST").is_ok_and(|v| v == "asleep");
    let panel = std::env::var("DBM_CREDITS_TEST").is_ok_and(|v| v == "panel");
    let played_at: Option<f64> = std::env::var("DBM_CREDITS_AT")
        .ok()
        .and_then(|at| at.parse().ok());
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1000));
        let playback = ui.global::<Playback>();
        // The pill is what is tested, so nothing presses it for the person.
        turn_skips(&ui, false);
        beat.rest(3).await;
        // Nothing is given to the file and nothing read off it: where
        // its credits are is for the player to have heard by the time
        // it is played into its last twenty seconds.
        let credits_at;
        if heard {
            credits_at = f64::from(playback.get_duration()) - 25.0;
            eprintln!(
                "dbm: credits: {} chapters, playing into the last 20s",
                playback.get_chapter_marks().row_count()
            );
        } else if own {
            let marks = playback.get_chapter_marks();
            credits_at = marks
                .row_count()
                .checked_sub(1)
                .and_then(|last| marks.row_data(last))
                .map_or(0.0, f64::from);
            eprintln!(
                "dbm: credits: the file's own {} chapters, the last from {credits_at:.1}s",
                marks.row_count()
            );
        } else {
            let duration = f64::from(playback.get_duration());
            credits_at = (duration - 120.0).max(duration / 2.0);
            // With `=scene`, the credits stop forty seconds short, for
            // a scene of the episode's own; with `=mid`, they stop
            // for one forty seconds in, and the last forty are more.
            let mut marks = vec![(0.0, "Episode"), (credits_at, "End Credits")];
            if scene {
                marks.push((duration - 40.0, "Post-Credits Scene"));
            }
            if mid {
                marks.push((duration - 80.0, "Mid-Credits Scene"));
                marks.push((duration - 40.0, "Credits"));
            }
            if let Err(e) = give_chapters(&mpv, &marks, duration) {
                eprintln!("dbm: credits: {e}");
                return;
            }
            eprintln!("dbm: credits from {credits_at:.0}s of {duration:.0}s");
            ui.global::<Playlist>()
                .invoke_open_path(playback.get_path(), true);
        }
        beat.rest(3).await;
        // As a click on its button opens it, with no ring.
        if heard {
            report_credits(&ui, "before");
        } else if panel {
            ui.invoke_open_menu(true);
        }
        beat.next().await;
        // The seek wakes the bar, as a click on the timeline does, so
        // `=asleep` lands short of the credits for the bar to have
        // gone by the time they start, and `=panel` for them to start
        // behind the menu.
        let duration = f64::from(playback.get_duration()).max(1.0);
        let to = played_at.unwrap_or(if asleep || panel {
            credits_at - 4.5
        } else {
            credits_at + 5.0
        });
        playback.invoke_seek_fraction((to / duration) as f32);
        beat.next().await;
        report_credits(&ui, "arrived");
        beat.rest(5).await;
        // Past `HIDE_AFTER`, with nothing touched since.
        report_credits(&ui, "left alone");
        beat.next().await;
        // The pill came up behind the menu, so closing it is what
        // brings the ring to the pill.
        // The ring came to the pill on its own, so Enter alone goes on.
        if panel {
            let escape = slint::SharedString::from(slint::platform::Key::Escape);
            chord(&ui, &[], &escape);
        } else {
            let enter = slint::SharedString::from(slint::platform::Key::Return);
            chord(&ui, &[], &enter);
        }
        beat.next().await;
        if panel {
            report_credits(&ui, "closed");
        }
        beat.next().await;
        // The next file, and the ring gone with the pill rather than
        // waiting on a stop that no longer exists.
        if panel {
            let enter = slint::SharedString::from(slint::platform::Key::Return);
            chord(&ui, &[], &enter);
        } else {
            report_credits(&ui, "after enter");
        }
        beat.next().await;
        // Past the scene, into the rest of the credits, which go on.
        if mid {
            let duration = f64::from(playback.get_duration()).max(1.0);
            playback.invoke_seek_fraction(((duration - 35.0) / duration) as f32);
        } else if !mid && !panel {
            eprintln!("dbm: --- credits test: holding ---");
        }
        beat.next().await;
        if panel {
            report_credits(&ui, "after enter");
        } else if mid {
            report_credits(&ui, "resumed");
        }
        beat.next().await;
        if panel || mid {
            eprintln!("dbm: --- credits test: holding ---");
        }
    })
}

/// Play into an episode's title sequence and skip it.
///
/// Waits for the sequence to be known first, however that comes about —
/// named by a chapter, remembered, or heard — because the listening is the
/// part with a real file and a real ffmpeg in it. Ten seconds short of the
/// end is inside any sequence there is: none is shorter than fifteen.
pub(super) fn intro_test(ui: &MainWindow) -> Script {
    let hold = std::env::var("DBM_INTRO_TEST").is_ok_and(|v| v == "hold");
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(700));
        let playback = ui.global::<Playback>();
        turn_skips(&ui, false);
        // Let the film open first, then wait for its title sequence to be
        // found.
        beat.rest(4).await;
        let mut waited = 0usize;
        while playback.get_intro_end() <= 0.0 {
            waited += 1;
            if waited == 40 {
                eprintln!("dbm: intro: no title sequence found in 28 s");
                return;
            }
            beat.next().await;
        }
        let end = playback.get_intro_end();
        eprintln!(
            "dbm: intro ends at {end:.1}s, known after {:.1}s of waiting",
            waited as f32 * 0.7
        );
        report_intro(&ui, "before");
        let duration = playback.get_duration().max(1.0);
        playback.invoke_seek_fraction((end - 10.0) / duration);
        if hold && !playback.get_paused() {
            playback.invoke_toggle_pause();
        }
        beat.rest(2).await;
        report_intro(&ui, "arrived");
        beat.next().await;
        if hold {
            eprintln!("dbm: --- intro test: holding ---");
            return;
        }
        // Past `HIDE_AFTER`, with nothing touched since.
        beat.rest(4).await;
        report_intro(&ui, "left alone");
        // The ring came to the pill on its own, so Enter alone skips.
        beat.next().await;
        let enter = slint::SharedString::from(slint::platform::Key::Return);
        chord(&ui, &[], &enter);
        beat.rest(2).await;
        report_intro(&ui, "after enter");
        beat.next().await;
        eprintln!("dbm: --- intro test done ---");
    })
}

/// Give the files opened from now on `marks` for chapters, each running to
/// the next and the last to `duration`: an ffmetadata file, which mpv reads
/// through `chapters-file` as a file opens.
fn give_chapters(mpv: &Mpv, marks: &[(f64, &str)], duration: f64) -> Result<(), String> {
    let mut text = String::from(";FFMETADATA1\n");
    for (at, &(start, title)) in marks.iter().enumerate() {
        let end = marks.get(at + 1).map_or(duration, |next| next.0);
        text += &format!(
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART={}\nEND={}\ntitle={title}\n",
            (start * 1000.0) as u64,
            (end * 1000.0) as u64,
        );
    }
    let file = std::env::temp_dir().join("dbm-chapters-test.ffmeta");
    std::fs::write(&file, text).map_err(|e| format!("cannot write chapters: {e}"))?;
    mpv.set(prop::CHAPTERS_FILE, file)
        .map_err(|e| format!("mpv refused chapters-file: {e}"))
}

/// Turn both skips on or off, as their switches do. The test settings
/// outlive a run, so an exercise that cares sets them itself.
fn turn_skips(ui: &MainWindow, on: bool) {
    let settings = ui.global::<Settings>();
    settings.set_skip_intros(on);
    settings.invoke_set_skip_intros(on);
    settings.set_skip_credits(on);
    settings.invoke_set_skip_credits(on);
}

/// Skip a title sequence and go on from the credits without a hand on
/// anything, as someone who turned both on would see it.
///
/// The chapters are supplied, as for `credits_test`: titles from 0:05 to
/// 0:20 and credits over the last fifteen seconds, which fits the minute-long
/// episodes `smoke-media.sh` makes. Needs an episode with a later one beside
/// it.
pub(super) fn skip_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> Script {
    script::run(ui, move |ui| async move {
        let verdict = Verdict::new("skip");
        let playback = ui.global::<Playback>();
        script::loaded(&ui).await;
        let duration = f64::from(playback.get_duration());
        let credits_at = duration - 15.0;
        let marks = [
            (0.0, "Cold Open"),
            (5.0, "Opening"),
            (20.0, "Part A"),
            (credits_at, "End Credits"),
        ];
        if let Err(e) = give_chapters(&mpv, &marks, duration) {
            verdict.check(false, &e);
            verdict.finish();
            return;
        }
        let film = playback.get_path();
        ui.global::<Playlist>().invoke_open_path(film.clone(), true);
        verdict
            .expect(
                || playback.get_chapter_marks().row_count() == marks.len(),
                "the file opens again with its titles and credits marked",
            )
            .await;
        turn_skips(&ui, true);
        let settings = ui.global::<Settings>();
        settings.set_autoplay(true);
        settings.invoke_set_autoplay(true);
        let at = || f64::from(playback.get_progress()) * duration;
        let seek = |to: f64| playback.invoke_seek_fraction((to / duration) as f32);

        seek(2.0);
        verdict
            .expect_after(2.0, || at() > 4.0, "the film plays up to its titles")
            .await;
        // Further than five seconds of playing could take it.
        verdict
            .expect(|| at() >= 19.5, "the titles are skipped as they start")
            .await;
        verdict
            .expect(
                || playback.get_back_offer() == "0:05",
                "the corner offers the way back to them",
            )
            .await;
        playback.invoke_take_back_offer();
        verdict
            .expect(|| (5.0..8.0).contains(&at()), "the way back goes to them")
            .await;
        Beat::new(Duration::from_millis(3000)).next().await;
        verdict.check(
            at() < 19.0 && playback.get_intro_rolling(),
            "and they play, offered to skip by hand",
        );

        seek(credits_at - 2.0);
        verdict
            .expect_after(
                2.0,
                || playback.get_credits_count() > 0.0,
                "the credits pill counts down",
            )
            .await;
        verdict
            .expect_after(
                2.5,
                || at() >= credits_at + 2.5,
                "the count runs with the film",
            )
            .await;
        // Back to read a name, and still in the credits.
        seek(credits_at);
        verdict
            .expect(|| playback.get_credits_count() == 0.0, "a seek stops it")
            .await;
        Beat::new(Duration::from_millis(6000)).next().await;
        verdict.check(
            playback.get_path() == film && playback.get_credits_rolling(),
            "and the credits roll",
        );

        // Back out of them and into them again, for a fresh count.
        seek(credits_at - 2.0);
        verdict
            .expect_after(
                4.5,
                || playback.get_credits_count() >= 0.5,
                "the next count gets half way",
            )
            .await;
        verdict
            .expect_after(
                2.5,
                || playback.get_path() != film,
                "and goes on to the next episode before the end",
            )
            .await;
        turn_skips(&ui, false);
        verdict.finish();
    })
}

fn report_intro(ui: &MainWindow, label: &str) {
    let rects: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| format!("{}x{}@{},{}", r.width, r.height, r.x, r.y))
        .collect();
    eprintln!(
        "dbm: intro {label:<11} rolling={} says={:?} idle={} ring={}@{} t={} | {} rect(s) {}",
        ui.global::<Playback>().get_intro_rolling(),
        ui.get_credits_label(),
        ui.global::<crate::Chrome>().get_idle(),
        ui.global::<crate::Chrome>().get_focus_visible(),
        ui.get_bar_focus(),
        ui.global::<Playback>().get_elapsed(),
        rects.len(),
        rects.join(" "),
    );
}

fn report_credits(ui: &MainWindow, label: &str) {
    let rects: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| format!("{}x{}@{},{}", r.width, r.height, r.x, r.y))
        .collect();
    eprintln!(
        "dbm: credits {label:<11} rolling={} says={:?} scene={:.1} chapter={:?} at-last={} idle={} menu={} ring={}@{}/row {} t={} {:?} | {} rect(s) {}",
        ui.global::<Playback>().get_credits_rolling(),
        ui.get_credits_label(),
        ui.global::<Playback>().get_after_credits(),
        ui.global::<Playback>().get_chapter_label(),
        ui.global::<Playback>().get_at_last(),
        ui.global::<crate::Chrome>().get_idle(),
        ui.get_menu_open(),
        ui.global::<crate::Chrome>().get_focus_visible(),
        ui.get_bar_focus(),
        ui.get_focus_row(),
        ui.global::<Playback>().get_elapsed(),
        ui.global::<Playback>()
            .get_path()
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_string(),
        rects.len(),
        rects.join(" "),
    );
}

/// Check that a playlist row's length and progress are real.
///
/// The part that cannot be checked by reading the code is whether our idea of
/// where mpv keeps a file's resume position agrees with mpv's. So this plays a
/// while, asks mpv to write that file, and then looks for it by the name we
/// would look for it under — and reports what the rows say before and after.
pub(super) fn progress_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1200));
        beat.next().await;
        report_progress(&ui, &mpv, "on load");
        beat.next().await;
        ui.global::<Playback>().invoke_seek_fraction(0.3);
        beat.next().await;
        // What the ten-second checkpoint does, without the wait.
        if let Err(e) = mpv.run(0, Command::WriteWatchLaterConfig) {
            eprintln!("dbm: progress test: {e}");
        }
        beat.next().await;
        report_resume_file(&mpv);
        beat.next().await;
        // Opening the panel is what asks for a fresh read.
        ui.invoke_open_playlist(true);
        beat.next().await;
        report_progress(&ui, &mpv, "panel open");
        beat.next().await;
        eprintln!("dbm: --- progress test done ---");
    })
}

fn report_progress(ui: &MainWindow, mpv: &Mpv, label: &str) {
    let rows: Vec<String> = playlist_rows(ui)
        .iter()
        .map(|e| {
            format!(
                "{}{}[{} {:.0}%]",
                if e.current { "*" } else { "" },
                short(&e.label),
                if e.length.is_empty() { "?" } else { &e.length },
                e.progress * 100.0,
            )
        })
        .collect();
    eprintln!(
        "dbm: progress {label:<12} t={} | {}",
        super::shown(mpv.get(prop::TIME_POS)),
        rows.join(" "),
    );
}

/// Whether mpv's resume file for the playing path is where we look for it.
///
/// This is the whole question: mpv names it by the MD5 of the path it was
/// given, and if our copy of that rule is wrong every row shows an empty bar
/// and nothing anywhere reports an error.
fn report_resume_file(mpv: &Mpv) {
    let Some(path) = mpv.get(prop::PATH) else {
        eprintln!("dbm: progress: nothing playing");
        return;
    };
    let name = dbm_library::durations::watch_later_name(&path);
    let file = dbm_library::paths::watch_later_dir().join(&name);
    let start = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("start=").map(str::to_owned))
        })
        .unwrap_or_else(|| "none".into());
    eprintln!(
        "dbm: progress resume   {name} exists={} start={start}",
        file.is_file(),
    );
}

/// Raise the action flash from the keyboard and watch it come and go.
///
/// Real key events, because the flash is raised inside the key handlers: an
/// invoked callback would skip the exact statements under test. Each report
/// names the glass rect published for the ring, which is what the flash
/// actually is — the glyph is only what sits in it.
pub(super) fn flash_test(ui: &MainWindow) -> Script {
    use slint::platform::Key;

    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(500));
        beat.next().await;
        report_flash(&ui, "before");
        beat.next().await;
        arrow(&ui, Key::RightArrow);
        beat.next().await;
        report_flash(&ui, "seek forward");
        beat.next().await;
        arrow(&ui, Key::LeftArrow);
        beat.next().await;
        report_flash(&ui, "seek back");
        beat.next().await;
        // Held keys are the common case: the dwell has to restart
        // rather than the second press landing on the first's fade.
        for _ in 0..3 {
            chord(&ui, &[], "z");
        }
        beat.next().await;
        report_flash(&ui, "delay, thrice");
        // Long enough for the dwell and the fade both.
        beat.rest(3).await;
        report_flash(&ui, "after it leaves");
        beat.next().await;
        eprintln!("dbm: --- flash test done ---");
    })
}

fn report_flash(ui: &MainWindow, label: &str) {
    // The flash publishes a ring, and a second rect for the figure when it
    // has one. Nothing else is open in this run, so anything clear of the
    // bar is the flash.
    let bar_top = ui.get_timeline_y();
    let rects: Vec<String> = ui
        .get_glass_rects()
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0 && r.y + r.height < bar_top)
        .map(|r| format!("{}x{}@{:.0},{:.0}", r.width, r.height, r.x, r.y))
        .collect();
    eprintln!(
        "dbm: flash {label:<18} seq={} delay={} | {} rect(s) {}",
        ui.global::<crate::Flash>().get_seq(),
        ui.global::<Subtitles>().get_delay_text(),
        rects.len(),
        rects.join(" "),
    );
}
