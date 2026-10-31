//! The interface's size, chosen in the settings, on top of the system's.
//!
//! Slint lays everything out in logical pixels and draws them at the window's
//! scale factor, which the system sets for its own display scaling. A larger
//! factor makes everything larger together, the type, the rows, and the glass
//! and its shadows, which the pipeline sizes by the same factor, with nothing
//! laid out any differently. The system's own factor is kept apart, so a
//! window moved to another display, or the display's scaling changed, lands
//! at the chosen multiple of the new one.
//!
//! The window's floor (`MainWindow`'s `floor-width`) is in logical pixels
//! too, so a larger size raises it on screen: the window grows to it, and the
//! size stops at the largest whose floor the display can still hold, which is
//! what fullscreen gives it.

use std::cell::Cell;

use slint::platform::WindowEvent;
use slint::winit_030::{winit, WinitWindowAccessor};
use slint::ComponentHandle;

use crate::settings::{SCALE_MAX, SCALE_MIN, SCALE_STEP};
use crate::MainWindow;

#[derive(Default)]
pub struct Scale {
    /// The factor the system set, before the chosen multiple.
    system: Cell<Option<f32>>,
    /// The factor last given the window, to tell the system replacing it.
    applied: Cell<f32>,
    /// Whether the display was known when the size was last fitted to it.
    /// It isn't until the window is on one, a frame or two in on Wayland.
    fitted: Cell<bool>,
}

impl Scale {
    /// Draw the interface at `chosen` times the system's size, or at the
    /// largest the display has room for, and show which. Returns it.
    pub fn apply(&self, ui: &MainWindow, chosen: f32) -> f32 {
        let window = ui.window();
        let current = window.scale_factor();
        let system = *self.system.get().get_or_insert(current);
        self.system.set(Some(system));
        let floor = slint::LogicalSize::new(ui.get_floor_width(), ui.get_floor_height());
        let screen = window.with_winit_window(|w| w.current_monitor().map(|m| m.size()));
        let screen = screen.flatten();
        self.fitted.set(screen.is_some());
        let drawn = chosen.clamp(SCALE_MIN, ceiling(screen, floor, system));
        ui.set_interface_scale(drawn);
        let factor = system * drawn;
        self.applied.set(factor);
        if (factor - current).abs() >= 1e-4 {
            // The window keeps its size on screen, so in the new factor's
            // terms it is a different size, which the layout has to be told.
            let size = window.size().to_logical(factor);
            window.dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: factor,
            });
            window.dispatch_event(WindowEvent::Resized { size });
            window.request_redraw();
        }
        // Not before the display is known, or a size it has no room for
        // would grow the window past its edges first.
        if screen.is_some() {
            hold_floor(window, floor.to_physical(factor));
        }
        drawn
    }

    /// From the frame path: the first frame is the first the system's own
    /// factor is known, and the system changing it later replaces the one
    /// given here, so either way the chosen multiple goes on top of it. Until
    /// the window is on a display, each frame asks again whether it fits.
    pub fn frame(&self, ui: &MainWindow, chosen: f32) {
        let current = ui.window().scale_factor();
        if self.system.get().is_none() || (current - self.applied.get()).abs() > 1e-4 {
            self.system.set(Some(current));
            self.apply(ui, chosen);
        } else if !self.fitted.get() {
            self.apply(ui, chosen);
        }
    }

    /// The system's own factor, for what belongs to the desktop rather than
    /// to the interface: the window's corners, which match the other windows'.
    pub fn system(&self, ui: &MainWindow) -> f32 {
        self.system
            .get()
            .unwrap_or_else(|| ui.window().scale_factor())
    }
}

/// The largest size offered whose floor fits within `screen`, or the largest
/// offered while the display is unknown.
fn ceiling(
    screen: Option<winit::dpi::PhysicalSize<u32>>,
    floor: slint::LogicalSize,
    system: f32,
) -> f32 {
    let Some(screen) = screen else {
        return SCALE_MAX;
    };
    let room = (screen.width as f32 / (floor.width * system))
        .min(screen.height as f32 / (floor.height * system));
    // Down to a step, with a little give, so a display exactly the floor's
    // multiple isn't refused it by rounding.
    let steps = ((room - SCALE_MIN) / SCALE_STEP + 1e-3).floor();
    (SCALE_MIN + steps * SCALE_STEP).clamp(SCALE_MIN, SCALE_MAX)
}

/// Keep the window from being made smaller than `floor`, and grow it to the
/// floor if it already is, unless it is fullscreen or maximized, where the
/// desktop sets its size. Slint gives winit the floor only when the layout's
/// own floor changes, which a new scale factor doesn't, so it is given here.
fn hold_floor(window: &slint::Window, floor: slint::PhysicalSize) {
    let grown = window.with_winit_window(|w| {
        w.set_min_inner_size(Some(winit::dpi::PhysicalSize::new(
            floor.width,
            floor.height,
        )));
        if w.fullscreen().is_some() || w.is_maximized() {
            return None;
        }
        let size = w.inner_size();
        let grown =
            slint::PhysicalSize::new(size.width.max(floor.width), size.height.max(floor.height));
        (grown.width != size.width || grown.height != size.height).then_some(grown)
    });
    // Through Slint rather than winit, which keeps track of a size it asked
    // for and hasn't been given yet.
    if let Some(grown) = grown.flatten() {
        window.set_size(grown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOOR: slint::LogicalSize = slint::LogicalSize {
        width: 960.0,
        height: 540.0,
    };

    fn screen(width: u32, height: u32) -> Option<winit::dpi::PhysicalSize<u32>> {
        Some(winit::dpi::PhysicalSize::new(width, height))
    }

    #[test]
    fn a_1080p_screen_holds_every_size() {
        assert_eq!(ceiling(screen(1920, 1080), FLOOR, 1.0), SCALE_MAX);
    }

    #[test]
    fn a_small_laptop_stops_short() {
        assert!((ceiling(screen(1366, 768), FLOOR, 1.0) - 1.4).abs() < 1e-4);
        assert!((ceiling(screen(2560, 1440), FLOOR, 2.0) - 1.3).abs() < 1e-4);
    }

    #[test]
    fn exactly_the_floors_multiple_is_room_enough() {
        assert!((ceiling(screen(1152, 648), FLOOR, 1.0) - 1.2).abs() < 1e-4);
    }

    #[test]
    fn a_screen_under_the_floor_still_gets_the_smallest() {
        assert_eq!(ceiling(screen(800, 600), FLOOR, 1.0), SCALE_MIN);
        assert_eq!(ceiling(None, FLOOR, 1.0), SCALE_MAX);
    }
}
