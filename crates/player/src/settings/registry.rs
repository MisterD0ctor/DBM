//! The parameters the sliders set: the glass, the ambient border and the
//! picture. Adding a knob is one entry in `REGISTRY`: it gets a slider with
//! the right range and reaches the shader, or mpv, without touching the UI.

use dbm_gpu::pipeline::{BorderParams, GlassParams};
use dbm_lang::marked;

use crate::playback::commands::PictureParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

impl Key {
    /// The key saved under `name`, where this build has one. A name from an
    /// older or newer file is simply not a key here.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|&key| param(key).name == name)
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

/// The order is also the order of `param-icon` in settings_panel.slint, which
/// picks each row's icon by its index here.
///
/// One list builds both the keys and the registry, so the two can't disagree
/// about which keys exist or what order they are in.
macro_rules! registry {
    ($($key:ident),* $(,)?) => {
        impl Key {
            /// Every key, in the order the registry has them.
            pub const ALL: &[Self] = &[$(Self::$key),*];
        }

        pub const REGISTRY: &[Param] = &[$(param(Key::$key)),*];
    };
}

registry!(
    Blur, Refract, Aberration, Rim, Tint, Absorb, Shadow, EdgeBlur, Spread, Falloff, Softness,
    Brightness, Contrast, Saturation, Gamma
);

/// The one entry for `key`: its panel, its name in the saved file, its
/// labels and its range.
#[rustfmt::skip]
pub const fn param(key: Key) -> Param {
    match key {
        Key::Blur => Param { key: Key::Blur, name: "glass.blur", section: Section::Glass, label: marked("Blur"), hint: marked("How much the glass blurs the picture behind it"), min: 0.0, max: 100.0 },
        Key::Refract => Param { key: Key::Refract, name: "glass.refract_ratio", section: Section::Glass, label: marked("Refraction"), hint: marked("How far each pane's rim bends the picture behind it"), min: 0.0, max: 1.0 },
        Key::Aberration => Param { key: Key::Aberration, name: "glass.aberration", section: Section::Glass, label: marked("Dispersion"), hint: marked("How widely the rim splits light into its colours, as a prism does"), min: 0.0, max: 1.0 },
        Key::Rim => Param { key: Key::Rim, name: "glass.rim", section: Section::Glass, label: marked("Edge light"), hint: marked("How brightly each pane's rim catches the light"), min: 0.0, max: 1.0 },
        Key::Tint => Param { key: Key::Tint, name: "glass.tint", section: Section::Glass, label: marked("Smoke"), hint: marked("How far the glass darkens toward black, as smoked glass does"), min: 0.0, max: 1.0 },
        Key::Absorb => Param { key: Key::Absorb, name: "glass.absorb", section: Section::Glass, label: marked("Dimming"), hint: marked("How much of a bright scene the glass holds back, more the brighter it is"), min: 0.0, max: 1.0 },
        Key::Shadow => Param { key: Key::Shadow, name: "glass.shadow", section: Section::Glass, label: marked("Shadow"), hint: marked("How dark the shadow each pane casts on the picture"), min: 0.0, max: 1.0 },
        Key::EdgeBlur => Param { key: Key::EdgeBlur, name: "border.edge_blur", section: Section::Border, label: marked("Edge blur"), hint: marked("How much the picture's edge is softened before it spreads"), min: 0.0, max: 0.1 },
        Key::Spread => Param { key: Key::Spread, name: "border.spread", section: Section::Border, label: marked("Spread"), hint: marked("How far the picture's light reaches into the black bars"), min: 0.0, max: 5.0 },
        Key::Falloff => Param { key: Key::Falloff, name: "border.falloff", section: Section::Border, label: marked("Falloff"), hint: marked("How quickly that light fades away from the picture"), min: 0.0, max: 12.0 },
        Key::Softness => Param { key: Key::Softness, name: "border.softness", section: Section::Border, label: marked("Softness"), hint: marked("How gently the light begins to fade"), min: 0.0, max: 1.0 },
        Key::Brightness => Param { key: Key::Brightness, name: "picture.brightness", section: Section::Picture, label: marked("Brightness"), hint: "", min: -100.0, max: 100.0 },
        Key::Contrast => Param { key: Key::Contrast, name: "picture.contrast", section: Section::Picture, label: marked("Contrast"), hint: "", min: -100.0, max: 100.0 },
        Key::Saturation => Param { key: Key::Saturation, name: "picture.saturation", section: Section::Picture, label: marked("Saturation"), hint: "", min: -100.0, max: 100.0 },
        Key::Gamma => Param { key: Key::Gamma, name: "picture.gamma", section: Section::Picture, label: marked("Gamma"), hint: marked("How light the picture's middle tones are, leaving black and white"), min: -100.0, max: 100.0 },
    }
}

/// The three sets of values the sliders tune.
#[derive(Clone, Copy, Default)]
pub(super) struct Params {
    pub(super) glass: GlassParams,
    pub(super) border: BorderParams,
    pub(super) picture: PictureParams,
}

impl Params {
    /// The value `key` names.
    pub(super) fn slot(&mut self, key: Key) -> &mut f32 {
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
    pub(super) fn take_section(&mut self, section: Section, from: &Self) {
        match section {
            Section::Glass => self.glass = from.glass,
            Section::Border => self.border = from.border,
            Section::Picture => self.picture = from.picture,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_in_the_registry_once_and_in_its_order() {
        assert_eq!(REGISTRY.len(), Key::ALL.len());
        for (key, param) in Key::ALL.iter().zip(REGISTRY) {
            assert_eq!(*key, param.key);
            assert_eq!(REGISTRY.iter().filter(|p| p.key == *key).count(), 1);
        }
    }

    #[test]
    fn every_key_is_addressed_by_its_saved_name() {
        for &key in Key::ALL {
            assert_eq!(Key::from_name(param(key).name), Some(key));
        }
        assert_eq!(Key::from_name("no.such.setting"), None);
    }

    #[test]
    fn every_saved_name_is_unique() {
        for (i, param) in REGISTRY.iter().enumerate() {
            assert!(
                !REGISTRY[..i].iter().any(|other| other.name == param.name),
                "{} is used twice",
                param.name
            );
        }
    }
}
