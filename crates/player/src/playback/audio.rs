//! Audio-device recovery.
//!
//! When the device backing mpv's audio output disappears mid-playback (a
//! Bluetooth headset out of range, switched off, or handed to another host),
//! mpv can't re-open it, drops the audio track and **plays on silently**, with
//! `aid` reading `no`. Nothing re-selects it when the device returns, so
//! without this the player stays mute until restarted.
//!
//! mpv only refreshes `audio-device-list` while something is observing it —
//! observing is what starts its hotplug monitor — so this observes it and
//! treats "a device is here that was not here a moment ago" as the cue to
//! rebuild the audio chain. A device that has just reconnected is usually also
//! made the system default, so re-opening is the right move even when the
//! current output is still technically alive on something else.
//!
//! Everything here runs on the UI thread except the rebuild, which reads the
//! track list, so belongs on the worker (see [`crate::worker`]).

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use dbm_mpv::Reader;
use dbm_mpv::prop::{self, TrackChoice};
use dbm_mpv::tracks::{self, AudioDevice, TrackId, TrackKind};

use crate::playback::state::Observed;
use crate::worker::Notice;

/// How long the device list must stay still before the audio chain is touched.
///
/// A reconnecting Bluetooth device churns the list several times while it
/// negotiates, and the endpoint is not usable for the first moment of that.
/// Waiting for quiet collapses the churn into one reload and gives the device
/// time to become real.
const SETTLE: Duration = Duration::from_millis(1200);

pub struct Watchdog {
    /// Runs [`reopen`] where blocking is allowed, with the track to put
    /// back, and says what it returns.
    reopen_elsewhere: Box<dyn Fn(Option<TrackId>)>,
    /// Device names from the last list seen. `None` until the first list
    /// arrives: that one is the baseline, never a reconnection.
    seen: RefCell<Option<BTreeSet<String>>>,
    /// The last audio track mpv reported as genuinely selected, kept because
    /// once mpv gives up on a device it reports `aid` as `no`.
    last_track: Cell<Option<TrackId>>,
    /// Set when the *user* turns audio off, so a device appearing does not
    /// helpfully turn it back on behind their back.
    user_disabled: Cell<bool>,
    /// Restarted on every device-list change, which is the coalescing: only
    /// the last change of a burst ever fires.
    settle: slint::Timer,
    trace: bool,
}

impl Watchdog {
    pub fn new(reopen_elsewhere: impl Fn(Option<TrackId>) + 'static) -> Rc<Self> {
        Rc::new(Self {
            reopen_elsewhere: Box::new(reopen_elsewhere),
            seen: RefCell::new(None),
            last_track: Cell::new(None),
            user_disabled: Cell::new(false),
            settle: slint::Timer::default(),
            trace: (cfg!(feature = "harness") && std::env::var_os("DBM_AUDIO_TEST").is_some())
                || crate::diagnostics::trace(),
        })
    }

    /// Follow one observed property's change, from the same drain that feeds
    /// the mirrored state.
    pub fn observe(self: &Rc<Self>, change: &Observed) {
        match change {
            // Only real ids are worth keeping, since `no` is either the
            // failure being recovered from or the user's own choice.
            Observed::Aid(choice) => {
                if let Some(id) = choice.and_then(TrackChoice::id) {
                    self.last_track.set(Some(id));
                }
            }
            Observed::AudioDeviceList(devices) => {
                self.devices_changed(devices.as_deref().unwrap_or_default());
            }
            _ => {}
        }
    }

    /// Record an explicit choice from the interface. Selecting nothing here is
    /// the one case where silence is what was asked for.
    pub fn note_user_selection(&self, track: Option<TrackId>) {
        self.user_disabled.set(track.is_none());
    }

    fn devices_changed(self: &Rc<Self>, devices: &[AudioDevice]) {
        let names: BTreeSet<String> = devices.iter().map(|d| d.name.clone()).collect();
        if names.is_empty() {
            // Unreadable, or genuinely empty. Either way it must not become
            // the baseline, or the next real list would look like every
            // device in the machine had just appeared.
            unexpected_shape();
            return;
        }
        if self.trace {
            log::info!(
                "audio devices: {}",
                names.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }

        let appeared: Vec<String> = match self.seen.replace(Some(names.clone())) {
            // First list of the session: a baseline, not an event.
            None => return,
            Some(previous) => names.difference(&previous).cloned().collect(),
        };
        if appeared.is_empty() {
            // Something went away, which mpv has already lost, so only a
            // reconnection is acted on.
            return;
        }
        log::info!("audio device appeared: {}", appeared.join(", "));

        let me = Rc::downgrade(self);
        // Restarting the timer cancels the pending fire, so a burst of
        // changes costs one recovery rather than one per change.
        self.settle
            .start(slint::TimerMode::SingleShot, SETTLE, move || {
                if let Some(me) = me.upgrade() {
                    me.recover();
                }
            });
    }

    fn recover(&self) {
        if self.user_disabled.get() {
            return;
        }
        // Reads the track list, which blocks on mpv's core lock. Returns a
        // notice when it actually repaired something, so the interface can
        // say so.
        (self.reopen_elsewhere)(self.last_track.get());
    }
}

/// Rebuild mpv's audio chain, output included.
///
/// **Blocking**: reads the track list. Returns what to tell the person
/// watching, or `None` when there was nothing to repair.
pub fn reopen(mpv: &Reader, remembered: Option<TrackId>) -> Option<Notice> {
    let tracks = mpv.get(prop::TRACK_LIST).unwrap_or_default();
    let audio: Vec<TrackId> = tracks::of_kind(&tracks, TrackKind::Audio)
        .iter()
        .map(|t| t.id)
        .collect();
    // Nothing loaded, or a file with no audio at all: nothing to restore, and
    // an id from a previous file would only be rejected.
    if audio.is_empty() {
        return None;
    }

    let current = mpv.get(prop::AID).and_then(TrackChoice::id);
    // Whether the person watching actually lost sound, which is when mpv
    // dropped the track. A device appearing while audio played fine repairs
    // the output unheard, and nobody needs telling.
    let was_silent = current.is_none();
    let target = match current {
        // Still selected, so mpv kept the track and it is the output
        // underneath that needs re-opening.
        Some(id) => id,
        // Dropped by mpv when the device died: put back what was playing,
        // provided this file still has it. With no track remembered and none
        // selected, nothing, since the person may have turned audio off,
        // perhaps in an earlier session that watch-later restores and
        // `user_disabled` can't know about.
        None => remembered.filter(|id| audio.contains(id))?,
    };

    // Deselect first, because writing back the id mpv holds is discarded as
    // a no-op, and the write is what reopens the output. mpv runs the two in
    // order, and back to back they still reopen it.
    for track in [TrackChoice::Off, TrackChoice::Id(target)] {
        if let Err(e) = mpv.set(prop::AID, track) {
            log::warn!("audio recovery: could not set aid={track}: {e}");
            return None;
        }
    }
    log::info!("audio recovery: reselected audio track {target}");
    // Names what happened rather than the fix, which is the log's business.
    was_silent.then_some(Notice::SoundBack)
}

/// Said once: an unreadable list means recovery will never fire, but saying
/// so on every hotplug would bury everything else.
fn unexpected_shape() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| log::warn!("no devices in audio-device-list; audio recovery is off"));
}
