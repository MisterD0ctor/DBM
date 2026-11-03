//! The preferences that aren't sliders: switches, the interface's size,
//! the language, and the subtitles' size and place.

use super::Store;

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
    /// Skip an episode's title sequence as it starts (see `skips`).
    SkipIntros,
    /// Go on from an episode's credits as they start, once their pill has
    /// counted down. Only as far as autoplay goes, so only while it is on.
    SkipCredits,
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
    /// Whether the timeline marks the file's chapters and the seek preview
    /// names them. Only what is drawn: the chapter keys and the skips still
    /// read them.
    Chapters,
}

impl Toggle {
    /// Every switch, in the order the file lists them and in declaration
    /// order, which indexes `Store::toggles`.
    pub(super) const ALL: [Self; 11] = [
        Self::Ambience,
        Self::Glass,
        Self::Autoplay,
        Self::SkipIntros,
        Self::SkipCredits,
        Self::Animations,
        Self::LegibleGlass,
        Self::Tips,
        Self::TipKeys,
        Self::Remaining,
        Self::Chapters,
    ];

    /// The name it is saved under, which outlives any label it is shown with.
    pub(super) const fn key(self) -> &'static str {
        match self {
            Self::Ambience => "ambience.enabled",
            Self::Glass => "glass.enabled",
            Self::Autoplay => "playback.autoplay",
            Self::SkipIntros => "playback.skip-intros",
            Self::SkipCredits => "playback.skip-credits",
            Self::Animations => "accessibility.animations",
            Self::LegibleGlass => "accessibility.legible-glass",
            Self::Tips => "accessibility.tooltips",
            Self::TipKeys => "accessibility.tooltip-keys",
            Self::Remaining => "playback.remaining",
            Self::Chapters => "playback.chapters",
        }
    }

    /// Before the person says otherwise. The skips cut something out of an
    /// episode, so only someone who asked for that gets it.
    pub(super) const fn default_on(self) -> bool {
        !matches!(self, Self::Remaining | Self::SkipIntros | Self::SkipCredits)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_toggle_indexes_its_own_cell() {
        for (index, toggle) in Toggle::ALL.into_iter().enumerate() {
            assert_eq!(toggle as usize, index, "{toggle:?}");
        }
    }
}
