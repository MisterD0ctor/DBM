//! Whether a person is watching this run, or the harness is driving it. Read
//! from the environment once, since the answer can't change while it runs.

use std::sync::LazyLock;

/// Whether any exercise is armed: a `DBM_*_TEST` switch of any kind.
///
/// Any at all, rather than a list of the ones that seem to matter, which goes
/// wrong: `DBM_KEY_TEST` presses B, and B is the ambience switch.
pub fn armed() -> bool {
    static ARMED: LazyLock<bool> = LazyLock::new(|| {
        std::env::vars_os().any(|(k, _)| {
            let k = k.to_string_lossy();
            k.starts_with("DBM_") && k.ends_with("_TEST")
        })
    });
    *ARMED
}

/// Whether the player is being driven or photographed rather than watched:
/// an exercise, a showcase or a capture. Each of them plays a film, and
/// nobody is listening to it.
pub fn unattended() -> bool {
    static UNATTENDED: LazyLock<bool> = LazyLock::new(|| {
        armed()
            || std::env::var_os("DBM_SHOWCASE").is_some()
            || std::env::var_os("DBM_CAPTURE").is_some()
    });
    *UNATTENDED
}
