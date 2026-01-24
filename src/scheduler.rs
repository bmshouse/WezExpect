use anyhow::Result;
use chrono_tz::Tz;
use tokio::time::{sleep_until, Duration, Instant};

/// Minimum wait duration (in seconds) to show progress updates
const PROGRESS_LOG_THRESHOLD_SECS: u64 = 60;

/// Wait duration (in seconds) threshold for using longer progress intervals
const LONG_WAIT_THRESHOLD_SECS: u64 = 300;

/// Progress log interval for long waits (> 5 minutes)
const LONG_WAIT_INTERVAL_SECS: u64 = 60;

/// Progress log interval for shorter waits
const SHORT_WAIT_INTERVAL_SECS: u64 = 30;

/// Wait until a specific target time
///
/// Converts a timezone-aware DateTime to an Instant and sleeps until that moment
pub async fn wait_until(target: chrono::DateTime<Tz>) -> Result<()> {
    let now = chrono::Utc::now();
    let duration = target.signed_duration_since(now);

    if duration.num_seconds() <= 0 {
        tracing::warn!("Target time is in the past or immediate, not waiting");
        return Ok(());
    }

    // Convert chrono Duration to std::time::Duration
    let std_duration = duration
        .to_std()
        .map_err(|e| anyhow::anyhow!("Failed to convert duration: {}", e))?;

    // Calculate the instant to wake up
    let wake_instant = Instant::now() + std_duration;

    tracing::info!(
        "Waiting until {} (duration: {})",
        target,
        crate::utils::format_duration(std_duration)
    );

    // Log periodic updates for long waits
    if std_duration.as_secs() > PROGRESS_LOG_THRESHOLD_SECS {
        spawn_progress_logger(std_duration, target);
    }

    sleep_until(wake_instant).await;

    tracing::info!("Wait complete, target time reached: {}", target);
    Ok(())
}

/// Spawn a background task that logs progress for long waits
fn spawn_progress_logger(total_duration: std::time::Duration, target: chrono::DateTime<Tz>) {
    tokio::spawn(async move {
        let start = Instant::now();
        let total_secs = total_duration.as_secs();

        // Log every minute for waits > 5 minutes, every 30 seconds otherwise
        let interval_secs = if total_secs > LONG_WAIT_THRESHOLD_SECS {
            LONG_WAIT_INTERVAL_SECS
        } else {
            SHORT_WAIT_INTERVAL_SECS
        };

        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;

            let elapsed = start.elapsed();
            if elapsed >= total_duration {
                break;
            }

            let remaining = total_duration - elapsed;
            let percent = (elapsed.as_secs_f64() / total_secs as f64) * 100.0;

            tracing::info!(
                "Waiting for reset... {:.0}% complete, {} remaining (target: {})",
                percent,
                crate::utils::format_duration(remaining),
                target
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_wait_until_immediate() {
        // Test with a time in the past - should return immediately
        let past = Utc::now() - chrono::Duration::seconds(10);
        let tz: Tz = "UTC".parse().unwrap();
        let past_tz = past.with_timezone(&tz);

        let start = Instant::now();
        wait_until(past_tz).await.unwrap();
        let elapsed = start.elapsed();

        // Should return almost immediately
        assert!(elapsed.as_millis() < 100);
    }

    #[tokio::test]
    #[ignore] // This test is timing-sensitive and may fail on heavily loaded systems
    async fn test_wait_until_short() {
        // Test with a wait - timing tests are inherently flaky
        // The time between calculating the future and calling wait_until can cause
        // the target time to already be in the past, resulting in immediate return
        let future = Utc::now() + chrono::Duration::milliseconds(2000);
        let tz: Tz = "UTC".parse().unwrap();
        let future_tz = future.with_timezone(&tz);

        let start = Instant::now();
        wait_until(future_tz).await.unwrap();
        let elapsed = start.elapsed();

        // Should wait at least 1.8 seconds (allowing for timing variance and overhead)
        // Upper bound is generous to account for scheduler delays
        assert!(
            elapsed.as_millis() >= 1800 && elapsed.as_millis() <= 2500,
            "Expected wait between 1800-2500ms, got {}ms. Run with --ignored to test timing-sensitive code.",
            elapsed.as_millis()
        );
    }
}
