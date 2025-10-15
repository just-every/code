use std::time::Duration;
use std::time::Instant;

/// Returns a string representing the elapsed time since `start_time` like
/// "1m 15s" or "1.50s".
pub fn format_elapsed(start_time: Instant) -> String {
    format_duration(start_time.elapsed())
}

/// Convert a [`std::time::Duration`] into a human-readable, compact string.
///
/// Formatting rules:
/// * < 1 s  -> "{milli}ms"
/// * < 60 s -> "{sec:.2}s" (two decimal places)
/// * < 60 m -> "{min}m {sec:02}s"
/// * < 24 h -> "{hour}h {minute:02}m" (rounded to the nearest minute)
/// * >= 24 h -> "{day}d {hour:02}h" (rounded to the nearest hour)
pub fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        return format!("{millis}ms");
    }

    let secs = duration.as_secs();
    if secs < 60 {
        return format!("{:.2}s", duration.as_secs_f64());
    }

    if secs < 3_600 {
        return format_minutes_seconds(duration);
    }

    if secs < 86_400 {
        return format_hours_minutes(duration);
    }

    format_days_hours(duration)
}

fn format_minutes_seconds(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}m {seconds:02}s")
}

fn format_hours_minutes(duration: Duration) -> String {
    let total_hours_f = duration.as_secs_f64() / 3_600.0;
    let mut hours = total_hours_f.floor() as u64;
    let mut minutes = ((total_hours_f - hours as f64) * 60.0).round() as u64;

    if minutes == 60 {
        minutes = 0;
        hours += 1;
    }

    if hours >= 24 {
        return format_days_hours(duration);
    }

    format!("{hours}h {minutes:02}m")
}

fn format_days_hours(duration: Duration) -> String {
    let total_hours_f = duration.as_secs_f64() / 3_600.0;
    let mut days = (total_hours_f / 24.0).floor() as u64;
    let mut hours = (total_hours_f - days as f64 * 24.0).round() as u64;

    if hours == 24 {
        hours = 0;
        days += 1;
    }

    format!("{days}d {hours:02}h")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_subsecond() {
        // Durations < 1s should be rendered in milliseconds with no decimals.
        let dur = Duration::from_millis(250);
        assert_eq!(format_duration(dur), "250ms");

        // Exactly zero should still work.
        let dur_zero = Duration::from_millis(0);
        assert_eq!(format_duration(dur_zero), "0ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        // Durations between 1s (inclusive) and 60s (exclusive) should be
        // printed with 2-decimal-place seconds.
        let dur = Duration::from_millis(1_500); // 1.5s
        assert_eq!(format_duration(dur), "1.50s");

        // 59.999s rounds to 60.00s
        let dur2 = Duration::from_millis(59_999);
        assert_eq!(format_duration(dur2), "60.00s");
    }

    #[test]
    fn test_format_duration_minutes() {
        // Durations ≥ 1 minute should be printed mmss.
        let dur = Duration::from_millis(75_000); // 1m15s
        assert_eq!(format_duration(dur), "1m 15s");

        let dur_exact = Duration::from_millis(60_000); // 1m0s
        assert_eq!(format_duration(dur_exact), "1m 00s");

        let dur_long = Duration::from_millis(3_601_000);
        assert_eq!(format_duration(dur_long), "1h 00m");
    }

    #[test]
    fn test_format_duration_one_hour_has_space() {
        let dur_hour = Duration::from_millis(3_600_000);
        assert_eq!(format_duration(dur_hour), "1h 00m");
    }

    #[test]
    fn test_format_duration_hours_rounds_minutes() {
        let dur = Duration::from_secs(4 * 3_600 + 58 * 60 + 40);
        assert_eq!(format_duration(dur), "4h 59m");
    }

    #[test]
    fn test_format_duration_days_rounds_hours() {
        let dur = Duration::from_secs(2 * 86_400 + 11 * 3_600 + 45 * 60);
        assert_eq!(format_duration(dur), "2d 12h");
    }
}
