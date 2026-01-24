use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::Duration;

use crate::action::{Action, ActionFactory};
use crate::config::Rule;

/// Number of lines to retrieve from pane history when checking for timeout messages
const PANE_HISTORY_LINES: &str = "-50";

/// Result of a successful pattern match
pub struct MatchResult {
    pub rule: Rule,
    pub action: Box<dyn Action>,
    pub match_data: Box<dyn std::any::Any + Send>,
}

/// Monitor a WezTerm pane for timeout messages
pub struct Monitor {
    pane_id: u32,
    rules: Vec<Rule>,
    factory: Arc<dyn ActionFactory>,
    last_content: Arc<Mutex<String>>,
    last_content_hash: Arc<Mutex<Option<u64>>>, // Hash of content when last matched
}

impl Monitor {
    /// Create a new monitor for a WezTerm pane with rules and action factory
    pub fn new(pane_id: u32, rules: Vec<Rule>, factory: Arc<dyn ActionFactory>) -> Self {
        Self {
            pane_id,
            rules,
            factory,
            last_content: Arc::new(Mutex::new(String::new())),
            last_content_hash: Arc::new(Mutex::new(None)),
        }
    }

    /// Check the pane for pattern matches across all enabled rules
    ///
    /// Returns the first matching rule and its action with match data
    pub async fn check_for_timeout(&self) -> Result<Option<MatchResult>> {
        // Get terminal content (last 50 lines to avoid processing entire history)
        let content = self.get_pane_text().await?;

        // Update last content
        *self.last_content.lock().await = content.clone();

        // Check if content unchanged since last match
        let current_hash = calculate_hash(&content);
        let last_hash = *self.last_content_hash.lock().await;

        if Some(current_hash) == last_hash {
            tracing::debug!("Skipping pattern check - content unchanged since last match");
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
                return Ok(());
            }
        }
    }

    /// Get text content from the pane
    async fn get_pane_text(&self) -> Result<String> {
        let output = Command::new("wezterm")
            .args([
                "cli",
                "get-text",
                "--pane-id",
                &self.pane_id.to_string(),
                "--start-line",
                PANE_HISTORY_LINES,
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

/// Calculate hash of terminal content for duplicate detection
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
        let monitor = Monitor::new(1, rules, factory);
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
        let monitor = Monitor::new(1, rules, factory);

        // Verify initial state
        assert_eq!(*monitor.last_content_hash.lock().await, None);
    }

    // Note: Comprehensive testing of check_for_timeout() and wait_for_content_change()
    // requires either WezTerm to be running or refactoring for dependency injection.
    // Integration tests should verify:
    // 1. Duplicate prevention on unchanged content
    // 2. Re-matching when new content appears
    // 3. Hash clearing in wait_for_content_change()
}
