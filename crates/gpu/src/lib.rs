//! Everything drawn with OpenGL.
//!
//! `pipeline` is the chain of passes — the ambient border, the backdrop
//! blur (`blur`), the glass — and `gfx` the few GL primitives they are built
//! from. Four things are drawn after Slint rather than under it, on the
//! finished frame: `fade`, the soft edge on a scrolling list; `way`, the
//! angle pointing a list back to its current row, which sits in that edge;
//! `shadow`, the thin dark rim under the interface's words and marks, the
//! angle's included; and `corners`, the window's round corners, cut from all
//! of it. All of it runs inside Slint's GL context, driven by the player's
//! `driver`, and none of it knows mpv: the film arrives as whatever draws it
//! into a framebuffer.

pub mod blur;
pub mod corners;
pub mod fade;
pub mod gfx;
pub mod pipeline;
pub mod rect;
pub mod shadow;
pub mod way;

/// The four passes drawn over Slint's finished frame, each absent where its
/// shader wouldn't build. The frame then goes without that one touch and
/// everything else still draws.
pub struct Overlays {
    /// Without it, lists are cut square by their panel.
    pub fader: Option<fade::Fader>,
    /// Without it, nothing points back to a list's current row.
    pub way: Option<way::Ways>,
    pub shadow: Option<shadow::Shadow>,
    /// Without it, the window's corners are square.
    pub corners: Option<corners::Corners>,
}

impl Overlays {
    pub fn new(gl: &glow::Context) -> Self {
        Self {
            fader: optional(fade::Fader::new(gl), "no soft edge on scrolling lists"),
            way: optional(way::Ways::new(gl), "no way back to a list's current row"),
            shadow: optional(shadow::Shadow::new(gl), "no shadow under the interface"),
            corners: optional(corners::Corners::new(gl), "no round corners on the window"),
        }
    }

    pub fn release(self, gl: &glow::Context) {
        if let Some(fader) = self.fader {
            fader.release(gl);
        }
        if let Some(way) = self.way {
            way.release(gl);
        }
        if let Some(shadow) = self.shadow {
            shadow.release(gl);
        }
        if let Some(corners) = self.corners {
            corners.release(gl);
        }
    }
}

/// A pass that built, or a line saying what the frame goes without.
fn optional<T>(built: Result<T, String>, without: &str) -> Option<T> {
    built.inspect_err(|e| log::warn!("{without}: {e}")).ok()
}
