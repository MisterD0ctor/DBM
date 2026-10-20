//! The desktop's media controls, over D-Bus.
//!
//! The Linux counterpart to `smtc`, doing the same two things: transport
//! buttons in, what is playing out. Both travel over MPRIS: a bus name under
//! `org.mpris.MediaPlayer2.`, two interfaces on one object, and a
//! `PropertiesChanged` signal whenever something moves. GNOME's media
//! section, Plasma's media applet, `playerctl` and the media keys are all
//! clients of it.
//!
//! Why this is a module of its own rather than a `cfg` arm of `smtc`:
//!
//! * Nothing has to be retried from the frame path: a bus name is claimed at
//!   startup, with no window handle to wait for.
//! * A D-Bus connection is a socket with a handshake and round trips, and
//!   nothing may block the frame path, so the connection lives on a thread of
//!   its own and hears about the player down a channel.
//! * State is pulled as well as pushed: a client may ask for any property at
//!   any moment, so the answers sit where the bus thread can read them, in
//!   the `Shown` inside the served object.
//!
//! Position is the exception the specification makes: it moves continuously,
//! is excluded from `PropertiesChanged`, and clients read it when they want
//! it. It rides an atomic the frame path stores and the bus thread loads.
//!
//! The outbound half rests on frames: `publish` is called from the render
//! driver, where mpv's events are drained, and a frame happens only when
//! there is something to draw. While a film plays, or is paused with the
//! window on screen, there always is; when nothing is drawn the last thing
//! said stands, and asking Slint for a frame doesn't change that. Hence
//! `Player::stop` pauses rather than unloading. The Windows side has the
//! same limitation.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use zbus::blocking::Connection;
use zbus::names::InterfaceName;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, Value};

use crate::playback::commands;
use crate::playback::mpv::Mpv;
use crate::playback::state::PlayerState;

/// What this application is called on a Linux desktop: its desktop entry's
/// name, the window's Wayland app id, and the tail of the bus name below. All
/// three must match, or the desktop can't tell that the window, the icon and
/// the media widget belong to one program.
pub const APP_ID: &str = "io.github.MisterD0ctor.DBM";

/// The object both interfaces sit on. Fixed by the specification: a client
/// that has found the bus name comes straight here without asking.
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

/// The trackid prefix: `APP_ID` with its dots turned into path separators,
/// spelled out rather than derived at run time.
const TRACK: &str = "/io/github/MisterD0ctor/DBM/track/";

/// How far the position may move, beyond what playing explains, before it is
/// a seek.
///
/// Half of the smallest seek the player offers (every seek here is a second
/// or more), leaving room for what moves the clock without a seek: ending a
/// pause, where mpv's position can step back by the buffered audio.
///
/// A needless `Seeked` costs a client one redraw, while a missed seek leaves
/// every progress bar on the desktop wrong, so when in doubt, report.
const JUMP: f64 = commands::FINE_SEEK_STEP / 2.0;

/// The bus name this player owns while it runs.
///
/// Suffixed with the whole app id for the Flatpak, whose D-Bus filter only
/// lets a sandboxed application own `org.mpris.MediaPlayer2.<app id>` and
/// names under it.
fn bus_name() -> String {
    format!("org.mpris.MediaPlayer2.{APP_ID}")
}

/// The name that was actually claimed, once the bus thread has claimed it.
///
/// A process owns one or none, so a global. Read by `DBM_MPRIS_TEST`, which
/// addresses the player from outside.
static CLAIMED: OnceLock<String> = OnceLock::new();

pub fn claimed() -> Option<&'static str> {
    CLAIMED.get().map(String::as_str)
}

// ---------------------------------------------------------------------------
// The frame path's side
// ---------------------------------------------------------------------------

pub struct Controls {
    /// Where playback is, in microseconds. Written every frame, read by the
    /// bus thread whenever a client asks (see the module's note on Position).
    position: Arc<AtomicI64>,
    sending: RefCell<Sending>,
}

/// What the frame path keeps so that it can say as little as possible.
struct Sending {
    /// `None` once the bus thread has gone, which is also what a session
    /// with no bus at all looks like from here.
    words: Option<Sender<Word>>,
    /// The last thing said, and what the served object therefore holds.
    shown: Shown,
    /// When the position was last looked at, what it was, and whether the
    /// film was paused then, which together make a jump recognisable.
    sampled: Option<(Instant, f64, bool)>,
}

/// Everything the frame path has to tell the bus thread.
enum Word {
    State(Shown),
    /// A jump, in microseconds, which `PropertiesChanged` doesn't carry.
    Seeked(i64),
}

impl Controls {
    pub fn new(mpv: Arc<Mpv>) -> Self {
        let position = Arc::new(AtomicI64::new(0));
        let (words, heard) = mpsc::channel();

        // Detached: the thread ends when the sender above is dropped, and
        // dropping the connection with it releases the bus name.
        let spawned = std::thread::Builder::new().name("dbm-mpris".into()).spawn({
            let position = position.clone();
            move || serve(heard, mpv, position)
        });
        if let Err(e) = &spawned {
            eprintln!("dbm: no thread for the media controls ({e})");
        }

        Self {
            position,
            sending: RefCell::new(Sending {
                words: spawned.is_ok().then_some(words),
                shown: Shown::default(),
                sampled: None,
            }),
        }
    }

    /// Tell the bus what is playing, if it has changed.
    ///
    /// The window is unused, but in the signature because `smtc` registers
    /// against it. `moved` says whether any mpv property changed, so a state
    /// that can't have moved isn't compared field by field.
    pub fn publish(&self, _window: &slint::Window, player: &PlayerState, moved: bool) {
        self.position
            .store(micros(player.time_pos), Ordering::Relaxed);

        let mut sending = self.sending.borrow_mut();
        if sending.words.is_none() {
            return;
        }
        // Every frame, not only the ones that moved, since the test is against
        // the wall clock and must sample the frame before, or unpausing after
        // a minute would look like a minute of position appearing at once.
        if let Some(position) = sending.jumped(player) {
            sending.say(Word::Seeked(position));
        }
        if !moved || sending.shown.describes(player) {
            return;
        }

        let track = sending.shown.track + u64::from(sending.shown.switched(player));
        let now = Shown::of(player, track);
        sending.shown = now.clone();
        sending.say(Word::State(now));
    }
}

impl Sending {
    fn say(&mut self, word: Word) {
        let Some(words) = self.words.as_ref() else {
            return;
        };
        if words.send(word).is_err() {
            // The bus thread gave up on the connection, so stop composing
            // things to say.
            self.words = None;
        }
    }

    /// Where playback has landed, when it landed there by some other means
    /// than playing.
    ///
    /// Worked out rather than reported, because the frame path deals in
    /// positions, not the commands that produced them. The test is against
    /// the wall clock and the speed, so ordinary playing never trips it (see
    /// `JUMP`).
    fn jumped(&mut self, player: &PlayerState) -> Option<i64> {
        let now = Instant::now();
        let sample = (now, player.time_pos, player.paused);
        let (then, before, was_paused) = self.sampled.replace(sample)?;
        // A file that has just opened is at the beginning because it is new.
        if self.shown.switched(player) {
            return None;
        }
        // Nothing plays while paused, so the clock explains nothing, or a
        // seek during a long pause would pass for playback. Either end paused
        // counts, since frames stop while paused and the sample before can be
        // from the far side of the pause.
        let played = if was_paused || player.paused {
            0.0
        } else {
            now.duration_since(then).as_secs_f64() * self.shown.rate
        };
        // Asymmetric, because playback is: it carries the position forward at
        // most as fast as the clock, and never back. So a seek is further on
        // than playing could reach, or anywhere behind. Falling short of the
        // clock is a stall, not a seek: while nothing is drawn, mpv's output
        // blocks and the film waits with it.
        let moved = player.time_pos - before;
        (moved > played + JUMP || moved < -JUMP).then(|| micros(player.time_pos))
    }
}

// ---------------------------------------------------------------------------
// What the bus is told
// ---------------------------------------------------------------------------

/// The player as the bus sees it.
///
/// Carries what the name was parsed from as well as the name, so
/// `describes` can answer without parsing it again.
#[derive(Clone)]
struct Shown {
    /// Which file this is, counted from the start of the run rather than the
    /// playlist position, which is a different file once the list is
    /// replaced. `SetPosition` uses it to refuse a seek meant for the file
    /// before.
    track: u64,
    /// mpv's `path`, as it was given. Empty when nothing is loaded.
    path: String,
    /// mpv's `media-title`, which arrives after the path does.
    media_title: String,
    /// The name as the bar shows it (`PlayerState::display_title`).
    title: String,
    /// Length in microseconds, zero when it is not known yet.
    length: i64,
    paused: bool,
    /// Whether there is anything to skip to.
    playlist: bool,
    seekable: bool,
    /// MPRIS counts 1.0 as normal, mpv counts 100.
    volume: f64,
    rate: f64,
}

impl Default for Shown {
    /// Nothing playing, a true answer rather than a placeholder, so the
    /// object is served with it and the first real state is a change like
    /// any other.
    fn default() -> Self {
        Self {
            track: 0,
            path: String::new(),
            media_title: String::new(),
            title: String::new(),
            length: 0,
            paused: false,
            playlist: false,
            seekable: false,
            volume: 1.0,
            rate: 1.0,
        }
    }
}

impl Shown {
    fn of(player: &PlayerState, track: u64) -> Self {
        Self {
            track,
            path: player.path.clone().unwrap_or_default(),
            media_title: player.media_title.clone().unwrap_or_default(),
            title: player.display_title(),
            length: micros(player.duration),
            paused: player.paused,
            playlist: playlist(player),
            seekable: seekable(player),
            volume: volume(player),
            rate: rate(player),
        }
    }

    /// Whether the player still says what this was built from.
    ///
    /// Not a comparison against a freshly built `Shown`, since building one
    /// parses a title, which allocates, and while a film plays every frame
    /// reaches here. So the test is against what the title is parsed from.
    fn describes(&self, player: &PlayerState) -> bool {
        self.path == player.path.as_deref().unwrap_or_default()
            && self.media_title == player.media_title.as_deref().unwrap_or_default()
            && self.length == micros(player.duration)
            && self.paused == player.paused
            && self.playlist == playlist(player)
            && self.seekable == seekable(player)
            && self.volume == volume(player)
            && self.rate == rate(player)
    }

    /// Whether a different file is loaded than the one this describes.
    fn switched(&self, player: &PlayerState) -> bool {
        self.path != player.path.as_deref().unwrap_or_default()
    }

    fn has_file(&self) -> bool {
        !self.path.is_empty()
    }

    /// `Stopped` covers both nothing loaded and nothing to load, which is
    /// also where `Stop` leaves the player.
    fn status(&self) -> &'static str {
        if !self.has_file() {
            "Stopped"
        } else if self.paused {
            "Paused"
        } else {
            "Playing"
        }
    }

    fn trackid(&self) -> ObjectPath<'static> {
        ObjectPath::try_from(format!("{TRACK}{}", self.track))
            .expect("fixed text and digits are a valid path")
    }

    fn metadata(&self) -> Meta {
        let mut meta = Meta::new();
        if !self.has_file() {
            // Empty, as the specification asks for when there is no track.
            return meta;
        }
        meta.insert("mpris:trackid", Value::from(self.trackid()));
        if self.length > 0 {
            meta.insert("mpris:length", Value::from(self.length));
        }
        meta.insert("xesam:title", Value::from(self.title.clone()));
        // Only an absolute path can become a URI, and mpv reports the path it
        // was given, which a command-line argument can leave relative.
        if self.path.starts_with('/') {
            meta.insert("xesam:url", Value::from(file_uri(&self.path)));
        }
        meta
    }
}

/// A metadata dictionary: `a{sv}`, which is the only shape MPRIS has for it.
type Meta = HashMap<&'static str, Value<'static>>;

/// What moved, for one `PropertiesChanged`.
///
/// One signal carrying everything rather than the generated per-property
/// helpers, which emit one apiece, so a client doesn't redraw through states
/// that were never true.
fn changes(before: &Shown, now: &Shown) -> HashMap<&'static str, Value<'static>> {
    let mut changed = HashMap::new();
    if before.status() != now.status() {
        changed.insert("PlaybackStatus", Value::from(now.status()));
    }
    if before.track != now.track
        || before.title != now.title
        || before.path != now.path
        || before.length != now.length
    {
        changed.insert("Metadata", Value::from(now.metadata()));
    }
    if before.playlist != now.playlist {
        changed.insert("CanGoNext", Value::from(now.playlist));
        changed.insert("CanGoPrevious", Value::from(now.playlist));
    }
    if before.has_file() != now.has_file() {
        changed.insert("CanPlay", Value::from(now.has_file()));
        changed.insert("CanPause", Value::from(now.has_file()));
    }
    if before.seekable != now.seekable {
        changed.insert("CanSeek", Value::from(now.seekable));
    }
    if before.volume != now.volume {
        changed.insert("Volume", Value::from(now.volume));
    }
    if before.rate != now.rate {
        changed.insert("Rate", Value::from(now.rate));
    }
    changed
}

// ---------------------------------------------------------------------------
// The two interfaces
// ---------------------------------------------------------------------------

/// `org.mpris.MediaPlayer2`: the application, rather than what it is playing.
struct Root;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    /// Bring the window forward.
    ///
    /// Nothing, and `can_raise` says so: Wayland won't let an application
    /// lift its own window without an activation token from the click that
    /// asked, and this request comes from another process.
    fn raise(&self) {}

    /// Close the player.
    ///
    /// Back through the event loop rather than straight out, since this
    /// arrives on a bus thread, and the window's own close is what lets mpv
    /// write the resume position.
    fn quit(&self) {
        let _ = slint::invoke_from_event_loop(|| {
            let _ = slint::quit_event_loop();
        });
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &'static str {
        "Death by MPV"
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &'static str {
        APP_ID
    }

    /// Both empty, which says the player won't open things for you: `OpenUri`
    /// isn't offered, since opening a file here means scanning its folder
    /// into a playlist, a long way from handing a path to a decoder.
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> &'static [&'static str] {
        &[]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> &'static [&'static str] {
        &[]
    }
}

/// `org.mpris.MediaPlayer2.Player`: the transport, and what is under it.
struct Player {
    mpv: Arc<Mpv>,
    position: Arc<AtomicI64>,
    /// The last thing the frame path said. Every property below is a view of
    /// it, since a property may be asked for at any moment, and reading mpv
    /// blocks on its core lock.
    shown: Shown,
}

/// Every method below **runs on a bus thread**, not the UI thread, as the
/// Windows button handler does. mpv's commands are queued and thread-safe,
/// and a press marshalled to the UI thread while the interface is asleep
/// would wait for a frame that isn't coming.
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    /// Skipping is stepping and playing, as the button at the end of a file
    /// does. Guarded by the same answer `CanGoNext` gives, since the
    /// specification says a method whose `Can…` is false must do nothing.
    fn next(&self) {
        if self.shown.playlist {
            commands::advance(&self.mpv, 1);
        }
    }

    fn previous(&self) {
        if self.shown.playlist {
            commands::advance(&self.mpv, -1);
        }
    }

    fn pause(&self) {
        commands::set_pause(&self.mpv, true);
    }

    fn play_pause(&self) {
        commands::toggle_pause(&self.mpv);
    }

    fn play(&self) {
        if self.shown.has_file() {
            commands::set_pause(&self.mpv, false);
        }
    }

    /// Stop, which here means pause.
    ///
    /// MPRIS means stop and rewind, and mpv's `stop` does that by unloading.
    /// But a player with no film draws nothing, and everything the bus is
    /// told rides a frame, so the bus would go on saying `Playing` with the
    /// old title until the window was touched. Rewinding without unloading is
    /// worse: the resume checkpoint would write the zero over the person's
    /// place. Pausing stops the sound, which is what pressing it wants, and
    /// keeps what the bus reports true. Windows doesn't implement Stop at all
    /// (`smtc::pressed` lets it fall through).
    fn stop(&self) {
        commands::set_pause(&self.mpv, true);
    }

    /// Seek by an offset, which MPRIS counts in microseconds and allows to
    /// be negative.
    fn seek(&self, offset: i64) {
        if self.shown.seekable {
            commands::seek_relative(&self.mpv, offset as f64 / 1e6);
        }
    }

    /// Seek to a point in a named track.
    ///
    /// The name matters: a bar clicked a moment after the file changed would
    /// otherwise land in the new file at a point that meant something in the
    /// old one.
    fn set_position(&self, track: ObjectPath<'_>, position: i64) {
        if self.shown.seekable && track == self.shown.trackid() {
            commands::seek_to(&self.mpv, position as f64 / 1e6);
        }
    }

    #[zbus(property)]
    fn playback_status(&self) -> &'static str {
        self.shown.status()
    }

    #[zbus(property)]
    fn metadata(&self) -> Meta {
        self.shown.metadata()
    }

    /// Where playback is, in microseconds.
    ///
    /// The one property with no signal: MPRIS excludes it from
    /// `PropertiesChanged` because it moves with the film, so clients read it
    /// when needed and extrapolate in between, with `Seeked` for the jumps.
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.position.load(Ordering::Relaxed)
    }

    /// A jump the client could not have predicted.
    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.shown.volume
    }

    /// Unmutes, as `commands::set_volume` does, since a client that places a
    /// level wants to hear it.
    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        commands::set_volume(&self.mpv, volume.max(0.0) * 100.0);
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.shown.rate
    }

    /// Zero is the specification's spelling of "pause", and it is explicit
    /// that a player should read it that way rather than stop the clock.
    #[zbus(property)]
    fn set_rate(&self, rate: f64) {
        if rate <= 0.0 {
            commands::set_pause(&self.mpv, true);
            return;
        }
        let (slowest, fastest) = commands::speed_limits();
        commands::set_speed(&self.mpv, rate.clamp(slowest, fastest));
    }

    /// The ends of the ladder `[` and `]` step through, so that a client
    /// offering a rate slider offers the same range the keyboard does.
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        commands::speed_limits().0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        commands::speed_limits().1
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.shown.playlist
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.shown.playlist
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.shown.has_file()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.shown.has_file()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.shown.seekable
    }

    /// Whether the player takes orders at all. It does; the `Can…` answers
    /// above say whether a given one makes sense now.
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// The bus thread
// ---------------------------------------------------------------------------

/// Hold the connection and keep the served object up to date.
///
/// Returns when the frame path drops its sender, which drops the connection
/// and releases the name with it.
fn serve(words: Receiver<Word>, mpv: Arc<Mpv>, position: Arc<AtomicI64>) {
    let conn = match claim(mpv, position) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!(
                "dbm: no media controls on the session bus ({e}); the media keys \
                 will reach whatever else is playing"
            );
            return;
        }
    };
    let served = conn
        .object_server()
        .interface::<_, Player>(PATH)
        .expect("the object was served a line ago");
    let iface_name =
        InterfaceName::try_from(PLAYER).expect("the specification's own interface name");

    for word in words {
        match word {
            Word::Seeked(position) => {
                let _ = zbus::block_on(served.signal_emitter().seeked(position));
            }
            Word::State(now) => {
                // Held only long enough to swap the state in: a client
                // reading a property waits on this lock.
                let changed = {
                    let mut iface = served.get_mut();
                    let before = std::mem::replace(&mut iface.shown, now);
                    changes(&before, &iface.shown)
                };
                if changed.is_empty() {
                    continue;
                }
                let _ = zbus::block_on(zbus::fdo::Properties::properties_changed(
                    served.signal_emitter(),
                    iface_name.clone(),
                    changed,
                    Cow::Borrowed(&[]),
                ));
            }
        }
    }
}

/// Connect, serve both interfaces, and take a name the desktop will look
/// under.
fn claim(mpv: Arc<Mpv>, position: Arc<AtomicI64>) -> zbus::Result<Connection> {
    let conn = zbus::blocking::connection::Builder::session()?
        .serve_at(PATH, Root)?
        .serve_at(
            PATH,
            Player {
                mpv,
                position,
                shown: Shown::default(),
            },
        )?
        .build()?;

    // Asked for here rather than through the builder, so `DoNotQueue` can be
    // set. Otherwise the bus queues a request for a held name and reports
    // success, leaving a second copy believing itself registered.
    let mut name = bus_name();
    let mut asked = conn.request_name_with_flags(
        name.as_str(),
        zbus::fdo::RequestNameFlags::DoNotQueue.into(),
    );
    if matches!(asked, Err(zbus::Error::NameTaken)) {
        // Another copy is already running, so, as the specification says,
        // add the process id, which reads as a second instance of the same
        // application.
        name = format!("{name}.instance{}", std::process::id());
        asked = conn.request_name_with_flags(
            name.as_str(),
            zbus::fdo::RequestNameFlags::DoNotQueue.into(),
        );
    }
    asked?;

    eprintln!("dbm: media keys registered as {name}");
    let _ = CLAIMED.set(name);
    Ok(conn)
}

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// The four answers that are arithmetic on mpv's own numbers rather than a
/// copy of one.
///
/// Each is written once because it is wanted twice, to build a `Shown` and
/// to ask whether one still holds, and the two must ask the same question.
fn playlist(player: &PlayerState) -> bool {
    player.playlist_count > 1
}

fn seekable(player: &PlayerState) -> bool {
    player.path.is_some() && player.duration > 0.0
}

/// MPRIS counts 1.0 as normal, mpv counts 100.
fn volume(player: &PlayerState) -> f64 {
    player.volume / 100.0
}

/// Zero until mpv first reports a speed, which reads as normal.
fn rate(player: &PlayerState) -> f64 {
    if player.speed > 0.0 {
        player.speed
    } else {
        1.0
    }
}

/// Seconds as microseconds, which is the only unit MPRIS has for time.
fn micros(seconds: f64) -> i64 {
    (seconds.max(0.0) * 1e6) as i64
}

/// A path as the `file://` URI `xesam:url` has to be.
///
/// Written out rather than taken from a crate, since the rule is one line:
/// every byte outside the unreserved set becomes a percent triple. Bytes
/// rather than characters, so a name in any encoding survives intact.
fn file_uri(path: &str) -> String {
    let mut uri = String::from("file://");
    for byte in path.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                uri.push(*byte as char);
            }
            _ => {
                let _ = write!(uri, "%{byte:02X}");
            }
        }
    }
    uri
}
