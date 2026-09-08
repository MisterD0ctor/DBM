//! Audio-device recovery.
//!
//! When the device backing mpv's audio output disappears mid-playback — a
//! Bluetooth headset going out of range, being turned off, or switching to
//! another host — mpv fails to re-open it, drops the audio track
//! (`error_on_track`, which is why `aid` reads back as `"no"` afterwards)
//! and plays on silently. Nothing re-selects the track when the device
//! returns, so the app stays mute until it's restarted.
//!
//! mpv only refreshes `audio-device-list` while something is observing it —
//! observing is what starts its hotplug monitor — so we observe it and treat
//! "a device appeared that wasn't there a moment ago" as the cue to rebuild
//! the audio chain. A device that just reconnected is usually also made the
//! default by the OS, so re-opening is the right move even when the current
//! output is still technically alive on some other device.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{info, warn};
use shared::{SpecialTrack, TrackKind, TrackSelection};
use tauri::{AppHandle, Listener, Manager};

use crate::mpv::MpvPlayer;

/// A reconnecting Bluetooth device churns the device list several times
/// while it negotiates. Wait for quiet before touching the audio chain —
/// both to collapse the churn into one reload and to give the endpoint a
/// moment to become usable.
const SETTLE: Duration = Duration::from_millis(1200);

#[derive(Default)]
pub struct AudioWatchdog {
    /// Device names from the last list we saw. `None` until the first list
    /// arrives: that one is the baseline, never a reconnect.
    seen: Mutex<Option<BTreeSet<String>>>,
    /// The last audio track id mpv reported as actually selected. Once mpv
    /// has given up on a device it reports `aid` as `"no"`, so at recovery
    /// time there's nothing left to read out of mpv — we have to have kept
    /// it ourselves.
    last_track: Mutex<Option<String>>,
    /// Set when the *user* turns audio off, so that a device reconnect
    /// doesn't helpfully turn it back on behind their back.
    user_disabled: AtomicBool,
    /// Bumped on every device-list change. A settle timer only acts if it
    /// still owns the latest value, so the last change of a burst wins.
    generation: AtomicU64,
}

impl AudioWatchdog {
    /// Record an explicit audio-track choice from the UI. Selecting "no"
    /// here is the one case where silence is intentional.
    pub fn note_user_selection(&self, selection: TrackSelection) {
        let disabled = matches!(selection, TrackSelection::Special(SpecialTrack::No));
        self.user_disabled.store(disabled, Ordering::Relaxed);
    }
}

/// Remember which audio track mpv is actually playing, so recovery knows
/// what to restore after mpv has dropped it.
pub fn install_listener(app: &AppHandle) {
    let app = app.clone();
    app.clone().listen("mpv://property", move |event| {
        let Ok(prop) = serde_json::from_str::<shared::MpvProperty>(event.payload()) else {
            return;
        };
        // Only real track ids are worth keeping: "no" is either the failure
        // we're recovering from or the user's own choice, and neither is a
        // useful restore target.
        if let shared::MpvProperty::Aid(Some(id)) = prop {
            if id.parse::<u32>().is_ok() {
                if let Some(watchdog) = app.try_state::<AudioWatchdog>() {
                    *watchdog.last_track.lock().unwrap() = Some(id);
                }
            }
        }
    });
}

/// Handle an `audio-device-list` change. Called from the mpv event dispatch
/// rather than the typed frontend stream — the list is a backend concern.
pub fn on_device_list_change(app: &AppHandle, data: &serde_json::Value) {
    let Some(watchdog) = app.try_state::<AudioWatchdog>() else {
        return;
    };

    let names = device_names(data);
    if names.is_empty() {
        // Unparseable or empty — don't let it clobber the baseline, or the
        // next real list would look like every device just appeared.
        return;
    }

    let appeared: Vec<String> = {
        let mut seen = watchdog.seen.lock().unwrap();
        match seen.replace(names.clone()) {
            // First list of the session: baseline only.
            None => return,
            Some(previous) => names.difference(&previous).cloned().collect(),
        }
    };
    if appeared.is_empty() {
        // Something went away. Nothing to re-open, and mpv has already lost
        // it — the reconnect is what we act on.
        return;
    }
    info!("audio device appeared: {}", appeared.join(", "));

    let generation = watchdog.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE);
        let Some(watchdog) = app.try_state::<AudioWatchdog>() else {
            return;
        };
        if watchdog.generation.load(Ordering::SeqCst) != generation {
            return; // Superseded mid-burst; the newer timer does the work.
        }
        reopen_audio(&app);
    });
}

/// Rebuild mpv's audio chain, AO included.
fn reopen_audio(app: &AppHandle) {
    let (Some(player), Some(watchdog)) = (
        app.try_state::<Arc<MpvPlayer>>(),
        app.try_state::<AudioWatchdog>(),
    ) else {
        return;
    };

    if !player.is_file_loaded() || watchdog.user_disabled.load(Ordering::Relaxed) {
        return;
    }
    // A file with no audio at all has nothing to restore, and a stale id
    // from a previous file would just make mpv reject the write.
    let has_audio = player
        .tracks()
        .map(|tracks| tracks.iter().any(|t| t.kind == TrackKind::Audio))
        .unwrap_or(false);
    if !has_audio {
        return;
    }

    let current = player
        .get_property("aid", "string")
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned));
    let target = match current {
        // Still selected: mpv kept the track, the output underneath it is
        // what needs re-opening.
        Some(id) if id.parse::<u32>().is_ok() => id,
        // Dropped by mpv when the device died — restore what it was playing.
        _ => match watchdog.last_track.lock().unwrap().clone() {
            Some(id) => id,
            None => return,
        },
    };

    // Deselect first. Writing back the id mpv already holds is discarded as
    // a no-op, and it's the write that reruns the chain and re-opens the AO.
    if let Err(e) = player.set_property_raw("aid", "no") {
        warn!("audio recovery: could not drop the audio track: {e}");
        return;
    }
    if let Err(e) = player.set_property_raw("aid", &target) {
        // Leaving audio off would be worse than the state we started in, so
        // let mpv pick a track rather than giving up here.
        warn!("audio recovery: could not reselect track {target}: {e}");
        if let Err(e) = player.set_property_raw("aid", "auto") {
            warn!("audio recovery: fallback to auto failed too: {e}");
        }
        return;
    }
    info!("audio recovery: reselected audio track {target}");
}

/// Pull the device names out of an `audio-device-list` payload. The wrapper
/// hands node properties over as JSON; `tracks()` shows it can also arrive
/// as a JSON string, so accept both shapes.
fn device_names(data: &serde_json::Value) -> BTreeSet<String> {
    let owned;
    let list = match data {
        serde_json::Value::String(s) => match serde_json::from_str(s) {
            Ok(v) => {
                owned = v;
                &owned
            }
            Err(_) => return BTreeSet::new(),
        },
        other => other,
    };

    list.as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.get("name").and_then(|n| n.as_str()))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
