//! Utility functions shared across the codebase

/// Format a duration in a human-readable way
///
/// # Examples
/// - 3661 seconds → "1h 1m 1s"
/// - 125 seconds → "2m 5s"
/// - 45 seconds → "45s"
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_duration_hours() {
        let duration = Duration::from_secs(3661); // 1h 1m 1s
        assert_eq!(format_duration(duration), "1h 1m 1s");
    }

    #[test]
    fn test_format_duration_minutes() {
        let duration = Duration::from_secs(125); // 2m 5s
        assert_eq!(format_duration(duration), "2m 5s");
    }

    #[test]
    fn test_format_duration_seconds() {
        let duration = Duration::from_secs(45); // 45s
        assert_eq!(format_duration(duration), "45s");
    }

    #[test]
    fn test_format_duration_zero() {
        let duration = Duration::from_secs(0);
        assert_eq!(format_duration(duration), "0s");
    }

    #[test]
    fn test_format_duration_complex() {
        let duration = Duration::from_secs(7325); // 2h 2m 5s
        assert_eq!(format_duration(duration), "2h 2m 5s");
    }
}
