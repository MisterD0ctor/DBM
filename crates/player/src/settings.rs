//! What the person has set, kept between runs.
//!
//! Two kinds of thing, saved to one file (see `file`). The parameters the
//! sliders set (the glass, the ambient border and the picture) are a registry
//! that the UI, the pipeline and mpv read, so adding a knob is one entry in
//! `registry`: it gets a slider with the right range and reaches the shader,
//! or mpv, without touching the UI. Beside them sit the preferences that
//! aren't sliders (see `preferences`): whether each effect runs, autoplay,
//! the time left, animations, the interface's size, darkening bright glass
//! and tips, and the subtitles' size and place.
//!
//! Values live in a shared store because the two ends run at different
//! times: the UI writes whenever a slider moves, the pipeline reads once a
//! frame. A dirty flag rather than a callback, so a drag that moves a slider
//! twenty times in a frame costs one pipeline update.

use std::cell::{Cell, RefCell};

use dbm_gpu::pipeline::{BorderParams, GlassParams};
use dbm_naming::Language;

use crate::playback::commands::{self, PictureParams};

mod file;
mod preferences;
mod registry;

pub use file::Persister;
pub use preferences::{SCALE_MAX, SCALE_MIN, SCALE_STEP, Toggle, offered_scale};
use registry::Params;
pub use registry::{Key, REGISTRY, Section};

/// What a settings page held just before it was reset.
///
/// Kept for undo, because settings save at once and a look tuned by eye
/// can't be typed back in.
#[derive(Clone, Copy)]
pub struct Before {
    params: Params,
    pub sub_scale: f32,
    pub sub_pos: f32,
}

/// Shared between the slider callbacks and the render driver.
pub struct Store {
    params: Cell<Params>,
    /// By `Toggle`, in declaration order.
    toggles: [Cell<bool>; Toggle::ALL.len()],
    /// The interface's size, as a multiple of what the system sets (see
    /// `interface::scale`).
    scale: Cell<f32>,
    /// Subtitle size and vertical placement. Preferences rather than per-file
    /// state, unlike the delay: a size that suits your eyes suits every file,
    /// and a delay never does.
    sub_scale: Cell<f32>,
    sub_pos: Cell<f32>,
    dirty: Cell<bool>,
    /// Set alongside `dirty`, but drained by the saver once things settle,
    /// where the pipeline drains `dirty` every frame.
    unsaved: Cell<bool>,
    /// The language chosen, or `None` to follow the system's.
    language: RefCell<Option<Language>>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            params: Cell::default(),
            toggles: Toggle::ALL.map(|toggle| Cell::new(toggle.default_on())),
            scale: Cell::new(SCALE_MIN),
            sub_scale: Cell::new(commands::SUB_SCALE_DEFAULT as f32),
            sub_pos: Cell::new(commands::SUB_POS_DEFAULT as f32),
            // So the first frame pushes the defaults through, rather than
            // trusting the pipeline to have been built with the same ones.
            dirty: Cell::new(true),
            unsaved: Cell::new(false),
            language: RefCell::new(None),
        }
    }
}

impl Store {
    pub fn value(&self, index: usize) -> f32 {
        REGISTRY
            .get(index)
            .map_or(0.0, |param| *self.params.get().slot(param.key))
    }

    pub fn set(&self, index: usize, v: f32) {
        let Some(param) = REGISTRY.get(index) else {
            return;
        };
        let mut params = self.params.get();
        *params.slot(param.key) = v.clamp(param.min, param.max);
        self.params.set(params);
        self.touch();
    }

    /// How mpv is to draw the picture, which the caller tells it.
    pub fn picture(&self) -> PictureParams {
        self.params.get().picture
    }

    /// Take the current values if anything moved since the last call.
    pub fn take_changes(&self) -> Option<(GlassParams, BorderParams)> {
        self.dirty.replace(false).then(|| {
            let params = self.params.get();
            (params.glass, params.border)
        })
    }

    /// Restore one section to the built-in defaults.
    pub fn reset(&self, section: Section) {
        self.take_section(section, &Params::default());
    }

    /// Everything a reset can change, as it stands now.
    pub fn before(&self) -> Before {
        Before {
            params: self.params.get(),
            sub_scale: self.sub_scale.get(),
            sub_pos: self.sub_pos.get(),
        }
    }

    /// Put one section back the way a snapshot had it. The subtitle values
    /// are applied by the caller, which is the one that can tell mpv.
    pub fn restore(&self, section: Section, before: &Before) {
        self.take_section(section, &before.params);
    }

    fn take_section(&self, section: Section, from: &Params) {
        let mut params = self.params.get();
        params.take_section(section, from);
        self.params.set(params);
        self.touch();
    }

    fn touch(&self) {
        self.dirty.set(true);
        self.unsaved.set(true);
    }
}
