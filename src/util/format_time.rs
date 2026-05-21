//! Port of `formatTime.js`. Same `M:SS` / `H:MM:SS` shape, with optional
//! fractional seconds. Computes in display units (10^-fraction_digits) so
//! the seconds field can't roll past 59 due to rounding — which the JS
//! version had as a latent bug.

pub fn format_time(seconds: f64, fraction_digits: usize) -> String {
    if !seconds.is_finite() {
        return "00:00".into();
    }
    let seconds = seconds.max(0.0);
    let scale = 10_f64.powi(fraction_digits as i32);
    let units = scale as i64;
    let total = (seconds * scale).floor() as i64;
    let whole = total / units.max(1);
    let frac = total % units.max(1);
    let h = whole / 3600;
    let m = (whole % 3600) / 60;
    let s = whole % 60;

    if fraction_digits == 0 {
        if h == 0 {
            format!("{m}:{s:02}")
        } else {
            format!("{h}:{m:02}:{s:02}")
        }
    } else {
        let frac_str = format!("{:0>width$}", frac, width = fraction_digits);
        if h == 0 {
            format!("{m}:{s:02}.{frac_str}")
        } else {
            format!("{h}:{m:02}:{s:02}.{frac_str}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero() {
        assert_eq!(format_time(0.0, 0), "0:00");
    }

    #[test]
    fn minutes_seconds() {
        assert_eq!(format_time(65.0, 0), "1:05");
        assert_eq!(format_time(125.0, 0), "2:05");
    }

    #[test]
    fn hours() {
        assert_eq!(format_time(3661.0, 0), "1:01:01");
    }

    #[test]
    fn fractional() {
        assert_eq!(format_time(5.5, 2), "0:05.50");
        assert_eq!(format_time(65.123, 1), "1:05.1");
    }

    #[test]
    fn no_rollover() {
        // The JS version would emit "0:60" here due to toFixed rounding.
        assert_eq!(format_time(59.99, 0), "0:59");
        assert_eq!(format_time(59.999, 1), "0:59.9");
    }

    #[test]
    fn handles_nan() {
        assert_eq!(format_time(f64::NAN, 0), "00:00");
        assert_eq!(format_time(f64::INFINITY, 0), "00:00");
    }
}
