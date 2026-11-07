//! How large a picture is, in physical pixels.
//!
//! Sizes travel between the passes far more often than points do, and nearly
//! every pass asks the same two things of one: whether there is anything to
//! draw, and what a shader wants it as. A `(u32, u32)` answers neither, and
//! at a call site it says nothing about which half is the width.

/// A width and height, in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Nothing to draw into: a minimised window, or one not laid out yet.
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The same size as the two floats a shader takes.
    pub fn as_f32(self) -> [f32; 2] {
        [self.width as f32, self.height as f32]
    }
}

impl From<(u32, u32)> for Size {
    fn from((width, height): (u32, u32)) -> Self {
        Self::new(width, height)
    }
}

impl From<Size> for (u32, u32) {
    fn from(size: Size) -> Self {
        (size.width, size.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_knows_when_there_is_nothing_to_draw() {
        assert!(Size::default().is_empty());
        assert!(Size::new(0, 100).is_empty());
        assert!(Size::new(100, 0).is_empty());
        assert!(!Size::new(1, 1).is_empty());
    }

    #[test]
    fn a_size_survives_the_trip_through_a_tuple_and_back() {
        let size = Size::from((640, 480));
        assert_eq!(size.as_f32(), [640.0, 480.0]);
        assert_eq!(<(u32, u32)>::from(size), (640, 480));
    }
}
