//! Handing memory the player has freed back to the system.
//!
//! glibc keeps what a thread frees for that thread's pool to reuse, and once
//! a block larger than its threshold has been freed it raises the threshold
//! to match, past which it returns nothing on its own. A file's decoders and
//! thumbnail leave their peak behind that way, in whichever pool they drew
//! on, so without asking the player would stand a little larger after every
//! episode.
//!
//! Windows' heap has no such call, and gives large free blocks back itself;
//! everything here compiles away there.

/// Return the pages the allocator holds free, all of them, in every pool.
/// A few milliseconds, so it belongs off the UI thread, and once a file,
/// when the last one's decoders are gone.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn give_back() {
    // SAFETY: takes no pointers; glibc walks and trims only its own pools.
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn give_back() {}
