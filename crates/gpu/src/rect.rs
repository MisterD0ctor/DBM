//! Rectangles whose units are part of their type.
//!
//! The same four numbers mean different things along the frame path: a video
//! rectangle is normalised to the window, Slint lays a panel out in logical
//! pixels, and the glass pass draws it in physical ones. Naming the unit in
//! the type makes handing one to the other a compile error rather than a
//! picture that sits in the wrong place.

use std::marker::PhantomData;

/// Normalised to a window: 0 is its left or top edge, 1 its right or bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Normalised {}

/// Slint's logical pixels: physical pixels divided by the window scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Logical {}

/// Physical pixels from the window's top-left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pixels {}

/// A rectangle in units `U`, from its top-left to its bottom-right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect<U> {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    unit: PhantomData<U>,
}

impl<U> Rect<U> {
    /// Private, so a rectangle is made through the constructor for its unit
    /// rather than by naming a unit at the call site.
    const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self {
            x0,
            y0,
            x1,
            y1,
            unit: PhantomData,
        }
    }

    /// The same rectangle as the four floats a shader takes.
    pub const fn as_array(self) -> [f32; 4] {
        [self.x0, self.y0, self.x1, self.y1]
    }

    pub fn width(self) -> f32 {
        self.x1 - self.x0
    }

    pub fn height(self) -> f32 {
        self.y1 - self.y0
    }

    pub fn centre(self) -> (f32, f32) {
        (
            f32::midpoint(self.x0, self.x1),
            f32::midpoint(self.y0, self.y1),
        )
    }
}

impl Rect<Normalised> {
    /// The whole window.
    pub const FULL: Self = Self::new(0.0, 0.0, 1.0, 1.0);

    pub const fn normalised(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::new(x0, y0, x1, y1)
    }
}

impl Rect<Logical> {
    pub const fn logical(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::new(x0, y0, x1, y1)
    }

    /// The same rectangle in physical pixels, for the glass pass.
    pub fn scaled(self, scale: f32) -> Rect<Pixels> {
        Rect::pixels(
            self.x0 * scale,
            self.y0 * scale,
            self.x1 * scale,
            self.y1 * scale,
        )
    }
}

impl Rect<Pixels> {
    pub const fn pixels(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::new(x0, y0, x1, y1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_logical_rect_scales_into_physical_pixels() {
        let rect = Rect::logical(10.0, 20.0, 30.0, 40.0);
        assert_eq!(rect.width(), 20.0);
        assert_eq!(rect.height(), 20.0);
        assert_eq!(rect.centre(), (20.0, 30.0));
        assert_eq!(rect.scaled(2.0), Rect::pixels(20.0, 40.0, 60.0, 80.0));
    }
}
