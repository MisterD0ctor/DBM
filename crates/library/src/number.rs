//! Numbers read from outside the player: a file's text, ffmpeg's reports,
//! mpv's watch-later entries.

/// `text` as a number that measures something, or `None`.
///
/// `"nan"` and `"inf"` parse as `f64`, and a comparison or a clamp lets NaN
/// through, so every number the player reads from outside comes in here.
pub fn finite(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_number_that_measures_something_is_one() {
        assert_eq!(finite(" 12.5 "), Some(12.5));
        assert_eq!(finite("-3"), Some(-3.0));
        for text in ["nan", "NaN", "inf", "-infinity", "", "12,5"] {
            assert_eq!(finite(text), None, "{text}");
        }
    }
}
