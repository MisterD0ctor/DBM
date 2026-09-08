//! Subtitle-timing helpers, shared by the tracks-menu control and the
//! `z` / `Z` shortcuts so the two can't drift apart on step size, rounding,
//! or how a delay is written out.

/// mpv's own increment for `sub-delay`, so a nudge here moves by the same
/// amount as a nudge in plain mpv.
pub const DELAY_STEP: f64 = 0.1;

/// Below this, a delay is just float noise and reads as "no delay".
const EPSILON: f64 = 0.001;

pub fn is_shifted(seconds: f64) -> bool {
    seconds.abs() >= EPSILON
}

/// Move a delay by `delta`, snapped back onto the step grid. mpv hands the
/// current value back as a float, so without the snap repeated nudges drift
/// into `0.30000000000000004`.
pub fn step_delay(current: f64, delta: f64) -> f64 {
    ((current + delta) / DELAY_STEP).round() * DELAY_STEP
}

/// Signed, fixed-width rendering: `"+0.50 s"` reads as shifted where a bare
/// `"0.50 s"` wouldn't, and zero drops the sign (including `-0.0`).
pub fn format_delay(seconds: f64) -> String {
    if is_shifted(seconds) {
        format!("{seconds:+.2} s")
    } else {
        "0.00 s".to_string()
    }
}
