//! Numbers as the interface writes them.
//!
//! Every figure the interface shows passes through one of these, so each kind
//! of number is written one way wherever it appears: a time on the timeline
//! and under the seek preview, a running time on a playlist row, what is left
//! of a film on the way in.

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
        return "under a minute left".into();
    }
    let minutes = (seconds / 60.0).ceil() as u64;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min left"),
        (h, 0) => format!("{h} h left"),
        (h, m) => format!("{h} h {m} min left"),
    }
}

/// A speed as a person writes it: `1×`, `1.5×`, `1.25×`. Zero, which is what
/// mpv reports before it has said anything, reads as normal speed.
pub fn speed(speed: f64) -> String {
    let speed = if speed > 0.0 { speed } else { 1.0 };
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}×")
}

/// A subtitle delay. Signed, because the sign is the whole point: a delay
/// says whether the subtitles are running early or late. Zero is written
/// without one, so the untouched case does not read as a setting someone
/// made.
pub fn delay(seconds: f64) -> String {
    if seconds.abs() < 0.005 {
        "0.00 s".into()
    } else {
        format!("{seconds:+.2} s")
    }
}

/// A subtitle size, as a multiple of the default: `1.15×`.
pub fn scale(multiple: f64) -> String {
    format!("{multiple:.2}×")
}

/// A subtitle position, in mpv's own percent of the window: `100%`.
pub fn position(percent: f64) -> String {
    format!("{percent:.0}%")
}

/// A slider's value, with enough precision to tune by and without a column
/// of noise.
///
/// From the parameter's range rather than its current value, which is what
/// the value alone cannot tell you: 5.000 is three meaningless digits on a
/// blur that runs to 40, and 0.010 is the whole story on an edge blur that
/// runs to 0.1. Same number, opposite needs.
pub fn readout(value: f32, min: f32, max: f32) -> String {
    let span = (max - min).abs();
    let decimals = if span >= 100.0 {
        0
    } else if span >= 10.0 {
        1
    } else if span >= 1.0 {
        2
    } else {
        3
    };
    format!("{value:.decimals$}")
}

/// A fragment as the start of a sentence. mpv's reasons are lower-case —
/// "unrecognized file format" — and a notice opens with one.
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
        assert_eq!(delay(0.3), "+0.30 s");
        assert_eq!(delay(-1.25), "-1.25 s");
    }

    #[test]
    fn a_readout_is_as_precise_as_its_range() {
        assert_eq!(readout(5.0, 0.0, 40.0), "5.0");
        assert_eq!(readout(0.01, 0.0, 0.1), "0.010");
        assert_eq!(readout(124.0, 0.0, 1024.0), "124");
        assert_eq!(readout(0.68, 0.0, 5.0), "0.68");
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
