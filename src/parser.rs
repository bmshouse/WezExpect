use anyhow::{Context, Result};
use chrono::{Duration, NaiveTime, TimeZone};
use chrono_tz::Tz;
use regex::Regex;

/// Parse a time string like "3pm", "11:30am", "12:00pm" into (hour, minute)
fn parse_time_string(time_str: &str) -> Result<(u32, u32)> {
    let time_str = time_str.trim().to_lowercase();

    // Check for am/pm
    let is_pm = time_str.ends_with("pm");
    let is_am = time_str.ends_with("am");

    if !is_am && !is_pm {
        anyhow::bail!("Time must end with 'am' or 'pm': {}", time_str);
    }

    // Remove am/pm suffix
    let time_part = time_str
        .trim_end_matches("pm")
        .trim_end_matches("am")
        .trim();

    // Split by colon if present
    let parts: Vec<&str> = time_part.split(':').collect();

    let (hour, minute) = match parts.len() {
        1 => {
            // Just hour, like "3pm"
            let hour: u32 = parts[0]
                .parse()
                .with_context(|| format!("Invalid hour: {}", parts[0]))?;
            (hour, 0)
        }
        2 => {
            // Hour and minute, like "11:30am"
            let hour: u32 = parts[0]
                .parse()
                .with_context(|| format!("Invalid hour: {}", parts[0]))?;
            let minute: u32 = parts[1]
                .parse()
                .with_context(|| format!("Invalid minute: {}", parts[1]))?;
            (hour, minute)
        }
        _ => anyhow::bail!("Invalid time format: {}", time_str),
    };

    // Validate ranges
    if !(1..=12).contains(&hour) {
        anyhow::bail!("Hour must be between 1 and 12: {}", hour);
    }
    if minute >= 60 {
        anyhow::bail!("Minute must be less than 60: {}", minute);
    }

    // Convert to 24-hour format
    let hour_24 = if is_pm {
        if hour == 12 {
            12 // 12pm = 12:00
        } else {
            hour + 12 // 1pm = 13:00, etc.
        }
    } else if hour == 12 {
        0 // 12am = 00:00
    } else {
        hour // 1am = 01:00, etc.
    };

    Ok((hour_24, minute))
}

/// Parse a reset time message and return a DateTime in the specified timezone
///
/// # Arguments
/// * `time_str` - Time string like "3pm", "11:30am"
/// * `tz_str` - Timezone string like "America/Santiago"
///
/// # Returns
/// A DateTime<Tz> representing the next occurrence of that time
pub fn parse_reset_time(time_str: &str, tz_str: &str) -> Result<chrono::DateTime<Tz>> {
    // Parse timezone
    let tz: Tz = tz_str
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid timezone '{}': {}", tz_str, e))?;

    // Parse time
    let (hour, minute) = parse_time_string(time_str)?;

    // Get current time in the target timezone
    let now = chrono::Utc::now().with_timezone(&tz);

    // Create target time for today
    let naive_time = NaiveTime::from_hms_opt(hour, minute, 0)
        .with_context(|| format!("Invalid time: {}:{}", hour, minute))?;

    let today = now.date_naive();
    let target_naive = today.and_time(naive_time);

    // Convert to timezone-aware datetime
    let mut target = tz
        .from_local_datetime(&target_naive)
        .single()
        .with_context(|| {
            format!(
                "Ambiguous datetime (DST transition?): {} in {}",
                target_naive, tz_str
            )
        })?;

    // If target time is in the past, add one day
    if target <= now {
        tracing::debug!(
            "Target time {} is in the past (now: {}), scheduling for tomorrow",
            target,
            now
        );
        target += Duration::days(1);
    }

    let duration_until = (target - now)
        .to_std()
        .context("Failed to convert duration (target time may be in the past)")?;

    tracing::info!(
        "Parsed reset time: {} {} → {} (in {} from now)",
        time_str,
        tz_str,
        target,
        crate::utils::format_duration(duration_until)
    );

    Ok(target)
}

/// Extract time and timezone from a message using the configured regex pattern
pub fn extract_time_and_timezone(text: &str, pattern: &str) -> Result<Option<(String, String)>> {
    let re = Regex::new(pattern)?;

    if let Some(captures) = re.captures(text) {
        if captures.len() >= 3 {
            let time_str = captures
                .get(1)
                .map(|m| m.as_str().to_string())
                .context("Missing time capture group")?;
            let tz_str = captures
                .get(2)
                .map(|m| m.as_str().to_string())
                .context("Missing timezone capture group")?;

            tracing::debug!("Extracted time='{}' timezone='{}'", time_str, tz_str);
            return Ok(Some((time_str, tz_str)));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_string() {
        assert_eq!(parse_time_string("3pm").unwrap(), (15, 0));
        assert_eq!(parse_time_string("11:30am").unwrap(), (11, 30));
        assert_eq!(parse_time_string("12:00pm").unwrap(), (12, 0));
        assert_eq!(parse_time_string("12:00am").unwrap(), (0, 0));
        assert_eq!(parse_time_string("1am").unwrap(), (1, 0));
        assert_eq!(parse_time_string("11pm").unwrap(), (23, 0));
    }

    #[test]
    fn test_parse_time_string_invalid() {
        assert!(parse_time_string("25pm").is_err());
        assert!(parse_time_string("3").is_err());
        assert!(parse_time_string("3:70pm").is_err());
    }

    #[test]
    fn test_extract_time_and_timezone() {
        let pattern = r#"Your limit will reset at (\d{1,2}(?::\d{2})?\s?(?:am|pm)) \((.*?)\)\."#;
        let text = "Your limit will reset at 3pm (America/Santiago).";

        let result = extract_time_and_timezone(text, pattern).unwrap();
        assert!(result.is_some());

        let (time, tz) = result.unwrap();
        assert_eq!(time, "3pm");
        assert_eq!(tz, "America/Santiago");
    }

    #[test]
    fn test_parse_reset_time() {
        // This test verifies the parsing logic works
        let result = parse_reset_time("3pm", "America/Santiago");
        assert!(result.is_ok());
    }
}
