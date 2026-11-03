//! Keeping the display on while a video plays.
//!
//! mpv does this itself (`stop-screensaver`) when it owns the window, but
//! here it renders into ours through the render API and never learns there
//! is one.
//!
//! Only while playing: paused, finished or empty, the machine may idle as it
//! would with anything else open. The request is made from the frame path,
//! only when the answer changes. Windows takes it per thread, and the frame
//! path is always the UI thread. Linux takes it over D-Bus, which can't be
//! waited on there, so a thread of its own makes the calls.

use crate::playback::state::PlayerState;

#[derive(Default)]
pub struct Awake {
    held: bool,
    #[cfg(not(windows))]
    bus: bus::Inhibitor,
}

impl Awake {
    /// Hold the display on or let it go, to match what is playing.
    pub fn follow(&mut self, player: &PlayerState) {
        let playing = player.path.is_some() && !player.paused && !player.eof_reached;
        if playing != self.held {
            self.held = playing;
            self.request(playing);
        }
    }

    #[cfg(windows)]
    fn request(&self, on: bool) {
        use windows::Win32::System::Power::{
            ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
        };
        let flags = if on {
            ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        // Returns the previous state, or zero when Windows refused.
        if unsafe { SetThreadExecutionState(flags) }.0 == 0 {
            log::warn!(
                "Windows would not {} the display",
                if on { "hold" } else { "release" }
            );
        }
    }

    #[cfg(not(windows))]
    fn request(&mut self, on: bool) {
        self.bus.request(on);
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        if self.held {
            self.request(false);
        }
    }
}

/// Linux: the desktop is asked over D-Bus, through the portal, which a
/// Flatpak can always reach, or else the older `org.freedesktop.ScreenSaver`
/// that GNOME and Plasma also answer outside one. Either releases what a
/// program held once it leaves the bus, so a player that quits or crashes
/// holding the display lets it go.
#[cfg(not(windows))]
mod bus {
    use std::collections::HashMap;
    use std::sync::mpsc::{self, Receiver, Sender};

    use zbus::blocking::Connection;
    use zbus::zvariant::{OwnedObjectPath, Value};

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const SCREENSAVER: &str = "org.freedesktop.ScreenSaver";

    /// The portal's flag for keeping the session from going idle, which is
    /// what dims, blanks and locks the screen, and suspends on a timer. Not
    /// suspend itself, which would stand in the way of a person putting the
    /// machine to sleep on purpose.
    const IDLE: u32 = 8;

    /// The thread that asks, started the first time something plays.
    #[derive(Default)]
    pub struct Inhibitor {
        asks: Option<Sender<bool>>,
    }

    impl Inhibitor {
        pub fn request(&mut self, on: bool) {
            let asks = self.asks.get_or_insert_with(|| {
                let (asks, heard) = mpsc::channel();
                // Ends when the sender drops with the `Awake`, releasing
                // anything still held.
                if let Err(e) = std::thread::Builder::new()
                    .name("dbm-awake".into())
                    .spawn(move || serve(&heard))
                {
                    log::warn!("no thread to keep the display on ({e})");
                }
                asks
            });
            let _ = asks.send(on);
        }
    }

    /// What is held, and how to let it go.
    enum Held {
        /// The portal's request object, closed to release it.
        Portal(OwnedObjectPath),
        /// The screensaver's cookie.
        Cookie(u32),
    }

    fn serve(asks: &Receiver<bool>) {
        let bus = match Connection::session() {
            Ok(bus) => bus,
            Err(e) => {
                log::warn!("no session bus to keep the display on: {e}");
                return;
            }
        };
        let mut held = None;
        for on in asks {
            match (on, held.take()) {
                (true, None) => held = hold(&bus),
                (false, Some(was)) => release(&bus, was),
                (_, still) => held = still,
            }
        }
        if let Some(was) = held {
            release(&bus, was);
        }
    }

    fn hold(bus: &Connection) -> Option<Held> {
        let reason = dbm_lang::tr("Playing a video");
        let options = HashMap::from([("reason", Value::from(reason))]);
        let portal = bus
            .call_method(
                Some(PORTAL),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Inhibit"),
                "Inhibit",
                &("", IDLE, options),
            )
            .and_then(|reply| reply.body().deserialize::<OwnedObjectPath>());
        let portal_error = match portal {
            Ok(request) => return Some(Held::Portal(request)),
            Err(e) => e,
        };
        let screensaver = bus
            .call_method(
                Some(SCREENSAVER),
                "/org/freedesktop/ScreenSaver",
                Some(SCREENSAVER),
                "Inhibit",
                &(crate::platform::media_keys::mpris::APP_ID, reason),
            )
            .and_then(|reply| reply.body().deserialize::<u32>());
        match screensaver {
            Ok(cookie) => Some(Held::Cookie(cookie)),
            Err(e) => {
                log::warn!("could not keep the display on: {portal_error}; {e}");
                None
            }
        }
    }

    fn release(bus: &Connection, held: Held) {
        let released = match &held {
            Held::Portal(request) => bus.call_method(
                Some(PORTAL),
                request,
                Some("org.freedesktop.portal.Request"),
                "Close",
                &(),
            ),
            Held::Cookie(cookie) => bus.call_method(
                Some(SCREENSAVER),
                "/org/freedesktop/ScreenSaver",
                Some(SCREENSAVER),
                "UnInhibit",
                &(*cookie,),
            ),
        };
        if let Err(e) = released {
            log::warn!("could not let the display go: {e}");
        }
    }
}
