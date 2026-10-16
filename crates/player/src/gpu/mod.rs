//! Everything drawn with OpenGL.
//!
//! `pipeline` is the chain of passes — the ambient border, the backdrop
//! blur, the glass — and `gfx` the few GL primitives they are built from.
//! `mark` is the player's own logo, drawn as the light behind an empty
//! window. Two things are drawn after Slint rather than under it, on the
//! finished frame: `shadow`, the thin dark rim under the interface's words
//! and marks, and `fade`, the soft edge on a scrolling list. All of it runs
//! inside Slint's GL context, driven by `driver`.

pub mod fade;
pub mod gfx;
pub mod mark;
pub mod pipeline;
pub mod shadow;
