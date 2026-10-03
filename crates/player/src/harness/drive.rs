//! Standing in for a hand: pointer moves, clicks, the wheel, and keys.

use slint::ComponentHandle;

use crate::MainWindow;

pub(super) fn hover(ui: &MainWindow, x: f32, y: f32) {
    use slint::platform::WindowEvent;
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(x, y),
    });
}

/// Click a point, the way a mouse would.
///
/// A `Flickable` has to tell a drag from a tap, so a list that scrolls
/// correctly can still be one whose rows have stopped responding. Nothing
/// short of a real click finds that.
pub(super) fn click(ui: &MainWindow, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let position = slint::LogicalPosition::new(x, y);
    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

/// Turn a wheel over a point, the way a mouse would.
pub(super) fn wheel(ui: &MainWindow, x: f32, y: f32, delta: f32) {
    use slint::platform::WindowEvent;
    let position = slint::LogicalPosition::new(x, y);
    let window = ui.window();
    // The pointer has to be over the list for the list to be what scrolls.
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x: 0.0,
        delta_y: delta,
    });
}

/// Press a chord, the way a keyboard would.
///
/// Slint tracks modifier state from the modifier key's own press and release,
/// so a chord is four events rather than one with flags.
pub(super) fn chord(ui: &MainWindow, modifiers: &[slint::platform::Key], text: &str) {
    use slint::platform::WindowEvent;
    let window = ui.window();
    for m in modifiers {
        window.dispatch_event(WindowEvent::KeyPressed { text: (*m).into() });
    }
    window.dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: text.into() });
    for m in modifiers.iter().rev() {
        window.dispatch_event(WindowEvent::KeyReleased { text: (*m).into() });
    }
}

/// A special key as the character Slint encodes it in.
pub(super) fn arrow(ui: &MainWindow, key: slint::platform::Key) {
    let text: slint::SharedString = key.into();
    chord(ui, &[], &text);
}
