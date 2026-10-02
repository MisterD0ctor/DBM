//! Everything drawn with OpenGL.
//!
//! `pipeline` is the chain of passes — the ambient border, the backdrop
//! blur, the glass — and `gfx` the few GL primitives they are built from.
//! `mark` is the player's own logo, drawn as the light behind an empty
//! window. All of it runs inside Slint's GL context, driven by `driver`.

pub mod gfx;
pub mod mark;
pub mod pipeline;
