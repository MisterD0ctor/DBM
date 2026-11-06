//! What goes wrong talking to mpv, and mpv's own reasons.

use std::ffi::c_int;
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load libmpv: {0}")]
    Load(String),
    #[error("mpv_create returned null")]
    Create,
    /// An mpv API call refused, for mpv's reason.
    #[error("{call} failed: {code}")]
    Api { call: &'static str, code: ErrorCode },
    #[error("string contained an interior NUL")]
    Nul,
    /// [`Events::run_and_wait`](crate::Events::run_and_wait) gave up, or mpv
    /// shut down first.
    #[error("mpv did not answer")]
    NoReply,
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn check(call: &'static str, status: c_int) -> Result<()> {
    match ErrorCode::of(status) {
        Some(code) => Err(Error::Api { call, code }),
        None => Ok(()),
    }
}

/// mpv's reason a call was refused or a file stopped (`mpv_error`), as a
/// value rather than its English words, so whoever shows it can say it in
/// their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    EventQueueFull,
    NoMemory,
    Uninitialized,
    InvalidParameter,
    OptionNotFound,
    OptionFormat,
    OptionError,
    PropertyNotFound,
    PropertyFormat,
    PropertyUnavailable,
    PropertyError,
    Command,
    LoadingFailed,
    AoInitFailed,
    VoInitFailed,
    NothingToPlay,
    UnknownFormat,
    Unsupported,
    NotImplemented,
    Generic,
    /// A code from an mpv newer than this binding.
    Other(c_int),
}

impl ErrorCode {
    /// The error a status reports, or `None` for success, which mpv's calls
    /// say with 0 or more.
    pub(crate) fn of(status: c_int) -> Option<Self> {
        Some(match status {
            0.. => return None,
            -1 => Self::EventQueueFull,
            -2 => Self::NoMemory,
            -3 => Self::Uninitialized,
            -4 => Self::InvalidParameter,
            -5 => Self::OptionNotFound,
            -6 => Self::OptionFormat,
            -7 => Self::OptionError,
            -8 => Self::PropertyNotFound,
            -9 => Self::PropertyFormat,
            -10 => Self::PropertyUnavailable,
            -11 => Self::PropertyError,
            -12 => Self::Command,
            -13 => Self::LoadingFailed,
            -14 => Self::AoInitFailed,
            -15 => Self::VoInitFailed,
            -16 => Self::NothingToPlay,
            -17 => Self::UnknownFormat,
            -18 => Self::Unsupported,
            -19 => Self::NotImplemented,
            -20 => Self::Generic,
            other => Self::Other(other),
        })
    }
}

/// In `mpv_error_string`'s words, for the log. Copied rather than asked for,
/// since asking needs the loaded library and a code is a plain value.
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EventQueueFull => "event queue full",
            Self::NoMemory => "memory allocation failed",
            Self::Uninitialized => "core not initialized",
            Self::InvalidParameter => "invalid parameter",
            Self::OptionNotFound => "option not found",
            Self::OptionFormat => "unsupported format for accessing option",
            Self::OptionError => "error setting option",
            Self::PropertyNotFound => "property not found",
            Self::PropertyFormat => "unsupported format for accessing property",
            Self::PropertyUnavailable => "property unavailable",
            Self::PropertyError => "error accessing property",
            Self::Command => "error running command",
            Self::LoadingFailed => "loading failed",
            Self::AoInitFailed => "audio output initialization failed",
            Self::VoInitFailed => "video output initialization failed",
            Self::NothingToPlay => "no audio or video data played",
            Self::UnknownFormat => "unrecognized file format",
            Self::Unsupported => "not supported",
            Self::NotImplemented => "operation not implemented",
            Self::Generic => "something happened",
            Self::Other(code) => return write!(f, "unknown error {code}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_negative_status_is_an_error() {
        assert_eq!(ErrorCode::of(0), None);
        assert_eq!(ErrorCode::of(7), None);
        assert_eq!(
            ErrorCode::of(-10),
            Some(ErrorCode::PropertyUnavailable),
            "a read with no value is not a refusal"
        );
        assert_eq!(ErrorCode::of(-8), Some(ErrorCode::PropertyNotFound));
        assert_eq!(ErrorCode::of(-17), Some(ErrorCode::UnknownFormat));
        assert_eq!(ErrorCode::of(-99), Some(ErrorCode::Other(-99)));
    }
}
