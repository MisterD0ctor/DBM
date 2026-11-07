//! What each operating system has to be asked for in its own way.
//!
//! Keeping the display awake, hiding the pointer, a native file dialog, files
//! dropped on the window, frames during a Windows resize, minimizing, the
//! window's round corners, whether the desktop asks for less motion, the
//! media keys (SMTC on Windows, MPRIS on Linux), where the files a Flatpak is
//! handed really are, and handing freed memory back. Each module covers both platforms, even where
//! one does nothing, so nothing outside this folder needs a `cfg`.

pub mod awake;
pub mod cursor;
pub mod dialog;
pub mod dropped;
pub mod media_keys;
pub mod memory;
pub mod modal_loop;
pub mod motion;
pub mod window;

/// Why a platform hook could not be set up.
#[derive(Debug)]
pub enum Error {
    /// A hook or platform call with no typed error of its own.
    #[cfg_attr(not(windows), allow(dead_code))]
    Message(String),
    /// A Windows API call refused.
    #[cfg(windows)]
    Windows(windows::core::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            #[cfg(windows)]
            Self::Windows(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Message(_) => None,
            #[cfg(windows)]
            Self::Windows(source) => Some(source),
        }
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for Error {
    fn from(source: windows::core::Error) -> Self {
        Self::Windows(source)
    }
}
