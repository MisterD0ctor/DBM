//! Saving and loading: one flat list of names and numbers, read at start and
//! written once a change settles.

use std::cell::Cell;
use std::io;

use super::{Key, REGISTRY, Store, Toggle};

const SCALE: &str = "accessibility.scale";

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

/// Where the settings are saved.
///
/// Plain text on purpose: it is a short list of numbers, and being able to
/// open it in an editor while tuning is worth more than a compact format.
fn file() -> std::path::PathBuf {
    // A run nobody is watching (an exercise, a showcase or a capture) keeps to
    // its own file, so it neither touches the real settings nor looks like
    // whatever they were last set to while trying something.
    let name = if crate::run_mode::unattended() {
        log::info!("unattended - settings go to settings.test.conf");
        "settings.test.conf"
    } else {
        "settings.conf"
    };
    dbm_library::paths::app_data_dir().join(name)
}

/// Read the saved settings. A missing file isn't an error; the defaults
/// stand. Nor is a value that isn't a finite number, which costs only the
/// setting it names. A file where no line could be read is moved aside first,
/// so the next save doesn't write defaults over it.
fn load(path: &std::path::Path) -> Vec<(String, f32)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let saved: Vec<(String, f32)> = lines
        .clone()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            let value = value.trim().parse().ok().filter(|v: &f32| v.is_finite())?;
            Some((name.trim().to_string(), value))
        })
        .collect();
    if saved.is_empty() && lines.count() > 0 {
        dbm_library::paths::set_aside(path);
    }
    saved
}

/// Writes settings out shortly after they stop changing.
///
/// Debounced, because a slider drag makes dozens of changes a second and each
/// would rewrite the whole file. The write goes to the worker, since the
/// frame path must not touch the disk.
pub struct Persister {
    store: std::rc::Rc<Store>,
    worker: std::rc::Rc<crate::worker::Worker>,
    path: std::path::PathBuf,
    /// When the pending change was noticed, or `None` if nothing is pending.
    since: Cell<Option<std::time::Instant>>,
    /// How many times the values have been taken to be written. See [`save`].
    taken: Cell<u64>,
}

/// Keeps settings saving as they change, until it is dropped.
pub struct Saving {
    persister: std::rc::Rc<Persister>,
    _poll: slint::Timer,
}

impl Saving {
    /// Write any change still settling, now and on this thread.
    ///
    /// For the end of a run, once the window is gone: a change made in the
    /// last second would otherwise be lost with the timer. Not through the
    /// worker, whose queue may hold slow jobs.
    pub fn flush(&self) {
        let persister = &self.persister;
        if persister.store.take_unsaved() || persister.since.take().is_some() {
            let (taken, values) = persister.take();
            if let Err(e) = save(&persister.path, &values, taken) {
                log::warn!("settings not saved before quitting: {e}");
            }
        }
    }
}

impl Persister {
    /// How long values must hold still before being written.
    const SETTLE: std::time::Duration = std::time::Duration::from_millis(700);
    /// How often the pending state is checked.
    const POLL: std::time::Duration = std::time::Duration::from_millis(250);

    /// Load whatever was saved, then start watching for changes.
    #[must_use]
    pub fn install(
        store: std::rc::Rc<Store>,
        worker: std::rc::Rc<crate::worker::Worker>,
    ) -> Saving {
        let path = file();
        let saved = load(&path);
        if !saved.is_empty() {
            store.apply_saved(&saved);
            log::info!("loaded {} saved setting(s)", saved.len());
        }

        let persister = std::rc::Rc::new(Self {
            store,
            worker,
            path,
            since: Cell::new(None),
            taken: Cell::new(0),
        });
        let poll = slint::Timer::default();
        poll.start(slint::TimerMode::Repeated, Self::POLL, {
            let persister = persister.clone();
            move || persister.poll()
        });
        Saving {
            persister,
            _poll: poll,
        }
    }

    /// The values to write, numbered in the order they were taken.
    fn take(&self) -> (u64, Vec<(String, f32)>) {
        self.taken.set(self.taken.get() + 1);
        (self.taken.get(), self.store.to_saved())
    }

    fn poll(&self) {
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
        let (taken, values) = self.take();
        let path = self.path.clone();
        // `submit` rather than `run`, so a failure reaches the person tuning,
        // whose adjustments are in it.
        self.worker.submit(move |_mpv| {
            save(&path, &values, taken).err().map(|source| {
                crate::worker::Completion::Warning(crate::worker::Warning::SettingsNotSaved(source))
            })
        });
    }
}

/// The number of the values last written. A save queued on the worker can
/// run after a newer one written by [`Saving::flush`], and must not undo it.
static WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);

/// Write settings out, unless newer ones have been already. Returns the
/// reason on failure rather than panicking: losing a saved slider must not
/// take the player down, nor pass in silence.
fn save(path: &std::path::Path, values: &[(String, f32)], taken: u64) -> Result<(), io::Error> {
    use std::fmt::Write as _;
    let mut written = WRITTEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if taken < *written {
        return Ok(());
    }
    let mut out = String::from("# Death by MPV settings\n");
    for (name, value) in values {
        let _ = writeln!(out, "{name} = {value}");
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    dbm_library::paths::write_atomic(path, out).inspect_err(|e| {
        log::warn!("could not save settings: {e}");
    })?;
    *written = taken;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_that_is_not_a_number_is_skipped() {
        let file = std::env::temp_dir().join(format!("dbm-settings-nan-{}", std::process::id()));
        std::fs::write(
            &file,
            "glass.refract = NaN\nglass.blur = inf\nsubtitles.pos = 0.5\n",
        )
        .unwrap();
        assert_eq!(load(&file), [("subtitles.pos".to_string(), 0.5)]);
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn a_file_with_nothing_readable_is_kept_aside() {
        let file = std::env::temp_dir().join(format!("dbm-settings-junk-{}", std::process::id()));
        let aside = std::path::PathBuf::from(format!("{}.unreadable", file.display()));
        std::fs::write(&file, "\0\0\0\0").unwrap();
        assert!(load(&file).is_empty());
        assert!(aside.is_file() && !file.exists());
        std::fs::remove_file(&aside).ok();
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

    #[test]
    fn skipping_is_off_until_asked_for_and_then_kept() {
        let store = Store::default();
        assert!(
            !store.is_on(Toggle::SkipIntros) && !store.is_on(Toggle::SkipCredits),
            "nothing is cut from an episode unasked"
        );
        store.turn(Toggle::SkipIntros, true);
        store.turn(Toggle::SkipCredits, true);
        let loaded = Store::default();
        loaded.apply_saved(&store.to_saved());
        assert!(loaded.is_on(Toggle::SkipIntros) && loaded.is_on(Toggle::SkipCredits));

        // A file from before the switches skips nothing.
        let older = Store::default();
        older.apply_saved(&[("playback.autoplay".to_string(), 1.0)]);
        assert!(!older.is_on(Toggle::SkipIntros) && !older.is_on(Toggle::SkipCredits));
    }

    #[test]
    fn chapters_survive_a_save_and_a_load() {
        let store = Store::default();
        assert!(
            store.is_on(Toggle::Chapters),
            "the timeline marks chapters until told not to"
        );
        store.turn(Toggle::Chapters, false);
        let saved = store.to_saved();
        assert!(
            saved.contains(&("playback.chapters".to_string(), 0.0)),
            "{saved:?}"
        );
        let loaded = Store::default();
        loaded.apply_saved(&saved);
        assert!(!loaded.is_on(Toggle::Chapters));

        // A file from before the switch keeps them.
        let older = Store::default();
        older.apply_saved(&[("playback.remaining".to_string(), 1.0)]);
        assert!(older.is_on(Toggle::Chapters));
    }
}
