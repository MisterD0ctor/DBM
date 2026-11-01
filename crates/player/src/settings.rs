//! What the person has set, kept between runs.
//!
//! Two kinds of thing, in one file. The parameters the sliders set (the
//! glass, the ambient border and the picture) are a registry that the UI, the
//! pipeline and mpv read, so adding a knob is one entry here: it gets a slider
//! with the right range and reaches the shader, or mpv, without touching the
//! UI. Beside them sit the preferences that aren't sliders: whether each
//! effect runs, autoplay, the time left, animations, the interface's size,
//! darkening bright glass and tips, and the subtitles' size and place.
//!
//! Values live in a shared store because the two ends run at different
//! times: the UI writes whenever a slider moves, the pipeline reads once a
//! frame. A dirty flag rather than a callback, so a drag that moves a slider
//! twenty times in a frame costs one pipeline update.

use std::cell::{Cell, RefCell};

use crate::gpu::pipeline::{BorderParams, GlassParams};
use crate::lang::marked;
use crate::playback::commands;

/// A preference that is a switch. Each rides in the saved file as 0 or 1, so
/// the file stays one flat list of numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Toggle {
    /// Whether the ambient border runs at all, kept apart from its parameters
    /// so turning it off doesn't lose their tuning.
    Ambience,
    /// The same for the glass. Never set from the interface, since a switch
    /// for it would erase the panel it sits on; it is in the plain-text
    /// settings file, for work on the shader.
    Glass,
    /// Advance through the playlist at the end of a file.
    Autoplay,
    /// Whether anything in the interface moves (see `Motion` in theme.slint),
    /// including the backdrop the pipeline fades up.
    Animations,
    /// Whether the glass is held dark enough for its words over a bright frame
    /// (see `pipeline::legible_ceiling`).
    LegibleGlass,
    /// Whether the bar's controls name themselves under the pointer (see
    /// `Tip` in globals.slint).
    Tips,
    /// Whether those names come with their keys.
    TipKeys,
    /// Whether the timeline's right end says what is left rather than the
    /// whole length.
    Remaining,
}

impl Toggle {
    /// Every switch, in the order the file lists them and in declaration
    /// order, which indexes `Store::toggles`.
    const ALL: [Self; 8] = [
        Self::Ambience,
        Self::Glass,
        Self::Autoplay,
        Self::Animations,
        Self::LegibleGlass,
        Self::Tips,
        Self::TipKeys,
        Self::Remaining,
    ];

    /// The name it is saved under, which outlives any label it is shown with.
    const fn key(self) -> &'static str {
        match self {
            Self::Ambience => "ambience.enabled",
            Self::Glass => "glass.enabled",
            Self::Autoplay => "playback.autoplay",
            Self::Animations => "accessibility.animations",
            Self::LegibleGlass => "accessibility.legible-glass",
            Self::Tips => "accessibility.tooltips",
            Self::TipKeys => "accessibility.tooltip-keys",
            Self::Remaining => "playback.remaining",
        }
    }

    /// Before the person says otherwise.
    const fn default_on(self) -> bool {
        !matches!(self, Self::Remaining)
    }
}

const SCALE: &str = "accessibility.scale";
/// The sizes the interface can be set to, as a multiple of the system's.
/// No smaller, since the type is already at its floor; no larger than fits
/// the window's own floor on a 1080p screen in fullscreen.
pub const SCALE_MIN: f32 = 1.0;
pub const SCALE_MAX: f32 = 1.5;
pub const SCALE_STEP: f32 = 0.1;

/// `scale` kept to the sizes offered, to a tenth, so a file edited by hand
/// can't set one the stepper never reaches.
pub fn offered_scale(scale: f32) -> f32 {
    ((scale / SCALE_STEP).round() * SCALE_STEP).clamp(SCALE_MIN, SCALE_MAX)
}

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
    // Picture, which mpv draws rather than the pipeline.
    Brightness,
    Contrast,
    Saturation,
    Gamma,
}

/// Which panel page a parameter appears on, and so what a reset applies to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Glass,
    Border,
    Picture,
}

impl Section {
    pub fn from_index(i: i32) -> Option<Self> {
        match i {
            0 => Some(Self::Glass),
            1 => Some(Self::Border),
            2 => Some(Self::Picture),
            _ => None,
        }
    }
}

/// How mpv draws the picture: each in its own terms, -100 to 100, and none
/// by default. A preference rather than a film's, as the subtitles' size is:
/// what a screen or a room needs, every film needs.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct PictureParams {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub gamma: f32,
}

impl PictureParams {
    /// Each with the mpv property it sets.
    pub fn properties(&self) -> [(&'static str, f32); 4] {
        [
            ("brightness", self.brightness),
            ("contrast", self.contrast),
            ("saturation", self.saturation),
            ("gamma", self.gamma),
        ]
    }
}

pub struct Param {
    pub key: Key,
    pub section: Section,
    /// Stable identifier used in the saved file. Distinct from `label`, so
    /// the display text can change without orphaning saved settings.
    pub name: &'static str,
    /// Shown in the panel, in English; translated where it is shown. Grouped
    /// by the headings below.
    pub label: &'static str,
    /// What it does, for someone who doesn't know the word: shown beside the
    /// panel when the row is rested on. Empty where the name says it.
    pub hint: &'static str,
    pub min: f32,
    pub max: f32,
}

/// The order is also the order of `param-icon` in app.slint, which picks each
/// row's icon by its index here.
#[rustfmt::skip]
pub const REGISTRY: &[Param] = &[
    Param { key: Key::Blur, name: "glass.blur", section: Section::Glass, label: marked("Blur"), hint: marked("How much the glass blurs the picture behind it"), min: 0.0, max: 100.0 },
    Param { key: Key::Refract, name: "glass.refract_ratio", section: Section::Glass, label: marked("Refraction"), hint: marked("How far each pane's rim bends the picture behind it"), min: 0.0, max: 1.0 },
    Param { key: Key::Aberration, name: "glass.aberration", section: Section::Glass, label: marked("Dispersion"), hint: marked("How widely the rim splits light into its colours, as a prism does"), min: 0.0, max: 1.0 },
    Param { key: Key::Rim, name: "glass.rim", section: Section::Glass, label: marked("Edge light"), hint: marked("How brightly each pane's rim catches the light"), min: 0.0, max: 1.0 },
    Param { key: Key::Tint, name: "glass.tint", section: Section::Glass, label: marked("Smoke"), hint: marked("How far the glass darkens toward black, as smoked glass does"), min: 0.0, max: 1.0 },
    Param { key: Key::Absorb, name: "glass.absorb", section: Section::Glass, label: marked("Dimming"), hint: marked("How much of a bright scene the glass holds back, more the brighter it is"), min: 0.0, max: 1.0 },
    Param { key: Key::Shadow, name: "glass.shadow", section: Section::Glass, label: marked("Shadow"), hint: marked("How dark the shadow each pane casts on the picture"), min: 0.0, max: 1.0 },
    Param { key: Key::EdgeBlur, name: "border.edge_blur", section: Section::Border, label: marked("Edge blur"), hint: marked("How much the picture's edge is softened before it spreads"), min: 0.0, max: 0.1 },
    Param { key: Key::Spread, name: "border.spread", section: Section::Border, label: marked("Spread"), hint: marked("How far the picture's light reaches into the black bars"), min: 0.0, max: 5.0 },
    Param { key: Key::Falloff, name: "border.falloff", section: Section::Border, label: marked("Falloff"), hint: marked("How quickly that light fades away from the picture"), min: 0.0, max: 12.0 },
    Param { key: Key::Softness, name: "border.softness", section: Section::Border, label: marked("Softness"), hint: marked("How gently the light begins to fade"), min: 0.0, max: 1.0 },
    Param { key: Key::Brightness, name: "picture.brightness", section: Section::Picture, label: marked("Brightness"), hint: "", min: -100.0, max: 100.0 },
    Param { key: Key::Contrast, name: "picture.contrast", section: Section::Picture, label: marked("Contrast"), hint: "", min: -100.0, max: 100.0 },
    Param { key: Key::Saturation, name: "picture.saturation", section: Section::Picture, label: marked("Saturation"), hint: "", min: -100.0, max: 100.0 },
    Param { key: Key::Gamma, name: "picture.gamma", section: Section::Picture, label: marked("Gamma"), hint: marked("How light the picture's middle tones are, leaving black and white"), min: -100.0, max: 100.0 },
];

/// The three sets of values the sliders tune.
#[derive(Clone, Copy, Default)]
struct Params {
    glass: GlassParams,
    border: BorderParams,
    picture: PictureParams,
}

impl Params {
    /// The value `key` names.
    fn slot(&mut self, key: Key) -> &mut f32 {
        match key {
            Key::Blur => &mut self.glass.blur_sigma,
            Key::Refract => &mut self.glass.refract,
            Key::Aberration => &mut self.glass.aberration,
            Key::Rim => &mut self.glass.rim,
            // The colour stays in code; how much of it shows is the part
            // worth adjusting by eye.
            Key::Tint => &mut self.glass.tint_amount,
            Key::Absorb => &mut self.glass.absorb,
            Key::Shadow => &mut self.glass.shadow,
            Key::EdgeBlur => &mut self.border.edge_blur,
            Key::Spread => &mut self.border.spread,
            Key::Falloff => &mut self.border.falloff,
            Key::Softness => &mut self.border.falloff_softness,
            Key::Brightness => &mut self.picture.brightness,
            Key::Contrast => &mut self.picture.contrast,
            Key::Saturation => &mut self.picture.saturation,
            Key::Gamma => &mut self.picture.gamma,
        }
    }

    /// Take one section's values from `from`.
    fn take_section(&mut self, section: Section, from: &Self) {
        match section {
            Section::Glass => self.glass = from.glass,
            Section::Border => self.border = from.border,
            Section::Picture => self.picture = from.picture,
        }
    }
}

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
    /// The language chosen, by code, or empty to follow the system's.
    language: RefCell<String>,
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
            language: RefCell::new(String::new()),
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

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

impl Store {
    pub fn is_on(&self, toggle: Toggle) -> bool {
        self.toggles[toggle as usize].get()
    }

    pub fn turn(&self, toggle: Toggle, on: bool) {
        self.toggles[toggle as usize].set(on);
        self.touch();
    }

    pub fn scale(&self) -> f32 {
        self.scale.get()
    }

    /// Kept to the sizes offered (see [`offered_scale`]).
    pub fn set_scale(&self, scale: f32) -> f32 {
        let scale = offered_scale(scale);
        self.scale.set(scale);
        self.touch();
        scale
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

    /// Current values, keyed for the saved file.
    pub fn to_saved(&self) -> Vec<(String, f32)> {
        let mut out: Vec<(String, f32)> = REGISTRY
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.to_string(), self.value(i)))
            .collect();
        for toggle in Toggle::ALL {
            let on = f32::from(u8::from(self.is_on(toggle)));
            out.push((toggle.key().into(), on));
        }
        out.push((SCALE.into(), self.scale.get()));
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
            if let Some(toggle) = Toggle::ALL.into_iter().find(|t| t.key() == name) {
                self.toggles[toggle as usize].set(*value >= 0.5);
                continue;
            }
            match name.as_str() {
                LEGACY_BEVEL => legacy_bevel = Some(*value),
                LEGACY_REFRACT => legacy_refract = Some(*value),
                SUB_SCALE => self.sub_scale.set(*value),
                SUB_POS => self.sub_pos.set(*value),
                SCALE => {
                    self.set_scale(*value);
                }
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
                log::info!("converted saved refract {px}px to a ratio of the bevel");
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
            log::info!("loaded {} saved setting(s)", saved.len());
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
        log::warn!("could not save settings: {e}");
        crate::lang::fill(crate::lang::tr("Settings could not be saved — {}"), &[&e])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_toggle_indexes_its_own_cell() {
        for (index, toggle) in Toggle::ALL.into_iter().enumerate() {
            assert_eq!(toggle as usize, index, "{toggle:?}");
        }
    }

    #[test]
    fn animations_survive_a_save_and_a_load() {
        let store = Store::default();
        assert!(
            store.is_on(Toggle::Animations),
            "the interface moves until told not to"
        );
        store.turn(Toggle::Animations, false);
        let saved = store.to_saved();
        assert!(
            saved.contains(&("accessibility.animations".to_string(), 0.0)),
            "{saved:?}"
        );

        let loaded = Store::default();
        loaded.apply_saved(&saved);
        assert!(!loaded.is_on(Toggle::Animations));
    }

    #[test]
    fn a_file_from_before_the_switch_keeps_the_interface_moving() {
        let store = Store::default();
        store.apply_saved(&[("playback.autoplay".to_string(), 0.0)]);
        assert!(store.is_on(Toggle::Animations));
    }

    #[test]
    fn darkening_bright_glass_survives_a_save_and_a_load() {
        let store = Store::default();
        assert!(
            store.is_on(Toggle::LegibleGlass),
            "the glass keeps its words readable until told not to"
        );
        store.turn(Toggle::LegibleGlass, false);
        let loaded = Store::default();
        loaded.apply_saved(&store.to_saved());
        assert!(!loaded.is_on(Toggle::LegibleGlass));

        // A file from before the switch keeps it on.
        let older = Store::default();
        older.apply_saved(&[("accessibility.animations".to_string(), 0.0)]);
        assert!(older.is_on(Toggle::LegibleGlass));
    }

    #[test]
    fn tooltips_survive_a_save_and_a_load() {
        let store = Store::default();
        assert!(
            store.is_on(Toggle::Tips) && store.is_on(Toggle::TipKeys),
            "tips name the key until told not to"
        );
        store.turn(Toggle::Tips, false);
        store.turn(Toggle::TipKeys, false);
        let saved = store.to_saved();

        let loaded = Store::default();
        loaded.apply_saved(&saved);
        assert!(!loaded.is_on(Toggle::Tips));
        assert!(!loaded.is_on(Toggle::TipKeys));

        // A file from before the switches keeps both.
        let older = Store::default();
        older.apply_saved(&[("accessibility.animations".to_string(), 0.0)]);
        assert!(older.is_on(Toggle::Tips) && older.is_on(Toggle::TipKeys));
    }
}
