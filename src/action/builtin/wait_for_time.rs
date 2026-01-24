use crate::action::{Action, ActionConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::any::Any;

/// Match data extracted by WaitForTimeAction
struct WaitForTimeMatch {
    time_str: String,
    tz_str: String,
}

/// Action that extracts time/timezone from pattern, waits until that time, then sends command
///
/// This action requires exactly 2 capture groups in the pattern:
/// - Group 1: Time string (e.g., "3pm", "11:30am")
/// - Group 2: Timezone string (e.g., "America/Santiago")
pub struct WaitForTimeAction;

#[async_trait]
impl Action for WaitForTimeAction {
    fn action_type(&self) -> &str {
        "wait_for_time"
    }

    fn validate_config(&self, config: &ActionConfig) -> Result<()> {
        // Command is required and cannot be empty or whitespace-only
        match &config.command {
            None => anyhow::bail!("wait_for_time action requires a 'command' field"),
            Some(cmd) if cmd.trim().is_empty() => {
                anyhow::bail!("wait_for_time action requires a non-empty 'command' field")
            }
            Some(_) => Ok(()),
        }
    }

    fn check_match(
        &self,
        content: &str,
        pattern: &str,
    ) -> Result<Option<Box<dyn Any + Send>>> {
        // Extract time and timezone using the pattern
        let result = crate::parser::extract_time_and_timezone(content, pattern)?;

        match result {
            Some((time_str, tz_str)) => {
                tracing::debug!(
                    "WaitForTimeAction matched: time='{}', tz='{}'",
                    time_str,
                    tz_str
                );
                Ok(Some(Box::new(WaitForTimeMatch { time_str, tz_str })))
            }
            None => Ok(None),
        }
    }

    async fn execute(
        &self,
        pane_id: u32,
        match_data: Box<dyn Any + Send>,
        config: &ActionConfig,
    ) -> Result<()> {
        // Downcast match data
        let match_data = match_data
            .downcast::<WaitForTimeMatch>()
            .map_err(|_| anyhow::anyhow!("Invalid match data type for WaitForTimeAction"))?;

        let time_str = &match_data.time_str;
        let tz_str = &match_data.tz_str;

        tracing::info!(
            "WaitForTimeAction executing: time='{}', tz='{}'",
            time_str,
            tz_str
        );

        // Parse time and timezone
        let target_time = crate::parser::parse_reset_time(time_str, tz_str)
            .with_context(|| format!("Failed to parse time '{}' in timezone '{}'", time_str, tz_str))?;

        tracing::info!("Waiting until {}", target_time);

        // Wait until target time
        crate::scheduler::wait_until(target_time).await?;

        // Send command
        let command = config.command.as_ref().unwrap();
        tracing::info!("Sending command: '{}'", command);
        crate::sender::send_command(pane_id, command).await?;

        tracing::info!("WaitForTimeAction completed successfully!");

        Ok(())
    }

    fn required_captures(&self) -> Option<usize> {
        // Requires exactly 2 capture groups (time, timezone)
        Some(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_type() {
        let action = WaitForTimeAction;
        assert_eq!(action.action_type(), "wait_for_time");
    }

    #[test]
    fn test_required_captures() {
        let action = WaitForTimeAction;
        assert_eq!(action.required_captures(), Some(2));
    }

    #[test]
    fn test_validate_config_missing_command() {
        let action = WaitForTimeAction;
        let config = ActionConfig {
            action_type: "wait_for_time".to_string(),
            command: None,
            extra: toml::Value::Table(Default::default()),
        };
        assert!(action.validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_empty_command() {
        let action = WaitForTimeAction;
        let config = ActionConfig {
            action_type: "wait_for_time".to_string(),
            command: Some("   ".to_string()),
            extra: toml::Value::Table(Default::default()),
        };
        assert!(action.validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_valid() {
        let action = WaitForTimeAction;
        let config = ActionConfig {
            action_type: "wait_for_time".to_string(),
            command: Some("continue\n".to_string()),
            extra: toml::Value::Table(Default::default()),
        };
        assert!(action.validate_config(&config).is_ok());
    }

    #[test]
    fn test_check_match_success() {
        let action = WaitForTimeAction;
        let pattern = r#"Your limit will reset at (\d{1,2}(?::\d{2})?\s?(?:am|pm)) \((.*?)\)\."#;
        let content = "Your limit will reset at 3pm (America/Santiago).";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_check_match_no_match() {
        let action = WaitForTimeAction;
        let pattern = r#"Your limit will reset at (\d{1,2}(?::\d{2})?\s?(?:am|pm)) \((.*?)\)\."#;
        let content = "This does not match the pattern.";

        let result = action.check_match(content, pattern).unwrap();
        assert!(result.is_none());
    }
}
