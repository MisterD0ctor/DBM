//! Whether the desktop asks for less motion.
//!
//! The settings' Animations switch is the person's say in this player; this
//! is their say everywhere, set once in the system, and either one asking for
//! stillness is enough (see `Motion.enabled`). Read off the frame path, since
//! on Linux it is a question over D-Bus.
//!
//! On Linux it comes from the desktop portal, which a Flatpak can reach, as
//! GNOME's "enable-animations" or KDE's animation speed at its slowest, and
//! a change is heard as it happens. On Windows it is "Show animations in
//! Windows", read when the player starts.

/// Call `still` with whether the desktop asks for less motion, now, and
/// again whenever that changes. It may be called on another thread.
#[cfg(not(windows))]
pub fn watch(still: impl Fn(bool) + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("dbm-motion".into())
        .spawn(move || {
            if let Err(e) = listen(&still) {
                if crate::diagnostics::trace() {
                    eprintln!("dbm: the desktop's motion setting: {e}");
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("dbm: could not watch the desktop's motion setting: {e}");
    }
}

#[cfg(not(windows))]
fn listen(still: &impl Fn(bool)) -> zbus::Result<()> {
    use zbus::zvariant::OwnedValue;

    let conn = zbus::blocking::Connection::session()?;
    let portal = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )?;
    // Subscribed before the first read, so a change between the two isn't
    // missed.
    let changes = portal.receive_signal("SettingChanged")?;
    for (namespace, key) in KEYS {
        let read: zbus::Result<OwnedValue> = portal
            .call("ReadOne", &(namespace, key))
            .or_else(|_| portal.call("Read", &(namespace, key)));
        if let Some(asked) = read.ok().and_then(|v| asks_for_stillness(key, &v)) {
            still(asked);
            break;
        }
    }
    for message in changes {
        let Ok((namespace, key, value)) =
            message.body().deserialize::<(String, String, OwnedValue)>()
        else {
            continue;
        };
        let known = KEYS.iter().any(|(n, k)| *n == namespace && *k == key);
        if let Some(asked) = asks_for_stillness(&key, &value).filter(|_| known) {
            still(asked);
        }
    }
    Ok(())
}

/// Where each desktop keeps it, in the portal's terms.
#[cfg(not(windows))]
const KEYS: [(&str, &str); 2] = [
    ("org.gnome.desktop.interface", "enable-animations"),
    ("org.kde.kdeglobals.KDE", "AnimationDurationFactor"),
];

/// Whether a value read for one of `KEYS` asks for stillness. The older
/// portal's `Read` wraps the value in a second variant.
#[cfg(not(windows))]
fn asks_for_stillness(key: &str, value: &zbus::zvariant::Value) -> Option<bool> {
    use zbus::zvariant::Value;
    match (key, value) {
        (_, Value::Value(inner)) => asks_for_stillness(key, inner),
        ("enable-animations", Value::Bool(on)) => Some(!on),
        ("AnimationDurationFactor", Value::F64(factor)) => Some(*factor <= 0.0),
        _ => None,
    }
}

#[cfg(windows)]
pub fn watch(still: impl Fn(bool) + Send + 'static) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };

    // A BOOL, which is an i32.
    let mut on: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL through the pointer,
    // which points at one, live for the call.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::from_mut(&mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if read.is_ok() {
        still(on == 0);
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    #[test]
    fn each_desktop_says_it_its_own_way() {
        assert_eq!(
            asks_for_stillness("enable-animations", &Value::Bool(false)),
            Some(true)
        );
        assert_eq!(
            asks_for_stillness("enable-animations", &Value::Bool(true)),
            Some(false)
        );
        assert_eq!(
            asks_for_stillness("AnimationDurationFactor", &Value::F64(0.0)),
            Some(true)
        );
        assert_eq!(
            asks_for_stillness("AnimationDurationFactor", &Value::F64(1.0)),
            Some(false)
        );
    }

    #[test]
    fn the_older_portal_wraps_it_twice() {
        let wrapped = Value::Value(Box::new(Value::Bool(false)));
        assert_eq!(
            asks_for_stillness("enable-animations", &wrapped),
            Some(true)
        );
    }
}
