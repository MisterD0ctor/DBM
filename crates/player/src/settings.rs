//! What the person has set, kept between runs.
//!
//! Two kinds of thing, in one file. The material parameters (the glass and
//! the ambient border) are a registry that both the UI and the pipeline read,
//! so adding a knob is one entry here: it gets a slider with the right range
//! and reaches the shader without touching the UI or the render code. Beside
//! them sit the preferences that aren't sliders: whether each effect runs,
//! autoplay, animations and tips, and the subtitles' size and place.
//!
//! Values live in a shared store because the two ends run at different
//! times: the UI writes whenever a slider moves, the pipeline reads once a
//! frame. A dirty flag rather than a callback, so a drag that moves a slider
//! twenty times in a frame costs one pipeline update.

use std::cell::{Cell, RefCell};

use crate::gpu::pipeline::{BorderParams, GlassParams};
use crate::playback::commands;

const AMBIENCE_ON: &str = "ambience.enabled";
const GLASS_ON: &str = "glass.enabled";
const AUTOPLAY: &str = "playback.autoplay";
const ANIMATIONS: &str = "accessibility.animations";
const TIPS: &str = "accessibility.tooltips";
const TIP_KEYS: &str = "accessibility.tooltip-keys";
/// What a chosen language is saved under, its code after the dot. The file is
/// a list of numbers, so the choice is in the name and the number is a one.
const LANGUAGE: &str = "language.";
/// What `bevel` and `refract` were called in files from when they were pixel
/// widths. They are ratios (of the corner radius, and of the bevel) under new
/// names, so an old pixel value is never read as a ratio; `apply_saved`
/// converts it instead, so a look tuned by eye survives.
///
/// Only `refract` is still a setting, but the old bevel is read too, because
/// the old refract was measured against it.
const LEGACY_BEVEL: &str = "glass.bevel";
const LEGACY_REFRACT: &str = "glass.refract";
/// The corner radius those pixel values were tuned against, which every panel
/// had then.
const LEGACY_RADIUS: f32 = 32.0;

const SUB_SCALE: &str = "subtitles.scale";
const SUB_POS: &str = "subtitles.pos";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // Glass. Bevel, IOR and Reflect are constants in `pipeline` instead,
    // because they are what the material is rather than how much of it you
    // want.
    Blur,
    Refract,
    Aberration,
    Rim,
    Tint,
    Absorb,
    Shadow,
    // Ambient border
    EdgeBlur,
    Spread,
    Falloff,
    Softness,
}

/// Which panel page a parameter appears on, and so what a reset applies to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Glass,
    Border,
}

impl Section {
    pub fn from_index(i: i32) -> Option<Self> {
        match i {
            0 => Some(Self::Glass),
            1 => Some(Self::Border),
            _ => None,
        }
    }
}

pub struct Param {
    pub key: Key,
    pub section: Section,
    /// Stable identifier used in the saved file. Distinct from `label`, so
    /// the display text can change without orphaning saved settings.
    pub name: &'static str,
    /// Shown in the panel. Grouped by the headings below.
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
}

/// The order is also the order of `param-icon` in app.slint, which picks each
/// row's icon by its index here.
#[rustfmt::skip]
pub const REGISTRY: &[Param] = &[
    Param { key: Key::Blur, name: "glass.blur", section: Section::Glass, label: "Blur", min: 0.0, max: 100.0 },
    Param { key: Key::Refract, name: "glass.refract_ratio", section: Section::Glass, label: "Refraction", min: 0.0, max: 4.0 },
    Param { key: Key::Aberration, name: "glass.aberration", section: Section::Glass, label: "Dispersion", min: 0.0, max: 4.0 },
    Param { key: Key::Rim, name: "glass.rim", section: Section::Glass, label: "Edge light", min: 0.0, max: 1.0 },
    Param { key: Key::Tint, name: "glass.tint", section: Section::Glass, label: "Tint", min: 0.0, max: 1.0 },
    Param { key: Key::Absorb, name: "glass.absorb", section: Section::Glass, label: "Dimming", min: 0.0, max: 1.0 },
    Param { key: Key::Shadow, name: "glass.shadow", section: Section::Glass, label: "Shadow", min: 0.0, max: 1.0 },
    Param { key: Key::EdgeBlur, name: "border.edge_blur", section: Section::Border, label: "Edge blur", min: 0.0, max: 0.1 },
    Param { key: Key::Spread, name: "border.spread", section: Section::Border, label: "Spread", min: 0.0, max: 5.0 },
    Param { key: Key::Falloff, name: "border.falloff", section: Section::Border, label: "Falloff", min: 0.0, max: 12.0 },
    Param { key: Key::Softness, name: "border.softness", section: Section::Border, label: "Softness", min: 0.0, max: 1.0 },
];

/// What a settings page held just before it was reset.
///
/// Kept for undo, because settings save at once and a look tuned by eye
/// can't be typed back in.
#[derive(Clone, Copy)]
pub struct Before {
    glass: GlassParams,
    border: BorderParams,
    pub sub_scale: f32,
    pub sub_pos: f32,
}

/// Shared between the slider callbacks and the render driver.
pub struct Store {
    glass: Cell<GlassParams>,
    border: Cell<BorderParams>,
    /// Whether each effect runs at all, kept apart from the parameters so
    /// turning an effect off doesn't lose its tuning.
    ambience_on: Cell<bool>,
    glass_on: Cell<bool>,
    /// Autoplay: advance through the playlist at the end of a file.
    autoplay: Cell<bool>,
    /// Whether anything in the interface moves (see `Motion` in theme.slint),
    /// including the backdrop the pipeline fades up.
    animations: Cell<bool>,
    /// Whether the bar's controls name themselves under the pointer, and
    /// whether the names come with their keys (see `Tip` in globals.slint).
    tips: Cell<bool>,
    tip_keys: Cell<bool>,
    /// Subtitle size and vertical placement. Preferences rather than per-file
    /// state, unlike the delay: a size that suits your eyes suits every file,
    /// and a delay never does.
    sub_scale: Cell<f32>,
    sub_pos: Cell<f32>,
    dirty: Cell<bool>,
    /// Set alongside `dirty`, but drained by the saver once things settle,
    /// where the pipeline drains `dirty` every frame.
    unsaved: Cell<bool>,
    /// The language chosen, by code, or empty to follow the system's.
    language: RefCell<String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            glass: Cell::new(GlassParams::default()),
            border: Cell::new(BorderParams::default()),
            ambience_on: Cell::new(true),
            glass_on: Cell::new(true),
            autoplay: Cell::new(true),
            animations: Cell::new(true),
            tips: Cell::new(true),
            tip_keys: Cell::new(true),
            sub_scale: Cell::new(commands::SUB_SCALE_DEFAULT as f32),
            sub_pos: Cell::new(commands::SUB_POS_DEFAULT as f32),
            // So the first frame pushes the defaults through, rather than
            // trusting the pipeline to have been built with the same ones.
            dirty: Cell::new(true),
            unsaved: Cell::new(false),
            language: RefCell::new(String::new()),
        }
    }
}

impl Store {
    pub fn value(&self, index: usize) -> f32 {
        let (glass, border) = (self.glass.get(), self.border.get());
        let Some(param) = REGISTRY.get(index) else {
            return 0.0;
        };
        match param.key {
            Key::Blur => glass.blur_sigma,
            Key::Refract => glass.refract,
            Key::Aberration => glass.aberration,
            Key::Rim => glass.rim,
            // The colour stays in code; how much of it shows is the part
            // worth adjusting by eye.
            Key::Tint => glass.tint_amount,
            Key::Absorb => glass.absorb,
            Key::Shadow => glass.shadow,
            Key::EdgeBlur => border.edge_blur,
            Key::Spread => border.spread,
            Key::Falloff => border.falloff,
            Key::Softness => border.falloff_softness,
        }
    }

    pub fn set(&self, index: usize, v: f32) {
        let Some(param) = REGISTRY.get(index) else {
            return;
        };
        let v = v.clamp(param.min, param.max);
        let mut glass = self.glass.get();
        let mut border = self.border.get();
        match param.key {
            Key::Blur => glass.blur_sigma = v,
            Key::Refract => glass.refract = v,
            Key::Aberration => glass.aberration = v,
            Key::Rim => glass.rim = v,
            Key::Tint => glass.tint_amount = v,
            Key::Absorb => glass.absorb = v,
            Key::Shadow => glass.shadow = v,
            Key::EdgeBlur => border.edge_blur = v,
            Key::Spread => border.spread = v,
            Key::Falloff => border.falloff = v,
            Key::Softness => border.falloff_softness = v,
        }
        self.glass.set(glass);
        self.border.set(border);
        self.touch();
    }

    /// Take the current values if anything moved since the last call.
    pub fn take_changes(&self) -> Option<(GlassParams, BorderParams)> {
        self.dirty
            .replace(false)
            .then(|| (self.glass.get(), self.border.get()))
    }

    /// Restore one section to the built-in defaults.
    pub fn reset(&self, section: Section) {
        let (glass, border) = (GlassParams::default(), BorderParams::default());
        match section {
            Section::Glass => self.glass.set(glass),
            Section::Border => self.border.set(border),
        }
        self.touch();
    }

    /// Everything a reset can change, as it stands now.
    pub fn before(&self) -> Before {
        Before {
            glass: self.glass.get(),
            border: self.border.get(),
            sub_scale: self.sub_scale.get(),
            sub_pos: self.sub_pos.get(),
        }
    }

    /// Put one section back the way a snapshot had it. The subtitle values
    /// are applied by the caller, which is the one that can tell mpv.
    pub fn restore(&self, section: Section, before: &Before) {
        match section {
            Section::Glass => self.glass.set(before.glass),
            Section::Border => self.border.set(before.border),
        }
        self.touch();
    }

    fn touch(&self) {
        self.dirty.set(true);
        self.unsaved.set(true);
    }
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

impl Store {
    pub fn ambience_on(&self) -> bool {
        self.ambience_on.get()
    }

    /// Never set from the interface, since a switch for it would erase the
    /// panel it sits on. The flag is in the plain-text settings file, for
    /// work on the shader.
    pub fn glass_on(&self) -> bool {
        self.glass_on.get()
    }

    pub fn autoplay(&self) -> bool {
        self.autoplay.get()
    }

    pub fn set_ambience(&self, on: bool) {
        self.ambience_on.set(on);
        self.touch();
    }

    pub fn set_autoplay(&self, on: bool) {
        self.autoplay.set(on);
        self.touch();
    }

    pub fn animations(&self) -> bool {
        self.animations.get()
    }

    pub fn set_animations(&self, on: bool) {
        self.animations.set(on);
        self.touch();
    }

    pub fn tips(&self) -> bool {
        self.tips.get()
    }

    pub fn set_tips(&self, on: bool) {
        self.tips.set(on);
        self.touch();
    }

    pub fn tip_keys(&self) -> bool {
        self.tip_keys.get()
    }

    pub fn set_tip_keys(&self, on: bool) {
        self.tip_keys.set(on);
        self.touch();
    }

    /// The language chosen, by code. Empty for the system's.
    pub fn language(&self) -> String {
        self.language.borrow().clone()
    }

    pub fn set_language(&self, code: &str) {
        *self.language.borrow_mut() = code.to_string();
        self.touch();
    }

    pub fn sub_scale(&self) -> f32 {
        self.sub_scale.get()
    }

    pub fn sub_pos(&self) -> f32 {
        self.sub_pos.get()
    }

    /// Record the size mpv was sent, which `commands::set_sub_scale` has
    /// already clamped.
    pub fn set_sub_scale(&self, value: f32) {
        self.sub_scale.set(value);
        self.touch();
    }

    /// Record where the person put the line. Only `subline` calls this, with
    /// a value it has already clamped (see there for why placement isn't read
    /// back from what mpv was sent).
    pub fn set_sub_pos(&self, value: f32) {
        self.sub_pos.set(value);
        self.touch();
    }
}

// ---------------------------------------------------------------------------
// Saving and loading
// ---------------------------------------------------------------------------

impl Store {
    /// Whether anything changed since the last call, clearing the flag.
    ///
    /// Only the fact matters: the saver reads the values when it writes, so
    /// it always writes the newest.
    pub fn take_unsaved(&self) -> bool {
        self.unsaved.replace(false)
    }

    /// Current values, keyed for the saved file. Booleans are 0/1, so the
    /// file stays one flat list of numbers.
    pub fn to_saved(&self) -> Vec<(String, f32)> {
        let mut out: Vec<(String, f32)> = REGISTRY
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.to_string(), self.value(i)))
            .collect();
        out.push((AMBIENCE_ON.into(), self.ambience_on.get() as u8 as f32));
        out.push((GLASS_ON.into(), self.glass_on.get() as u8 as f32));
        out.push((AUTOPLAY.into(), self.autoplay.get() as u8 as f32));
        out.push((ANIMATIONS.into(), self.animations.get() as u8 as f32));
        out.push((TIPS.into(), self.tips.get() as u8 as f32));
        out.push((TIP_KEYS.into(), self.tip_keys.get() as u8 as f32));
        out.push((SUB_SCALE.into(), self.sub_scale.get()));
        out.push((SUB_POS.into(), self.sub_pos.get()));
        let language = self.language.borrow();
        if !language.is_empty() {
            out.push((format!("{LANGUAGE}{language}"), 1.0));
        }
        out
    }

    /// Apply saved values, ignoring anything unrecognised.
    ///
    /// Unknown keys are skipped rather than rejected, so a file from a later
    /// version, or one with a typo, costs only the setting it names.
    pub fn apply_saved(&self, saved: &[(String, f32)]) {
        let (mut legacy_bevel, mut legacy_refract) = (None, None);
        let mut saw_refract = false;
        for (name, value) in saved {
            if let Some(index) = REGISTRY.iter().position(|p| p.name == name) {
                self.set(index, *value);
                if REGISTRY[index].key == Key::Refract {
                    saw_refract = true;
                }
                continue;
            }
            match name.as_str() {
                LEGACY_BEVEL => legacy_bevel = Some(*value),
                LEGACY_REFRACT => legacy_refract = Some(*value),
                SUB_SCALE => self.sub_scale.set(*value),
                SUB_POS => self.sub_pos.set(*value),
                // Booleans ride in the same file as 0/1.
                AMBIENCE_ON => self.ambience_on.set(*value >= 0.5),
                GLASS_ON => self.glass_on.set(*value >= 0.5),
                AUTOPLAY => self.autoplay.set(*value >= 0.5),
                ANIMATIONS => self.animations.set(*value >= 0.5),
                TIPS => self.tips.set(*value >= 0.5),
                TIP_KEYS => self.tip_keys.set(*value >= 0.5),
                other => {
                    if let Some(code) = other.strip_prefix(LANGUAGE) {
                        *self.language.borrow_mut() = code.to_string();
                    }
                }
            }
        }

        // Only where the file has no value in the current terms, which never
        // loses to a converted one.
        let index_of = |key: Key| REGISTRY.iter().position(|p| p.key == key);
        if !saw_refract {
            // Against the bevel it was tuned beside, which is what the new
            // value is a fraction of.
            let was = legacy_bevel
                .unwrap_or(LEGACY_RADIUS * 0.6)
                .max(f32::EPSILON);
            if let (Some(px), Some(i)) = (legacy_refract, index_of(Key::Refract)) {
                self.set(i, px / was);
                eprintln!("dbm: converted saved refract {px}px to a ratio of the bevel");
            }
        }
        // Loaded values still have to reach the pipeline once.
        self.dirty.set(true);
        // Loading is not a user edit; nothing needs writing straight back.
        self.unsaved.set(false);
    }
}

/// Read the saved settings. A missing or unreadable file isn't an error; the
/// defaults stand.
fn load(path: &std::path::Path) -> Vec<(String, f32)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            Some((name.trim().to_string(), value.trim().parse().ok()?))
        })
        .collect()
}

/// Writes settings out shortly after they stop changing.
///
/// Debounced, because a slider drag makes dozens of changes a second and each
/// would rewrite the whole file. The write goes to the worker, since the
/// frame path must not touch the disk.
pub struct Persister {
    store: std::rc::Rc<Store>,
    worker: std::rc::Rc<crate::worker::Worker>,
    /// When the pending change was noticed, or `None` if nothing is pending.
    since: Cell<Option<std::time::Instant>>,
}

impl Persister {
    /// How long values must hold still before being written.
    const SETTLE: std::time::Duration = std::time::Duration::from_millis(700);
    /// How often the pending state is checked.
    const POLL: std::time::Duration = std::time::Duration::from_millis(250);

    /// Load whatever was saved, then start watching for changes.
    ///
    /// The returned timer must be kept alive; dropping it stops saving.
    #[must_use]
    pub fn install(
        store: std::rc::Rc<Store>,
        worker: std::rc::Rc<crate::worker::Worker>,
    ) -> slint::Timer {
        let path = crate::paths::settings_file();
        let saved = load(&path);
        if !saved.is_empty() {
            store.apply_saved(&saved);
            eprintln!("dbm: loaded {} saved setting(s)", saved.len());
        }

        let this = Self {
            store,
            worker,
            since: Cell::new(None),
        };
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Self::POLL, move || {
            this.poll(&path);
        });
        timer
    }

    fn poll(&self, path: &std::path::Path) {
        // Any change restarts the clock, so a drag in progress never writes.
        if self.store.take_unsaved() {
            self.since.set(Some(std::time::Instant::now()));
            return;
        }
        let Some(since) = self.since.get() else {
            return;
        };
        if since.elapsed() < Self::SETTLE {
            return;
        }
        self.since.set(None);
        let values = self.store.to_saved();
        let path = path.to_path_buf();
        // `submit` rather than `run`, so a failure reaches the person tuning,
        // whose adjustments are in it.
        self.worker.submit(move |_mpv| {
            save(&path, &values)
                .err()
                .map(crate::worker::Completion::Warning)
        });
    }
}

/// Write settings out. Runs on the worker, and returns what to say on failure
/// rather than panicking: losing a saved slider must not take the player
/// down, nor pass in silence.
fn save(path: &std::path::Path, values: &[(String, f32)]) -> Result<(), String> {
    use std::fmt::Write as _;
    let mut out = String::from("# Death by MPV settings\n");
    for (name, value) in values {
        let _ = writeln!(out, "{name} = {value}");
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, out).map_err(|e| {
        eprintln!("dbm: could not save settings: {e}");
        crate::lang::fill(crate::lang::tr("Settings could not be saved — {}"), &[&e])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animations_survive_a_save_and_a_load() {
        let store = Store::default();
        assert!(store.animations(), "the interface moves until told not to");
        store.set_animations(false);
        let saved = store.to_saved();
        assert!(saved.contains(&(ANIMATIONS.to_string(), 0.0)), "{saved:?}");

        let loaded = Store::default();
        loaded.apply_saved(&saved);
        assert!(!loaded.animations());
    }

    #[test]
    fn a_file_from_before_the_switch_keeps_the_interface_moving() {
        let store = Store::default();
        store.apply_saved(&[(AUTOPLAY.to_string(), 0.0)]);
        assert!(store.animations());
    }

    #[test]
    fn tooltips_survive_a_save_and_a_load() {
        let store = Store::default();
        assert!(
            store.tips() && store.tip_keys(),
            "tips name the key until told not to"
        );
        store.set_tips(false);
        store.set_tip_keys(false);
        let saved = store.to_saved();

        let loaded = Store::default();
        loaded.apply_saved(&saved);
        assert!(!loaded.tips());
        assert!(!loaded.tip_keys());

        // A file from before the switches keeps both.
        let older = Store::default();
        older.apply_saved(&[(ANIMATIONS.to_string(), 0.0)]);
        assert!(older.tips() && older.tip_keys());
    }
}
