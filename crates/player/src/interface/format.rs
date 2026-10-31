//! Numbers as the interface writes them.
//!
//! Every figure the interface shows passes through one of these, so each kind
//! of number is written one way wherever it appears: a time on the timeline
//! and under the seek preview, a running time on a playlist row, what is left
//! of a film on the way in.

use crate::lang::{fill, tr};

/// `H:MM:SS`, dropping the hours field when it would be zero.
pub fn time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".into();
    }
    let total = seconds as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// How much of a film is left, the way a person says it: "43 min left",
/// "1 h 12 min left". Minutes are rounded up, so a row never claims nothing
/// is left of something that has not ended.
pub fn left(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 60.0 {
        return tr("under a minute left").into();
    }
    let minutes = (seconds / 60.0).ceil() as u64;
    match (minutes / 60, minutes % 60) {
        (0, m) => fill(tr("{} min left"), &[&m]),
        (h, 0) => fill(tr("{} h left"), &[&h]),
        (h, m) => fill(tr("{} h {} min left"), &[&h, &m]),
    }
}

/// A speed as a person writes it: `1×`, `1.5×`, `1.25×`. Zero, which is what
/// mpv reports before it has said anything, reads as normal speed.
pub fn speed(speed: f64) -> String {
    let speed = if speed > 0.0 { speed } else { 1.0 };
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{}×", decimal(text))
}

/// A subtitle delay, saying which way the subtitles were moved, since a sign
/// leaves that to be remembered. Zero has no way, so the untouched case
/// doesn't read as a setting someone made.
pub fn delay(seconds: f64) -> String {
    if seconds.abs() < 0.005 {
        format!("{} s", decimal("0.00"))
    } else {
        let pattern = if seconds > 0.0 {
            tr("{} s later")
        } else {
            tr("{} s earlier")
        };
        fill(pattern, &[&decimal(&format!("{:.2}", seconds.abs()))])
    }
}

/// The widest a delay reads, either way, a line each, for its readout to be
/// as wide as in the language spoken.
pub fn delay_widest() -> String {
    format!("{}\n{}", delay(10.0), delay(-10.0))
}

/// What is left of a film, as the timeline's right end says it when asked
/// to: `−12:34`, a minus sign rather than a hyphen, to stand as wide as the
/// digits do.
pub fn remaining(seconds: f64) -> String {
    format!("\u{2212}{}", time(seconds.max(0.0)))
}

/// A subtitle size, as a multiple of the default: `1.15×`.
pub fn scale(multiple: f64) -> String {
    format!("{}×", decimal(&format!("{multiple:.2}")))
}

/// A subtitle position, in mpv's own percent of the window: `100%`.
pub fn position(percent: f64) -> String {
    format!("{percent:.0}%")
}

/// A slider's value, as how far along its range it is: `35%`.
///
/// The values themselves are the shader's units, an edge blur of 0.007 or a
/// falloff of 4.0, which say nothing to anyone tuning by eye, and each slider
/// would need its own precision to say even that. How far along is what the
/// track already shows, so the figure and the fill agree.
///
/// A value either side of nothing, such as a picture's brightness, says how
/// far and which way in its own units instead, `+10` or `−20`, since half
/// way along is where none of it is.
pub fn readout(value: f32, min: f32, max: f32) -> String {
    if min < 0.0 {
        let v = value.round();
        return if v > 0.0 {
            format!("+{v:.0}")
        } else if v < 0.0 {
            format!("\u{2212}{:.0}", -v)
        } else {
            "0".into()
        };
    }
    let span = (max - min).abs().max(f32::EPSILON);
    let share = ((value - min) / span).clamp(0.0, 1.0) * 100.0;
    format!("{share:.0}%")
}

/// A figure written with a full stop, with the decimal mark of the language
/// spoken instead: `1.25` or `1,25`.
pub fn decimal(figure: &str) -> String {
    // The mark is its own entry, which each language's file gives as the one
    // it writes.
    figure.replace('.', tr("."))
}

/// A fragment as the start of a sentence, since mpv's reasons are lower-case
/// ("unrecognized file format") and a notice opens with one.
pub fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_drops_the_hours_until_there_are_some() {
        assert_eq!(time(0.0), "0:00");
        assert_eq!(time(65.9), "1:05");
        assert_eq!(time(3600.0), "1:00:00");
        assert_eq!(time(5025.67), "1:23:45");
        // Nothing to show is the start, not a negative clock.
        assert_eq!(time(-3.0), "0:00");
        assert_eq!(time(f64::NAN), "0:00");
    }

    #[test]
    fn what_is_left_rounds_up_to_the_minute() {
        assert_eq!(left(30.0), "under a minute left");
        assert_eq!(left(61.0), "2 min left");
        assert_eq!(left(43.0 * 60.0), "43 min left");
        assert_eq!(left(2.0 * 3600.0), "2 h left");
        assert_eq!(left(72.0 * 60.0), "1 h 12 min left");
    }

    #[test]
    fn a_speed_carries_no_trailing_zeros() {
        assert_eq!(speed(1.0), "1×");
        assert_eq!(speed(1.5), "1.5×");
        assert_eq!(speed(1.25), "1.25×");
        assert_eq!(speed(0.0), "1×");
    }

    #[test]
    fn a_delay_is_signed_except_at_zero() {
        assert_eq!(delay(0.0), "0.00 s");
        assert_eq!(delay(0.001), "0.00 s");
        assert_eq!(delay(0.3), "0.30 s later");
        assert_eq!(delay(-1.25), "1.25 s earlier");
    }

    #[test]
    fn a_readout_is_how_far_along_its_range() {
        assert_eq!(readout(10.0, 0.0, 100.0), "10%");
        assert_eq!(readout(0.007, 0.0, 0.1), "7%");
        assert_eq!(readout(4.0, 0.0, 12.0), "33%");
        assert_eq!(readout(1.0, 0.0, 1.0), "100%");
    }

    #[test]
    fn a_readout_either_side_of_nothing_says_which_way() {
        assert_eq!(readout(10.0, -100.0, 100.0), "+10");
        assert_eq!(readout(-20.4, -100.0, 100.0), "\u{2212}20");
        assert_eq!(readout(0.2, -100.0, 100.0), "0");
    }

    #[test]
    fn a_sentence_starts_with_a_capital() {
        assert_eq!(
            sentence("unrecognized file format"),
            "Unrecognized file format"
        );
        assert_eq!(sentence(""), "");
    }
}
