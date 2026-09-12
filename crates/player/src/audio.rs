//! Audio-device recovery.
//!
//! When the device backing mpv's audio output disappears mid-playback — a
//! Bluetooth headset going out of range, being switched off, or handed to
//! another host — mpv cannot re-open it, drops the audio track and **plays on
//! silently**. `aid` reads back as `no` afterwards. Nothing re-selects it when
//! the device returns, so the player stays mute until it is restarted, and
//! nothing on screen says why.
//!
//! mpv only refreshes `audio-device-list` while something is observing it —
//! observing is what starts its hotplug monitor — so this observes it and
//! treats "a device is here that was not here a moment ago" as the cue to
//! rebuild the audio chain. A device that has just reconnected is usually also
//! made the system default, so re-opening is the right move even when the
//! current output is still technically alive on something else.
//!
//! Everything here runs on the UI thread except the rebuild itself, which
//! reads the track list and writes properties and therefore belongs on the
//! worker — see [`crate::worker`].

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use crate::mpv::{Event, Mpv, Value, FORMAT_STRING};
use crate::tracks::{self, TrackKind};
use crate::worker::{Completion, Worker};

/// How long the device list must stay still before the audio chain is touched.
///
/// A reconnecting Bluetooth device churns the list several times while it
/// negotiates, and the endpoint is not usable for the first moment of that.
/// Waiting for quiet collapses the churn into one reload and gives the device
/// time to become real.
const SETTLE: Duration = Duration::from_millis(1200);

/// Start mpv's hotplug monitor.
///
/// Separate from [`crate::state::OBSERVED`] on purpose: that list is the
/// properties the interface binds to, and this one is never displayed. It is
/// observed for the side effect — mpv does not track devices otherwise — and
/// read only here.
pub fn observe(mpv: &Mpv) {
    if let Err(e) = mpv.observe("audio-device-list", FORMAT_STRING) {
        eprintln!(
            "dbm: cannot observe audio-device-list ({e}); audio will not recover \
             from a device disappearing"
        );
    }
}

pub struct Watchdog {
    worker: Rc<Worker>,
    /// Device names from the last list seen. `None` until the first list
    /// arrives: that one is the baseline, never a reconnection.
    seen: RefCell<Option<BTreeSet<String>>>,
    /// The last audio track mpv reported as genuinely selected. Once mpv has
    /// given up on a device it reports `aid` as `no`, so by recovery time
    /// there is nothing left to read out of mpv — it has to have been kept.
    last_track: RefCell<Option<String>>,
    /// Set when the *user* turns audio off, so a device appearing does not
    /// helpfully turn it back on behind their back.
    user_disabled: Cell<bool>,
    /// Restarted on every device-list change, which is the coalescing: only
    /// the last change of a burst ever fires.
    settle: slint::Timer,
    trace: bool,
}

impl Watchdog {
    pub fn new(worker: Rc<Worker>) -> Rc<Self> {
        Rc::new(Self {
            worker,
            seen: RefCell::new(None),
            last_track: RefCell::new(None),
            user_disabled: Cell::new(false),
            settle: slint::Timer::default(),
            trace: std::env::var_os("DBM_AUDIO_TEST").is_some()
                || std::env::var_os("DBM_TRACE").is_some(),
        })
    }

    /// Route one mpv event. Called for every event, from the same drain that
    /// feeds the mirrored state.
    pub fn on_event(self: &Rc<Self>, event: &Event) {
        let Event::Property { name, value } = event else {
            return;
        };
        match (name.as_str(), value) {
            // Only real ids are worth keeping: `no` is either the failure
            // being recovered from or the user's own choice, and neither is a
            // useful thing to restore.
            ("aid", Value::Str(id)) if id.parse::<u32>().is_ok() => {
                *self.last_track.borrow_mut() = Some(id.clone());
            }
            ("audio-device-list", Value::Str(payload)) => self.devices_changed(payload),
            _ => {}
        }
    }

    /// Record an explicit choice from the interface. Selecting nothing here is
    /// the one case where silence is what was asked for.
    pub fn note_user_selection(&self, track: Option<i64>) {
        self.user_disabled.set(track.is_none());
    }

    fn devices_changed(self: &Rc<Self>, payload: &str) {
        let names = device_names(payload);
        if names.is_empty() {
            // Unparseable, or genuinely empty. Either way it must not become
            // the baseline, or the next real list would look like every
            // device in the machine had just appeared.
            unexpected_shape(payload);
            return;
        }
        if self.trace {
            eprintln!("dbm: audio devices: {}", names.iter().cloned().collect::<Vec<_>>().join(", "));
        }

        let appeared: Vec<String> = match self.seen.replace(Some(names.clone())) {
            // First list of the session: a baseline, not an event.
            None => return,
            Some(previous) => names.difference(&previous).cloned().collect(),
        };
        if appeared.is_empty() {
            // Something went away. There is nothing to re-open — mpv has
            // already lost it — so the reconnection is what gets acted on.
            return;
        }
        eprintln!("dbm: audio device appeared: {}", appeared.join(", "));

        let me = Rc::downgrade(self);
        // Restarting the timer cancels the pending fire, so a burst of
        // changes costs one recovery rather than one per change.
        self.settle.start(slint::TimerMode::SingleShot, SETTLE, move || {
            if let Some(me) = me.upgrade() {
                me.recover();
            }
        });
    }

    fn recover(&self) {
        if self.user_disabled.get() {
            return;
        }
        let remembered = self.last_track.borrow().clone();
        // Reads the track list and writes properties, both of which block on
        // mpv's core lock. Returns a notice when it actually repaired
        // something, so the interface can say so.
        self.worker
            .submit(move |mpv| reopen(mpv, remembered.as_deref()).map(Completion::Notice));
    }
}

/// Rebuild mpv's audio chain, output included.
///
/// **Worker thread only.** Returns what to tell the person watching, or
/// `None` when there was nothing to repair.
fn reopen(mpv: &Mpv, remembered: Option<&str>) -> Option<String> {
    let tracks = tracks::read_tracks(mpv);
    let audio: Vec<i64> = tracks::of_kind(&tracks, TrackKind::Audio)
        .iter()
        .map(|t| t.id)
        .collect();
    // Nothing loaded, or a file with no audio at all: nothing to restore, and
    // an id from a previous file would only be rejected.
    if audio.is_empty() {
        return None;
    }

    let current = mpv.get_property("aid");
    // Whether the person watching actually lost sound. mpv drops the track
    // when the device dies, and *that* is the case worth mentioning; a device
    // appearing while audio played fine the whole time repairs the output
    // underneath and nobody heard a thing, so nobody needs telling.
    let was_silent = !current
        .as_deref()
        .is_some_and(|id| id.parse::<u32>().is_ok());
    let target = match current.as_deref() {
        // Still selected, so mpv kept the track and it is the output
        // underneath that needs re-opening.
        Some(id) if id.parse::<u32>().is_ok() => id.to_owned(),
        // Dropped by mpv when the device died — put back what was playing,
        // provided this file still has it.
        _ => {
            let usable = remembered.filter(|id| {
                id.parse::<i64>().is_ok_and(|n| audio.contains(&n))
            });
            match usable {
                Some(id) => id.to_owned(),
                // No memory of a track playing, and mpv reports none
                // selected. Choosing one here would turn audio on for someone
                // who had turned it off — including off in an earlier session,
                // which watch-later restores and `user_disabled` cannot know
                // about. Silence that was asked for is not a fault to fix.
                None => return None,
            }
        }
    };

    // Deselect first. Writing back the id mpv already holds is discarded as a
    // no-op, and it is the write that reruns the chain and re-opens the
    // output — which is the entire point of coming here.
    if let Err(e) = mpv.set_property("aid", "no") {
        eprintln!("dbm: audio recovery: could not drop the audio track: {e}");
        return None;
    }
    if let Err(e) = mpv.set_property("aid", &target) {
        eprintln!("dbm: audio recovery: could not reselect track {target}: {e}");
        // Leaving audio off would be worse than the state this started in, so
        // let mpv pick rather than giving up here.
        if let Err(e) = mpv.set_property("aid", "auto") {
            eprintln!("dbm: audio recovery: falling back to auto failed too: {e}");
        }
        return None;
    }
    eprintln!("dbm: audio recovery: reselected audio track {target}");
    // Names what happened rather than what was done to fix it: "reselected
    // audio track 1" is the log's business, not the viewer's.
    was_silent.then(|| "Sound is back".to_string())
}

/// Said once. A list that cannot be read means recovery will never fire, which
/// is worth knowing; saying so on every hotplug would bury everything else.
fn unexpected_shape(payload: &str) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let sample: String = payload.chars().take(120).collect();
        eprintln!("dbm: no device names in audio-device-list; audio recovery is off ({sample})");
    });
}

/// The `name` of every device in an `audio-device-list` payload.
///
/// mpv renders node-shaped properties as JSON when they are observed as
/// strings, so this is `[{"name":…,"description":…}, …]`. Rather than take a
/// JSON dependency for one property, the strings are walked in order: a string
/// followed by a colon is a key, and the string after the key `name` is a
/// device. Anything that is not that shape yields nothing, which the caller
/// treats as "do not act".
///
/// Escapes are decoded only as far as equality needs — `\uXXXX` is left as it
/// was written. Two readings of the same device always produce the same text,
/// which is all a set comparison asks for.
fn device_names(payload: &str) -> BTreeSet<String> {
    let bytes = payload.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    // Whether the string about to be read is the value of a `name` key.
    let mut expecting = false;

    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let Some((text, end)) = read_string(bytes, i) else {
            // Unterminated: the payload is truncated or not JSON at all.
            break;
        };
        i = end;
        // A colon after it makes it a key rather than a value.
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if bytes.get(j) == Some(&b':') {
            expecting = text == "name";
        } else if expecting {
            out.insert(text);
            expecting = false;
        }
    }
    out
}

/// Read the JSON string starting at the quote at `start`. Returns its contents
/// and the index just past the closing quote.
fn read_string(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut out = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                return Some((String::from_utf8_lossy(&out).into_owned(), i + 1));
            }
            b'\\' => {
                let next = *bytes.get(i + 1)?;
                match next {
                    b'"' => out.push(b'"'),
                    b'\\' => out.push(b'\\'),
                    b'/' => out.push(b'/'),
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0c),
                    // Left as written, including `\uXXXX`: it only has to be
                    // consistent, not readable.
                    other => {
                        out.push(b'\\');
                        out.push(other);
                    }
                }
                i += 2;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(payload: &str) -> Vec<String> {
        device_names(payload).into_iter().collect()
    }

    #[test]
    fn reads_a_device_list() {
        let payload = r#"[{"name":"auto","description":"Autoselect device"},
            {"name":"wasapi/{0.0.0.00000000}.{1c2b}","description":"Speakers (Realtek)"}]"#;
        assert_eq!(
            names(payload),
            vec![
                "auto".to_string(),
                "wasapi/{0.0.0.00000000}.{1c2b}".to_string()
            ]
        );
    }

    #[test]
    fn a_description_that_says_name_is_not_a_device() {
        // The word appears as a value, not a key, so it must not be taken for
        // one — and the device whose description it is still has to be read.
        let payload = r#"[{"description":"name","name":"auto"}]"#;
        assert_eq!(names(payload), vec!["auto".to_string()]);
    }

    #[test]
    fn decodes_the_escapes_a_device_name_can_carry() {
        let payload = r#"[{"name":"wasapi/a\\b\"c"}]"#;
        assert_eq!(names(payload), vec!["wasapi/a\\b\"c".to_string()]);
    }

    #[test]
    fn a_quoted_colon_inside_a_name_does_not_make_a_key() {
        let payload = r#"[{"name":"odd\"name\":","description":"x"}]"#;
        assert_eq!(names(payload), vec!["odd\"name\":".to_string()]);
    }

    #[test]
    fn anything_that_is_not_a_list_yields_nothing() {
        for payload in ["", "[]", "null", "not json at all", r#"[{"name":"#] {
            assert!(names(payload).is_empty(), "{payload:?}");
        }
    }

    #[test]
    fn a_device_appearing_is_the_difference_between_two_lists() {
        let before = device_names(r#"[{"name":"auto"},{"name":"speakers"}]"#);
        let after = device_names(r#"[{"name":"auto"},{"name":"speakers"},{"name":"buds"}]"#);
        assert_eq!(
            after.difference(&before).cloned().collect::<Vec<_>>(),
            vec!["buds".to_string()]
        );
        // Read the other way round it is a device going away, which is not
        // something to act on: nothing new is there.
        assert!(before.difference(&after).next().is_none());
    }
}
