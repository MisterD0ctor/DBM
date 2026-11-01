//! Threads of the player's own, each named so a debugger's thread list says
//! which job it is doing.

use std::thread::JoinHandle;

/// `std::thread::spawn`, under a name. Linux keeps only its first 15 bytes.
///
/// Panics where `std::thread::spawn` does: when the system has no thread to
/// give.
pub fn spawn<F, T>(name: &str, f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(f)
        .expect("failed to spawn thread")
}
