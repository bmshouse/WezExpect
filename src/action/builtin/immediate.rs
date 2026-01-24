use crate::action::{Action, ActionConfig};
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::any::Any;

/// Empty match data for immediate action (no extraction needed)
struct ImmediateMatch;

/// Action that sends a command immediately when pattern matches
///
/// This action sends the configured command as soon as the pattern is detected
/// in the terminal output, without any waiting or extraction.
pub struct ImmediateAction;

#[async_trait]
impl Action for ImmediateAction {
    fn action_type(&self) -> &str {
        "immediate"
    }

    fn validate_config(&self, config: &ActionConfig) -> Result<()> {
        // Command is required and cannot be empty or whitespace-only
        // Note: "\n" is valid because it sends a newline to the terminal
        match &config.command {
            None => anyhow::bail!("immediate action requires a 'command' field"),
            Some(cmd) if cmd.is_empty() => {
                anyhow::bail!("immediate action requires a non-empty 'command' field")
            }
            Some(_) => Ok(()),
        }
    }

    fn check_match(
        &self,
        content: &str,
        pattern: &str,
    ) -> Result<Option<Box<dyn Any + Send>>> {
        let re = Regex::new(pattern)?;

        if re.is_match(content) {
            tracing::debug!("ImmediateAction matched pattern: {}", pattern);
            Ok(Some(Box::new(ImmediateMatch)))
        } else {
            Ok(None)
        }
    }

    async fn execute(
        &self,
        pane_id: u32,
        _match_data: Box<dyn Any + Send>,
        config: &ActionConfig,
    ) -> Result<()> {
        tracing::info!("ImmediateAction executing");

        // Send command immediately
        let command = config.command.as_ref().unwrap();
        tracing::info!("Sending command: '{}'", command);
        crate::sender::send_command(pane_id, command).await?;

        tracing::info!("ImmediateAction completed successfully!");

        Ok(())
    }

    async fn post_execute(&self, _pane_id: u32) -> Result<()> {
        // Note: Content change detection is handled by the monitor
        // This action doesn't need to do anything in post_execute
        Ok(())
    }

    fn required_captures(&self) -> Option<usize> {
        // No capture group requirements for immediate action
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_type() {
        let action = ImmediateAction;
        assert_eq!(action.action_type(), "immediate");
    }

    #[test]
    fn test_required_captures() {
        let action = ImmediateAction;
        assert_eq!(action.required_captures(), None);
    }

    #[test]
    fn test_validate_config_missing_command() {
        let action = ImmediateAction;
        let config = ActionConfig {
            action_type: "immediate".to_string(),
            command: None,
            extra: toml::Value::Table(Default::default()),
        };
        assert!(action.validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_valid() {
        let action = ImmediateAction;
        let config = ActionConfig {
            action_type: "immediate".to_string(),
            command: Some("\n".to_string()),
            extra: toml::Value::Table(Default::default()),
        };
        assert!(action.validate_config(&config).is_ok());
    }

    #[test]
    fn test_check_match_success() {
        let action = ImmediateAction;
        let pattern = "Press Enter to continue";
        let content = "Press Enter to continue...";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_check_match_no_match() {
        let action = ImmediateAction;
        let pattern = "Press Enter";
        let content = "This does not match.";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_check_match_regex() {
        let action = ImmediateAction;
        let pattern = r"Error: \d+";
        let content = "Error: 404 - Not Found";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_some());
    }
}
