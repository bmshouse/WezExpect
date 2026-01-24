use crate::action::{Action, ActionConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use std::any::Any;

/// Configuration fields specific to conditional action
#[derive(Debug, Clone, Deserialize)]
struct ConditionalConfig {
    /// Pattern to test against the captured group
    condition_pattern: String,
    /// Command to send if condition matches
    then_command: String,
    /// Command to send if condition doesn't match
    else_command: String,
    /// Which capture group to apply the condition to (1-based index)
    #[serde(default = "default_apply_to_capture")]
    apply_to_capture: usize,
}

fn default_apply_to_capture() -> usize {
    1
}

/// Match data for conditional action
struct ConditionalMatch {
    /// Original terminal content that matched the pattern
    content: String,
    /// The regex pattern that was matched
    pattern: String,
}

/// Action that executes different commands based on pattern matching
///
/// This action extracts capture groups from the main pattern, then tests
/// a specific capture group against a condition pattern. It sends different
/// commands based on whether the condition matches.
pub struct ConditionalAction;

impl ConditionalAction {
    /// Parse action-specific config from the flattened TOML
    fn parse_config(&self, config: &ActionConfig) -> Result<ConditionalConfig> {
        let conditional_config: ConditionalConfig = config
            .extra
            .clone()
            .try_into()
            .context("Failed to parse conditional action config")?;
        Ok(conditional_config)
    }
}

#[async_trait]
impl Action for ConditionalAction {
    fn action_type(&self) -> &str {
        "conditional"
    }

    fn validate_config(&self, config: &ActionConfig) -> Result<()> {
        // Parse the conditional-specific config
        let cond_config = self.parse_config(config)?;

        // Validate condition_pattern compiles
        Regex::new(&cond_config.condition_pattern).with_context(|| {
            format!(
                "Invalid condition_pattern: {}",
                cond_config.condition_pattern
            )
        })?;

        // Validate commands are not empty
        if cond_config.then_command.trim().is_empty() {
            anyhow::bail!("conditional action requires a non-empty 'then_command' field");
        }
        if cond_config.else_command.trim().is_empty() {
            anyhow::bail!("conditional action requires a non-empty 'else_command' field");
        }

        // Validate apply_to_capture is at least 1
        if cond_config.apply_to_capture < 1 {
            anyhow::bail!("apply_to_capture must be at least 1 (capture groups are 1-indexed)");
        }

        Ok(())
    }

    fn check_match(&self, content: &str, pattern: &str) -> Result<Option<Box<dyn Any + Send>>> {
        let re = Regex::new(pattern)?;

        if re.is_match(content) {
            tracing::debug!("ConditionalAction matched pattern");
            Ok(Some(Box::new(ConditionalMatch {
                content: content.to_string(),
                pattern: pattern.to_string(),
            })))
        } else {
            Ok(None)
        }
    }

    async fn execute(
        &self,
        pane_id: u32,
        match_data: Box<dyn Any + Send>,
        config: &ActionConfig,
    ) -> Result<()> {
        tracing::info!("ConditionalAction executing");

        // 1. Downcast match_data to ConditionalMatch
        let match_data = match_data
            .downcast::<ConditionalMatch>()
            .map_err(|_| anyhow::anyhow!("Invalid match data type for ConditionalAction"))?;

        // 2. Parse conditional config
        let cond_config = self.parse_config(config)?;

        // 3. Re-match the pattern to get captures
        let re = Regex::new(&match_data.pattern)?;
        let captures = re
            .captures(&match_data.content)
            .context("Pattern no longer matches content")?;

        // 4. Validate apply_to_capture bounds
        let num_captures = captures.len() - 1; // Exclude full match at index 0
        if cond_config.apply_to_capture > num_captures {
            anyhow::bail!(
                "apply_to_capture={} exceeds available captures ({})",
                cond_config.apply_to_capture,
                num_captures
            );
        }

        // 5. Extract target capture (1-based index, so we use apply_to_capture directly)
        let target_capture = captures
            .get(cond_config.apply_to_capture)
            .map(|m| m.as_str())
            .unwrap_or("");

        // 6. Compile condition pattern
        let condition_re =
            Regex::new(&cond_config.condition_pattern).context("Invalid condition_pattern")?;

        // 7. Test condition
        let condition_met = condition_re.is_match(target_capture);

        // 8. Select command based on condition result
        let command = if condition_met {
            &cond_config.then_command
        } else {
            &cond_config.else_command
        };

        // Log the decision
        tracing::info!(
            "ConditionalAction: condition {} (testing '{}' against '{}'), sending {}",
            if condition_met { "MET" } else { "NOT MET" },
            target_capture,
            cond_config.condition_pattern,
            if condition_met {
                "then_command"
            } else {
                "else_command"
            }
        );

        // 9. Send the selected command
        crate::sender::send_command(pane_id, command).await?;

        tracing::info!("ConditionalAction completed successfully!");

        Ok(())
    }

    fn required_captures(&self) -> Option<usize> {
        // Requires at least 1 capture group, but the exact number depends on apply_to_capture
        Some(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config(then_cmd: &str, else_cmd: &str) -> ActionConfig {
        let mut extra = toml::value::Table::new();
        extra.insert(
            "condition_pattern".to_string(),
            toml::Value::String("succeeded".to_string()),
        );
        extra.insert(
            "then_command".to_string(),
            toml::Value::String(then_cmd.to_string()),
        );
        extra.insert(
            "else_command".to_string(),
            toml::Value::String(else_cmd.to_string()),
        );
        extra.insert("apply_to_capture".to_string(), toml::Value::Integer(1));

        ActionConfig {
            action_type: "conditional".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        }
    }

    #[test]
    fn test_action_type() {
        let action = ConditionalAction;
        assert_eq!(action.action_type(), "conditional");
    }

    #[test]
    fn test_validate_config_valid() {
        let action = ConditionalAction;
        let config = create_test_config("deploy\n", "echo failed\n");
        assert!(action.validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_empty_then_command() {
        let action = ConditionalAction;
        let config = create_test_config("", "echo failed\n");
        assert!(action.validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_empty_else_command() {
        let action = ConditionalAction;
        let config = create_test_config("deploy\n", "");
        assert!(action.validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_invalid_pattern() {
        let action = ConditionalAction;
        let mut extra = toml::value::Table::new();
        extra.insert(
            "condition_pattern".to_string(),
            toml::Value::String("(invalid[regex".to_string()),
        );
        extra.insert(
            "then_command".to_string(),
            toml::Value::String("cmd1".to_string()),
        );
        extra.insert(
            "else_command".to_string(),
            toml::Value::String("cmd2".to_string()),
        );

        let config = ActionConfig {
            action_type: "conditional".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        };

        assert!(action.validate_config(&config).is_err());
    }

    #[test]
    fn test_check_match_success() {
        let action = ConditionalAction;
        let pattern = r"Build (succeeded|failed)";
        let content = "Build succeeded";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_check_match_no_match() {
        let action = ConditionalAction;
        let pattern = r"Build (succeeded|failed)";
        let content = "No build information";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_check_match_stores_content_and_pattern() {
        let action = ConditionalAction;
        let pattern = r"Build (succeeded|failed) in (\d+)s";
        let content = "Build succeeded in 42s";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_some());

        // Downcast to verify content and pattern were stored
        let match_data = result.unwrap();
        let conditional_match = match_data.downcast::<ConditionalMatch>().unwrap();
        assert_eq!(conditional_match.content, content);
        assert_eq!(conditional_match.pattern, pattern);
    }

    #[test]
    fn test_validate_config_zero_apply_to_capture() {
        let action = ConditionalAction;
        let mut extra = toml::value::Table::new();
        extra.insert(
            "condition_pattern".to_string(),
            toml::Value::String("succeeded".to_string()),
        );
        extra.insert(
            "then_command".to_string(),
            toml::Value::String("cmd1".to_string()),
        );
        extra.insert(
            "else_command".to_string(),
            toml::Value::String("cmd2".to_string()),
        );
        extra.insert(
            "apply_to_capture".to_string(),
            toml::Value::Integer(0), // Invalid: must be >= 1
        );

        let config = ActionConfig {
            action_type: "conditional".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        };

        let result = action.validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 1"));
    }

    #[tokio::test]
    async fn test_execute_condition_met() {
        let action = ConditionalAction;
        let pattern = r"Build (succeeded|failed) in (\d+)s";
        let content = "Build succeeded in 42s";

        // Create match data
        let match_data = Box::new(ConditionalMatch {
            content: content.to_string(),
            pattern: pattern.to_string(),
        });

        // Create config with condition that will match "succeeded"
        let mut extra = toml::value::Table::new();
        extra.insert(
            "condition_pattern".to_string(),
            toml::Value::String("succeeded".to_string()),
        );
        extra.insert(
            "then_command".to_string(),
            toml::Value::String("echo 'Deploy started'\n".to_string()),
        );
        extra.insert(
            "else_command".to_string(),
            toml::Value::String("echo 'Build failed'\n".to_string()),
        );
        extra.insert("apply_to_capture".to_string(), toml::Value::Integer(1));

        let config = ActionConfig {
            action_type: "conditional".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        };

        // Note: This test will fail when actually trying to send the command
        // because we don't have a real WezTerm pane. We're just testing the logic flow.
        // In a real scenario, we'd need to mock sender::send_command
        let result = action.execute(999, match_data, &config).await;

        // The execute will fail at send_command, but we can verify it gets that far
        // by checking the error message doesn't mention capture groups or pattern matching
        if let Err(e) = result {
            let err_msg = e.to_string();
            // Should not be a pattern/capture error
            assert!(!err_msg.contains("Pattern no longer matches"));
            assert!(!err_msg.contains("exceeds available captures"));
        }
    }

    #[tokio::test]
    async fn test_execute_condition_not_met() {
        let action = ConditionalAction;
        let pattern = r"Build (succeeded|failed) in (\d+)s";
        let content = "Build failed in 42s";

        let match_data = Box::new(ConditionalMatch {
            content: content.to_string(),
            pattern: pattern.to_string(),
        });

        // Create config with condition that will NOT match "failed"
        let mut extra = toml::value::Table::new();
        extra.insert(
            "condition_pattern".to_string(),
            toml::Value::String("succeeded".to_string()), // Won't match "failed"
        );
        extra.insert(
            "then_command".to_string(),
            toml::Value::String("echo 'Deploy started'\n".to_string()),
        );
        extra.insert(
            "else_command".to_string(),
            toml::Value::String("echo 'Build failed'\n".to_string()),
        );
        extra.insert("apply_to_capture".to_string(), toml::Value::Integer(1));

        let config = ActionConfig {
            action_type: "conditional".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        };

        let result = action.execute(999, match_data, &config).await;

        // Similar to above - will fail at send_command, but shouldn't have pattern errors
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("Pattern no longer matches"));
            assert!(!err_msg.contains("exceeds available captures"));
        }
    }

    #[tokio::test]
    async fn test_execute_apply_to_capture_out_of_bounds() {
        let action = ConditionalAction;
        let pattern = r"Build (succeeded|failed) in (\d+)s";
        let content = "Build succeeded in 42s";

        let match_data = Box::new(ConditionalMatch {
            content: content.to_string(),
            pattern: pattern.to_string(),
        });

        // Create config with apply_to_capture = 5 (out of bounds - only 2 captures)
        let mut extra = toml::value::Table::new();
        extra.insert(
            "condition_pattern".to_string(),
            toml::Value::String("succeeded".to_string()),
        );
        extra.insert(
            "then_command".to_string(),
            toml::Value::String("cmd1".to_string()),
        );
        extra.insert(
            "else_command".to_string(),
            toml::Value::String("cmd2".to_string()),
        );
        extra.insert(
            "apply_to_capture".to_string(),
            toml::Value::Integer(5), // Out of bounds!
        );

        let config = ActionConfig {
            action_type: "conditional".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        };

        let result = action.execute(999, match_data, &config).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("exceeds available captures"));
    }

    #[tokio::test]
    async fn test_execute_with_multiple_captures() {
        let action = ConditionalAction;
        let pattern = r"Build (succeeded|failed) in (\d+)s";
        let content = "Build succeeded in 42s";

        let match_data = Box::new(ConditionalMatch {
            content: content.to_string(),
            pattern: pattern.to_string(),
        });

        // Test condition against second capture group (the number)
        let mut extra = toml::value::Table::new();
        extra.insert(
            "condition_pattern".to_string(),
            toml::Value::String(r"^\d+$".to_string()), // Match digits
        );
        extra.insert(
            "then_command".to_string(),
            toml::Value::String("cmd1".to_string()),
        );
        extra.insert(
            "else_command".to_string(),
            toml::Value::String("cmd2".to_string()),
        );
        extra.insert(
            "apply_to_capture".to_string(),
            toml::Value::Integer(2), // Second capture group
        );

        let config = ActionConfig {
            action_type: "conditional".to_string(),
            command: None,
            extra: toml::Value::Table(extra),
        };

        let result = action.execute(999, match_data, &config).await;

        // Should not have capture-related errors
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("exceeds available captures"));
        }
    }
}
