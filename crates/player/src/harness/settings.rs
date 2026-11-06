//! The settings pages: sliders, switches, resets, and the subtitle steppers.

use std::path::Path;
use std::time::Duration;

use dbm_mpv::prop::{self, TrackChoice};
use dbm_mpv::tracks::TrackId;
use dbm_mpv::{Command, Property, Reader, SeekTarget};
use slint::{ComponentHandle, Model};

use super::script::{self, Beat, Script, Verdict, sleep};
use crate::{Find, MainWindow, Motion, Reset, Settings, Subtitles, SyncState, Tracks};

/// Open the settings panel and drive one parameter to its maximum, so the
/// probe frame lands with a value nothing else would have produced.
pub(super) fn param_test(ui: &MainWindow) -> Script {
    script::run(ui, move |ui| async move {
        sleep(Duration::from_millis(600)).await;
        ui.invoke_open_settings(true);
        // The glass tint amount, by its stable key. Tint runs 0..1, so the
        // value is also the slider's fraction. A non-numeric value opens
        // the panel without touching anything, to probe a value loaded from
        // disk.
        let tint_key = crate::settings::param(crate::settings::Key::Tint).name;
        match std::env::var("DBM_PARAM_TEST")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
        {
            Some(tint) => {
                ui.global::<Settings>()
                    .invoke_set_param(tint_key.into(), tint);
                eprintln!("dbm: settings opened, tint set to {tint}");
            }
            None => eprintln!("dbm: settings opened, values untouched"),
        }
    })
}

/// What mpv holds against what the panel is showing.
///
/// Both, because either alone would miss half of what can go wrong: mpv
/// alone would not catch a readout that never updates, and the readout alone
/// would not catch a value that never left the process.
fn report_subs(ui: &MainWindow, mpv: &Reader, label: &str) {
    // Blocking reads, acceptable only off the frame path.
    //
    // The two positions agree only while nothing lifts the line and the
    // picture fills the window's height: the panel shows where the person
    // put it, mpv where that is in the window, as far up as the bar lets it
    // sit. The subtitles page asks for no lift, so on it, over a film
    // filling the window, they should match.
    eprintln!(
        "dbm: {label:<20} mpv delay={:?} scale={:?} pos={:?} | panel {} {} {} \
         | margins {:?} floor {:.3} lift {:.2}",
        mpv.get(prop::SUB_DELAY),
        mpv.get(prop::SUB_SCALE),
        mpv.get(prop::SUB_POS),
        ui.global::<Subtitles>().get_delay_text(),
        ui.global::<Subtitles>().get_scale_text(),
        ui.global::<Subtitles>().get_pos_text(),
        mpv.get(prop::SUB_USE_MARGINS),
        ui.get_subtitle_floor(),
        ui.get_subtitle_lift(),
    );
}

/// Step each subtitle adjustment, push one past its limit, then reset.
///
/// The clamp is the part worth driving: it is in `commands` so the value
/// saved is the one mpv was given, and a drift between the two would only
/// show as a setting that comes back wrong next run.
pub(super) fn subs_test(ui: &MainWindow, mpv: Reader) -> Script {
    script::run(ui, move |ui| async move {
        let verdict = Verdict::new("subtitle");
        let subtitles = ui.global::<Subtitles>();
        let read = |property: Property<f64>| mpv.get(property).unwrap_or(f64::NAN);
        let near = |property, want: f64| (read(property) - want).abs() < 0.005;
        let mut beat = Beat::new(Duration::from_millis(900));
        beat.next().await;
        report_subs(&ui, &mpv, "start");
        beat.next().await;
        for _ in 0..4 {
            subtitles.invoke_nudge_delay(subtitles.get_delay_step());
        }
        beat.next().await;
        report_subs(&ui, &mpv, "delay +4 steps");
        verdict.check(near(prop::SUB_DELAY, 0.4), "four steps of delay are 0.4 s");
        beat.next().await;
        for _ in 0..3 {
            subtitles.invoke_nudge_scale(subtitles.get_scale_step());
        }
        beat.next().await;
        report_subs(&ui, &mpv, "size +3 steps");
        verdict.check(near(prop::SUB_SCALE, 1.15), "three steps of size are 1.15");
        beat.next().await;
        for _ in 0..5 {
            subtitles.invoke_nudge_pos(-subtitles.get_pos_step());
        }
        beat.next().await;
        report_subs(&ui, &mpv, "position -5 steps");
        verdict.check(subtitles.get_pos_text() == "90%", "five steps up are 90%");
        beat.next().await;
        // Far past the ceiling in one go, which is what a held button
        // amounts to.
        for _ in 0..80 {
            subtitles.invoke_nudge_scale(subtitles.get_scale_step());
        }
        beat.next().await;
        report_subs(&ui, &mpv, "size held up");
        verdict.check(near(prop::SUB_SCALE, 3.0), "the size stops at its ceiling");
        beat.next().await;
        for _ in 0..120 {
            subtitles.invoke_nudge_pos(-subtitles.get_pos_step());
        }
        beat.next().await;
        report_subs(&ui, &mpv, "position held down");
        verdict.check(
            subtitles.get_pos_text() == "0%",
            "the place stops at the top",
        );
        beat.next().await;
        // Both resets of the subtitles menu's pages: size and position,
        // which are saved, and the timing, the film's own.
        subtitles.invoke_reset();
        subtitles.invoke_set_timing(0.0, 1.0);
        beat.next().await;
        report_subs(&ui, &mpv, "after reset");
        verdict.check(
            near(prop::SUB_DELAY, 0.0)
                && near(prop::SUB_SCALE, 1.0)
                && subtitles.get_pos_text() == "100%",
            "the resets put everything back",
        );
        verdict.finish();
    })
}

/// Open the subtitles menu, open its search, take the first result with the
/// keyboard, walk the ring to it and take it back out with Delete's two
/// presses, and report what the menu holds at each step. With `DBM_FIND_FAKE` set the search answers from a fixed list
/// and the download is a subtitle written on the spot, so no key or
/// connection is needed.
pub(super) fn find_test(ui: &MainWindow, mpv: Reader) -> Script {
    use super::drive::chord;
    use slint::Model;
    use slint::platform::Key;

    let report = move |ui: &MainWindow, mpv: &Reader, label: &str| {
        let subs: Vec<String> = ui
            .global::<Tracks>()
            .get_subtitles()
            .iter()
            .map(|t| format!("{}{}", if t.selected { "*" } else { "" }, t.label))
            .collect();
        let hits: Vec<String> = ui
            .global::<Find>()
            .get_hits()
            .iter()
            .map(|h| format!("{} [{}]", h.label, h.detail))
            .collect();
        eprintln!(
            "dbm: find {label:<14} offered={} open={} state={:?} row={} fetching={} said={:?}",
            ui.global::<Find>().get_offered(),
            ui.get_find_open(),
            ui.global::<Find>().get_state(),
            ui.get_focus_row(),
            ui.global::<Find>().get_fetching(),
            ui.global::<Find>().get_said(),
        );
        eprintln!("dbm: find {label:<14} tracks {subs:?}");
        if !hits.is_empty() {
            eprintln!("dbm: find {label:<14} hits {hits:?}");
        }
        eprintln!(
            "dbm: find {label:<14} line={:?}",
            mpv.get(prop::SUB_TEXT).unwrap_or_default()
        );
    };
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(500));
        let down = slint::SharedString::from(Key::DownArrow);
        let up = slint::SharedString::from(Key::UpArrow);
        let enter = slint::SharedString::from(Key::Return);
        let delete = slint::SharedString::from(Key::Delete);
        beat.rest(5).await;
        let _ = mpv.set(prop::PAUSE, true);
        let _ = mpv.run(
            0,
            Command::Seek {
                to: SeekTarget::To(5.0),
                exact: true,
            },
        );
        ui.invoke_open_menu(true);
        beat.next().await;
        report(&ui, &mpv, "menu");
        beat.next().await;
        ui.invoke_open_find();
        beat.rest(2).await;
        report(&ui, &mpv, "searched");
        beat.next().await;
        // The ring onto the first result, and Enter on it.
        chord(&ui, &[], down.as_str());
        beat.next().await;
        chord(&ui, &[], down.as_str());
        beat.next().await;
        eprintln!("dbm: find enter on row {}", ui.get_focus_row());
        chord(&ui, &[], enter.as_str());
        beat.rest(3).await;
        report(&ui, &mpv, "fetched");
        beat.rest(2).await;
        // The ring onto the fetched track, wherever the film's own
        // tracks put it, and Delete twice, since the first asks.
        let fetched = ui
            .global::<Tracks>()
            .get_subtitles()
            .iter()
            .position(|t| t.external && t.selected)
            .map(|i| i as i32 + 1);
        if let Some(fetched) = fetched {
            for _ in 0..8 {
                let row = ui.get_focus_row();
                if row == fetched {
                    break;
                }
                let toward = if row < fetched { &down } else { &up };
                chord(&ui, &[], toward.as_str());
            }
        }
        eprintln!("dbm: find delete on row {}", ui.get_focus_row());
        chord(&ui, &[], delete.as_str());
        beat.next().await;
        chord(&ui, &[], delete.as_str());
        beat.next().await;
        report(&ui, &mpv, "removed");
        beat.next().await;
        eprintln!("dbm: --- find test done ---");
    })
}

/// Run out of downloads without an account and sign in past them, all from
/// the keyboard: a download refused, which opens the sign-in, a wrong
/// password refused, then the right one, and the download that asked going
/// through. Then the account's row under the results, and out. Wants
/// `DBM_FIND_FAKE=spent`.
///
/// It also checks the password field keeps what is in it, where there is a
/// clipboard to check with. First the name, `fake`, is copied and pasted as
/// the password: if that signs in, the clipboard works. Then, signed out and
/// back at the sign-in by way of a result, the password field is told to
/// copy *leak*, and the name is pasted over with
/// the clipboard. If the field kept its own, the name is still `fake` and the
/// sign-in goes through; if it leaked, the name is *leak*, and it is refused.
pub(super) fn sign_in_test(ui: &MainWindow) -> Script {
    use super::drive::chord;
    use slint::platform::Key;

    let report = |ui: &MainWindow, label: &str| {
        eprintln!(
            "dbm: sign-in {label:<16} find={} account={} row={} busy={} said={:?} \
             as={:?} left={} fetching={}",
            ui.get_find_open(),
            ui.get_account_open(),
            ui.get_focus_row(),
            ui.global::<Find>().get_signing_in(),
            ui.global::<Find>().get_sign_in_said(),
            ui.global::<Find>().get_account(),
            ui.global::<Find>().get_left(),
            ui.global::<Find>().get_fetching(),
        );
    };
    let typed = |ui: &MainWindow, text: &str| {
        for c in text.chars() {
            chord(ui, &[], &c.to_string());
        }
    };
    let press = |ui: &MainWindow, key: Key| chord(ui, &[], &slint::SharedString::from(key));
    let control = |ui: &MainWindow, text: &str| chord(ui, &[Key::Control], text);

    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(400));
        let find = ui.global::<Find>();
        beat.rest(5).await;
        ui.invoke_open_menu(true);
        beat.next().await;
        ui.invoke_open_find();
        beat.rest(2).await;
        // The ring up, onto the first result, and Enter on it.
        press(&ui, Key::DownArrow);
        beat.next().await;
        press(&ui, Key::DownArrow);
        beat.next().await;
        press(&ui, Key::Return);
        beat.rest(2).await;
        // Refused, so the sign-in's page, on its first field.
        report(&ui, "refused");
        beat.next().await;
        typed(&ui, "fake");
        beat.next().await;
        press(&ui, Key::Return);
        beat.next().await;
        typed(&ui, "nope");
        beat.next().await;
        press(&ui, Key::Return);
        beat.next().await;
        report(&ui, "wrong password");
        beat.rest(3).await;
        // A second on, for the sign-ins' own limit: the name copied
        // and pasted as the password.
        press(&ui, Key::UpArrow);
        control(&ui, "a");
        control(&ui, "c");
        press(&ui, Key::DownArrow);
        control(&ui, "v");
        press(&ui, Key::Return);
        beat.rest(2).await;
        let clipboard = find.get_account() == "fake";
        if clipboard {
            report(&ui, "pasted, in");
        } else {
            report(&ui, "no clipboard");
            typed(&ui, "fake");
            press(&ui, Key::Return);
        }
        beat.rest(2).await;
        report(&ui, "signed in");
        beat.rest(2).await;
        report(&ui, "fetched");
        beat.next().await;
        ui.invoke_open_menu(true);
        ui.invoke_open_find();
        beat.next().await;
        // Down until the ring is on the account's row, under the
        // results, however many presses that is from wherever it was
        // left.
        report(&ui, "searched again");
        let account = 1 + find.get_hits().row_count() as i32;
        for _ in 0..=account {
            if ui.get_focus_row() == account {
                break;
            }
            press(&ui, Key::DownArrow);
        }
        press(&ui, Key::Return);
        beat.next().await;
        report(&ui, "account again");
        press(&ui, Key::Return);
        beat.next().await;
        report(&ui, "signed out");
        beat.next().await;
        // The leak: a result pressed for the sign-in again, the
        // password field told to copy, and the name pasted over with
        // whatever it held.
        if clipboard {
            press(&ui, Key::Return);
        }
        beat.rest(2).await;
        if clipboard {
            typed(&ui, "fake");
            press(&ui, Key::DownArrow);
            typed(&ui, "leak");
            control(&ui, "a");
            control(&ui, "c");
            control(&ui, "a");
            press(&ui, Key::Backspace);
            press(&ui, Key::UpArrow);
            control(&ui, "a");
            control(&ui, "v");
            press(&ui, Key::DownArrow);
            typed(&ui, "fake");
            press(&ui, Key::Return);
        }
        beat.rest(2).await;
        if clipboard {
            let kept = find.get_account() == "fake";
            report(&ui, if kept { "nothing copied" } else { "LEAKED" });
        }
        beat.next().await;
        eprintln!(
            "dbm: --- sign-in test done{} ---",
            if clipboard {
                ""
            } else {
                " (no clipboard, so no leak check)"
            }
        );
    })
}

/// Adds a subtitle file to a paused film the way a finished download does,
/// with the subtitles menu open, and then touches nothing, reporting what the
/// menu knows of it. A paused film draws no frames of its own, so the menu
/// has only the wake-ups to learn from.
pub(super) fn added_test(ui: &MainWindow, mpv: Reader) -> Script {
    let file = std::env::var("DBM_ADDED_TEST").unwrap_or_default();
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(250));
        beat.rest(12).await;
        crate::playback::commands::set_pause(&mpv, true);
        beat.rest(4).await;
        ui.invoke_open_menu(true);
        beat.rest(4).await;
        let language = dbm_naming::Language::parse("en");
        crate::playback::commands::add_subtitle(
            &mpv,
            std::path::Path::new(&file),
            language.as_ref(),
            true,
        );
        // What the menu holds every half second for five seconds after.
        for half in 0..=10 {
            eprintln!(
                "dbm: added t+{:.1}s tracks={} current={} visible={}",
                half as f32 * 0.5,
                ui.global::<Tracks>().get_subtitles().row_count(),
                ui.global::<Tracks>().get_subtitle_current(),
                ui.global::<Subtitles>().get_visible(),
            );
            beat.rest(2).await;
        }
    })
}

/// Sync the subtitles to the film's speech, from the keyboard, and read back
/// what mpv then shows.
///
/// `DBM_SYNC_TEST` names the subtitle track, by mpv's `sid` or as a file to
/// load, and `DBM_SYNC_AT` a moment to read the line from afterwards: the
/// only check that the timing went the right way, since a sync five seconds
/// the wrong way reports the same number.
///
/// The row is reached with real keys (V for the subtitles menu, End to its
/// last track, down once to Timing under the tracks, Enter into its page and
/// down once to the sync), the path the menu's Enter answers and a click
/// never touches. Then Enter again takes the sync back.
pub(super) fn sync_test(ui: &MainWindow, mpv: Reader) -> Script {
    use super::drive::chord;
    use slint::SharedString;
    use slint::platform::Key;

    let track = std::env::var("DBM_SYNC_TEST").unwrap_or_default();
    let at = std::env::var("DBM_SYNC_AT")
        .ok()
        .and_then(|at| at.parse().ok())
        .unwrap_or(19.0);
    let report = move |ui: &MainWindow, mpv: &Reader, label: &str| {
        eprintln!(
            "dbm: sync {label:<20} state={:?} delay={:+.2} speed={:.4} said={:?} line={:?}",
            ui.global::<Subtitles>().get_sync_state(),
            ui.global::<Subtitles>().get_delay(),
            ui.global::<Subtitles>().get_speed(),
            ui.global::<crate::Flash>().get_message(),
            mpv.get(prop::SUB_TEXT).unwrap_or_default(),
        );
    };
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(500));
        let subtitles = ui.global::<Subtitles>();
        let down = SharedString::from(Key::DownArrow);
        let end = SharedString::from(Key::End);
        let enter = SharedString::from(Key::Return);
        // Let the film open first: there is no track to pick before.
        beat.rest(5).await;
        if let Ok(id) = track.parse::<i64>() {
            let _ = mpv.set(prop::SID, TrackChoice::Id(TrackId(id)));
        } else {
            let _ = mpv.run(
                0,
                Command::SubAdd {
                    file: Path::new(&track),
                    select: true,
                    title: None,
                    lang: None,
                },
            );
        }
        let _ = mpv.set(prop::PAUSE, true);
        let _ = mpv.run(
            0,
            Command::Seek {
                to: SeekTarget::To(at),
                exact: true,
            },
        );
        beat.rest(2).await;
        report(&ui, &mpv, "before");
        for key in [
            "v",
            end.as_str(),
            down.as_str(),
            enter.as_str(),
            down.as_str(),
        ] {
            beat.next().await;
            chord(&ui, &[], key);
        }
        beat.rest(4).await;
        eprintln!("dbm: sync enter on row {}", ui.get_focus_row());
        chord(&ui, &[], enter.as_str());
        // Hold here until the row stops listening.
        beat.next().await;
        let mut waited = 0usize;
        while subtitles.get_sync_state() == SyncState::Listening {
            waited += 1;
            if waited % 4 == 1 {
                eprintln!("dbm: sync listening...");
            }
            beat.next().await;
        }
        // A beat for mpv to redraw the line at its new timing.
        beat.rest(2).await;
        report(&ui, &mpv, "after the sync");
        // Only where there is a sync to take back, or Enter would start
        // another. `DBM_SYNC_KEEP` leaves it standing, for a run that goes on
        // to look at what was saved.
        beat.next().await;
        if subtitles.get_sync_state() == SyncState::Undoable
            && std::env::var_os("DBM_SYNC_KEEP").is_none()
        {
            chord(&ui, &[], enter.as_str());
        }
        beat.rest(2).await;
        report(&ui, &mpv, "after taking it back");
        beat.next().await;
        eprintln!("dbm: --- sync test done ---");
    })
}

/// The glass slider rows, as the panel would show them, named in the output
/// so a reordered registry can't silently change what is reported.
fn report_row(ui: &MainWindow, label: &str) {
    let rows: Vec<String> = ui
        .global::<Settings>()
        .get_glass_params()
        .iter()
        .map(|p| format!("{}={}", p.label, p.readout))
        .collect();
    eprintln!("dbm: {label:<18} {}", rows.join(" "));
}

/// Every motion token, as the interface reads it.
fn report_motion(ui: &MainWindow, label: &str) {
    let motion = ui.global::<Motion>();
    eprintln!(
        "dbm: {label:<18} highlight={}ms track-grow={}ms surface={}ms switch={}ms \
         drill={}ms",
        motion.get_highlight(),
        motion.get_track_grow(),
        motion.get_surface(),
        motion.get_switch(),
        motion.get_drill(),
    );
}

/// Walk the effect switches, autoplay and the animations switch, leaving each
/// one back where it started so a run does not quietly rewrite the saved
/// settings.
///
/// What matters is not the property afterwards, which only says the click
/// landed, but what the probe reads from the framebuffer between steps, and
/// what mpv says `keep-open` became.
pub(super) fn toggle_test(ui: &MainWindow, mpv: Reader) -> Script {
    script::run(ui, move |ui| async move {
        let mut beat = Beat::new(Duration::from_millis(1200));
        // A panel has to be on screen for there to be glass to sample.
        let flip = |name: &str, on: bool| eprintln!("dbm: {name} -> {on}");
        beat.next().await;
        ui.invoke_open_settings(true);
        beat.next().await;
        // Only the ambient border has a switch. The probe still
        // samples the glass under the open panel between steps.
        ui.global::<Settings>().set_ambience_on(false);
        ui.global::<Settings>().invoke_set_ambience(false);
        flip("ambience", false);
        beat.next().await;
        ui.global::<Settings>().set_ambience_on(true);
        ui.global::<Settings>().invoke_set_ambience(true);
        flip("ambience", true);
        beat.next().await;
        ui.global::<Settings>().set_autoplay(false);
        ui.global::<Settings>().invoke_set_autoplay(false);
        beat.next().await;
        // A blocking read, acceptable only off the frame path.
        // Autoplay has no picture to check, so mpv is the witness.
        eprintln!(
            "dbm: autoplay off -> keep-open={:?}",
            mpv.get(prop::KEEP_OPEN)
        );
        beat.next().await;
        ui.global::<Settings>().set_autoplay(true);
        ui.global::<Settings>().invoke_set_autoplay(true);
        beat.next().await;
        eprintln!(
            "dbm: autoplay on  -> keep-open={:?}",
            mpv.get(prop::KEEP_OPEN)
        );
        beat.next().await;
        // Animations, the way the switch turns them off: `Motion`
        // first, then the callback that saves it. The tokens every
        // `animate` reads should be zero with it off, and back with it
        // on.
        ui.global::<Motion>().set_chosen(false);
        ui.global::<Settings>().invoke_set_animations(false);
        report_motion(&ui, "animations off");
        beat.next().await;
        ui.global::<Motion>().set_chosen(true);
        ui.global::<Settings>().invoke_set_animations(true);
        report_motion(&ui, "animations on");
        beat.next().await;
        // Reset: drive one slider somewhere it never is by default,
        // put the section back, and read the row again. The row is
        // what to check, since `DBM_PARAM_TEST` covers the pipeline,
        // and a model rebuilt rather than updated in place would lose
        // the drag. Tint is found by key, as in `param_test`.
        let tint = crate::settings::param(crate::settings::Key::Tint).name;
        ui.global::<Settings>().invoke_set_param(tint.into(), 1.0);
        eprintln!("dbm: tint driven to max");
        beat.next().await;
        report_row(&ui, "after tint max");
        beat.next().await;
        ui.global::<Settings>().invoke_reset_section(Reset::Glass);
        eprintln!("dbm: glass section reset");
        beat.next().await;
        report_row(&ui, "after reset");
        beat.next().await;
        eprintln!("dbm: --- toggle test done ---");
    })
}
