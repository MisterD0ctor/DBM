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

/// A rectangle in whole physical pixels, from the window's top-left. The
/// right and bottom edges are exclusive, as a scissor or a blit takes them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Region {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Region {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// The whole pixels a rectangle of physical pixels covers.
    pub fn covering(rect: Rect<Pixels>) -> Self {
        Self {
            left: rect.x0.floor() as i32,
            top: rect.y0.floor() as i32,
            right: rect.x1.ceil() as i32,
            bottom: rect.y1.ceil() as i32,
        }
    }

    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub const fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// The part of this region inside `other`.
    pub fn intersection(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    /// The same edges as the four integers a GL call takes.
    pub const fn as_array(self) -> [i32; 4] {
        [self.left, self.top, self.right, self.bottom]
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

    #[test]
    fn a_region_covers_whole_pixels() {
        let region = Region::covering(Rect::pixels(1.2, 2.8, 10.1, 11.0));
        assert_eq!(region, Region::new(1, 2, 11, 11));
        assert_eq!(region.width(), 10);
        assert_eq!(region.height(), 9);
        assert_eq!(region.as_array(), [1, 2, 11, 11]);
    }

    #[test]
    fn an_intersection_can_be_empty() {
        let region = Region::new(0, 0, 10, 10);
        assert_eq!(
            region.intersection(Region::new(20, 20, 30, 30)),
            Region::new(20, 20, 10, 10)
        );
        assert!(region.intersection(Region::new(20, 20, 30, 30)).is_empty());
        assert_eq!(
            region.intersection(Region::new(5, 5, 20, 20)),
            Region::new(5, 5, 10, 10)
        );
    }
}
