//! The tunable material parameters, in one list.
//!
//! Both the UI and the pipeline read from this registry rather than each
//! keeping its own copy of what exists, so adding a knob is a single entry
//! here: it appears in the panel, gets a slider with the right range, and
//! reaches the shader, without touching the UI file or the render code.
//!
//! Values live in a shared store because the two ends run at different
//! times — the UI writes whenever a slider moves, the pipeline reads once a
//! frame. A dirty flag rather than a callback, so a drag that moves a slider
//! twenty times in a frame still costs one pipeline update.

use std::cell::Cell;

use crate::commands;
use crate::pipeline::{BorderParams, GlassParams};

const AMBIENCE_ON: &str = "ambience.enabled";
const GLASS_ON: &str = "glass.enabled";
const AUTOPLAY: &str = "playback.autoplay";
/// What `bevel` and `refract` were called while they were pixel widths.
///
/// Both are ratios now — of a panel's corner radius, and of the bevel — so a
/// saved number means something different than it did. Rather than let an old
/// 39.5 arrive as a ratio of 39.5, the names changed, which makes the old keys
/// unknown and therefore skipped; these convert them instead, so a look tuned
/// by eye survives the change rather than silently snapping back to default.
const LEGACY_BEVEL: &str = "glass.bevel";
const LEGACY_REFRACT: &str = "glass.refract";
/// The corner radius those pixel values were tuned against: every panel had
/// one, before the pills arrived with radii of their own.
const LEGACY_RADIUS: f32 = 32.0;

const SUB_SCALE: &str = "subtitles.scale";
const SUB_POS: &str = "subtitles.pos";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // Glass
    Blur,
    Bevel,
    Refract,
    Ior,
    Aberration,
    Specular,
    Sky,
    Tint,
    // Ambient border
    EdgeBlur,
    Spread,
    Falloff,
    Softness,
    Grain,
}

/// Which panel page a parameter appears on. Also the unit a reset applies
/// to, so "reset" always means something a user can point at.
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
    /// Stable identifier used in the saved file. Distinct from `label`,
    /// which is display text and free to change without orphaning anyone's
    /// saved settings.
    pub name: &'static str,
    /// Shown in the panel. Grouped by the headings below.
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
}

pub const REGISTRY: &[Param] = &[
    Param { key: Key::Blur, name: "glass.blur", section: Section::Glass, label: "Blur", min: 0.0, max: 40.0 },
    Param { key: Key::Bevel, name: "glass.bevel_ratio", section: Section::Glass, label: "Bevel", min: 0.05, max: 2.0 },
    Param { key: Key::Refract, name: "glass.refract_ratio", section: Section::Glass, label: "Refract", min: 0.0, max: 4.0 },
    Param { key: Key::Ior, name: "glass.ior", section: Section::Glass, label: "IOR", min: 1.0, max: 3.0 },
    Param { key: Key::Aberration, name: "glass.aberration", section: Section::Glass, label: "Fringe", min: 0.0, max: 1.0 },
    Param { key: Key::Specular, name: "glass.specular", section: Section::Glass, label: "Reflect", min: 0.0, max: 2.0 },
    Param { key: Key::Sky, name: "glass.sky", section: Section::Glass, label: "Sky", min: 0.0, max: 4.0 },
    Param { key: Key::Tint, name: "glass.tint", section: Section::Glass, label: "Tint", min: 0.0, max: 1.0 },
    Param { key: Key::EdgeBlur, name: "border.edge_blur", section: Section::Border, label: "Edge blur", min: 0.0, max: 0.1 },
    Param { key: Key::Spread, name: "border.spread", section: Section::Border, label: "Spread", min: 0.0, max: 5.0 },
    Param { key: Key::Falloff, name: "border.falloff", section: Section::Border, label: "Falloff", min: 0.0, max: 12.0 },
    Param { key: Key::Softness, name: "border.softness", section: Section::Border, label: "Softness", min: 0.0, max: 1.0 },
    Param { key: Key::Grain, name: "border.grain", section: Section::Border, label: "Grain", min: 0.0, max: 1024.0 },
];

/// What a settings page held just before it was reset.
///
/// Kept because settings always save: a reset lands on disk within a second,
/// and the thing it throws away is a look tuned by eye over an evening, which
/// no one can type back in.
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
    /// Whether each effect runs at all. Kept beside the parameters because
    /// they are saved and restored together, and because turning an effect
    /// off should not lose the tuning behind it.
    ambience_on: Cell<bool>,
    glass_on: Cell<bool>,
    /// Autoplay: advance through the playlist at the end of a file.
    autoplay: Cell<bool>,
    /// Subtitle size and vertical placement. Preferences rather than
    /// per-file state, which is why they are here and the delay is not: a
    /// size that suits your eyes suits every file, a delay never does.
    sub_scale: Cell<f32>,
    sub_pos: Cell<f32>,
    dirty: Cell<bool>,
    /// Set alongside `dirty`, but consumed by the saver rather than the
    /// renderer. Two flags because they are drained at very different rates:
    /// the pipeline every frame, the disk once things settle.
    unsaved: Cell<bool>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            glass: Cell::new(GlassParams::default()),
            border: Cell::new(BorderParams::default()),
            ambience_on: Cell::new(true),
            glass_on: Cell::new(true),
            autoplay: Cell::new(true),
            sub_scale: Cell::new(commands::SUB_SCALE_DEFAULT as f32),
            sub_pos: Cell::new(commands::SUB_POS_DEFAULT as f32),
            // Set so the first frame pushes the defaults through, rather
            // than relying on the pipeline having been built with the same
            // ones.
            dirty: Cell::new(true),
            unsaved: Cell::new(false),
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
            Key::Bevel => glass.bevel,
            Key::Refract => glass.refract,
            Key::Ior => glass.ior,
            Key::Aberration => glass.aberration,
            Key::Specular => glass.specular,
            Key::Sky => glass.sky,
            // The colour stays in code; how much of it shows is the part
            // worth adjusting by eye.
            Key::Tint => glass.tint_amount,
            Key::EdgeBlur => border.edge_blur,
            Key::Spread => border.spread,
            Key::Falloff => border.falloff,
            Key::Softness => border.falloff_softness,
            Key::Grain => border.grain,
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
            Key::Bevel => glass.bevel = v,
            Key::Refract => glass.refract = v,
            Key::Ior => glass.ior = v,
            Key::Aberration => glass.aberration = v,
            Key::Specular => glass.specular = v,
            Key::Sky => glass.sky = v,
            Key::Tint => glass.tint_amount = v,
            Key::EdgeBlur => border.edge_blur = v,
            Key::Spread => border.spread = v,
            Key::Falloff => border.falloff = v,
            Key::Softness => border.falloff_softness = v,
            Key::Grain => border.grain = v,
        }
        self.glass.set(glass);
        self.border.set(border);
        self.touch();
    }

    /// Take the current values if anything moved since the last call.
    ///
    /// Coalescing on a flag rather than reacting per change means a slider
    /// dragged across twenty pixels in one frame costs one pipeline update.
    pub fn take_changes(&self) -> Option<(GlassParams, BorderParams)> {
        self.dirty
            .replace(false)
            .then(|| (self.glass.get(), self.border.get()))
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

impl Store {
    pub fn ambience_on(&self) -> bool {
        self.ambience_on.get()
    }

    /// Read on every parameter change and never written from the interface:
    /// there is no switch any more, because turning the glass off erased the
    /// panel the switch was printed on. The flag survives in the settings
    /// file, which is plain text, for anyone working on the shader who wants
    /// the unrefracted picture back for a minute.
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

    pub fn sub_scale(&self) -> f32 {
        self.sub_scale.get()
    }

    pub fn sub_pos(&self) -> f32 {
        self.sub_pos.get()
    }

    /// Record what was actually sent to mpv. The clamp lives in `commands`,
    /// which is the only place that knows the range.
    pub fn set_sub_scale(&self, value: f32) {
        self.sub_scale.set(value);
        self.touch();
    }

    pub fn set_sub_pos(&self, value: f32) {
        self.sub_pos.set(value);
        self.touch();
    }

    /// Restore one section to the built-in defaults.
    ///
    /// The defaults are the reset target now that the saved file is the
    /// source of truth — see `Persister`.
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

    /// Whether anything changed since the last call, clearing the flag.
    ///
    /// Only the fact matters, not the values: the saver re-reads them when it
    /// finally writes, which is always the newest state. Separate from
    /// `take_changes` because the two are drained at very different rates.
    pub fn take_unsaved(&self) -> bool {
        self.unsaved.replace(false)
    }

    /// Current values, keyed for the saved file. Booleans ride along as
    /// 0/1 so the file stays one flat list of numbers.
    pub fn to_saved(&self) -> Vec<(String, f32)> {
        let mut out: Vec<(String, f32)> = REGISTRY
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.to_string(), self.value(i)))
            .collect();
        out.push((AMBIENCE_ON.into(), self.ambience_on.get() as u8 as f32));
        out.push((GLASS_ON.into(), self.glass_on.get() as u8 as f32));
        out.push((AUTOPLAY.into(), self.autoplay.get() as u8 as f32));
        out.push((SUB_SCALE.into(), self.sub_scale.get()));
        out.push((SUB_POS.into(), self.sub_pos.get()));
        out
    }

    /// Apply saved values, ignoring anything unrecognised.
    ///
    /// Unknown keys are skipped rather than rejected: a file written by a
    /// later version, or one hand-edited with a typo, should cost the
    /// setting it names and nothing else.
    pub fn apply_saved(&self, saved: &[(String, f32)]) {
        let (mut legacy_bevel, mut legacy_refract) = (None, None);
        let (mut saw_bevel, mut saw_refract) = (false, false);
        for (name, value) in saved {
            if let Some(index) = REGISTRY.iter().position(|p| p.name == name) {
                self.set(index, *value);
                match REGISTRY[index].key {
                    Key::Bevel => saw_bevel = true,
                    Key::Refract => saw_refract = true,
                    _ => {}
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
                _ => {}
            }
        }

        // Only where the file has not already been written in the new terms;
        // a real value never loses to a converted one.
        let index_of = |key: Key| REGISTRY.iter().position(|p| p.key == key);
        if !saw_bevel {
            if let (Some(px), Some(i)) = (legacy_bevel, index_of(Key::Bevel)) {
                self.set(i, px / LEGACY_RADIUS);
                eprintln!("dbm: converted saved bevel {px}px to a ratio of the radius");
            }
        }
        if !saw_refract {
            // Against the bevel it was tuned beside, which is what the new
            // value is a fraction of.
            let was = legacy_bevel.unwrap_or(LEGACY_RADIUS * 0.6).max(f32::EPSILON);
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

/// Read the saved settings. A missing or unreadable file is not an error —
/// it just means the defaults stand.
pub fn load(path: &std::path::Path) -> Vec<(String, f32)> {
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
/// Debounced rather than immediate: a slider drag produces dozens of changes
/// a second, and each one is a whole-file rewrite. Waiting for a quiet moment
/// turns a drag into one write. The write itself goes to the worker, because
/// touching the disk on the frame path is exactly the mistake this codebase
/// keeps having to unlearn.
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
        // `submit` rather than `run`: a failure here is the one the person
        // tuning most needs to hear about, because everything they have just
        // adjusted is in it.
        self.worker.submit(move |_mpv| {
            let borrowed: Vec<(&str, f32)> =
                values.iter().map(|(n, v)| (n.as_str(), *v)).collect();
            save_owned(&path, &borrowed)
                .err()
                .map(crate::worker::Completion::Notice)
        });
    }
}

/// Write settings out. Runs on the worker, and returns what to say if it
/// fails rather than failing loudly: losing a saved slider must not take the
/// player down, but it must not pass in silence either. Every material
/// parameter the person has just tuned is in this file, and the first they
/// knew of it not being written was the next time they started the player.
fn save_owned(path: &std::path::Path, values: &[(&str, f32)]) -> Result<(), String> {
    use std::fmt::Write as _;
    let mut out = String::from("# Death by MPV - material parameters\n");
    for (name, value) in values {
        let _ = writeln!(out, "{name} = {value}");
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, out).map_err(|e| {
        eprintln!("dbm: could not save settings: {e}");
        format!("Settings could not be saved — {e}")
    })
}
