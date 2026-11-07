//! What a property's value is as it crosses to and from mpv, and the Rust
//! types that stand for it.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::PathBuf;

use crate::MediaPath;
use crate::ffi::{FORMAT_DOUBLE, FORMAT_FLAG, FORMAT_INT64, FORMAT_STRING};

/// The shape a value is handed over in. Only the scalar ones: a list is read
/// as the JSON mpv prints it (see `tracks`), which spares marshalling a node
/// tree across the FFI boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    String,
    Flag,
    Int,
    Double,
}

impl Format {
    pub(crate) const fn code(self) -> c_int {
        match self {
            Self::String => FORMAT_STRING,
            Self::Flag => FORMAT_FLAG,
            Self::Int => FORMAT_INT64,
            Self::Double => FORMAT_DOUBLE,
        }
    }
}

/// A value as mpv reports or takes it, before it is anything more specific.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Flag(bool),
    Int(i64),
    Double(f64),
    Str(String),
}

impl Value {
    pub(crate) const fn format(&self) -> Format {
        match self {
            Self::Flag(_) => Format::Flag,
            Self::Int(_) => Format::Int,
            Self::Double(_) => Format::Double,
            Self::Str(_) => Format::String,
        }
    }

    /// Copy out a value mpv laid out as `format`. `None` for a format that
    /// isn't one of the scalars, which is how mpv says there is no value.
    ///
    /// # Safety
    /// `data` points at a value of `format`, laid out as client.h says, and
    /// is valid for the call.
    pub(crate) unsafe fn read(format: c_int, data: *const c_void) -> Option<Self> {
        if data.is_null() {
            return None;
        }
        match format {
            // SAFETY: the caller vouches that `data` points at an `int`.
            FORMAT_FLAG => Some(Self::Flag(unsafe { *data.cast::<c_int>() } != 0)),
            // SAFETY: the caller vouches that `data` points at an `int64_t`.
            FORMAT_INT64 => Some(Self::Int(unsafe { *data.cast::<i64>() })),
            // SAFETY: the caller vouches that `data` points at a `double`.
            FORMAT_DOUBLE => Some(Self::Double(unsafe { *data.cast::<f64>() })),
            FORMAT_STRING => {
                // SAFETY: the caller vouches that `data` points at a `char *`.
                let text = unsafe { *data.cast::<*const c_char>() };
                if text.is_null() {
                    return None;
                }
                // SAFETY: non-null, and NUL-terminated as every mpv string is.
                let text = unsafe { CStr::from_ptr(text) };
                Some(Self::Str(text.to_string_lossy().into_owned()))
            }
            _ => None,
        }
    }
}

/// A type a property can be read as.
pub trait FromMpv: Sized {
    /// What to ask mpv for. It converts between the numeric formats itself,
    /// so an integer property can be read as a double.
    const FORMAT: Format;

    /// `None` when mpv's value isn't one of these.
    fn from_mpv(value: Value) -> Option<Self>;
}

/// A type a property can be set to.
pub trait ToMpv {
    fn to_mpv(&self) -> Value;
}

impl FromMpv for bool {
    const FORMAT: Format = Format::Flag;
    fn from_mpv(value: Value) -> Option<Self> {
        match value {
            Value::Flag(on) => Some(on),
            _ => None,
        }
    }
}

impl ToMpv for bool {
    fn to_mpv(&self) -> Value {
        Value::Flag(*self)
    }
}

impl FromMpv for i64 {
    const FORMAT: Format = Format::Int;
    fn from_mpv(value: Value) -> Option<Self> {
        match value {
            Value::Int(n) => Some(n),
            _ => None,
        }
    }
}

impl ToMpv for i64 {
    fn to_mpv(&self) -> Value {
        Value::Int(*self)
    }
}

impl FromMpv for f64 {
    const FORMAT: Format = Format::Double;
    fn from_mpv(value: Value) -> Option<Self> {
        match value {
            Value::Double(n) => Some(n),
            _ => None,
        }
    }
}

impl ToMpv for f64 {
    fn to_mpv(&self) -> Value {
        Value::Double(*self)
    }
}

impl FromMpv for String {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        match value {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }
}

impl ToMpv for String {
    fn to_mpv(&self) -> Value {
        Value::Str(self.clone())
    }
}

/// An empty path reads as none, which is what mpv reports between files.
impl FromMpv for MediaPath {
    const FORMAT: Format = Format::String;
    fn from_mpv(value: Value) -> Option<Self> {
        String::from_mpv(value)
            .filter(|path| !path.is_empty())
            .map(Self::reported)
    }
}

impl ToMpv for PathBuf {
    fn to_mpv(&self) -> Value {
        Value::Str(self.to_string_lossy().into_owned())
    }
}
