//! The media keys, checked from the outside: SMTC on Windows, MPRIS on Linux.

use std::time::Duration;

use slint::ComponentHandle;

use crate::playback::mpv::Mpv;
use crate::MainWindow;

/// Drive the OS media overlay from both ends.
///
/// The outbound half is read back out of Windows rather than out of our own
/// copy of it: `GlobalSystemMediaTransportControlsSessionManager` is the same
/// API the lock screen uses, so whatever it reports is what the machine
/// believes. The inbound half asks Windows to deliver a transport
/// button to this player's own session, which is the route a headset button
/// takes — Windows calling our `ButtonPressed` — aimed rather than broadcast.
/// Every report also names the session Windows has in front, because that is
/// who a real key press would have reached instead.
#[cfg(windows)]
pub(super) fn smtc_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => report_smtc(&ui, &mpv, "start"),
                1 => ask(Button::Pause),
                2 => report_smtc(&ui, &mpv, "pause button"),
                3 => ask(Button::Play),
                4 => report_smtc(&ui, &mpv, "play button"),
                5 => ask(Button::Next),
                6 => report_smtc(&ui, &mpv, "next button"),
                7 => eprintln!("dbm: --- media overlay test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// Ask Windows to deliver one transport button to *this* player's session.
///
/// A real media key is the same route — Windows decides which session gets it
/// and calls that application's `ButtonPressed` — with the aiming done for us.
/// `SendInput` with `VK_MEDIA_PLAY_PAUSE` was the first version of this and had
/// to be abandoned: the session Windows has in front is usually somebody
/// else's, so the press either went to another application's music or the test
/// declined to happen at all. Naming the session removes both problems and
/// covers strictly more of the path.
#[cfg(windows)]
fn ask(button: Button) {
    let Some(session) = our_session() else {
        eprintln!("dbm: smtc test: Windows has no session for this player");
        return;
    };
    let asked = match button {
        Button::Play => session.TryPlayAsync().and_then(|op| op.get()),
        Button::Pause => session.TryPauseAsync().and_then(|op| op.get()),
        Button::Next => session.TrySkipNextAsync().and_then(|op| op.get()),
    };
    match asked {
        // The bool says Windows accepted the request, not that anything came
        // of it — that is what the next report is for.
        Ok(true) => {}
        Ok(false) => eprintln!("dbm: smtc test: Windows refused {button:?}"),
        Err(e) => eprintln!("dbm: smtc test: {button:?} failed: {e}"),
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
enum Button {
    Play,
    Pause,
    Next,
}

/// This player's own entry among the media sessions Windows is tracking.
///
/// Found by the executable name, which is what an unpackaged application's
/// `SourceAppUserModelId` amounts to.
#[cfg(windows)]
fn our_session() -> Option<windows::Media::Control::GlobalSystemMediaTransportControlsSession> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;

    let manager = Manager::RequestAsync().ok()?.get().ok()?;
    manager.GetSessions().ok()?.into_iter().find(|session| {
        session
            .SourceAppUserModelId()
            .is_ok_and(|id| id.to_string_lossy().contains("dbm-player"))
    })
}

/// What the player thinks, and what Windows thinks, side by side. They are the
/// two ends of the same claim and only worth reading together.
#[cfg(windows)]
fn report_smtc(ui: &MainWindow, mpv: &Mpv, label: &str) {
    let (app, title, status) =
        current_session().unwrap_or_else(|| ("nothing".into(), "-".into(), "no session".into()));
    eprintln!(
        "dbm: smtc {label:<14} ours[paused={} entry={} {}] windows[{app} {status} {title:?}]",
        ui.get_paused(),
        mpv.get_property("playlist-pos")
            .unwrap_or_else(|| "-".into()),
        ui.get_media_title(),
    );
}

/// The session Windows would send a media key to: who owns it, what it says is
/// playing, and whether it is playing. Not necessarily us — that is the point
/// of reporting it.
#[cfg(windows)]
fn current_session() -> Option<(String, String, String)> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;

    // Blocking waits, on the UI thread. Acceptable here and nowhere else.
    let manager = Manager::RequestAsync().ok()?.get().ok()?;
    let session = manager.GetCurrentSession().ok()?;
    let app = session
        .SourceAppUserModelId()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|_| "?".into());
    let title = session
        .TryGetMediaPropertiesAsync()
        .ok()
        .and_then(|op| op.get().ok())
        .and_then(|props| props.Title().ok())
        .map(|t| t.to_string_lossy())
        .unwrap_or_else(|| "?".into());
    let status = session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .map(|s| {
            match s.0 {
                0 => "closed",
                1 => "opened",
                2 => "changing",
                3 => "stopped",
                4 => "playing",
                5 => "paused",
                _ => "unknown",
            }
            .to_string()
        })
        .unwrap_or_else(|_| "?".into());
    Some((app, title, status))
}

/// Drive the player's own MPRIS object from outside, the way a desktop
/// widget does.
///
/// Over a client connection of its own, not the one the player serves on: a
/// media key arrives as a method call from another process, and anything
/// short of that would be reading our own copy of what we published back to
/// ourselves. Every report puts the two ends of the same claim side by side
/// — what the player believes, and what a client asking the bus is told.
///
/// The presses run in the order that leaves each one's effect visible to the
/// next report: pause, play, a seek far enough that playing could not have
/// carried the film there between two reports, the next file, and a stop —
/// which pauses, for the reason `mpris::Player::stop` sets out.
#[cfg(not(windows))]
pub(super) fn mpris_test(ui: &MainWindow, mpv: std::sync::Arc<Mpv>) -> slint::Timer {
    let timer = slint::Timer::default();
    let bus = match zbus::blocking::Connection::session() {
        Ok(bus) => bus,
        Err(e) => {
            eprintln!("dbm: mpris test: no session bus ({e})");
            return timer;
        }
    };
    let heard = std::sync::Arc::new(Heard::default());
    let weak = ui.as_weak();
    let mut step = 0usize;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(1200),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match step {
                0 => {
                    neighbours(&bus);
                    listen(&bus, heard.clone());
                    report_mpris(&ui, &mpv, &bus, &heard, "start");
                }
                1 => press(&bus, "Pause", None),
                2 => report_mpris(&ui, &mpv, &bus, &heard, "pause button"),
                3 => press(&bus, "Play", None),
                4 => report_mpris(&ui, &mpv, &bus, &heard, "play button"),
                // Thirty seconds on, which is far enough that no amount of
                // playing could have got there in the tick between reports.
                5 => press(&bus, "Seek", Some(30_000_000)),
                6 => report_mpris(&ui, &mpv, &bus, &heard, "seek"),
                7 => press(&bus, "Next", None),
                8 => report_mpris(&ui, &mpv, &bus, &heard, "next button"),
                9 => press(&bus, "Stop", None),
                10 => report_mpris(&ui, &mpv, &bus, &heard, "stop button"),
                11 => eprintln!("dbm: --- media controls test done ---"),
                _ => {}
            }
            step += 1;
        },
    );
    timer
}

/// What the player has been overheard saying, rather than what it answers
/// when asked.
///
/// Reading properties back proves the answers are right; it does not prove
/// anyone was told. A widget subscribes once and then believes what it
/// hears, so a player whose signals never fire reads as correct here and
/// sits stale on the desktop. These two counts are the difference.
#[cfg(not(windows))]
#[derive(Default)]
struct Heard {
    changed: std::sync::atomic::AtomicUsize,
    seeked: std::sync::atomic::AtomicUsize,
}

#[cfg(not(windows))]
fn listen(bus: &zbus::blocking::Connection, heard: std::sync::Arc<Heard>) {
    let Some(name) = crate::platform::media_keys::mpris::claimed() else {
        eprintln!("dbm: mpris test: nothing of ours on the bus to listen to");
        return;
    };
    // By sender as well as path: every other player on the machine puts its
    // signals on this same object path, and counting theirs would be a pass
    // for having a neighbour.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(name)
        .and_then(|rule| rule.path("/org/mpris/MediaPlayer2"))
        .map(zbus::match_rule::Builder::build);
    let signals = match rule
        .and_then(|rule| zbus::blocking::MessageIterator::for_match_rule(rule, bus, None))
    {
        Ok(signals) => signals,
        Err(e) => {
            eprintln!("dbm: mpris test: cannot listen ({e})");
            return;
        }
    };
    // Detached, and ends with the process: the iterator holds its own share
    // of the connection, and there is nothing after the run to tidy.
    std::thread::spawn(move || {
        use std::sync::atomic::Ordering;
        for message in signals.flatten() {
            let counter = match message.header().member().map(|m| m.as_str()) {
                Some("PropertiesChanged") => &heard.changed,
                Some("Seeked") => &heard.seeked,
                _ => continue,
            };
            counter.fetch_add(1, Ordering::Relaxed);
        }
    });
}

/// Every media player registered on the bus, this one among them.
///
/// The counterpart to naming the session Windows has in front: a media key
/// reaches exactly one of these, and which one is the desktop's choice.
#[cfg(not(windows))]
fn neighbours(bus: &zbus::blocking::Connection) {
    // Two error types, one question: the proxy fails with zbus's own error
    // and the call with the bus's, so both are reduced to their words.
    let asked = zbus::blocking::fdo::DBusProxy::new(bus)
        .map_err(|e| e.to_string())
        .and_then(|dbus| dbus.list_names().map_err(|e| e.to_string()));
    let names = match asked {
        Ok(names) => names,
        Err(e) => {
            eprintln!("dbm: mpris test: cannot list the bus ({e})");
            return;
        }
    };
    let players: Vec<&str> = names
        .iter()
        .map(|name| name.as_str())
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
        .collect();
    eprintln!(
        "dbm: mpris ours={} players on the bus: {}",
        crate::platform::media_keys::mpris::claimed().unwrap_or("none"),
        if players.is_empty() {
            "none".into()
        } else {
            players.join(", ")
        }
    );
}

/// Ask the bus to deliver one transport button to *this* player.
///
/// The same route a media key takes — a client calls the player the desktop
/// has in front — with the choosing done for us, so the test covers the path
/// rather than the desktop's taste in players.
///
/// `offset` is for `Seek`, the only one of these with anything to say.
#[cfg(not(windows))]
fn press(bus: &zbus::blocking::Connection, button: &str, offset: Option<i64>) {
    let Some(player) = ours(bus) else {
        eprintln!("dbm: mpris test: no player of ours on the bus to press");
        return;
    };
    let asked = match offset {
        Some(offset) => player.call_method(button, &(offset,)),
        None => player.call_method(button, &()),
    };
    if let Err(e) = asked {
        eprintln!("dbm: mpris test: {button} refused: {e}");
    }
}

#[cfg(not(windows))]
fn ours(bus: &zbus::blocking::Connection) -> Option<zbus::blocking::Proxy<'static>> {
    zbus::blocking::Proxy::new(
        bus,
        crate::platform::media_keys::mpris::claimed()?,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .ok()
}

/// What the player thinks, and what the bus says it said. The two ends of
/// the same claim, and only worth reading together.
#[cfg(not(windows))]
fn report_mpris(
    ui: &MainWindow,
    mpv: &Mpv,
    bus: &zbus::blocking::Connection,
    heard: &Heard,
    label: &str,
) {
    use std::sync::atomic::Ordering;

    let (status, title, position, next) =
        read_back(bus).unwrap_or_else(|| ("no player".into(), "-".into(), 0.0, false));
    eprintln!(
        "dbm: mpris {label:<14} ours[paused={} entry={} {}] \
         bus[{status} {title:?} at {position:.1}s next={next}] \
         heard[changed={} seeked={}]",
        ui.get_paused(),
        mpv.get_property("playlist-pos")
            .unwrap_or_else(|| "-".into()),
        ui.get_media_title(),
        heard.changed.load(Ordering::Relaxed),
        heard.seeked.load(Ordering::Relaxed),
    );
}

/// The four properties worth reading every time: what is playing, what it is
/// called, where it is, and whether the skip button should be live.
///
/// Blocking calls, on the UI thread. Acceptable here and nowhere else.
#[cfg(not(windows))]
fn read_back(bus: &zbus::blocking::Connection) -> Option<(String, String, f64, bool)> {
    use std::collections::HashMap;
    use zbus::zvariant::OwnedValue;

    let player = ours(bus)?;
    let status = player.get_property::<String>("PlaybackStatus").ok()?;
    let title = player
        .get_property::<HashMap<String, OwnedValue>>("Metadata")
        .ok()
        .and_then(|meta| {
            let title = meta.get("xesam:title")?;
            <&str>::try_from(title).ok().map(str::to_owned)
        })
        .unwrap_or_else(|| "-".into());
    let position = player.get_property::<i64>("Position").unwrap_or(0) as f64 / 1e6;
    let next = player.get_property::<bool>("CanGoNext").unwrap_or(false);
    Some((status, title, position, next))
}
