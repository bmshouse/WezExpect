//! Terminal monitoring with pattern matching and duplicate prevention.
//!
//! This module implements the core monitoring logic for watching WezTerm panes
//! and detecting pattern matches. It provides:
//! - Content-based duplicate prevention using hashing
//! - Time-based cooldown to prevent rapid re-matching
//! - Configurable lookback lines to limit terminal history retrieval
//! - Automatic content change detection
//! - First-match-wins rule processing

use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use crate::action::{Action, ActionFactory, MatchData};
use crate::config::Rule;

/// Result of a successful pattern match
pub struct MatchResult {
    pub rule: Rule,
    pub action: Box<dyn Action>,
    pub match_data: MatchData,
}

/// Monitor a WezTerm pane for timeout messages
pub struct Monitor {
    pane_id: u32,
    rules: Vec<Rule>,
    factory: Arc<dyn ActionFactory>,
    last_content: Arc<Mutex<String>>,
    last_content_hash: Arc<Mutex<Option<u64>>>, // Hash of content when last matched
    last_match_time: Arc<Mutex<Option<Instant>>>, // Timestamp of last pattern match
    match_cooldown: Duration,                   // Minimum time between pattern matches
    lookback_lines: u32,                        // Number of lines to retrieve from pane history
}

impl Monitor {
    /// Create a new monitor for a WezTerm pane with rules and action factory
    ///
    /// # Arguments
    /// * `pane_id` - The WezTerm pane ID to monitor
    /// * `rules` - List of rules to check against (processed in order, first match wins)
    /// * `factory` - Factory for creating action instances
    /// * `match_cooldown_secs` - Minimum seconds between pattern matches (prevents rapid re-matching)
    /// * `lookback_lines` - Number of lines to retrieve from pane history (prevents processing entire history)
    ///
    /// # Examples
    /// ```no_run
    /// use wez_expect::{Monitor, Config};
    /// use wez_expect::action::factory::BuiltinActionFactory;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = Config::load("config.toml")?;
    /// let factory = Arc::new(BuiltinActionFactory::new());
    ///
    /// let monitor = Monitor::new(
    ///     1,                                      // pane_id
    ///     config.rules.clone(),                   // rules
    ///     factory,                                // factory
    ///     60,                                     // 60 second cooldown
    ///     50,                                     // lookback 50 lines
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        pane_id: u32,
        rules: Vec<Rule>,
        factory: Arc<dyn ActionFactory>,
        match_cooldown_secs: u64,
        lookback_lines: u32,
    ) -> Self {
        Self {
            pane_id,
            rules,
            factory,
            last_content: Arc::new(Mutex::new(String::new())),
            last_content_hash: Arc::new(Mutex::new(None)),
            last_match_time: Arc::new(Mutex::new(None)),
            match_cooldown: Duration::from_secs(match_cooldown_secs),
            lookback_lines,
        }
    }

    /// Check the pane for pattern matches across all enabled rules
    ///
    /// Returns the first matching rule and its action with match data
    pub async fn check_for_timeout(&self) -> Result<Option<MatchResult>> {
        // Get terminal content (last N lines to avoid processing entire history)
        let content = self.get_pane_text().await?;

        // Update last content
        *self.last_content.lock().await = content.clone();

        // Check if within cooldown period from last match
        if let Some(last_match) = *self.last_match_time.lock().await {
            let elapsed = last_match.elapsed();
            if elapsed < self.match_cooldown {
                let remaining = self.match_cooldown - elapsed;
                tracing::debug!(
                    "Skipping pattern check - within cooldown period ({:.1}s remaining)",
                    remaining.as_secs_f64()
                );
                return Ok(None);
            }
        }

        // Check if content unchanged since last match
        let current_hash = calculate_hash(&content);
        let last_hash = *self.last_content_hash.lock().await;

        if Some(current_hash) == last_hash {
            tracing::debug!(
                "Skipping pattern check - content unchanged since last match (hash: {})",
                current_hash
            );
            return Ok(None);
        }

        // Check each enabled rule in order (first match wins)
        for rule in self.rules.iter().filter(|r| r.enabled) {
            // Create action instance for this rule
            let action = self
                .factory
                .create(&rule.action.action_type)
                .ok_or_else(|| {
                    anyhow::anyhow!("Unknown action type: '{}'", rule.action.action_type)
                })?;

            // Check if pattern matches
            if let Some(match_data) = action.check_match(&content, &rule.pattern)? {
                let rule_name = rule.name.as_deref().unwrap_or("unnamed");
                tracing::info!("Matched rule '{}' ({})", rule_name, rule.action.action_type);

                // Store content hash to prevent re-matching on unchanged content
                *self.last_content_hash.lock().await = Some(current_hash);

                // Store match time to enforce cooldown period
                *self.last_match_time.lock().await = Some(Instant::now());

                return Ok(Some(MatchResult {
                    rule: rule.clone(),
                    action,
                    match_data,
                }));
            }
        }

        Ok(None)
    }

    /// Wait for terminal content to change (prevents re-matching same pattern)
    pub async fn wait_for_content_change(&self, poll_interval: Duration) -> Result<()> {
        // Use hash comparison to avoid cloning large strings
        let last_content_hash = calculate_hash(&self.last_content.lock().await);

        loop {
            tokio::time::sleep(poll_interval).await;

            let current_content = self.get_pane_text().await?;
            let current_hash = calculate_hash(&current_content);

            if current_hash != last_content_hash {
                *self.last_content.lock().await = current_content;
                // Clear hash when content changes - allows matching again in new context
                *self.last_content_hash.lock().await = None;
                tracing::info!(
                    "Terminal content changed - duplicate prevention cleared, pattern matching will resume"
                );
                return Ok(());
            }
        }
    }

    /// Get text content from the pane
    async fn get_pane_text(&self) -> Result<String> {
        // Format lookback_lines as negative number for wezterm (e.g., 50 -> "-50")
        let start_line = format!("-{}", self.lookback_lines);

        let output = Command::new("wezterm")
            .args([
                "cli",
                "get-text",
                "--pane-id",
                &self.pane_id.to_string(),
                "--start-line",
                &start_line,
            ])
            .output()
            .await
            .context("Failed to execute 'wezterm cli get-text'")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "wezterm cli get-text failed for pane {}: {}",
                self.pane_id,
                stderr
            );
        }

        let content = String::from_utf8(output.stdout)
            .context("wezterm cli get-text output is not valid UTF-8")?;

        Ok(content)
    }

    /// Get the pane ID being monitored
    pub fn pane_id(&self) -> u32 {
        self.pane_id
    }
}

/// Calculate hash of terminal content for duplicate detection.
///
/// Uses Rust's `DefaultHasher` (currently SipHash 1-3) for fast,
/// non-cryptographic hashing of terminal content. The hash is used to
/// detect when terminal content has changed since the last pattern match.
fn calculate_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{factory::BuiltinActionFactory, ActionConfig};
    use crate::config::Rule;

    #[tokio::test]
    async fn test_monitor_creation() {
        let rules = vec![Rule {
            name: Some(String::from("test")),
            pattern: r#"Your limit will reset at (\d{1,2}(?::\d{2})?\s?(?:am|pm)) \((.*?)\)\."#
                .to_string(),
            action: ActionConfig {
                action_type: String::from("wait_for_time"),
                command: Some(String::from("continue")),
                extra: toml::Value::Table(Default::default()),
            },
            enabled: true,
        }];
        let factory = Arc::new(BuiltinActionFactory::new());
        let monitor = Monitor::new(1, rules, factory, 60, 50);
        assert_eq!(monitor.pane_id(), 1);
    }

    #[test]
    fn test_calculate_hash_deterministic() {
        let content1 = "Build succeeded\nDone";
        let content2 = "Build succeeded\nDone";
        let content3 = "Build succeeded\nDone\n";

        // Same content should produce same hash
        assert_eq!(
            super::calculate_hash(content1),
            super::calculate_hash(content2)
        );

        // Different content should produce different hash
        assert_ne!(
            super::calculate_hash(content1),
            super::calculate_hash(content3)
        );
    }

    #[test]
    fn test_calculate_hash_changes_on_new_content() {
        let content1 = "Build 1 succeeded\nDone";
        let content2 = "Build 1 succeeded\nBuild 2 succeeded\nDone";

        // Adding new lines changes the hash
        assert_ne!(
            super::calculate_hash(content1),
            super::calculate_hash(content2)
        );
    }

    #[tokio::test]
    async fn test_hash_tracking_in_monitor() {
        // Test that monitor initializes with None hash
        let rules = vec![Rule {
            name: Some(String::from("test")),
            pattern: r#"Build succeeded"#.to_string(),
            action: ActionConfig {
                action_type: String::from("immediate"),
                command: Some(String::from("echo done")),
                extra: toml::Value::Table(Default::default()),
            },
            enabled: true,
        }];
        let factory = Arc::new(BuiltinActionFactory::new());
        let monitor = Monitor::new(1, rules, factory, 60, 50);

        // Verify initial state
        assert_eq!(*monitor.last_content_hash.lock().await, None);
    }

    #[tokio::test]
    async fn test_monitor_lookback_lines_default() {
        let rules = vec![Rule {
            name: Some(String::from("test")),
            pattern: r#"test"#.to_string(),
            action: ActionConfig {
                action_type: String::from("immediate"),
                command: Some(String::from("cmd")),
                extra: toml::Value::Table(Default::default()),
            },
            enabled: true,
        }];
        let factory = Arc::new(BuiltinActionFactory::new());
        let monitor = Monitor::new(1, rules, factory, 60, 50);

        assert_eq!(monitor.lookback_lines, 50);
    }

    #[tokio::test]
    async fn test_monitor_lookback_lines_custom() {
        let rules = vec![Rule {
            name: Some(String::from("test")),
            pattern: r#"test"#.to_string(),
            action: ActionConfig {
                action_type: String::from("immediate"),
                command: Some(String::from("cmd")),
                extra: toml::Value::Table(Default::default()),
            },
            enabled: true,
        }];
        let factory = Arc::new(BuiltinActionFactory::new());

        // Test various lookback_lines values
        let monitor_100 = Monitor::new(1, rules.clone(), factory.clone(), 60, 100);
        assert_eq!(monitor_100.lookback_lines, 100);

        let monitor_200 = Monitor::new(2, rules.clone(), factory.clone(), 60, 200);
        assert_eq!(monitor_200.lookback_lines, 200);

        let monitor_10 = Monitor::new(3, rules, factory, 60, 10);
        assert_eq!(monitor_10.lookback_lines, 10);
    }

    // Note: Comprehensive testing of check_for_timeout() and wait_for_content_change()
    // requires either WezTerm to be running or refactoring for dependency injection.
    // Integration tests should verify:
    // 1. Duplicate prevention on unchanged content
    // 2. Re-matching when new content appears
    // 3. Hash clearing in wait_for_content_change()
    // 4. Correct usage of lookback_lines when calling wezterm cli get-text
}
